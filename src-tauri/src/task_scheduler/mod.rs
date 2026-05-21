//! M3-T4 — Task scheduler queue + priority types (pilot).
//!
//! The kernel's task queue routes ready-to-run tasks to workers under
//! a priority + age policy. ADR §M3-T4 specifies:
//!
//! - 5 priority bands (Critical → Background)
//! - FIFO within band (oldest scheduled wins on equal priority)
//! - Stable id-based tiebreak when timestamps collide
//! - Deadline-aware: an entry with a deadline closer than its band-
//!   peer's deadline jumps ahead of that peer
//!
//! This pilot ships the data structures + the ordering predicate.
//! The actual `tokio` runner that drains the queue lives in M3-T4
//! commit 2 next to `runtime::task::TaskScheduler`.
//!
//! Layout:
//!
//! - [`queue`] — `Priority`, `ScheduledTask`, `ScheduleQueue`, `ScheduleStats`

pub mod queue;

pub use queue::{Priority, ScheduleQueue, ScheduleStats, ScheduledTask};
