//! Flusso device contro un server Google finto in locale.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Server finto: /device/code → codice fisso; /token → prima
/// `authorization_pending`, poi token con refresh; i refresh successivi
/// → nuovo access token.
async fn mock_google() -> (u16, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("porta").port();
    let token_calls = Arc::new(AtomicUsize::new(0));
    let counter = token_calls.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let counter = counter.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let Ok(n) = socket.read(&mut buf).await else {
                    return;
                };
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                let body = if path.contains("device/code") {
                    serde_json::json!({
                        "device_code": "dev-1",
                        "user_code": "USER-CODE",
                        "verification_url": "https://example.com/device",
                        "verification_url_complete": "https://example.com/device?code=USER-CODE",
                        "expires_in": 600,
                        "interval": 1
                    })
                    .to_string()
                } else if path.contains("token") {
                    let n = counter.fetch_add(1, Ordering::SeqCst);
                    if req.contains("grant_type=refresh_token") {
                        serde_json::json!({"access_token": "acc-2", "expires_in": 3600})
                            .to_string()
                    } else if n == 0 {
                        serde_json::json!({"error": "authorization_pending"}).to_string()
                    } else {
                        serde_json::json!({
                            "access_token": "acc-1",
                            "refresh_token": "ref-1",
                            "expires_in": 3600
                        })
                        .to_string()
                    }
                } else {
                    serde_json::json!({"error": "not_found"}).to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (port, token_calls)
}

#[tokio::test]
async fn device_flow_then_refresh() {
    let (port, token_calls) = mock_google().await;
    let dir = std::env::temp_dir().join(format!("mia-google-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("cartella test");
    let token_path = dir.join("token.json");

    // Endpoint finti via variabili d'ambiente (vedi GoogleAuth::new).
    unsafe {
        std::env::set_var(
            "MIA_GOOGLE_DEVICE_URL",
            format!("http://127.0.0.1:{port}/device/code"),
        );
        std::env::set_var(
            "MIA_GOOGLE_TOKEN_URL",
            format!("http://127.0.0.1:{port}/token"),
        );
        std::env::set_var("GOOGLE_CLIENT_ID", "test-id");
        std::env::set_var("GOOGLE_CLIENT_SECRET", "test-secret");
    }
    let auth = mia_google::GoogleAuth::from_env(&token_path).expect("auth");
    assert!(!auth.has_refresh_token());

    let device = auth
        .start_device_flow(&["https://www.googleapis.com/auth/calendar"])
        .await
        .expect("device flow");
    assert_eq!(device.user_code, "USER-CODE");
    assert_eq!(device.interval, 1);

    auth.poll_device_flow(&device).await.expect("poll");
    assert!(auth.has_refresh_token());
    assert_eq!(token_calls.load(Ordering::SeqCst), 2, "pending + successo");

    // Access token in cache: nessuna chiamata HTTP.
    assert_eq!(auth.access_token().await.expect("cache"), "acc-1");
    assert_eq!(token_calls.load(Ordering::SeqCst), 2);

    // Scade la cache → refresh → nuovo access token.
    let saved = std::fs::read_to_string(&token_path).expect("token file");
    let mut value: serde_json::Value = serde_json::from_str(&saved).expect("json");
    value["access_expires_at"] = serde_json::json!(1);
    std::fs::write(&token_path, serde_json::to_string(&value).expect("json"))
        .expect("scrittura");
    assert_eq!(auth.access_token().await.expect("refresh"), "acc-2");

    unsafe {
        std::env::remove_var("MIA_GOOGLE_DEVICE_URL");
        std::env::remove_var("MIA_GOOGLE_TOKEN_URL");
        std::env::remove_var("GOOGLE_CLIENT_ID");
        std::env::remove_var("GOOGLE_CLIENT_SECRET");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
