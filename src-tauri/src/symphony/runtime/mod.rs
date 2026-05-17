//! Symphony runtime — the live execution layer.
//!
//! Layered top-down:
//! - `service`   — `SymphonyService: ManagedService`, the only public face
//!   wired into `main.rs` Stage 3.
//! - `run_actor` — one tokio task per in-flight workflow run. Owns the DAG
//!   scheduler.
//! - `node_run`  — adapts a single `SymphonyNode` into one
//!   `HeadlessDelegate`-driven `run_agentic_loop` call.
//! - `recovery`  — restart reconciliation (mark stalled, rebuild actors).
//! - `stall`     — per-node heartbeat tracking + deadline expiry detection.
//! - `cost`      — per-day total + cap helpers (atop `cost_records`).
//! - `retry`     — Symphony SPEC backoff formula.
//! - `run_session` — analog of `automation::runtime::run_session`: home
//!   space + per-node `agent_sessions` row + transcript persistence.

pub mod cost;
pub mod retry;
