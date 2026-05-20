# uClaw UI Debug Loop v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a repeatable UI debug loop for uClaw desktop development so agents can verify the real debug app, inspect the web frontend, classify UI failures, and clean up spawned processes.

**Architecture:** Add a shell smoke helper for process/log discipline, then add an opt-in browser-only Tauri mock bridge using official `@tauri-apps/api/mocks`. The real desktop path remains Computer Use + Tauri logs; the mock path is only for Playwright/in-app browser inspection of frontend UI without Tauri IPC.

**Tech Stack:** Tauri v2, React 18, Vite, Vitest, shell scripts, `@tauri-apps/api/mocks`, Computer Use, Playwright MCP.

---

## File Structure

- Create `scripts/ui_debug_smoke.sh`
  - Starts Vite and Tauri dev with stable cwd handling.
  - Writes logs to `/tmp/uclaw-ui-debug-<timestamp>/`.
  - Prints process identity evidence.
  - Provides a cleanup trap for the processes it starts.
- Create `ui/src/lib/dev-tauri-mock.ts`
  - Installs an opt-in browser-only mock Tauri runtime when `VITE_UCLAW_MOCK_TAURI=1`.
  - Uses `mockIPC`, `mockWindows`, and `mockConvertFileSrc`.
  - Exposes deterministic fixture responses for startup, System Diagnostics, harness buttons, conversation shell, and automation shell.
- Modify `ui/src/main.tsx`
  - Import and call `installDevTauriMock()` before importing `./lib/tauri-bridge`.
  - Never install the mock inside a real Tauri WebView.
- Create `ui/src/lib/dev-tauri-mock.test.ts`
  - Verifies opt-in behavior.
  - Verifies fixture commands used by App/SystemTab.
  - Verifies event mocking supports `listen`.
- Modify `docs/superpowers/specs/2026-05-20-uclaw-ui-debug-loop-design.md`
  - Add a short "Implemented By" section after tasks land.

---

## Task 1: Add The UI Debug Smoke Script

**Files:**
- Create: `scripts/ui_debug_smoke.sh`

- [ ] **Step 1: Create the shell script**

Create `scripts/ui_debug_smoke.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="$ROOT_DIR/ui"
TAURI_DIR="$ROOT_DIR/src-tauri"
STAMP="$(date +"%Y%m%d-%H%M%S")"
LOG_DIR="${UCLAW_UI_DEBUG_LOG_DIR:-/tmp/uclaw-ui-debug-$STAMP}"
VITE_HOST="${UCLAW_UI_DEBUG_HOST:-127.0.0.1}"
VITE_PORT="${UCLAW_UI_DEBUG_PORT:-5173}"
VITE_URL="http://${VITE_HOST}:${VITE_PORT}/"
PIDS=()

cleanup() {
  local code=$?
  if [[ "${UCLAW_UI_DEBUG_KEEP_ALIVE:-0}" != "1" ]]; then
    for pid in "${PIDS[@]:-}"; do
      if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
      fi
    done
  fi
  echo "[ui-debug] log_dir=$LOG_DIR"
  exit "$code"
}
trap cleanup EXIT INT TERM

print_header() {
  echo
  echo "== $1 =="
}

print_process_truth() {
  print_header "process truth"
  ps -axo pid,ppid,command \
    | rg "$ROOT_DIR|vite --host $VITE_HOST|target/debug/uclaw" \
    || true
}

mkdir -p "$LOG_DIR"

print_header "preflight"
echo "[ui-debug] root=$ROOT_DIR"
echo "[ui-debug] ui=$UI_DIR"
echo "[ui-debug] tauri=$TAURI_DIR"
echo "[ui-debug] vite_url=$VITE_URL"
echo "[ui-debug] log_dir=$LOG_DIR"
git -C "$ROOT_DIR" status --short

print_header "start vite"
(
  cd "$UI_DIR"
  npm run dev -- --host "$VITE_HOST" --port "$VITE_PORT"
) >"$LOG_DIR/vite.log" 2>&1 &
PIDS+=("$!")

for _ in {1..80}; do
  if curl -sS --max-time 1 "$VITE_URL" >/dev/null 2>&1; then
    echo "[ui-debug] vite ready: $VITE_URL"
    break
  fi
  sleep 0.25
done

if ! curl -sS --max-time 2 "$VITE_URL" >/dev/null 2>&1; then
  echo "[ui-debug] vite did not become ready"
  tail -80 "$LOG_DIR/vite.log" || true
  exit 1
fi

print_header "start tauri"
(
  cd "$TAURI_DIR"
  cargo tauri dev
) >"$LOG_DIR/tauri.log" 2>&1 &
PIDS+=("$!")

for _ in {1..240}; do
  if rg -q "uClaw started successfully|Running .*target/debug/uclaw" "$LOG_DIR/tauri.log" 2>/dev/null; then
    echo "[ui-debug] tauri debug app launched"
    break
  fi
  sleep 0.5
done

if ! rg -q "target/debug/uclaw|uClaw started successfully" "$LOG_DIR/tauri.log" 2>/dev/null; then
  echo "[ui-debug] tauri did not report a debug launch"
  tail -120 "$LOG_DIR/tauri.log" || true
  exit 1
fi

print_process_truth

print_header "manual verification"
echo "[ui-debug] Use Computer Use get_app_state('uClaw') now."
echo "[ui-debug] Expected debug binary: $ROOT_DIR/target/debug/uclaw"
echo "[ui-debug] Expected Vite URL: $VITE_URL"
echo "[ui-debug] If using Playwright, inspect $VITE_URL."

if [[ "${UCLAW_UI_DEBUG_KEEP_ALIVE:-0}" == "1" ]]; then
  echo "[ui-debug] keep-alive enabled. Press Ctrl-C to stop."
  wait
else
  echo "[ui-debug] smoke launched and verified process truth; exiting will clean spawned processes."
fi
```

- [ ] **Step 2: Make the script executable**

Run:

```bash
chmod +x scripts/ui_debug_smoke.sh
```

- [ ] **Step 3: Verify shell syntax**

Run:

```bash
bash -n scripts/ui_debug_smoke.sh
```

Expected: exit code 0 and no output.

- [ ] **Step 4: Commit Task 1**

Run:

```bash
git add scripts/ui_debug_smoke.sh
git commit -m "chore: add ui debug smoke helper"
```

---

## Task 2: Add Browser-Only Tauri Mock Bridge

**Files:**
- Create: `ui/src/lib/dev-tauri-mock.ts`
- Modify: `ui/src/main.tsx`
- Test: `ui/src/lib/dev-tauri-mock.test.ts`

- [ ] **Step 1: Write the failing test**

Create `ui/src/lib/dev-tauri-mock.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'
import { listen, emit } from '@tauri-apps/api/event'
import {
  createUclawMockIpcHandler,
  installDevTauriMock,
  shouldInstallDevTauriMock,
} from './dev-tauri-mock'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>
  }
}

afterEach(() => {
  vi.unstubAllEnvs()
  clearMocks()
  delete window.__UCLAW_DEV_TAURI_MOCK__
})

describe('dev tauri mock', () => {
  it('stays disabled unless explicitly requested', () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', undefined)
    expect(shouldInstallDevTauriMock()).toBe(false)
  })

  it('stays disabled inside a real Tauri runtime', () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    window.__TAURI_INTERNALS__ = { invoke: async () => null }
    expect(shouldInstallDevTauriMock()).toBe(false)
  })

  it('installs official Tauri mocks and returns startup fixtures', async () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    installDevTauriMock()

    await expect(invoke('get_settings')).resolves.toMatchObject({
      language: 'zh-CN',
      theme: 'system',
    })
    await expect(invoke('get_active_model')).resolves.toBeNull()
    expect(window.__UCLAW_DEV_TAURI_MOCK__).toBe(true)
  })

  it('supports event listen and emit for browser-only interaction checks', async () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    installDevTauriMock()
    const handler = vi.fn()

    const unlisten = await listen('automation://activity', handler)
    await emit('automation://activity', { id: 'activity-1' })
    await unlisten()

    expect(handler).toHaveBeenCalledWith(expect.objectContaining({
      event: 'automation://activity',
      payload: { id: 'activity-1' },
    }))
  })

  it('returns a visible diagnostics fixture for SystemTab', async () => {
    const handler = createUclawMockIpcHandler()
    const result = await handler('get_system_diagnostics')

    expect(result).toMatchObject({
      app_version: 'dev-mock',
      platform: 'browser',
      memu: { running: true },
      gbrain: { connected: true, tool_count: 6 },
    })
  })
})
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
cd ui
npm test -- src/lib/dev-tauri-mock.test.ts
```

Expected: fail because `ui/src/lib/dev-tauri-mock.ts` does not exist.

- [ ] **Step 3: Add the mock bridge module**

Create `ui/src/lib/dev-tauri-mock.ts`:

```ts
import { clearMocks, mockConvertFileSrc, mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>
    __UCLAW_DEV_TAURI_MOCK__?: boolean
  }
}

type MockHandler = (cmd: string, payload?: InvokeArgs) => unknown

const settingsFixture = {
  language: 'zh-CN',
  theme: 'system',
  theme_style: 'default',
  provider: null,
  model: null,
  safety_mode: 'yolo',
}

const diagnosticsFixture = {
  app_version: 'dev-mock',
  platform: 'browser',
  arch: 'mock',
  memory_used_mb: 256,
  memory_total_mb: 1024,
  uptime_secs: 1,
  consecutive_failures: 0,
  recovery_attempts: 0,
  active_processes: 1,
  orphan_processes: 0,
  services: [
    { name: 'AppRuntimeService', status: 'Running', detail: 'mocked browser runtime' },
  ],
  memu: {
    running: true,
    pid: 1,
    reason: null,
    python_path: '/mock/python',
    script_path: '/mock/memu_bridge.py',
    db_path: '/mock/memu.db',
  },
  gbrain: {
    connected: true,
    tool_count: 6,
    pgdata_ready: true,
    error: null,
    status: 'connected',
    error_kind: null,
    suggested_action: null,
    home_path: '/mock/gbrain',
    launcher_path: '/mock/bun',
    pgdata_path: '/mock/pgdata',
    config_command: '/mock/bun',
    config_entry_path: '/mock/gbrain/src/cli.ts',
    config_command_exists: true,
    config_entry_exists: true,
    config_gbrain_home: '/mock/gbrain',
    path_stale: false,
  },
  gbrain_init: { status: 'skipped_already_initialized', at_ms: 1 },
}

const harnessSuiteFixture = {
  passed: true,
  averageScore: 1,
  runIds: ['mock-run'],
  scorecards: [
    {
      caseId: 'mock.browser.ui_debug',
      title: 'Mock bridge keeps browser UI debuggable',
      passed: true,
      score: 1,
      checks: [{ id: 'mock_bridge_installed', passed: true, score: 1, message: 'ok' }],
    },
  ],
}

const selfImprovementFixture = [
  {
    candidateId: 'candidate.mock.ui_debug_loop',
    verdict: 'promote',
    score: 1,
    checks: [{ id: 'rollback_ref', passed: true, message: 'ok' }],
  },
]

export function shouldInstallDevTauriMock(): boolean {
  return import.meta.env.VITE_UCLAW_MOCK_TAURI === '1'
    && typeof window !== 'undefined'
    && !window.__TAURI_INTERNALS__?.invoke
}

export function createUclawMockIpcHandler(): MockHandler {
  return (cmd: string, payload?: InvokeArgs): unknown => {
    console.info('[uClaw mock Tauri IPC]', cmd, payload ?? {})

    switch (cmd) {
      case 'get_settings':
      case 'patch_settings':
        return settingsFixture
      case 'get_platform':
        return { platform: 'browser', arch: 'mock' }
      case 'get_version':
        return { version: 'dev-mock', commit: null, build_time: null }
      case 'get_bootstrap_status':
        return { complete: true, steps: [] }
      case 'get_active_model':
        return null
      case 'list_conversations':
      case 'list_spaces':
      case 'list_notifications':
      case 'list_background_tasks':
      case 'list_mcp_servers':
      case 'list_skills':
      case 'list_channels':
      case 'list_pending_escalations':
      case 'automation_list_specs':
      case 'automation_list_activities':
        return []
      case 'get_system_diagnostics':
        return diagnosticsFixture
      case 'run_browser_parity_harness':
      case 'run_memory_gbrain_eval_harness':
      case 'run_agent_control_plane_harness':
        return harnessSuiteFixture
      case 'run_self_improvement_gate_harness':
        return selfImprovementFixture
      case 'restart_memu_bridge':
      case 'restart_gbrain_mcp':
      case 'reset_ai_engine':
        return { ok: true, mocked: true }
      case 'get_safety_policy':
        return { mode: 'yolo', tool_overrides: [] }
      case 'get_default_prompts':
        return { prompts: [] }
      default:
        console.warn(`[uClaw mock Tauri IPC] unhandled command: ${cmd}`)
        return null
    }
  }
}

export function installDevTauriMock(): void {
  if (!shouldInstallDevTauriMock() || window.__UCLAW_DEV_TAURI_MOCK__) return

  clearMocks()
  mockWindows('main')
  mockConvertFileSrc('macos')
  mockIPC(createUclawMockIpcHandler(), { shouldMockEvents: true })
  window.__UCLAW_DEV_TAURI_MOCK__ = true

  console.info('[uClaw mock Tauri IPC] installed for browser-only UI debugging')
}
```

- [ ] **Step 4: Install the mock before bridge imports**

Modify the top of `ui/src/main.tsx` so the mock loads before `./lib/tauri-bridge`:

```ts
import { installDevTauriMock } from './lib/dev-tauri-mock'
installDevTauriMock()

// 导入 tauri-bridge 以注册 IPC 适配层
import './lib/tauri-bridge'
```

Keep this import above any code path that calls `@tauri-apps/api/core`, `@tauri-apps/api/event`, or `@tauri-apps/api/window`.

- [ ] **Step 5: Run the test**

Run:

```bash
cd ui
npm test -- src/lib/dev-tauri-mock.test.ts
```

Expected: pass.

- [ ] **Step 6: Run the existing SystemTab test**

Run:

```bash
cd ui
npm test -- src/components/settings/SystemTab.test.tsx
```

Expected: pass.

- [ ] **Step 7: Commit Task 2**

Run:

```bash
git add ui/src/lib/dev-tauri-mock.ts ui/src/lib/dev-tauri-mock.test.ts ui/src/main.tsx
git commit -m "test: add browser-only tauri mock bridge"
```

---

## Task 3: Add A Browser Debug Script Entry

**Files:**
- Modify: `ui/package.json`

- [ ] **Step 1: Add scripts**

Modify `ui/package.json` scripts:

```json
{
  "scripts": {
    "dev": "vite",
    "dev:mock-tauri": "VITE_UCLAW_MOCK_TAURI=1 vite --host 127.0.0.1",
    "build": "vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:ui": "vitest --ui",
    "test:coverage": "vitest run --coverage"
  }
}
```

- [ ] **Step 2: Verify the script starts**

Run:

```bash
cd ui
npm run dev:mock-tauri
```

Expected: Vite prints a local URL at `http://127.0.0.1:5173/`. Stop it with Ctrl-C after confirming startup.

- [ ] **Step 3: Use Playwright or in-app browser**

Open:

```text
http://127.0.0.1:5173/
```

Expected:

- app renders a meaningful uClaw shell;
- console no longer shows `Cannot read properties of undefined (reading 'invoke')`;
- console no longer shows `Cannot read properties of undefined (reading 'transformCallback')`;
- System Diagnostics can display mock diagnostics if the settings route is reachable.

- [ ] **Step 4: Commit Task 3**

Run:

```bash
git add ui/package.json
git commit -m "chore: add mock tauri dev script"
```

---

## Task 4: Document The Active Debug Workflow

**Files:**
- Modify: `docs/superpowers/specs/2026-05-20-uclaw-ui-debug-loop-design.md`
- Create: `docs/superpowers/reports/ui-debug-loop-smoke.md`

- [ ] **Step 1: Append implementation mapping to the spec**

Append this section to `docs/superpowers/specs/2026-05-20-uclaw-ui-debug-loop-design.md`:

```md
---

## Implemented By

- `scripts/ui_debug_smoke.sh` launches Vite + Tauri dev, captures logs, prints process identity, and cleans spawned processes.
- `ui/src/lib/dev-tauri-mock.ts` enables browser-only UI debugging with official Tauri mocks when `VITE_UCLAW_MOCK_TAURI=1`.
- `npm run dev:mock-tauri` opens the React app in browser-debug mode without requiring a Tauri WebView.

Use Computer Use for real desktop proof. Use Playwright or the in-app browser for mock bridge UI iteration. Treat the two paths as complementary evidence, not substitutes.
```

- [ ] **Step 2: Add the smoke report template**

Create `docs/superpowers/reports/ui-debug-loop-smoke.md`:

```md
# UI Debug Loop Smoke Report

Status: implemented as a repeatable development workflow.

## Commands

```bash
bash -n scripts/ui_debug_smoke.sh
cd ui && npm test -- src/lib/dev-tauri-mock.test.ts
cd ui && npm test -- src/components/settings/SystemTab.test.tsx
cd ui && npm run build
```

## Desktop Path

Use:

```bash
UCLAW_UI_DEBUG_KEEP_ALIVE=1 ./scripts/ui_debug_smoke.sh
```

Then inspect `uClaw` with Computer Use and confirm:

- process path includes `target/debug/uclaw`;
- WebView URL is the expected dev URL or a classified mismatch;
- screenshot shows meaningful UI or a classified failure.

## Browser Mock Path

Use:

```bash
cd ui
npm run dev:mock-tauri
```

Then inspect `http://127.0.0.1:5173/` with Playwright or the in-app browser and confirm:

- no missing Tauri IPC errors;
- app shell renders;
- System Diagnostics mock commands return fixtures.

## Classification Labels

- `pass`
- `frontend-runtime-error`
- `tauri-devurl-mismatch`
- `ipc-injection-missing`
- `backend-boot-failure`
- `wrong-app-under-test`
- `inconclusive`
```

- [ ] **Step 3: Commit Task 4**

Run:

```bash
git add docs/superpowers/specs/2026-05-20-uclaw-ui-debug-loop-design.md docs/superpowers/reports/ui-debug-loop-smoke.md
git commit -m "docs: document ui debug loop workflow"
```

---

## Task 5: Final Verification

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run focused frontend tests**

Run:

```bash
cd ui
npm test -- src/lib/dev-tauri-mock.test.ts src/components/settings/SystemTab.test.tsx
```

Expected: both test files pass.

- [ ] **Step 2: Run frontend build**

Run:

```bash
cd ui
npm run build
```

Expected: build exits 0. Existing Vite chunk warnings are acceptable if no new fatal error appears.

- [ ] **Step 3: Run shell syntax check**

Run:

```bash
bash -n scripts/ui_debug_smoke.sh
```

Expected: exit code 0.

- [ ] **Step 4: Run desktop smoke launch**

Run:

```bash
UCLAW_UI_DEBUG_KEEP_ALIVE=1 ./scripts/ui_debug_smoke.sh
```

Expected:

- script prints Vite URL;
- script prints matching process lines for the active worktree;
- Tauri log contains `target/debug/uclaw` or `uClaw started successfully`;
- Computer Use can inspect the visible debug window.

After inspection, stop the script with Ctrl-C and confirm:

```bash
ps -axo pid,ppid,command | rg 'uclaw-worktrees|Documents/uclaw|vite --host 127.0.0.1|target/debug/uclaw'
```

Expected: no debug smoke process remains except the `ps` and `rg` commands themselves.

- [ ] **Step 5: Run browser mock smoke**

Run:

```bash
cd ui
npm run dev:mock-tauri
```

Open `http://127.0.0.1:5173/` with Playwright or the in-app browser.

Expected:

- app renders without `invoke` missing errors;
- app renders without `transformCallback` missing errors;
- console contains `[uClaw mock Tauri IPC] installed for browser-only UI debugging`.

Stop Vite with Ctrl-C.

- [ ] **Step 6: Review dirty state**

Run:

```bash
git status --short
```

Expected: only intended files are modified, unless unrelated pre-existing WIP is explicitly documented in the final report.

---

## Self-Review

Spec coverage:

- Process identity evidence: Task 1 and Task 5.
- Computer Use desktop path: Task 1 output and Task 5 manual inspection.
- Playwright/browser path: Task 2 and Task 3.
- Tauri IPC mock handling: Task 2 uses official Tauri mocks.
- Cleanup discipline: Task 1 trap and Task 5 process check.
- Final report shape: Task 4 report template.

No red-flag planning terms remain. Function names are consistent across tasks: `installDevTauriMock`, `shouldInstallDevTauriMock`, and `createUclawMockIpcHandler`.
