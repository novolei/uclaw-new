# Evidence-Gated Eval TDD Plan

> **For agentic workers:** Execute this plan step by step. Keep the first
> slice additive and fail-closed.

**Goal:** Add a Deep Module that turns eval traces and artifacts into a
machine-readable evidence verdict.

**Spec:** `docs/superpowers/specs/2026-06-01-evidence-gated-eval-design.md`

## Recon Notes

- uClaw campaign cases already carry `required_event_kinds` and
  `required_artifacts`.
- `EvalEpisode` already stores trace events and attached artifacts.
- `EvalEvent::kind()` is the stable event-kind Interface.
- Pi's validation broker and conformance modules use schema-tagged evidence,
  classified failures, and no-data fail-closed decisions.
- GitNexus impact for `EvalRuntime`, `EvalEpisode`, and `EvalArtifact`
  returned UNKNOWN because the symbols were not resolved in the index. This
  slice adds a new Module and consumes public Interfaces instead of changing
  those structs.

## Files

- Create: `src-tauri/src/eval/evidence.rs`
- Modify: `src-tauri/src/eval/mod.rs`
- Update: `docs/superpowers/plans/2026-05-31-pi-modernization-six-modules.md`

## Steps

- [x] **Step 1: Add failing evidence-gate tests**

Add tests in the new `evidence.rs` module proving:

1. complete event and artifact evidence passes;
2. missing evidence fails closed with missing records;
3. empty requirements fail closed;
4. reports attach as eval artifacts.

Run:

```bash
cargo test --lib eval::evidence -- --nocapture
```

Observed before implementation: compilation failed because the Module types and
functions did not exist.

- [x] **Step 2: Implement evidence schema and gate**

Add:

- `EvalEvidenceGateVerdict`;
- `EvalEvidenceCheckStatus`;
- `EvalEvidenceRequirement`;
- `EvalEvidenceRecord`;
- `EvalEvidenceGateReport`;
- `gate_eval_evidence`.

Use sorted, deduplicated observed and missing kind lists for stable output.

- [x] **Step 3: Implement artifact attachment**

Add `attach_eval_evidence_report` that writes the report via
`EvalRuntime::attach_json_artifact` with kind `eval_evidence_report`.

- [x] **Step 4: Export the Module and verify**

Export the evidence types from `eval::mod`, then run:

```bash
cargo test --lib eval::evidence -- --nocapture
git diff --check
```

Observed: all evidence tests passed and whitespace check had no output.

- [x] **Step 5: Run GitNexus detect-changes and commit**

Stage only Evidence-Gated Eval files and run GitNexus `detect_changes` on
staged changes. Commit with verification output in the commit body.

Observed before commit: GitNexus `detect_changes` on staged files reported
`risk_level: none`.

## Continuation Slice: Manifest and Local Gate Command

The parent program spec requires a scenario/evidence manifest parser and a
local/CI gate that fails closed when required evidence is missing. The first
slice added the in-memory gate only. This continuation adds the file-based
Interface that CI can call without running live adapters.

- [x] **Step 1: Add failing manifest and local gate tests**

Add tests proving:

- A manifest JSON parses per-case required event and artifact kinds.
- A file-based local gate returns exit code `1` and writes a report when an
  episode misses required evidence.

Observed RED:

```text
error[E0433]: cannot find type `EvalEvidenceManifest` in this scope
error[E0425]: cannot find function `run_eval_evidence_gate_files` in this scope
```

- [x] **Step 2: Implement manifest parser and gate runner**

Add an `eval::evidence_gate` Module with:

- `EvalEvidenceManifest`
- `EvalEvidenceManifestCase`
- `gate_eval_evidence_manifest`
- `run_eval_evidence_gate_files`

- [x] **Step 3: Add local command adapter**

Add `src-tauri/src/bin/eval-evidence-gate.rs` so CI can run:

```bash
cargo run --bin eval-evidence-gate -- --manifest <manifest.json> --episode <episode.json> --report <report.json>
```

The command exits `0` for pass, `1` for fail-closed, and `2` for usage/IO/JSON
errors.

- [x] **Step 4: Verify and commit**

Run:

```bash
cargo test --lib eval::evidence -- --nocapture
cargo test --lib eval::evidence_gate -- --nocapture
cargo test --bin eval-evidence-gate -- --nocapture
git diff --check
```

Then run GitNexus `detect_changes(scope: "staged")` and commit.

Observed GREEN before commit:

- `cargo test --lib eval::evidence_gate -- --nocapture`: 3 passed.
- `cargo test --lib eval::evidence -- --nocapture`: 7 passed.
- `cargo test --bin eval-evidence-gate -- --nocapture`: binary compiled, 0
  tests.
- `git diff --check`: no output.
