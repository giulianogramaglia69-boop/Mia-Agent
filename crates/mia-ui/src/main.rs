//! `mia` — the native Mia Agent app (Phase 3 binary).
//!
//! Usage:
//! `mia [--data-dir <path>] [--wiki-dir <path>] [--graph-db <path>] [--user-md <path>]`
//! `mia --setup [--data-dir <path>]`: scegli il provider del modello
//! (Gemini, Groq, DeepSeek, Muse Spark, OpenAI, Ollama locale o un
//! endpoint OpenAI-compatibile) e salva la chiave in `model.yaml`.
//! `mia --dry-run`: mostra quale modello verrebbe usato, senza aprire la GUI.
//!
//! Data defaults to `./mia-data` (wiki in `mia-data/wiki`, graph in
//! `mia-data/graph.db`, preferences in `mia-data/USER.md`, model config
//! in `mia-data/model.yaml`).
//!
//! Model: la precedenza è `MIA_MODEL_KEY` (con `MIA_MODEL_BASE_URL` e
//! `MIA_MODEL_ID`, default Meta `https://api.meta.ai/v1` / `muse-spark-1.1`),
//! poi `model.yaml` scritto dal setup, poi `NoModel`: preferenze, fatti e
//! documenti funzionano comunque, e alle domande risponde che serve un
//! modello.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use harness_core::Model;
use harness_models::{LlmConfig, OpenAiCompat};
use mia_graph::SqliteGraph;
use mia_runtime::{MiaRuntime, RuntimeBudget};
use mia_ui::{MiaApp, ModelConfig, NoModel, run_setup};
use mia_wiki::FileWiki;

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Options::parse(env::args().skip(1));
    let data_dir = opts
        .data_dir
        .unwrap_or_else(|| PathBuf::from("mia-data"));
    let wiki_dir = opts.wiki_dir.unwrap_or_else(|| data_dir.join("wiki"));
    let graph_db = opts.graph_db.unwrap_or_else(|| data_dir.join("graph.db"));
    let user_md = opts.user_md.unwrap_or_else(|| data_dir.join("USER.md"));

    if opts.setup {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        run_setup(&data_dir, &mut input, &mut output)?;
        return Ok(());
    }

    let (model, model_desc): (Arc<dyn Model>, String) =
        match env::var("MIA_MODEL_KEY") {
            Ok(key) if !key.trim().is_empty() => {
                let base = env::var("MIA_MODEL_BASE_URL")
                    .unwrap_or_else(|_| "https://api.meta.ai/v1".to_string());
                let model_id = env::var("MIA_MODEL_ID")
                    .unwrap_or_else(|_| "muse-spark-1.1".to_string());
                let desc = format!("Muse Spark (Meta) — {model_id} via env MIA_MODEL_KEY");
                (
                    Arc::new(OpenAiCompat::new(LlmConfig::new(
                        "mia-model", base, key, model_id,
                    ))),
                    desc,
                )
            }
            _ => match ModelConfig::load(&data_dir) {
                Some(config) => {
                    let desc = format!(
                        "{} — {} (da {})",
                        config.provider().label(),
                        config.model,
                        ModelConfig::path(&data_dir).display()
                    );
                    (Arc::new(OpenAiCompat::new(config.to_llm_config())), desc)
                }
                None => {
                    eprintln!(
                        "Mia: nessun modello configurato (mia --setup per sceglierne uno, o imposta MIA_MODEL_KEY)."
                    );
                    (Arc::new(NoModel), "nessun modello (NoModel)".to_string())
                }
            },
        };

    if opts.dry_run {
        println!("Modello: {model_desc}");
        return Ok(());
    }

    let wiki = Arc::new(FileWiki::open(&wiki_dir)?);
    let graph = Arc::new(SqliteGraph::open(&graph_db)?);

    // The harness loop sleeps between turns, so runtime calls need a
    // Tokio runtime. It lives for the whole app lifetime.
    let tokio_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = tokio_rt.handle().clone();
    let runtime = tokio_rt.block_on(MiaRuntime::new(
        model,
        wiki.clone(),
        wiki.clone(),
        graph.clone(),
        vec![],
        user_md,
        RuntimeBudget::default(),
    ))?;

    MiaApp::new(Arc::new(runtime), wiki, graph, handle).run()?;
    Ok(())
}

/// The `--flag <value>` options of the binary.
struct Options {
    data_dir: Option<PathBuf>,
    wiki_dir: Option<PathBuf>,
    graph_db: Option<PathBuf>,
    user_md: Option<PathBuf>,
    setup: bool,
    dry_run: bool,
}

impl Options {
    /// Parse the arguments after the program name. Unknown flags are
    /// reported on stderr and ignored.
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut opts = Self {
            data_dir: None,
            wiki_dir: None,
            graph_db: None,
            user_md: None,
            setup: false,
            dry_run: false,
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--setup" => opts.setup = true,
                "--dry-run" => opts.dry_run = true,
                _ => {}
            }
            let slot = match arg.as_str() {
                "--data-dir" => &mut opts.data_dir,
                "--wiki-dir" => &mut opts.wiki_dir,
                "--graph-db" => &mut opts.graph_db,
                "--user-md" => &mut opts.user_md,
                "--setup" | "--dry-run" => continue,
                other => {
                    eprintln!("Mia: argomento sconosciuto ignorato: {other}");
                    continue;
                }
            };
            match args.next() {
                Some(value) => *slot = Some(PathBuf::from(value)),
                None => eprintln!("Mia: {arg} richiede un valore, ignorato."),
            }
        }
        opts
    }
}
