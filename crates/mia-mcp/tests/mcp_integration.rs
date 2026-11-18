//! `McpProvider` contro il vero server delle note: connessione stdio,
//! elenco tool, lettura/scrittura e cancello delle scritture.

use mia_core::{MiaError, ToolProvider};
use mia_mcp::{McpProvider, McpServerConfig, McpTransport, WritePolicy};
use serde_json::json;

fn notes_binary() -> std::path::PathBuf {
    let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/mia-notes");
    if !exe.exists() {
        let out = std::process::Command::new("cargo")
            .args(["build", "-p", "mia-notes"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("cargo build");
        assert!(
            out.status.success(),
            "build di mia-notes fallita: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    exe
}

fn config(dir: &std::path::Path, policy: WritePolicy) -> McpServerConfig {
    McpServerConfig {
        name: "notes".to_string(),
        transport: McpTransport::Stdio {
            program: notes_binary().to_string_lossy().to_string(),
            args: vec![dir.to_string_lossy().to_string()],
        },
        read_only_tools: vec!["list-notes".to_string(), "read-note".to_string()],
        write_policy: policy,
    }
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mia-mcp-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("cartella test");
    dir
}

#[tokio::test]
async fn provider_reads_and_writes_notes() {
    let dir = tmpdir("rw");
    let provider = McpProvider::connect(&config(&dir, WritePolicy::Allow))
        .await
        .expect("connessione al server note");

    let tools = provider.list_tools().await.expect("elenco tool");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    for want in ["list-notes", "read-note", "write-note", "delete-note"] {
        assert!(names.contains(&want), "manca {want} in {names:?}");
    }

    let written = provider
        .call("write-note", json!({"name": "prova.md", "content": "# Prova\n"}))
        .await
        .expect("scrittura nota");
    assert!(
        written["text"].as_str().unwrap_or("").contains("wrote prova.md"),
        "scrittura: {written}"
    );

    let read = provider
        .call("read-note", json!({"name": "prova.md"}))
        .await
        .expect("lettura nota");
    assert!(
        read["text"].as_str().unwrap_or("").contains("# Prova"),
        "lettura: {read}"
    );

    let listed = provider
        .call("list-notes", json!({}))
        .await
        .expect("elenco note");
    assert!(
        listed.to_string().contains("prova.md"),
        "elenco: {listed}"
    );

    let err = provider.call("inesistente", json!({})).await.unwrap_err();
    assert!(
        matches!(err, MiaError::ToolNotFound(_)),
        "atteso ToolNotFound, avuto: {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deny_policy_blocks_writes() {
    let dir = tmpdir("deny");
    let provider = McpProvider::connect(&config(&dir, WritePolicy::Deny))
        .await
        .expect("connessione al server note");

    // La lettura passa anche con deny…
    let listed = provider.call("list-notes", json!({})).await;
    assert!(listed.is_ok(), "la lettura non deve essere bloccata: {listed:?}");

    // …la scrittura no.
    let err = provider
        .call("write-note", json!({"name": "x.md", "content": "x"}))
        .await
        .unwrap_err();
    match &err {
        MiaError::Tool { tool, message } => {
            assert_eq!(tool, "write-note");
            assert!(message.contains("conferma"), "messaggio: {message}");
        }
        other => panic!("atteso MiaError::Tool, avuto: {other}"),
    }
    assert!(!dir.join("x.md").exists(), "il file non deve esistere");
    let _ = std::fs::remove_dir_all(&dir);
}
