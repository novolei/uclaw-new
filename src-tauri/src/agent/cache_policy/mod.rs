//! M2-I — Prompt-caching policy (pilot).
//!
//! Anthropic Claude (and similar providers) support **prompt caching**
//! via `cache_control: { type: "ephemeral" }` markers. Marking long-
//! stable prefixes (baseline prompt, large skill metadata, recent
//! conversation history) lets the provider cache embeddings across
//! calls — typically a 4-10× cost reduction on multi-turn sessions.
//!
//! Anthropic's API allows **up to 4 cache breakpoints** per request.
//! L2-I decides which 4 segments are most valuable to mark.
//!
//! Default 4-segment partition (per ADR §M2-I):
//!
//! 1. **Baseline** — static system prompt (M2-A). Changes only when
//!    UCLAW.md or a baseline block is edited. Cache-friendly.
//! 2. **SkillMetadata** — selected skill metadata (M2-H L3). Stable
//!    across the same skill set; flips when the pin set changes.
//! 3. **ContextFragments** — injected fragments (M2-B). Most volatile;
//!    diff updates (M2-D) try to keep this stable.
//! 4. **Conversation** — message history (everything before the
//!    current turn). Grows monotonically.
//!
//! The current turn's user message is **not** marked — it changes
//! every turn so caching is pointless.
//!
//! This pilot ships the **policy types + segment selection logic**.
//! The actual provider-request wire-up (Anthropic `cache_control`
//! injection / OpenAI equivalent) lives in M2-I commit 2.
//!
//! Layout:
//!
//! - [`policy`] — `CacheSegmentKind`, `CacheBreakpoint`, `CachePolicy`

pub mod policy;

pub use policy::{
    place_breakpoints, CacheBreakpoint, CachePolicy, CacheSegmentKind, PolicyStats,
    MAX_BREAKPOINTS,
};
