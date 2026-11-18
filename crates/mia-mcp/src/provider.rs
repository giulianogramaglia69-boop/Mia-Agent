//! Un server MCP visto come `ToolProvider` di Mia.
//!
//! `McpProvider` si collega a un server MCP (processo stdio o URL HTTP)
//! con `harness-mcp-client` e ne espone i tool al runtime. Il runtime li
//! vede tutti come `Idempotent` (è `ProviderTool` a deciderlo), quindi il
//! cancello per le scritture sta qui: un tool non elencato in
//! `read_only_tools` è una scrittura e segue `write_policy` — rifiutato
//! di default, perché la spec vieta scritture senza conferma esplicita
//! (niente `mail_send` diretto, mai).

use std::sync::Arc;

use async_trait::async_trait;
use harness_core::{Tool, ToolRisk};
use harness_mcp_client::McpClient;
use mia_core::{MiaError, Result, ToolProvider, ToolSpec};

use crate::config::{McpServerConfig, McpTransport, WritePolicy};

/// Un server MCP collegato, con i suoi tool e la sua politica di scrittura.
pub struct McpProvider {
    name: String,
    /// I tool del server, come proxy harness (ognuno tiene in vita la
    /// sessione MCP finché esiste).
    tools: Vec<Arc<dyn Tool>>,
    write_policy: WritePolicy,
}

impl McpProvider {
    /// Si collega al server descritto dalla configurazione e ne elenca i tool.
    pub async fn connect(config: &McpServerConfig) -> Result<Self> {
        let client = match &config.transport {
            McpTransport::Stdio { program, args } => {
                let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
                McpClient::connect_stdio(program, &args_ref)
                    .await
                    .map_err(|e| {
                        MiaError::Transport(format!(
                            "MCP {}: avvio di `{program}` fallito: {e}",
                            config.name
                        ))
                    })?
            }
            McpTransport::Http { url } => McpClient::connect_http(url).await.map_err(|e| {
                MiaError::Transport(format!("MCP {}: connessione a `{url}` fallita: {e}", config.name))
            })?,
        };
        let read_only: Vec<&str> = config
            .read_only_tools
            .iter()
            .map(String::as_str)
            .collect();
        let tools = client.tools_with_read_only(&read_only);
        Ok(Self {
            name: config.name.clone(),
            tools,
            write_policy: config.write_policy,
        })
    }

    /// Nome logico del server.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn find(&self, tool: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == tool)
    }

    /// Il cancello per le scritture: decide se un tool di scrittura può
    /// girare, secondo `write_policy`.
    async fn confirm_write(&self, tool: &str, args: &serde_json::Value) -> Result<()> {
        match self.write_policy {
            WritePolicy::Allow => Ok(()),
            WritePolicy::Deny => Err(MiaError::Tool {
                tool: tool.to_string(),
                message: format!(
                    "scrittura non confermata: `{tool}` modifica lo stato e la conferma è richiesta (write_policy: deny)"
                ),
            }),
            WritePolicy::Prompt => {
                let question = format!(
                    "Mia vuole usare `{tool}` con argomenti {args}. Confermi? [s/n]: "
                );
                let answer = tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    let mut stdout = std::io::stdout();
                    let _ = write!(stdout, "{question}");
                    let _ = stdout.flush();
                    let mut line = String::new();
                    let _ = std::io::stdin().read_line(&mut line);
                    line.trim().to_lowercase()
                })
                .await
                .map_err(|e| MiaError::Internal(format!("lettura della conferma fallita: {e}")))?;
                match answer.as_str() {
                    "s" | "si" | "sì" | "y" | "yes" => Ok(()),
                    _ => Err(MiaError::Tool {
                        tool: tool.to_string(),
                        message: "scrittura rifiutata dall'utente".to_string(),
                    }),
                }
            }
        }
    }
}

#[async_trait]
impl ToolProvider for McpProvider {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        Ok(self
            .tools
            .iter()
            .map(|t| {
                let s = t.schema();
                ToolSpec {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_schema: s.input.clone(),
                }
            })
            .collect())
    }

    async fn call(&self, tool: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        let proxy = self
            .find(tool)
            .ok_or_else(|| MiaError::ToolNotFound(tool.to_string()))?;
        if proxy.risk() != ToolRisk::ReadOnly {
            self.confirm_write(tool, &args).await?;
        }
        // Il proxy ignora il mondo, ma il tratto Tool lo richiede: un mondo
        // di default è sufficiente.
        let mut world = harness_context::default_world(".");
        let result = proxy.invoke(args, &mut world).await.map_err(|e| MiaError::Tool {
            tool: tool.to_string(),
            message: e.to_string(),
        })?;
        if result.ok {
            Ok(result.content)
        } else {
            Err(MiaError::Tool {
                tool: tool.to_string(),
                message: result.content.to_string(),
            })
        }
    }
}
