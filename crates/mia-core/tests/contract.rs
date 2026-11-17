//! Contract tests for the `mia-core` traits.
//!
//! These tests define the behaviour every engine must have, using
//! minimal in-memory fakes. A real engine (SQLite, tantivy, MCP)
//! must pass the same contract: ingest/search round-trip, triple
//! extraction, path tracing, tool dispatch and typed errors.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use mia_core::{
    Entity, EntityKind, GraphEngine, GraphFact, KnowledgeEngine, KnowledgeEntry, KnowledgeId,
    MiaError, Path, PathStep, Relation, Result, ToolProvider, ToolSpec,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// In-memory wiki: one page per ingest, substring search.
struct FakeWiki {
    pages: Mutex<HashMap<String, KnowledgeEntry>>,
    next_id: Mutex<u64>,
}

impl FakeWiki {
    fn new() -> Self {
        Self {
            pages: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }
}

#[async_trait]
impl KnowledgeEngine for FakeWiki {
    async fn ingest(&self, source: &str) -> Result<KnowledgeId> {
        if source.trim().is_empty() {
            return Err(MiaError::InvalidInput("source must not be empty".into()));
        }
        let mut next = self.next_id.lock().unwrap();
        let id = KnowledgeId::new(format!("page-{next}"));
        *next += 1;
        let title = source.lines().next().unwrap_or("Untitled").to_string();
        self.pages.lock().unwrap().insert(
            id.0.clone(),
            KnowledgeEntry {
                id: id.clone(),
                title,
                content: source.to_string(),
                score: 1.0,
            },
        );
        Ok(id)
    }

    async fn search(&self, query: &str) -> Result<Vec<KnowledgeEntry>> {
        let needle = query.to_lowercase();
        let mut hits: Vec<KnowledgeEntry> = self
            .pages
            .lock()
            .unwrap()
            .values()
            .filter(|p| {
                p.title.to_lowercase().contains(&needle)
                    || p.content.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(hits)
    }
}

/// In-memory graph: triples written as `A --rel--> B`.
struct FakeGraph {
    facts: Mutex<Vec<GraphFact>>,
}

impl FakeGraph {
    fn new() -> Self {
        Self {
            facts: Mutex::new(Vec::new()),
        }
    }

    /// Extract `subject --relation--> object` triples from text.
    fn extract(text: &str) -> Vec<GraphFact> {
        let mut facts = Vec::new();
        let mut rest = text;
        while let Some(arrow) = rest.find("-->") {
            let before = &rest[..arrow];
            let after = &rest[arrow + 3..];
            if let Some(dash) = before.rfind("--") {
                let rel = before[dash + 2..].trim();
                let subject = before[..dash]
                    .split_whitespace()
                    .next_back()
                    .unwrap_or("")
                    .trim_matches(|c: char| !c.is_alphanumeric());
                let object = after
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c: char| !c.is_alphanumeric());
                if !subject.is_empty() && !rel.is_empty() && !object.is_empty() {
                    facts.push(GraphFact {
                        subject: subject.to_string(),
                        relation: rel.to_string(),
                        object: object.to_string(),
                    });
                }
            }
            rest = after;
        }
        facts
    }
}

#[async_trait]
impl GraphEngine for FakeGraph {
    async fn remember(&self, text: &str) -> Result<Vec<GraphFact>> {
        let facts = Self::extract(text);
        let mut stored = self.facts.lock().unwrap();
        for fact in &facts {
            if !stored.contains(fact) {
                stored.push(fact.clone());
            }
        }
        Ok(facts)
    }

    async fn recall(&self, query: &str) -> Result<Vec<Relation>> {
        let needle = query.to_lowercase();
        let out = self
            .facts
            .lock()
            .unwrap()
            .iter()
            .filter(|f| {
                f.subject.to_lowercase().contains(&needle)
                    || f.relation.to_lowercase().contains(&needle)
                    || f.object.to_lowercase().contains(&needle)
            })
            .map(|f| Relation {
                source: f.subject.clone(),
                target: f.object.clone(),
                relation: f.relation.clone(),
                weight: 1.0,
            })
            .collect();
        Ok(out)
    }

    async fn trace(&self, from: &str, to: &str) -> Result<Option<Path>> {
        // Breadth-first search over stored triples.
        use std::collections::VecDeque;
        let facts = self.facts.lock().unwrap().clone();
        let mut queue: VecDeque<Vec<PathStep>> = VecDeque::from([vec![PathStep {
            entity: from.to_string(),
            relation_to_next: None,
        }]]);
        let mut seen: HashSet<String> = HashSet::from([from.to_string()]);
        while let Some(path) = queue.pop_front() {
            let current = &path.last().unwrap().entity;
            if current == to {
                let mut steps = path;
                for i in 0..steps.len().saturating_sub(1) {
                    let rel = facts
                        .iter()
                        .find(|f| {
                            f.subject == steps[i].entity && f.object == steps[i + 1].entity
                        })
                        .map(|f| f.relation.clone());
                    steps[i].relation_to_next = rel;
                }
                return Ok(Some(Path { steps }));
            }
            for fact in &facts {
                if fact.subject == *current && !seen.contains(&fact.object) {
                    seen.insert(fact.object.clone());
                    let mut next = path.clone();
                    next.push(PathStep {
                        entity: fact.object.clone(),
                        relation_to_next: None,
                    });
                    queue.push_back(next);
                }
            }
        }
        Ok(None)
    }

    async fn entities(&self) -> Result<Vec<Entity>> {
        let mut names: Vec<String> = self
            .facts
            .lock()
            .unwrap()
            .iter()
            .flat_map(|f| [f.subject.clone(), f.object.clone()])
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        names.sort();
        Ok(names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Entity {
                id: i as i64 + 1,
                name,
                kind: EntityKind::Concept,
                first_seen: "2026-11-17T00:00:00Z".into(),
                last_seen: "2026-11-17T00:00:00Z".into(),
            })
            .collect())
    }
}

/// In-memory tools: `echo` returns its args, `add` sums `a` + `b`.
struct FakeTools;

#[async_trait]
impl ToolProvider for FakeTools {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        Ok(vec![
            ToolSpec {
                name: "echo".into(),
                description: "Return the given arguments unchanged.".into(),
                input_schema: json!({"type": "object"}),
            },
            ToolSpec {
                name: "add".into(),
                description: "Add two integers a and b.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "a": {"type": "integer"},
                        "b": {"type": "integer"}
                    },
                    "required": ["a", "b"]
                }),
            },
        ])
    }

    async fn call(&self, tool: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        match tool {
            "echo" => Ok(args),
            "add" => {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| MiaError::InvalidInput("add: missing integer a".into()))?;
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| MiaError::InvalidInput("add: missing integer b".into()))?;
                Ok(json!({"sum": a + b}))
            }
            other => Err(MiaError::ToolNotFound(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// KnowledgeEngine contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn knowledge_ingest_then_search_finds_it() {
    let wiki = FakeWiki::new();
    let id = wiki
        .ingest("# Alpha\nMario ha collaborato con Luigi al progetto Alpha.")
        .await
        .unwrap();
    let hits = wiki.search("Alpha").await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);
    assert_eq!(hits[0].title, "# Alpha");
}

#[tokio::test]
async fn knowledge_search_no_match_returns_empty() {
    let wiki = FakeWiki::new();
    wiki.ingest("Contenuto sul progetto Alpha.").await.unwrap();
    assert!(wiki.search("Zebra").await.unwrap().is_empty());
}

#[tokio::test]
async fn knowledge_ingest_empty_source_is_invalid_input() {
    let wiki = FakeWiki::new();
    let err = wiki.ingest("   ").await.unwrap_err();
    assert!(matches!(err, MiaError::InvalidInput(_)));
}

// ---------------------------------------------------------------------------
// GraphEngine contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graph_remember_extracts_triples() {
    let graph = FakeGraph::new();
    let facts = graph
        .remember("Mario --lavora_con--> Luigi. Mario --progetto--> Alpha.")
        .await
        .unwrap();
    assert_eq!(facts.len(), 2);
    assert!(facts.contains(&GraphFact {
        subject: "Mario".into(),
        relation: "lavora_con".into(),
        object: "Luigi".into(),
    }));
}

#[tokio::test]
async fn graph_remember_without_triples_returns_empty() {
    let graph = FakeGraph::new();
    assert!(graph.remember("Solo testo narrativo.").await.unwrap().is_empty());
}

#[tokio::test]
async fn graph_recall_finds_relation_by_name() {
    let graph = FakeGraph::new();
    graph.remember("Mario --lavora_con--> Luigi.").await.unwrap();
    let rels = graph.recall("Mario").await.unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].source, "Mario");
    assert_eq!(rels[0].target, "Luigi");
    assert_eq!(rels[0].relation, "lavora_con");
}

#[tokio::test]
async fn graph_trace_finds_path() {
    let graph = FakeGraph::new();
    graph
        .remember("Mario --lavora_con--> Luigi. Luigi --progetto--> Alpha.")
        .await
        .unwrap();
    let path = graph.trace("Mario", "Alpha").await.unwrap().unwrap();
    let names: Vec<&str> = path.steps.iter().map(|s| s.entity.as_str()).collect();
    assert_eq!(names, vec!["Mario", "Luigi", "Alpha"]);
    assert_eq!(path.steps[0].relation_to_next.as_deref(), Some("lavora_con"));
    assert_eq!(path.steps[1].relation_to_next.as_deref(), Some("progetto"));
    assert_eq!(path.steps[2].relation_to_next, None);
}

#[tokio::test]
async fn graph_trace_unconnected_returns_none() {
    let graph = FakeGraph::new();
    graph.remember("Mario --lavora_con--> Luigi.").await.unwrap();
    assert!(graph.trace("Mario", "Alpha").await.unwrap().is_none());
}

#[tokio::test]
async fn graph_entities_lists_distinct_entities() {
    let graph = FakeGraph::new();
    graph
        .remember("Mario --lavora_con--> Luigi. Mario --progetto--> Alpha.")
        .await
        .unwrap();
    let names: Vec<String> = graph
        .entities()
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["Alpha", "Luigi", "Mario"]);
}

// ---------------------------------------------------------------------------
// ToolProvider contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tools_list_describes_tools() {
    let tools = FakeTools;
    let specs = tools.list_tools().await.unwrap();
    assert_eq!(specs.len(), 2);
    assert!(specs.iter().any(|s| s.name == "echo"));
    assert!(specs.iter().any(|s| s.name == "add"));
}

#[tokio::test]
async fn tools_call_add_returns_sum() {
    let tools = FakeTools;
    let out = tools.call("add", json!({"a": 40, "b": 2})).await.unwrap();
    assert_eq!(out, json!({"sum": 42}));
}

#[tokio::test]
async fn tools_call_unknown_is_tool_not_found() {
    let tools = FakeTools;
    let err = tools.call("nope", json!({})).await.unwrap_err();
    assert!(matches!(err, MiaError::ToolNotFound(_)));
}

#[tokio::test]
async fn tools_call_add_missing_arg_is_invalid_input() {
    let tools = FakeTools;
    let err = tools.call("add", json!({"a": 1})).await.unwrap_err();
    assert!(matches!(err, MiaError::InvalidInput(_)));
}
