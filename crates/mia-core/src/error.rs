//! Typed errors for Mia Agent.
//!
//! Domain code returns [`MiaError`]; binaries may convert it into
//! [`anyhow::Error`] at their boundary. No error variant ever carries
//! a secret (token, password, API key).
//!
//! ```rust
//! use mia_core::{MiaError, Result};
//!
//! fn lookup(id: &str) -> Result<String> {
//!     if id.is_empty() {
//!         return Err(MiaError::InvalidInput("id must not be empty".into()));
//!     }
//!     Err(MiaError::NotFound(id.into()))
//! }
//! ```

use thiserror::Error;

/// The single error type shared by every Mia crate.
#[derive(Debug, Error)]
pub enum MiaError {
    /// A knowledge entry, entity or tool that does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Caller-supplied input that fails validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The storage backend (file system, SQLite, tantivy) failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// A tool call failed on the provider side.
    #[error("tool error ({tool}): {message}")]
    Tool {
        /// Name of the tool that failed.
        tool: String,
        /// Provider-supplied reason.
        message: String,
    },

    /// The requested tool is not exposed by this provider.
    #[error("tool not found: {0}")]
    ToolNotFound(String),

    /// The transport to a container or MCP server failed.
    #[error("transport error: {0}")]
    Transport(String),

    /// A budget (token, time or iteration) was exhausted.
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Anything else. Prefer a specific variant when one fits.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for MiaError {
    fn from(err: serde_json::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

/// Convenience alias used by every trait in this crate.
pub type Result<T> = std::result::Result<T, MiaError>;
