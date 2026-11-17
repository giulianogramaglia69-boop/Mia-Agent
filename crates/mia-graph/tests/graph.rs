//! Integration tests for the `mia-graph` SQLite engine.
//!
//! Every test opens a fresh in-memory graph, except the persistence
//! test, which uses a temporary file.

use mia_core::{EntityKind, GraphEngine};
use mia_graph::SqliteGraph;

const SAMPLE: &str = "Mario --lavora_con--> Luigi. Mario --progetto--> Alpha. Luigi --progetto--> Alpha.";

#[tokio::test]
async fn remember_stores_and_returns_facts() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    let facts = graph.remember(SAMPLE).await.unwrap();
    assert_eq!(facts.len(), 3);
    assert_eq!(facts[0].subject, "Mario");
    assert_eq!(facts[0].relation, "lavora_con");
    assert_eq!(facts[0].object, "Luigi");
}

#[tokio::test]
async fn remember_without_triples_returns_empty() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    assert!(graph
        .remember("Mario ha collaborato con Luigi al progetto Alpha.")
        .await
        .unwrap()
        .is_empty());
    assert!(graph.entities().await.unwrap().is_empty());
}

#[tokio::test]
async fn remember_twice_does_not_duplicate() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    graph.remember(SAMPLE).await.unwrap();
    assert_eq!(graph.recall("Mario").await.unwrap().len(), 2);
    assert_eq!(graph.entities().await.unwrap().len(), 3);
}

#[tokio::test]
async fn recall_finds_by_entity_and_relation() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();

    let by_entity = graph.recall("mario").await.unwrap();
    assert_eq!(by_entity.len(), 2);
    assert!(by_entity.iter().all(|r| r.source == "Mario"));

    let by_relation = graph.recall("progetto").await.unwrap();
    assert_eq!(by_relation.len(), 2);
    assert!(by_relation.iter().all(|r| r.relation == "progetto"));
    assert_eq!(by_relation[0].weight, 1.0);
}

#[tokio::test]
async fn recall_no_match_returns_empty() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    assert!(graph.recall("Zebra").await.unwrap().is_empty());
}

#[tokio::test]
async fn recall_treats_wildcards_as_literals() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    // "%" matches nothing literally.
    assert!(graph.recall("%").await.unwrap().is_empty());
    // "_" is literal: only "lavora_con" contains it — not every row.
    let underscore = graph.recall("_").await.unwrap();
    assert_eq!(underscore.len(), 1);
    assert_eq!(underscore[0].relation, "lavora_con");
}

#[tokio::test]
async fn trace_finds_path() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    let path = graph.trace("Mario", "Alpha").await.unwrap().unwrap();
    let names: Vec<&str> = path.steps.iter().map(|s| s.entity.as_str()).collect();
    // Mario -> Alpha directly (shortest path), not via Luigi.
    assert_eq!(names, vec!["Mario", "Alpha"]);
    assert_eq!(path.steps[0].relation_to_next.as_deref(), Some("progetto"));
    assert_eq!(path.steps[1].relation_to_next, None);
}

#[tokio::test]
async fn trace_through_intermediate_when_no_direct_edge() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph
        .remember("Mario --lavora_con--> Luigi. Luigi --progetto--> Alpha.")
        .await
        .unwrap();
    let path = graph.trace("Mario", "Alpha").await.unwrap().unwrap();
    let names: Vec<&str> = path.steps.iter().map(|s| s.entity.as_str()).collect();
    assert_eq!(names, vec!["Mario", "Luigi", "Alpha"]);
    assert_eq!(path.steps[0].relation_to_next.as_deref(), Some("lavora_con"));
    assert_eq!(path.steps[1].relation_to_next.as_deref(), Some("progetto"));
}

#[tokio::test]
async fn trace_unconnected_returns_none() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember("Mario --lavora_con--> Luigi.").await.unwrap();
    assert!(graph.trace("Mario", "Alpha").await.unwrap().is_none());
    assert!(graph.trace("Zebra", "Alpha").await.unwrap().is_none());
}

#[tokio::test]
async fn trace_same_entity_is_a_single_step() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    let path = graph.trace("Mario", "Mario").await.unwrap().unwrap();
    assert_eq!(path.steps.len(), 1);
    assert_eq!(path.steps[0].entity, "Mario");
    assert_eq!(path.steps[0].relation_to_next, None);
}

#[tokio::test]
async fn entities_lists_distinct_sorted_with_unknown_kind() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    graph.remember(SAMPLE).await.unwrap();
    let entities = graph.entities().await.unwrap();
    let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Luigi", "Mario"]);
    assert!(entities.iter().all(|e| e.kind == EntityKind::Unknown));
    assert!(entities.iter().all(|e| e.id > 0));
    assert!(entities
        .iter()
        .all(|e| !e.first_seen.is_empty() && !e.last_seen.is_empty()));
}

#[tokio::test]
async fn data_survives_reopen() {
    let path = std::env::temp_dir().join(format!("mia-graph-test-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let graph = SqliteGraph::open(&path).unwrap();
        graph.remember(SAMPLE).await.unwrap();
    }
    {
        let graph = SqliteGraph::open(&path).unwrap();
        assert_eq!(graph.recall("Mario").await.unwrap().len(), 2);
        assert_eq!(graph.entities().await.unwrap().len(), 3);
    }
    let _ = std::fs::remove_file(&path);
}
