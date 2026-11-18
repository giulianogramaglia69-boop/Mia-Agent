//! `mia` — the native Mia Agent app (Phase 3 binary).
//!
//! Usage:
//! `mia [--data-dir <path>] [--wiki-dir <path>] [--graph-db <path>] [--user-md <path>]`
//!
//! Data defaults to `./mia-data` (wiki in `mia-data/wiki`, graph in
//! `mia-data/graph.db`, preferences in `mia-data/USER.md`).
//!
//! Model: when `MIA_MODEL_KEY` is set, the app builds an OpenAI-compatible
//! model from `MIA_MODEL_BASE_URL` (default `https://api.meta.ai/v1`) and
//! `MIA_MODEL_ID` (default `muse-spark-1.1`). Without a key it runs with
//! `NoModel`: preferences, facts and documents still work, and questions
//! get an honest "no model configured" answer.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use harness_core::Model;
use harness_models::{LlmConfig, OpenAiCompat};
use mia_graph::SqliteGraph;
use mia_runtime::{MiaRuntime, RuntimeBudget};
use mia_ui::{MiaApp, NoModel};
use mia_wiki::FileWiki;

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Options::parse(env::args().skip(1));
    let data_dir = opts
        .data_dir
        .unwrap_or_else(|| PathBuf::from("mia-data"));
    let wiki_dir = opts.wiki_dir.unwrap_or_else(|| data_dir.join("wiki"));
    let graph_db = opts.graph_db.unwrap_or_else(|| data_dir.join("graph.db"));
    let user_md = opts.user_md.unwrap_or_else(|| data_dir.join("USER.md"));

    let model: Arc<dyn Model> = match env::var("MIA_MODEL_KEY") {
        Ok(key) if !key.trim().is_empty() => {
            let base = env::var("MIA_MODEL_BASE_URL")
                .unwrap_or_else(|_| "https://api.meta.ai/v1".to_string());
            let model_id =
                env::var("MIA_MODEL_ID").unwrap_or_else(|_| "muse-spark-1.1".to_string());
            Arc::new(OpenAiCompat::new(LlmConfig::new(
                "mia-model", base, key, model_id,
            )))
        }
        _ => {
            eprintln!(
                "Mia: nessun modello configurato (imposta MIA_MODEL_KEY per le domande al modello)."
            );
            Arc::new(NoModel)
        }
    };

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
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let slot = match arg.as_str() {
                "--data-dir" => &mut opts.data_dir,
                "--wiki-dir" => &mut opts.wiki_dir,
                "--graph-db" => &mut opts.graph_db,
                "--user-md" => &mut opts.user_md,
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
