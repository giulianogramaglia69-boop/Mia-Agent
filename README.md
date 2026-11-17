# Mia Agent

Personal everyday agent with a native GUI, written entirely in Rust.
It lives on the user's computer, learns from them, and acts in the
real world (browser, notes, calendar, mail) through isolated Docker
containers exposed via MCP.

> Status: **Phase 1 — Foundations** (`mia-core` ✓, `mia-graph` ✓).
> The roadmap is in the project prompt: 5 phases over 10 weeks.

## Rules

- Rust edition 2024, MSRV 1.88+. No Python, no Node.js in runtime or UI.
- Native GUI with `sequoia-ui` (wgpu). No Tauri, no Electron, no WebView.
- Strict separation: Wiki (narrative) ≠ Graph (relational) ≠ USER.md
  (preferences). Never duplicate data across levels.
- Every real-world capability runs in its own Docker container,
  exposed via MCP.
- License: Apache-2.0.

## Workspace

```
mia-agent/
├── Cargo.toml
├── crates/
│   ├── mia-core/      # shared traits, types, errors (Phase 1 ✓)
│   ├── mia-runtime/   # agentic loop, hooks, MCP client (Phase 2)
│   ├── mia-wiki/       # Markdown ingest, search, storage (Phase 2)
│   ├── mia-graph/      # SQLite entities/relations (Phase 1 ✓)
│   ├── mia-ui/         # Sequoia app, panels, widgets (Phase 3)
│   └── mia-cli/        # run, ingest, export (Phase 2)
├── containers/          # browser, notes, calendar, mail (Phase 4)
├── docker-compose.yml
└── README.md
```

## Phase 1 — `mia-core` ✓

Shared contracts every other crate implements:

- `KnowledgeEngine`: `ingest(source) -> KnowledgeId`, `search(query)`
- `GraphEngine`: `remember(text)`, `recall(query)`, `trace(from, to)`,
  `entities()`
- `ToolProvider`: `list_tools()`, `call(tool, args)`
- Types: `KnowledgeId`, `KnowledgeEntry`, `GraphFact`, `Relation`,
  `Entity`, `EntityKind`, `Path`, `PathStep`, `ToolSpec`
- Errors: `MiaError` (`thiserror`), `Result<T>` alias

Note: `GraphEngine` in `mia-core` has the four-method form from
Strato 2b (with `entities()`), which is a superset of the three-method
form in Strato 1.

## Phase 1 — `mia-graph` ✓

SQLite-backed `GraphEngine` (`rusqlite` 0.32, bundled SQLite):

- Schema from the spec: `entities` (id, name UNIQUE, kind,
  first_seen, last_seen) + `relations` (source_id, target_id,
  relation, weight, created_at, UNIQUE triple) + source/target indexes.
- `remember` / `recall` (case-insensitive, wildcards escaped) /
  `trace` (BFS, shortest path) / `entities` (sorted by name).
- `SqliteGraph::open(path)` for files, `open_in_memory()` for tests.
- `chrono` added to the workspace (motivated: the schema requires
  RFC 3339 timestamps for `first_seen`/`last_seen`/`created_at`).
- Extractor note (v0.1): only explicit triples written as
  `Subject --relation--> Object` are stored; free narrative yields
  no facts. LLM-based extraction from prose arrives with the runtime
  in Phase 2, where the model emits triples in this notation.
  Entities created by extraction get kind `Unknown`.

## Verify

```sh
cargo test -p mia-core
cargo test -p mia-graph
cargo clippy -p mia-graph --all-targets -- -D warnings
```
