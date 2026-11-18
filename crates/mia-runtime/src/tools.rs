//! Adapters from Mia engines and providers to harness tools.
//!
//! The loop only speaks [`harness_core::Tool`]. This module exposes
//! the wiki and the graph as tools the model can call
//! (`wiki_search`, `wiki_read`, `wiki_ingest`, `graph_remember`,
//! `graph_recall`, `graph_trace`, `graph_entities`) and wraps every
//! [`ToolProvider`](mia_core::ToolProvider) tool as-is.
//!
//! ```rust
//! use std::sync::Arc;
//! use harness_core::Tool;
//! use mia_graph::SqliteGraph;
//! use mia_runtime::tools::{EngineTool, EngineToolKind};
//! use mia_wiki::FileWiki;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let dir = std::env::temp_dir().join("mia-tools-doctest");
//! let _ = std::fs::remove_dir_all(&dir);
//! let wiki = Arc::new(FileWiki::open(&dir).unwrap());
//! let graph = Arc::new(SqliteGraph::open_in_memory().unwrap());
//! let tool = EngineTool::new(EngineToolKind::GraphEntities, wiki.clone(), wiki, graph);
//! assert_eq!(tool.name(), "graph_entities");
//! let _ = std::fs::remove_dir_all(&dir);
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use harness_core::{Tool, ToolError, ToolResult, ToolRisk, ToolSchema, World};
use mia_core::{GraphEngine, KnowledgeEngine, ToolProvider, ToolSpec};
use mia_wiki::{PageId, WikiEngine};
use serde_json::{json, Value};

/// Which engine capability one [`EngineTool`] exposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineToolKind {
    /// `wiki_search {query}` — full-text search over wiki pages.
    WikiSearch,
    /// `wiki_read {id}` — read one wiki page.
    WikiRead,
    /// `wiki_ingest {markdown}` — compile Markdown into a wiki page.
    WikiIngest,
    /// `graph_remember {text}` — store triples from text.
    GraphRemember,
    /// `graph_recall {query}` — relations touching a name.
    GraphRecall,
    /// `graph_trace {from, to}` — shortest path between two entities.
    GraphTrace,
    /// `graph_entities {}` — every known entity.
    GraphEntities,
}

fn spec_for(kind: EngineToolKind) -> (String, String, Value, ToolRisk) {
    match kind {
        EngineToolKind::WikiSearch => (
            "wiki_search".into(),
            "Search the wiki (narrative knowledge) for pages matching the query.".into(),
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
            ToolRisk::ReadOnly,
        ),
        EngineToolKind::WikiRead => (
            "wiki_read".into(),
            "Read one wiki page by id.".into(),
            json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
            ToolRisk::ReadOnly,
        ),
        EngineToolKind::WikiIngest => (
            "wiki_ingest".into(),
            "Compile Markdown text into a wiki page. Returns the page id.".into(),
            json!({
                "type": "object",
                "properties": {"markdown": {"type": "string"}},
                "required": ["markdown"]
            }),
            ToolRisk::Idempotent,
        ),
        EngineToolKind::GraphRemember => (
            "graph_remember".into(),
            "Store relational facts from text written as Subject --relation--> Object. Returns the stored facts.".into(),
            json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            }),
            ToolRisk::Idempotent,
        ),
        EngineToolKind::GraphRecall => (
            "graph_recall".into(),
            "Recall relations touching the entity or relation named by the query.".into(),
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
            ToolRisk::ReadOnly,
        ),
        EngineToolKind::GraphTrace => (
            "graph_trace".into(),
            "Find the shortest path between two entities, or null when unconnected.".into(),
            json!({
                "type": "object",
                "properties": {
                    "from": {"type": "string"},
                    "to": {"type": "string"}
                },
                "required": ["from", "to"]
            }),
            ToolRisk::ReadOnly,
        ),
        EngineToolKind::GraphEntities => (
            "graph_entities".into(),
            "List every known entity in the graph.".into(),
            json!({"type": "object", "properties": {}}),
            ToolRisk::ReadOnly,
        ),
    }
}

/// One engine capability exposed as a harness tool.
pub struct EngineTool {
    kind: EngineToolKind,
    schema: ToolSchema,
    wiki: Arc<dyn KnowledgeEngine>,
    wiki_ext: Arc<dyn WikiEngine>,
    graph: Arc<dyn GraphEngine>,
}

impl EngineTool {
    /// Build the tool for `kind` over the given engines.
    pub fn new(
        kind: EngineToolKind,
        wiki: Arc<dyn KnowledgeEngine>,
        wiki_ext: Arc<dyn WikiEngine>,
        graph: Arc<dyn GraphEngine>,
    ) -> Self {
        let (name, description, input, _risk) = spec_for(kind);
        Self {
            kind,
            schema: ToolSchema {
                name,
                description,
                input,
            },
            wiki,
            wiki_ext,
            graph,
        }
    }

    fn risk(&self) -> ToolRisk {
        spec_for(self.kind).3
    }
}

fn arg_string(args: &Value, field: &str, tool: &str) -> Result<String, ToolError> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::InvalidArgs {
            name: tool.to_string(),
            reason: format!("missing string field `{field}`"),
        })
}

#[async_trait]
impl Tool for EngineTool {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn risk(&self) -> ToolRisk {
        self.risk()
    }

    async fn invoke(&self, args: Value, _world: &mut World) -> Result<ToolResult, ToolError> {
        let name = self.schema.name.clone();
        let content = match self.kind {
            EngineToolKind::WikiSearch => {
                let query = arg_string(&args, "query", &name)?;
                let pages = self
                    .wiki
                    .search(&query)
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "pages": pages })
            }
            EngineToolKind::WikiRead => {
                let id = arg_string(&args, "id", &name)?;
                let page = self
                    .wiki_ext
                    .read(&PageId::new(id))
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "page": page })
            }
            EngineToolKind::WikiIngest => {
                let markdown = arg_string(&args, "markdown", &name)?;
                let id = self
                    .wiki
                    .ingest(&markdown)
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "page_id": id.to_string() })
            }
            EngineToolKind::GraphRemember => {
                let text = arg_string(&args, "text", &name)?;
                let facts = self
                    .graph
                    .remember(&text)
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "facts": facts })
            }
            EngineToolKind::GraphRecall => {
                let query = arg_string(&args, "query", &name)?;
                let relations = self
                    .graph
                    .recall(&query)
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "relations": relations })
            }
            EngineToolKind::GraphTrace => {
                let from = arg_string(&args, "from", &name)?;
                let to = arg_string(&args, "to", &name)?;
                let path = self
                    .graph
                    .trace(&from, &to)
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "path": path })
            }
            EngineToolKind::GraphEntities => {
                let entities = self
                    .graph
                    .entities()
                    .await
                    .map_err(|e| ToolError::Exec(e.to_string()))?;
                json!({ "entities": entities })
            }
        };
        Ok(ToolResult {
            ok: true,
            content,
            trace: None,
        })
    }
}

/// One tool of a [`ToolProvider`](mia_core::ToolProvider), exposed as-is.
///
/// The provider does not declare a risk, so the tool is registered
/// as [`ToolRisk::Idempotent`]; Phase 4 MCP servers will declare
/// their own risk per tool.
pub struct ProviderTool {
    provider: Arc<dyn ToolProvider>,
    schema: ToolSchema,
}

impl ProviderTool {
    /// Wrap `spec` of `provider` as a harness tool.
    pub fn new(provider: Arc<dyn ToolProvider>, spec: ToolSpec) -> Self {
        Self {
            provider,
            schema: ToolSchema {
                name: spec.name,
                description: spec.description,
                input: spec.input_schema,
            },
        }
    }
}

#[async_trait]
impl Tool for ProviderTool {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn risk(&self) -> ToolRisk {
        ToolRisk::Idempotent
    }

    async fn invoke(&self, args: Value, _world: &mut World) -> Result<ToolResult, ToolError> {
        let value = self
            .provider
            .call(&self.schema.name, args)
            .await
            .map_err(|e| ToolError::Exec(e.to_string()))?;
        Ok(ToolResult {
            ok: true,
            content: value,
            trace: None,
        })
    }
}
