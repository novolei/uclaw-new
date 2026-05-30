// SPDX-License-Identifier: Apache-2.0
//! Source-tree bucket-seal mechanics (openhuman port — Phase 3a).
//!
//! Public surface at PR8:
//! - [`registry::get_or_create_source_tree`] — idempotent tree lookup
//! - [`bucket_seal::append_leaf`] — push a chunk into its tree, cascade-seal on budget
//! - [`summariser::inert::InertSummariser`] — deterministic fallback summariser
//!
//! Deferred to follow-up PRs:
//! - `flush.rs` (time-based stale buffer seal)
//! - `source_file.rs` (Obsidian vault .md writer for trees)
//! - `summariser/llm.rs` (LLM-driven summariser, PR12)

pub mod bucket_seal;
pub mod registry;
pub mod store;
pub mod summariser;
pub mod types;

pub use bucket_seal::{append_leaf, append_leaf_deferred, cascade_all_from, LabelStrategy, LeafRef};
pub use registry::get_or_create_source_tree;
pub use store::{get_summary_embedding, set_summary_embedding};
pub use summariser::{
    build_summariser, inert::InertSummariser, LlmSummariser, Summariser,
};
pub use types::{
    Buffer, SummaryNode, Tree, TreeKind, TreeStatus, INPUT_TOKEN_BUDGET, OUTPUT_TOKEN_BUDGET,
    SUMMARY_FANOUT,
};
