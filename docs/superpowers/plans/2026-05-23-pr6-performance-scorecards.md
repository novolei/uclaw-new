# PR-6 Performance Scorecards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable, model-free performance scorecard infrastructure that can record latency, size, token, and resume metrics as harness JSON artifacts.

**Architecture:** PR-6 adds a derived-only `harness::performance_scorecard` module. It defines a stable JSON scorecard schema, deterministic percentile summaries, threshold verdicts, and a helper that attaches scorecards to existing `HarnessRuntime` artifacts. It does not optimize hot paths or claim speedups.

**Tech Stack:** Rust, serde, existing `HarnessRuntime`, existing `HarnessSubject`, existing harness artifact store, sibling Rust tests.

---

## ADR §18 Answers

1. **Intent:** Make performance evidence first-class before later runtime optimization PRs.
2. **Autonomy:** No autonomy behavior changes; scorecards only observe model-free measurements.
3. **Truth source:** Harness JSON artifacts are the scorecard evidence; runtime truth remains `TaskEvent` rollout JSONL and existing domain stores.
4. **TaskEvent entries:** None added in PR-6.
5. **Context:** Reads explicit measurement samples supplied by deterministic tests or future harness adapters. Evidence is cited by artifact path/id, suite id, source commit, generatedAt, case_id, and metric name.
6. **Capabilities:** No new capability cards; later PRs can use scorecards to gate capability promotion.
7. **Hooks:** No policy hooks changed.
8. **Projection:** Produces performance scorecard artifacts that future WorldProjection/UI surfaces can render.
9. **Harness:** Unit tests prove percentile math, threshold verdicts, JSON shape, and artifact attachment.
10. **Rollback:** Revert the new module/export/tests/docs. Generated JSON scorecard artifacts are disposable.
11. **Does not own:** Search replacement, DB pooling, stream supervisor, startup lazy loading, browser provider, team runtime, automation runtime, UI wiring, or CI gates.

## File Structure

- Create `src-tauri/src/harness/performance_scorecard.rs`: schema, summary math, threshold verdicts, artifact helper.
- Create `src-tauri/src/harness/performance_scorecard_tests.rs`: sibling tests; no inline test bodies.
- Modify `src-tauri/src/harness/mod.rs`: export the module and public types.
- Create `docs/superpowers/reports/performance-scorecards.md`: operating note for PR-6 scorecard semantics.
- Modify `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: mark PR-6 in progress and record impact/verification.

## Impact Summary

- `HarnessRuntime`: LOW impact; consumed by helper, not modified.
- `HarnessArtifactStore`: LOW impact; used through existing `HarnessRuntime::attach_json_artifact`, not modified.
- `ToolBudgetManager`: LOW impact; performance scorecards can measure output sizes later, but PR-6 does not modify it.
- `harness/mod.rs`: additive module export only.

## Task 1: Scorecard Schema and Summary Math

**Files:**
- Create: `src-tauri/src/harness/performance_scorecard.rs`
- Create: `src-tauri/src/harness/performance_scorecard_tests.rs`
- Modify: `src-tauri/src/harness/mod.rs`

- [x] **Step 1: Add sibling tests for percentile math and JSON shape**

Add tests that construct samples with known values:

```rust
let samples = vec![
    PerformanceSample::milliseconds("startup.visible_ready", 10.0),
    PerformanceSample::milliseconds("startup.visible_ready", 20.0),
    PerformanceSample::milliseconds("startup.visible_ready", 30.0),
    PerformanceSample::milliseconds("startup.visible_ready", 40.0),
];
let summary = PerformanceMetricSummary::from_samples("startup.visible_ready", &samples).unwrap();
assert_eq!(summary.sample_count, 4);
assert_eq!(summary.min, 10.0);
assert_eq!(summary.max, 40.0);
assert_eq!(summary.p50, 20.0);
assert_eq!(summary.p95, 40.0);
```

Also assert serde uses camelCase:

```rust
let value = serde_json::to_value(&summary).unwrap();
assert_eq!(value["sampleCount"], 4);
assert_eq!(value["metric"], "startup.visible_ready");
```

- [x] **Step 2: Record resumed-run red-test note**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
```

Expected: compile failure because `performance_scorecard` does not exist.

Execution note: this branch resumed after the module scaffold existed, so the
red-only compile failure was not rerun as a separate command. The sibling tests
were retained and validated in the final focused test run.

- [x] **Step 3: Implement scorecard types**

Implement:

```rust
pub const PERFORMANCE_SCORECARD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceVerdict {
    Pass,
    Warn,
    Fail,
}
```

Add:

- `PerformanceSample { metric, value, unit, lower_is_better }`
- `PerformanceMetricSummary { metric, unit, sample_count, min, max, avg, p50, p95, p99 }`
- `PerformanceThreshold { metric, warn_at, fail_at, lower_is_better }`
- `PerformanceCaseScore { case_id, subject, samples, summaries, verdict }`
- `PerformanceScorecard { schema_version, suite_id, generated_at, commit, artifact_kind, cases, summary }`
- `PerformanceScorecardSummary { case_count, pass_count, warn_count, fail_count, overall_verdict }`

- [x] **Step 4: Implement deterministic summary math**

Rules:

- Ignore samples with different metric names when summarizing a metric.
- Return `None` for empty metric sample sets.
- Percentiles use nearest-rank with zero-based index: `ceil(percentile * n) - 1`, clamped to `0..n-1`.
- Round nothing in Rust; preserve `f64` values.

- [x] **Step 5: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
```

Expected: scorecard schema tests pass.

## Task 2: Threshold Verdicts and Harness Artifact Attachment

**Files:**
- Modify: `src-tauri/src/harness/performance_scorecard.rs`
- Modify: `src-tauri/src/harness/performance_scorecard_tests.rs`

- [x] **Step 1: Add tests for verdicts**

Test pass/warn/fail thresholds:

```rust
let threshold = PerformanceThreshold::new("tool.search.latency_ms", 50.0, 100.0);
assert_eq!(PerformanceVerdict::from_value(25.0, &threshold), PerformanceVerdict::Pass);
assert_eq!(PerformanceVerdict::from_value(75.0, &threshold), PerformanceVerdict::Warn);
assert_eq!(PerformanceVerdict::from_value(125.0, &threshold), PerformanceVerdict::Fail);
```

Add a case score test:

```rust
let case = PerformanceCaseScore::from_samples(
    "search-small",
    HarnessSubject::Tools,
    samples,
    &[threshold],
);
assert_eq!(case.verdict, PerformanceVerdict::Warn);
```

- [x] **Step 2: Add test for harness artifact attachment**

Use `HarnessRuntime::new(temp.path())`, start an episode, attach a scorecard, and assert:

```rust
assert_eq!(artifact.kind, "performance_scorecard");
assert!(std::fs::read_to_string(&artifact.path).unwrap().contains("performance_scorecard"));
```

- [x] **Step 3: Implement verdict helpers**

Implement:

- `PerformanceVerdict::from_value(value, threshold)`
- `PerformanceVerdict::combine(iter)`
- `PerformanceCaseScore::from_samples(case_id, subject, samples, thresholds)`
- `PerformanceScorecard::new(suite_id, commit, cases)`

Use the worst verdict ordering: `Fail > Warn > Pass`.

- [x] **Step 4: Implement artifact helper**

Add:

```rust
pub fn attach_performance_scorecard(
    runtime: &HarnessRuntime,
    run_id: &str,
    scorecard: &PerformanceScorecard,
) -> Result<Option<HarnessArtifact>, ArtifactStoreError>
```

The helper calls `runtime.attach_json_artifact(run_id, "performance_scorecard", &serde_json::to_value(scorecard)?)`.

- [x] **Step 5: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
```

Expected: all performance scorecard tests pass.

## Task 3: Docs, Status, and Final Verification

**Files:**
- Create: `docs/superpowers/reports/performance-scorecards.md`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Modify: `docs/superpowers/plans/2026-05-23-pr6-performance-scorecards.md`

- [x] **Step 1: Write the scorecard operating note**

Document:

- PR-6 defines the scorecard substrate only.
- It is model-free and deterministic.
- It supports startup, tools, browser, team, automation, projection, and token-budget metrics later.
- It must not be used to claim performance improvements without before/after benchmarks.

- [x] **Step 2: Update the status ledger**

Mark PR-6 in progress on branch `codex/agent-os-jcode-pr6-performance-scorecards`, with impact notes and verification commands.

- [x] **Step 3: Run final verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::artifacts
git diff --check -- docs/superpowers/plans/2026-05-23-pr6-performance-scorecards.md docs/superpowers/reports/performance-scorecards.md docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md src-tauri/src/harness/mod.rs src-tauri/src/harness/performance_scorecard.rs src-tauri/src/harness/performance_scorecard_tests.rs
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr6-performance-scorecards
```

Expected:

- performance scorecard tests pass;
- existing harness runtime/artifact tests pass;
- diff checks pass;
- GitNexus detect reports no unexpected HIGH/CRITICAL risk.

## Self-Review

- Spec coverage: PR-6 covers the scorecard substrate required before later benchmark campaigns.
- Placeholder scan: no TBD/TODO placeholders.
- Type consistency: all names are defined in this plan before use.
