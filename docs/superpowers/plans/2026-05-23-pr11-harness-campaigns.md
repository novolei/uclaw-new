# PR11 Harness Campaigns Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` task-by-task. Keep Rust tests in sibling `*_tests.rs` files.

**Goal:** Add jcode-style model-free smoke and performance campaign definitions to uClaw's existing harness layer, while keeping uClaw HarnessCase/Episode/Artifact as the canonical Evolution Layer gate.

**Architecture:** Add a pure `harness::campaign` module that packages repeatable campaign manifests for tool smoke, browser readiness, soft-interrupt/checkpoint, and scheduled-worker evidence. Campaigns produce typed `HarnessCase` definitions and PR6 `PerformanceThreshold`s, but do not execute tools, launch browsers, mutate DB state, or wire UI commands.

**Tech Stack:** Rust, serde, existing `HarnessCase`, `HarnessRuntime`, `PerformanceScorecard` substrate, sibling Rust tests, existing GitNexus workflow.

---

## Scope Anchors

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr11-harness-campaigns`
- Branch: `codex/agent-os-jcode-pr11-harness-campaigns`
- Source docs:
  - `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
  - `docs/jcode_comparison/03_performance_optimization.md`
  - `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
  - `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`
  - `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
  - `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- jcode reference:
  - `/Users/ryanliu/Documents/jcode/src/bin/harness.rs`

## ADR Section 18 Answers

| Question | PR11 Answer |
|---|---|
| 1. What user intent does this support? | Developers can prove Agent OS tool/browser/background changes with cheap campaign manifests before asking for higher autonomy or promotion. |
| 2. What autonomy level can it run at? | Campaign definition is L0 metadata. Future execution can gate L1-L5 promotion decisions, but PR11 executes no autonomous work. |
| 3. What is the source of truth? | `HarnessCase`, `HarnessEpisode`, harness artifacts, and PR6 performance scorecards remain truth. PR11 adds derived manifests only. |
| 4. Which TaskEvent does it emit? | None in PR11. Campaign cases declare required harness event kinds that later runners should produce. |
| 5. What context does it read? | None at runtime. Static campaign definitions cite jcode harness and prior PR contracts. |
| 6. What capability does it require? | Harness manifest read capability only. Future execution consumes existing tool, browser, agent-loop, and automation capabilities under their policies. |
| 7. Which policy hooks can block it? | None for manifest creation. Future execution remains blocked by safety/path policy, browser policy, automation permissions, and boundary yields. |
| 8. What world projection does the UI render? | Future UI can render campaign id, cases, required events, artifacts, p95 thresholds, and promotion gate status. |
| 9. What harness cases prove it works? | Unit tests prove campaign contents, model-free tool coverage, browser/scheduled-worker/soft-interrupt manifests, serde shape, and artifact attachment. |
| 10. What is the rollback path? | Remove `harness/campaign.rs`, `harness/campaign_tests.rs`, the module export, this plan, and status-ledger edits. Existing harness runtime behavior remains unchanged. |
| 11. What does this not own? | No runner, no CLI command, no Tauri command, no DB migration, no tool execution rewrite, no browser launch, no CI gate, no UI surface. |

## Allowed Files

- Create: `src-tauri/src/harness/campaign.rs`
- Create: `src-tauri/src/harness/campaign_tests.rs`
- Modify: `src-tauri/src/harness/mod.rs`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Create/modify: this plan file

## Explicit Non-Goals

- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not modify `src-tauri/src/db/migrations.rs`.
- Do not modify `src-tauri/src/harness/runtime.rs`.
- Do not modify `src-tauri/src/harness/performance_scorecard.rs`.
- Do not execute real tools or browsers.
- Do not create a new harness truth store.

## Impact Notes

- `HarnessRuntime`: LOW, 0 affected processes; PR11 consumes it only for artifact attachment.
- `PerformanceScorecard`: LOW, 0 affected processes; PR11 consumes thresholds only.
- `attach_performance_scorecard`: LOW, 1 direct test caller; PR11 does not modify it.
- `HarnessSubject`: LOW, 0 affected processes; PR11 consumes existing subjects and does not add enum variants.
- `harness/mod.rs`: additive module export only.

## Task 1: Add Campaign Manifest Contract

**Files:**
- Create: `src-tauri/src/harness/campaign.rs`
- Create: `src-tauri/src/harness/campaign_tests.rs`
- Modify: `src-tauri/src/harness/mod.rs`

- [x] **Step 1: Write sibling tests first**

Tests should cover:

- Default Agent OS campaign pack includes tool smoke, browser readiness, soft interrupt/checkpoint, and scheduled worker campaigns.
- Tool smoke is model-free and covers uClaw active equivalents for jcode write/read/edit/patch/search/shell/todo/batch patterns.
- Network-backed tool cases are excluded by default and included only when requested.
- Browser readiness campaign requires provider-status artifacts and p95 setup/probe thresholds.
- Soft interrupt and scheduled-worker campaigns require boundary/checkpoint/run-finished evidence.
- Campaign manifests serialize camelCase and attach as harness JSON artifacts.

- [x] **Step 2: Add pure campaign module**

Define:

- `HARNESS_CAMPAIGN_SCHEMA_VERSION`
- `HarnessCampaignKind`
- `HarnessCampaignCadence`
- `HarnessCampaignCase`
- `HarnessCampaign`
- `jcode_tool_smoke_campaign(include_network)`
- `browser_provider_readiness_campaign()`
- `soft_interrupt_checkpoint_campaign()`
- `scheduled_worker_campaign()`
- `agent_os_harness_campaigns()`
- `attach_harness_campaign_manifest(runtime, run_id, campaign)`

Keep every input explicit. Do not execute cases.

- [x] **Step 3: Export module**

Add `pub mod campaign;` and re-export the campaign types/functions from `harness/mod.rs`.

## Task 2: Update Status Ledger

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [x] **Step 1: Mark PR11 in progress**

Set current phase to PR11 in progress, owner `Codex`, and record worktree/branch.

## Task 3: Verify And Commit

- [x] **Step 1: Run focused tests**

```bash
rustfmt --edition 2021 --check src-tauri/src/harness/campaign.rs src-tauri/src/harness/campaign_tests.rs
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::campaign
```

- [x] **Step 2: Run adjacent harness slice**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::performance_scorecard
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::runtime
```

- [ ] **Step 3: Check staged scope**

```bash
git diff --cached --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr11-harness-campaigns
```

- [ ] **Step 4: Commit**

Commit body must include verification commands and expected output.
