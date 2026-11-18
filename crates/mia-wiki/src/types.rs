//! Types for the wiki engine.
//!
//! A [`Page`] is narrative knowledge: a compiled Markdown document.
//! It never holds atomic facts (those live in the graph) or user
//! preferences (those live in `USER.md`).
//!
//! ```rust
//! use mia_wiki::{Page, PageId};
//!
//! let page = Page {
//!     id: PageId::new("alpha"),
//!     title: "Progetto Alpha".into(),
//!     content: "# Progetto Alpha\n\nMario e Luigi ci lavorano.".into(),
//!     frontmatter: Default::default(),
//!     score: 0.0,
//! };
//! assert_eq!(page.title, "Progetto Alpha");
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Opaque identifier of a wiki page (its slug).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub String);

impl PageId {
    /// Build an id from a slug.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One compiled wiki page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    /// Stable id (slug).
    pub id: PageId,
    /// Human-readable title.
    pub title: String,
    /// Full Markdown content (frontmatter excluded).
    pub content: String,
    /// Parsed frontmatter metadata.
    pub frontmatter: Frontmatter,
    /// Relevance for the query that produced this page, `0.0..=1.0`.
    /// `0.0` when the page was read directly, not searched.
    pub score: f32,
}

/// Index entry for a page (no content).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageMeta {
    /// Stable id (slug).
    pub id: PageId,
    /// Human-readable title.
    pub title: String,
}

/// Frontmatter metadata (`---` YAML block at the top of a page).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Title override (wins over the first H1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Free-form tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A source to ingest into the wiki.
#[derive(Clone, Debug)]
pub enum Source {
    /// Raw Markdown text. `title` overrides the title found in the text.
    Text {
        /// Optional title override.
        title: Option<String>,
        /// Markdown content.
        markdown: String,
    },
    /// A Markdown file on disk. Its content is copied into `raw/`
    /// so the wiki stays self-contained.
    File(PathBuf),
}
