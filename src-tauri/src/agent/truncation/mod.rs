//! M2-H L1 — Tool-output truncation policy.
//!
//! The Token Defense subsystem in ADR §M2-H stacks **7 layers** to keep
//! the per-turn context budget under control. **L1** is the simplest
//! and most universal: at every tool-handler exit point, truncate the
//! handler's textual output to a per-handler byte budget before it
//! flows into the agent's transcript.
//!
//! This module ships:
//!
//! - [`TruncationPolicy`] — per-handler byte budgets, with a sane
//!   default table baked in (`TruncationPolicy::default_budgets()`).
//! - [`HandlerKind`] — strongly-typed enumeration of the handler
//!   classes the agent dispatches to (shell, file, search, web, mcp,
//!   ...). Adding a new handler kind here is the explicit place to
//!   register its budget.
//! - [`truncate_with_marker`] — UTF-8-safe truncation helper that
//!   appends a single-line marker (`"…[truncated 1234 of 5678 bytes]"`)
//!   when the input exceeds the budget.
//!
//! What this PR explicitly **does not** ship:
//!
//! - Wire-up into agent handler exits — that lands in `M2-H L1 commit 2`
//!   (per-handler call-site updates touch 8+ files; isolating the
//!   policy first lets reviewers see the API clearly).
//! - User-overridable `[tool_output_budgets]` config table — `M2-H L1
//!   commit 3` adds a settings surface that hydrates the policy from
//!   `~/.uclaw/settings.toml`.
//! - The remaining 6 layers (L2 tool exposure, L3 schema normalize,
//!   L4 baseline rotation, L5 history window, L6 fold-on-overflow,
//!   L7 hard cap). Those are tracked separately under M2-H.
//!
//! Layout:
//!
//! - [`policy`] — `TruncationPolicy` + `HandlerKind` + `truncate_with_marker`

pub mod policy;

pub use policy::{truncate_with_marker, HandlerKind, TruncationPolicy};
