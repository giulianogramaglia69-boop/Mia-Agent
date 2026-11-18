//! `fetch-url` contro un server HTTP locale: la pagina arriva come testo.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::timeout;

const PAGE: &str = "<html><head><style>.x{}</style></head>\
    <body><h1>Pagina di prova</h1><p>Contenuto vero.</p></body></html>";

/// Un server HTTP minimo su 127.0.0.1 che serve sempre `PAGE`.
async fn serve_once() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("porta").port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 4096];
        let _ = socket.read(&mut buf).await;
        let body = PAGE.as_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.write_all(body).await;
    });
    port
}

struct Session {
    stdin: tokio::process::ChildStdin,
    responses: mpsc::Receiver<(i64, serde_json::Value)>,
    next_id: i64,
}

impl Session {
    async fn start(exe: &str) -> Self {
        let mut child = tokio::process::Command::new(exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("avvio mia-browser");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                    && let Some(id) = v.get("id").and_then(|i| i.as_i64())
                {
                    let _ = tx.send((id, v)).await;
                }
            }
            let _ = child.wait().await;
        });
        Self {
            stdin,
            responses: rx,
            next_id: 1,
        }
    }

    async fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.stdin
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .expect("scrittura");
        self.stdin.flush().await.expect("flush");
        timeout(Duration::from_secs(10), async {
            loop {
                let (got, value) = self.responses.recv().await.expect("canale risposte");
                if got == id {
                    return value;
                }
            }
        })
        .await
        .expect("timeout risposta")
    }

    async fn notify(&mut self, method: &str) {
        let msg = serde_json::json!({"jsonrpc":"2.0","method":method});
        self.stdin
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .expect("scrittura");
        self.stdin.flush().await.expect("flush");
    }
}

#[tokio::test]
async fn fetch_url_returns_page_as_text() {
    let port = serve_once().await;
    let mut s = Session::start(env!("CARGO_BIN_EXE_mia-browser")).await;

    let init = s
        .request(
            "initialize",
            serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}),
        )
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "mia-browser", "init: {init}");
    s.notify("notifications/initialized").await;

    let fetched = s
        .request(
            "tools/call",
            serde_json::json!({"name":"fetch-url","arguments":{"url": format!("http://127.0.0.1:{port}/")}}),
        )
        .await;
    let text = fetched["result"]["content"][0]["text"]
        .as_str()
        .expect("testo");
    assert!(text.contains("Status: 200"), "status: {text}");
    assert!(text.contains("Pagina di prova"), "titolo: {text}");
    assert!(text.contains("Contenuto vero."), "corpo: {text}");
    assert!(!text.contains("<h1>"), "i tag devono essere rimossi: {text}");
}
