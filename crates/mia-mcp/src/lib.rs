//! Client MCP per Mia Agent (Fase 4).
//!
//! Collega i container MCP (note, browser, calendario, mail) al runtime
//! come `ToolProvider`: ogni server diventa un fornitore di tool, con un
//! cancello esplicito per le scritture (la spec vieta scritture senza
//! conferma — niente `mail_send` diretto, mai).

pub mod config;
pub mod provider;

pub use config::{McpConfig, McpServerConfig, McpTransport, WritePolicy};
pub use provider::McpProvider;
