//! EntityPage metadata schema — the "compiled-truth + timeline" doctrine
//! for per-entity wiki pages, introduced by Memory OS Foundation Phase 1.
//!
//! Design ref: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` §4.2.2.
//!
//! ## Storage model — convention, not a new table
//!
//! An EntityPage is a regular row in `memory_nodes` with
//! `kind = MemoryNodeKind::EntityPage`. The page's `compiled_truth`
//! (the LLM-synthesized "what we currently know" markdown) lives in
//! `memory_versions.content` of the active version so that it participates
//! in FTS5 indexing automatically.
//!
//! Everything else — timeline (append-only evidence stream), aliases,
//! contradictions, slug, subkind, enrichment tier — is a JSON convention
//! on top of the existing `memory_nodes.metadata_json` column. **No schema
//! migration is required for the metadata itself**; only the V35 tables
//! created alongside Phase 1 (`memory_edge_audit`, `wiki_artifacts`,
//! `memory_health_findings`) need V35 to be applied.
//!
//! ## Forward-compatibility
//!
//! All fields default to empty / `None` and skip on serialize when empty,
//! so older rows (created before Phase 1) deserialize without error and
//! newer rows remain compatible with code that doesn't know the schema.

use serde::{Deserialize, Serialize};

/// Structured view of `memory_nodes.metadata_json` for `EntityPage` nodes.
///
/// Use [`EntityPageMetadata::from_value`] to decode any node's metadata
/// blob and [`EntityPageMetadata::into_value`] / [`EntityPageMetadata::to_value`]
/// to encode back. Both are total functions: malformed or partial JSON
/// degrades to [`Default::default`] rather than erroring, mirroring the
/// "tolerant reader" pattern uClaw uses everywhere else (see
/// `store.rs::row_to_node`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct EntityPageMetadata {
    /// Append-only stream of per-event entries about this entity. The
    /// `compiled_truth` is the upper layer; this is the lower layer.
    /// Sorted oldest-first by convention so callers can simply `push`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<TimelineEntry>,

    /// Display aliases — alternate spellings, abbreviations, native-name
    /// variants. The first entry, when present, is treated as the
    /// canonical slug source (see [`EntityPageMetadata::canonical_slug`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Flagged contradictions between sources or timeline entries. Lint
    /// (Phase 5) writes these; the UI surfaces them in the EntityPage card.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contradictions: Vec<Contradiction>,

    /// Stable, human-readable identifier. When `None`, the node UUID is
    /// the only handle; when `Some`, [`alias_resolver`](crate::memory_graph)
    /// (Phase 15) and the auto-link post-hook (Phase 2) can resolve
    /// `[[entity:<slug>]]` references against this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,

    /// Sub-category within EntityPage: `"entity"`, `"concept"`, etc.
    /// Cognitive Layer Phase 8 promotes this to a typed enum
    /// ([`WikiSubkind`](super)); Foundation Phase 1 stores the raw string
    /// to keep the metadata forward-compatible without a code change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subkind: Option<String>,

    /// Enrichment tier (1 = full, 2 = rich, 3 = stub). Tier escalator
    /// scenario (Phase 6) maintains this based on mention frequency.
    /// Default 3 (stub) for newly created pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrichment_tier: Option<u8>,

    /// ISO 8601 timestamp of the last LLM-driven re-synthesis of the
    /// compiled_truth. Used by Lint to flag stale entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synthesized_at: Option<String>,

    /// How many source documents informed the most recent synthesis.
    /// Surfaced in the UI as a confidence signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesis_source_count: Option<u32>,
}

/// One append-only event in an EntityPage's timeline.
///
/// Each entry corresponds to a moment in time when this entity was
/// observed or interacted with. Cognitive Phase 10 enriches this with
/// segment-level provenance; Foundation Phase 1 keeps it minimal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TimelineEntry {
    /// Day of the event, ISO 8601 date (`YYYY-MM-DD`). Days are coarse
    /// enough to keep the timeline scannable without overwhelming the UI.
    pub date: String,

    /// Free-form summary of what happened. Typically 1-2 sentences.
    pub text: String,

    /// Optional pointer back to the originating memory node (e.g. the
    /// Episode or Reference that produced this entry).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,

    /// Optional pointer back to the agent session in which this entry
    /// was created. Lets the UI deep-link to the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_session_id: Option<String>,
}

/// A noted contradiction between two or more claims about this entity.
///
/// Lint (Phase 5) writes these when two timeline entries or sources
/// disagree. The page status moves to `disputed` automatically once a
/// contradiction is present; resolving it (via the review queue) clears
/// the entry and reverts the status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Contradiction {
    /// `memory_nodes.id`s of the source nodes whose claims disagree.
    /// Length must be `>= 2` to be meaningful.
    pub between_source_ids: Vec<String>,

    /// Short paraphrase of the first conflicting claim.
    pub claim_a: String,

    /// Short paraphrase of the second conflicting claim.
    pub claim_b: String,

    /// ISO 8601 timestamp recording when Lint first noticed the issue.
    pub noticed_at: String,
}

impl EntityPageMetadata {
    /// Decode metadata from a raw `serde_json::Value`. Any failure
    /// (missing fields, type mismatches, malformed shapes) yields
    /// [`Default::default`] so callers never need to special-case
    /// pre-Phase-1 rows that lack the schema.
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// Decode metadata from the `Option<serde_json::Value>` shape used
    /// on [`super::models::MemoryNode::metadata`]. `None` yields the
    /// default; malformed values also yield the default (see
    /// [`EntityPageMetadata::from_value`]).
    pub fn from_optional(value: &Option<serde_json::Value>) -> Self {
        value
            .as_ref()
            .map(Self::from_value)
            .unwrap_or_default()
    }

    /// Encode metadata to a `serde_json::Value`. Empty / `None` fields
    /// are omitted thanks to the `skip_serializing_if` attributes, so
    /// callers writing back to the DB produce compact JSON.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Consume `self` and produce the encoded value. Same semantics as
    /// [`EntityPageMetadata::to_value`] but avoids the borrow when the
    /// caller is done with the struct.
    pub fn into_value(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Best-effort canonical slug. Returns the explicit [`slug`](Self::slug)
    /// if present, else the first alias, else `None`.
    pub fn canonical_slug(&self) -> Option<&str> {
        self.slug
            .as_deref()
            .or_else(|| self.aliases.first().map(String::as_str))
    }

    /// Append a timeline entry. Timeline rows are append-only by
    /// convention; callers that want to mutate must reconstruct the
    /// vector themselves and call [`EntityPageMetadata::to_value`].
    pub fn push_timeline(&mut self, entry: TimelineEntry) {
        self.timeline.push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_metadata_round_trips_to_empty_object() {
        let meta = EntityPageMetadata::default();
        let value = meta.to_value();
        // All fields skip_serializing_if when empty/None -> output is "{}"
        assert_eq!(value, json!({}));
        let decoded = EntityPageMetadata::from_value(&value);
        assert_eq!(decoded, EntityPageMetadata::default());
    }

    #[test]
    fn from_value_tolerates_unknown_fields() {
        let raw = json!({
            "slug": "zhang-san",
            "aliases": ["张三", "Zhang San"],
            "some_future_field_added_in_phase_42": "ignored"
        });
        let meta = EntityPageMetadata::from_value(&raw);
        assert_eq!(meta.slug.as_deref(), Some("zhang-san"));
        assert_eq!(meta.aliases.len(), 2);
        // No panic, no error — unknown fields silently dropped.
    }

    #[test]
    fn from_value_tolerates_malformed_input() {
        // String where object is expected -> default fallback.
        let raw = json!("not an object");
        let meta = EntityPageMetadata::from_value(&raw);
        assert_eq!(meta, EntityPageMetadata::default());
    }

    #[test]
    fn from_optional_handles_none_and_some() {
        assert_eq!(
            EntityPageMetadata::from_optional(&None),
            EntityPageMetadata::default()
        );
        let some = Some(json!({"slug": "x"}));
        let meta = EntityPageMetadata::from_optional(&some);
        assert_eq!(meta.slug.as_deref(), Some("x"));
    }

    #[test]
    fn timeline_round_trip_preserves_order() {
        let mut meta = EntityPageMetadata {
            slug: Some("acme".into()),
            ..Default::default()
        };
        meta.push_timeline(TimelineEntry {
            date: "2026-05-01".into(),
            text: "First mention".into(),
            source_node_id: Some("src-a".into()),
            source_session_id: None,
        });
        meta.push_timeline(TimelineEntry {
            date: "2026-05-15".into(),
            text: "Follow-up".into(),
            source_node_id: Some("src-b".into()),
            source_session_id: Some("sess-1".into()),
        });

        let encoded = meta.clone().into_value();
        let decoded = EntityPageMetadata::from_value(&encoded);
        assert_eq!(decoded.timeline.len(), 2);
        assert_eq!(decoded.timeline[0].date, "2026-05-01");
        assert_eq!(decoded.timeline[1].source_session_id.as_deref(), Some("sess-1"));
        // Round-trip identity (modulo skipped empty fields).
        assert_eq!(decoded, meta);
    }

    #[test]
    fn contradictions_round_trip() {
        let meta = EntityPageMetadata {
            slug: Some("alice".into()),
            contradictions: vec![Contradiction {
                between_source_ids: vec!["a".into(), "b".into()],
                claim_a: "works at Acme".into(),
                claim_b: "works at Beta".into(),
                noticed_at: "2026-05-18T03:00:00Z".into(),
            }],
            ..Default::default()
        };
        let v = meta.clone().into_value();
        // Make sure the field is serialized (not skipped) when non-empty.
        assert!(v.get("contradictions").is_some());
        assert_eq!(EntityPageMetadata::from_value(&v), meta);
    }

    #[test]
    fn canonical_slug_prefers_explicit_then_first_alias() {
        let a = EntityPageMetadata {
            slug: Some("explicit".into()),
            aliases: vec!["fallback".into()],
            ..Default::default()
        };
        assert_eq!(a.canonical_slug(), Some("explicit"));

        let b = EntityPageMetadata {
            slug: None,
            aliases: vec!["from-alias".into(), "another".into()],
            ..Default::default()
        };
        assert_eq!(b.canonical_slug(), Some("from-alias"));

        let c = EntityPageMetadata::default();
        assert_eq!(c.canonical_slug(), None);
    }

    #[test]
    fn enrichment_tier_round_trips_when_set() {
        let meta = EntityPageMetadata {
            enrichment_tier: Some(2),
            synthesis_source_count: Some(7),
            last_synthesized_at: Some("2026-05-18T10:30:00Z".into()),
            ..Default::default()
        };
        let v = meta.clone().into_value();
        let decoded = EntityPageMetadata::from_value(&v);
        assert_eq!(decoded.enrichment_tier, Some(2));
        assert_eq!(decoded.synthesis_source_count, Some(7));
    }

    #[test]
    fn legacy_node_with_unrelated_metadata_decodes_to_default_safely() {
        // A pre-Phase-1 EntityPage doesn't exist (the kind is new), but a
        // future Phase-1 reader might see a different node kind whose
        // metadata happens to share the JSON column. The decoder must not
        // crash or pollute fields.
        let raw = json!({
            "skill_type": "learned",
            "enabled": true,
            "cited_count": 12,
            "lifecycle": "promoted"
        });
        let meta = EntityPageMetadata::from_value(&raw);
        // None of the known EntityPage fields appear -> default.
        assert!(meta.timeline.is_empty());
        assert!(meta.aliases.is_empty());
        assert!(meta.slug.is_none());
    }
}
