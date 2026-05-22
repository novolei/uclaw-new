//! `StructuredFold::to_markdown` — render the 8-field fold as a
//! single, LLM-readable Markdown block.
//!
//! The output is designed to slot in **as a user message body** in
//! place of N compacted turns, so when the next LLM call reads the
//! conversation, it sees a structured recap rather than a free-text
//! lossy summary.
//!
//! Format:
//!
//! ```text
//! ## Earlier conversation (compacted)
//!
//! ### Facts established
//! - Fact statement [evidence: artifact-id]
//!
//! ### Decisions made
//! - Decision (rationale: …; alternatives: …)
//!
//! ### Unresolved questions
//! - Open question
//!
//! ### Failed attempts (do not retry without changes)
//! - What was tried — why it failed
//!
//! ### Active constraints
//! - Constraint description
//!
//! ### Next actions
//! - Action item
//!
//! ### Rollback checkpoints
//! - checkpoint-id — note
//!
//! ### Evidence references
//! - artifact-id — label
//! ```
//!
//! Empty sections are omitted so the rendering stays tight. The
//! producer (LLM call → JSON → StructuredFold → markdown) should
//! ensure at least one section is populated; an empty fold renders
//! to a single line warning.

use super::fold::StructuredFold;
use super::fold_diff::FoldDelta;

impl StructuredFold {
    /// Render the fold as a Markdown block. Sections with no items
    /// are omitted entirely — never produces empty headings.
    ///
    /// Designed for direct substitution into the LLM message stream
    /// (place this string as the content of a `user`-role message
    /// that replaces the compacted history).
    pub fn to_markdown(&self) -> String {
        if self.is_empty() {
            return String::from(
                "## Earlier conversation (compacted)\n\n\
                 (no structured facts extracted — rely on the placeholder \
                 message for context boundary)",
            );
        }

        let mut out = String::with_capacity(1024);
        out.push_str("## Earlier conversation (compacted)\n");

        if !self.facts.is_empty() {
            out.push_str("\n### Facts established\n");
            for f in &self.facts {
                out.push_str("- ");
                out.push_str(&f.statement);
                if !f.evidence.is_empty() {
                    out.push_str(" _[evidence: ");
                    out.push_str(
                        &f.evidence
                            .iter()
                            .map(|a| a.label.clone().unwrap_or_else(|| a.id.clone()))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    out.push(']');
                    out.push('_');
                }
                if let Some(c) = f.confidence {
                    out.push_str(&format!(" _(conf={:.2})_", c));
                }
                out.push('\n');
            }
        }

        if !self.decisions.is_empty() {
            out.push_str("\n### Decisions made\n");
            for d in &self.decisions {
                out.push_str("- **");
                out.push_str(&d.decision);
                out.push_str("** — ");
                out.push_str(&d.rationale);
                if !d.alternatives_considered.is_empty() {
                    out.push_str(" _(alternatives weighed: ");
                    out.push_str(&d.alternatives_considered.join(", "));
                    out.push(')');
                    out.push('_');
                }
                out.push('\n');
            }
        }

        if !self.unresolved_questions.is_empty() {
            out.push_str("\n### Unresolved questions\n");
            for q in &self.unresolved_questions {
                out.push_str("- ");
                out.push_str(q);
                out.push('\n');
            }
        }

        if !self.failed_attempts.is_empty() {
            out.push_str("\n### Failed attempts (do not retry without changes)\n");
            for a in &self.failed_attempts {
                out.push_str("- **");
                out.push_str(&a.what_was_tried);
                out.push_str("** — failed because: ");
                out.push_str(&a.why_it_failed);
                out.push('\n');
            }
        }

        if !self.active_constraints.is_empty() {
            out.push_str("\n### Active constraints\n");
            for c in &self.active_constraints {
                out.push_str("- ");
                // Constraint has a kind field; we render its Debug shape
                // since the runtime::contracts::Constraint is the canonical
                // form and we want zero-copy renderability without coupling
                // to its internal field layout.
                out.push_str(&format!("{:?}", c));
                out.push('\n');
            }
        }

        if !self.next_actions.is_empty() {
            out.push_str("\n### Next actions\n");
            for n in &self.next_actions {
                out.push_str("- ");
                out.push_str(n);
                out.push('\n');
            }
        }

        if !self.rollback_points.is_empty() {
            out.push_str("\n### Rollback checkpoints\n");
            for r in &self.rollback_points {
                out.push_str("- `");
                out.push_str(&r.id);
                out.push('`');
                if let Some(note) = &r.note {
                    out.push_str(" — ");
                    out.push_str(note);
                }
                out.push('\n');
            }
        }

        if !self.evidence_refs.is_empty() {
            out.push_str("\n### Evidence references\n");
            for a in &self.evidence_refs {
                out.push_str("- `");
                out.push_str(&a.id);
                out.push('`');
                if let Some(label) = &a.label {
                    out.push_str(" — ");
                    out.push_str(label);
                }
                out.push('\n');
            }
        }

        out
    }
}

/// Bundle 17-B — render a `FoldDelta` as a compact LLM-facing block
/// that names what changed across the 8 StructuredFold axes since the
/// prior baseline.
///
/// The block is meant to be appended to the prior fold's `to_markdown`
/// output, NOT to replace it — together they form the delta-rendered
/// `/compact` placeholder. Result:
///
/// ```text
/// <prior_fold.to_markdown() — byte-stable, hits prompt cache>
///
/// <context_changes_since_last_fold>
/// + decisions: Use sqlx for V52 baseline storage
/// - decisions: Use rusqlite directly
/// ~ next_actions key="Wire dispatcher" prior:"…" now:"…"
/// </context_changes_since_last_fold>
/// ```
///
/// Per spec
/// [`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md`](../../../../docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md) §6.2,
/// the inner shape mirrors Bundle 16's
/// [`crate::agent::context_diff::render_delta_annotation`] so the LLM
/// sees a familiar pattern. We collapse the 8-axis structure into a
/// flat `[axis-name] item` list — loses per-axis visual grouping but
/// keeps a uniform parse shape and avoids per-axis case-splitting in
/// the M2-I cache placement policy.
///
/// Returns `None` when the delta is empty — caller should fall through
/// to the standard `prior_fold.to_markdown()` (no `changes` block to
/// emit) or, equivalently, not enter the delta-rendered path at all.
pub fn render_fold_delta_block(delta: &FoldDelta) -> Option<String> {
    if delta.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("<context_changes_since_last_fold baseline_hash=\"");
    out.push_str(&delta.baseline_hash);
    out.push_str("\">\n");

    // ── facts ─────────────────────────────────────────────────────
    for f in &delta.facts.added {
        out.push_str("+ [facts] ");
        out.push_str(&f.statement);
        out.push('\n');
    }
    for f in &delta.facts.removed {
        out.push_str("- [facts] ");
        out.push_str(&f.statement);
        out.push('\n');
    }
    for (prior, new) in &delta.facts.changed {
        out.push_str("~ [facts] ");
        out.push_str(&prior.statement);
        out.push_str("\n    prior: ");
        out.push_str(&prior.statement);
        out.push_str("\n    now:   ");
        out.push_str(&new.statement);
        out.push('\n');
    }

    // ── decisions ─────────────────────────────────────────────────
    for d in &delta.decisions.added {
        out.push_str("+ [decisions] ");
        out.push_str(&d.decision);
        out.push('\n');
    }
    for d in &delta.decisions.removed {
        out.push_str("- [decisions] ");
        out.push_str(&d.decision);
        out.push('\n');
    }
    for (prior, new) in &delta.decisions.changed {
        out.push_str("~ [decisions] ");
        out.push_str(&prior.decision);
        out.push_str("\n    prior rationale: ");
        out.push_str(&prior.rationale);
        out.push_str("\n    now   rationale: ");
        out.push_str(&new.rationale);
        out.push('\n');
    }

    // ── unresolved_questions (Vec<String>) ────────────────────────
    for q in &delta.unresolved_questions.added {
        out.push_str("+ [unresolved] ");
        out.push_str(q);
        out.push('\n');
    }
    for q in &delta.unresolved_questions.removed {
        out.push_str("- [unresolved] ");
        out.push_str(q);
        out.push('\n');
    }
    // String content_hash == stable_key, so changed is always empty for
    // this axis (see fold_diff::AxisItem impl for String). Skip.

    // ── evidence_refs ─────────────────────────────────────────────
    for e in &delta.evidence_refs.added {
        out.push_str("+ [evidence] ");
        out.push_str(&e.id);
        if let Some(l) = &e.label {
            out.push_str(" — ");
            out.push_str(l);
        }
        out.push('\n');
    }
    for e in &delta.evidence_refs.removed {
        out.push_str("- [evidence] ");
        out.push_str(&e.id);
        out.push('\n');
    }
    for (prior, new) in &delta.evidence_refs.changed {
        out.push_str("~ [evidence] ");
        out.push_str(&prior.id);
        out.push_str(" label: ");
        out.push_str(prior.label.as_deref().unwrap_or(""));
        out.push_str(" → ");
        out.push_str(new.label.as_deref().unwrap_or(""));
        out.push('\n');
    }

    // ── failed_attempts ───────────────────────────────────────────
    for a in &delta.failed_attempts.added {
        out.push_str("+ [failed_attempts] ");
        out.push_str(&a.what_was_tried);
        out.push_str(" — ");
        out.push_str(&a.why_it_failed);
        out.push('\n');
    }
    for a in &delta.failed_attempts.removed {
        out.push_str("- [failed_attempts] ");
        out.push_str(&a.what_was_tried);
        out.push('\n');
    }
    for (prior, new) in &delta.failed_attempts.changed {
        out.push_str("~ [failed_attempts] ");
        out.push_str(&prior.what_was_tried);
        out.push_str("\n    prior: ");
        out.push_str(&prior.why_it_failed);
        out.push_str("\n    now:   ");
        out.push_str(&new.why_it_failed);
        out.push('\n');
    }

    // ── active_constraints ────────────────────────────────────────
    for c in &delta.active_constraints.added {
        out.push_str("+ [constraint] ");
        out.push_str(&format!("{:?}", c));
        out.push('\n');
    }
    for c in &delta.active_constraints.removed {
        out.push_str("- [constraint] ");
        out.push_str(&format!("{:?}", c));
        out.push('\n');
    }
    for (prior, new) in &delta.active_constraints.changed {
        out.push_str("~ [constraint] ");
        out.push_str(&format!("{:?} → {:?}", prior, new));
        out.push('\n');
    }

    // ── next_actions (Vec<String>) ────────────────────────────────
    for n in &delta.next_actions.added {
        out.push_str("+ [next_actions] ");
        out.push_str(n);
        out.push('\n');
    }
    for n in &delta.next_actions.removed {
        out.push_str("- [next_actions] ");
        out.push_str(n);
        out.push('\n');
    }
    // String — no changed entries by construction.

    // ── rollback_points ───────────────────────────────────────────
    for r in &delta.rollback_points.added {
        out.push_str("+ [rollback] ");
        out.push_str(&r.id);
        if let Some(note) = &r.note {
            out.push_str(" — ");
            out.push_str(note);
        }
        out.push('\n');
    }
    for r in &delta.rollback_points.removed {
        out.push_str("- [rollback] ");
        out.push_str(&r.id);
        out.push('\n');
    }
    for (prior, new) in &delta.rollback_points.changed {
        out.push_str("~ [rollback] ");
        out.push_str(&prior.id);
        out.push_str(" note: ");
        out.push_str(prior.note.as_deref().unwrap_or(""));
        out.push_str(" → ");
        out.push_str(new.note.as_deref().unwrap_or(""));
        out.push('\n');
    }

    out.push_str("</context_changes_since_last_fold>");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::compact::fold::{
        ArtifactRef, CheckpointRef, DecisionWithRationale, FactWithEvidence, FailedAttempt,
    };

    #[test]
    fn empty_fold_renders_to_placeholder_text() {
        let out = StructuredFold::default().to_markdown();
        assert!(out.contains("Earlier conversation (compacted)"));
        assert!(out.contains("no structured facts extracted"));
    }

    #[test]
    fn fact_with_evidence_renders_with_bracketed_refs() {
        let fold = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "OAuth uses PKCE flow".into(),
            evidence: vec![
                ArtifactRef::labeled("rollout:1:5", "auth.rs"),
                ArtifactRef::new("rollout:1:7"),
            ],
            confidence: Some(0.95),
        }]);
        let out = fold.to_markdown();
        assert!(out.contains("### Facts established"));
        assert!(out.contains("OAuth uses PKCE flow"));
        assert!(out.contains("evidence: auth.rs, rollout:1:7"));
        assert!(out.contains("conf=0.95"));
    }

    #[test]
    fn decision_renders_decision_rationale_alternatives() {
        let fold = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Use Kafka not SQS".into(),
            rationale: "ordering guarantees".into(),
            alternatives_considered: vec!["SQS".into(), "RabbitMQ".into()],
            evidence: vec![],
        }]);
        let out = fold.to_markdown();
        assert!(out.contains("**Use Kafka not SQS** — ordering guarantees"));
        assert!(out.contains("alternatives weighed: SQS, RabbitMQ"));
    }

    #[test]
    fn failed_attempt_renders_what_and_why() {
        let fold = StructuredFold::default().with_failed_attempts(vec![FailedAttempt {
            what_was_tried: "increase batch size to 10k".into(),
            why_it_failed: "OOM on Kafka broker".into(),
            evidence: None,
        }]);
        let out = fold.to_markdown();
        assert!(out.contains("### Failed attempts (do not retry without changes)"));
        assert!(out.contains("**increase batch size to 10k** — failed because: OOM on Kafka broker"));
    }

    #[test]
    fn rollback_checkpoint_renders_id_and_note() {
        let fold = StructuredFold::default().with_rollback_points(vec![
            CheckpointRef::with_note("ckpt-001", "before db migration"),
            CheckpointRef::new("ckpt-002"),
        ]);
        let out = fold.to_markdown();
        assert!(out.contains("`ckpt-001` — before db migration"));
        assert!(out.contains("`ckpt-002`"));
    }

    #[test]
    fn empty_sections_are_omitted() {
        // Only facts populated — should not see headings for decisions / failed / etc.
        let fold = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "x".into(),
            evidence: vec![],
            confidence: None,
        }]);
        let out = fold.to_markdown();
        assert!(out.contains("### Facts established"));
        assert!(!out.contains("### Decisions made"));
        assert!(!out.contains("### Failed attempts"));
        assert!(!out.contains("### Unresolved questions"));
    }

    #[test]
    fn multi_section_fold_renders_in_expected_order() {
        let fold = StructuredFold::default()
            .with_facts(vec![FactWithEvidence {
                statement: "f1".into(),
                evidence: vec![],
                confidence: None,
            }])
            .with_decisions(vec![DecisionWithRationale {
                decision: "d1".into(),
                rationale: "r1".into(),
                alternatives_considered: vec![],
                evidence: vec![],
            }])
            .with_next_actions(vec!["a1".into(), "a2".into()]);
        let out = fold.to_markdown();
        // Facts come first, then decisions, then next_actions
        let i_facts = out.find("### Facts established").unwrap();
        let i_decisions = out.find("### Decisions made").unwrap();
        let i_actions = out.find("### Next actions").unwrap();
        assert!(i_facts < i_decisions);
        assert!(i_decisions < i_actions);
    }

    // ── Bundle 17-B — render_fold_delta_block ────────────────────────

    #[test]
    fn empty_delta_renders_none() {
        let a = StructuredFold::default();
        let delta = a.diff(&a);
        assert!(
            render_fold_delta_block(&delta).is_none(),
            "empty delta must not produce a block"
        );
    }

    #[test]
    fn added_fact_renders_with_axis_prefix_and_wrapper() {
        let prior = StructuredFold::default();
        let new = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "uClaw uses V52 baseline storage".into(),
            evidence: vec![],
            confidence: None,
        }]);
        let delta = prior.diff(&new);
        let out = render_fold_delta_block(&delta).expect("non-empty delta must render");

        assert!(out.starts_with("<context_changes_since_last_fold"));
        assert!(out.contains("baseline_hash="));
        assert!(out.contains("+ [facts] uClaw uses V52 baseline storage"));
        assert!(out.ends_with("</context_changes_since_last_fold>"));
    }

    #[test]
    fn removed_decision_renders_with_minus_prefix() {
        let prior = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Use rusqlite directly".into(),
            rationale: "shipped baseline".into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }]);
        let new = StructuredFold::default();
        let delta = prior.diff(&new);
        let out = render_fold_delta_block(&delta).unwrap();
        assert!(out.contains("- [decisions] Use rusqlite directly"));
    }

    #[test]
    fn changed_decision_renders_prior_and_now_rationale() {
        let prior = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Storage choice".into(),
            rationale: "rusqlite".into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }]);
        let new = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Storage choice".into(),
            rationale: "sqlx async-first".into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }]);
        let delta = prior.diff(&new);
        let out = render_fold_delta_block(&delta).unwrap();
        assert!(out.contains("~ [decisions] Storage choice"));
        assert!(out.contains("prior rationale: rusqlite"));
        assert!(out.contains("now   rationale: sqlx async-first"));
    }

    #[test]
    fn multi_axis_delta_emits_all_changes() {
        let prior = StructuredFold::default()
            .with_unresolved_questions(vec!["old question".into()])
            .with_next_actions(vec!["old action".into()]);
        let new = StructuredFold::default()
            .with_unresolved_questions(vec!["new question".into()])
            .with_next_actions(vec!["new action".into()])
            .with_rollback_points(vec![CheckpointRef::new("ckpt-X")]);
        let delta = prior.diff(&new);
        let out = render_fold_delta_block(&delta).unwrap();

        // Each axis surfaces with its [axis-name] tag.
        assert!(out.contains("+ [unresolved] new question"));
        assert!(out.contains("- [unresolved] old question"));
        assert!(out.contains("+ [next_actions] new action"));
        assert!(out.contains("- [next_actions] old action"));
        assert!(out.contains("+ [rollback] ckpt-X"));
    }

    #[test]
    fn baseline_hash_in_wrapper_matches_prior_hash() {
        // baseline_hash on the FoldDelta is `prior.baseline_hash()`. Render
        // must propagate that into the wrapper attribute so the LLM (and
        // log analysis) can verify the delta applies on top of the
        // expected prior.
        let prior = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "stable".into(),
            evidence: vec![],
            confidence: None,
        }]);
        let new = prior
            .clone()
            .with_next_actions(vec!["new action".into()]);
        let delta = prior.diff(&new);
        let out = render_fold_delta_block(&delta).unwrap();
        let expected = format!("baseline_hash=\"{}\"", prior.baseline_hash());
        assert!(
            out.contains(&expected),
            "render must propagate prior baseline_hash into wrapper attribute; got: {out}"
        );
    }
}
