//! Memory recall evaluation: golden-set parsing, hit matching and ranked
//! metrics. The CLI is `examples/memory_recall_eval` (ported from uclaw-pi,
//! T7 of docs/superpowers/plans/2026-11-09-uclaw-pi-backport.md).

pub mod golden;
pub mod matcher;
pub mod metrics;

pub use golden::{parse_golden, GoldenItem};
pub use matcher::Matcher;
pub use metrics::{group_stats, GroupStats, RankedResult};
