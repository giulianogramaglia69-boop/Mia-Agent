//! Markdown parsing helpers: frontmatter, title, plain text, slugs.
//!
//! ```rust
//! use mia_wiki::parse::{plain_text, split_frontmatter, title_of};
//!
//! let md = "# Alpha\n\nMario e Luigi ci lavorano.";
//! assert_eq!(title_of(md), "Alpha");
//! assert!(plain_text(md).contains("Mario"));
//! assert!(split_frontmatter(md).unwrap().0.is_none());
//! ```

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::types::Frontmatter;

/// Split `---` YAML frontmatter from the Markdown body.
///
/// Returns `(frontmatter, body)`. A malformed YAML block is an error;
/// a document without frontmatter returns `(None, whole_text)`.
pub fn split_frontmatter(markdown: &str) -> Result<(Option<Frontmatter>, &str), String> {
    let Some(rest) = markdown.strip_prefix("---\n").or_else(|| markdown.strip_prefix("---\r\n"))
    else {
        return Ok((None, markdown));
    };
    let Some(end) = rest.find("\n---") else {
        return Ok((None, markdown));
    };
    let yaml = &rest[..end];
    let fm: Frontmatter =
        serde_yaml::from_str(yaml).map_err(|e| format!("invalid frontmatter YAML: {e}"))?;
    let body = &rest[end + 4..];
    let body = body.strip_prefix('\n').unwrap_or(body);
    Ok((Some(fm), body))
}

/// Title of a Markdown document: the first H1, or `"Untitled"`.
pub fn title_of(markdown: &str) -> String {
    let mut in_h1 = false;
    let mut title = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading {
                level: pulldown_cmark::HeadingLevel::H1,
                ..
            }) => {
                in_h1 = true;
            }
            Event::End(TagEnd::Heading(pulldown_cmark::HeadingLevel::H1)) => {
                break;
            }
            Event::Text(text) if in_h1 => title.push_str(&text),
            Event::Code(code) if in_h1 => title.push_str(&code),
            _ => {}
        }
    }
    let title = title.trim();
    if title.is_empty() {
        "Untitled".to_string()
    } else {
        title.to_string()
    }
}

/// Plain searchable text of a Markdown document: all text and inline
/// code, headings included, markup stripped.
pub fn plain_text(markdown: &str) -> String {
    let mut out = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(text) | Event::Code(text) => {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Turn a title into a URL-safe slug (`"Progetto Alpha"` → `"progetto-alpha"`).
/// Accented letters are transliterated; anything else becomes `-`.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in title.chars() {
        let base = match ch {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        };
        if base.is_ascii_alphanumeric() {
            slug.push(base.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "page".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_title_and_tags() {
        let md = "---\ntitle: Alpha\ntags: [rust, mia]\n---\n# Ignored\n\nBody.";
        let (fm, body) = split_frontmatter(md).unwrap();
        let fm = fm.unwrap();
        assert_eq!(fm.title.as_deref(), Some("Alpha"));
        assert_eq!(fm.tags, vec!["rust", "mia"]);
        assert!(body.starts_with("# Ignored"));
    }

    #[test]
    fn no_frontmatter_returns_whole_text() {
        let md = "# Alpha\n\nBody.";
        let (fm, body) = split_frontmatter(md).unwrap();
        assert!(fm.is_none());
        assert_eq!(body, md);
    }

    #[test]
    fn bad_frontmatter_is_an_error() {
        let md = "---\ntitle: [unclosed\n---\nBody.";
        assert!(split_frontmatter(md).is_err());
    }

    #[test]
    fn title_prefers_first_h1() {
        assert_eq!(title_of("# Alpha\n\ntext"), "Alpha");
        assert_eq!(title_of("no heading here"), "Untitled");
    }

    #[test]
    fn plain_text_strips_markup() {
        let text = plain_text("# Alpha\n\nMario **e** Luigi `codice`.");
        assert!(text.contains("Alpha"));
        assert!(text.contains("Mario"));
        assert!(!text.contains("**"));
    }

    #[test]
    fn slugs_are_clean() {
        assert_eq!(slugify("Progetto Alpha"), "progetto-alpha");
        assert_eq!(slugify("Città d'arte!"), "citta-d-arte");
        assert_eq!(slugify("---"), "page");
    }
}
