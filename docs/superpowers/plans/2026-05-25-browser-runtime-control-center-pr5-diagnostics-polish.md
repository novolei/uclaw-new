# Browser Runtime Control Center PR5 Diagnostics Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Polish Browser Runtime Control Center diagnostics with artifact links, probe history, raw JSON report, clearer status vocabulary, and manual frontend validation.

**Architecture:** Keep diagnostics read-only by default. Add compact probe history to the Rust report, expose raw JSON in a collapsed UI panel, and add frontend tests that prevent regression to static/mock-looking text. Finish with manual Browser verification from Settings and Kaleidoscope.

**Tech Stack:** Rust serde models, React, TypeScript, Vitest, Testing Library, in-app Browser/manual QA.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/browser/runtime_provider_probe.rs` | Probe history model and append/trim helpers. |
| `src-tauri/src/browser/runtime_control_center.rs` | Include probe history and raw diagnostic report metadata. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.ts` | Diagnostic view-model helpers for artifacts, JSON, and status vocabulary. |
| `ui/src/components/settings/BrowserRuntimeSettings.tsx` | Collapsed diagnostics section and polished copy. |
| `ui/src/components/settings/BrowserRuntimeSettings.test.tsx` | Regression tests for no static/mock copy and status labels. |
| `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx` | Show MCP probe/artifact diagnostics in the built-in detail. |
| `docs/superpowers/reports/2026-05-25-browser-runtime-control-center-frontend-validation.md` | Manual frontend validation notes and screenshots/checklist. |

## Boundaries

- This PR does not add new provider execution behavior.
- This PR does not change provider priority semantics.
- This PR does not expose raw MCP tools.
- This PR does not redesign unrelated Settings or Kaleidoscope modules.

## ADR 18 Answers

1. Intent: users can understand why a provider is active, skipped, blocked, or healthy.
2. Autonomy: read-only diagnostics plus probe history display.
3. Truth source: Rust Control Center report and persisted probe summaries.
4. TaskEvent: no new TaskEvent writes; diagnostics display event names already produced.
5. Context: reads provider route/probe metadata and artifact ids.
6. Capability: improves observability for CLI/MCP/Local Chromium lanes.
7. Hooks: regression tests block misleading static/mock labels.
8. Projection: raw JSON and summarized diagnostics render in Control Center and MCP integration detail.
9. Harness: UI tests and manual app validation cover Settings and Kaleidoscope flows.
10. Rollback: revert polish PR; PR1-PR4 functionality remains.
11. Non-ownership: no route promotion, no probe behavior changes, no raw MCP exposure.

### Task 1: Add Probe History Summary

**Files:**
- Modify: `src-tauri/src/browser/runtime_provider_probe.rs`
- Modify: `src-tauri/src/browser/runtime_control_center.rs`

- [ ] **Step 1: Write history tests**

Add:

```rust
#[test]
fn probe_history_keeps_latest_five_entries_newest_first() {
    let history = (0..7)
        .map(|idx| BrowserRuntimeProviderProbeSummary::passed("browser.playwright_cli", idx))
        .fold(Vec::new(), append_probe_history);

    assert_eq!(history.len(), 5);
    assert_eq!(history[0].checked_at_ms, 6);
    assert_eq!(history[4].checked_at_ms, 2);
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe`

Expected: FAIL because `append_probe_history` does not exist.

- [ ] **Step 3: Implement history helper**

Add:

```rust
pub fn append_probe_history(
    mut history: Vec<BrowserRuntimeProviderProbeSummary>,
    summary: BrowserRuntimeProviderProbeSummary,
) -> Vec<BrowserRuntimeProviderProbeSummary> {
    history.insert(0, summary);
    history.sort_by(|left, right| right.checked_at_ms.cmp(&left.checked_at_ms));
    history.truncate(5);
    history
}
```

Add `provider_probe_history: BTreeMap<String, Vec<BrowserRuntimeProviderProbeSummary>>` to provider config and include per-lane `probe_history` in the report.

- [ ] **Step 4: Run Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe browser::runtime_control_center`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/runtime_provider_probe.rs src-tauri/src/browser/runtime_control_center.rs src-tauri/src/settings.rs
git commit -m "feat(browser-runtime): keep provider probe history" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe browser::runtime_control_center (expected PASS)"
```

### Task 2: Polish Diagnostics UI

**Files:**
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

- [ ] **Step 1: Write diagnostics tests**

Add:

```ts
it('uses product status vocabulary instead of raw unavailable/setup copy', () => {
  const model = deriveBrowserRuntimeControlCenterViewModel(reportWithDisabledMcp())

  expect(model.providerRows.map((row) => row.statusLabel)).toContain('Off')
  expect(JSON.stringify(model)).not.toContain('feature flag disabled')
  expect(JSON.stringify(model)).not.toContain('setup 未完成')
})

it('renders raw JSON diagnostics collapsed by default', () => {
  renderWithProviders(<BrowserRuntimeSettings />)

  expect(screen.getByRole('button', { name: 'Show raw Browser Runtime report' })).toBeInTheDocument()
  expect(screen.queryByText('"desiredProviderPriority"')).not.toBeInTheDocument()
})
```

- [ ] **Step 2: Run failing tests**

Run: `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`

Expected: FAIL until diagnostics view model/UI is added.

- [ ] **Step 3: Add diagnostics view model**

Add:

```ts
export function rawControlCenterJson(report?: BrowserRuntimeControlCenterReport): string {
  if (!report) return '{}'
  return JSON.stringify(report, null, 2)
}

export function artifactLabel(artifactId?: string): string {
  return artifactId ? artifactId : 'No artifact yet'
}
```

Ensure `laneStatusLabel` returns only these product labels:

```ts
const ALLOWED_STATUS_LABELS = [
  'Off',
  'Enabled',
  'Needs runtime pack',
  'Needs probe',
  'Probe failed',
  'Ready',
  'Active',
  'Fallback active',
  'Advanced',
  'Not routable',
] as const
```

- [ ] **Step 4: Add collapsed diagnostics UI**

In `BrowserRuntimeSettings.tsx`, add:

```tsx
const [rawReportOpen, setRawReportOpen] = React.useState(false)

<SettingsSection title="Diagnostics">
  <SettingsCard>
    <SettingsRow label="Route evidence" description={controlModel.routeSummary.reasonLabel} />
    <SettingsRow label="Probe artifacts" description={controlModel.providerRows.map((row) => artifactLabel(row.lane.lastProbeArtifact)).join(' · ')} />
    <button
      type="button"
      className="min-h-11 px-4 text-left text-sm"
      aria-label={rawReportOpen ? 'Hide raw Browser Runtime report' : 'Show raw Browser Runtime report'}
      onClick={() => setRawReportOpen((open) => !open)}
    >
      {rawReportOpen ? 'Hide raw report' : 'Show raw report'}
    </button>
    {rawReportOpen ? (
      <pre className="max-h-80 overflow-auto rounded-md bg-muted p-4 text-xs">
        {rawControlCenterJson(controlCenter)}
      </pre>
    ) : null}
  </SettingsCard>
</SettingsSection>
```

- [ ] **Step 5: Run frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/browser-runtime/browser-runtime-control-center.ts ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx
git commit -m "feat(browser-runtime): polish control center diagnostics" -m "Verification: cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx (expected PASS)"
```

### Task 3: Add MCP Detail Diagnostics

**Files:**
- Modify: `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx`
- Modify: `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

- [ ] **Step 1: Write MCP detail diagnostics test**

Add:

```tsx
it('shows Playwright MCP probe diagnostics without raw tool exposure', async () => {
  renderWithProviders(<IntegrationsModule />)
  await userEvent.click(await screen.findByRole('button', { name: 'Open Playwright MCP integration' }))

  expect(screen.getByText('Last sidecar probe')).toBeInTheDocument()
  expect(screen.getByText('Last action envelope')).toBeInTheDocument()
  expect(screen.getByText('Raw MCP tools locked off')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run failing test**

Run: `cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

Expected: FAIL until detail rows are added.

- [ ] **Step 3: Add diagnostics rows**

Add rows:

```tsx
<SettingsRow label="Last sidecar probe" description="Read from Browser Runtime Control Center probe history" />
<SettingsRow label="Last action envelope" description="uClaw-wrapped action envelope only" />
<SettingsRow label="Last artifact/error route" description="Artifacts stay under Browser Runtime Supervisor ownership" />
```

- [ ] **Step 4: Run test**

Run: `cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx
git commit -m "feat(browser-runtime): show MCP integration diagnostics" -m "Verification: cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx (expected PASS)"
```

### Task 4: Manual Frontend Validation Report

**Files:**
- Create: `docs/superpowers/reports/2026-05-25-browser-runtime-control-center-frontend-validation.md`

- [ ] **Step 1: Start app**

Run:

```bash
npm --prefix ui run build
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center browser::runtime_provider_probe
```

Expected: build exits 0 and Rust tests PASS.

- [ ] **Step 2: Validate Settings manually**

Open the app and verify:

```text
Settings > Browser Runtime shows:
- Browser Runtime Control Center first
- Desired route: Playwright CLI > Playwright MCP > Local Chromium
- Active route is computed from Rust report
- Run probe buttons call IPC and refresh state
- Configure MCP routes to Kaleidoscope only after PR3
- No static "Prepare the pinned Browser runtime pack..." button-looking text
- No misleading "No active local Chromium context" global warning
```

- [ ] **Step 3: Validate Kaleidoscope manually**

Open:

```text
Kaleidoscope > Integrations > Playwright MCP
```

Verify:

```text
- Built-in integration is visible
- Advanced label is visible
- Raw MCP tools locked off
- Wrapped browser actions only
- Diagnostics rows are visible
```

- [ ] **Step 4: Write report**

Create the report:

```markdown
# Browser Runtime Control Center Frontend Validation

Date: 2026-05-25

## Commands

- npm --prefix ui run build
- cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center browser::runtime_provider_probe

## Settings Validation

- Browser Runtime Control Center renders first: PASS
- Desired route shows CLI > MCP > Local Chromium: PASS
- Active route is Rust-derived: PASS
- Probe controls refresh report: PASS
- Static/mock-looking action preview copy removed: PASS

## Kaleidoscope Validation

- Playwright MCP built-in integration card renders: PASS
- Detail view shows Advanced/provider guardrails: PASS
- Raw MCP tools remain locked off: PASS

## Notes

No unrelated Settings or Integrations surfaces were redesigned in this PR.
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/reports/2026-05-25-browser-runtime-control-center-frontend-validation.md
git commit -m "docs(browser-runtime): record control center frontend validation" -m "Verification: npm --prefix ui run build && cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center browser::runtime_provider_probe (expected PASS)"
```

## Final Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center browser::runtime_provider_probe
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx
npm --prefix ui run build
git diff --check
```

Expected:

- Rust tests PASS.
- Vitest files PASS.
- UI build exits 0.
- `git diff --check` exits 0.
