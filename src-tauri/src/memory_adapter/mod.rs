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
//! - PR17: `MemUAdapter` (wraps `MemUClient`)
//!
//! Until then, `AppState.memory_adapters` is an empty `HashMap`.
//!
//! ## Backend roster (end-state, 2026-05-31)
//! - `bucket_seal` — **canonical default** (openhuman bucket-seal port); `default_memory_backend`.
//! - `gbrain` — retained: chat/MCP recall surface.
//! - `memu` — retained: item-based memory (memU bridge).
//! - `legacy_kv` / `legacy_steward` — **deprecated**; reachable only by explicit
//!   `legacy_kv:`/`legacy_steward:` namespace prefix. Data migration + removal
//!   deferred. Do not route new writes here.
//! - Freeze exemptions: proactive `tool_memory` (co-used graph) + `skill_parser` (versioned skill store) still write memory_graph by design (deferred effort).

pub mod edges;
pub mod gbrain;
pub mod gbrain_page_migration;
pub mod page_dual_write;
pub mod skills;
mod legacy_kv;
mod legacy_steward;
pub mod memu;
pub mod pages;
mod router;
mod traits;
mod types;

pub use gbrain::GbrainAdapter;
pub use legacy_kv::LegacyKvAdapter;
pub use legacy_steward::LegacyStewardAdapter;
pub use memu::MemUAdapter;
pub use router::{
    format_entries, load_context, merge_dedupe_budget, resolve_backend, resolve_backend_in,
    route_recall, route_recall_in, split_namespace_prefix, RecallOptsIpc, ResolvedBackend,
};
pub use edges::{relate, neighbors, Edge};
pub use pages::{get_page, put_page, search_pages, Page, PageHit};
pub use skills::{Skill, put_skill, get_skill, top_skills, bump_cited};
pub use traits::MemoryAdapter;
pub use types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

#[cfg(test)]
mod tests;
