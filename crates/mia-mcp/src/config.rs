//! Configurazione dei server MCP (`mcp.yaml` nella cartella dati).
//!
//! Ogni voce descrive un container/servizio MCP: come raggiungerlo
//! (processo stdio o URL Streamable HTTP), quali suoi tool sono di sola
//! lettura e cosa fare quando il modello chiama un tool di scrittura.
//! La lista dei tool di sola lettura è esplicita qui perché il client
//! MCP non espone le annotazioni del server: chi scrive la configurazione
//! decide, strumento per strumento, cosa può girare senza conferma.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Tutta la configurazione MCP (file `mcp.yaml`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McpConfig {
    /// Server MCP da collegare all'avvio, nell'ordine dato.
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

/// Un server MCP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Nome logico (es. `notes`); usato nei messaggi e nei log.
    pub name: String,
    /// Come raggiungere il server.
    #[serde(flatten)]
    pub transport: McpTransport,
    /// Tool del server che sono di sola lettura e girano senza conferma.
    /// Ogni altro tool è una scrittura e segue `write_policy`.
    #[serde(default)]
    pub read_only_tools: Vec<String>,
    /// Cosa fare per i tool di scrittura (default: rifiutare).
    #[serde(default)]
    pub write_policy: WritePolicy,
}

/// Trasporto verso un server MCP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransport {
    /// Lancia un processo e parla MCP sulla sua stdin/stdout.
    /// Per un container: `program: "docker"`, `args: ["run", "-i", "--rm", "mia-notes", ...]`.
    Stdio {
        /// Programma da lanciare (es. `mia-notes`, `docker`).
        program: String,
        /// Argomenti (default: nessuno).
        #[serde(default)]
        args: Vec<String>,
    },
    /// Server MCP su Streamable HTTP.
    Http {
        /// URL del server (es. `http://localhost:8080/mcp`).
        url: String,
    },
}

/// Politica per i tool di scrittura (tutto ciò che non è in
/// `read_only_tools`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WritePolicy {
    /// Rifiuta sempre: la scrittura non avviene mai senza un'interfaccia
    /// di conferma (la GUI non ne ha ancora una — vedi Fase 5).
    #[default]
    Deny,
    /// Esegue sempre (solo per server locali fidati, es. le note).
    Allow,
    /// Chiede all'utente sul terminale (s/n) a ogni chiamata.
    Prompt,
}

impl McpConfig {
    /// Percorso del file di configurazione.
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("mcp.yaml")
    }

    /// Legge la configurazione; file mancante o non valido → vuota.
    pub fn load(data_dir: &Path) -> Self {
        fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|text| serde_yaml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Salva la configurazione (crea la cartella dati se serve).
    pub fn save(&self, data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(data_dir)?;
        fs::write(Self::path(data_dir), serde_yaml::to_string(self)?)?;
        Ok(())
    }

    /// Esempio: note locali (scrittura libera) + browser (sola lettura).
    pub fn example() -> Self {
        Self {
            servers: vec![
                McpServerConfig {
                    name: "notes".to_string(),
                    transport: McpTransport::Stdio {
                        program: "mia-notes".to_string(),
                        args: vec!["./mia-data/notes".to_string()],
                    },
                    read_only_tools: vec![
                        "list-notes".to_string(),
                        "read-note".to_string(),
                    ],
                    write_policy: WritePolicy::Allow,
                },
                McpServerConfig {
                    name: "browser".to_string(),
                    transport: McpTransport::Stdio {
                        program: "mia-browser".to_string(),
                        args: vec![],
                    },
                    read_only_tools: vec!["fetch-url".to_string()],
                    write_policy: WritePolicy::Deny,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_round_trip() {
        let config = McpConfig::example();
        let text = serde_yaml::to_string(&config).expect("yaml");
        let back: McpConfig = serde_yaml::from_str(&text).expect("parse");
        assert_eq!(config, back);
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].write_policy, WritePolicy::Allow);
        assert_eq!(config.servers[1].write_policy, WritePolicy::Deny);
    }

    #[test]
    fn example_yaml_is_readable() {
        let text = serde_yaml::to_string(&McpConfig::example()).expect("yaml");
        assert!(text.contains("transport: stdio"), "il trasporto è leggibile:\n{text}");
        assert!(text.contains("mia-notes"), "il programma è leggibile");
        assert!(text.contains("write_policy: allow"), "la policy è leggibile");
    }

    #[test]
    fn load_missing_is_empty() {
        let dir = std::env::temp_dir().join(format!(
            "mia-mcp-test-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(McpConfig::load(&dir), McpConfig::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "mia-mcp-test-save-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let config = McpConfig::example();
        config.save(&dir).expect("salvataggio");
        assert_eq!(McpConfig::load(&dir), config);
        let _ = fs::remove_dir_all(&dir);
    }
}
