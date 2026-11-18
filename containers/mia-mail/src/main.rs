//! `mia-mail` — server MCP per Gmail (Fase 4).
//!
//! Strumenti: `list-messages` (sola lettura), `read-message` (sola
//! lettura), `send-message` (scrittura — la spec vieta l'invio diretto
//! senza conferma: il cancello sta nel provider MCP di Mia, che per
//! `send-message` richiede `write_policy` esplicita), `google-auth-status`
//! (sola lettura). L'autorizzazione Google avviene una volta col comando
//! `mia-mail auth` (flusso device).
//!
//! Uso: `mia-mail [data-dir]` (default `./data`); credenziali OAuth da
//! `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET` o `GOOGLE_CLIENT_SECRET_FILE`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use mia_google::GoogleAuth;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAIL_SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";
const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1";

#[derive(Clone)]
struct MailServer {
    auth: Option<GoogleAuth>,
    client: reqwest::Client,
    tool_router: ToolRouter<Self>,
}

impl MailServer {
    fn require_auth(&self) -> Result<&GoogleAuth, String> {
        self.auth.as_ref().ok_or_else(|| {
            "Google non configurato: imposta GOOGLE_CLIENT_ID/GOOGLE_CLIENT_SECRET (o GOOGLE_CLIENT_SECRET_FILE) e poi esegui `mia-mail auth`".to_string()
        })
    }

    async fn api_get(&self, url: &str) -> Result<serde_json::Value, String> {
        let token = self
            .require_auth()?
            .access_token()
            .await
            .map_err(|e| e.to_string())?;
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Gmail API non raggiungibile: {e}"))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Gmail API {status}: {body}"));
        }
        serde_json::from_str(&body).map_err(|e| format!("risposta incomprensibile: {e}"))
    }
}

/// Estrae il corpo test/plain da un payload Gmail (ricorsivo).
fn plain_body(payload: &serde_json::Value) -> Option<String> {
    if payload["mimeType"].as_str() == Some("text/plain")
        && let Some(data) = payload["body"]["data"].as_str()
        && let Ok(bytes) = URL_SAFE_NO_PAD.decode(data)
    {
        return String::from_utf8(bytes).ok();
    }
    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            if let Some(text) = plain_body(part) {
                return Some(text);
            }
        }
    }
    None
}

fn header<'a>(message: &'a serde_json::Value, name: &str) -> &'a str {
    message["payload"]["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
}

#[derive(Deserialize, JsonSchema)]
struct ListMessages {
    /// Ricerca Gmail (es. "from:amazon is:unread"), facoltativa.
    query: Option<String>,
    /// Numero massimo di messaggi (default 10).
    max_results: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct ReadMessage {
    /// Id del messaggio Gmail.
    id: String,
}

#[derive(Deserialize, JsonSchema)]
struct SendMessage {
    /// Destinatario.
    to: String,
    /// Oggetto.
    subject: String,
    /// Corpo in testo semplice.
    body: String,
}

#[derive(Serialize)]
struct MessageOut {
    id: String,
    from: String,
    subject: String,
    date: String,
    snippet: String,
}

#[tool_router(router = tool_router)]
impl MailServer {
    /// Elenca i messaggi (con mittente, oggetto e data).
    #[tool(
        name = "list-messages",
        description = "Elenca i messaggi Gmail (mittente, oggetto, data).",
        annotations(title = "List messages", read_only_hint = true)
    )]
    async fn list_messages(&self, p: Parameters<ListMessages>) -> Result<String, String> {
        let max_results = p.0.max_results.unwrap_or(10).min(25);
        let mut url = format!("{GMAIL_API}/users/me/messages?maxResults={max_results}");
        if let Some(q) = &p.0.query {
            url.push_str(&format!("&q={}", urlencode(q)));
        }
        let list = self.api_get(&url).await?;
        let mut out = Vec::new();
        for item in list.get("messages").and_then(|m| m.as_array()).into_iter().flatten() {
            let id = item["id"].as_str().unwrap_or("");
            let message = self
                .api_get(&format!(
                    "{GMAIL_API}/users/me/messages/{id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date"
                ))
                .await?;
            out.push(MessageOut {
                id: id.to_string(),
                from: header(&message, "From").to_string(),
                subject: header(&message, "Subject").to_string(),
                date: header(&message, "Date").to_string(),
                snippet: message["snippet"].as_str().unwrap_or("").to_string(),
            });
        }
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }

    /// Legge un messaggio (corpo in testo semplice).
    #[tool(
        name = "read-message",
        description = "Legge il corpo di un messaggio Gmail.",
        annotations(title = "Read message", read_only_hint = true)
    )]
    async fn read_message(&self, p: Parameters<ReadMessage>) -> Result<String, String> {
        let message = self
            .api_get(&format!(
                "{GMAIL_API}/users/me/messages/{}?format=full",
                urlencode(&p.0.id)
            ))
            .await?;
        let body = plain_body(&message["payload"])
            .unwrap_or_else(|| message["snippet"].as_str().unwrap_or("").to_string());
        Ok(format!(
            "From: {}\nSubject: {}\nDate: {}\n\n{}",
            header(&message, "From"),
            header(&message, "Subject"),
            header(&message, "Date"),
            body
        ))
    }

    /// Invia un messaggio (richiede conferma nel provider MCP).
    #[tool(
        name = "send-message",
        description = "Invia un messaggio Gmail.",
        annotations(title = "Send message", destructive_hint = true)
    )]
    async fn send_message(&self, p: Parameters<SendMessage>) -> Result<String, String> {
        let token = self
            .require_auth()?
            .access_token()
            .await
            .map_err(|e| e.to_string())?;
        let raw = format!(
            "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}",
            p.0.to, p.0.subject, p.0.body
        );
        let encoded = URL_SAFE_NO_PAD.encode(raw.as_bytes());
        let response = self
            .client
            .post(format!("{GMAIL_API}/users/me/messages/send"))
            .bearer_auth(token)
            .json(&serde_json::json!({"raw": encoded}))
            .send()
            .await
            .map_err(|e| format!("Gmail API non raggiungibile: {e}"))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Gmail API {status}: {text}"));
        }
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("risposta incomprensibile: {e}"))?;
        Ok(format!("inviato messaggio {}", value["id"].as_str().unwrap_or("")))
    }

    /// Dice se Google è autorizzato.
    #[tool(
        name = "google-auth-status",
        description = "Dice se l'account Google è configurato e autorizzato.",
        annotations(title = "Google auth status", read_only_hint = true)
    )]
    async fn auth_status(&self) -> Result<String, String> {
        match &self.auth {
            None => Ok("Google non configurato: mancano le credenziali OAuth.".to_string()),
            Some(auth) if auth.has_refresh_token() => {
                Ok("Google autorizzato (refresh token salvato).".to_string())
            }
            Some(_) => {
                Ok("Credenziali presenti ma account non autorizzato: esegui `mia-mail auth`.".to_string())
            }
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MailServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mia-mail", "0.1.0"))
            .with_instructions(
                "Gmail: list-messages lists messages, read-message reads one, send-message sends one (needs confirmation).",
            )
    }
}

fn data_dir_from(args: &[String]) -> std::path::PathBuf {
    args.iter()
        .find(|a| !a.starts_with('-'))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("./data"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("auth") {
        return auth_command(&data_dir_from(&args[1..])).await;
    }
    let data_dir = data_dir_from(&args);
    tokio::fs::create_dir_all(&data_dir).await?;
    let auth = match GoogleAuth::from_env(data_dir.join("token.json")) {
        Ok(auth) => Some(auth),
        Err(e) => {
            eprintln!("mia-mail: {e}");
            None
        }
    };
    let server = MailServer {
        auth,
        client: reqwest::Client::new(),
        tool_router: MailServer::tool_router(),
    };
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

/// `mia-mail auth`: flusso device, stampa URL + codice e attende.
async fn auth_command(data_dir: &std::path::Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(data_dir).await?;
    let auth = GoogleAuth::from_env(data_dir.join("token.json"))?;
    let device = auth.start_device_flow(&[MAIL_SCOPE]).await?;
    println!("Apri questa pagina e inserisci il codice:");
    println!("  {}", device.verification_url_complete.as_deref().unwrap_or(&device.verification_url));
    println!("Codice: {}", device.user_code);
    println!("Attendo l'autorizzazione…");
    auth.poll_device_flow(&device).await?;
    println!("Autorizzazione salvata in {}.", data_dir.join("token.json").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_body_decodes_base64url() {
        let encoded = URL_SAFE_NO_PAD.encode("Ciao mondo");
        let payload = serde_json::json!({
            "mimeType": "multipart/alternative",
            "parts": [
                {"mimeType": "text/html", "body": {"data": URL_SAFE_NO_PAD.encode("<b>x</b>")}},
                {"mimeType": "text/plain", "body": {"data": encoded}}
            ]
        });
        assert_eq!(plain_body(&payload).as_deref(), Some("Ciao mondo"));
    }

    #[test]
    fn plain_body_missing_returns_none() {
        let payload = serde_json::json!({"mimeType": "text/html", "body": {"data": "eA"}});
        assert_eq!(plain_body(&payload), None);
    }

    #[test]
    fn header_is_case_insensitive() {
        let message = serde_json::json!({"payload": {"headers": [
            {"name": "subject", "value": "Prova"},
            {"name": "FROM", "value": "a@b.it"}
        ]}});
        assert_eq!(header(&message, "Subject"), "Prova");
        assert_eq!(header(&message, "from"), "a@b.it");
        assert_eq!(header(&message, "Date"), "");
    }
}
