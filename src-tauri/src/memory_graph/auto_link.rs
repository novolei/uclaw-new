//! Zero-LLM reference extractor + link-type inferrer for the Memory OS
//! Foundation Phase 2 auto-link post-hook.
//!
//! Design ref: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` §4.3.1.
//!
//! ## Why zero-LLM
//!
//! When an Agent writes `[[entity:zhang-san]]` in a version's compiled_truth,
//! we already have a structured reference — paying an LLM to "extract
//! entities" from that text would be redundant and expensive. The
//! post-hook just needs to (a) find the references syntactically and
//! (b) infer a *type* for the resulting edge based on cheap signals:
//! the source/destination node kinds and a handful of substring matches
//! in the surrounding context. The LLM stays out of the write path
//! entirely.
//!
//! ## Two-shape reference grammar
//!
//! The hook recognises three syntactic shapes inside compiled_truth, all
//! markdown-safe (won't render visibly in a markdown viewer unless the
//! viewer is markdown-link-aware):
//!
//! - `[[entity:zhang-san]]`       — slug lookup against EntityPage metadata
//! - `[[node:01abc...uuid...]]`   — direct UUID, no slug resolution needed
//! - `[Display Text](entity/zhang-san)` — gbrain-flavoured markdown link
//!
//! Code blocks (```fenced```) are stripped before the regex pass so we
//! never confuse code samples (`let foo = "[[entity:x]]"`) with real
//! references.

use once_cell::sync::Lazy;
use regex::Regex;

use super::models::{MemoryNodeKind, MemoryRelationKind};

/// A reference parsed out of a markdown text. The variant determines how
/// the auto-link hook resolves it to a `memory_nodes.id`:
///
/// - [`Self::EntitySlug`] → slug lookup on `entity_aliases` (Phase 15)
///   or, in Phase 2, `metadata_json.$.slug` of EntityPage rows.
/// - [`Self::NodeUuid`] → direct primary-key lookup.
/// - [`Self::MarkdownLink`] → path component is treated as
///   `<directory>/<slug>` and currently only `entity/<slug>` is wired.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExtractedRef {
    EntitySlug(String),
    NodeUuid(String),
    /// gbrain-flavoured markdown link: `[Text](dir/slug)`. We carry the
    /// directory so future phases can branch on it (e.g. `concept/<slug>`).
    MarkdownLink { dir: String, slug: String },
}

impl ExtractedRef {
    /// The lookup key — slug or uuid — without the wrapping syntax.
    /// Useful for hashing equivalent references across surface forms.
    pub fn key(&self) -> &str {
        match self {
            Self::EntitySlug(s) => s,
            Self::NodeUuid(u) => u,
            Self::MarkdownLink { slug, .. } => slug,
        }
    }
}

// ─── Regex patterns ────────────────────────────────────────────────────
//
// All patterns compiled once via Lazy so the hot path on create_version
// never re-parses.

/// Strips ```fenced code blocks``` (and ~~~ fenced) before extraction so
/// example code snippets don't leak into the reference set.
static RE_CODE_FENCE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)```.*?```|~~~.*?~~~").expect("code-fence regex"));

/// `[[entity:slug]]` — slug is `[a-z0-9-]+`. Trailing/leading whitespace
/// allowed inside the brackets to match natural markdown-editor habits.
static RE_ENTITY_SLUG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[\[\s*entity:\s*([a-z0-9][a-z0-9-]{0,127})\s*\]\]")
        .expect("entity-slug regex")
});

/// `[[node:UUID]]` — UUID grammar matches RFC-4122 form
/// (8-4-4-4-12 hex with dashes). Case-insensitive.
static RE_NODE_UUID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\[\[\s*node:\s*([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\s*\]\]")
        .expect("node-uuid regex")
});

/// `[Text](directory/slug)` — markdown link with a slash-separated
/// reference path. Only `entity/<slug>` is consumed by Phase 2; future
/// directories (concept/, source/, ...) flow through unchanged.
static RE_MD_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[([^\]\n]+)\]\((entity|concept|source|reference)/([a-z0-9][a-z0-9-]{0,127})\)")
        .expect("md-link regex")
});

/// Strip ALL fenced code blocks from `text`. Cheap when none exist.
fn strip_code_fences(text: &str) -> String {
    RE_CODE_FENCE.replace_all(text, "").into_owned()
}

/// Extract every reference appearing in `markdown` *outside* of code
/// fences. Returns refs in the order they first appear; duplicates are
/// preserved so callers that want a set can dedup themselves.
///
/// This function is allocation-light: it compiles regexes once (Lazy),
/// strips fences via one pass, and runs three find_iter() passes over
/// the stripped string.
pub fn extract_refs(markdown: &str) -> Vec<ExtractedRef> {
    let stripped = strip_code_fences(markdown);
    let mut out = Vec::new();

    for cap in RE_ENTITY_SLUG.captures_iter(&stripped) {
        out.push(ExtractedRef::EntitySlug(cap[1].to_string()));
    }
    for cap in RE_NODE_UUID.captures_iter(&stripped) {
        // Normalize UUID to lowercase so equality / dedup is stable
        // regardless of how the caller capitalized it in their text.
        out.push(ExtractedRef::NodeUuid(cap[1].to_lowercase()));
    }
    for cap in RE_MD_LINK.captures_iter(&stripped) {
        out.push(ExtractedRef::MarkdownLink {
            dir: cap[2].to_string(),
            slug: cap[3].to_string(),
        });
    }

    out
}

// ─── Link-type inference ───────────────────────────────────────────────
//
// gbrain's `inferLinkType` heuristic, faithful to the original spec
// §4.3.1 / src/core/link-extraction.ts. Tier 1: kind-pair gating
// (e.g. EntityPage→EntityPage is the only pair that can yield WorksAt /
// Founded / etc.). Tier 2: substring tests for English + Chinese cues.
//
// The cues are intentionally simple — this is a 5ms write-path hook,
// not an NER pipeline. Phase 15 adds NER for the harder cases.

/// Infer a typed edge between `src_kind` and `dst_kind` based on the
/// surrounding `context_text`. Falls back to [`MemoryRelationKind::Mentions`]
/// when no specific rule matches.
///
/// The context is lower-cased once at the start, so callers don't need
/// to pre-normalize.
pub fn infer_link_type(
    src_kind: MemoryNodeKind,
    dst_kind: MemoryNodeKind,
    context_text: &str,
) -> MemoryRelationKind {
    use MemoryNodeKind::*;
    use MemoryRelationKind::*;

    let lower = context_text.to_lowercase();

    // Default rule: any edge into a Reference is treated as a citation.
    if dst_kind == Reference {
        return Source;
    }

    // Domain-specific rules currently apply only when the relationship
    // is between two "page-like" nodes (an EntityPage or UserProfile
    // talking about another EntityPage). Anything else stays Mentions.
    match (src_kind, dst_kind) {
        (UserProfile | EntityPage, EntityPage) => {
            // Order matters: more specific cues before generic ones.
            if contains_any(&lower, &["works at", "works as", "employed at",
                                      "在职于", "员工", "就职于"])
            {
                WorksAt
            } else if contains_any(&lower, &["founded", "co-founded", "started",
                                             "创立", "创办", "creator of"])
            {
                Founded
            } else if contains_any(&lower, &["invested in", "backed", "led the round",
                                             "领投", "投资了"])
            {
                InvestedIn
            } else if contains_any(&lower, &["advises", "advisor", "advising",
                                             "顾问", "担任顾问"])
            {
                Advises
            } else if contains_any(&lower, &["attended", "alumni", "graduated from",
                                             "出席", "毕业于"])
            {
                Attended
            } else {
                Mentions
            }
        }
        _ => Mentions,
    }
}

/// Tiny helper: does `haystack` contain any of `needles`?
/// `haystack` is expected to be lowercase already.
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── strip_code_fences ────────────────────────────────────────

    #[test]
    fn strip_fences_removes_triple_backtick_blocks() {
        let input = "before\n```rust\nlet x = \"[[entity:fake]]\";\n```\nafter";
        let out = strip_code_fences(input);
        assert!(!out.contains("[[entity:fake]]"));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn strip_fences_removes_tilde_blocks() {
        let input = "text\n~~~\n[[node:00000000-0000-0000-0000-000000000000]]\n~~~\nmore";
        let out = strip_code_fences(input);
        assert!(!out.contains("[[node:00000000-0000-0000-0000-000000000000]]"));
    }

    #[test]
    fn strip_fences_is_noop_when_no_fence() {
        let input = "plain text with [[entity:slug]] and nothing else";
        assert_eq!(strip_code_fences(input), input);
    }

    // ─── extract_refs ──────────────────────────────────────────────

    #[test]
    fn extracts_entity_slug() {
        let refs = extract_refs("Met with [[entity:zhang-san]] today.");
        assert_eq!(refs, vec![ExtractedRef::EntitySlug("zhang-san".into())]);
    }

    #[test]
    fn extracts_node_uuid_case_insensitive_normalized_lower() {
        let refs = extract_refs(
            "Referencing [[node:00000000-AAAA-BBBB-CCCC-FFFFFFFFFFFF]] elsewhere.",
        );
        assert_eq!(
            refs,
            vec![ExtractedRef::NodeUuid(
                "00000000-aaaa-bbbb-cccc-ffffffffffff".into()
            )]
        );
    }

    #[test]
    fn extracts_markdown_link_entity_dir() {
        let refs = extract_refs("See [Acme Inc](entity/acme-inc) for details.");
        assert_eq!(
            refs,
            vec![ExtractedRef::MarkdownLink {
                dir: "entity".into(),
                slug: "acme-inc".into(),
            }]
        );
    }

    #[test]
    fn extracts_markdown_link_other_dirs() {
        // concept / source / reference are all allowed; the hook will
        // route them to the appropriate resolver in Phase 2.
        let refs = extract_refs("[RAG](concept/rag) and [paper](source/karpathy-gist)");
        assert_eq!(refs.len(), 2);
        assert!(matches!(refs[0], ExtractedRef::MarkdownLink { ref dir, .. } if dir == "concept"));
        assert!(matches!(refs[1], ExtractedRef::MarkdownLink { ref dir, .. } if dir == "source"));
    }

    #[test]
    fn extracts_multiple_refs_in_order() {
        let txt = "[[entity:a]] and [[entity:b]] and [[entity:c]]";
        let refs = extract_refs(txt);
        let slugs: Vec<&str> = refs
            .iter()
            .filter_map(|r| match r {
                ExtractedRef::EntitySlug(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(slugs, vec!["a", "b", "c"]);
    }

    #[test]
    fn ignores_refs_inside_code_fence() {
        let txt = "Real: [[entity:real]]\n\
                   ```\n\
                   Fake: [[entity:fake-1]] [[entity:fake-2]]\n\
                   ```\n\
                   Also real: [[entity:real-2]]";
        let refs = extract_refs(txt);
        let slugs: Vec<_> = refs
            .iter()
            .filter_map(|r| match r {
                ExtractedRef::EntitySlug(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(slugs, vec!["real", "real-2"]);
    }

    #[test]
    fn rejects_invalid_slug_characters() {
        // Capital letters, spaces, or punctuation in slug → no match.
        let refs = extract_refs("[[entity:Has Space]] [[entity:Caps]] [[entity:dot.com]]");
        assert!(refs.is_empty(), "got: {:?}", refs);
    }

    #[test]
    fn rejects_malformed_uuid() {
        // Wrong length, wrong segments → no match.
        let refs = extract_refs(
            "[[node:not-a-uuid]] [[node:12345678]] \
             [[node:00000000-0000-0000-0000-tooShortHex]]",
        );
        assert!(refs.is_empty(), "got: {:?}", refs);
    }

    #[test]
    fn tolerates_whitespace_inside_brackets() {
        let refs = extract_refs("[[  entity:  hello-world  ]]");
        assert_eq!(refs, vec![ExtractedRef::EntitySlug("hello-world".into())]);
    }

    #[test]
    fn returns_empty_for_text_with_no_refs() {
        assert!(extract_refs("Just plain markdown, nothing to extract.").is_empty());
        assert!(extract_refs("").is_empty());
    }

    #[test]
    fn key_helper_extracts_underlying_id() {
        assert_eq!(ExtractedRef::EntitySlug("a-slug".into()).key(), "a-slug");
        let uuid = "00000000-0000-0000-0000-000000000000";
        assert_eq!(ExtractedRef::NodeUuid(uuid.into()).key(), uuid);
        assert_eq!(
            ExtractedRef::MarkdownLink {
                dir: "entity".into(),
                slug: "x".into()
            }
            .key(),
            "x"
        );
    }

    // ─── infer_link_type ──────────────────────────────────────────

    #[test]
    fn reference_destination_always_yields_source() {
        // Highest-priority rule, regardless of source kind.
        for src in [
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::UserProfile,
            MemoryNodeKind::Episode,
            MemoryNodeKind::Procedure,
        ] {
            assert_eq!(
                infer_link_type(src, MemoryNodeKind::Reference, "anything goes"),
                MemoryRelationKind::Source
            );
        }
    }

    #[test]
    fn works_at_detected_for_entity_to_entity() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "She works at Acme on search infrastructure.",
        );
        assert_eq!(rk, MemoryRelationKind::WorksAt);
    }

    #[test]
    fn works_at_detected_chinese_cue() {
        let rk = infer_link_type(
            MemoryNodeKind::UserProfile,
            MemoryNodeKind::EntityPage,
            "他就职于 Acme,负责搜索基础设施。",
        );
        assert_eq!(rk, MemoryRelationKind::WorksAt);
    }

    #[test]
    fn founded_detected() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Acme was founded in 2020.",
        );
        assert_eq!(rk, MemoryRelationKind::Founded);
    }

    #[test]
    fn invested_in_detected() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Sequoia led the round and backed the company heavily.",
        );
        assert_eq!(rk, MemoryRelationKind::InvestedIn);
    }

    #[test]
    fn advises_detected() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Acts as an advisor to the early-stage team.",
        );
        assert_eq!(rk, MemoryRelationKind::Advises);
    }

    #[test]
    fn attended_detected_alumni_cue() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Alumni of Stanford CS.",
        );
        assert_eq!(rk, MemoryRelationKind::Attended);
    }

    #[test]
    fn falls_back_to_mentions_for_entity_to_entity_with_no_cue() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Saw them briefly at the meetup.",
        );
        assert_eq!(rk, MemoryRelationKind::Mentions);
    }

    #[test]
    fn unrelated_kind_pair_yields_mentions() {
        // Episode → EntityPage: no domain rule applies, fall through.
        let rk = infer_link_type(
            MemoryNodeKind::Episode,
            MemoryNodeKind::EntityPage,
            "She works at Acme — but we don't infer WorksAt from an Episode.",
        );
        assert_eq!(rk, MemoryRelationKind::Mentions);
    }

    #[test]
    fn case_insensitive_match() {
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "WORKS AT a stealth startup.",
        );
        assert_eq!(rk, MemoryRelationKind::WorksAt);
    }

    #[test]
    fn cue_order_specific_wins_over_general() {
        // "works at" sits in a sentence that also has "advisor" — but
        // works_at is the more specific employment cue and we list it
        // first in the chain, so it wins.
        let rk = infer_link_type(
            MemoryNodeKind::EntityPage,
            MemoryNodeKind::EntityPage,
            "Works at Acme, also serves as advisor elsewhere.",
        );
        assert_eq!(rk, MemoryRelationKind::WorksAt);
    }
}
