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
}
