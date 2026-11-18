# Mia Agent

Personal everyday agent with a native GUI, written entirely in Rust.
It lives on the user's computer, learns from them, and acts in the
real world (browser, notes, calendar, mail) through isolated Docker
containers exposed via MCP.

> Status: **Phase 2 — Runtime + Wiki** (`mia-wiki` ✓, `mia-runtime` ✓).
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
│   ├── mia-runtime/   # agentic loop, hooks, MCP client (Phase 2 ✓)
│   ├── mia-wiki/       # Markdown ingest, search, storage (Phase 2 ✓)
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

## Phase 2 — `mia-wiki` + `mia-runtime` ✓

### `mia-wiki`

Narrative knowledge: Markdown pages + a persistent tantivy index
(`data/wiki/raw/` immutable sources, `pages/` compiled pages,
`index/` tantivy). Implements `WikiEngine` (`ingest`/`search`/
`read`/`list_pages`) and `mia-core::KnowledgeEngine`.

- Title from frontmatter, else first H1, else `"Untitled"`; slugs
  are unique (`alpha`, `alpha-2`, …). Frontmatter YAML (`title`,
  `tags`) via `serde_yaml` — added to the workspace because the
  spec requires frontmatter metadata.
- Search is tantivy full-text over title + body (top 10); the body
  is indexed but not stored — the page file is the source of truth.
  A query that fails to parse falls back to a literal phrase query.

### `mia-runtime`

The agentic loop, built on `harness-rs` 0.0.72
(`AgentLoop` + `harness_core::Model`/`Tool`/`Hook`, `with_profile`
for the `World`).

- **Note on the dependency**: the spec names the git URL, but the
  repo root is a virtual workspace and cargo cannot select a
  sub-crate from a git dependency, so `harness-rs` comes from
  crates.io (0.0.72, same code, published). The facade's lib name
  is `harness`; the loop lives in `harness-rs-loop`, the traits in
  `harness-rs-core`, the world builder in `harness-rs-context`.
- **No MCP client yet**: `turul-mcp-client`/`neva` are Phase 4
  (containers). `harness` ships its own `harness-mcp-client` —
  the likely candidate then.
- **No model client yet**: the model is injected as
  `Arc<dyn harness_core::Model>`. Real-model wiring
  (`harness-models::OpenAiCompat`) comes with the CLI, once a
  provider is chosen.
- `MiaRuntime::handle` runs the anti-overlap flow: `classify`
  (preference → `USER.md` max 20 lines, fact → graph, document →
  wiki, question → consult wiki + graph, inject the context, run
  the loop). The model reaches the engines through seven tools
  (`wiki_search`/`wiki_read`/`wiki_ingest`, `graph_remember`/
  `graph_recall`/`graph_trace`/`graph_entities`) plus every
  `ToolProvider` tool (registered `Idempotent` — providers declare
  no risk; MCP servers will in Phase 4).
- Budgets: `max_iters` (default 10) + per-call `tool_timeout`
  (default 30s); the stuck detector stops spinning runs.
  `run_subagent` runs an isolated loop on a sub-task.
  `RecordingHook` records every loop event for observability.
- Classification is rule-based heuristics in v0.1 (explicit
  triple notation, Italian preference markers, Markdown
  heading/length); model-based classification comes with a real
  model.

## Verify

```sh
cargo test -p mia-core
cargo test -p mia-graph
cargo test -p mia-wiki
cargo test -p mia-runtime
cargo clippy -p mia-wiki -p mia-runtime --all-targets -- -D warnings
```
