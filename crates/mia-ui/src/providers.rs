//! Provider di modelli: i preset tra cui si sceglie nel setup.
//!
//! Tutti i provider qui sono compatibili con il protocollo OpenAI
//! (`OpenAiCompat` di harness-models): cambia solo l'URL base, il modello
//! di default e la variabile d'ambiente della chiave. La chiave non sta
//! mai nel codice: la scrive l'utente nel setup (file `model.yaml`) o la
//! passa via variabile d'ambiente.

/// Un provider di modelli tra cui scegliere nel setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvider {
    /// Google Gemini, via endpoint compatibile OpenAI.
    Gemini,
    /// Groq (inferenza veloce, modelli open).
    Groq,
    /// DeepSeek.
    DeepSeek,
    /// Muse Spark (Meta Model API).
    Meta,
    /// OpenAI.
    OpenAI,
    /// Ollama in locale (nessuna chiave, nessun cloud).
    Ollama,
    /// Endpoint compatibile OpenAI a scelta dell'utente.
    Custom,
}

impl ModelProvider {
    /// Tutti i provider, nell'ordine mostrato nel setup.
    pub const ALL: [ModelProvider; 7] = [
        ModelProvider::Gemini,
        ModelProvider::Groq,
        ModelProvider::DeepSeek,
        ModelProvider::Meta,
        ModelProvider::OpenAI,
        ModelProvider::Ollama,
        ModelProvider::Custom,
    ];

    /// Identificatore stabile, usato nel file `model.yaml`.
    pub fn id(&self) -> &'static str {
        match self {
            ModelProvider::Gemini => "gemini",
            ModelProvider::Groq => "groq",
            ModelProvider::DeepSeek => "deepseek",
            ModelProvider::Meta => "meta",
            ModelProvider::OpenAI => "openai",
            ModelProvider::Ollama => "ollama",
            ModelProvider::Custom => "custom",
        }
    }

    /// Dall'identificatore del file di configurazione.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Dal numero digitato nel setup (1-based, come mostrato in lista).
    pub fn from_number(n: usize) -> Option<Self> {
        Self::ALL.get(n.wrapping_sub(1)).copied()
    }

    /// Etichetta mostrata nel setup.
    pub fn label(&self) -> &'static str {
        match self {
            ModelProvider::Gemini => "Gemini (Google)",
            ModelProvider::Groq => "Groq",
            ModelProvider::DeepSeek => "DeepSeek",
            ModelProvider::Meta => "Muse Spark (Meta)",
            ModelProvider::OpenAI => "OpenAI",
            ModelProvider::Ollama => "Ollama (in locale, senza chiave)",
            ModelProvider::Custom => "Personalizzato (endpoint OpenAI-compatibile)",
        }
    }

    /// URL base dell'API (vuoto per Personalizzato: lo chiede il setup).
    pub fn base_url(&self) -> &'static str {
        match self {
            ModelProvider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai/",
            ModelProvider::Groq => "https://api.groq.com/openai/v1",
            ModelProvider::DeepSeek => "https://api.deepseek.com/v1",
            ModelProvider::Meta => "https://api.meta.ai/v1",
            ModelProvider::OpenAI => "https://api.openai.com/v1",
            ModelProvider::Ollama => "http://localhost:11434/v1",
            ModelProvider::Custom => "",
        }
    }

    /// Modello proposto di default (Invio nel setup per accettarlo).
    pub fn default_model(&self) -> &'static str {
        match self {
            ModelProvider::Gemini => "gemini-2.5-flash",
            ModelProvider::Groq => "llama-3.3-70b-versatile",
            ModelProvider::DeepSeek => "deepseek-chat",
            ModelProvider::Meta => "muse-spark-1.1",
            ModelProvider::OpenAI => "gpt-4o-mini",
            ModelProvider::Ollama => "llama3.1",
            ModelProvider::Custom => "",
        }
    }

    /// Variabile d'ambiente da cui leggere la chiave, se esiste.
    /// Ollama non usa chiavi: il setup scrive un segnaposto.
    pub fn key_env_var(&self) -> Option<&'static str> {
        match self {
            ModelProvider::Gemini => Some("GEMINI_API_KEY"),
            ModelProvider::Groq => Some("GROQ_API_KEY"),
            ModelProvider::DeepSeek => Some("DEEPSEEK_API_KEY"),
            ModelProvider::Meta => Some("MIA_MODEL_KEY"),
            ModelProvider::OpenAI => Some("OPENAI_API_KEY"),
            ModelProvider::Ollama => None,
            ModelProvider::Custom => Some("MIA_MODEL_KEY"),
        }
    }

    /// Dove creare la chiave (mostrato nel setup).
    pub fn key_url(&self) -> Option<&'static str> {
        match self {
            ModelProvider::Gemini => Some("https://aistudio.google.com/apikey"),
            ModelProvider::Groq => Some("https://console.groq.com/keys"),
            ModelProvider::DeepSeek => Some("https://platform.deepseek.com/api_keys"),
            ModelProvider::Meta => Some("https://dev.meta.ai/"),
            ModelProvider::OpenAI => Some("https://platform.openai.com/api-keys"),
            ModelProvider::Ollama => None,
            ModelProvider::Custom => None,
        }
    }

    /// Se il provider richiede una chiave API.
    pub fn needs_key(&self) -> bool {
        !matches!(self, ModelProvider::Ollama)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_complete() {
        for p in ModelProvider::ALL {
            assert!(!p.label().is_empty(), "{p:?}: etichetta vuota");
            if p != ModelProvider::Custom {
                // Custom chiede modello e URL nel setup: vuoti di default.
                assert!(!p.default_model().is_empty(), "{p:?}: modello vuoto");
                assert!(!p.base_url().is_empty(), "{p:?}: url vuoto");
                assert!(p.base_url().starts_with("http"), "{p:?}: url non http");
            }
            if p.needs_key() {
                assert!(p.key_env_var().is_some(), "{p:?}: env var mancante");
                // Custom non ha un url dove creare la chiave: l'utente
                // porta endpoint e chiave propri.
                if p != ModelProvider::Custom {
                    let url = p.key_url().expect("chiave senza url");
                    assert!(url.starts_with("https://"), "{p:?}: url chiave non https");
                }
            }
        }
    }

    #[test]
    fn id_and_number_round_trip() {
        for (i, p) in ModelProvider::ALL.iter().enumerate() {
            assert_eq!(ModelProvider::from_id(p.id()), Some(*p));
            assert_eq!(ModelProvider::from_number(i + 1), Some(*p));
        }
        assert_eq!(ModelProvider::from_id("sconosciuto"), None);
        assert_eq!(ModelProvider::from_number(0), None);
        assert_eq!(ModelProvider::from_number(99), None);
    }
}
