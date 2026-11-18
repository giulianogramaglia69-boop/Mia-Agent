//! `mia-wiki`: narrative knowledge, compiled in Markdown.
//!
//! Implements [`WikiEngine`] (and [`KnowledgeEngine`](mia_core::KnowledgeEngine))
//! on top of Markdown files plus a persistent tantivy index. It holds
//! summaries, study notes, project history and decisions with reasons.
//! It never holds atomic facts (graph) or user preferences (`USER.md`).
//!
//! ```rust
//! use mia_wiki::{FileWiki, Source, WikiEngine};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let dir = std::env::temp_dir().join("mia-wiki-doctest");
//! let _ = std::fs::remove_dir_all(&dir);
//! let wiki = FileWiki::open(&dir).unwrap();
//! let id = wiki.ingest(&Source::Text {
//!     title: None,
//!     markdown: "# Alpha\n\nMario e Luigi ci lavorano.".into(),
//! }).await.unwrap();
//! let hits = wiki.search("Alpha").await.unwrap();
//! assert_eq!(hits.len(), 1);
//! assert_eq!(hits[0].id, id);
//! let _ = std::fs::remove_dir_all(&dir);
//! # }
//! ```

pub mod engine;
pub mod parse;
pub mod types;

pub use engine::FileWiki;
pub use types::{Frontmatter, Page, PageId, PageMeta, Source};

use async_trait::async_trait;
use mia_core::Result;

/// Narrative knowledge: compiled Markdown pages.
///
/// `ingest` compiles a source into a page; `search` is full-text
/// (tantivy) over titles and bodies; `read` returns one page by id;
/// `list_pages` enumerates the wiki.
#[async_trait]
pub trait WikiEngine: Send + Sync {
    /// Ingest `source` and return the id of the compiled page.
    /// Empty sources are rejected with [`mia_core::MiaError::InvalidInput`].
    async fn ingest(&self, source: &Source) -> Result<PageId>;

    /// Full-text search over compiled pages, best match first.
    /// An empty result is a normal outcome, not an error.
    async fn search(&self, query: &str) -> Result<Vec<Page>>;

    /// Read one page by id.
    async fn read(&self, id: &PageId) -> Result<Page>;

    /// Every page (id + title), sorted by title.
    async fn list_pages(&self) -> Result<Vec<PageMeta>>;
}
