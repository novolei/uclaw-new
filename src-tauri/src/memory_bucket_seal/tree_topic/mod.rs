// SPDX-License-Identifier: Apache-2.0
//! Topic-tree bucket-seal mechanics (Phase 3c — openhuman port).
//!
//! Phase 3a's [`crate::memory_bucket_seal::tree_source`] subsystem is
//! already generic over [`crate::memory_bucket_seal::tree_source::types::TreeKind`].
//! Topic trees reuse the same store layer, the same cascade-seal pipeline,
//! and the same summariser/embedder injection. The only new bits are:
//! - [`registry::get_or_create_topic_tree`] — idempotent per-entity tree lookup
//!
//! Adapter integration sits in `BucketSealAdapter::store` — after a source
//! `append_leaf` succeeds for a chunk, entities are extracted and each
//! entity's topic tree gets its own `append_leaf` for the same `LeafRef`.

pub mod registry;

pub use registry::get_or_create_topic_tree;
