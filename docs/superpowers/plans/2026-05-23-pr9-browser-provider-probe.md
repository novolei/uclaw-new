# PR9 BrowserProvider Status Setup Probe Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` task-by-task. Keep Rust tests in sibling `*_tests.rs` files.

**Goal:** Formalize a small BrowserProvider readiness contract around uClaw Browser Agent v2, borrowing jcode's status/setup/probe ergonomics without replacing uClaw's browser runtime.

**Architecture:** Add a provider-neutral browser status module under `src-tauri/src/browser/`. The module describes provider capabilities, setup checks, readiness probes, remediation text, and the built-in Local Chromium provider card. It does not launch browsers, mutate sessions, add commands, or change browser action execution.

**Tech Stack:** Rust, `serde`, existing `browser` module, sibling Rust tests, existing GitNexus workflow.

---

## Scope Anchors

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr9-browser-provider-probe`
- Branch: `codex/agent-os-jcode-pr9-browser-provider-probe`
- Source docs:
  - `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
  - `docs/jcode_comparison/README.md`
  - `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
  - `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`
  - `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
  - `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- jcode reference:
  - `/Users/ryanliu/Documents/jcode/src/tool/browser.rs`
  - `/Users/ryanliu/Documents/jcode/src/browser.rs`

## ADR Section 18 Answers

| Question | PR9 Answer |
|---|---|
| 1. What user intent does this support? | Before a browser task starts, users and agents can understand whether browser automation is ready, missing setup, degraded, or unavailable, with clear remediation. |
| 2. What autonomy level can it run at? | Metadata/probe evaluation only; safe at L0-L5 because it does not perform browser actions. Actual browser actions stay governed by existing policies. |
| 3. What is the source of truth? | Existing uClaw Browser Agent v2 remains source of runtime truth: `BrowserContextManager`, task store, checkpoints, intervention bridge, and browser memory adapter. PR9 adds derived provider status metadata only. |
| 4. Which TaskEvent does it emit? | None in PR9. Later wiring may emit `browser.ready`, `browser.probe`, `browser.action`, or `boundary.yielded` through existing runtime contracts. |
| 5. What context does it read? | None in the pure module. Future adapters may read manager status, profile availability, action probe results, and setup diagnostics. |
| 6. What capability does it require? | Browser provider status/probe read capability only. No network, file mutation, CDP, browser launch, or login capability is exercised in PR9. |
| 7. Which policy hooks can block it? | None for static evaluation. Future setup/launch/action wiring must pass browser login policy, SafetyManager, profile policy, and user-boundary hooks. |
| 8. What world projection does the UI render? | Future UI can render provider id, readiness, setup completeness, failed checks, missing capabilities, active context count, and remediation steps. |
| 9. What harness cases prove it works? | Model-free tests for ready/degraded/needs-setup/unavailable evaluation, required action probes, deterministic Local Chromium capabilities, and remediation copy. |
| 10. What is the rollback path? | Remove `browser/provider.rs`, `browser/provider_tests.rs`, exports, and the status ledger update. Browser runtime behavior remains unchanged. |
| 11. What does this not own? | No browser runtime replacement, no Playwright worker, no jcode Firefox bridge import, no Tauri commands, no migrations, no UI surface, no BrowserContextManager behavior changes. |

## Numbering Note

`AGENT_OS_JCODE_UPGRADE_STATUS.md` is authoritative for this series: BrowserProvider is PR-9. Older blueprint text still says PR-11 for BrowserProvider alignment; PR9 treats that as historical numbering drift.

## Allowed Files

- Create: `src-tauri/src/browser/provider.rs`
- Create: `src-tauri/src/browser/provider_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Create/modify: this plan file

## Explicit Non-Goals

- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not modify `src-tauri/src/browser/context_manager.rs`.
- Do not modify `src-tauri/src/browser/agent_loop.rs`.
- Do not modify `src-tauri/src/browser/task_store.rs`.
- Do not modify `src-tauri/src/db/migrations.rs`.
- Do not add a browser provider database table.
- Do not add Playwright, browser-use, Browserbase, Firecrawl, or jcode Firefox bridge execution.
- Do not change existing browser action execution or token/tool registration.

## Impact Notes

- `BrowserContextManager`: LOW for struct and impl; PR9 does not edit it.
- `BrowserService`: LOW; legacy compatibility surface is not edited.
- `BrowserActionRegistry`: LOW; action execution is not edited.
- `tauri_commands.rs`: DMZ; PR9 avoids it.
- `main.rs`: startup/invoke wiring; PR9 avoids it.

## Task 1: Add Provider Status Contract

**Files:**
- Create: `src-tauri/src/browser/provider.rs`
- Create: `src-tauri/src/browser/provider_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Write sibling tests first**

Tests should cover:

- Local Chromium capability card includes uClaw Browser Agent v2 strengths: DOM snapshot, screenshot, action execution, checkpoint resume, auth profiles, user intervention, task store.
- Passing setup checks plus required action probes produces `Ready`.
- Missing profile or manager setup produces `NeedsSetup`.
- Unsupported required setup produces `Unavailable`.
- Failed required action probe produces `Degraded`.
- Remediation text is present and specific.

- [x] **Step 2: Add pure provider module**

Define:

- `BrowserProviderReadiness`
- `BrowserProbeStatus`
- `BrowserSetupCheck`
- `BrowserCapabilityProbe`
- `BrowserProviderCapabilities`
- `BrowserProviderReadinessProbe`
- `BrowserProviderStatus`
- `local_chromium_capabilities()`
- `local_chromium_status(probe)`

Keep all inputs explicit and test-controlled. Do not read the filesystem, launch Chromium, or call CDP.

- [x] **Step 3: Export module**

Add `pub mod provider;` and re-export the primary types from `browser/mod.rs`.

## Task 2: Update Status Ledger

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [x] **Step 1: Mark PR9 in progress**

Set current phase to PR9 in progress, owner `Codex`, and record worktree/branch.

## Task 3: Verify And Commit

- [x] **Step 1: Run focused tests**

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider
```

- [x] **Step 2: Run broader browser compile/test slice**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser
```

- [x] **Step 3: Check staged scope**

```bash
git diff --cached --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr9-browser-provider-probe
```

- [x] **Step 4: Commit**

Commit body must include verification commands and expected output.
