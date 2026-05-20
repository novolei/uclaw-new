//! M3-T3 — Worker / sub-agent type skeleton (pilot).
//!
//! Teams orchestration spawns sub-agents (workers) that handle bounded
//! sub-tasks under a coordinator. Each worker has:
//!
//! - A typed **role** (`WorkerRole`) telling the LLM what its job is
//!   (researcher / reviewer / implementor / synthesizer / monitor).
//! - A **status** indicating where it is in the lifecycle.
//! - A **scope** limiting what it can do (tools allowed, max turns,
//!   max budget).
//! - Lifecycle events fired into M5 HookBus (`WorkerSpawned`,
//!   `WorkerCompleted`, `WorkerFailed`).
//!
//! This pilot ships the type-only surface. The actual orchestrator
//! that spawns + supervises workers lives in M3-T3 commit 2.
//!
//! Stacked on #338 for `WorkerId` (defined there).
//!
//! Layout:
//!
//! - [`spec`] — `WorkerRole`, `WorkerScope`, `WorkerStatus`, `WorkerSpec`

pub mod spec;

pub use spec::{
    WorkerLifecycleEvent, WorkerRole, WorkerScope, WorkerSpec, WorkerStatus,
    WorkerTerminationReason,
};
