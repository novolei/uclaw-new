# Evidence-Gated Eval Module Design

**Date:** 2026-06-01
**Status:** Evidence gate implemented; manifest/local command continuation in progress
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi_agent_rust/src/validation_broker.rs`, `/Users/ryanliu/Documents/pi_agent_rust/src/conformance_shapes.rs`, `/Users/ryanliu/Documents/pi_agent_rust/src/extension_validation.rs`

## Problem

uClaw already has an eval runtime, campaign manifests, artifacts, traces,
graders, and performance scorecards. The seam is still shallow when a campaign
or adapter needs to prove readiness: required event kinds and artifact kinds
are described in several places, but there is no small Module that turns those
requirements into a machine-readable evidence verdict.

That makes promotion claims easy to state but harder to gate. A missing
artifact can be interpreted by custom caller logic instead of failing closed at
one Interface.

## Goal

Add an `EvalEvidence` Module that normalizes verification claims into one
schema and fail-closed gate:

```text
EvalEpisode + evidence requirements
  -> gate_eval_evidence(...)
       -> observed event kinds
       -> observed artifact kinds
       -> missing requirements
       -> pass/fail-closed verdict
  -> attach_eval_evidence_report(...)
```

Adapters, campaigns, and future CI commands should be able to emit or attach
the same evidence report without learning each other's trace details.

## Current uClaw Truth

- `src-tauri/src/eval/runtime.rs` owns episode lifecycle and artifact
  attachment.
- `src-tauri/src/eval/campaign.rs` already records `required_event_kinds` and
  `required_artifacts` on campaign cases.
- `src-tauri/src/eval/episode.rs` stores trace events and artifacts.
- `src-tauri/src/eval/trace.rs` gives every event a stable `kind()`.
- Existing tests prove campaign manifests and scorecards attach as artifacts,
  but there is no generic evidence gate.

GitNexus returned UNKNOWN for `EvalRuntime`, `EvalEpisode`, and
`EvalArtifact` symbols in this worktree. The first slice avoids modifying those
existing structs and adds a new Module that consumes their public Interfaces.

## Pi Reference Truth

Pi's validation and conformance code uses explicit evidence and fail-closed
admission:

- `validation_broker.rs` stores schema-tagged validation slots and decisions;
  malformed or unavailable records produce degraded snapshots instead of green
  state.
- Validation requests carry expected artifact schemas and hashes.
- `extension_validation.rs` classifies candidates from evidence signals and
  returns `Unknown` when no signals exist.
- `conformance_shapes.rs` produces classified failures and summaries rather
  than a bare boolean.

The transferable design is a schema-tagged evidence record with a no-data
fail-closed verdict and a compact summary that callers can use as a promotion
gate.

## uClaw Adaptation

Add `src-tauri/src/eval/evidence.rs` with:

- `EVAL_EVIDENCE_SCHEMA = "uclaw.eval.evidence.v1"`.
- `EvalEvidenceRequirement` containing required event kinds and artifact kinds.
- `EvalEvidenceRecord` for pass/fail/missing checks.
- `EvalEvidenceGateReport` with observed kinds, missing kinds, verdict, and
  records.
- `gate_eval_evidence(&EvalEpisode, &EvalEvidenceRequirement)`.
- `attach_eval_evidence_report(&EvalRuntime, run_id, &EvalEvidenceGateReport)`.

This keeps the first slice local and lets campaigns/adapters opt in without
changing their execution semantics.

## Interface

```rust
let requirement = EvalEvidenceRequirement::new(
    ["tool_call", "tool_result"],
    ["tool_result", "performance_scorecard"],
);
let report = gate_eval_evidence(&episode, &requirement);
attach_eval_evidence_report(&runtime, &episode.run_id, &report)?;
```

Verdict rules:

- Missing required event kinds -> `FailClosed`.
- Missing required artifact kinds -> `FailClosed`.
- No requirements -> `FailClosed`, because a gate with no evidence target is
  not a meaningful promotion claim.
- All required event and artifact kinds observed -> `Pass`.

## Acceptance Evidence

- Tests prove a report passes when all required event and artifact kinds are
  present.
- Tests prove missing events or artifacts fail closed with missing records.
- Tests prove empty requirements fail closed.
- Tests prove the report serializes with the schema and camelCase fields.
- Tests prove the report attaches as an eval artifact and appears on the
  episode.
- Tests prove an evidence manifest JSON parses into per-case required event and
  artifact kinds.
- Tests prove a file-based local gate reads a manifest plus episode JSON and
  returns a non-zero exit code when evidence is missing.
- `cargo test --lib eval::evidence -- --nocapture` passes.
- `cargo test --lib eval::evidence_gate -- --nocapture` passes.
- `cargo test --bin eval-evidence-gate -- --nocapture` passes.
- `git diff --check` passes.
- GitNexus `detect_changes` is recorded before commit.

## Non-Goals

- Do not replace `EvalGraderRegistry`.
- Do not run live browser, plugin, provider, or agent-loop scenarios in this
  slice; the command gates already-recorded evidence files.
- Do not rewrite browser, plugin, provider, or agent-loop adapters.
- Do not persist a validation lease store in uClaw.

## Rollback

Revert the Evidence-Gated Eval commits. Since the first slice only adds a new
Module and exports it from `eval::mod`, existing eval runtime behaviour remains
available without schema migration.
