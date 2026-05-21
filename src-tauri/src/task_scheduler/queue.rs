//! `ScheduleQueue` — priority queue with deadline-aware ordering.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

/// 5-band priority. Higher rung wins; `Critical` runs before `High`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// User explicitly asked "now" / safety-related work.
    Critical,
    /// User-initiated foreground request.
    High,
    /// Default — ordinary task.
    Normal,
    /// Background work that may yield to anything above.
    Low,
    /// Lowest — cleanup, indexing, telemetry.
    Background,
}

impl Priority {
    /// Higher = runs first. `Critical = 4`, `Background = 0`.
    pub const fn rung(self) -> u8 {
        match self {
            Self::Critical => 4,
            Self::High => 3,
            Self::Normal => 2,
            Self::Low => 1,
            Self::Background => 0,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rung().cmp(&other.rung())
    }
}

/// One queue entry. `created_at_micros` is monotonically increasing
/// (caller uses a sequence counter; not wall-clock). `deadline_micros`
/// is optional — entries with deadlines win ties against deadline-less
/// peers in the same band when the deadline is closer than the peer's
/// effective deadline (`+infinity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    pub task_id: String,
    pub priority: Priority,
    /// Monotonic sequence number assigned at enqueue time. Smaller =
    /// older. The caller is responsible for ensuring monotonicity
    /// (typically `AtomicU64::fetch_add`).
    pub created_at_seq: u64,
    /// Optional hard deadline (microseconds since epoch). `None` = no
    /// deadline — entry treated as deadline = +infinity in compares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_micros: Option<u64>,
}

impl ScheduledTask {
    pub fn new(task_id: impl Into<String>, priority: Priority, created_at_seq: u64) -> Self {
        Self {
            task_id: task_id.into(),
            priority,
            created_at_seq,
            deadline_micros: None,
        }
    }

    pub fn with_deadline(mut self, deadline_micros: u64) -> Self {
        self.deadline_micros = Some(deadline_micros);
        self
    }

    /// Effective deadline for compare purposes — `None` becomes
    /// `u64::MAX` (far future).
    fn effective_deadline(&self) -> u64 {
        self.deadline_micros.unwrap_or(u64::MAX)
    }
}

/// `Ord` implementation produces the **pop-this-first** order
/// recognized by `BinaryHeap`. Higher = pops first.
///
/// Comparison (most significant first):
/// 1. Priority rung (higher wins).
/// 2. Effective deadline (lower wins — closer deadlines pop first).
/// 3. `created_at_seq` (lower wins — older pops first).
/// 4. `task_id` (lexicographic ascending wins — deterministic tiebreak).
impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Priority: higher rung pops first → use natural cmp.
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {}
            o => return o,
        }
        // Deadline: lower micros pops first → INVERT compare.
        match self.effective_deadline().cmp(&other.effective_deadline()) {
            Ordering::Equal => {}
            o => return o.reverse(),
        }
        // created_at_seq: lower (older) pops first → INVERT.
        match self.created_at_seq.cmp(&other.created_at_seq) {
            Ordering::Equal => {}
            o => return o.reverse(),
        }
        // task_id: lexicographic ascending → INVERT.
        self.task_id.cmp(&other.task_id).reverse()
    }
}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Queue stats (UI / diagnostic).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScheduleStats {
    pub total_enqueued: u64,
    pub total_popped: u64,
    pub current_depth: usize,
    pub by_priority_depth: [usize; 5],
}

impl ScheduleStats {
    pub fn priority_depth(&self, p: Priority) -> usize {
        self.by_priority_depth[p.rung() as usize]
    }
}

/// Bounded priority queue of `ScheduledTask`s.
///
/// Wraps `BinaryHeap` (which is a max-heap) — our `Ord` impl ensures
/// "max" means "pop first" per the rules in [`ScheduledTask::cmp`].
#[derive(Debug, Clone)]
pub struct ScheduleQueue {
    heap: BinaryHeap<ScheduledTask>,
    stats: ScheduleStats,
}

impl ScheduleQueue {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            stats: ScheduleStats::default(),
        }
    }

    pub fn push(&mut self, task: ScheduledTask) {
        self.stats.total_enqueued += 1;
        self.stats.current_depth += 1;
        self.stats.by_priority_depth[task.priority.rung() as usize] += 1;
        self.heap.push(task);
    }

    /// Pop the highest-priority ready task. Returns `None` when empty.
    pub fn pop(&mut self) -> Option<ScheduledTask> {
        let task = self.heap.pop()?;
        self.stats.total_popped += 1;
        self.stats.current_depth -= 1;
        let rung = task.priority.rung() as usize;
        // Saturating sub guards against any future double-counting bug.
        self.stats.by_priority_depth[rung] =
            self.stats.by_priority_depth[rung].saturating_sub(1);
        Some(task)
    }

    /// Peek at the next-pop entry without removing it.
    pub fn peek(&self) -> Option<&ScheduledTask> {
        self.heap.peek()
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn stats(&self) -> &ScheduleStats {
        &self.stats
    }
}

impl Default for ScheduleQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, p: Priority, seq: u64) -> ScheduledTask {
        ScheduledTask::new(id, p, seq)
    }

    // ── Priority ────────────────────────────────────────────────────

    #[test]
    fn priority_rungs_strict_descending() {
        assert_eq!(Priority::Critical.rung(), 4);
        assert_eq!(Priority::High.rung(), 3);
        assert_eq!(Priority::Normal.rung(), 2);
        assert_eq!(Priority::Low.rung(), 1);
        assert_eq!(Priority::Background.rung(), 0);
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
        assert!(Priority::Low > Priority::Background);
    }

    #[test]
    fn priority_serde_snake_case() {
        let v = serde_json::to_value(Priority::Critical).unwrap();
        assert_eq!(v, serde_json::json!("critical"));
        let v = serde_json::to_value(Priority::Background).unwrap();
        assert_eq!(v, serde_json::json!("background"));
    }

    // ── push + pop ──────────────────────────────────────────────────

    #[test]
    fn empty_queue_peek_pop_return_none() {
        let mut q = ScheduleQueue::new();
        assert!(q.peek().is_none());
        assert!(q.pop().is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn higher_priority_pops_first() {
        let mut q = ScheduleQueue::new();
        q.push(t("a", Priority::Low, 1));
        q.push(t("b", Priority::Critical, 2));
        q.push(t("c", Priority::Normal, 3));
        assert_eq!(q.pop().unwrap().task_id, "b");
        assert_eq!(q.pop().unwrap().task_id, "c");
        assert_eq!(q.pop().unwrap().task_id, "a");
    }

    // ── same priority: older first ─────────────────────────────────

    #[test]
    fn same_priority_older_seq_pops_first() {
        let mut q = ScheduleQueue::new();
        q.push(t("late", Priority::Normal, 10));
        q.push(t("early", Priority::Normal, 1));
        q.push(t("middle", Priority::Normal, 5));
        assert_eq!(q.pop().unwrap().task_id, "early");
        assert_eq!(q.pop().unwrap().task_id, "middle");
        assert_eq!(q.pop().unwrap().task_id, "late");
    }

    // ── deadline: closer wins within band ──────────────────────────

    #[test]
    fn deadline_closer_wins_over_older_seq() {
        // Both Normal: older entry has no deadline, newer has tight deadline.
        let mut q = ScheduleQueue::new();
        q.push(t("old-no-deadline", Priority::Normal, 1));
        q.push(t("new-with-deadline", Priority::Normal, 2).with_deadline(100));
        // new-with-deadline pops first (deadline 100 < +infinity).
        assert_eq!(q.pop().unwrap().task_id, "new-with-deadline");
        assert_eq!(q.pop().unwrap().task_id, "old-no-deadline");
    }

    #[test]
    fn deadline_between_two_deadlined_entries_closer_wins() {
        let mut q = ScheduleQueue::new();
        q.push(t("later-deadline", Priority::High, 1).with_deadline(500));
        q.push(t("sooner-deadline", Priority::High, 2).with_deadline(100));
        assert_eq!(q.pop().unwrap().task_id, "sooner-deadline");
        assert_eq!(q.pop().unwrap().task_id, "later-deadline");
    }

    // ── id tiebreak when seq AND deadline match ───────────────────

    #[test]
    fn id_lex_tiebreak_when_priority_deadline_seq_all_equal() {
        let mut q = ScheduleQueue::new();
        q.push(t("zebra", Priority::Normal, 5));
        q.push(t("alpha", Priority::Normal, 5));
        q.push(t("mango", Priority::Normal, 5));
        // Lex ascending wins.
        assert_eq!(q.pop().unwrap().task_id, "alpha");
        assert_eq!(q.pop().unwrap().task_id, "mango");
        assert_eq!(q.pop().unwrap().task_id, "zebra");
    }

    // ── priority dominates everything ─────────────────────────────

    #[test]
    fn higher_priority_dominates_better_deadline_in_lower_band() {
        let mut q = ScheduleQueue::new();
        q.push(t("low-but-urgent", Priority::Low, 1).with_deadline(1));
        q.push(t("high-relaxed", Priority::High, 100));
        // High priority pops first even though Low has tight deadline.
        assert_eq!(q.pop().unwrap().task_id, "high-relaxed");
        assert_eq!(q.pop().unwrap().task_id, "low-but-urgent");
    }

    // ── peek ──────────────────────────────────────────────────────

    #[test]
    fn peek_does_not_remove() {
        let mut q = ScheduleQueue::new();
        q.push(t("a", Priority::High, 1));
        q.push(t("b", Priority::Normal, 2));
        assert_eq!(q.peek().unwrap().task_id, "a");
        assert_eq!(q.peek().unwrap().task_id, "a");
        assert_eq!(q.len(), 2);
    }

    // ── stats ─────────────────────────────────────────────────────

    #[test]
    fn stats_track_push_and_pop_per_priority() {
        let mut q = ScheduleQueue::new();
        q.push(t("a", Priority::Critical, 1));
        q.push(t("b", Priority::Critical, 2));
        q.push(t("c", Priority::Low, 3));
        let s = q.stats();
        assert_eq!(s.total_enqueued, 3);
        assert_eq!(s.total_popped, 0);
        assert_eq!(s.current_depth, 3);
        assert_eq!(s.priority_depth(Priority::Critical), 2);
        assert_eq!(s.priority_depth(Priority::Low), 1);
        assert_eq!(s.priority_depth(Priority::Normal), 0);

        q.pop();
        let s = q.stats();
        assert_eq!(s.total_popped, 1);
        assert_eq!(s.current_depth, 2);
        assert_eq!(s.priority_depth(Priority::Critical), 1);
    }

    // ── serde ─────────────────────────────────────────────────────

    #[test]
    fn task_serde_roundtrip_with_deadline() {
        let task = t("x", Priority::High, 7).with_deadline(123_456);
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"createdAtSeq\":7"));
        assert!(json.contains("\"deadlineMicros\":123456"));
        let back: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert_eq!(task, back);
    }

    #[test]
    fn task_serde_skips_none_deadline() {
        let task = t("x", Priority::Low, 1);
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("deadlineMicros"));
    }
}
