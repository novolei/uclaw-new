//! M3-T6 — Policy evaluator (pilot).
//!
//! ADR §"Hook policies and risk classes" describes a `PolicySpec`
//! that evaluates concrete `ActionRequest`s against rules and emits
//! a `HookDecision` (Allow / Deny / AskUser). This pilot ships:
//!
//! - **`ActionRequest`** — a typed request for a guarded action
//!   (tool call, network access, file write, ...).
//! - **`PolicyRule`** — one rule with a `pattern` (action class +
//!   matcher) and an `outcome` (HookDecision template).
//! - **`PolicySpec`** — ordered list of rules.
//! - **`evaluate(spec, request) -> HookDecision`** — walks rules
//!   in order; first matching rule wins. Falls through to `Allow`
//!   when no rule matches.
//!
//! Wire-up: PolicySpec plugs into the M5 HookBus (#340) as a
//! `HookSubscriber` for the 5 decision-capable events. That wire-up
//! lives in M3-T6 commit 2.
//!
//! Layout:
//!
//! - [`spec`] — `ActionRequest`, `PolicyRule`, `PolicySpec`,
//!   `evaluate`, `MatchPattern`

pub mod spec;
pub mod subscriber;

pub use spec::{evaluate, ActionRequest, MatchPattern, PolicyRule, PolicySpec};
pub use subscriber::PolicySpecSubscriber;
