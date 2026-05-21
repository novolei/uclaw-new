//! M2-D — Diff-based context re-injection.
//!
//! After M2-G `StructuredFold` lands, follow-up turns can re-inject
//! either:
//!
//! - the **full fold** (expensive: ~total_items() tokens), or
//! - a **diff** against the previous turn's fold baseline.
//!
//! M2-D produces the diff. The diff is itself a `StructuredFold`-
//! shaped delta: lists of added / removed / changed entries per axis.
//! When the diff is small (the common case for incremental work) it
//! costs a fraction of the full fold and the LLM reassembles the
//! current state from `(prior_fold + diff)`.
//!
//! This pilot ships:
//!
//! - **`FragmentSnapshot`** — minimal per-fragment digest (ref + hash
//!   of content + token estimate). The actual `ContextArtifact` from
//!   M2-C is too heavy for snapshot comparison.
//! - **`ContextDiff`** — added / removed / changed sets + an
//!   `unchanged_count` for stats.
//! - **`diff_snapshots`** — pure function: previous snapshot list +
//!   new snapshot list → `ContextDiff`.
//!
//! The fold-diff (StructuredFold-vs-StructuredFold) and the dispatcher
//! wire-up that decides "send fold or send diff" live in M2-D commit 2.
//!
//! Layout:
//!
//! - [`diff`] — `FragmentSnapshot`, `ContextDiff`, `diff_snapshots`
//! - [`line_snapshot`] (Bundle 16-A) — `LineFragmentSnapshot`,
//!   `LineDiff`, `line_diff`, `render_delta_annotation` — line-level
//!   diff used by M2-D Phase 2 Track A for cross-turn
//!   memory_context delta injection.

pub mod diff;
pub mod line_snapshot;

pub use diff::{
    diff_snapshots, ChangedFragment, ContextDiff, DiffStats, FragmentSnapshot,
};
pub use line_snapshot::{
    line_diff, render_delta_annotation, ChangedLine, LineDiff, LineDiffStats, LineEntry,
    LineFragmentSnapshot,
};
