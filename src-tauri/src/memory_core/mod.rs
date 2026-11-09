//! memory_core — a segmented BM25 index over graph facts and bucket_seal
//! chunks, so Chinese questions find their memories (T7 of the uclaw-pi
//! backport; plan docs/superpowers/plans/2026-11-09-memory-core-bm25-t7.md).

pub mod index;
pub mod segment;
