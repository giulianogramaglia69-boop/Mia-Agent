//! Setup del modello: scelta del provider e salvataggio della chiave.
//!
//! Il wizard (`mia --setup`) chiede quale provider usare — Gemini, Groq,
//! DeepSeek, Muse Spark, OpenAI, Ollama locale o un endpoint
//! OpenAI-compatibile a scelta — e salva tutto in `<data-dir>/model.yaml`.
//! La chiave resta solo in quel file, sulla macchina dell'utente, e non
//! viene mai stampata. La variabile `MIA_MODEL_KEY` resta l'override:
//! se impostata, vince sul file (comportamento già esistente).

use std::error::Error;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use harness_models::LlmConfig;
use serde::{Deserialize, Serialize};

use crate::providers::ModelProvider;

/// La configurazione del modello, salvata in `<data-dir>/model.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Identificatore del provider (`ModelProvider::id`).
    pub provider: String,
    /// Chiave API (per Ollama: segnaposto `"ollama"`).
    pub api_key: String,
    /// Id del modello (es. `llama-3.3-70b-versatile`).
    pub model: String,
    /// URL base dell'API.
    pub base_url: String,
}

impl ModelConfig {
    /// Percorso del file di configurazione.
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("model.yaml")
    }

    /// Legge la configurazione, se esiste ed è valida.
    pub fn load(data_dir: &Path) -> Option<Self> {
        let text = fs::read_to_string(Self::path(data_dir)).ok()?;
        serde_yaml::from_str(&text).ok()
    }

    /// Salva la configurazione (crea la cartella dati se serve).
    pub fn save(&self, data_dir: &Path) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(data_dir)?;
        let text = serde_yaml::to_string(self)?;
        fs::write(Self::path(data_dir), text)?;
        Ok(())
    }

    /// Il provider, risolto dall'identificatore (sconosciuto → Custom).
    pub fn provider(&self) -> ModelProvider {
        ModelProvider::from_id(&self.provider).unwrap_or(ModelProvider::Custom)
    }

    /// La configurazione che `OpenAiCompat` vuole.
    pub fn to_llm_config(&self) -> LlmConfig {
        LlmConfig::new(
            "mia-model",
            self.base_url.clone(),
            self.api_key.clone(),
            self.model.clone(),
        )
    }
}

/// Wizard interattivo: sceglie il provider, chiede modello e chiave,
/// salva `model.yaml`. `input`/`out` sono iniettabili per i test.
pub fn run_setup(
    data_dir: &Path,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<ModelConfig, Box<dyn Error>> {
    writeln!(out, "Setup del modello di Mia — scegli il provider:")?;
    for (i, p) in ModelProvider::ALL.iter().enumerate() {
        let default = if p.default_model().is_empty() {
            String::new()
        } else {
            format!(" (modello: {})", p.default_model())
        };
        writeln!(out, "  {}. {}{}", i + 1, p.label(), default)?;
    }
    writeln!(out, "  (q per annullare)")?;

    let provider = loop {
        let raw = read_line(input, out, "Provider [1-7]: ")?;
        if raw.eq_ignore_ascii_case("q") {
            return Err("setup annullato".into());
        }
        match raw.parse::<usize>() {
            Ok(n) => match ModelProvider::from_number(n) {
                Some(p) => break p,
                None => writeln!(out, "Scelta non valida, riprova.")?,
            },
            Err(_) => writeln!(out, "Scrivi un numero da 1 a 7 (o q).")?,
        }
    };

    let (base_url, model) = if provider == ModelProvider::Custom {
        let url = loop {
            let raw = read_line(
                input,
                out,
                "URL base dell'API (es. https://api.example.com/v1): ",
            )?;
            if raw.starts_with("http://") || raw.starts_with("https://") {
                break raw;
            }
            writeln!(out, "L'URL deve iniziare con http:// o https://.")?;
        };
        let model = loop {
            let raw = read_line(input, out, "Modello: ")?;
            if !raw.is_empty() {
                break raw;
            }
            writeln!(out, "Il modello non può essere vuoto.")?;
        };
        (url, model)
    } else {
        let model = read_line(
            input,
            out,
            &format!("Modello [{}]: ", provider.default_model()),
        )?;
        let model = if model.is_empty() {
            provider.default_model().to_string()
        } else {
            model
        };
        (provider.base_url().to_string(), model)
    };

    let api_key = if !provider.needs_key() {
        "ollama".to_string()
    } else if let Some(var) = provider.key_env_var()
        && let Ok(key) = std::env::var(var)
        && !key.trim().is_empty()
    {
        let use_it = read_line(
            input,
            out,
            &format!("Ho trovato {var} nell'ambiente. Usarla? [S/n]: "),
        )?;
        if use_it.eq_ignore_ascii_case("n") {
            ask_key(input, out, &provider)?
        } else {
            key
        }
    } else {
        ask_key(input, out, &provider)?
    };

    let config = ModelConfig {
        provider: provider.id().to_string(),
        api_key,
        model,
        base_url,
    };
    config.save(data_dir)?;
    writeln!(out)?;
    writeln!(out, "Salvato: {} — modello {}", provider.label(), config.model)?;
    writeln!(out, "File: {}", ModelConfig::path(data_dir).display())?;
    writeln!(out, "La chiave resta solo in quel file, su questa macchina.")?;
    Ok(config)
}

/// Chiede la chiave (mostra dove crearla) finché non è vuota.
fn ask_key(
    input: &mut impl BufRead,
    out: &mut impl Write,
    provider: &ModelProvider,
) -> Result<String, Box<dyn Error>> {
    if let Some(url) = provider.key_url() {
        writeln!(out, "Crea la chiave qui: {url}")?;
    }
    loop {
        let raw = read_line(input, out, "Chiave API: ")?;
        if raw.eq_ignore_ascii_case("q") {
            return Err("setup annullato".into());
        }
        if !raw.is_empty() {
            return Ok(raw);
        }
        writeln!(out, "La chiave non può essere vuota (o q per annullare).")?;
    }
}

/// Scrive il prompt, legge una riga e la restituisce senza spazi.
fn read_line(
    input: &mut impl BufRead,
    out: &mut impl Write,
    prompt: &str,
) -> Result<String, Box<dyn Error>> {
    write!(out, "{prompt}")?;
    out.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Err("input terminato".into());
    }
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn tmp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mia-setup-test-{tag}-{}", std::process::id()))
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tmp_dir("roundtrip");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(ModelConfig::load(&dir), None);

        let config = ModelConfig {
            provider: "groq".to_string(),
            api_key: "gsk_test".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
        };
        config.save(&dir).expect("salvataggio");
        assert_eq!(ModelConfig::load(&dir), Some(config.clone()));
        assert_eq!(config.provider(), ModelProvider::Groq);

        let llm = config.to_llm_config();
        assert_eq!(llm.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(llm.api_key, "gsk_test");
        assert_eq!(llm.model, "llama-3.3-70b-versatile");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wizard_groq_with_defaults() {
        let dir = tmp_dir("wizard");
        let _ = fs::remove_dir_all(&dir);
        // Scelta 2 (Groq), Invio per il modello di default, chiave digitata.
        let mut input = Cursor::new(b"2\n\nsk-groq-test\n".to_vec());
        let mut output = Vec::new();
        let config = run_setup(&dir, &mut input, &mut output).expect("wizard");
        assert_eq!(config.provider, "groq");
        assert_eq!(config.model, "llama-3.3-70b-versatile");
        assert_eq!(config.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(config.api_key, "sk-groq-test");
        assert_eq!(ModelConfig::load(&dir), Some(config));
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.contains("Groq"), "l'output nomina Groq");
        assert!(!text.contains("sk-groq-test"), "la chiave non viene ristampata");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wizard_abort_with_q() {
        let dir = tmp_dir("abort");
        let _ = fs::remove_dir_all(&dir);
        let mut input = Cursor::new(b"q\n".to_vec());
        let mut output = Vec::new();
        let err = run_setup(&dir, &mut input, &mut output).expect_err("si annulla");
        assert!(err.to_string().contains("annullato"));
        assert_eq!(ModelConfig::load(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
