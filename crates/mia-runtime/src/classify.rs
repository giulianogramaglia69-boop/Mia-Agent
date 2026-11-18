//! Input classification for the learning flow (anti-overlap rule).
//!
//! Every input is classified once, before anything else happens:
//! a stable preference goes to `USER.md`, a relational fact to the
//! graph, a document or concept to the wiki. A question consults the
//! engines and then runs the loop. Nothing is ever stored twice.
//!
//! v0.1 uses explicit, deterministic markers (see [`classify`]).
//! Model-based classification arrives once a real model is wired.
//!
//! ```rust
//! use mia_runtime::{classify, InputKind};
//!
//! assert_eq!(classify("Mario --lavora_con--> Luigi."), InputKind::RelationalFact);
//! assert_eq!(classify("ricorda che preferisco risposte brevi"), InputKind::Preference);
//! assert_eq!(classify("# Appunti\n\nDa studiare."), InputKind::Document);
//! assert_eq!(classify("Chi lavora con Luigi?"), InputKind::Question);
//! ```

/// Where one input belongs, per the anti-overlap rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// A stable user preference → `USER.md` (max 20 lines).
    Preference,
    /// A relational fact (`A --rel--> B`) → `mia-graph`.
    RelationalFact,
    /// A document or concept (Markdown) → `mia-wiki`.
    Document,
    /// A question → consult wiki + graph, then run the loop.
    Question,
}

/// Classify `input` with v0.1 heuristics, in order:
///
/// 1. contains an explicit triple (`--rel-->`) → [`InputKind::RelationalFact`]
/// 2. starts with an Italian preference marker
///    (`preferisco…`, `ricorda che…`, `mi piace…`, `non mi piace…`)
///    → [`InputKind::Preference`]
/// 3. starts with a Markdown heading, is long (> 500 chars), or has
///    multiple paragraphs → [`InputKind::Document`]
/// 4. anything else → [`InputKind::Question`]
pub fn classify(input: &str) -> InputKind {
    let text = input.trim();
    let lower = text.to_lowercase();
    if text.contains("--") && text.contains("-->") {
        return InputKind::RelationalFact;
    }
    if lower.starts_with("preferisco")
        || lower.starts_with("ricorda che")
        || lower.starts_with("mi piace")
        || lower.starts_with("non mi piace")
    {
        return InputKind::Preference;
    }
    if text.starts_with('#') || text.len() > 500 || text.contains("\n\n") {
        return InputKind::Document;
    }
    InputKind::Question
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triples_are_relational_facts() {
        assert_eq!(
            classify("Mario --lavora_con--> Luigi."),
            InputKind::RelationalFact
        );
    }

    #[test]
    fn italian_preference_markers() {
        assert_eq!(
            classify("ricorda che preferisco risposte brevi"),
            InputKind::Preference
        );
        assert_eq!(classify("Preferisco il buio"), InputKind::Preference);
        assert_eq!(classify("Non mi piace il rumore"), InputKind::Preference);
    }

    #[test]
    fn markdown_is_a_document() {
        assert_eq!(
            classify("# Appunti\n\nDa studiare per domani."),
            InputKind::Document
        );
        assert_eq!(classify(&"x".repeat(600)), InputKind::Document);
    }

    #[test]
    fn plain_questions_are_questions() {
        assert_eq!(classify("Chi lavora con Luigi?"), InputKind::Question);
        assert_eq!(classify("Che tempo fa?"), InputKind::Question);
    }
}
