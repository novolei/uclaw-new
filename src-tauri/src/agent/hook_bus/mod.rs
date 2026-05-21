//! M5 — HookBus 类型骨架 (pilot).
//!
//! ADR §"Hook policies and HookBus" specifies a **13-event bus** the
//! agent fires across the turn lifecycle. Each event passes through
//! every registered `HookSubscriber` which may return a
//! `HookDecision` (Allow / Deny / AskUser) — the bus aggregates those
//! decisions into a final verdict, with Deny winning over AskUser
//! winning over Allow.
//!
//! 13 events (per ADR §5.4 Hook Events):
//!
//! | Event | Phase | Decision-capable? |
//! |---|---|---|
//! | `PreToolUse`         | before a tool runs | yes |
//! | `PostToolUse`        | after a tool returns | observe-only |
//! | `PreLlmCall`         | before LLM request | yes |
//! | `PostLlmCall`        | after LLM response | observe-only |
//! | `PrePermission`      | before permission resolution | yes |
//! | `PostPermission`     | after permission resolution | observe-only |
//! | `PreContextInject`   | before fragment injection | yes |
//! | `PostContextInject`  | after fragment injection | observe-only |
//! | `TaskStart`          | new SessionTask spawned | observe-only |
//! | `TaskEnd`            | SessionTask completed/cancelled/failed | observe-only |
//! | `MemoryWrite`        | memory graph write | yes (can block) |
//! | `MemoryRecall`       | memory graph read | observe-only |
//! | `Checkpoint`         | rollback checkpoint emitted | observe-only |
//!
//! This pilot ships the **types + bus + aggregation rules**. Wire-up
//! into agentic_loop / safety manager / memory_graph lives in M5
//! commit 2.
//!
//! Layout:
//!
//! - [`event`]       — `HookEvent` 13-variant enum
//! - [`subscriber`]  — `HookSubscriber` trait + `BusError`
//! - [`bus`]         — `HookBus` dispatcher + decision aggregation

pub mod bus;
pub mod event;
pub mod subscriber;

pub use bus::{HookBus, BusError};
pub use event::{HookEvent, HookEventKind};
pub use subscriber::{HookSubscriber, SubscriberId};
