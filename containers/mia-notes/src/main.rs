//! `mia-notes` — server MCP per le note (Fase 4).
//!
//! Le note sono file di testo in una cartella. Strumenti:
//! `list-notes` (sola lettura), `read-note` (sola lettura),
//! `write-note` (crea/sovrascrive, idempotente), `delete-note`
//! (distruttivo). Parla MCP su stdio: un oggetto JSON-RPC per riga.
//!
//! Uso: `mia-notes [cartella]` (default: `NOTES_DIR` o `./notes`).
//! Mai scrivere su stdout: è il canale del protocollo.

use std::path::PathBuf;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone)]
struct NotesServer {
    dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

/// Un nome di nota è solo un nome di file: niente percorsi, niente `..`.
fn sanitize(name: &str) -> Result<&str, String> {
    if name.is_empty() {
        return Err("il nome della nota non può essere vuoto".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("nome di nota non valido: {name}"));
    }
    Ok(name)
}

#[derive(Deserialize, JsonSchema)]
struct NoteName {
    /// Nome del file della nota, es. "todo.md".
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct WriteNote {
    /// Nome del file della nota.
    name: String,
    /// Contenuto (sovrascrive il file).
    content: String,
}

#[tool_router(router = tool_router)]
impl NotesServer {
    /// Elenca le note presenti.
    #[tool(
        name = "list-notes",
        description = "Elenca le note presenti, in ordine alfabetico.",
        annotations(title = "List notes", read_only_hint = true)
    )]
    async fn list_notes(&self) -> Result<Json<Vec<String>>, String> {
        let mut names = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.dir)
            .await
            .map_err(|e| e.to_string())?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            if entry.file_type().await.map_err(|e| e.to_string())?.is_file()
                && let Some(name) = entry.file_name().to_str()
            {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(Json(names))
    }

    /// Legge una nota.
    #[tool(
        name = "read-note",
        description = "Legge il contenuto di una nota.",
        annotations(title = "Read note", read_only_hint = true)
    )]
    async fn read_note(&self, p: Parameters<NoteName>) -> Result<String, String> {
        let name = sanitize(&p.0.name)?;
        tokio::fs::read_to_string(self.dir.join(name))
            .await
            .map_err(|e| e.to_string())
    }

    /// Crea o sovrascrive una nota.
    #[tool(
        name = "write-note",
        description = "Crea o sovrascrive una nota.",
        annotations(title = "Write note", destructive_hint = false, idempotent_hint = true)
    )]
    async fn write_note(&self, p: Parameters<WriteNote>) -> Result<String, String> {
        let name = sanitize(&p.0.name)?;
        tokio::fs::write(self.dir.join(name), &p.0.content)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("wrote {name}"))
    }

    /// Elimina una nota.
    #[tool(
        name = "delete-note",
        description = "Elimina una nota.",
        annotations(title = "Delete note", destructive_hint = true)
    )]
    async fn delete_note(&self, p: Parameters<NoteName>) -> Result<String, String> {
        let name = sanitize(&p.0.name)?;
        tokio::fs::remove_file(self.dir.join(name))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("deleted {name}"))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for NotesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mia-notes", "0.1.0"))
            .with_instructions(
                "Notes are text files. list-notes lists them, read-note reads one, write-note saves one, delete-note removes one.",
            )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var("NOTES_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("./notes"));
    tokio::fs::create_dir_all(&dir).await?;
    let server = NotesServer {
        dir,
        tool_router: NotesServer::tool_router(),
    };
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_accepts_plain_names() {
        assert_eq!(sanitize("todo.md"), Ok("todo.md"));
        assert_eq!(sanitize("appunti"), Ok("appunti"));
    }

    #[test]
    fn sanitize_rejects_paths() {
        assert!(sanitize("").is_err());
        assert!(sanitize("../segreto.md").is_err());
        assert!(sanitize("sotto/nota.md").is_err());
        assert!(sanitize("..\\windows.md").is_err());
        assert!(sanitize("..").is_err());
    }

    #[test]
    fn sanitize_allows_dots_inside_names() {
        // "v1.2.md" contiene punti ma non è un percorso.
        assert_eq!(sanitize("v1.2.md"), Ok("v1.2.md"));
    }
}
