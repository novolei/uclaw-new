// SPDX-License-Identifier: Apache-2.0
//! Source-tree bucket-seal mechanics (openhuman port — Phase 3a).
//!
//! Lifts admitted chunks into a hierarchy of sealed summary nodes, one tree
//! per ingest source. Public surface at PR8:
//! - [`registry::get_or_create_source_tree`] — idempotent tree lookup
//! - [`bucket_seal::append_leaf`] — push a chunk into its tree, cascade-seal on budget
//! - [`summariser::inert::InertSummariser`] — deterministic fallback summariser
//!
//! Defers: `flush.rs` (time-based seal), `source_file.rs` (Obsidian vault output),
//! `summariser/llm.rs` (LLM-driven summariser, PR12+).

pub mod bucket_seal;
pub mod registry;
pub mod store;
pub mod summariser;
pub mod types;
