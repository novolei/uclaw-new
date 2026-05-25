# Browser Runtime Real Pack Install PR3 UI Integration And E2E Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect Browser Runtime Control Center UI to the real confirmed runtime-pack execution path so `Needs runtime pack` becomes dry-run -> confirm -> execute -> refresh -> probe.

**Architecture:** Add a frontend bridge for `execute_browser_runtime_action`, extend the Control Center view model so runtime-pack-not-ready lanes choose `Prepare runtime pack` instead of `Run probe`, and add a confirmation panel in `BrowserRuntimeSettings`. Keep provider probes disabled until runtime pack readiness is true.

**Tech Stack:** React 18, TypeScript, Vitest, Tauri invoke bridge, existing Browser Runtime Settings components.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `ui/src/lib/tauri-bridge.ts` | Add `executeBrowserRuntimeAction` wrapper. |
| `ui/src/lib/tauri-bridge.browser-runtime.test.ts` | Verify invoke payload. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.ts` | Change provider action labels/gates for runtime-pack-not-ready. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts` | View-model tests for prepare-vs-probe behavior. |
| `ui/src/components/settings/BrowserRuntimeSettings.tsx` | Add dry-run -> confirm -> execute UI and refresh behavior. |
| `ui/src/components/settings/BrowserRuntimeSettings.test.tsx` | UI tests for prepare confirmation and execution refresh. |
| `ui/src/lib/dev-tauri-mock.ts` | Mock execute IPC for browser-only smoke. |
| `ui/src/lib/dev-tauri-mock.test.ts` | Verify mock command. |
| `docs/superpowers/reports/2026-05-26-browser-runtime-real-pack-e2e-validation.md` | Manual validation evidence after generator + app smoke. |

## Boundaries

- This PR assumes PR1 and PR2 are merged.
- This PR does not change generator internals.
- This PR does not expose raw MCP tools.
- This PR does not remove Local Chromium fallback.
- This PR should not redesign unrelated Settings sections.

## ADR 18 Answers

1. Intent: let the user prepare the real runtime pack from Settings.
2. Autonomy: confirmed local runtime installation from app-managed source.
3. Truth source: Rust IPC reports and refreshed Control Center status.
4. TaskEvent: execution report event names and artifacts are shown in UI.
5. Context: current provider lanes, runtime pack actions, dry-run report, confirmed execution result.
6. Capability: unlocks CLI/MCP probes after runtime pack readiness.
7. Hooks: confirmation UI, bridge tests, view-model tests, UI tests, E2E smoke.
8. Projection: user sees source/target/steps and route transition.
9. Harness: Vitest plus manual macOS arm64 generator/install/probe validation.
10. Rollback: disable providers or keep Local Chromium fallback.
11. Non-ownership: no packaging/download redesign, no raw MCP exposure.

### Task 1: Add Execute Bridge

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/lib/tauri-bridge.browser-runtime.test.ts`

- [ ] **Step 1: Write bridge test**

Add to `tauri-bridge.browser-runtime.test.ts`:

```ts
it('invokes execute_browser_runtime_action with confirmation flag', async () => {
  invoke.mockResolvedValueOnce({
    operation: 'prepare',
    mode: 'managed',
    status: 'succeeded',
    summary: 'Installed runtime pack.',
    artifactId: 'browser-runtime-prepare-browser-runtime-pack-v1-succeeded',
    eventNames: ['browser.runtime.prepare.succeeded'],
    stepReports: [],
    manifestPackVersion: 'browser-runtime-pack-v1',
    runtimeRoot: '/tmp/runtime',
    currentPackDir: '/tmp/runtime/packs/browser-runtime-pack-v1',
    usesNetwork: false,
    destructive: false,
    requiresConfirmation: false,
    keepsCurrentPack: false,
  })

  await executeBrowserRuntimeAction('prepare', true)

  expect(invoke).toHaveBeenCalledWith('execute_browser_runtime_action', {
    action: 'prepare',
    confirmed: true,
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix ui test -- --run src/lib/tauri-bridge.browser-runtime.test.ts
```

Expected: FAIL because `executeBrowserRuntimeAction` is not exported.

- [ ] **Step 3: Implement bridge**

In `ui/src/lib/tauri-bridge.ts`, add near `dryRunBrowserRuntimeAction`:

```ts
export const executeBrowserRuntimeAction = (
  action: BrowserRuntimePackAction,
  confirmed: boolean,
): Promise<BrowserRuntimePackExecutionReport> =>
  invoke('execute_browser_runtime_action', { action, confirmed });
```

- [ ] **Step 4: Run bridge test**

Run:

```bash
npm --prefix ui test -- --run src/lib/tauri-bridge.browser-runtime.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/lib/tauri-bridge.browser-runtime.test.ts
git commit -m "feat(browser-runtime): add runtime pack execute bridge" -m "Verification: npm --prefix ui test -- --run src/lib/tauri-bridge.browser-runtime.test.ts (expected PASS)"
```

### Task 2: Change Control Center Provider Actions

**Files:**
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`

- [ ] **Step 1: Write view-model test**

Add:

```ts
it('uses prepare runtime pack instead of run probe when provider is pack-gated', () => {
  const model = deriveBrowserRuntimeControlCenterViewModel({
    featureFlags: {
      playwrightCli: true,
      playwrightMcp: false,
      hostedBrowser: false,
      forceLegacyLocalChromium: false,
    },
    desiredProviderPriority: [
      'browser.playwright_cli',
      'browser.playwright_mcp',
      'browser.local_chromium',
    ],
    activeProviderRoute: {
      providerId: 'browser.local_chromium',
      displayName: 'Local Chromium',
      fallbackReason: 'runtime_pack_not_ready',
    },
    providerLanes: [
      {
        providerId: 'browser.playwright_cli',
        displayName: 'Playwright CLI',
        enabled: true,
        priorityRank: 1,
        readiness: 'needs_setup',
        routable: false,
        routeRole: 'desired_first',
        probeState: 'not_run',
        fallbackReason: 'runtime_pack_not_ready',
        nextAction: 'prepare_runtime_pack',
        probeHistory: [],
      },
      {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
        enabled: true,
        priorityRank: 2,
        readiness: 'ready',
        routable: true,
        routeRole: 'active',
        probeState: 'passed',
        nextAction: 'none',
        probeHistory: [],
      },
    ],
    mcpIntegrationSummary: {
      builtIn: true,
      enabled: false,
      rawToolsExposed: false,
      configureRouteReady: true,
    },
    updatedAtMs: 1,
  })

  const cli = model.providerRows.find((row) => row.lane.providerId === 'browser.playwright_cli')
  expect(cli?.statusLabel).toBe('Needs runtime pack')
  expect(cli?.nextActionLabel).toBe('Prepare runtime pack')
  expect(cli?.canRunProbe).toBe(false)
  expect(cli?.canPrepareRuntimePack).toBe(true)
})
```

- [ ] **Step 2: Run failing test**

Run:

```bash
npm --prefix ui test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts
```

Expected: FAIL because `canPrepareRuntimePack` is not defined.

- [ ] **Step 3: Update row view model**

In `browser-runtime-control-center.ts`, extend `BrowserRuntimeProviderRowViewModel`:

```ts
canPrepareRuntimePack: boolean
```

Update row mapping:

```ts
const packGated = lane.fallbackReason === 'runtime_pack_not_ready'
return {
  lane,
  statusLabel: laneStatusLabel(lane),
  nextActionLabel: nextActionLabel(lane.nextAction),
  configureMcpClickable:
    lane.providerId === 'browser.playwright_mcp' &&
    report.mcpIntegrationSummary.configureRouteReady,
  canEnable: lane.providerId !== 'browser.local_chromium' && !lane.enabled,
  canSetFirst: lane.providerId !== report.desiredProviderPriority[0],
  canPrepareRuntimePack: packGated,
  canRunProbe:
    lane.enabled &&
    !packGated &&
    (lane.providerId === 'browser.playwright_cli' ||
      lane.providerId === 'browser.playwright_mcp'),
  isFirst: lane.providerId === report.desiredProviderPriority[0],
}
```

- [ ] **Step 4: Run view-model tests**

Run:

```bash
npm --prefix ui test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/browser-runtime/browser-runtime-control-center.ts ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts
git commit -m "feat(browser-runtime): route pack-gated providers to prepare action" -m "Verification: npm --prefix ui test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts (expected PASS)"
```

### Task 3: Add Confirmed Execute UI

**Files:**
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

- [ ] **Step 1: Write UI test for prepare action from provider row**

Add:

```tsx
it('shows dry-run then confirm install for pack-gated provider', async () => {
  const user = userEvent.setup()
  vi.mocked(dryRunBrowserRuntimeAction).mockResolvedValueOnce({
    operation: 'prepare',
    mode: 'dry_run',
    status: 'succeeded',
    summary: 'Prepare the pinned Browser runtime pack.',
    artifactId: 'browser-runtime-prepare-browser-runtime-pack-v1-succeeded',
    eventNames: ['browser.runtime.prepare.dry_run_succeeded'],
    stepReports: [],
    manifestPackVersion: 'browser-runtime-pack-v1',
    runtimeRoot: '/Users/test/.uclaw/browser-runtime',
    currentPackDir: '/Users/test/.uclaw/browser-runtime/packs/browser-runtime-pack-v1',
    usesNetwork: false,
    destructive: false,
    requiresConfirmation: true,
    keepsCurrentPack: false,
  })
  vi.mocked(executeBrowserRuntimeAction).mockResolvedValueOnce({
    operation: 'prepare',
    mode: 'managed',
    status: 'succeeded',
    summary: 'Installed Browser runtime pack.',
    artifactId: 'browser-runtime-prepare-browser-runtime-pack-v1-succeeded',
    eventNames: ['browser.runtime.prepare.succeeded'],
    stepReports: [],
    manifestPackVersion: 'browser-runtime-pack-v1',
    runtimeRoot: '/Users/test/.uclaw/browser-runtime',
    currentPackDir: '/Users/test/.uclaw/browser-runtime/packs/browser-runtime-pack-v1',
    usesNetwork: false,
    destructive: false,
    requiresConfirmation: false,
    keepsCurrentPack: false,
  })
  render(<BrowserRuntimeSettings />)

  await user.click(await screen.findByRole('button', { name: /Prepare runtime pack/i }))
  expect(await screen.findByText('Prepare the pinned Browser runtime pack.')).toBeInTheDocument()

  await user.click(screen.getByRole('button', { name: /Confirm install/i }))
  expect(executeBrowserRuntimeAction).toHaveBeenCalledWith('prepare', true)
  expect(await screen.findByText('Installed Browser runtime pack.')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run failing UI test**

Run:

```bash
npm --prefix ui test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: FAIL because `executeBrowserRuntimeAction` and confirm UI are not wired.

- [ ] **Step 3: Import execute bridge and add state**

In `BrowserRuntimeSettings.tsx`, import:

```ts
executeBrowserRuntimeAction,
```

Add state near dry-run state:

```ts
const [executeReports, setExecuteReports] = React.useState<
  Partial<Record<BrowserRuntimeSettingsAction['id'], BrowserRuntimePackExecutionReport>>
>({})
const [executeErrors, setExecuteErrors] = React.useState<Partial<Record<BrowserRuntimeSettingsAction['id'], string>>>({})
const [executePendingActionId, setExecutePendingActionId] =
  React.useState<BrowserRuntimeSettingsAction['id'] | null>(null)
```

- [ ] **Step 4: Add execute helper**

Add callback:

```ts
const executeRuntimeAction = React.useCallback(async (actionId: BrowserRuntimePackAction) => {
  if (status || executePendingActionId) return

  setExecutePendingActionId(actionId)
  setExecuteErrors((current) => {
    const next = { ...current }
    delete next[actionId]
    return next
  })
  try {
    const report = await executeBrowserRuntimeAction(actionId, true)
    if (mountedRef.current) {
      setExecuteReports((current) => ({ ...current, [actionId]: report }))
      await refreshLiveStatus()
      await refreshControlCenter()
    }
  } catch (error) {
    if (mountedRef.current) {
      setExecuteErrors((current) => ({
        ...current,
        [actionId]: error instanceof Error ? error.message : String(error),
      }))
    }
  } finally {
    if (mountedRef.current) {
      setExecutePendingActionId(null)
    }
  }
}, [executePendingActionId, refreshControlCenter, refreshLiveStatus, status])
```

- [ ] **Step 5: Wire provider row prepare action**

Extend `ProviderPriorityRowProps`:

```ts
onPrepareRuntimePack: () => void
```

Pass from parent:

```tsx
onPrepareRuntimePack={() => {
  setSelectedActionId('prepare')
  if (!status) void dryRunAction('prepare')
}}
```

Inside `ProviderPriorityRow`, render before `canEnable`:

```tsx
{row.canPrepareRuntimePack ? (
  <Button
    type="button"
    variant="outline"
    size="sm"
    disabled={disabled}
    aria-label="Prepare runtime pack"
    onClick={onPrepareRuntimePack}
  >
    <Download />
    Prepare runtime pack
  </Button>
) : row.canEnable ? (
  // existing enable button
```

- [ ] **Step 6: Add confirm button to operation preview**

In the selected action preview card, derive:

```ts
const selectedExecuteReport = selectedAction ? executeReports[selectedAction.id] : undefined
const selectedExecuteError = selectedAction ? executeErrors[selectedAction.id] : undefined
const selectedExecutePending = selectedAction?.id === executePendingActionId
```

Change description precedence:

```tsx
selectedExecutePending
  ? '正在执行后端安装。'
  : selectedExecuteError
    ?? selectedExecuteReport?.summary
    ?? selectedDryRunError
    ?? selectedDryRunReport?.summary
    ?? selectedAction.preview.summary
```

Add confirm button inside the preview `SettingsCard` for prepare/repair:

```tsx
{selectedAction && isDryRunAction(selectedAction.id) && selectedDryRunReport ? (
  <div className="flex justify-end p-4 pt-0">
    <Button
      type="button"
      variant="default"
      size="sm"
      disabled={selectedExecutePending || Boolean(status)}
      onClick={() => {
        void executeRuntimeAction(selectedAction.id)
      }}
    >
      <Download />
      {selectedExecutePending ? 'Installing' : 'Confirm install'}
    </Button>
  </div>
) : null}
```

For actions other than `prepare` and `repair`, hide the confirm button:

```tsx
{selectedAction && (selectedAction.id === 'prepare' || selectedAction.id === 'repair') && selectedDryRunReport ? ( ... ) : null}
```

- [ ] **Step 7: Run UI test**

Run:

```bash
npm --prefix ui test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx
git commit -m "feat(browser-runtime): confirm runtime pack install from settings" -m "Verification: npm --prefix ui test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx (expected PASS)"
```

### Task 4: Update Dev Tauri Mock

**Files:**
- Modify: `ui/src/lib/dev-tauri-mock.ts`
- Modify: `ui/src/lib/dev-tauri-mock.test.ts`

- [ ] **Step 1: Write mock test**

Add:

```ts
await expect(await handler('execute_browser_runtime_action', {
  action: 'prepare',
  confirmed: true,
})).toMatchObject({
  operation: 'prepare',
  mode: 'managed',
  status: 'succeeded',
})
```

- [ ] **Step 2: Run failing mock test**

Run:

```bash
npm --prefix ui test -- --run src/lib/dev-tauri-mock.test.ts
```

Expected: FAIL because mock command is not handled.

- [ ] **Step 3: Add mock handler**

In `dev-tauri-mock.ts`, add case:

```ts
case 'execute_browser_runtime_action':
  return {
    operation: payload?.action ?? 'prepare',
    mode: 'managed',
    status: payload?.confirmed ? 'succeeded' : 'requires_confirmation',
    summary: payload?.confirmed
      ? 'Mock runtime pack install completed.'
      : 'Confirm Browser runtime pack installation before writing files.',
    artifactId: 'browser-runtime-mock-install',
    eventNames: ['browser.runtime.mock.install_succeeded'],
    stepReports: [],
    manifestPackVersion: browserRuntimeStatusFixture.manifestPackVersion,
    runtimeRoot: browserRuntimeStatusFixture.runtimeRoot,
    currentPackDir: browserRuntimeStatusFixture.currentPackDir,
    usesNetwork: false,
    destructive: false,
    requiresConfirmation: !payload?.confirmed,
    keepsCurrentPack: false,
  }
```

- [ ] **Step 4: Run mock test**

Run:

```bash
npm --prefix ui test -- --run src/lib/dev-tauri-mock.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/dev-tauri-mock.ts ui/src/lib/dev-tauri-mock.test.ts
git commit -m "test(browser-runtime): mock runtime pack execute ipc" -m "Verification: npm --prefix ui test -- --run src/lib/dev-tauri-mock.test.ts (expected PASS)"
```

### Task 5: E2E Validation Report

**Files:**
- Create: `docs/superpowers/reports/2026-05-26-browser-runtime-real-pack-e2e-validation.md`

- [ ] **Step 1: Run focused frontend tests**

Run:

```bash
npm --prefix ui test -- --run \
  src/lib/tauri-bridge.browser-runtime.test.ts \
  src/lib/browser-runtime/browser-runtime-control-center.test.ts \
  src/components/settings/BrowserRuntimeSettings.test.tsx \
  src/lib/dev-tauri-mock.test.ts
```

Expected: PASS.

- [ ] **Step 2: Run UI build**

Run:

```bash
npm --prefix ui run build
```

Expected: PASS. Existing Vite chunk/dynamic import warnings may remain.

- [ ] **Step 3: Run manual full-pack validation on macOS arm64**

Run:

```bash
node scripts/browser-runtime/generate-runtime-pack.mjs
node scripts/browser-runtime/validate-runtime-pack.mjs src-tauri/.runtime-pack-staging/browser-runtime-pack-v1
cargo tauri dev
```

Expected:

- generator succeeds
- validator prints `Runtime pack valid`
- Settings shows `Prepare runtime pack`
- confirmed install creates `~/.uclaw/browser-runtime/packs/browser-runtime-pack-v1`
- refresh changes CLI/MCP from `Needs runtime pack` to `Needs probe`
- `Run probe` for CLI passes
- active route becomes `Playwright CLI`

- [ ] **Step 4: Write validation report**

Create `docs/superpowers/reports/2026-05-26-browser-runtime-real-pack-e2e-validation.md`:

```markdown
# Browser Runtime Real Pack E2E Validation

Date: 2026-05-26

## Automated Commands

- `npm --prefix ui test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/lib/dev-tauri-mock.test.ts`: PASS.
- `npm --prefix ui run build`: PASS.

## Manual macOS arm64 Runtime Pack

- `node scripts/browser-runtime/generate-runtime-pack.mjs`: PASS.
- `node scripts/browser-runtime/validate-runtime-pack.mjs src-tauri/.runtime-pack-staging/browser-runtime-pack-v1`: PASS.
- `cargo tauri dev`: PASS.

## Product Flow

- Initial Settings provider lanes show Playwright CLI/MCP as `Needs runtime pack`: PASS.
- `Prepare runtime pack` opens dry-run preview before execution: PASS.
- `Confirm install` executes `execute_browser_runtime_action(prepare, true)`: PASS.
- Installed pack exists at `~/.uclaw/browser-runtime/packs/browser-runtime-pack-v1`: PASS.
- Runtime status reports ready and can run browser tasks: PASS.
- Playwright CLI probe passes: PASS.
- Active route becomes Playwright CLI: PASS.

## Notes

- Local Chromium remained available as fallback.
- Raw Playwright MCP tools remained hidden.
```

- [ ] **Step 5: Commit report**

```bash
git add docs/superpowers/reports/2026-05-26-browser-runtime-real-pack-e2e-validation.md
git commit -m "docs(browser-runtime): record real pack e2e validation" -m "Verification: npm --prefix ui test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/lib/dev-tauri-mock.test.ts; npm --prefix ui run build; node scripts/browser-runtime/generate-runtime-pack.mjs; node scripts/browser-runtime/validate-runtime-pack.mjs src-tauri/.runtime-pack-staging/browser-runtime-pack-v1; cargo tauri dev (expected PASS on macOS arm64)"
```

### Task 6: Final PR3 Verification

**Files:**
- Verify only.

- [ ] **Step 1: Run final focused tests**

Run:

```bash
npm --prefix ui test -- --run \
  src/lib/tauri-bridge.browser-runtime.test.ts \
  src/lib/browser-runtime/browser-runtime-control-center.test.ts \
  src/components/settings/BrowserRuntimeSettings.test.tsx \
  src/lib/dev-tauri-mock.test.ts
npm --prefix ui run build
git diff --check
```

Expected: PASS / exit 0.

- [ ] **Step 2: Run GitNexus detect**

Run:

```bash
npx gitnexus detect-changes --scope staged --repo uclaw-new
```

Expected: exit 0. Include stale-index warnings in PR body if present.

- [ ] **Step 3: Final status**

Run:

```bash
git status --short --branch
```

Expected: clean working tree, branch ahead of `origin/main`.
