//! OAuth 2.0 per le API Google (Fase 4), flusso "device" (RFC 8628).
//!
//! Niente libreria OAuth esterna: il flusso device è due POST e un
//! polling, ed è scritto qui in chiaro così da poterlo testare contro
//! un server finto. Le credenziali (client id/secret) arrivano da
//! variabili d'ambiente o da un `client_secret.json` di Google Cloud;
//! il refresh token salvato resta solo nel file indicato (mai nel repo).
//!
//! Uso: `GoogleAuth::from_env(token_path)` → `start_device_flow(scopes)`
//! stampa `user_code` e `verification_url` (fuori dal server MCP: lo
//! stdout del server è il protocollo!) → l'utente autorizza nel browser
//! → `poll_device_flow` attende e salva il refresh token → da lì in poi
//! `access_token()` lo rinnova da solo.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mia_core::{MiaError, Result};
use serde::{Deserialize, Serialize};

/// Endpoint Google (sovrascrivibili nei test).
const DEFAULT_DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Stato di un'autorizzazione device in corso.
#[derive(Debug, Clone)]
pub struct DeviceAuth {
    /// Codice da inserire sul sito di verifica.
    pub user_code: String,
    /// URL dove inserire il codice.
    pub verification_url: String,
    /// URL con il codice già incluso (se Google lo fornisce).
    pub verification_url_complete: Option<String>,
    /// Codice segreto del dispositivo (non mostrarlo all'utente).
    pub device_code: String,
    /// Secondi di validità del codice.
    pub expires_in: u64,
    /// Secondi tra un polling e l'altro.
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    verification_url_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Token salvati su disco (solo refresh token + access token in cache).
#[derive(Debug, Default, Serialize, Deserialize)]
struct SavedTokens {
    refresh_token: Option<String>,
    access_token: Option<String>,
    /// Epoch (secondi) di scadenza dell'access token in cache.
    access_expires_at: Option<u64>,
}

/// Autenticazione Google con flusso device.
#[derive(Debug, Clone)]
pub struct GoogleAuth {
    client_id: String,
    client_secret: String,
    token_path: PathBuf,
    device_code_url: String,
    token_url: String,
    http: reqwest::Client,
}

impl GoogleAuth {
    /// Costruisce da client id/secret espliciti.
    ///
    /// Le variabili `MIA_GOOGLE_DEVICE_URL` / `MIA_GOOGLE_TOKEN_URL`,
    /// se presenti, sovrascrivono gli endpoint di Google: esistono
    /// solo per i test contro un server finto.
    pub fn new(client_id: String, client_secret: String, token_path: impl AsRef<Path>) -> Self {
        Self {
            client_id,
            client_secret,
            token_path: token_path.as_ref().to_path_buf(),
            device_code_url: std::env::var("MIA_GOOGLE_DEVICE_URL")
                .unwrap_or_else(|_| DEFAULT_DEVICE_CODE_URL.to_string()),
            token_url: std::env::var("MIA_GOOGLE_TOKEN_URL")
                .unwrap_or_else(|_| DEFAULT_TOKEN_URL.to_string()),
            http: reqwest::Client::new(),
        }
    }

    /// Da ambiente: `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET`, oppure
    /// `GOOGLE_CLIENT_SECRET_FILE` (il JSON scaricato da Google Cloud,
    /// formato `installed` o `web`).
    pub fn from_env(token_path: impl AsRef<Path>) -> Result<Self> {
        let (client_id, client_secret) = match (
            std::env::var("GOOGLE_CLIENT_ID"),
            std::env::var("GOOGLE_CLIENT_SECRET"),
        ) {
            (Ok(id), Ok(secret)) if !id.is_empty() && !secret.is_empty() => (id, secret),
            _ => {
                let file = std::env::var("GOOGLE_CLIENT_SECRET_FILE").map_err(|_| {
                    MiaError::InvalidInput(
                        "manca GOOGLE_CLIENT_ID/GOOGLE_CLIENT_SECRET o GOOGLE_CLIENT_SECRET_FILE"
                            .to_string(),
                    )
                })?;
                read_client_secret_file(&file)?
            }
        };
        Ok(Self::new(client_id, client_secret, token_path))
    }

    /// Percorso del file dei token.
    pub fn token_path(&self) -> &Path {
        &self.token_path
    }

    /// C'è un refresh token salvato?
    pub fn has_refresh_token(&self) -> bool {
        self.load_tokens()
            .map(|t| t.refresh_token.is_some())
            .unwrap_or(false)
    }

    /// Avvia il flusso device: ritorna il codice da mostrare all'utente.
    pub async fn start_device_flow(&self, scopes: &[&str]) -> Result<DeviceAuth> {
        let response = self
            .http
            .post(&self.device_code_url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", &scopes.join(" ")),
            ])
            .send()
            .await
            .map_err(|e| MiaError::Transport(format!("device/code non raggiungibile: {e}")))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| MiaError::Transport(format!("lettura device/code: {e}")))?;
        if !status.is_success() {
            return Err(MiaError::Transport(format!(
                "device/code ha risposto {status}: {body}"
            )));
        }
        let parsed: DeviceCodeResponse = serde_json::from_str(&body).map_err(|e| {
            MiaError::Transport(format!("device/code: risposta incomprensibile: {e}"))
        })?;
        Ok(DeviceAuth {
            user_code: parsed.user_code,
            verification_url: parsed.verification_url,
            verification_url_complete: parsed.verification_url_complete,
            device_code: parsed.device_code,
            expires_in: parsed.expires_in,
            interval: parsed.interval.unwrap_or(5),
        })
    }

    /// Attende l'autorizzazione dell'utente (polling) e salva i token.
    /// Ritorna quando l'utente ha autorizzato, o errore se il codice
    /// scade o l'utente rifiuta.
    pub async fn poll_device_flow(&self, auth: &DeviceAuth) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(auth.expires_in);
        let mut interval = Duration::from_secs(auth.interval.max(1));
        loop {
            if Instant::now() >= deadline {
                return Err(MiaError::Timeout(Duration::from_secs(auth.expires_in)));
            }
            tokio::time::sleep(interval).await;
            let response = self
                .http
                .post(&self.token_url)
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("client_secret", self.client_secret.as_str()),
                    ("device_code", auth.device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await
                .map_err(|e| MiaError::Transport(format!("token non raggiungibile: {e}")))?;
            let parsed: TokenResponse = response.json().await.map_err(|e| {
                MiaError::Transport(format!("token: risposta incomprensibile: {e}"))
            })?;
            match parsed.error.as_deref() {
                None => {
                    let access = parsed.access_token.ok_or_else(|| {
                        MiaError::Transport("token: manca access_token".to_string())
                    })?;
                    self.save_tokens(SavedTokens {
                        refresh_token: parsed.refresh_token,
                        access_token: Some(access),
                        access_expires_at: parsed
                            .expires_in
                            .map(|s| now_epoch() + s.saturating_sub(30)),
                    })?;
                    return Ok(());
                }
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    interval += Duration::from_secs(5);
                }
                Some("access_denied") => {
                    return Err(MiaError::InvalidInput(
                        "autorizzazione rifiutata dall'utente".to_string(),
                    ));
                }
                Some("expired_token") => {
                    return Err(MiaError::InvalidInput(
                        "codice scaduto: avvia di nuovo il flusso".to_string(),
                    ));
                }
                Some(other) => {
                    return Err(MiaError::Transport(format!(
                        "token: {other}{}",
                        parsed
                            .error_description
                            .map(|d| format!(": {d}"))
                            .unwrap_or_default()
                    )));
                }
            }
        }
    }

    /// Un access token valido: dalla cache se non scaduto, altrimenti
    /// rinnovato col refresh token. Senza refresh token → errore che
    /// spiega di eseguire il flusso device (CLI `auth` del server).
    pub async fn access_token(&self) -> Result<String> {
        let mut tokens = self.load_tokens()?;
        if let (Some(access), Some(expires_at)) = (&tokens.access_token, tokens.access_expires_at)
            && expires_at > now_epoch() + 10
        {
            return Ok(access.clone());
        }
        let refresh = tokens.refresh_token.clone().ok_or_else(|| {
            MiaError::InvalidInput(
                "Google non autorizzato: esegui l'autenticazione device (comando `auth` del server)"
                    .to_string(),
            )
        })?;
        let response = self
            .http
            .post(&self.token_url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|e| MiaError::Transport(format!("refresh non raggiungibile: {e}")))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| MiaError::Transport(format!("lettura refresh: {e}")))?;
        if !status.is_success() {
            return Err(MiaError::Transport(format!(
                "refresh fallito ({status}): {body}"
            )));
        }
        let parsed: TokenResponse = serde_json::from_str(&body).map_err(|e| {
            MiaError::Transport(format!("refresh: risposta incomprensibile: {e}"))
        })?;
        let access = parsed.access_token.ok_or_else(|| {
            MiaError::Transport("refresh: manca access_token".to_string())
        })?;
        tokens.access_token = Some(access.clone());
        tokens.access_expires_at = parsed
            .expires_in
            .map(|s| now_epoch() + s.saturating_sub(30));
        self.save_tokens(tokens)?;
        Ok(access)
    }

    fn load_tokens(&self) -> Result<SavedTokens> {
        match std::fs::read_to_string(&self.token_path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| MiaError::Storage(format!("token file corrotto: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SavedTokens::default()),
            Err(e) => Err(MiaError::Storage(format!(
                "lettura token file: {e}"
            ))),
        }
    }

    fn save_tokens(&self, tokens: SavedTokens) -> Result<()> {
        if let Some(parent) = self.token_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MiaError::Storage(format!("creazione cartella token: {e}")))?;
        }
        let text = serde_json::to_string(&tokens)
            .map_err(|e| MiaError::Storage(format!("serializzazione token: {e}")))?;
        std::fs::write(&self.token_path, text)
            .map_err(|e| MiaError::Storage(format!("scrittura token file: {e}")))?;
        Ok(())
    }
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Legge client id/secret dal JSON di Google Cloud (`installed` o `web`).
fn read_client_secret_file(path: &str) -> Result<(String, String)> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        MiaError::InvalidInput(format!("lettura {path}: {e}"))
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        MiaError::InvalidInput(format!("{path} non è JSON valido: {e}"))
    })?;
    for section in ["installed", "web"] {
        if let Some(client_id) = value[section]["client_id"].as_str()
            && let Some(secret) = value[section]["client_secret"].as_str()
        {
            return Ok((client_id.to_string(), secret.to_string()));
        }
    }
    Err(MiaError::InvalidInput(format!(
        "{path}: mancano client_id/client_secret in installed o web"
    )))
}
