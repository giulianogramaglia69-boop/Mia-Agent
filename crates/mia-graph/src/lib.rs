//! `mia-graph`: relational knowledge graph on SQLite.
//!
//! Implements [`GraphEngine`](mia_core::GraphEngine) with `rusqlite`
//! (bundled SQLite). It stores only triples — `(subject, relation,
//! object)` — with typed entities. Narrative belongs to the wiki,
//! never here.
//!
//! ```rust
//! use mia_core::GraphEngine;
//! use mia_graph::SqliteGraph;
//!
//! #[tokio::main]
//! async fn main() {
//!     let graph = SqliteGraph::open_in_memory().unwrap();
//!     let facts = graph.remember("Mario --lavora_con--> Luigi.").await.unwrap();
//!     assert_eq!(facts.len(), 1);
//!     let rels = graph.recall("Mario").await.unwrap();
//!     assert_eq!(rels[0].target, "Luigi");
//! }
//! ```

pub mod extract;
pub mod graph;

pub use extract::extract_triples;
pub use graph::SqliteGraph;
