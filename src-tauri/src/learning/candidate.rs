//! Learning candidate buffer — Sprint 1.2 of the openhuman warm-start port.
//!
//! Defines:
//! - The six-class taxonomy [`FacetClass`] (Style / Identity / Tooling / Veto /
//!   Goal / Channel) with the budgets + half-lives the stability detector
//!   (Sprint 1.3) will key off.
//! - The four-family cue weighting [`CueFamily`] (Explicit=1.0 / Structural=0.9 /
//!   Behavioral=0.7 / Recurrence=0.6).
//! - A typed [`EvidenceRef`] pointer back to the originating substrate row —
//!   slimmed from openhuman's full taxonomy to what our uClaw substrate can
//!   actually produce today. Composio-shaped variants are deferred until we
//!   have integrations.
//! - The unit-of-work [`LearningCandidate`], one row per (class, name, value)
//!   triple emitted by a producer.
//! - A bounded thread-safe [`Buffer`] (FIFO ring; oldest is evicted on
//!   overflow). The producer side (Sprint 1.9 chat-turn extractor + future
//!   integrations) pushes; the consumer side (Sprint 1.3/1.4 stability
//!   detector + scheduler) drains.
//!
//! Reference: openhuman `src/openhuman/learning/candidate.rs:1-120`.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

// ─── Taxonomy ──────────────────────────────────────────────────────────

/// Six-class taxonomy of what the facet store can hold.
///
/// The class controls two things the stability detector cares about:
/// 1. The half-life used in the exp(-Δt/half_life) decay term.
/// 2. The hard budget of "how many active facets in this class are allowed
///    in the system prompt at once" — budgets are enforced by promoting
///    only the top-K by stability when more than K cross the promote threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacetClass {
    /// Communication style preferences — verbosity, formality, code-block format.
    Style,
    /// Stable biographical facts — timezone, name, language, role.
    Identity,
    /// Developer toolchain preferences — editor, package manager, OS, framework.
    Tooling,
    /// Hard user vetoes — things the user has explicitly rejected.
    Veto,
    /// Active user goals or ongoing projects.
    Goal,
    /// Preferred communication channel or platform.
    Channel,
}

impl FacetClass {
    /// Wire-format string for SQLite `class` column.
    pub fn as_str(self) -> &'static str {
        match self {
            FacetClass::Style => "style",
            FacetClass::Identity => "identity",
            FacetClass::Tooling => "tooling",
            FacetClass::Veto => "veto",
            FacetClass::Goal => "goal",
            FacetClass::Channel => "channel",
        }
    }

    /// Half-life in days for the exp(-Δt/half_life) decay term. Values
    /// match openhuman's `stability_detector.rs:54-59` directly.
    ///
    /// Identity facts age slowly (90d) — your name doesn't change. Style
    /// preferences shift faster (14d) — moods drift. Channel info ages
    /// fastest (7d) — people switch tools weekly.
    pub fn half_life_days(self) -> f64 {
        match self {
            FacetClass::Channel => 7.0,
            FacetClass::Style => 14.0,
            FacetClass::Tooling => 30.0,
            FacetClass::Veto => 30.0,
            FacetClass::Goal => 60.0,
            FacetClass::Identity => 90.0,
        }
    }

    /// Maximum number of "active" facets in this class allowed in the
    /// prompt at once. Values match openhuman's
    /// `stability_detector.rs:75-82`.
    ///
    /// Channel is uniquely budgeted at 1 — the user has one primary
    /// comm channel at a time. Tooling gets the highest budget (5)
    /// because devs juggle multiple tools.
    pub fn budget(self) -> usize {
        match self {
            FacetClass::Channel => 1,
            FacetClass::Style => 4,
            FacetClass::Identity => 4,
            FacetClass::Veto => 3,
            FacetClass::Goal => 3,
            FacetClass::Tooling => 5,
        }
    }
}

/// How a candidate signal was produced. Determines the weight multiplier
/// applied in the stability formula.
///
/// Higher-weight families contribute more strongly per evidence item.
/// Phase 1 canonical values match openhuman:
/// `Explicit=1.0, Structural=0.9, Behavioral=0.7, Recurrence=0.6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueFamily {
    /// Direct declaration of intent by the user (highest weight, 1.0).
    ///
    /// Examples: "I prefer pnpm", "always use terse replies",
    /// "I work in PST".
    Explicit,
    /// Inferred from structured file or provider metadata (0.9).
    ///
    /// Examples: `package.json#packageManager`, Gmail display name,
    /// VS Code `settings.json` keys.
    Structural,
    /// Inferred by heuristics or LLM from observed user behaviour (0.7).
    ///
    /// Examples: agent edits get reverted; "is X working?" → user
    /// always answers yes; correction-repeat signal.
    Behavioral,
    /// Materialized from recurrence statistics (0.6).
    ///
    /// Examples: tier_escalator backlink hotness, source frequency.
    Recurrence,
}

impl CueFamily {
    /// Weight multiplier for this cue family in the stability formula.
    pub fn weight(self) -> f64 {
        match self {
            CueFamily::Explicit => 1.0,
            CueFamily::Structural => 0.9,
            CueFamily::Behavioral => 0.7,
            CueFamily::Recurrence => 0.6,
        }
    }

    /// Wire-format string for the `cue_families_json` map key.
    pub fn as_str(self) -> &'static str {
        match self {
            CueFamily::Explicit => "explicit",
            CueFamily::Structural => "structural",
            CueFamily::Behavioral => "behavioral",
            CueFamily::Recurrence => "recurrence",
        }
    }
}

// ─── Evidence reference ────────────────────────────────────────────────

/// A typed pointer back to the substrate row that produced this candidate.
///
/// Slimmed from openhuman's taxonomy to what our uClaw substrate produces
/// today. Composio-shaped variants (`Provider`, `EmailMessage`, etc.)
/// will be added if/when we wire integrations.
///
/// Serialized with a snake_case `type` discriminator so payloads are
/// human-readable in `cost_records.metadata_json` / health findings UI:
/// `{"type":"chat_turn","session_id":"...","turn_id":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceRef {
    /// One agent turn — the chat-turn extractor (Sprint 1.9) will
    /// produce candidates anchored to this. Both fields refer to
    /// `agent_messages` and `agent_turns` table rows.
    ChatTurn {
        session_id: String,
        turn_id: String,
    },
    /// A workspace file under a mounted folder. `sha256` lets the
    /// detector skip re-counting a file that hasn't changed (analog of
    /// our Phase 7 sha256 short-circuit).
    WorkspaceFile {
        path: String,
        sha256: String,
    },
    /// An auto-link `[[entity:slug]]` mention discovered by the
    /// Phase 2 hook, producing structural evidence for an entity
    /// becoming a Tooling/Goal/Channel facet.
    EntityPageMention {
        node_id: String,
        version_id: String,
    },
    /// Catch-all for cases we haven't typed yet (e.g. "the user
    /// directly wrote this to PROFILE.md by hand"). Keep the body
    /// short — it's stored verbatim in the facet's
    /// `cue_families_json.metadata`.
    Manual { note: String },
}

// ─── Learning candidate ────────────────────────────────────────────────

/// One unit of learning evidence emitted by a producer.
///
/// Each candidate asserts a specific `(class, name, value)` triple
/// alongside the cue family + evidence pointer that backs it. The
/// stability detector (Sprint 1.3) aggregates competing candidates for
/// the same `(class, name)` pair and produces a single
/// `user_profile_facets` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningCandidate {
    pub class: FacetClass,
    /// Short slot name within the class — e.g. `editor`, `timezone`,
    /// `verbosity`. UNIQUE per class (enforced at the table level by
    /// V39 migration).
    pub name: String,
    /// Free-form short value — e.g. `helix`, `PST`, `terse`.
    pub value: String,
    pub cue: CueFamily,
    pub evidence: EvidenceRef,
    /// Producer-supplied confidence in `[0.0, 1.0]`. Multiplied into
    /// the cue family weight at stability-time. Default 1.0.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Wall-clock at which this candidate was emitted (epoch ms). The
    /// detector reads this for the exp(-Δt/half_life) decay term.
    pub observed_at_ms: i64,
}

fn default_confidence() -> f64 {
    1.0
}

impl LearningCandidate {
    /// Convenience constructor stamping `observed_at_ms` to now.
    pub fn new(
        class: FacetClass,
        name: impl Into<String>,
        value: impl Into<String>,
        cue: CueFamily,
        evidence: EvidenceRef,
    ) -> Self {
        Self {
            class,
            name: name.into(),
            value: value.into(),
            cue,
            evidence,
            confidence: 1.0,
            observed_at_ms: now_epoch_ms(),
        }
    }

    pub fn with_confidence(mut self, c: f64) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }
}

fn now_epoch_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ─── Bounded buffer ────────────────────────────────────────────────────

/// Bounded thread-safe ring buffer. Producers push; consumers drain.
///
/// When at capacity, [`push`](Self::push) evicts the oldest entry first
/// (FIFO overflow). This matches openhuman's behaviour and avoids
/// blocking the producer when the consumer (stability detector) is busy.
pub struct Buffer {
    capacity: usize,
    inner: Mutex<VecDeque<LearningCandidate>>,
}

impl Buffer {
    /// Create a new buffer with the given capacity. A capacity of 0
    /// silently accepts pushes but never retains them (useful in
    /// tests when you just want the API surface).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    /// Push one candidate. Returns `true` when a candidate was
    /// evicted to make room (caller can use this for telemetry).
    pub fn push(&self, candidate: LearningCandidate) -> bool {
        let mut q = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(), // recover from poisoning
        };
        if self.capacity == 0 {
            return false;
        }
        let evicted = q.len() >= self.capacity;
        if evicted {
            q.pop_front();
        }
        q.push_back(candidate);
        evicted
    }

    /// Drain all currently-buffered candidates. Returns them in FIFO
    /// order. The consumer (stability detector) takes ownership and
    /// aggregates into `user_profile_facets`.
    pub fn drain(&self) -> Vec<LearningCandidate> {
        let mut q = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        q.drain(..).collect()
    }

    /// How many candidates are currently buffered.
    pub fn len(&self) -> usize {
        self.inner.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// True when no candidates are buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Configured capacity (max entries before FIFO eviction).
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_candidate(class: FacetClass, name: &str, value: &str) -> LearningCandidate {
        LearningCandidate::new(
            class,
            name,
            value,
            CueFamily::Explicit,
            EvidenceRef::Manual {
                note: "test".into(),
            },
        )
    }

    // ── Taxonomy ──────────────────────────────────────────────

    #[test]
    fn facet_class_string_roundtrip() {
        for class in [
            FacetClass::Style,
            FacetClass::Identity,
            FacetClass::Tooling,
            FacetClass::Veto,
            FacetClass::Goal,
            FacetClass::Channel,
        ] {
            let s = class.as_str();
            assert!(!s.is_empty());
            assert_eq!(s.chars().all(|c| c.is_ascii_lowercase()), true);
        }
    }

    #[test]
    fn facet_class_half_lives_match_openhuman() {
        // Direct check that we ported the canonical values without typos.
        assert_eq!(FacetClass::Channel.half_life_days(), 7.0);
        assert_eq!(FacetClass::Style.half_life_days(), 14.0);
        assert_eq!(FacetClass::Tooling.half_life_days(), 30.0);
        assert_eq!(FacetClass::Veto.half_life_days(), 30.0);
        assert_eq!(FacetClass::Goal.half_life_days(), 60.0);
        assert_eq!(FacetClass::Identity.half_life_days(), 90.0);
        // Identity > Goal > Tooling = Veto > Style > Channel — half-lives
        // strictly monotone from longest-lived to shortest-lived.
        assert!(FacetClass::Identity.half_life_days() > FacetClass::Channel.half_life_days());
    }

    #[test]
    fn facet_class_budgets_match_openhuman() {
        assert_eq!(FacetClass::Channel.budget(), 1);
        assert_eq!(FacetClass::Style.budget(), 4);
        assert_eq!(FacetClass::Identity.budget(), 4);
        assert_eq!(FacetClass::Veto.budget(), 3);
        assert_eq!(FacetClass::Goal.budget(), 3);
        assert_eq!(FacetClass::Tooling.budget(), 5);
        // Tooling gets the largest budget — devs juggle multiple tools.
        let total: usize = [
            FacetClass::Style,
            FacetClass::Identity,
            FacetClass::Tooling,
            FacetClass::Veto,
            FacetClass::Goal,
            FacetClass::Channel,
        ]
        .iter()
        .map(|c| c.budget())
        .sum();
        assert_eq!(total, 20, "Total budget across all classes is 20");
    }

    #[test]
    fn cue_family_weights_match_canonical() {
        assert_eq!(CueFamily::Explicit.weight(), 1.0);
        assert_eq!(CueFamily::Structural.weight(), 0.9);
        assert_eq!(CueFamily::Behavioral.weight(), 0.7);
        assert_eq!(CueFamily::Recurrence.weight(), 0.6);
        // Strict ordering: explicit declarations beat structural metadata
        // beat behavioural inference beat recurrence stats.
        assert!(CueFamily::Explicit.weight() > CueFamily::Structural.weight());
        assert!(CueFamily::Structural.weight() > CueFamily::Behavioral.weight());
        assert!(CueFamily::Behavioral.weight() > CueFamily::Recurrence.weight());
    }

    #[test]
    fn cue_family_keys_match_json_format() {
        for cue in [
            CueFamily::Explicit,
            CueFamily::Structural,
            CueFamily::Behavioral,
            CueFamily::Recurrence,
        ] {
            let s = cue.as_str();
            // Used as JSON map key — must be a clean snake_case identifier.
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    // ── Evidence ref serde ────────────────────────────────────

    #[test]
    fn evidence_ref_chat_turn_round_trips() {
        let e = EvidenceRef::ChatTurn {
            session_id: "sess-123".into(),
            turn_id: "turn-456".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"type\":\"chat_turn\""));
        assert!(s.contains("\"session_id\":\"sess-123\""));
        let decoded: EvidenceRef = serde_json::from_str(&s).unwrap();
        assert_eq!(decoded, e);
    }

    #[test]
    fn evidence_ref_manual_with_note() {
        let e = EvidenceRef::Manual {
            note: "user said this directly".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"type\":\"manual\""));
        let decoded: EvidenceRef = serde_json::from_str(&s).unwrap();
        assert_eq!(decoded, e);
    }

    // ── LearningCandidate construction ───────────────────────

    #[test]
    fn learning_candidate_new_stamps_observed_at() {
        let c = mk_candidate(FacetClass::Tooling, "editor", "helix");
        assert!(c.observed_at_ms > 0);
        assert_eq!(c.confidence, 1.0);
        assert_eq!(c.class, FacetClass::Tooling);
    }

    #[test]
    fn learning_candidate_with_confidence_clamps_to_unit_interval() {
        let c1 = mk_candidate(FacetClass::Style, "v", "x").with_confidence(0.5);
        assert_eq!(c1.confidence, 0.5);
        let c2 = mk_candidate(FacetClass::Style, "v", "x").with_confidence(1.5);
        assert_eq!(c2.confidence, 1.0, "confidence > 1 must clamp down to 1");
        let c3 = mk_candidate(FacetClass::Style, "v", "x").with_confidence(-0.2);
        assert_eq!(c3.confidence, 0.0, "negative confidence must clamp up to 0");
    }

    // ── Buffer ────────────────────────────────────────────────

    #[test]
    fn buffer_push_then_drain_returns_in_fifo_order() {
        let buf = Buffer::new(10);
        buf.push(mk_candidate(FacetClass::Style, "verbosity", "terse"));
        buf.push(mk_candidate(FacetClass::Identity, "name", "Alice"));
        buf.push(mk_candidate(FacetClass::Tooling, "editor", "helix"));
        let drained = buf.drain();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].name, "verbosity");
        assert_eq!(drained[1].name, "name");
        assert_eq!(drained[2].name, "editor");
        assert!(buf.is_empty(), "drain should leave the buffer empty");
    }

    #[test]
    fn buffer_evicts_oldest_when_at_capacity() {
        let buf = Buffer::new(3);
        for i in 0..5 {
            buf.push(mk_candidate(
                FacetClass::Tooling,
                "editor",
                &format!("v{}", i),
            ));
        }
        // Capacity 3, pushed 5 → oldest 2 evicted.
        assert_eq!(buf.len(), 3);
        let drained = buf.drain();
        // Should be v2, v3, v4 (v0 and v1 evicted).
        assert_eq!(drained[0].value, "v2");
        assert_eq!(drained[1].value, "v3");
        assert_eq!(drained[2].value, "v4");
    }

    #[test]
    fn buffer_push_returns_true_on_eviction() {
        let buf = Buffer::new(2);
        assert_eq!(buf.push(mk_candidate(FacetClass::Style, "a", "1")), false);
        assert_eq!(buf.push(mk_candidate(FacetClass::Style, "a", "2")), false);
        // Third push evicts the first.
        assert_eq!(buf.push(mk_candidate(FacetClass::Style, "a", "3")), true);
    }

    #[test]
    fn buffer_with_zero_capacity_is_a_sink() {
        let buf = Buffer::new(0);
        buf.push(mk_candidate(FacetClass::Tooling, "x", "y"));
        buf.push(mk_candidate(FacetClass::Tooling, "x", "z"));
        assert!(buf.is_empty(), "capacity 0 buffer never retains");
        assert_eq!(buf.drain().len(), 0);
    }

    #[test]
    fn buffer_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;
        let buf = Arc::new(Buffer::new(100));
        let mut handles = vec![];
        for tid in 0..4 {
            let b = buf.clone();
            handles.push(thread::spawn(move || {
                for i in 0..10 {
                    b.push(mk_candidate(
                        FacetClass::Tooling,
                        &format!("t{}-{}", tid, i),
                        "v",
                    ));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            buf.len(),
            40,
            "4 threads × 10 pushes each = 40 entries in a capacity-100 buffer"
        );
    }
}
