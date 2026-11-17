//! Supporting types for the traits in [`crate`].
//!
//! These types are the wire format between the engines: they are
//! plain data (`Clone + Debug + Serialize + Deserialize`), carry no
//! behaviour, and never duplicate each other — a narrative paragraph
//! lives in [`KnowledgeEntry`], a single fact in [`GraphFact`], a
//! preference nowhere (that is `USER.md`'s job).
//!
//! ```rust
//! use mia_core::{Entity, EntityKind, GraphFact, KnowledgeEntry, KnowledgeId};
//!
//! let fact = GraphFact {
//!     subject: "Mario".into(),
//!     relation: "lavora_con".into(),
//!     object: "Luigi".into(),
//! };
//! assert_eq!(fact.relation, "lavora_con");
//! ```

use serde::{Deserialize, Serialize};

/// Opaque identifier of a knowledge entry (a wiki page or chunk).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KnowledgeId(pub String);

impl KnowledgeId {
    /// Build an id from any string. Implementations decide the scheme
    /// (content hash, slug, UUID); the core only guarantees opacity.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for KnowledgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One narrative knowledge entry: a wiki page, or a chunk of one.
///
/// Returned by [`crate::KnowledgeEngine::search`] with a relevance
/// `score` in `0.0..=1.0`. It never holds atomic facts — those live
/// in the graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    /// Stable id of the entry.
    pub id: KnowledgeId,
    /// Human-readable title (page title or chunk heading).
    pub title: String,
    /// Narrative content (Markdown).
    pub content: String,
    /// Relevance of this entry for the query that produced it.
    pub score: f32,
}

/// One atomic relational fact: `(subject, relation, object)`.
///
/// Example: `("Mario", "lavora_con", "Luigi")`. The narrative
/// sentence ("Mario ha collaborato con Luigi al progetto Alpha")
/// belongs to the wiki, never here.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphFact {
    /// Name of the source entity.
    pub subject: String,
    /// Relation label, `snake_case` (`lavora_con`, `progetto`).
    pub relation: String,
    /// Name of the target entity.
    pub object: String,
}

/// A typed entity in the knowledge graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Storage id (SQLite row id). `0` means "not persisted yet".
    pub id: i64,
    /// Unique name.
    pub name: String,
    /// What kind of thing this is.
    pub kind: EntityKind,
    /// First-seen timestamp, RFC 3339.
    pub first_seen: String,
    /// Last-seen timestamp, RFC 3339.
    pub last_seen: String,
}

/// The kind of an [`Entity`], mirroring the `kind` column of the
/// graph schema (`person`, `project`, `org`, `concept`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    /// A person.
    Person,
    /// A project.
    Project,
    /// An organisation.
    Org,
    /// An idea or topic.
    Concept,
    /// Anything the extractor could not classify.
    #[serde(other)]
    Unknown,
}

/// A directed, weighted relation between two entities, by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// Name of the source entity.
    pub source: String,
    /// Name of the target entity.
    pub target: String,
    /// Relation label, `snake_case`.
    pub relation: String,
    /// Strength of the relation, `0.0..=1.0` (default `1.0`).
    pub weight: f32,
}

/// A path through the graph, as an ordered list of steps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    /// Steps from the start entity to the end entity.
    pub steps: Vec<PathStep>,
}

/// One step of a [`Path`]: the entity reached, and the relation
/// that leads to the *next* step (`None` on the last step).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathStep {
    /// Name of the entity at this step.
    pub entity: String,
    /// Relation to the next step, if any.
    pub relation_to_next: Option<String>,
}

/// A tool exposed by a [`crate::ToolProvider`] (an MCP tool).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Tool name, e.g. `note_create`.
    pub name: String,
    /// One-line description for the model.
    pub description: String,
    /// JSON Schema object describing the accepted arguments.
    pub input_schema: serde_json::Value,
}
