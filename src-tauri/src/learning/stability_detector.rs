//! Stability detector — Sprint 1.3 of the openhuman warm-start port.
//!
//! Pure-function half-life-decayed stability scorer + state-transition
//! engine. Takes a list of [`LearningCandidate`]s (drained from
//! [`crate::learning::candidate::Buffer`]) plus a snapshot of the
//! current `user_profile_facets` rows; produces a list of
//! [`FacetTransition`]s the caller (Sprint 1.4 scheduler) writes to DB.
//!
//! ## Stability formula
//!
//! ```text
//! stability(class, name) = base × cue_mult
//!
//! base      = Σ(cue_family.weight × exp(-Δt / half_life(class)) × ln(1 + evidence_count))
//! cue_mult  = 2.0 if any contributing evidence was CueFamily::Explicit
//!             else 1.0
//! ```
//!
//! No `UserState` override yet (openhuman has Pinned/Forgotten for
//! human-in-the-loop control — we'll add when Phase 13 review queue
//! lands).
//!
//! ## Thresholds
//!
//! | Symbol             | Value | State transition                       |
//! |---|---|---|
//! | TAU_PROMOTE        | 1.5   | enter / stay Active                    |
//! | TAU_PROVISIONAL    | 0.7   | enter / stay Provisional               |
//! | TAU_EVICT          | 0.4   | retain as Candidate (below → Forgotten)|
//!
//! ## Class budgets
//!
//! After threshold-based state assignment, each [`FacetClass`] is
//! trimmed: only the top-K Active rows by stability survive (K =
//! `class.budget()`); the rest get demoted to Provisional. Per
//! [`candidate::FacetClass::budget`] —
//! `Style=4, Identity=4, Tooling=5, Veto=3, Goal=3, Channel=1`.
//!
//! ## Reference
//!
//! openhuman source:
//! `/Users/ryanliu/Documents/openhuman/src/openhuman/learning/stability_detector.rs:1-94`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::learning::candidate::{CueFamily, FacetClass, LearningCandidate};

// ─── Thresholds ────────────────────────────────────────────────────────

/// Score >= this enters / stays Active.
pub const TAU_PROMOTE: f64 = 1.5;
/// Score in [TAU_PROVISIONAL, TAU_PROMOTE) enters / stays Provisional.
pub const TAU_PROVISIONAL: f64 = 0.7;
/// Score in [TAU_EVICT, TAU_PROVISIONAL) retains as Candidate.
/// Below `TAU_EVICT` → Forgotten.
pub const TAU_EVICT: f64 = 0.4;

/// Multiplier applied when any contributing evidence row has
/// `CueFamily::Explicit`. Doubles the score so a single explicit
/// "I prefer X" beats many weaker structural / behavioral signals.
pub const EXPLICIT_BOOST: f64 = 2.0;

// ─── FacetState ────────────────────────────────────────────────────────

/// Lifecycle state stored in `user_profile_facets.state`. Promoted by
/// [`StabilityDetector::rebuild`] when score crosses [`TAU_PROVISIONAL`]
/// / [`TAU_PROMOTE`]; demoted to Forgotten when below [`TAU_EVICT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacetState {
    /// New evidence; not yet promoted. Score in `[TAU_EVICT, TAU_PROVISIONAL)`.
    Candidate,
    /// Score crossed `TAU_PROVISIONAL` but is below `TAU_PROMOTE`,
    /// OR class budget bumped a stronger candidate down.
    Provisional,
    /// Score >= `TAU_PROMOTE` AND fits within class budget. Rendered
    /// in the system prompt.
    Active,
    /// Score < `TAU_EVICT`. Hidden from the prompt. Kept in the table
    /// for audit; not deleted (consistent with our Phase 4 health
    /// findings dedup contract).
    Forgotten,
}

impl FacetState {
    pub fn as_str(self) -> &'static str {
        match self {
            FacetState::Candidate => "candidate",
            FacetState::Provisional => "provisional",
            FacetState::Active => "active",
            FacetState::Forgotten => "forgotten",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => FacetState::Active,
            "provisional" => FacetState::Provisional,
            "forgotten" => FacetState::Forgotten,
            _ => FacetState::Candidate,
        }
    }
}

/// Map a stability score to a candidate state (pre-budget).
pub fn state_for_score(score: f64) -> FacetState {
    if score >= TAU_PROMOTE {
        FacetState::Active
    } else if score >= TAU_PROVISIONAL {
        FacetState::Provisional
    } else if score >= TAU_EVICT {
        FacetState::Candidate
    } else {
        FacetState::Forgotten
    }
}

// ─── Stability formula ─────────────────────────────────────────────────

/// Cue-family weights summed across evidence rows. Keyed by string
/// (matches the `cue_families_json` column shape on disk).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CueWeights {
    pub explicit: f64,
    pub structural: f64,
    pub behavioral: f64,
    pub recurrence: f64,
}

impl CueWeights {
    pub fn add(&mut self, cue: CueFamily, weight: f64) {
        match cue {
            CueFamily::Explicit => self.explicit += weight,
            CueFamily::Structural => self.structural += weight,
            CueFamily::Behavioral => self.behavioral += weight,
            CueFamily::Recurrence => self.recurrence += weight,
        }
    }

    pub fn sum(&self) -> f64 {
        self.explicit + self.structural + self.behavioral + self.recurrence
    }

    pub fn has_explicit(&self) -> bool {
        self.explicit > 0.0
    }

    /// JSON-compatible map for persisting into `cue_families_json`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "explicit":    self.explicit,
            "structural":  self.structural,
            "behavioral":  self.behavioral,
            "recurrence":  self.recurrence,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        let g = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
        Self {
            explicit: g("explicit"),
            structural: g("structural"),
            behavioral: g("behavioral"),
            recurrence: g("recurrence"),
        }
    }
}

/// Compute the stability score for a `(class, name)` aggregate.
///
/// `evidence_count` is the total number of contributing candidate rows
/// (used in the `ln(1 + n)` term — bounded growth so 1000 weak signals
/// don't outweigh 5 explicit ones).
///
/// `last_seen_ms` is the most recent evidence timestamp; `now_ms` is
/// wall-clock now. The decay term is `exp(-Δt/half_life)` where
/// `half_life` is class-specific (7-90 days, see
/// [`FacetClass::half_life_days`]).
pub fn stability(
    class: FacetClass,
    weights: &CueWeights,
    evidence_count: u32,
    last_seen_ms: i64,
    now_ms: i64,
) -> f64 {
    if evidence_count == 0 {
        return 0.0;
    }
    let dt_days = ((now_ms.saturating_sub(last_seen_ms)) as f64) / (86_400_000.0);
    let recency = (-dt_days / class.half_life_days()).exp();
    let log_term = ((1.0 + evidence_count as f64).ln()).max(0.0);
    let base = weights.sum() * recency * log_term;
    let cue_mult = if weights.has_explicit() {
        EXPLICIT_BOOST
    } else {
        1.0
    };
    base * cue_mult
}

// ─── Snapshots + transitions ───────────────────────────────────────────

/// One row from `user_profile_facets` as read by the rebuild pass. The
/// detector consumes a `Vec<FacetSnapshot>` plus the drained candidate
/// buffer and outputs [`FacetTransition`]s.
#[derive(Debug, Clone)]
pub struct FacetSnapshot {
    pub facet_id: String,
    pub class: FacetClass,
    pub name: String,
    pub value: String,
    pub state: FacetState,
    pub stability: f64,
    pub cue_weights: CueWeights,
    pub evidence_count: u32,
    pub last_seen_ms: i64,
}

/// The write-back the rebuild pass produces. Caller (Sprint 1.4
/// scheduler) translates each into an `INSERT OR UPDATE` against
/// `user_profile_facets`.
#[derive(Debug, Clone, PartialEq)]
pub struct FacetTransition {
    pub facet_id: String,
    pub class: FacetClass,
    pub name: String,
    /// New value (may differ from previous on value drift — last writer wins).
    pub value: String,
    pub new_state: FacetState,
    pub new_stability: f64,
    pub new_cue_weights: CueWeights,
    pub new_evidence_count: u32,
    pub new_last_seen_ms: i64,
}

/// Summary of one rebuild pass — caller logs at info-level for
/// telemetry. Mirrors openhuman's RebuildOutcome shape.
#[derive(Debug, Clone, Default)]
pub struct RebuildOutcome {
    /// Facets newly entering Active state this pass.
    pub promoted_to_active: u32,
    /// Facets newly entering Provisional state.
    pub promoted_to_provisional: u32,
    /// Facets demoted Active → Provisional due to class budget.
    pub demoted_for_budget: u32,
    /// Facets newly entering Forgotten (score < TAU_EVICT).
    pub forgotten: u32,
    /// Facets that did not change state (carried over).
    pub unchanged: u32,
    /// Total facet rows after the rebuild.
    pub total: u32,
}

// ─── Detector ──────────────────────────────────────────────────────────

/// The stability detector itself. Stateless — the I/O caller passes
/// in snapshots and gets back transitions.
pub struct StabilityDetector;

impl StabilityDetector {
    /// One rebuild pass.
    ///
    /// Algorithm:
    /// 1. Aggregate candidates by `(class, name)` into intermediate
    ///    accumulators carrying summed cue weights + last_seen +
    ///    evidence_count.
    /// 2. Merge those accumulators with the existing `FacetSnapshot`
    ///    rows — same key combines weights additively, value drift
    ///    is last-writer-wins (most recent observed_at_ms).
    /// 3. Score each aggregate via [`stability`].
    /// 4. Pre-budget state via [`state_for_score`].
    /// 5. Enforce per-class budget: at most `class.budget()` Active
    ///    rows survive; the rest demote to Provisional.
    /// 6. Emit one [`FacetTransition`] per (class, name) — including
    ///    no-op transitions for facets that didn't change.
    pub fn rebuild(
        existing: Vec<FacetSnapshot>,
        candidates: Vec<LearningCandidate>,
        now_ms: i64,
    ) -> (Vec<FacetTransition>, RebuildOutcome) {
        // (class, name) → accumulator
        struct Acc {
            facet_id: String,
            value: String,
            old_state: FacetState,
            weights: CueWeights,
            evidence_count: u32,
            last_seen_ms: i64,
        }
        let mut acc: HashMap<(FacetClass, String), Acc> = HashMap::new();

        // Seed from existing snapshots so untouched facets keep flowing.
        for snap in existing {
            acc.insert(
                (snap.class, snap.name.clone()),
                Acc {
                    facet_id: snap.facet_id,
                    value: snap.value,
                    old_state: snap.state,
                    weights: snap.cue_weights,
                    evidence_count: snap.evidence_count,
                    last_seen_ms: snap.last_seen_ms,
                },
            );
        }

        // Fold each new candidate in. Same key = combine.
        for cand in candidates {
            let key = (cand.class, cand.name.clone());
            let entry = acc.entry(key.clone()).or_insert_with(|| Acc {
                facet_id: new_facet_id(cand.class, &cand.name),
                value: cand.value.clone(),
                old_state: FacetState::Candidate,
                weights: CueWeights::default(),
                evidence_count: 0,
                last_seen_ms: 0,
            });
            // Cue family weight × producer confidence.
            entry
                .weights
                .add(cand.cue, cand.cue.weight() * cand.confidence);
            entry.evidence_count = entry.evidence_count.saturating_add(1);
            // Value drift = last-writer-wins.
            if cand.observed_at_ms > entry.last_seen_ms {
                entry.value = cand.value;
                entry.last_seen_ms = cand.observed_at_ms;
            }
        }

        // Score + pre-budget state.
        struct Scored {
            facet_id: String,
            class: FacetClass,
            name: String,
            value: String,
            old_state: FacetState,
            score: f64,
            weights: CueWeights,
            evidence_count: u32,
            last_seen_ms: i64,
        }
        let mut scored: Vec<Scored> = acc
            .into_iter()
            .map(|((class, name), a)| {
                let score = stability(class, &a.weights, a.evidence_count, a.last_seen_ms, now_ms);
                Scored {
                    facet_id: a.facet_id,
                    class,
                    name,
                    value: a.value,
                    old_state: a.old_state,
                    score,
                    weights: a.weights,
                    evidence_count: a.evidence_count,
                    last_seen_ms: a.last_seen_ms,
                }
            })
            .collect();

        // Per-class budget enforcement: sort descending by score within
        // each class, mark top-K with score >= TAU_PROMOTE as Active,
        // the rest of those crossing TAU_PROVISIONAL → Provisional.
        let mut by_class: HashMap<FacetClass, Vec<usize>> = HashMap::new();
        for (i, s) in scored.iter().enumerate() {
            by_class.entry(s.class).or_default().push(i);
        }

        let mut state_for: HashMap<usize, FacetState> = HashMap::new();
        let mut budget_demotions: u32 = 0;
        for (class, indices) in by_class {
            let budget = class.budget();
            // Sort indices by score DESC.
            let mut idx = indices.clone();
            idx.sort_by(|a, b| scored[*b].score.partial_cmp(&scored[*a].score).unwrap_or(std::cmp::Ordering::Equal));
            let mut active_taken = 0usize;
            for i in idx {
                let s = &scored[i];
                let raw = state_for_score(s.score);
                let final_state = match raw {
                    FacetState::Active => {
                        if active_taken < budget {
                            active_taken += 1;
                            FacetState::Active
                        } else {
                            budget_demotions += 1;
                            FacetState::Provisional
                        }
                    }
                    other => other,
                };
                state_for.insert(i, final_state);
            }
        }

        // Emit transitions + count outcome.
        let mut outcome = RebuildOutcome::default();
        let mut transitions: Vec<FacetTransition> = Vec::with_capacity(scored.len());
        for (i, s) in scored.into_iter().enumerate() {
            let new_state = state_for.get(&i).copied().unwrap_or(FacetState::Candidate);
            // Stats.
            outcome.total += 1;
            match (s.old_state, new_state) {
                (a, b) if a == b => outcome.unchanged += 1,
                (_, FacetState::Active) => outcome.promoted_to_active += 1,
                (_, FacetState::Provisional) => outcome.promoted_to_provisional += 1,
                (_, FacetState::Forgotten) => outcome.forgotten += 1,
                _ => {}
            }
            transitions.push(FacetTransition {
                facet_id: s.facet_id,
                class: s.class,
                name: s.name,
                value: s.value,
                new_state,
                new_stability: s.score,
                new_cue_weights: s.weights,
                new_evidence_count: s.evidence_count,
                new_last_seen_ms: s.last_seen_ms,
            });
        }
        outcome.demoted_for_budget = budget_demotions;
        (transitions, outcome)
    }
}

/// Deterministic-ish facet id for a new (class, name) tuple. Uses
/// uuid v4 so cardinality is unbounded; collisions are mathematically
/// impossible at our scale.
fn new_facet_id(class: FacetClass, name: &str) -> String {
    // class+name as a memo would be nicer but UUID is collision-free
    // and the (class, name) UNIQUE index in V39 enforces uniqueness
    // anyway.
    let _ = (class, name); // suppress unused warning while uuid is the chosen approach
    uuid::Uuid::new_v4().to_string()
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::candidate::{CueFamily, EvidenceRef};

    fn cand(
        class: FacetClass,
        name: &str,
        value: &str,
        cue: CueFamily,
        observed_at_ms: i64,
    ) -> LearningCandidate {
        let mut c = LearningCandidate::new(
            class,
            name,
            value,
            cue,
            EvidenceRef::Manual { note: "t".into() },
        );
        c.observed_at_ms = observed_at_ms;
        c
    }

    const ONE_DAY_MS: i64 = 86_400_000;

    // ─── Threshold + state mapping ────────────────────────────────

    #[test]
    fn state_for_score_thresholds_match_constants() {
        assert_eq!(state_for_score(2.0), FacetState::Active);
        assert_eq!(state_for_score(TAU_PROMOTE), FacetState::Active);
        assert_eq!(state_for_score(1.4), FacetState::Provisional);
        assert_eq!(state_for_score(TAU_PROVISIONAL), FacetState::Provisional);
        assert_eq!(state_for_score(0.6), FacetState::Candidate);
        assert_eq!(state_for_score(TAU_EVICT), FacetState::Candidate);
        assert_eq!(state_for_score(0.3), FacetState::Forgotten);
        assert_eq!(state_for_score(0.0), FacetState::Forgotten);
    }

    #[test]
    fn facet_state_string_roundtrip() {
        for s in [
            FacetState::Candidate,
            FacetState::Provisional,
            FacetState::Active,
            FacetState::Forgotten,
        ] {
            assert_eq!(FacetState::from_str(s.as_str()), s);
        }
        // Unknown defaults to Candidate.
        assert_eq!(FacetState::from_str("garbage"), FacetState::Candidate);
    }

    // ─── CueWeights ───────────────────────────────────────────────

    #[test]
    fn cue_weights_add_accumulates_by_family() {
        let mut w = CueWeights::default();
        w.add(CueFamily::Explicit, 1.0);
        w.add(CueFamily::Explicit, 0.5);
        w.add(CueFamily::Behavioral, 0.7);
        assert_eq!(w.explicit, 1.5);
        assert_eq!(w.behavioral, 0.7);
        assert_eq!(w.structural, 0.0);
        assert!(w.has_explicit());
        assert!((w.sum() - 2.2).abs() < 1e-9);
    }

    #[test]
    fn cue_weights_json_roundtrip() {
        let mut w = CueWeights::default();
        w.add(CueFamily::Structural, 0.9);
        w.add(CueFamily::Recurrence, 1.2);
        let j = w.to_json();
        let parsed = CueWeights::from_json(&j);
        assert!((parsed.structural - 0.9).abs() < 1e-9);
        assert!((parsed.recurrence - 1.2).abs() < 1e-9);
        assert!(!parsed.has_explicit());
    }

    // ─── stability formula ────────────────────────────────────────

    #[test]
    fn stability_zero_evidence_is_zero() {
        let w = CueWeights::default();
        assert_eq!(stability(FacetClass::Tooling, &w, 0, 0, 0), 0.0);
    }

    #[test]
    fn stability_decays_with_age() {
        // Same evidence count, same weights — older facet should score lower.
        let mut w = CueWeights::default();
        w.add(CueFamily::Explicit, 1.0);
        let now = 100 * ONE_DAY_MS;
        let fresh = stability(FacetClass::Tooling, &w, 5, now - ONE_DAY_MS, now);
        let stale = stability(FacetClass::Tooling, &w, 5, now - 60 * ONE_DAY_MS, now);
        assert!(
            fresh > stale,
            "fresher evidence must score higher: fresh={}, stale={}",
            fresh,
            stale
        );
    }

    #[test]
    fn stability_identity_decays_slower_than_channel() {
        // Half-life: Identity=90d, Channel=7d. Same input, Identity > Channel after 30 days.
        let mut w = CueWeights::default();
        w.add(CueFamily::Explicit, 1.0);
        let now = 100 * ONE_DAY_MS;
        let identity = stability(FacetClass::Identity, &w, 3, now - 30 * ONE_DAY_MS, now);
        let channel = stability(FacetClass::Channel, &w, 3, now - 30 * ONE_DAY_MS, now);
        assert!(
            identity > channel,
            "Identity must decay slower than Channel: id={}, ch={}",
            identity,
            channel
        );
    }

    #[test]
    fn stability_explicit_doubles_score() {
        let mut explicit = CueWeights::default();
        explicit.add(CueFamily::Explicit, 1.0);
        let mut structural = CueWeights::default();
        // Same TOTAL weight but no explicit.
        structural.add(CueFamily::Structural, 1.0);
        let now = 0i64;
        let s_explicit = stability(FacetClass::Tooling, &explicit, 1, 0, now);
        let s_structural = stability(FacetClass::Tooling, &structural, 1, 0, now);
        assert!(
            (s_explicit - 2.0 * s_structural).abs() < 1e-9,
            "explicit boost should be 2.0×: expl={}, struct={}",
            s_explicit,
            s_structural
        );
    }

    #[test]
    fn stability_grows_with_evidence_count_but_sublinearly() {
        let mut w = CueWeights::default();
        w.add(CueFamily::Explicit, 1.0);
        let one = stability(FacetClass::Tooling, &w, 1, 0, 0);
        let ten = stability(FacetClass::Tooling, &w, 10, 0, 0);
        let hundred = stability(FacetClass::Tooling, &w, 100, 0, 0);
        // log growth: 10 should not be 10× 1, hundred should not be 10× ten.
        assert!(ten > one);
        assert!(hundred > ten);
        assert!(
            ten / one < 5.0,
            "log growth: 10× evidence should be <5× score, got ratio {}",
            ten / one
        );
    }

    // ─── End-to-end rebuild ───────────────────────────────────────

    #[test]
    fn rebuild_promotes_explicit_to_active() {
        // One explicit, recent candidate — should land Active.
        let now = 100 * ONE_DAY_MS;
        let candidates = vec![cand(
            FacetClass::Tooling,
            "editor",
            "helix",
            CueFamily::Explicit,
            now - 100, // 100ms ago
        )];
        let (transitions, outcome) =
            StabilityDetector::rebuild(vec![], candidates, now);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].new_state, FacetState::Active);
        assert!(transitions[0].new_stability >= TAU_PROMOTE);
        assert_eq!(outcome.promoted_to_active, 1);
        assert_eq!(outcome.total, 1);
    }

    #[test]
    fn rebuild_demotes_to_forgotten_below_tau_evict() {
        // Single behavioral signal from 100 days ago → exp decay kills it.
        let now = 200 * ONE_DAY_MS;
        let candidates = vec![cand(
            FacetClass::Channel,
            "platform",
            "slack",
            CueFamily::Behavioral,
            now - 100 * ONE_DAY_MS,
        )];
        let (transitions, outcome) =
            StabilityDetector::rebuild(vec![], candidates, now);
        assert_eq!(transitions[0].new_state, FacetState::Forgotten);
        assert_eq!(outcome.forgotten, 1);
    }

    #[test]
    fn rebuild_same_key_combines_evidence() {
        // Three behavioral signals for the same fact → score crosses
        // TAU_PROVISIONAL even though individually each is weak.
        let now = 0i64;
        let candidates = vec![
            cand(FacetClass::Style, "verbosity", "terse", CueFamily::Behavioral, now),
            cand(FacetClass::Style, "verbosity", "terse", CueFamily::Behavioral, now),
            cand(FacetClass::Style, "verbosity", "terse", CueFamily::Behavioral, now),
            cand(FacetClass::Style, "verbosity", "terse", CueFamily::Behavioral, now),
        ];
        let (transitions, _) = StabilityDetector::rebuild(vec![], candidates, now);
        assert_eq!(transitions.len(), 1, "combined into one (class, name)");
        assert_eq!(transitions[0].new_evidence_count, 4);
        assert!(
            transitions[0].new_stability >= TAU_PROVISIONAL,
            "4 behavioral signals should at least reach provisional, got {}",
            transitions[0].new_stability
        );
    }

    #[test]
    fn rebuild_enforces_class_budget_by_demoting_to_provisional() {
        // Channel budget is 1 — 2 strong candidates → 1 Active + 1 Provisional.
        let now = 0i64;
        let candidates = vec![
            cand(FacetClass::Channel, "primary", "wechat", CueFamily::Explicit, now),
            cand(FacetClass::Channel, "primary", "wechat", CueFamily::Explicit, now),
            cand(FacetClass::Channel, "primary", "wechat", CueFamily::Explicit, now),
            cand(FacetClass::Channel, "secondary", "slack", CueFamily::Explicit, now),
            cand(FacetClass::Channel, "secondary", "slack", CueFamily::Explicit, now),
        ];
        let (transitions, outcome) = StabilityDetector::rebuild(vec![], candidates, now);
        let actives: Vec<_> = transitions
            .iter()
            .filter(|t| t.new_state == FacetState::Active)
            .collect();
        let provisionals: Vec<_> = transitions
            .iter()
            .filter(|t| t.new_state == FacetState::Provisional)
            .collect();
        assert_eq!(actives.len(), 1, "Channel budget is 1");
        assert_eq!(provisionals.len(), 1, "Other promotion demoted by budget");
        assert_eq!(outcome.demoted_for_budget, 1);
        // The winner is the higher-stability candidate ("primary" has 3 evidence rows).
        assert_eq!(actives[0].name, "primary");
    }

    #[test]
    fn rebuild_value_drift_uses_last_writer_wins() {
        // Same (class, name), different value over time — final value
        // should be the most recently observed.
        let now = 100 * ONE_DAY_MS;
        let candidates = vec![
            cand(FacetClass::Tooling, "editor", "vscode", CueFamily::Explicit, now - 10 * ONE_DAY_MS),
            cand(FacetClass::Tooling, "editor", "helix", CueFamily::Explicit, now - 1 * ONE_DAY_MS),
        ];
        let (transitions, _) = StabilityDetector::rebuild(vec![], candidates, now);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].value, "helix", "newer observation wins");
        assert_eq!(transitions[0].new_evidence_count, 2);
    }

    #[test]
    fn rebuild_carries_unchanged_facets() {
        // Pre-existing Active facet, no new candidates → stays Active.
        let now = 0i64;
        let existing = vec![FacetSnapshot {
            facet_id: "f1".into(),
            class: FacetClass::Identity,
            name: "name".into(),
            value: "Alice".into(),
            state: FacetState::Active,
            stability: 2.5,
            cue_weights: {
                let mut w = CueWeights::default();
                w.add(CueFamily::Explicit, 1.0);
                w
            },
            evidence_count: 3,
            last_seen_ms: now - ONE_DAY_MS,
        }];
        let (transitions, outcome) =
            StabilityDetector::rebuild(existing, vec![], now);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].new_state, FacetState::Active);
        assert_eq!(outcome.unchanged, 1);
    }

    #[test]
    fn rebuild_new_candidate_adds_to_existing_facet() {
        // Pre-existing Provisional + new strong candidate → may promote to Active.
        let now = 0i64;
        let existing = vec![FacetSnapshot {
            facet_id: "f1".into(),
            class: FacetClass::Tooling,
            name: "editor".into(),
            value: "helix".into(),
            state: FacetState::Provisional,
            stability: 1.0,
            cue_weights: {
                let mut w = CueWeights::default();
                w.add(CueFamily::Behavioral, 0.7);
                w
            },
            evidence_count: 2,
            last_seen_ms: now - 1000,
        }];
        let candidates = vec![cand(
            FacetClass::Tooling,
            "editor",
            "helix",
            CueFamily::Explicit,
            now,
        )];
        let (transitions, _) = StabilityDetector::rebuild(existing, candidates, now);
        assert_eq!(transitions.len(), 1);
        assert_eq!(
            transitions[0].new_state,
            FacetState::Active,
            "explicit cue + existing evidence should promote to active"
        );
        assert_eq!(transitions[0].new_evidence_count, 3, "2 existing + 1 new");
    }

    #[test]
    fn rebuild_returns_total_count_matching_transition_count() {
        // Sanity: outcome.total == transitions.len() always.
        let now = 0i64;
        let candidates = vec![
            cand(FacetClass::Style, "a", "x", CueFamily::Explicit, now),
            cand(FacetClass::Tooling, "b", "y", CueFamily::Explicit, now),
            cand(FacetClass::Identity, "c", "z", CueFamily::Explicit, now),
        ];
        let (transitions, outcome) = StabilityDetector::rebuild(vec![], candidates, now);
        assert_eq!(outcome.total as usize, transitions.len());
        assert_eq!(outcome.total, 3);
    }
}
