//! End-to-end tests for the Phase 2 flow:
//! input → classify → wiki/graph → loop → answer.
//!
//! A scripted model plays the LLM: it returns tool calls and texts
//! in a fixed order, so the loop, the tools and the budgets are all
//! exercised without a network call.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use harness_core::{Context, Model, ModelError, ModelInfo, ModelOutput, StopReason, ToolCall, Usage};
use mia_core::{ToolProvider, ToolSpec};
use mia_graph::SqliteGraph;
use mia_runtime::{MiaRuntime, OutcomeKind, RuntimeBudget};
use mia_wiki::FileWiki;
use serde_json::json;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// A model that returns scripted outputs in order.
struct ScriptedModel {
    outputs: Mutex<VecDeque<ModelOutput>>,
}

impl ScriptedModel {
    fn new(outputs: Vec<ModelOutput>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into()),
        }
    }

    fn text(text: &str) -> ModelOutput {
        ModelOutput {
            text: Some(text.to_string()),
            tool_calls: vec![],
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
            reasoning: None,
            images: vec![],
        }
    }

    fn tool_call(name: &str, args: serde_json::Value) -> ModelOutput {
        ModelOutput {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: name.to_string(),
                args,
            }],
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            reasoning: None,
            images: vec![],
        }
    }
}

#[async_trait]
impl Model for ScriptedModel {
    async fn complete(&self, _ctx: &Context) -> Result<ModelOutput, ModelError> {
        Ok(self
            .outputs
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Self::text("(fine script)")))
    }

    fn info(&self) -> ModelInfo {
        ModelInfo {
            handle: "scripted".into(),
            provider: "test".into(),
            model: "scripted".into(),
            context_window: 4096,
            input_cost_usd_per_million_tokens: None,
            output_cost_usd_per_million_tokens: None,
            supports_tool_use: true,
            supports_streaming: false,
            supports_web_grounding: false,
        }
    }
}

/// A provider with a single `echo` tool, to prove provider tools are
/// registered and callable by the model.
struct FakeProvider;

#[async_trait]
impl ToolProvider for FakeProvider {
    async fn list_tools(&self) -> mia_core::Result<Vec<ToolSpec>> {
        Ok(vec![ToolSpec {
            name: "echo".into(),
            description: "Return the given arguments unchanged.".into(),
            input_schema: json!({"type": "object"}),
        }])
    }

    async fn call(&self, tool: &str, args: serde_json::Value) -> mia_core::Result<serde_json::Value> {
        if tool == "echo" {
            Ok(args)
        } else {
            Err(mia_core::MiaError::ToolNotFound(tool.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Fixture {
    runtime: MiaRuntime,
    user_md: PathBuf,
    dir: PathBuf,
}

async fn setup(name: &str, outputs: Vec<ModelOutput>) -> Fixture {
    let dir = std::env::temp_dir().join(format!("mia-runtime-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let wiki = Arc::new(FileWiki::open(dir.join("wiki")).unwrap());
    let graph = Arc::new(SqliteGraph::open_in_memory().unwrap());
    let model = Arc::new(ScriptedModel::new(outputs));
    let user_md = dir.join("USER.md");
    let runtime = MiaRuntime::new(
        model,
        wiki.clone(),
        wiki,
        graph,
        vec![Arc::new(FakeProvider)],
        user_md.clone(),
        RuntimeBudget::default(),
    )
    .await
    .unwrap();
    Fixture {
        runtime,
        user_md,
        dir,
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn question_uses_graph_and_loop() {
    let fx = setup("graph", vec![
        ScriptedModel::tool_call("graph_recall", json!({"query": "Luigi"})),
        ScriptedModel::text("Mario lavora con Luigi."),
    ])
    .await;

    let stored = fx
        .runtime
        .handle("Mario --lavora_con--> Luigi.")
        .await
        .unwrap();
    assert!(stored.text.contains("lavora_con"));

    let answer = fx.runtime.handle("Chi lavora con Luigi?").await.unwrap();
    assert_eq!(answer.text, "Mario lavora con Luigi.");
    assert_eq!(answer.outcome, OutcomeKind::Done);
    assert!(answer.tools_called >= 1);
    assert!(!fx.runtime.recorder().is_empty());
}

#[tokio::test]
async fn question_uses_wiki_and_loop() {
    let fx = setup("wiki", vec![
        ScriptedModel::tool_call("wiki_search", json!({"query": "Alpha"})),
        ScriptedModel::text("Il progetto Alpha coinvolge Mario e Luigi."),
    ])
    .await;

    let stored = fx
        .runtime
        .handle("# Progetto Alpha\n\nMario ha collaborato con Luigi al progetto Alpha.")
        .await
        .unwrap();
    assert!(stored.text.contains("progetto-alpha"));

    let answer = fx
        .runtime
        .handle("Cosa sai del progetto Alpha?")
        .await
        .unwrap();
    assert!(answer.text.contains("Alpha"));
    assert_eq!(answer.outcome, OutcomeKind::Done);
    assert!(answer.tools_called >= 1);
}

#[tokio::test]
async fn preference_goes_to_user_md() {
    let fx = setup("preference", vec![]).await;
    fx.runtime
        .handle("ricorda che preferisco risposte brevi")
        .await
        .unwrap();
    fx.runtime.handle("preferisco il buio").await.unwrap();
    let content = std::fs::read_to_string(&fx.user_md).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("preferisco risposte brevi"));
}

#[tokio::test]
async fn budget_guard_stops_endless_tool_calls() {
    let mut outputs = Vec::new();
    for _ in 0..20 {
        outputs.push(ScriptedModel::tool_call("graph_entities", json!({})));
    }
    let fx = setup("budget", outputs).await;
    let answer = fx.runtime.handle("elenca le entità").await.unwrap();
    assert_ne!(answer.outcome, OutcomeKind::Done);
    assert!(answer.iters <= RuntimeBudget::default().max_iters);
}

#[tokio::test]
async fn subagent_returns_text() {
    let fx = setup("subagent", vec![ScriptedModel::text("Risposta del sub-agente.")]).await;
    let text = fx
        .runtime
        .run_subagent("fai qualcosa di utile", 5)
        .await
        .unwrap();
    assert_eq!(text, "Risposta del sub-agente.");
}

#[tokio::test]
async fn provider_tool_is_callable_by_the_model() {
    let fx = setup("provider", vec![
        ScriptedModel::tool_call("echo", json!({"hello": "mondo"})),
        ScriptedModel::text("eco fatto."),
    ])
    .await;
    let answer = fx.runtime.handle("usa echo per favore").await.unwrap();
    assert_eq!(answer.text, "eco fatto.");
    assert!(answer.tools_called >= 1);
}
