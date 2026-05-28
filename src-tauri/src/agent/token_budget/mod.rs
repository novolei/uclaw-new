//! M2-J — Token budget snapshot (UI backend contract).
//!
//! The M2-J UI is the "where's my token budget going?" view. It needs
//! a single typed payload that aggregates every place tokens are
//! spent / saved across one LLM turn:
//!
//! - **Baseline** tokens (M2-A) — fixed cost per turn
//! - **Skill metadata** — emitted via `skills_manifest::build_skills_manifest`
//!   (M2-H L3 token-budget top-K selection was deleted in P1 cleanup as
//!   never-wired; budget-aware selection is deferred to 阶段 3 Pi convergence)
//! - **Context fragments** (M2-B) — injected per turn
//! - **Conversation** — message history
//! - **Tool output** truncations (M2-H L1)
//! - **Image stripping** savings (M2-H L5)
//! - **Tool schema** normalization savings (M2-H L2)
//! - **Orphan call** synthesis count (M2-H L6)
//! - **Compaction** state (M2-H L7)
//! - **Cache breakpoints** placed (M2-I)
//!
//! `TokenBudgetSnapshot` is a flat, serde-friendly struct the
//! backend assembles once per turn (or per-frame for live updates)
//! and pushes to the UI as a Tauri event payload.
//!
//! This pilot ships the **type definitions + builder + computed
//! totals**. The collector that walks each L-layer's stats and
//! assembles a snapshot lives in M2-J commit 2 (next to the
//! ContextManager).
//!
//! Layout:
//!
//! - [`snapshot`] — `TokenBudgetSnapshot` + sub-records + builders

pub mod snapshot;

pub use snapshot::{
    BreakpointSummary, ContextSegmentBreakdown, DefenseSavings, TokenBudgetSnapshot,
};
