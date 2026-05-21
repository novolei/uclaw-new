//! Runtime contracts — the typed kernel of Agent OS v2.
//!
//! Per `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`:
//!
//! - **IntentSpec** is what a user / trigger / system event produces. It
//!   says *what should happen* without prescribing how.
//! - **TaskSpec** is the executable form of an intent. The scheduler /
//!   capability mesh resolves an IntentSpec into one or more TaskSpecs.
//! - **TaskEvent** is the stream of observable events a task emits while
//!   it runs (model turns, tool calls, memory access, permission checks,
//!   ...).
//!
//! M1 (Phase 0.5 + Runtime Contracts) introduces these as **pure
//! definitions** — no wire-up to the agent loop yet. M1-T2 wraps
//! `agentic_loop::run_agentic_loop` in a `SessionTask`. M1-T3 promotes
//! `harness::trace::HarnessEvent` to the canonical `TaskEvent` type.
//! M1-T4 fans out adapters across the agent / browser / automation
//! domains. M1-T5 lands the rollout JSONL writer + V44 migration.
//!
//! Layout:
//!
//! - [`contracts`]      — type definitions (M1-T1, PR #304)
//! - [`task`]           — SessionTask trait + TaskScheduler (M1-T2a, PR #305)
//! - [`rollout`]        — JSONL writer + task_events_rollout mirror (M1-T5, PR #310)
//! - [`context`]        — ContextFragment / ContextSource / ContextRef (M2-C, PR #329)
//! - [`context_tools`]  — ContextToolSet: search + read + pin/release/cite tools (M2-F pilot)

pub mod context;
pub mod context_tools;
pub mod contracts;
pub mod rollout;
pub mod task;
