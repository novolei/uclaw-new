//! `MemoryAdapter` — uClaw's unified memory contract.
//!
//! PR1 of 阶段 4 (see
//! `docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`).
//! This PR introduces the trait + types only; concrete adapter impls
//! ship in subsequent PRs:
//!
//! - PR2: `LegacyKvAdapter` (wraps `crate::memory::MemoryStore`)
//! - PR3: `LegacyStewardAdapter` (wraps `crate::memory_graph::MemoryGraphStore`)
//! - PR9: `BucketSealAdapter` (new openhuman bucket-seal port)
//! - PR13: `GbrainAdapter` (wraps `mcp__gbrain__*`)
//! - PR14: `MemUAdapter` (wraps `MemUClient`)
//!
//! Until then, `AppState.memory_adapters` is an empty `HashMap`.

mod legacy_kv;
mod traits;
mod types;

pub use legacy_kv::LegacyKvAdapter;
pub use traits::MemoryAdapter;
pub use types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

#[cfg(test)]
mod tests;
