//! The three-column app shell: wiki tree | chat | graph canvas.
//!
//! Layout: an outer horizontal `SplitPanel` separates the wiki tree from
//! the rest; an inner horizontal `SplitPanel` separates the chat from the
//! graph canvas. Both dividers are draggable and their fractions live in
//! signals, so the layout survives re-renders.
//!
//! Threading: button callbacks are synchronous, but `MiaRuntime::handle`
//! is async — and the harness loop inside it needs a Tokio runtime
//! (it sleeps between turns). The submit path therefore runs the
//! runtime call through `AsyncRuntime::spawn_future`, whose future
//! blocks on the Tokio handle; the answer is applied back on the UI
//! thread, where it refreshes the chat, the wiki tree and the graph.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mia_core::{Entity, GraphEngine, Relation};
use mia_runtime::MiaRuntime;
use mia_wiki::{PageMeta, WikiEngine};
use sequoia_ui::core::{BrushStroke, Fill};
use sequoia_ui::platform::AppError;
use sequoia_ui::prelude::*;
use sequoia_ui::view::{Canvas, Painter};

use crate::chat::{ChatMessage, ChatRole, ChatRow};
use crate::layout::layout_graph;
use crate::tree::pages_to_nodes;

/// Accent blue for nodes and the user's label.
const ACCENT: Color = Color::hex(0x0A84FF);
/// Green for Mia's label.
const MIA_GREEN: Color = Color::hex(0x30D158);
/// Amber for wiki pages.
const WIKI_AMBER: Color = Color::hex(0xE5A50A);
/// Muted grey for edges and labels.
const MUTED: Color = Color::hex(0x94A3B8);
/// Dark canvas background.
const CANVAS_BG: Color = Color::hex(0x1C1C1E);

/// The running app: runtime + UI state, all cheaply cloneable so view
/// callbacks can own what they need.
#[derive(Clone)]
pub struct MiaApp {
    runtime: Arc<MiaRuntime>,
    wiki: Arc<dyn WikiEngine>,
    graph: Arc<dyn GraphEngine>,
    async_rt: AsyncRuntime,
    tokio: tokio::runtime::Handle,
    messages: Signal<Vec<ChatRow>>,
    next_id: Arc<AtomicU64>,
    draft: Signal<String>,
    pages: Signal<Vec<PageMeta>>,
    entities: Signal<Vec<Entity>>,
    relations: Signal<Vec<Relation>>,
    split_outer: Signal<f32>,
    split_inner: Signal<f32>,
}

impl MiaApp {
    /// Build the app around an already-constructed runtime and engines.
    /// Wiki pages and graph facts are loaded once here; they are
    /// reloaded after every answer (a reply may have stored new facts).
    pub fn new(
        runtime: Arc<MiaRuntime>,
        wiki: Arc<dyn WikiEngine>,
        graph: Arc<dyn GraphEngine>,
        tokio: tokio::runtime::Handle,
    ) -> Self {
        let app = Self {
            runtime,
            wiki,
            graph,
            async_rt: AsyncRuntime::with_default_threads(),
            tokio,
            messages: Signal::new(vec![ChatRow::new(
                0,
                ChatMessage::mia(
                    "Ciao, sono Mia. Scrivimi qui: salvo preferenze, fatti e pagine, \
                     e rispondo alle domande con quello che so.",
                ),
            )]),
            next_id: Arc::new(AtomicU64::new(1)),
            draft: Signal::new(String::new()),
            pages: Signal::new(Vec::new()),
            entities: Signal::new(Vec::new()),
            relations: Signal::new(Vec::new()),
            split_outer: Signal::new(0.22),
            split_inner: Signal::new(0.55),
        };
        app.refresh_all();
        app
    }

    /// Start the window and run the event loop on the current thread.
    pub fn run(self) -> Result<(), AppError> {
        App::new()
            .title("Mia Agent")
            .size(1280.0, 800.0)
            .root(self.view())
            .run()
    }

    /// The whole window: wiki | chat | graph. Consumes the app: the
    /// view tree must own its state, not borrow the caller.
    fn view(self) -> impl View {
        let outer = self.split_outer.clone();
        let inner = self.split_inner.clone();
        SplitPanel::new(outer)
            .horizontal()
            .min_first(180.0)
            .min_second(420.0)
            .handle_width(7.0)
            .first(self.wiki_column())
            .second(
                SplitPanel::new(inner)
                    .horizontal()
                    .min_first(320.0)
                    .min_second(240.0)
                    .handle_width(7.0)
                    .first(self.chat_column())
                    .second(self.graph_column()),
            )
    }

    /// Left column: the wiki tree. Clicking a page reads it into the chat.
    fn wiki_column(&self) -> impl View + use<> {
        let pages = self.pages.clone();
        let app = self.clone();
        Column::new()
            .fill_size()
            .padding(12.0)
            .gap(8.0)
            .child(Text::new("Wiki").size(15.0).bold())
            .child(
                TreeView::new(move || pages_to_nodes(&pages.get()))
                    .row(move |page: &PageMeta, _depth: usize| {
                        let page = page.clone();
                        let app = app.clone();
                        Container::new()
                            .child(Text::new(page.title.clone()).size(13.0))
                            .on_click(move || app.open_page(page.clone()))
                    })
                    .row_height(28.0)
                    .indent(16.0),
            )
    }

    /// Middle column: the conversation log plus the input row.
    fn chat_column(&self) -> impl View + use<> {
        let messages = self.messages.clone();
        let pending = self.async_rt.pending();
        let draft_field = self.draft.clone();
        let draft_submit = self.draft.clone();
        let draft_send = self.draft.clone();
        let app_submit = self.clone();
        let app_send = self.clone();
        Column::new()
            .fill_size()
            .padding(12.0)
            .gap(8.0)
            .child(
                Row::new()
                    .gap(8.0)
                    .child(Text::new("Chat").size(15.0).bold())
                    .child(
                        Text::with(move || {
                            if pending.get() > 0 {
                                "Mia sta pensando…".to_string()
                            } else {
                                String::new()
                            }
                        })
                        .size(13.0)
                        .color(MUTED),
                    ),
            )
            .child(
                Container::new().flex(1.0).child(
                    List::new(move || messages.get())
                        .key(|row: &ChatRow| row.id)
                        .item(|row: &ChatRow| {
                            let (label, color) = match row.message.role {
                                ChatRole::User => ("Tu", ACCENT),
                                ChatRole::Mia => ("Mia", MIA_GREEN),
                                ChatRole::Wiki => ("Wiki", WIKI_AMBER),
                            };
                            Column::new()
                                .gap(2.0)
                                .child(Text::new(label).size(12.0).bold().color(color))
                                .child(Text::new(row.message.text.clone()).size(13.0))
                        }),
                ),
            )
            .child(
                Row::new().gap(8.0).child(
                    Container::new().flex(1.0).child(
                        TextField::new(draft_field)
                            .placeholder("Scrivi a Mia…")
                            .on_submit(move |_| {
                                let text = draft_submit.get_untracked();
                                app_submit.submit(text);
                            }),
                    ),
                )
                .child(
                    Button::new("Invia")
                        .style(ButtonStyle::PRIMARY)
                        .on_click(move || {
                            let text = draft_send.get_untracked();
                            app_send.submit(text);
                        }),
                ),
            )
    }

    /// Right column: the knowledge graph, drawn on a canvas.
    fn graph_column(&self) -> impl View + use<> {
        let entities = self.entities.clone();
        let relations = self.relations.clone();
        Column::new()
            .fill_size()
            .padding(12.0)
            .gap(8.0)
            .child(Text::new("Graph").size(15.0).bold())
            .child(
                Canvas::new()
                    .on_draw(move |painter: &mut Painter| {
                        let rect = painter.rect();
                        let (w, h) = (rect.size.width, rect.size.height);
                        painter.fill(rect, Fill::solid(CANVAS_BG));
                        let layout =
                            layout_graph(&entities.get(), &relations.get(), w, h);
                        for edge in &layout.edges {
                            let from = &layout.nodes[edge.from];
                            let to = &layout.nodes[edge.to];
                            painter.line(
                                Point { x: from.x, y: from.y },
                                Point { x: to.x, y: to.y },
                                BrushStroke::new(1.5),
                                MUTED,
                            );
                            painter.text(
                                &edge.relation,
                                Point {
                                    x: (from.x + to.x) / 2.0 + 4.0,
                                    y: (from.y + to.y) / 2.0,
                                },
                                11.0,
                                MUTED,
                            );
                        }
                        for node in &layout.nodes {
                            painter.circle(
                                Point { x: node.x, y: node.y },
                                18.0,
                                Fill::solid(ACCENT),
                            );
                            painter.text_center(
                                Rect::from_xywh(node.x - 60.0, node.y + 22.0, 120.0, 16.0),
                                &node.name,
                                12.0,
                                Color::hex(0xFFFFFF),
                            );
                        }
                    })
                    .fill_size(),
            )
    }

    /// Append a message to the log with the next stable id.
    fn push(&self, message: ChatMessage) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.messages
            .update(|rows| rows.push(ChatRow::new(id, message)));
    }

    /// Send the user's input to the runtime. The answer (or the error)
    /// lands in the chat, then the wiki tree and the graph reload.
    fn submit(&self, input: String) {
        let input = input.trim().to_string();
        if input.is_empty() {
            return;
        }
        self.push(ChatMessage::user(&input));
        self.draft.set(String::new());
        let runtime = self.runtime.clone();
        let tokio = self.tokio.clone();
        let app = self.clone();
        self.async_rt.spawn_future(
            async move { tokio.block_on(runtime.handle(&input)) },
            move |result| {
                match result {
                    Ok(answer) => app.push(ChatMessage::mia(answer.text)),
                    Err(err) => app.push(ChatMessage::mia(format!("Errore: {err}"))),
                }
                app.refresh_all();
            },
        );
    }

    /// Read a wiki page and show it in the chat. The read is async, so
    /// it goes through the bridge and the message lands on the UI thread.
    fn open_page(&self, page: PageMeta) {
        let wiki = self.wiki.clone();
        let tokio = self.tokio.clone();
        let app = self.clone();
        let title = page.title.clone();
        self.async_rt.spawn_future(
            async move { tokio.block_on(wiki.read(&page.id)) },
            move |result| match result {
                Ok(page) => app.push(ChatMessage::wiki(format!(
                    "{}\n\n{}",
                    page.title, page.content
                ))),
                Err(err) => app.push(ChatMessage::mia(format!(
                    "Non riesco a leggere la pagina {title}: {err}"
                ))),
            },
        );
    }

    /// Reload the wiki tree and the graph canvas from the engines.
    /// The engines are async, so the load runs on the bridge and the
    /// signals are set back on the UI thread.
    fn refresh_all(&self) {
        let wiki = self.wiki.clone();
        let graph = self.graph.clone();
        let tokio = self.tokio.clone();
        let pages_sig = self.pages.clone();
        let entities_sig = self.entities.clone();
        let relations_sig = self.relations.clone();
        self.async_rt.spawn_future(
            async move {
                tokio.block_on(async {
                    let pages = wiki.list_pages().await.unwrap_or_default();
                    let entities = graph.entities().await.unwrap_or_default();
                    let relations = graph.recall("").await.unwrap_or_default();
                    (pages, entities, relations)
                })
            },
            move |(pages, entities, relations)| {
                pages_sig.set(pages);
                entities_sig.set(entities);
                relations_sig.set(relations);
            },
        );
    }
}
