//! M6-T1 — Memory graph contract types (pilot).
//!
//! Sits beside the existing `memory_graph` module (which is the
//! concrete implementation today). This pilot ships the **typed
//! contract** the agent kernel relies on — a thin layer separating
//! agent code from gbrain-specific shapes so we can swap or A/B test
//! memory backends later (M6-T2 = SurrealDB option, M6-T3 = vector-
//! only fallback).
//!
//! Types:
//!
//! - `MemoryNamespace` — coarse partition (user_facts / project_notes /
//!   conversation / scratch / ...).
//! - `MemoryNodeKind` — entity / fact / decision / preference /
//!   custom(string).
//! - `MemoryNode` — opaque id + kind + namespace + body + tags +
//!   timestamps.
//! - `MemoryEdgeKind` — relates / contradicts / supersedes / mentions /
//!   custom(string).
//! - `MemoryEdge` — source/target node ids + kind + optional weight.
//! - `MemoryQuery` — text + filters (namespace, kind, tags).
//! - `MemoryQueryResult` — ranked hits with relevance scores.
//! - `MemoryAdapter` async trait — write / query / delete.
//!
//! Concrete adapter (`GbrainAdapter`) lives in M6-T1 commit 2.
//!
//! Layout:
//!
//! - [`types`] — the type definitions
//! - [`adapter`] — `MemoryAdapter` trait + error type

pub mod adapter;
pub mod types;

pub use adapter::{MemoryAdapter, MemoryAdapterError};
pub use types::{
    MemoryEdge, MemoryEdgeKind, MemoryHit, MemoryNamespace, MemoryNode, MemoryNodeKind,
    MemoryQuery, MemoryQueryResult,
};
