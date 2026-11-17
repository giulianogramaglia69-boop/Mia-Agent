//! SQLite-backed implementation of [`GraphEngine`](mia_core::GraphEngine).
//!
//! Schema (from the project spec):
//!
//! ```sql
//! CREATE TABLE entities (
//!     id INTEGER PRIMARY KEY,
//!     name TEXT NOT NULL UNIQUE,
//!     kind TEXT NOT NULL,
//!     first_seen TEXT NOT NULL,
//!     last_seen TEXT NOT NULL
//! );
//! CREATE TABLE relations (
//!     id INTEGER PRIMARY KEY,
//!     source_id INTEGER NOT NULL REFERENCES entities(id),
//!     target_id INTEGER NOT NULL REFERENCES entities(id),
//!     relation TEXT NOT NULL,
//!     weight REAL DEFAULT 1.0,
//!     created_at TEXT NOT NULL,
//!     UNIQUE(source_id, target_id, relation)
//! );
//! ```
//!
//! v0.1 notes:
//! - `remember` stores only triples from [`extract_triples`](crate::extract::extract_triples);
//!   entities created this way get kind `Unknown` — kinds are assigned
//!   by the runtime once it classifies them (Phase 2).
//! - The SQLite calls run inline in the async methods (local file,
//!   milliseconds). A dedicated blocking thread arrives if the DB grows.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path as FsPath;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use mia_core::{
    Entity, EntityKind, GraphEngine, GraphFact, MiaError, Path, PathStep, Relation, Result,
};
use rusqlite::{params, Connection};

use crate::extract::extract_triples;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entities (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS relations (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES entities(id),
    target_id INTEGER NOT NULL REFERENCES entities(id),
    relation TEXT NOT NULL,
    weight REAL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    UNIQUE(source_id, target_id, relation)
);
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
";

/// A [`GraphEngine`] backed by a SQLite database file (or memory).
pub struct SqliteGraph {
    conn: Mutex<Connection>,
}

impl SqliteGraph {
    /// Open (or create) the graph database at `path`, creating the
    /// schema when the file is new or empty.
    pub fn open(path: impl AsRef<FsPath>) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(storage_err)?;
        let graph = Self {
            conn: Mutex::new(conn),
        };
        graph.init_schema()?;
        Ok(graph)
    }

    /// Open an in-memory graph. Used by tests and ephemeral runs;
    /// nothing is persisted.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(storage_err)?;
        let graph = Self {
            conn: Mutex::new(conn),
        };
        graph.init_schema()?;
        Ok(graph)
    }

    fn init_schema(&self) -> Result<()> {
        self.lock()?
            .execute_batch(SCHEMA)
            .map_err(storage_err)?;
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| MiaError::Internal("graph lock poisoned".into()))
    }
}

fn storage_err(err: rusqlite::Error) -> MiaError {
    MiaError::Storage(err.to_string())
}

/// Insert `name` if absent (kind `Unknown`, timestamps `now`), refresh
/// its `last_seen`, and return its id.
fn upsert_entity(conn: &Connection, name: &str, now: &str) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO entities (name, kind, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(name) DO UPDATE SET last_seen = excluded.last_seen",
        params![name, kind_to_str(&EntityKind::Unknown), now],
    )?;
    conn.query_row(
        "SELECT id FROM entities WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
}

fn kind_to_str(kind: &EntityKind) -> &'static str {
    match kind {
        EntityKind::Person => "person",
        EntityKind::Project => "project",
        EntityKind::Org => "org",
        EntityKind::Concept => "concept",
        EntityKind::Unknown => "unknown",
    }
}

fn kind_from_str(s: &str) -> EntityKind {
    match s {
        "person" => EntityKind::Person,
        "project" => EntityKind::Project,
        "org" => EntityKind::Org,
        "concept" => EntityKind::Concept,
        _ => EntityKind::Unknown,
    }
}

/// Escape a user query for a `LIKE` pattern (`%`, `_`, `\` are literals).
fn like_pattern(query: &str) -> String {
    let escaped: String = query
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect();
    format!("%{}%", escaped.to_lowercase())
}

#[async_trait]
impl GraphEngine for SqliteGraph {
    async fn remember(&self, text: &str) -> Result<Vec<GraphFact>> {
        let facts = extract_triples(text);
        if facts.is_empty() {
            return Ok(vec![]);
        }
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        for fact in &facts {
            let source_id = upsert_entity(&conn, &fact.subject, &now).map_err(storage_err)?;
            let target_id = upsert_entity(&conn, &fact.object, &now).map_err(storage_err)?;
            conn.execute(
                "INSERT INTO relations (source_id, target_id, relation, weight, created_at)
                 VALUES (?1, ?2, ?3, 1.0, ?4)
                 ON CONFLICT(source_id, target_id, relation) DO NOTHING",
                params![source_id, target_id, fact.relation, now],
            )
            .map_err(storage_err)?;
        }
        Ok(facts)
    }

    async fn recall(&self, query: &str) -> Result<Vec<Relation>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT s.name, t.name, r.relation, r.weight
                 FROM relations r
                 JOIN entities s ON s.id = r.source_id
                 JOIN entities t ON t.id = r.target_id
                 WHERE lower(s.name) LIKE ?1 ESCAPE '\\'
                    OR lower(t.name) LIKE ?1 ESCAPE '\\'
                    OR lower(r.relation) LIKE ?1 ESCAPE '\\'
                 ORDER BY s.name, r.relation, t.name",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![like_pattern(query)], |row| {
                Ok(Relation {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    relation: row.get(2)?,
                    weight: row.get(3)?,
                })
            })
            .map_err(storage_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(storage_err)?);
        }
        Ok(out)
    }

    async fn trace(&self, from: &str, to: &str) -> Result<Option<Path>> {
        // Load the adjacency list once, then BFS in memory.
        let mut adjacency: HashMap<String, Vec<(String, String)>> = HashMap::new();
        {
            let conn = self.lock()?;
            let mut stmt = conn
                .prepare(
                    "SELECT s.name, r.relation, t.name
                     FROM relations r
                     JOIN entities s ON s.id = r.source_id
                     JOIN entities t ON t.id = r.target_id",
                )
                .map_err(storage_err)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(storage_err)?;
            for row in rows {
                let (source, relation, target) = row.map_err(storage_err)?;
                adjacency
                    .entry(source)
                    .or_default()
                    .push((relation, target));
            }
        }

        if from == to {
            return Ok(Some(Path {
                steps: vec![PathStep {
                    entity: from.to_string(),
                    relation_to_next: None,
                }],
            }));
        }

        let mut queue: VecDeque<Vec<PathStep>> = VecDeque::from([vec![PathStep {
            entity: from.to_string(),
            relation_to_next: None,
        }]]);
        let mut seen: HashSet<String> = HashSet::from([from.to_string()]);
        while let Some(path) = queue.pop_front() {
            let current = path.last().expect("path is never empty").entity.clone();
            if let Some(edges) = adjacency.get(&current) {
                for (relation, target) in edges {
                    if seen.contains(target) {
                        continue;
                    }
                    seen.insert(target.clone());
                    let mut next = path.clone();
                    if let Some(prev) = next.last_mut() {
                        prev.relation_to_next = Some(relation.clone());
                    }
                    next.push(PathStep {
                        entity: target.clone(),
                        relation_to_next: None,
                    });
                    if target == to {
                        return Ok(Some(Path { steps: next }));
                    }
                    queue.push_back(next);
                }
            }
        }
        Ok(None)
    }

    async fn entities(&self) -> Result<Vec<Entity>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, kind, first_seen, last_seen
                 FROM entities ORDER BY name",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: kind_from_str(&row.get::<_, String>(2)?),
                    first_seen: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            })
            .map_err(storage_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(storage_err)?);
        }
        Ok(out)
    }
}
