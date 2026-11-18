//! Demo: una conversazione vera con il runtime di Mia, senza finestra.
//!
//! Usa gli stessi motori dell'app (wiki su disco, grafo SQLite, runtime
//! harness) ma gira in terminale: ogni riga è un input reale passato a
//! `MiaRuntime::handle` e la risposta è quella vera del runtime.
//!
//! Il modello qui è `NoModel` (nessuna chiave): preferenze, fatti e
//! documenti funzionano per davvero, mentre alle domande risponde che
//! serve un modello. Con `MIA_MODEL_KEY` impostata, l'app vera usa
//! `OpenAiCompat` e le domande vengono risposte dal modello.
//!
//! Esegui con: `cargo run -p mia-ui --example chat_demo`
//! I dati vanno in /tmp/mia-demo e vengono ricreati a ogni avvio.

use std::fs;
use std::sync::Arc;

use harness_core::Model;
use mia_core::GraphEngine;
use mia_graph::SqliteGraph;
use mia_runtime::{MiaRuntime, RuntimeBudget};
use mia_ui::NoModel;
use mia_wiki::{FileWiki, WikiEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::temp_dir().join("mia-demo");
    let _ = fs::remove_dir_all(&data);
    let wiki_dir = data.join("wiki");
    let graph_db = data.join("graph.db");
    let user_md = data.join("USER.md");

    let wiki = Arc::new(FileWiki::open(&wiki_dir)?);
    let graph = Arc::new(SqliteGraph::open(&graph_db)?);
    let model: Arc<dyn Model> = Arc::new(NoModel);

    let tokio_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let runtime = tokio_rt.block_on(MiaRuntime::new(
        model,
        wiki.clone(),
        wiki.clone(),
        graph.clone(),
        vec![],
        user_md.clone(),
        RuntimeBudget::default(),
    ))?;

    let turni = [
        "preferisco risposte brevi",
        "Mario --lavora_con--> Luigi",
        "# Appunti di matematica\n\nI sistemi lineari di due equazioni in due incognite si risolvono con sostituzione, riduzione, confronto o Cramer.",
        "Chi lavora con Luigi?",
        "Come si risolve un sistema lineare?",
    ];

    for input in turni {
        println!("Tu: {input}");
        let answer = tokio_rt.block_on(runtime.handle(input))?;
        println!("Mia: {}", answer.text);
        println!(
            "    (iterazioni: {}, strumenti usati: {}, esito: {:?})\n",
            answer.iters, answer.tools_called, answer.outcome
        );
    }

    println!("--- Stato salvato ---");
    println!("USER.md:\n{}", fs::read_to_string(&user_md)?);
    let (entities, pages) = tokio_rt.block_on(async {
        (
            graph.entities().await.unwrap_or_default(),
            wiki.list_pages().await.unwrap_or_default(),
        )
    });
    println!("Entità nel grafo: {entities:?}");
    println!("Pagine nella wiki: {pages:?}");
    Ok(())
}
