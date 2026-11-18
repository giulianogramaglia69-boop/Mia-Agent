# Mia Agent

Personal everyday agent with a native GUI, written entirely in Rust.
It lives on the user's computer, learns from them, and acts in the
real world (browser, notes, calendar, mail) through isolated Docker
containers exposed via MCP.

> Status: **Phase 5 — Polish** (`mia-mcp` ✓, containers ✓, WGSL graph glow ✓)
> + **setup in-app** ✓ (primo avvio guidato dentro la GUI) e **installazione
> user-friendly** ✓ (installer `.rpm` / `.AppImage` / `.exe`, `packaging/install.sh`,
> release precompilate su tag).
> The roadmap is in the project prompt: 5 phases over 10 weeks.

## Installazione (senza terminale, dopo la prima volta)

1. Scarica l'installer della tua piattaforma dalla
   [pagina Release](https://github.com/giulianogramaglia69-boop/Mia-Agent/releases):
   **`.rpm`** (Fedora/RHEL: `sudo dnf install mia-agent-*.rpm`),
   **`.AppImage`** (Linux: rendilo eseguibile e aprilo),
   **`.exe`** (Windows: installer con scorciatoia nel menu Start).
   In alternativa, i binari semplici (`mia-linux-x86_64`,
   `mia-windows-x86_64.exe`, `mia-macos-aarch64`) oppure compila e
   installa dal repo: `./packaging/install.sh` (mette `mia` in
   `~/.local/bin` e "Mia Agent" nel menu applicazioni).
2. Avvia Mia: al primo avvio la schermata di setup ti fa scegliere il
   provider (Gemini, Groq, DeepSeek, Muse Spark, OpenAI, Ollama locale o
   un endpoint tuo), incollare la chiave e partire — niente terminale.
   La chiave resta solo in `~/mia-data/model.yaml`, sul tuo computer.
3. "Salta per ora" apre Mia senza modello: preferenze, fatti e pagine
   funzionano comunque; per rivedere il setup cancella
   `~/mia-data/.setup-skipped` (o dai `mia --setup` da terminale).

Gli strumenti (note, browser, calendario, mail) si attivano da
`mia-data/mcp.yaml` — l'esempio pronto è in `containers/README.md`.
Calendario e mail chiedono una sola autorizzazione Google
(`mia-calendar auth` / `mia-mail auth`, codice su google.com/device).

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
│   ├── mia-ui/         # Sequoia app, panels, widgets (Phase 3 ✓)
│   ├── mia-mcp/        # MCP client, provider, write gate (Phase 4 ✓)
│   ├── mia-google/     # Google OAuth device flow (Phase 4 ✓)
│   └── mia-cli/        # run, ingest, export (Phase 2)
├── containers/          # browser, notes, calendar, mail (Phase 4 ✓)
├── docker-compose.yml
├── packaging/           # install.sh, mia.desktop, mia.svg, build-appimage.sh, windows/mia.nsi
├── .github/workflows/   # containers.yml (CI) + release.yml (binari su tag)
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
- **MCP client**: `harness-rs-mcp-client` 0.0.72 (the harness family's
  own client; the spec's `turul-mcp-client`/`neva` were not needed).
  `mia-mcp` (Phase 4) wraps it as a `ToolProvider` — see below.
- **No model client yet**: the model is injected as
  `Arc<dyn harness_core::Model>`. The `mia` GUI binary wires one
  when `MIA_MODEL_KEY` is set (`harness-models::OpenAiCompat` over
  `LlmConfig`; base URL and model id from `MIA_MODEL_BASE_URL` /
  `MIA_MODEL_ID`), and falls back to `mia-ui::NoModel` otherwise.
  The CLI wiring is still open (`mia-cli` is not built yet).
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

## Phase 3 — `mia-ui` ✓

The native GUI, built on `sequoia-ui` 0.1.5 (wgpu, retained view
tree, fine-grained `Signal`s — no WebView, no Tauri, per the spec).
Run it with `cargo run -p mia-ui` (binary `mia`).

- **Layout**: nested `SplitPanel`s — wiki tree | chat | graph canvas
  (outer 0.22, inner 0.55, both dividers draggable). Chat: a reactive
  `List` of messages (role-coloured labels) + `TextField` + Invia.
  Wiki: a `TreeView` of `list_pages` under one "Wiki" root; clicking
  a page reads it into the chat. Graph: a `Canvas` that draws the
  entities on a circle and the relations as labelled edges
  (2D `Painter` drawing in v0.1; the canvas also supports WGSL
  shader passes for later polish).- **Threading**: button callbacks are synchronous, but
  `MiaRuntime::handle` is async and the harness loop needs a Tokio
  runtime (it sleeps between turns). Submit goes through
  `AsyncRuntime::spawn_future`, whose future blocks on a Tokio
  `Handle`; the answer is applied on the UI thread, then the wiki
  tree and the graph reload. A `pending()` signal shows
  "Mia sta pensando…" while a reply is in flight.
- **Model**: `mia-ui::NoModel` answers every completion with an
  honest "no model configured" message, so the app runs and stays
  useful without a provider key — preferences, facts and documents
  never touch the model. To use a real model, run `mia --setup` and
  pick a provider — Gemini, Groq, DeepSeek, Muse Spark (Meta),
  OpenAI, Ollama (local, no key) or a custom OpenAI-compatible
  endpoint — then the model and key; the choice is saved in
  `model.yaml` in the data dir (the key stays in that file, on the
  machine). Precedence at startup: `MIA_MODEL_KEY` (with
  `MIA_MODEL_BASE_URL` / `MIA_MODEL_ID`, defaults
  `https://api.meta.ai/v1` / `muse-spark-1.1`) wins over `model.yaml`,
  which wins over `NoModel`. `mia --dry-run` prints which model
  would be used without opening the GUI. Note: `OpenAiCompat` is
  built from an `LlmConfig`, not from a `with_key` constructor (that
  one belongs to the speech models).
- **Data**: `--data-dir` (default `$HOME/mia-data`) holds `wiki/`,
  `graph.db`, `USER.md` and `model.yaml`; `--wiki-dir` / `--graph-db` /
  `--user-md` override each path.
- **Deviation from the spec**: the spec pins `wgpu` 24, but
  `sequoia-ui` 0.1.5 brings its own wgpu through
  `sequoia_platform` — `mia-ui` does not depend on `wgpu`
  directly. The graph canvas is 2D in v0.1, with a WGSL glow pass
  on top (Phase 5, `mia-ui::glow`).
- **Smoke-tested headless**: on a VM without a display the app runs
  under Xvfb with the Mesa lavapipe software Vulkan driver
  (`libxkbcommon-x11-0` + `mesa-vulkan-drivers` installed) and stays
  alive without panicking — the first real run with interaction
  still belongs on a machine with a display.

## Phase 4 — MCP containers ✓

Every real-world capability is its own Docker container exposing MCP
over stdio; `mia` connects to them from `mia-data/mcp.yaml`.

- **`mia-mcp`** — the client side. `McpProvider` connects to one
  server (stdio process or Streamable HTTP) with
  `harness-mcp-client` 0.0.72 and exposes its tools as a
  `ToolProvider` for the runtime. The write gate lives here: a tool
  not listed in `read_only_tools` is a write and follows
  `write_policy` — `deny` (default, refuses), `allow` (local trusted
  servers only), `prompt` (asks on the terminal, s/n). The spec's
  rule is enforced in code: no write without explicit confirmation,
  never a direct `mail_send`. A server that fails to start does not
  crash the app — `mia` warns on stderr and continues without it.
  `mia --dry-run` prints the model and the connected MCP servers.
  `McpConfig` (`mcp.yaml`) is serde YAML: name, transport
  (`stdio` + program/args, or `http` + url), `read_only_tools`,
  `write_policy`. Note: the harness client is built on rmcp 1.7
  while the servers use rmcp 3.11 — protocol negotiation makes them
  interoperate (verified by the integration tests).
- **`mia-google`** — Google OAuth device flow (RFC 8628) written by
  hand with reqwest, no OAuth library: `start_device_flow` returns
  the user code + verification URL, `poll_device_flow` waits and
  saves the refresh token, `access_token()` renews it silently.
  Credentials come from `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`
  or `GOOGLE_CLIENT_SECRET_FILE`; tokens live only in
  `<data>/token.json`. Tested against a fake Google server.
- **Containers** (`containers/`, see `containers/README.md`):
  `mia-notes` (text files: `list-notes`, `read-note`, `write-note`,
  `delete-note`), `mia-browser` (`fetch-url`, http/https reader with
  HTML stripped to text — no JavaScript), `mia-calendar`
  (`list-events`, `create-event`), `mia-mail` (`list-messages`,
  `read-message`, `send-message`). Calendar and mail have an `auth`
  subcommand for the one-time device flow; without credentials they
  still start and their tools explain how to authorize. Each has a
  multi-stage Dockerfile (Rust 1.88 builder → Debian slim, non-root
  user); `docker-compose.yml` wires the four services with volumes
  and stdin attached for the stdio transport. CI
  (`.github/workflows/containers.yml`) builds all four images,
  validates the compose file and runs the MCP/container tests.
  Docker is not on the dev VM, so images are verified by CI, not
  locally.

## Phase 5 — Polish ✓

- **WGSL graph glow** (`mia-ui::glow`): the graph canvas keeps its
  2D `Painter` drawing and wraps it in `Canvas::with_effect` with a
  `graph_glow` shader (`shader!` macro, validated by naga at
  compile time): a subtle accent-coloured lift proportional to pixel
  luminance. Registered with `App::shader_type::<GraphGlow>()` —
  without registration the draw is silently skipped. The shader
  path is a fragment effect (≤256 B uniforms, max 3 textures), not
  geometry: procedural nodes inside the shader don't scale, so the
  2D drawing stays (full custom geometry would need `WgpuSurface`).
- **Docs**: this README now covers Phases 4–5; `containers/README.md`
  documents build, auth and the `mcp.yaml` wiring.
- **Still open**: `mia-cli` (run/ingest/export, Phase 2 leftover),
  a GUI confirmation dialog for MCP writes (today `write_policy`
  is `deny` in the GUI — writes need the terminal `prompt` policy
  or an explicit `allow`), and JS rendering in `mia-browser`.

## Verify

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
