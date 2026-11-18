//! Integration tests for the `mia-wiki` engine.
//!
//! Each test opens a fresh wiki in a temporary directory.

use mia_core::KnowledgeEngine;
use mia_wiki::{FileWiki, PageId, Source, WikiEngine};

fn fresh_wiki(name: &str) -> (FileWiki, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mia-wiki-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let wiki = FileWiki::open(&dir).unwrap();
    (wiki, dir)
}

#[tokio::test]
async fn ingest_text_then_search_finds_it() {
    let (wiki, dir) = fresh_wiki("search");
    let id = WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "# Progetto Alpha\n\nMario ha collaborato con Luigi al progetto Alpha.".into(),
        })
        .await
        .unwrap();
    assert_eq!(id.0, "progetto-alpha");
    let hits = WikiEngine::search(&wiki, "Alpha").await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);
    assert!(hits[0].score > 0.0);
    assert!(hits[0].content.contains("Mario"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn search_no_match_returns_empty() {
    let (wiki, dir) = fresh_wiki("nomatch");
    WikiEngine::ingest(&wiki, &Source::Text {
        title: None,
        markdown: "# Alpha\n\nContenuto sul progetto Alpha.".into(),
    })
    .await
    .unwrap();
    assert!(WikiEngine::search(&wiki, "Zebra").await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ingest_empty_source_is_invalid_input() {
    let (wiki, dir) = fresh_wiki("empty");
    let err = WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "   ".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, mia_core::MiaError::InvalidInput(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn frontmatter_title_wins_over_h1() {
    let (wiki, dir) = fresh_wiki("frontmatter");
    let id = WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "---\ntitle: Titolo vero\ntags: [mia, test]\n---\n# Titolo H1\n\nCorpo.".into(),
        })
        .await
        .unwrap();
    assert_eq!(id.0, "titolo-vero");
    let page = wiki.read(&id).await.unwrap();
    assert_eq!(page.title, "Titolo vero");
    assert_eq!(page.frontmatter.tags, vec!["mia", "test"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn duplicate_titles_get_unique_slugs() {
    let (wiki, dir) = fresh_wiki("dup");
    let first = WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "# Alpha\n\nPrimo.".into(),
        })
        .await
        .unwrap();
    let second = WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "# Alpha\n\nSecondo.".into(),
        })
        .await
        .unwrap();
    assert_eq!(first.0, "alpha");
    assert_eq!(second.0, "alpha-2");
    assert_eq!(wiki.list_pages().await.unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn read_missing_page_is_not_found() {
    let (wiki, dir) = fresh_wiki("missing");
    let err = wiki.read(&PageId::new("nope")).await.unwrap_err();
    assert!(matches!(err, mia_core::MiaError::NotFound(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn list_pages_is_sorted_by_title() {
    let (wiki, dir) = fresh_wiki("list");
    for title in ["Zebra", "Alpha", "Mario"] {
        WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: format!("# {title}\n\nCorpo."),
        })
        .await
        .unwrap();
    }
    let titles: Vec<String> = wiki
        .list_pages()
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert_eq!(titles, vec!["Alpha", "Mario", "Zebra"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ingest_file_copies_it_into_the_wiki() {
    let (wiki, dir) = fresh_wiki("file");
    let src = dir.join("appunti.md");
    std::fs::write(&src, "# Appunti\n\nDa ricordare.").unwrap();
    let id = WikiEngine::ingest(&wiki, &Source::File(src)).await.unwrap();
    assert_eq!(id.0, "appunti");
    assert!(dir.join("raw").join("appunti.md").exists());
    assert!(dir.join("pages").join("appunti.md").exists());
    assert_eq!(WikiEngine::search(&wiki, "ricordare").await.unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn data_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("mia-wiki-test-reopen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let wiki = FileWiki::open(&dir).unwrap();
        WikiEngine::ingest(&wiki, &Source::Text {
            title: None,
            markdown: "# Alpha\n\nContenuto.".into(),
        })
        .await
        .unwrap();
    }
    {
        let wiki = FileWiki::open(&dir).unwrap();
        assert_eq!(wiki.list_pages().await.unwrap().len(), 1);
        assert_eq!(WikiEngine::search(&wiki, "Alpha").await.unwrap().len(), 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn knowledge_engine_trait_works() {
    let (wiki, dir) = fresh_wiki("knowledge");
    let id = KnowledgeEngine::ingest(&wiki, "# Alpha\n\nMario e Luigi.").await.unwrap();
    let hits = KnowledgeEngine::search(&wiki, "Luigi").await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);
    let _ = std::fs::remove_dir_all(&dir);
}
