# Performance Scorecards

Status: PR-6 substrate.

PR-6 adds the model-free performance scorecard schema used by future benchmark
campaigns. It does not claim a speedup and does not change search, database,
streaming, startup, browser, team, or automation runtime behavior.

The scorecard artifact kind is `performance_scorecard`. Each artifact records:

- schema version;
- suite id;
- source commit;
- generated timestamp;
- case scores by `HarnessSubject`;
- raw samples;
- per-metric min/max/avg/p50/p95/p99 summaries;
- pass/warn/fail threshold verdicts.

The first supported metric families are intentionally generic:

- `startup.*.latency_ms`;
- `tool.*.latency_ms`;
- `tool.*.output_bytes`;
- `projection.*.latency_ms`;
- `browser.*.latency_ms`;
- `team.*.latency_ms`;
- `automation.*.latency_ms`;
- `token.*.tokens`.

Scorecards are evidence containers, not performance claims. A PR may claim an
optimization only when it includes a before/after benchmark using the same
suite id, machine context, feature flags, and threshold policy.

Recommended focused verification:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::artifacts
```
