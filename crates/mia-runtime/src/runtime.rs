//! The runtime: classification, the harness loop, budgets, sub-agents.
//!
//! [`MiaRuntime`] owns no memory of its own — it always queries the
//! wiki and the graph. One [`MiaRuntime::handle`] call runs the
//! anti-overlap flow from the spec:
//!
//! 1. classify the input (preference → `USER.md`, fact → graph,
//!    document → wiki, question → loop);
//! 2. for a question, consult wiki + graph first and inject the
//!    results into the task, then run the harness loop;
//! 3. the model may call the engine tools (and any provider tools)
//!    during the run; knowledge they produce is already stored by
//!    the tools themselves.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use harness_context::with_profile;
use harness_core::{DynModel, Model, Task, UserProfile};
use harness_loop::{AgentLoop, Outcome};
use mia_core::{GraphEngine, KnowledgeEngine, MiaError, Result, ToolProvider};
use mia_wiki::WikiEngine;

use crate::classify::{classify, InputKind};
use crate::hook::RecordingHook;
use crate::tools::{EngineTool, EngineToolKind, ProviderTool};

/// Budgets that bound one run.
#[derive(Clone, Debug)]
pub struct RuntimeBudget {
    /// Maximum ReAct iterations per run.
    pub max_iters: u32,
    /// Deadline on each individual tool call.
    pub tool_timeout: Duration,
}

impl Default for RuntimeBudget {
    fn default() -> Self {
        Self {
            max_iters: 10,
            tool_timeout: Duration::from_secs(30),
        }
    }
}

/// How a run ended, in Mia's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    /// The model stopped on its own with an answer.
    Done,
    /// The iteration budget ran out first.
    BudgetExhausted,
    /// The stuck detector stopped a spinning run.
    Stuck,
    /// The run was cancelled.
    Cancelled,
}

/// The answer to one [`MiaRuntime::handle`] call.
#[derive(Clone, Debug)]
pub struct RuntimeAnswer {
    /// Final text for the user.
    pub text: String,
    /// Iterations the loop ran.
    pub iters: u32,
    /// Tool calls the loop dispatched.
    pub tools_called: u32,
    /// How the run ended.
    pub outcome: OutcomeKind,
}

/// The agent runtime: a model plus engines plus tools, driven by a
/// harness [`AgentLoop`].
pub struct MiaRuntime {
    model: Arc<dyn Model>,
    wiki: Arc<dyn KnowledgeEngine>,
    graph: Arc<dyn GraphEngine>,
    tools: Vec<Arc<dyn harness_core::Tool>>,
    user_md: PathBuf,
    budget: RuntimeBudget,
    recorder: Arc<RecordingHook>,
}

impl MiaRuntime {
    /// Build a runtime. The model is injected (any [`Model`]); the
    /// engines are shared with the caller. Provider tools are listed
    /// once here, so construction is async.
    pub async fn new(
        model: Arc<dyn Model>,
        wiki: Arc<dyn KnowledgeEngine>,
        wiki_ext: Arc<dyn WikiEngine>,
        graph: Arc<dyn GraphEngine>,
        providers: Vec<Arc<dyn ToolProvider>>,
        user_md: PathBuf,
        budget: RuntimeBudget,
    ) -> Result<Self> {
        let mut tools: Vec<Arc<dyn harness_core::Tool>> = Vec::new();
        for kind in [
            EngineToolKind::WikiSearch,
            EngineToolKind::WikiRead,
            EngineToolKind::WikiIngest,
            EngineToolKind::GraphRemember,
            EngineToolKind::GraphRecall,
            EngineToolKind::GraphTrace,
            EngineToolKind::GraphEntities,
        ] {
            tools.push(Arc::new(EngineTool::new(
                kind,
                wiki.clone(),
                wiki_ext.clone(),
                graph.clone(),
            )));
        }
        for provider in providers {
            for spec in provider.list_tools().await? {
                tools.push(Arc::new(ProviderTool::new(provider.clone(), spec)));
            }
        }
        Ok(Self {
            model,
            wiki,
            graph,
            tools,
            user_md,
            budget,
            recorder: Arc::new(RecordingHook::new()),
        })
    }

    /// The observability hook: every event the loop emitted so far.
    pub fn recorder(&self) -> Arc<RecordingHook> {
        self.recorder.clone()
    }

    fn build_loop(&self) -> AgentLoop<DynModel> {
        let mut loop_ = AgentLoop::new(DynModel(self.model.clone()))
            .with_tool_timeout(Some(self.budget.tool_timeout))
            .with_streaming(false)
            .with_hook(self.recorder.clone() as Arc<dyn harness_core::Hook>);
        for tool in &self.tools {
            loop_ = loop_.with_tool(tool.clone());
        }
        loop_
    }

    fn world() -> harness_core::World {
        with_profile(
            ".",
            UserProfile {
                name: Some("Giuliano".into()),
                tz: Some("Europe/Rome".into()),
                locale: Some("it-IT".into()),
                extra: Default::default(),
            },
        )
    }

    /// Handle one user input through the anti-overlap flow.
    pub async fn handle(&self, input: &str) -> Result<RuntimeAnswer> {
        match classify(input) {
            InputKind::Preference => {
                self.append_preference(input).await?;
                Ok(RuntimeAnswer {
                    text: "Preferenza salvata in USER.md.".into(),
                    iters: 0,
                    tools_called: 0,
                    outcome: OutcomeKind::Done,
                })
            }
            InputKind::RelationalFact => {
                let facts = self.graph.remember(input).await?;
                let text = if facts.is_empty() {
                    "Nessun fatto estraibile da questo testo.".into()
                } else {
                    let list: Vec<String> = facts
                        .iter()
                        .map(|f| format!("({} --{}--> {})", f.subject, f.relation, f.object))
                        .collect();
                    format!("Fatti salvati nel graph: {}.", list.join(", "))
                };
                Ok(RuntimeAnswer {
                    text,
                    iters: 0,
                    tools_called: 0,
                    outcome: OutcomeKind::Done,
                })
            }
            InputKind::Document => {
                let id = self.wiki.ingest(input).await?;
                Ok(RuntimeAnswer {
                    text: format!("Pagina salvata nella wiki: {id}."),
                    iters: 0,
                    tools_called: 0,
                    outcome: OutcomeKind::Done,
                })
            }
            InputKind::Question => self.ask(input).await,
        }
    }

    /// A question: consult the engines, inject the context, run the loop.
    async fn ask(&self, input: &str) -> Result<RuntimeAnswer> {
        let wiki_hits = self.wiki.search(input).await.unwrap_or_default();
        let graph_rels = self.graph.recall(input).await.unwrap_or_default();
        let mut context = String::new();
        if !wiki_hits.is_empty() {
            context.push_str("\n\nDalla wiki:\n");
            for hit in &wiki_hits {
                let snippet: String = hit.content.chars().take(500).collect();
                context.push_str(&format!("- {}: {}\n", hit.title, snippet));
            }
        }
        if !graph_rels.is_empty() {
            context.push_str("\nDal graph:\n");
            for rel in &graph_rels {
                context.push_str(&format!(
                    "- ({} --{}--> {})\n",
                    rel.source, rel.relation, rel.target
                ));
            }
        }
        let description = format!(
            "Domanda dell'utente: {input}{context}\n\nRispondi in italiano, usando solo il contesto sopra e gli strumenti a disposizione."
        );
        let task = Task {
            description,
            source: None,
            deadline: None,
        };
        let outcome = self
            .build_loop()
            .run_with_max_iters(task, &mut Self::world(), self.budget.max_iters)
            .await
            .map_err(|e| MiaError::Internal(e.to_string()))?;
        Ok(Self::to_answer(outcome))
    }

    /// Run a sub-task on an isolated loop (same model and tools, its
    /// own iteration budget) and return its final text.
    pub async fn run_subagent(&self, task_description: &str, max_iters: u32) -> Result<String> {
        let task = Task {
            description: task_description.to_string(),
            source: None,
            deadline: None,
        };
        let outcome = self
            .build_loop()
            .run_with_max_iters(task, &mut Self::world(), max_iters)
            .await
            .map_err(|e| MiaError::Internal(e.to_string()))?;
        Ok(match outcome {
            Outcome::Done { text, .. } => text.unwrap_or_default(),
            Outcome::BudgetExhausted { last_text, .. }
            | Outcome::Stuck { last_text, .. }
            | Outcome::Cancelled { last_text, .. } => last_text.unwrap_or_default(),
        })
    }

    fn to_answer(outcome: Outcome) -> RuntimeAnswer {
        match outcome {
            Outcome::Done {
                text,
                iters,
                tools_called,
                ..
            } => RuntimeAnswer {
                text: text.unwrap_or_else(|| "(nessuna risposta)".into()),
                iters,
                tools_called,
                outcome: OutcomeKind::Done,
            },
            Outcome::BudgetExhausted {
                iters,
                last_text,
                tools_called,
                ..
            } => RuntimeAnswer {
                text: last_text
                    .unwrap_or_else(|| "Budget esaurito prima di una risposta.".into()),
                iters,
                tools_called,
                outcome: OutcomeKind::BudgetExhausted,
            },
            Outcome::Stuck {
                iters,
                last_text,
                tools_called,
                ..
            } => RuntimeAnswer {
                text: last_text.unwrap_or_else(|| "Esecuzione interrotta: loop bloccato.".into()),
                iters,
                tools_called,
                outcome: OutcomeKind::Stuck,
            },
            Outcome::Cancelled {
                iters,
                last_text,
                tools_called,
                ..
            } => RuntimeAnswer {
                text: last_text.unwrap_or_else(|| "Esecuzione annullata.".into()),
                iters,
                tools_called,
                outcome: OutcomeKind::Cancelled,
            },
        }
    }

    /// Append a preference line to `USER.md`, keeping at most 20 lines.
    async fn append_preference(&self, input: &str) -> Result<()> {
        let line = input.trim();
        let mut lines: Vec<String> = match tokio::fs::read_to_string(&self.user_md).await {
            Ok(content) => content.lines().map(str::to_string).collect(),
            Err(_) => Vec::new(),
        };
        if !lines.iter().any(|l| l.trim() == line) {
            lines.push(line.to_string());
        }
        while lines.len() > 20 {
            lines.remove(0);
        }
        if let Some(parent) = self.user_md.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| MiaError::Storage(e.to_string()))?;
        }
        tokio::fs::write(&self.user_md, lines.join("\n") + "\n")
            .await
            .map_err(|e| MiaError::Storage(e.to_string()))?;
        Ok(())
    }
}
