// SPDX-License-Identifier: Apache-2.0
//! Stub entity extractor for topic-tree fan-out (Phase 3c — PR10).
//!
//! Pure-regex pattern matcher with stopword filter. Three patterns:
//! - `@mentions` like `@alice`
//! - `#hashtags` like `#design`
//! - Capitalized 1-3 word phrases like `Alice Wong` or `Project Phoenix`
//!
//! Returns a deterministically-sorted, deduplicated, capped (top 20) Vec
//! per chunk.
//!
//! Future work: PR12 jobs swap this for an LLM-driven NER pass that
//! handles CJK, case normalization, and entity-id canonicalization.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeSet;

const MAX_ENTITIES_PER_CHUNK: usize = 20;

/// Stopwords filtered from capitalized-phrase matches only — mentions and
/// hashtags are kept verbatim. A single-token capitalized match that
/// equals (case-sensitive) one of these is dropped.
const STOPWORDS: &[&str] = &[
    "The", "A", "An", "And", "Or", "But", "If", "I", "You", "We", "They",
    "He", "She", "It", "Is", "Was", "Are", "Be", "Been", "This", "That",
    "These", "Those", "What", "When", "Where", "Who", "Why", "How",
];

static MENTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@(\w{2,})").unwrap());
static HASHTAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#(\w{2,})").unwrap());
static CAPS_PHRASE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2}\b").unwrap());

/// Extract entities from `text`. Returns up to [`MAX_ENTITIES_PER_CHUNK`]
/// unique entities, sorted ascending for deterministic order.
pub fn extract_entities(text: &str) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();

    // @mentions — keep the leading @ so trees stay distinguishable from
    // bare capitalized phrases ("@Alice" and "Alice" → separate trees).
    for cap in MENTION_RE.captures_iter(text) {
        if let Some(m) = cap.get(0) {
            out.insert(m.as_str().to_string());
        }
    }

    // #hashtags
    for cap in HASHTAG_RE.captures_iter(text) {
        if let Some(m) = cap.get(0) {
            out.insert(m.as_str().to_string());
        }
    }

    // Capitalized phrases — single-word matches checked against STOPWORDS.
    for m in CAPS_PHRASE_RE.find_iter(text) {
        let s = m.as_str();
        let is_single_word = !s.contains(' ');
        if is_single_word && STOPWORDS.contains(&s) {
            continue;
        }
        out.insert(s.to_string());
    }

    out.into_iter().take(MAX_ENTITIES_PER_CHUNK).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mentions() {
        let out = extract_entities("hello @alice and @bob");
        assert!(out.contains(&"@alice".to_string()));
        assert!(out.contains(&"@bob".to_string()));
    }

    #[test]
    fn extract_hashtags() {
        let out = extract_entities("#design and #ux meeting");
        assert!(out.contains(&"#design".to_string()));
        assert!(out.contains(&"#ux".to_string()));
    }

    #[test]
    fn extract_capitalized_single_word() {
        let out = extract_entities("Project Phoenix is launching tomorrow.");
        assert!(out.contains(&"Project Phoenix".to_string()));
    }

    #[test]
    fn extract_capitalized_two_words() {
        let out = extract_entities("Met with Alice Wong yesterday.");
        assert!(out.contains(&"Alice Wong".to_string()));
    }

    #[test]
    fn extract_capitalized_three_words() {
        let out = extract_entities("North San Francisco is congested.");
        assert!(out.contains(&"North San Francisco".to_string()));
    }

    #[test]
    fn filter_single_word_stopwords() {
        let out = extract_entities("The quick brown fox jumps");
        assert!(!out.contains(&"The".to_string()));
    }

    #[test]
    fn keep_capitalized_words_in_multi_word_phrase() {
        // "The" as part of a multi-word capitalized phrase IS kept because
        // it's a phrase, not a single token. e.g., "The Beatles" stays.
        let out = extract_entities("The Beatles released a new album.");
        assert!(out.contains(&"The Beatles".to_string()));
    }

    #[test]
    fn dedup_repeated_matches() {
        let out = extract_entities("Alice met Alice in Alice's office.");
        let alice_count = out.iter().filter(|s| s == &&"Alice".to_string()).count();
        assert_eq!(alice_count, 1);
    }

    #[test]
    fn dedup_preserves_sorted_order() {
        let out = extract_entities("Bob met Alice yesterday.");
        // sorted ascending: "Alice" < "Bob"
        let alice_idx = out.iter().position(|s| s == "Alice");
        let bob_idx = out.iter().position(|s| s == "Bob");
        if let (Some(a), Some(b)) = (alice_idx, bob_idx) {
            assert!(a < b);
        }
    }

    #[test]
    fn empty_text_returns_empty() {
        let out = extract_entities("");
        assert!(out.is_empty());
    }

    #[test]
    fn no_entities_returns_empty() {
        let out = extract_entities("the quick brown fox jumps over the lazy dog");
        assert!(out.is_empty());
    }

    #[test]
    fn cap_at_max_entities_per_chunk() {
        // Build a text with 30 distinct capitalized names; cap should limit to 20.
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!("Person{i:02} mentioned. "));
        }
        let out = extract_entities(&text);
        assert!(out.len() <= MAX_ENTITIES_PER_CHUNK);
    }

    #[test]
    fn min_length_2_for_mentions_and_hashtags() {
        let out = extract_entities("@a and #b are too short");
        assert!(!out.iter().any(|s| s == "@a" || s == "#b"));
    }
}
