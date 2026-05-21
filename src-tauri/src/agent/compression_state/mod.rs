//! M2-H L7 — Compaction state machine.
//!
//! When a conversation approaches the context budget, uClaw can take
//! one of three compaction paths. Which path is legal depends on
//! where the conversation has been:
//!
//! ```text
//!                 ┌────────────────────────────────────────┐
//!                 │              None                      │
//!                 └─────────────┬──────────────────────────┘
//!                               │  (token% reaches threshold)
//!                               ▼
//!                 ┌────────────────────────────────────────┐
//!                 │           Approaching                  │
//!                 └──┬───────────────────┬─────────────────┘
//!                    │                   │
//!     legacy choice  │                   │  modern choice
//!                    ▼                   ▼
//!         ┌─────────────────┐  ┌───────────────────┐
//!         │ LegacyCompacted │  │ StructuredFolded  │ (M2-G)
//!         └─────────────────┘  └────────┬──────────┘
//!                                       │
//!                              once folded, may also:
//!                                       ▼
//!                              ┌───────────────────┐
//!                              │  DiffInjected     │ (M2-D)
//!                              └───────────────────┘
//! ```
//!
//! Critical invariant: **LegacyCompacted → DiffInjected** is **never
//! legal**. Diff-based re-injection (M2-D) depends on having a
//! `StructuredFold` baseline to diff against. Skipping the structured
//! step would corrupt the next-turn context. This rule is enforced by
//! [`transition_to`].
//!
//! Other useful invariants:
//!
//! - **None → LegacyCompacted/StructuredFolded** without passing
//!   through `Approaching` is allowed (forced compaction by user
//!   command).
//! - **Any state → None** is allowed (manual reset / new session).
//! - **StructuredFolded → LegacyCompacted** is rejected — once you
//!   have a structured fold you can't go back to opaque text; the
//!   diff layer would lose its baseline.
//! - **DiffInjected → DiffInjected** is allowed (subsequent turns
//!   continue diffing against the latest folded baseline).
//! - **DiffInjected → StructuredFolded** is allowed (re-fold to
//!   re-anchor when diff overhead grows).
//!
//! Layout:
//!
//! - [`state`] — `CompressionState` enum + `Compactor` state machine

pub mod state;

pub use state::{
    Compactor, CompactorTrigger, CompressionState, TransitionError, DEFAULT_APPROACHING_THRESHOLD,
    DEFAULT_COMPACT_THRESHOLD,
};
