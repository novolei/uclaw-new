//! W2: agent retry-budget extension.
//!
//! See `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4.

pub mod backoff;

pub use backoff::{BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS, compute_delay};
