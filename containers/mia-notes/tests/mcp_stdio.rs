//! Parla MCP grezzo al server delle note su stdio e verifica
//! initialize, tools/list e due chiamate (scrittura + lettura).
//!
//! Le richieste sono inviate una alla volta aspettando la risposta:
//! il server elabora le chiamate in parallelo, quindi inviare tutto
//! insieme farebbe correre la lettura prima della scrittura.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::timeout;

struct Session {
    stdin: tokio::process::ChildStdin,
    responses: mpsc::Receiver<(i64, serde_json::Value)>,
    next_id: i64,
}

impl Session {
    async fn start(exe: &str, dir: &std::path::Path) -> Self {
        let mut child = tokio::process::Command::new(exe)
            .arg(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("avvio mia-notes");
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
            // Il server vive finché il processo test vive: quando il test
            // finisce, il figlio viene ucciso con il test.
            let _ = child.wait().await;
        });
        Self {
            stdin,
            responses: rx,
            next_id: 1,
        }
    }

    /// Invia una richiesta e aspetta la risposta con lo stesso id.
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

    /// Invia una notifica (senza risposta attesa).
    async fn notify(&mut self, method: &str) {
        let msg = serde_json::json!({"jsonrpc":"2.0","method":method});
        self.stdin
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .expect("scrittura");
        self.stdin.flush().await.expect("flush");
    }

    fn text_of(result: &serde_json::Value) -> String {
        result["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }
}

#[tokio::test]
async fn stdio_session_lists_and_reads() {
    let dir = std::env::temp_dir().join(format!("mia-notes-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("cartella test");

    let mut s = Session::start(env!("CARGO_BIN_EXE_mia-notes"), &dir).await;

    let init = s
        .request(
            "initialize",
            serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}),
        )
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "mia-notes", "init: {init}");
    s.notify("notifications/initialized").await;

    let list = s.request("tools/list", serde_json::json!({})).await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("nome tool"))
        .collect();
    for want in ["list-notes", "read-note", "write-note", "delete-note"] {
        assert!(names.contains(&want), "manca {want} in {names:?}");
    }

    let written = s
        .request(
            "tools/call",
            serde_json::json!({"name":"write-note","arguments":{"name":"prova.md","content":"# Prova\n"}}),
        )
        .await;
    assert!(
        Session::text_of(&written).contains("wrote prova.md"),
        "write: {written}"
    );

    let read = s
        .request(
            "tools/call",
            serde_json::json!({"name":"read-note","arguments":{"name":"prova.md"}}),
        )
        .await;
    assert!(
        Session::text_of(&read).contains("# Prova"),
        "read: {read}"
    );

    assert_eq!(
        std::fs::read_to_string(dir.join("prova.md")).expect("file nota"),
        "# Prova\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
