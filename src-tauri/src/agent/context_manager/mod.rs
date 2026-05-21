//! M2-B — `ContextManager`: per-turn context composition skeleton.
//!
//! `ContextManager` is the central orchestrator that the agent loop
//! calls once per turn to produce the system prompt + the set of
//! context fragments to inject. It composes the building blocks
//! already landed in M2:
//!
//! - **`BaselineBlock` registry** (M2-A) — static, every-turn prompt
//!   sections (header / safety guardrails / capability declarations).
//! - **`ContextFragment` set** (M2-C) — dynamic, on-demand fragments
//!   the agent can search / read / pin.
//!
//! This pilot ships the orchestrator **with both** of those wired in.
//! Three pieces remain for follow-up PRs:
//!
//! 1. **`ContextToolSet` (M2-F #330) wire-up** — once that lands the
//!    manager will offer a `tool_set(&self) -> &ContextToolSet` so
//!    the agent can search/read fragments mid-turn. Currently the
//!    fragment set is read-only — populated at `add_fragment` time.
//! 2. **UCLAW.md hot reload** — a file watcher that swaps in new
//!    project-local baseline content when `UCLAW.md` changes on disk.
//! 3. **Token-defense integration** — apply L1 (TruncationPolicy),
//!    L2 (ToolExposure), L5 (image strip), L6 (audit_call_outputs)
//!    at `for_prompt` time. Currently `for_prompt` is a pure
//!    composer; the L-layer wire-up is M2-B commit 2.
//!
//! Layout:
//!
//! - [`manager`] — `ContextManager`, `ComposeQuery`, `ComposedContext`,
//!   `ComposeStats`

pub mod manager;

pub use manager::{ComposedContext, ComposeQuery, ComposeStats, ContextManager};
