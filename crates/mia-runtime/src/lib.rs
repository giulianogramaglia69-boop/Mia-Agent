//! `mia-runtime`: the agentic loop, classification and hooks.
//!
//! Built on `harness-rs` (the `AgentLoop` ReAct loop with budgets,
//! stuck detection and hooks). The runtime owns no memory: it always
//! queries the wiki and the graph, and the model reaches them through
//! tools. The model itself is injected — any
//! [`harness_core::Model`] works, from a scripted fake in tests to an
//! OpenAI-compatible provider in production.
//!
//! ```rust
//! use std::sync::Arc;
//! use mia_core::GraphEngine;
//! use mia_graph::SqliteGraph;
//! use mia_runtime::{MiaRuntime, RuntimeBudget};
//! use mia_wiki::{FileWiki, WikiEngine};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // See tests/e2e.rs for a full run with a scripted model.
//! let dir = std::env::temp_dir().join("mia-runtime-doctest");
//! let _ = std::fs::remove_dir_all(&dir);
//! let wiki = Arc::new(FileWiki::open(dir.join("wiki")).unwrap());
//! let graph = Arc::new(SqliteGraph::open_in_memory().unwrap());
//! assert_eq!(wiki.list_pages().await.unwrap().len(), 0);
//! assert_eq!(graph.entities().await.unwrap().len(), 0);
//! let _ = std::fs::remove_dir_all(&dir);
//! # }
//! ```

pub mod classify;
pub mod hook;
pub mod runtime;
pub mod tools;

pub use classify::{classify, InputKind};
pub use hook::RecordingHook;
pub use runtime::{MiaRuntime, OutcomeKind, RuntimeAnswer, RuntimeBudget};
