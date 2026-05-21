//! Slice 1 — runtime telemetry collector.
//!
//! Bridges the agent-loop usage hook to the M2-J `TokenBudgetSnapshot`
//! UI contract. Per turn, the agent loop calls
//! `TokenBudgetCollector::record_turn(...)` after `delegate.on_usage()`
//! fires; the collector keeps the latest snapshot per `task_id` in
//! memory so the Tauri `get_latest_token_budget(task_id)` command can
//! return it on demand.
//!
//! Why per-task in-memory only (not SQLite-mirrored yet):
//! - Token budget is fundamentally ephemeral per-turn — historical
//!   snapshots are captured by the M1-T5 rollout JSONL writer
//! - UI subscription wants "what's the latest snapshot for this active
//!   session?" — in-memory is the right cache
//! - SQLite mirror is a Slice 1 follow-up if needed for cross-restart
//!   inspection
//!
//! Layout:
//!
//! - [`collector`] — `TokenBudgetCollector` per-task latest snapshot

pub mod collector;

pub use collector::TokenBudgetCollector;
