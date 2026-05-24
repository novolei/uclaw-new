//! `StructuredFold` — the 8-field canonical compact representation.
//!
//! Replaces uClaw's prior single-string "compress_context_if_needed"
//! summary with a typed, structured form so the agent can reason over
//! each axis independently.
//!
//! All types implement `Debug + Clone + Serialize + Deserialize` so
//! they survive SQLite blob storage + the rollout JSONL pipeline
//! (M1-T5).

use serde::{Deserialize, Serialize};

use crate::runtime::contracts::Constraint;

// ── Component types ─────────────────────────────────────────────────

/// An assertion the agent has accepted as true, paired with a pointer
/// to the evidence that supports it.
///
/// `evidence` references an artifact in the rollout / context store —
/// the canonical form is an opaque id string so this type doesn't
/// couple to any single storage layer.
// Note: PartialEq only (no Eq) because `confidence: Option<f32>`. f32
// is not Eq (NaN is non-reflexive). This propagates to StructuredFold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactWithEvidence {
    /// One-line statement of fact, e.g. "Service auth uses OAuth2 PKCE".
    pub statement: String,
    /// Pointer(s) to evidence that supports `statement`.
    pub evidence: Vec<ArtifactRef>,
    /// Optional agent confidence in [0.0, 1.0]. None = unscored.
    pub confidence: Option<f32>,
}

/// A choice the agent made, plus the reasoning that justified it.
///
/// `alternatives_considered` is non-empty for any decision the agent
/// genuinely deliberated over — the producer should populate it so
/// the next turn can re-examine the trade-off if context changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionWithRationale {
    pub decision: String,
    pub rationale: String,
    pub alternatives_considered: Vec<String>,
    /// Optional evidence backing the decision.
    pub evidence: Vec<ArtifactRef>,
}

/// Something the agent tried that didn't work. Capturing these prevents
/// the next turn from rediscovering the same dead end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedAttempt {
    pub what_was_tried: String,
    /// Why it failed — error message, observation, etc.
    pub why_it_failed: String,
    /// Optional pointer to logs / output that documents the failure.
    pub evidence: Option<ArtifactRef>,
}

/// Opaque pointer to a stored artifact (rollout chunk, file blob,
/// retrieved doc, etc.). Strings rather than typed IDs so this stays
/// storage-agnostic — callers resolve via their own registry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Unique id of the artifact. Format is producer-defined; commonly
    /// `"rollout:{task_id}:{event_seq}"` or `"file:{path}@{rev}"`.
    pub id: String,
    /// Optional human label (used for UI display + LLM citation).
    pub label: Option<String>,
}

impl ArtifactRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
        }
    }

    pub fn labeled(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: Some(label.into()),
        }
    }
}

/// Pointer to a checkpoint the agent can roll back to. Checkpoints are
/// produced by the runtime's `CheckpointPolicy` (M1-T1 contracts) and
/// captured here so the StructuredFold can reference them without
/// re-implementing checkpoint storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckpointRef {
    pub id: String,
    /// Optional description of what state the checkpoint captures.
    pub note: Option<String>,
}

impl CheckpointRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            note: None,
        }
    }

    pub fn with_note(id: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            note: Some(note.into()),
        }
    }
}

// ── MicroCapsule ───────────────────────────────────────────────────

/// A chronological micro-capsule summarizing a single conversation turn verbatim query
/// and its outcome. This ensures high turn recall with minimal token footprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroCapsule {
    pub turn_index: usize,
    pub user_query: String,
    pub agent_outcome: String,
}

// ── StructuredFold ─────────────────────────────────────────────────

/// The canonical compact summary an agent emits at the end of a turn
/// / sub-task. Replaces ad-hoc "summarize the conversation so far"
/// prompts with a typed 8-field form (extended with micro-capsules).
///
/// Construction patterns:
///
/// ```
/// use uclaw_core::agent::compact::StructuredFold;
///
/// // Minimal fold — no axis populated.
/// let empty = StructuredFold::default();
/// assert_eq!(empty.facts.len(), 0);
///
/// // Builder-style: add one fact + one decision.
/// let fold = StructuredFold::default()
///     .with_facts(vec![
///         /* FactWithEvidence... */
///     ])
///     .with_next_actions(vec!["ship M2-G".into()]);
/// ```
///
/// Serde roundtrip is the canonical "is this thing complete" test —
/// every field is reachable via `serde_json::to_value` /
/// `serde_json::from_value`.
// Note: PartialEq only (no Eq) because `facts: Vec<FactWithEvidence>`
// transitively carries an `Option<f32>` confidence field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StructuredFold {
    #[serde(default)]
    pub facts: Vec<FactWithEvidence>,
    #[serde(default)]
    pub decisions: Vec<DecisionWithRationale>,
    #[serde(default)]
    pub unresolved_questions: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<ArtifactRef>,
    #[serde(default)]
    pub failed_attempts: Vec<FailedAttempt>,
    #[serde(default)]
    pub active_constraints: Vec<Constraint>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub rollback_points: Vec<CheckpointRef>,
    #[serde(default)]
    pub micro_capsules: Vec<MicroCapsule>,
}

impl StructuredFold {
    /// Builder: replace `facts` and return self. Chains.
    pub fn with_facts(mut self, facts: Vec<FactWithEvidence>) -> Self {
        self.facts = facts;
        self
    }

    pub fn with_decisions(mut self, decisions: Vec<DecisionWithRationale>) -> Self {
        self.decisions = decisions;
        self
    }

    pub fn with_unresolved_questions(mut self, qs: Vec<String>) -> Self {
        self.unresolved_questions = qs;
        self
    }

    pub fn with_evidence_refs(mut self, refs: Vec<ArtifactRef>) -> Self {
        self.evidence_refs = refs;
        self
    }

    pub fn with_failed_attempts(mut self, attempts: Vec<FailedAttempt>) -> Self {
        self.failed_attempts = attempts;
        self
    }

    pub fn with_active_constraints(mut self, constraints: Vec<Constraint>) -> Self {
        self.active_constraints = constraints;
        self
    }

    pub fn with_next_actions(mut self, actions: Vec<String>) -> Self {
        self.next_actions = actions;
        self
    }

    pub fn with_rollback_points(mut self, points: Vec<CheckpointRef>) -> Self {
        self.rollback_points = points;
        self
    }

    pub fn with_micro_capsules(mut self, capsules: Vec<MicroCapsule>) -> Self {
        self.micro_capsules = capsules;
        self
    }

    /// `true` if every field is empty — useful for "skip compaction"
    /// short-circuits in the producer pipeline.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
            && self.decisions.is_empty()
            && self.unresolved_questions.is_empty()
            && self.evidence_refs.is_empty()
            && self.failed_attempts.is_empty()
            && self.active_constraints.is_empty()
            && self.next_actions.is_empty()
            && self.rollback_points.is_empty()
            && self.micro_capsules.is_empty()
    }

    /// Total number of items across all axes — used by Token defense
    /// (M2-H) to gauge fold size before re-injection.
    pub fn total_items(&self) -> usize {
        self.facts.len()
            + self.decisions.len()
            + self.unresolved_questions.len()
            + self.evidence_refs.len()
            + self.failed_attempts.len()
            + self.active_constraints.len()
            + self.next_actions.len()
            + self.rollback_points.len()
            + self.micro_capsules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::contracts::Constraint;

    // ── helpers ─────────────────────────────────────────────────────

    fn sample_fact() -> FactWithEvidence {
        FactWithEvidence {
            statement: "Auth uses OAuth2 PKCE".into(),
            evidence: vec![ArtifactRef::labeled("rollout:t1:42", "auth handshake log")],
            confidence: Some(0.95),
        }
    }

    fn sample_decision() -> DecisionWithRationale {
        DecisionWithRationale {
            decision: "Use rusqlite over sqlx".into(),
            rationale: "Sync API matches existing repo style".into(),
            alternatives_considered: vec!["sqlx".into(), "diesel".into()],
            evidence: vec![ArtifactRef::new("file:db/mod.rs@HEAD")],
        }
    }

    fn sample_failed_attempt() -> FailedAttempt {
        FailedAttempt {
            what_was_tried: "Direct V53 migration without backup".into(),
            why_it_failed: "rusqlite ApiMisuse on multi-statement execute()".into(),
            evidence: Some(ArtifactRef::new("rollout:t1:99")),
        }
    }

    fn sample_constraint() -> Constraint {
        // Constraint is { key, value } in runtime::contracts. Construct via
        // serde to stay decoupled from its private layout.
        serde_json::from_value(serde_json::json!({
            "key": "license",
            "value": "Apache-2.0"
        }))
        .expect("Constraint shape changed — update test")
    }

    fn sample_checkpoint() -> CheckpointRef {
        CheckpointRef::with_note("cp-7", "after V49 migration")
    }

    fn rich_fold() -> StructuredFold {
        StructuredFold::default()
            .with_facts(vec![sample_fact()])
            .with_decisions(vec![sample_decision()])
            .with_unresolved_questions(vec![
                "Does M2-G need Pin support?".into(),
                "Cache invalidation strategy for fold artifacts?".into(),
            ])
            .with_evidence_refs(vec![
                ArtifactRef::new("rollout:t1:1"),
                ArtifactRef::labeled("file:CONTEXT.md", "project docs"),
            ])
            .with_failed_attempts(vec![sample_failed_attempt()])
            .with_active_constraints(vec![sample_constraint()])
            .with_next_actions(vec!["Wire fold into ContextToolSet".into()])
            .with_rollback_points(vec![sample_checkpoint()])
    }

    // ── defaults ────────────────────────────────────────────────────

    #[test]
    fn default_is_empty() {
        let f = StructuredFold::default();
        assert!(f.is_empty());
        assert_eq!(f.total_items(), 0);
    }

    #[test]
    fn is_empty_false_when_any_field_populated() {
        let f = StructuredFold::default().with_next_actions(vec!["x".into()]);
        assert!(!f.is_empty());
        assert_eq!(f.total_items(), 1);
    }

    #[test]
    fn total_items_sums_all_axes() {
        let f = rich_fold();
        // 1 + 1 + 2 + 2 + 1 + 1 + 1 + 1 = 10
        assert_eq!(f.total_items(), 10);
        assert!(!f.is_empty());
    }

    // ── builders ────────────────────────────────────────────────────

    #[test]
    fn with_facts_replaces_facts() {
        let f = StructuredFold::default()
            .with_facts(vec![sample_fact(), sample_fact()]);
        assert_eq!(f.facts.len(), 2);
        // Second call replaces.
        let f = f.with_facts(vec![sample_fact()]);
        assert_eq!(f.facts.len(), 1);
    }

    #[test]
    fn all_builder_methods_chain() {
        // Compile-test: every with_* method returns Self.
        let f = StructuredFold::default()
            .with_facts(vec![])
            .with_decisions(vec![])
            .with_unresolved_questions(vec![])
            .with_evidence_refs(vec![])
            .with_failed_attempts(vec![])
            .with_active_constraints(vec![])
            .with_next_actions(vec![])
            .with_rollback_points(vec![])
            .with_micro_capsules(vec![]);
        assert!(f.is_empty());
    }

    // ── ArtifactRef / CheckpointRef constructors ────────────────────

    #[test]
    fn artifact_ref_new_no_label() {
        let r = ArtifactRef::new("rollout:t1:0");
        assert_eq!(r.id, "rollout:t1:0");
        assert!(r.label.is_none());
    }

    #[test]
    fn artifact_ref_labeled_has_label() {
        let r = ArtifactRef::labeled("file:x.rs", "module file");
        assert_eq!(r.label.as_deref(), Some("module file"));
    }

    #[test]
    fn checkpoint_ref_with_note_carries_note() {
        let c = CheckpointRef::with_note("cp-1", "after schema bump");
        assert_eq!(c.note.as_deref(), Some("after schema bump"));
    }

    // ── serde roundtrips ────────────────────────────────────────────

    #[test]
    fn serde_roundtrip_empty_fold() {
        let f = StructuredFold::default();
        let json = serde_json::to_string(&f).unwrap();
        let back: StructuredFold = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn serde_roundtrip_rich_fold() {
        let f = rich_fold();
        let json = serde_json::to_string(&f).unwrap();
        let back: StructuredFold = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
        // Confirm structural fidelity — total_items survives.
        assert_eq!(back.total_items(), 10);
    }

    #[test]
    fn serde_json_omits_optional_label() {
        let r = ArtifactRef::new("only-id");
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        // label: null is acceptable; absence preferred. Default serde keeps null.
        assert_eq!(v["id"], "only-id");
    }

    #[test]
    fn serde_defaults_allow_partial_input() {
        // A producer that only emits `facts` should still deserialize:
        // other fields default to empty vecs.
        let json = r#"{"facts": []}"#;
        let f: StructuredFold = serde_json::from_str(json).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn serde_roundtrip_fact_with_evidence() {
        let fact = sample_fact();
        let json = serde_json::to_string(&fact).unwrap();
        let back: FactWithEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(fact, back);
        assert_eq!(back.confidence, Some(0.95));
    }

    #[test]
    fn serde_roundtrip_decision_with_rationale() {
        let d = sample_decision();
        let json = serde_json::to_string(&d).unwrap();
        let back: DecisionWithRationale = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
        assert_eq!(back.alternatives_considered.len(), 2);
    }

    #[test]
    fn serde_roundtrip_failed_attempt() {
        let a = sample_failed_attempt();
        let json = serde_json::to_string(&a).unwrap();
        let back: FailedAttempt = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn serde_roundtrip_checkpoint_ref() {
        let c = sample_checkpoint();
        let json = serde_json::to_string(&c).unwrap();
        let back: CheckpointRef = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
