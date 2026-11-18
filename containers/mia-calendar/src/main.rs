//! `mia-calendar` — server MCP per Google Calendar (Fase 4).
//!
//! Strumenti: `list-events` (sola lettura), `create-event` (scrittura),
//! `google-auth-status` (sola lettura). L'autorizzazione Google avviene
//! una volta fuori dal server, col comando `mia-calendar auth` (flusso
//! device: mostra URL + codice, attende, salva il refresh token in
//! `<data-dir>/token.json`). Il server MCP non è interattivo: il suo
//! stdout è il protocollo.
//!
//! Uso: `mia-calendar [data-dir]` (default `./data`); le credenziali
//! OAuth arrivano da `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET` o da
//! `GOOGLE_CLIENT_SECRET_FILE`.

use chrono::{DateTime, Duration, Utc};
use mia_google::GoogleAuth;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";
const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3";

#[derive(Clone)]
struct CalendarServer {
    auth: Option<GoogleAuth>,
    client: reqwest::Client,
    tool_router: ToolRouter<Self>,
}

impl CalendarServer {
    fn require_auth(&self) -> Result<&GoogleAuth, String> {
        self.auth.as_ref().ok_or_else(|| {
            "Google non configurato: imposta GOOGLE_CLIENT_ID/GOOGLE_CLIENT_SECRET (o GOOGLE_CLIENT_SECRET_FILE) e poi esegui `mia-calendar auth`".to_string()
        })
    }

    async fn get(&self, url: &str) -> Result<serde_json::Value, String> {
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
            .map_err(|e| format!("Calendar API non raggiungibile: {e}"))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Calendar API {status}: {body}"));
        }
        serde_json::from_str(&body).map_err(|e| format!("risposta incomprensibile: {e}"))
    }
}

#[derive(Deserialize, JsonSchema)]
struct ListEvents {
    /// Inizio intervallo RFC 3339 (default: ora).
    time_min: Option<String>,
    /// Fine intervallo RFC 3339 (default: tra 7 giorni).
    time_max: Option<String>,
    /// Numero massimo di eventi (default 10).
    max_results: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct CreateEvent {
    /// Titolo dell'evento.
    summary: String,
    /// Inizio RFC 3339, es. "2026-11-19T09:00:00+01:00".
    start: String,
    /// Fine RFC 3339.
    end: String,
    /// Descrizione (facoltativa).
    description: Option<String>,
    /// Luogo (facoltativo).
    location: Option<String>,
}

#[derive(Serialize)]
struct EventOut {
    id: String,
    summary: String,
    start: String,
    end: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
}

fn event_time(v: &serde_json::Value) -> String {
    v.get("dateTime")
        .and_then(|d| d.as_str())
        .or_else(|| v.get("date").and_then(|d| d.as_str()))
        .unwrap_or("")
        .to_string()
}

#[tool_router(router = tool_router)]
impl CalendarServer {
    /// Elenca gli eventi del calendario principale.
    #[tool(
        name = "list-events",
        description = "Elenca gli eventi del calendario principale in un intervallo.",
        annotations(title = "List events", read_only_hint = true)
    )]
    async fn list_events(&self, p: Parameters<ListEvents>) -> Result<String, String> {
        let now = Utc::now();
        let time_min = p.0.time_min.clone().unwrap_or_else(|| now.to_rfc3339());
        let time_max = p
            .0
            .time_max
            .clone()
            .unwrap_or_else(|| (now + Duration::days(7)).to_rfc3339());
        let max_results = p.0.max_results.unwrap_or(10).min(50);
        let url = format!(
            "{CALENDAR_API}/calendars/primary/events?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
            urlencode(&time_min),
            urlencode(&time_max),
            max_results
        );
        let value = self.get(&url).await?;
        let mut out = Vec::new();
        for item in value.get("items").and_then(|i| i.as_array()).into_iter().flatten() {
            out.push(EventOut {
                id: item["id"].as_str().unwrap_or("").to_string(),
                summary: item["summary"].as_str().unwrap_or("(senza titolo)").to_string(),
                start: event_time(&item["start"]),
                end: event_time(&item["end"]),
                location: item["location"].as_str().map(|s| s.to_string()),
            });
        }
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }

    /// Crea un evento nel calendario principale.
    #[tool(
        name = "create-event",
        description = "Crea un evento nel calendario principale.",
        annotations(title = "Create event", destructive_hint = false, idempotent_hint = false)
    )]
    async fn create_event(&self, p: Parameters<CreateEvent>) -> Result<String, String> {
        let _ = DateTime::parse_from_rfc3339(&p.0.start)
            .map_err(|_| "start non è RFC 3339 (es. 2026-11-19T09:00:00+01:00)".to_string())?;
        let _ = DateTime::parse_from_rfc3339(&p.0.end)
            .map_err(|_| "end non è RFC 3339 (es. 2026-11-19T10:00:00+01:00)".to_string())?;
        let token = self
            .require_auth()?
            .access_token()
            .await
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "summary": p.0.summary,
            "start": {"dateTime": p.0.start},
            "end": {"dateTime": p.0.end},
            "description": p.0.description,
            "location": p.0.location,
        });
        let response = self
            .client
            .post(format!("{CALENDAR_API}/calendars/primary/events"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Calendar API non raggiungibile: {e}"))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Calendar API {status}: {text}"));
        }
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("risposta incomprensibile: {e}"))?;
        Ok(format!(
            "creato evento {} ({})",
            value["id"].as_str().unwrap_or(""),
            value["summary"].as_str().unwrap_or("")
        ))
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
            Some(_) => Ok(
                "Credenziali presenti ma account non autorizzato: esegui `mia-calendar auth`."
                    .to_string(),
            ),
        }
    }
}

fn urlencode(s: &str) -> String {
    // RFC 3339 in un query param: i due punti e il + vanno codificati.
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
impl ServerHandler for CalendarServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mia-calendar", "0.1.0"))
            .with_instructions("Google Calendar: list-events lists events, create-event creates one.")
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
            eprintln!("mia-calendar: {e}");
            None
        }
    };
    let server = CalendarServer {
        auth,
        client: reqwest::Client::new(),
        tool_router: CalendarServer::tool_router(),
    };
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

/// `mia-calendar auth`: flusso device, stampa URL + codice e attende.
async fn auth_command(data_dir: &std::path::Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(data_dir).await?;
    let auth = GoogleAuth::from_env(data_dir.join("token.json"))?;
    let device = auth.start_device_flow(&[CALENDAR_SCOPE]).await?;
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
    fn urlencode_encodes_colons_and_plus() {
        assert_eq!(
            urlencode("2026-11-19T09:00:00+01:00"),
            "2026-11-19T09%3A00%3A00%2B01%3A00"
        );
    }

    #[test]
    fn event_time_prefers_datetime() {
        let v = serde_json::json!({"dateTime": "2026-11-19T09:00:00+01:00", "date": "2026-11-19"});
        assert_eq!(event_time(&v), "2026-11-19T09:00:00+01:00");
        let v = serde_json::json!({"date": "2026-11-19"});
        assert_eq!(event_time(&v), "2026-11-19");
    }
}
