//! `mia-browser` — server MCP per il web (Fase 4).
//!
//! Un solo strumento, `fetch-url` (sola lettura): scarica una pagina
//! http(s) e la restituisce come testo (l'HTML viene ripulito dai tag).
//! Niente JavaScript, niente navigazione: è un lettore, non un browser
//! completo — il rendering JS è un passo successivo, non un segnaposto.
//! Limiti: solo http/https, timeout 15 s, corpo troncato a 100.000
//! caratteri.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Corpo massimo restituito al modello (caratteri).
const MAX_CHARS: usize = 100_000;

/// Scarica `url` e restituisce `Status / Content-Type / testo`.
async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("URL non valido: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("schema non supportato: {other} (solo http/https)")),
    }
    let response = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|e| format!("richiesta fallita: {e}"))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = response.text().await.map_err(|e| format!("lettura corpo: {e}"))?;
    let text = if content_type.contains("html") {
        html_to_text(&body)
    } else {
        body
    };
    let truncated: String = text.chars().take(MAX_CHARS).collect();
    Ok(format!(
        "URL: {url}\nStatus: {status}\nContent-Type: {content_type}\n\n{truncated}"
    ))
}

/// Toglie tag, script e style da una pagina HTML e compatta gli spazi.
/// Parser ingenuo ma sufficiente per leggere una pagina: non è un
/// rendering, e non pretende di esserlo.
fn html_to_text(html: &str) -> String {
    let without_blocks = strip_block(html, "script");
    let without_blocks = strip_block(&without_blocks, "style");
    let mut out = String::with_capacity(without_blocks.len());
    let mut in_tag = false;
    for ch in without_blocks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rimuove i blocchi `<tag ...>...</tag>` (case-insensitive sul nome).
fn strip_block(html: &str, tag: &str) -> String {
    let lower = html.to_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut rest_lower = lower.as_str();
    loop {
        let Some(open) = rest_lower.find(&format!("<{tag}")) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        let after_open_lower = &rest_lower[open..];
        let Some(close_tag) = after_open_lower.find(&format!("</{tag}>")) else {
            break;
        };
        let skip = close_tag + tag.len() + 3; // "</tag>"
        rest = &after_open[skip..];
        rest_lower = &after_open_lower[skip..];
    }
    out
}

#[derive(Clone)]
struct BrowserServer {
    client: reqwest::Client,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
struct FetchUrl {
    /// URL http(s) da scaricare.
    url: String,
}

#[tool_router(router = tool_router)]
impl BrowserServer {
    /// Scarica una pagina web e la restituisce come testo.
    #[tool(
        name = "fetch-url",
        description = "Scarica una pagina http(s) e la restituisce come testo.",
        annotations(title = "Fetch URL", read_only_hint = true)
    )]
    async fn fetch_url_tool(&self, p: Parameters<FetchUrl>) -> Result<String, String> {
        fetch_url(&self.client, &p.0.url).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for BrowserServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mia-browser", "0.1.0"))
            .with_instructions("fetch-url downloads an http(s) page and returns it as plain text.")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("mia-browser/0.1")
        .build()?;
    let server = BrowserServer {
        client,
        tool_router: BrowserServer::tool_router(),
    };
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_strips_tags_and_blocks() {
        let html = "<html><head><style>.x{color:red}</style><script>var a=1;</script></head>\
            <body><h1>Titolo</h1><p>Testo &amp; altro</p></body></html>";
        assert_eq!(html_to_text(html), "Titolo Testo & altro");
    }

    #[test]
    fn html_to_text_collapses_whitespace() {
        assert_eq!(html_to_text("<p>a\n\n  b\t c</p>"), "a b c");
    }

    #[test]
    fn strip_block_is_case_insensitive() {
        let html = "prima<SCRIPT>x</SCRIPT>dopo";
        assert_eq!(strip_block(html, "script"), "primadopo");
    }

    #[tokio::test]
    async fn fetch_rejects_non_http() {
        let client = reqwest::Client::new();
        assert!(fetch_url(&client, "file:///etc/passwd").await.is_err());
        assert!(fetch_url(&client, "gopher://example.com").await.is_err());
        assert!(fetch_url(&client, "non un url").await.is_err());
    }
}
