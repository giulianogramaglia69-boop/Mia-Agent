//! `mia-core`: shared traits, types and errors for Mia Agent.
//!
//! This crate has **no behaviour of its own**: it only defines the
//! contracts every other crate implements. Engines never duplicate
//! each other's data — the wiki holds narrative, the graph holds
//! triples, `USER.md` holds preferences — and the runtime only talks
//! to engines through these traits.
//!
//! ```rust
//! use mia_core::{GraphEngine, KnowledgeEngine, ToolProvider};
//!
//! // The runtime depends on traits, never on concrete engines.
//! async fn answer<E: KnowledgeEngine>(wiki: &E, query: &str) -> mia_core::Result<String> {
//!     let pages = wiki.search(query).await?;
//!     Ok(pages.into_iter().map(|p| p.content).collect::<Vec<_>>().join("\n---\n"))
//! }
//! ```

pub mod error;
pub mod types;

pub use error::{MiaError, Result};
pub use types::{
    Entity, EntityKind, GraphFact, KnowledgeEntry, KnowledgeId, Path, PathStep, Relation,
    ToolSpec,
};

use async_trait::async_trait;

/// Narrative knowledge: compiled Markdown pages (the wiki).
///
/// Implementations ingest immutable sources into pages and answer
/// full-text searches. They never store atomic facts or preferences.
#[async_trait]
pub trait KnowledgeEngine: Send + Sync {
    /// Ingest a source (a path, a URL, or raw Markdown text) and
    /// return the id of the compiled page. Empty sources are
    /// rejected with [`MiaError::InvalidInput`].
    async fn ingest(&self, source: &str) -> Result<KnowledgeId>;

    /// Full-text search over compiled pages, best match first.
    /// An empty result is a normal outcome, not an error.
    async fn search(&self, query: &str) -> Result<Vec<KnowledgeEntry>>;
}

/// Relational knowledge: entities and triples (the graph).
///
/// Implementations store only `(subject, relation, object)` triples
/// with typed entities. They never store narrative paragraphs.
#[async_trait]
pub trait GraphEngine: Send + Sync {
    /// Extract atomic facts from `text` and store them.
    /// Returns the facts that were stored (empty when the text
    /// holds no extractable triple).
    async fn remember(&self, text: &str) -> Result<Vec<GraphFact>>;

    /// Relations touching the entity or relation named by `query`.
    async fn recall(&self, query: &str) -> Result<Vec<Relation>>;

    /// Shortest path from `from` to `to`, or `None` when the two
    /// entities are not connected.
    async fn trace(&self, from: &str, to: &str) -> Result<Option<Path>>;

    /// Every known entity, ordered by name.
    async fn entities(&self) -> Result<Vec<Entity>>;
}

/// A provider of tools that touch the real world (MCP servers).
///
/// Each tool runs in its own container; the provider only describes
/// and dispatches. Writes always require explicit user confirmation
/// at the runtime layer — never inside a provider.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Tools this provider exposes, with their JSON Schemas.
    async fn list_tools(&self) -> Result<Vec<ToolSpec>>;

    /// Call `tool` with JSON `args`. Unknown tools fail with
    /// [`MiaError::ToolNotFound`]; provider-side failures with
    /// [`MiaError::Tool`].
    async fn call(&self, tool: &str, args: serde_json::Value) -> Result<serde_json::Value>;
}
