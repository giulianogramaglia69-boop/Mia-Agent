//! Triple extraction from text.
//!
//! v0.1 extracts only explicit triples written as
//! `Subject --relation--> Object` — the same notation used in the
//! `mia-core` contract tests. Free narrative text yields no facts:
//! turning prose into triples is the runtime's job (Phase 2), where
//! the model emits triples in this notation and `remember` stores them.
//!
//! ```rust
//! use mia_graph::extract_triples;
//!
//! let facts = extract_triples("Mario --lavora_con--> Luigi.");
//! assert_eq!(facts.len(), 1);
//! assert_eq!(facts[0].subject, "Mario");
//! assert_eq!(facts[0].relation, "lavora_con");
//! assert_eq!(facts[0].object, "Luigi");
//! ```

use mia_core::GraphFact;

/// Extract all `Subject --relation--> Object` triples from `text`.
///
/// Triples are deduplicated; malformed fragments are skipped.
pub fn extract_triples(text: &str) -> Vec<GraphFact> {
    let mut facts = Vec::new();
    let mut rest = text;
    while let Some(arrow) = rest.find("-->") {
        let before = &rest[..arrow];
        let after = &rest[arrow + 3..];
        if let Some(dash) = before.rfind("--") {
            let relation = before[dash + 2..].trim();
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
            if !subject.is_empty() && !relation.is_empty() && !object.is_empty() {
                let fact = GraphFact {
                    subject: subject.to_string(),
                    relation: relation.to_string(),
                    object: object.to_string(),
                };
                if !facts.contains(&fact) {
                    facts.push(fact);
                }
            }
        }
        rest = after;
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_two_triples() {
        let facts = extract_triples("Mario --lavora_con--> Luigi. Mario --progetto--> Alpha.");
        assert_eq!(facts.len(), 2);
        assert_eq!(
            facts[0],
            GraphFact {
                subject: "Mario".into(),
                relation: "lavora_con".into(),
                object: "Luigi".into(),
            }
        );
        assert_eq!(facts[1].object, "Alpha");
    }

    #[test]
    fn narrative_without_triples_yields_nothing() {
        assert!(extract_triples("Mario ha collaborato con Luigi al progetto Alpha.").is_empty());
        assert!(extract_triples("").is_empty());
    }

    #[test]
    fn duplicate_triples_are_deduplicated() {
        let facts = extract_triples("Mario --lavora_con--> Luigi. Mario --lavora_con--> Luigi.");
        assert_eq!(facts.len(), 1);
    }

    #[test]
    fn malformed_fragments_are_skipped() {
        // No "-->" arrow, no relation, no object: nothing extractable.
        assert!(extract_triples("Mario --lavora_con Luigi.").is_empty());
        assert!(extract_triples("Mario --> Luigi.").is_empty());
        assert!(extract_triples("--lavora_con--> Luigi.").is_empty());
    }
}
