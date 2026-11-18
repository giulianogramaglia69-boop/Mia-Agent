//! Native GUI for Mia Agent (Phase 3), built on `sequoia-ui` (wgpu).
//!
//! Three columns in a resizable `SplitPanel`: the wiki tree on the left,
//! the chat in the middle, and the knowledge-graph canvas on the right.
//! The chat talks to [`mia_runtime::MiaRuntime`]; wiki pages open from the
//! tree, and the graph redraws whenever new facts land in the graph.
//!
//! The GUI is only the shell: all knowledge still lives in `mia-wiki`
//! (narrative), `mia-graph` (relations) and `USER.md` (preferences), and
//! the runtime still classifies input before answering.

pub mod app;
pub mod chat;
pub mod layout;
pub mod model;
pub mod tree;

pub use app::MiaApp;
pub use chat::{ChatMessage, ChatRole, ChatRow};
pub use layout::{EdgeLayout, GraphLayout, NodeLayout, layout_graph};
pub use model::NoModel;
pub use tree::pages_to_nodes;
