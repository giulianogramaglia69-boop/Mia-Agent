//! File-system wiki engine: Markdown pages + a persistent tantivy index.
//!
//! Layout under the base directory:
//!
//! ```text
//! data/wiki/
//! ├── raw/          # immutable sources, one file per ingest
//! ├── pages/        # compiled pages (<slug>.md)
//! └── index/        # tantivy index (persistent)
//! ```
//!
//! The index stores `id` and `title`; the body is indexed but not
//! stored — search reads the page file back for content, so the file
//! system stays the single source of truth.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use mia_core::{KnowledgeEngine, KnowledgeEntry, KnowledgeId, MiaError, Result};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::parse::{plain_text, slugify, split_frontmatter, title_of};
use crate::types::{Page, PageId, PageMeta, Source};
use crate::WikiEngine;

/// A wiki backed by Markdown files and a tantivy index.
pub struct FileWiki {
    base: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    id_field: Field,
    title_field: Field,
    body_field: Field,
}

fn storage_err(err: impl std::fmt::Display) -> MiaError {
    MiaError::Storage(err.to_string())
}

impl FileWiki {
    /// Open (or create) a wiki at `base`, creating `raw/`, `pages/`
    /// and `index/` when missing.
    pub fn open(base: impl AsRef<Path>) -> Result<Self> {
        let base = base.as_ref().to_path_buf();
        for sub in ["raw", "pages", "index"] {
            fs::create_dir_all(base.join(sub)).map_err(storage_err)?;
        }
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index_dir = base.join("index");
        let index = match Index::open_in_dir(&index_dir) {
            Ok(index) => index,
            Err(_) => Index::create_in_dir(&index_dir, schema).map_err(storage_err)?,
        };
        let writer = index.writer(50_000_000).map_err(storage_err)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(storage_err)?;
        Ok(Self {
            base,
            inner: Mutex::new(Inner {
                index,
                writer,
                reader,
                id_field,
                title_field,
                body_field,
            }),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|_| MiaError::Internal("wiki lock poisoned".into()))
    }

    /// Pick a free slug for `title` (appends `-2`, `-3`, … on collision).
    fn free_slug(&self, title: &str) -> Result<String> {
        let base_slug = slugify(title);
        let mut slug = base_slug.clone();
        let mut n = 2;
        while self.base.join("pages").join(format!("{slug}.md")).exists() {
            slug = format!("{base_slug}-{n}");
            n += 1;
        }
        Ok(slug)
    }

    /// Read and parse a compiled page file.
    fn read_page_file(&self, id: &PageId) -> Result<Page> {
        let path = self.base.join("pages").join(format!("{}.md", id.0));
        let raw = fs::read_to_string(&path).map_err(|_| MiaError::NotFound(id.0.clone()))?;
        let (frontmatter, body) = split_frontmatter(&raw).map_err(MiaError::InvalidInput)?;
        let title = frontmatter
            .as_ref()
            .and_then(|fm| fm.title.clone())
            .unwrap_or_else(|| title_of(body));
        Ok(Page {
            id: id.clone(),
            title,
            content: body.to_string(),
            frontmatter: frontmatter.unwrap_or_default(),
            score: 0.0,
        })
    }
}

#[async_trait]
impl WikiEngine for FileWiki {
    async fn ingest(&self, source: &Source) -> Result<PageId> {
        let (title_override, markdown) = match source {
            Source::Text { title, markdown } => (title.clone(), markdown.clone()),
            Source::File(path) => (
                None,
                fs::read_to_string(path).map_err(|e| {
                    MiaError::InvalidInput(format!("cannot read {}: {e}", path.display()))
                })?,
            ),
        };
        if markdown.trim().is_empty() {
            return Err(MiaError::InvalidInput("source must not be empty".into()));
        }
        let (frontmatter, body) =
            split_frontmatter(&markdown).map_err(MiaError::InvalidInput)?;
        let title = title_override
            .or_else(|| frontmatter.as_ref().and_then(|fm| fm.title.clone()))
            .unwrap_or_else(|| title_of(body));
        let slug = self.free_slug(&title)?;

        // Immutable source copy, then the compiled page.
        let raw_name = format!("{slug}.md");
        fs::write(self.base.join("raw").join(&raw_name), &markdown).map_err(storage_err)?;
        fs::write(
            self.base.join("pages").join(format!("{slug}.md")),
            &markdown,
        )
        .map_err(storage_err)?;

        // Index the plain text.
        let body_text = plain_text(body);
        let mut inner = self.lock()?;
        inner
            .writer
            .add_document(doc!(
                inner.id_field => slug.clone(),
                inner.title_field => title.clone(),
                inner.body_field => body_text,
            ))
            .map_err(storage_err)?;
        inner.writer.commit().map_err(storage_err)?;
        Ok(PageId::new(slug))
    }

    async fn search(&self, query: &str) -> Result<Vec<Page>> {
        if query.trim().is_empty() {
            return Err(MiaError::InvalidInput("query must not be empty".into()));
        }
        let inner = self.lock()?;
        inner.reader.reload().map_err(storage_err)?;
        let searcher = inner.reader.searcher();
        let parser = QueryParser::for_index(&inner.index, vec![inner.title_field, inner.body_field]);
        let parsed = parser.parse_query(query).or_else(|_| {
            // Fallback: treat the whole query as a literal phrase.
            parser.parse_query(&format!("\"{}\"", query.replace('"', "")))
        });
        let query = parsed.map_err(|e| MiaError::InvalidInput(format!("bad query: {e}")))?;
        let id_field = inner.id_field;
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(10))
            .map_err(storage_err)?;
        drop(inner);
        let mut pages = Vec::new();
        for (score, addr) in top_docs {
            let stored: TantivyDocument = searcher.doc(addr).map_err(storage_err)?;
            let id = stored
                .get_first(id_field)
                .and_then(|v| match v {
                    tantivy::schema::OwnedValue::Str(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let mut page = self.read_page_file(&PageId::new(id))?;
            page.score = score;
            pages.push(page);
        }
        Ok(pages)
    }

    async fn read(&self, id: &PageId) -> Result<Page> {
        self.read_page_file(id)
    }

    async fn list_pages(&self) -> Result<Vec<PageMeta>> {
        let mut metas = Vec::new();
        let dir = fs::read_dir(self.base.join("pages")).map_err(storage_err)?;
        for entry in dir {
            let entry = entry.map_err(storage_err)?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                let id = PageId::new(
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
                let page = self.read_page_file(&id)?;
                metas.push(PageMeta {
                    id,
                    title: page.title,
                });
            }
        }
        metas.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(metas)
    }
}

#[async_trait]
impl KnowledgeEngine for FileWiki {
    async fn ingest(&self, source: &str) -> Result<KnowledgeId> {
        let id = WikiEngine::ingest(
            self,
            &Source::Text {
                title: None,
                markdown: source.to_string(),
            },
        )
        .await?;
        Ok(KnowledgeId::new(id.0))
    }

    async fn search(&self, query: &str) -> Result<Vec<KnowledgeEntry>> {
        let pages = WikiEngine::search(self, query).await?;
        Ok(pages
            .into_iter()
            .map(|p| KnowledgeEntry {
                id: KnowledgeId::new(p.id.0),
                title: p.title,
                content: p.content,
                score: p.score,
            })
            .collect())
    }
}
