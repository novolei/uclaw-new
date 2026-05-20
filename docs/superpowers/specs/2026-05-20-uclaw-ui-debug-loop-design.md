# uClaw UI Debug Loop v1 — Design Spec

**Date:** 2026-05-20  
**Status:** Approved, pending implementation plan  
**Scope:** Desktop UI debugging workflow for Tauri + React surfaces

---

## Overview

uClaw needs a repeatable UI debug loop that lets an agent see the same application a user sees, correlate it with frontend console output and backend logs, and fix issues while the app is running. The loop must avoid the failure mode exposed by the recent smoke run: mistaking a release app bundle or a plain browser page for the active debug build.

The workflow combines three evidence sources:

- **Computer Use:** observes the real macOS desktop window and its WebView URL.
- **Playwright:** inspects the Vite web page, DOM, screenshots, and browser console.
- **Tauri/runtime logs:** prove backend boot, IPC registration, services, automation runtime, and bridge health.

No UI change is considered verified until these sources agree.

---

## Goals

1. Verify that the window under test is the intended debug build, usually `target/debug/uclaw` from the active worktree.
2. Capture reliable evidence for blank screens, stale release apps, bad `devUrl` wiring, frontend runtime errors, and IPC failures.
3. Give agents a fixed repair loop: observe, classify, patch, restart, re-observe.
4. Leave no smoke-test residue: close spawned debug processes and remove temporary screenshots/log folders unless explicitly requested.

## Non-Goals

- This spec does not replace Rust unit tests, Vitest, or app-native harness scorecards.
- This spec does not require live external websites.
- This spec does not make Playwright a substitute for Tauri WebView IPC testing; plain Playwright does not have Tauri IPC injection.

---

## Core Rule

**Computer Use proves the real desktop render. Playwright proves the web render. Logs prove runtime truth. Process paths prove which build was tested.**

If any one of these disagrees, the result is a debug finding, not a pass.

---

## Workflow

### 1. Preflight: Identify The Target

Before launching anything, record:

- repo/worktree path;
- current branch and `git status --short`;
- expected app binary path;
- expected frontend URL;
- expected test flow.

For uClaw dev work, the expected binary is normally:

```text
<worktree>/target/debug/uclaw
```

The expected frontend URL is normally:

```text
http://127.0.0.1:5173/
```

If `/Applications/uClaw.app` appears in Computer Use, treat it as the macOS bundle identity only. It is not proof that the release app is under test. The process list must confirm the actual command path.

### 2. Launch: Start Frontend And Desktop Separately

Start Vite first:

```bash
cd ui
npm run dev -- --host 127.0.0.1
```

Then start Tauri from the correct project root/cwd for the repo's `tauri.conf.json`:

```bash
cd src-tauri
cargo tauri dev
```

If a config override is needed, it must preserve the configured `devUrl`. An override that removes or bypasses `devUrl` can make the app fall back to `tauri://localhost`, which invalidates the smoke test for Vite-backed development.

### 3. Process Truth Check

After launch, filter processes for the active worktree:

```bash
ps -axo pid,ppid,command | rg '<worktree>|vite --host 127.0.0.1|target/debug/uclaw'
```

Expected evidence:

- one Vite process under `<worktree>/ui`;
- one `target/debug/uclaw` process under `<worktree>`;
- optional bridge child processes such as embedded Python or Bun, also under `<worktree>/target/debug`.

If the debug binary is missing or the process belongs to another checkout, stop and relaunch correctly.

### 4. Desktop Observation With Computer Use

Use Computer Use against `uClaw` after the debug process is visible:

- capture the app state;
- record WebView URL;
- capture screenshot;
- inspect accessibility tree for meaningful content.

Interpretation:

| Observation | Meaning |
| --- | --- |
| `http://127.0.0.1:5173` with rendered UI | devUrl path likely correct |
| `tauri://localhost` during dev smoke | likely static fallback, wrong config, or devUrl bypass |
| blank white window | classify via logs + Playwright console before patching |
| release-looking stale UI | verify process path before trusting the window |

Computer Use is the only source that proves what the user-visible desktop window actually shows.

### 5. Web Observation With Playwright

Use Playwright against the Vite URL:

- page URL and title;
- DOM snapshot;
- console errors and warnings;
- screenshot;
- one interaction where possible.

Important limitation:

Plain Playwright at `http://127.0.0.1:5173` does not have Tauri IPC injection. Errors such as missing `window.__TAURI_INTERNALS__`, `invoke`, or `transformCallback` are expected unless the app provides a mock bridge for browser-only testing. These errors are useful, but they do not by themselves prove the Tauri WebView is broken.

### 6. Runtime Log Observation

Read the Tauri dev output and classify boot state:

- app boot completed;
- `invoke_handler` registration did not panic;
- gbrain/memU bridges initialized or reported structured failures;
- background services started;
- automation runtime activated expected specs;
- frontend-facing events are emitted without fatal backend errors.

For automation and harness surfaces, successful backend boot should include the relevant runtime service start logs and command registration evidence from code.

### 7. Classification

Every UI smoke result must be one of:

- `pass`: desktop render, web render, logs, and process identity agree.
- `frontend-runtime-error`: Playwright or WebView console shows app-breaking JS errors.
- `tauri-devurl-mismatch`: debug desktop is on `tauri://localhost` when Vite devUrl is expected.
- `ipc-injection-missing`: Tauri WebView lacks IPC where it should exist.
- `backend-boot-failure`: Tauri logs show app/runtime/service startup failure.
- `wrong-app-under-test`: process identity points to a release app or another worktree.
- `inconclusive`: evidence is incomplete; do not claim pass.

### 8. Repair Loop

For each failure:

1. Pick one hypothesis.
2. Patch the smallest relevant file.
3. Restart only the processes needed to refresh the evidence.
4. Re-run the same Computer Use, Playwright, log, and process checks.
5. Record before/after evidence in the final report.

The loop ends only when the classification is `pass` or when a blocker is documented with exact evidence.

### 9. Cleanup

Before final response:

- close the debug desktop app;
- stop Vite if the agent started it;
- confirm no matching debug processes remain;
- delete temporary `.playwright-mcp` or screenshot folders created by the agent unless the user asked to keep artifacts;
- report any unrelated dirty files without touching them.

---

## Recommended Implementation

### Phase 1: Manual Playbook

Use this spec as a checklist for immediate UI debugging. This requires no code changes.

### Phase 2: Helper Script

Add a script such as:

```text
scripts/ui_debug_smoke.sh
```

Responsibilities:

- launch Vite and Tauri with safe cwd handling;
- preserve `devUrl`;
- print process identity evidence;
- tee Tauri logs to `/tmp/uclaw-ui-debug-<timestamp>.log`;
- provide a cleanup command for spawned PIDs.

The script should not automate Computer Use, because Computer Use is the visual proof layer controlled by the agent.

### Phase 3: Browser-Only Mock Bridge

For Playwright-only UI debugging, add a deliberate mock Tauri bridge mode:

```text
VITE_UCLAW_MOCK_TAURI=1
```

This mode should:

- mock `invoke`;
- mock event `listen`;
- expose deterministic fixture data for key settings/automation surfaces;
- clearly show a visual "mock bridge" marker in development.

This makes Playwright useful for interaction testing without confusing it with real Tauri IPC validation.

### Phase 4: App-Native UI Smoke Harness

Longer term, add an app-native harness that can:

- run selected Tauri commands;
- capture scorecards;
- expose "open debug checklist" in System Diagnostics;
- persist UI smoke reports next to existing harness artifacts.

---

## Final Report Shape

Every UI debug run should end with:

- **Target:** worktree, branch, binary path, URL.
- **Desktop Evidence:** Computer Use URL, screenshot result, visible state.
- **Web Evidence:** Playwright URL/title, console summary, screenshot result.
- **Runtime Evidence:** key Tauri boot/service logs.
- **Process Evidence:** matching debug processes before cleanup, none after cleanup.
- **Classification:** one of the fixed labels.
- **Fixes Applied:** files changed, if any.
- **Remaining Risk:** what was not tested.

---

## Acceptance Criteria

- An agent can distinguish debug `target/debug/uclaw` from `/Applications/uClaw.app`.
- A blank window is classified with concrete evidence instead of guessed.
- Plain Playwright Tauri IPC errors are interpreted correctly.
- The workflow always produces screenshot, console/log, and process evidence.
- The workflow leaves no self-created test residue.

---

## Implemented By

- `scripts/ui_debug_smoke.sh` launches Vite + Tauri dev, captures logs, prints process identity, and cleans spawned processes.
- `ui/src/lib/dev-tauri-mock.ts` enables browser-only UI debugging with official Tauri mocks when `VITE_UCLAW_MOCK_TAURI=1`.
- `npm run dev:mock-tauri` opens the React app in browser-debug mode without requiring a Tauri WebView.

Use Computer Use for real desktop proof. Use Playwright or the in-app browser for mock bridge UI iteration. Treat the two paths as complementary evidence, not substitutes.
