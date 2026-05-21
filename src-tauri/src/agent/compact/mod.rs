//! M2-G — Structured compact / fold types.
//!
//! When a turn or sub-task ends, the agent often needs to collapse a
//! large trace of intermediate work into a small, structured artifact
//! that can be re-injected into the next turn's context window.
//!
//! Per ADR §"Context Fabric" the canonical compact form is a
//! **StructuredFold** with 8 fields:
//!
//! | Field | Meaning |
//! |---|---|
//! | `facts` | observed truths with evidence pointers |
//! | `decisions` | choices made + rationale |
//! | `unresolved_questions` | open questions to revisit |
//! | `evidence_refs` | pointers to artifacts that support the above |
//! | `failed_attempts` | what was tried + why it didn't work |
//! | `active_constraints` | constraints still binding in next turn |
//! | `next_actions` | planned next steps |
//! | `rollback_points` | checkpoints we can return to |
//!
//! This module ships the **types only** (M2-G pilot) — serialization,
//! roundtrip tests, and the convenience constructors. The fold
//! **producer** (LLM call + parser that turns a turn trace into a
//! StructuredFold) lands in a follow-up PR, as does the wire-up that
//! replaces `runtime::context_tools::ContextToolSet::fold/compare`
//! Storage("M2-G") stubs.
//!
//! Layout:
//!
//! - [`fold`] — `StructuredFold` + 8 component types + serde roundtrip tests

pub mod fold;
pub mod render;
pub mod summarize;

pub use fold::{
    ArtifactRef, CheckpointRef, DecisionWithRationale, FactWithEvidence, FailedAttempt,
    StructuredFold,
};
pub use summarize::{summarize_to_fold, SummarizeError};
