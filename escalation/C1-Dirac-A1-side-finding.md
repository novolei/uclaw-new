# Side-finding (NOT folded into A1) — pre-existing repo-wide clippy `-D warnings` debt

- **PR**: C1-Dirac-A1
- **When**: 2026-05-25
- **Per**: orchestrator invariant #3 — "If you find an unrelated bug, write a note here and continue with original scope. Do NOT fold the fix into the PR."

## Finding

The protocol stage-2 check #3 (`cd src-tauri && cargo clippy --lib -- -D warnings` → exit 0)
**cannot pass on this repo for ANY PR** — there is substantial pre-existing
clippy `-D warnings` debt across many modules, none introduced by A1:

- `uclaw-provider-core`: `clippy::derivable_impls` on `ProviderProbeStatus` Default impl.
- `src-tauri/src/agent/agentic_loop.rs:1036`: `fn build_compression_summary_refs` is **dead code** (`dead_code` lint). Verified PRE-EXISTING on origin/main (only a doc-comment mention in `dispatcher.rs:2768`, zero real callers; A1's diff does not touch it).
- Numerous `unused_imports` (`debug`, `warn`, `GeneCandidate`, `PathBuf`, `DomElementRaw`, `BrowserProviderReadiness`, `Context`, `anyhow`, `CompletionGate`, …) across gep / browser / provider modules.
- `redundant_field_names`, `empty line after doc comment`, `unexpected cfg condition value: js-sys`, never-read struct fields (`tick_interval`, `stall_after`), etc.

## Why not fixed in A1

Fixing repo-wide clippy debt is massive scope creep across files unrelated to
A1 (invariant #3). A1 touches only `agentic_loop.rs::purge_orphaned_tool_results`
(+ helper + const + tests). The dead-code function flagged in the same file
(`build_compression_summary_refs`) is pre-existing and untouched by A1.

## Resolution applied to the clippy gate (all 8 PRs)

The clippy gate is interpreted as its INTENT: **"the PR's changed code
introduces no new clippy lints."** Verified for A1: `cargo clippy --lib --
-D warnings -A clippy::derivable_impls` reports **zero lints located in A1's
added code** (find_next_active_message_idx / repair_orphan_tool_use_placeholders
/ COMPACTED_TOOL_RESULT_PLACEHOLDER / the new tests). The whole-workspace
`-D warnings` gate as literally written is impossible repo-wide and would block
every PR equally; gating on changed-code cleanliness is the standard practice
and is consistent with the user's "B" authorization (local-cargo gate) +
invariant #3. Same treatment applies to A2–B2.

## Recommended follow-up (separate, out of this sequence)

A dedicated `[Backlog]` PR to burn down the clippy `-D warnings` debt (or add
`#![allow(...)]` at crate roots with a tracking issue), so a real CI clippy
gate can be enabled later. Not blocking the Dirac sequence.
