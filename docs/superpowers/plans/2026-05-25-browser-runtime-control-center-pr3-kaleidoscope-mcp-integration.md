# Browser Runtime Control Center PR3 Kaleidoscope MCP Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Playwright MCP as a built-in Kaleidoscope Integrations detail surface and wire Control Center `Configure MCP` to that real route.

**Architecture:** Keep Playwright MCP provider configuration inside the existing Kaleidoscope Integrations module, next to ordinary MCP servers, but model it as a built-in integration card that cannot expose raw MCP tools. Control Center only links to this route once the built-in integration UI exists.

**Tech Stack:** React, Jotai atoms, existing Kaleidoscope Integrations components, TypeScript, Vitest, Testing Library.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `ui/src/atoms/kaleidoscope.ts` | Add selected integration detail atom for built-in Playwright MCP route. |
| `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinCard.tsx` | Built-in integration card shown in Integrations grid. |
| `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx` | Detail panel for status/config/diagnostics with raw MCP exposure locked off. |
| `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx` | Render built-in card and detail route alongside ordinary MCP servers. |
| `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx` | Test built-in card/detail and locked raw tools copy. |
| `ui/src/components/settings/BrowserRuntimeSettings.tsx` | Make `Configure MCP` clickable and route into Kaleidoscope Integrations. |
| `ui/src/components/settings/BrowserRuntimeSettings.test.tsx` | Test real Control Center to Kaleidoscope route. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.ts` | Mark `configureMcpClickable` when report exposes `configureRouteReady`. |
| `src-tauri/src/browser/runtime_control_center.rs` | Set `configure_route_ready: true` now that PR3 owns the route. |

## Boundaries

- This PR does not add raw Playwright MCP tool exposure.
- This PR does not execute browser tasks through MCP.
- This PR does not add a separate settings page for MCP.
- This PR does not change ordinary user-added MCP server semantics.

## ADR 18 Answers

1. Intent: users can configure the advanced Playwright MCP provider from the app-integrations surface.
2. Autonomy: UI/configuration only; no autonomous browser action routing.
3. Truth source: Control Center report plus built-in integration UI state.
4. TaskEvent: no task events.
5. Context: reads Control Center MCP summary and ordinary MCP list data without exposing raw tools.
6. Capability: adds a built-in integration representation for `browser.playwright_mcp`.
7. Hooks: raw MCP tools remain locked off by product UI and backend summary.
8. Projection: Kaleidoscope shows Playwright MCP status/config/diagnostics.
9. Harness: UI tests prove the card appears, detail opens, and raw tools are locked off.
10. Rollback: revert this PR; Control Center returns to non-clickable MCP configure copy.
11. Non-ownership: no provider execution, no probe implementation, no ordinary MCP server migration.

### Task 1: Add Built-In Integration Route State

**Files:**
- Modify: `ui/src/atoms/kaleidoscope.ts`
- Modify: `ui/src/atoms/kaleidoscope.test.ts`

- [ ] **Step 1: Write atom test**

Add:

```ts
import { createStore } from 'jotai'
import { kaleidoscopeModuleAtom, selectedBuiltinIntegrationAtom } from './kaleidoscope'

it('can route to the Playwright MCP built-in integration', () => {
  const store = createStore()
  store.set(kaleidoscopeModuleAtom, 'integrations')
  store.set(selectedBuiltinIntegrationAtom, 'playwright_mcp')

  expect(store.get(kaleidoscopeModuleAtom)).toBe('integrations')
  expect(store.get(selectedBuiltinIntegrationAtom)).toBe('playwright_mcp')
})
```

- [ ] **Step 2: Run failing test**

Run: `cd ui && npm test -- --run src/atoms/kaleidoscope.test.ts`

Expected: FAIL because `selectedBuiltinIntegrationAtom` does not exist.

- [ ] **Step 3: Add atom**

In `kaleidoscope.ts`:

```ts
export type BuiltinIntegrationId = 'playwright_mcp'

export const selectedBuiltinIntegrationAtom = atom<BuiltinIntegrationId | null>(null)
```

- [ ] **Step 4: Run atom test**

Run: `cd ui && npm test -- --run src/atoms/kaleidoscope.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/atoms/kaleidoscope.ts ui/src/atoms/kaleidoscope.test.ts
git commit -m "feat(kaleidoscope): add built-in integration route state" -m "Verification: cd ui && npm test -- --run src/atoms/kaleidoscope.test.ts (expected PASS)"
```

### Task 2: Add Playwright MCP Built-In Integration UI

**Files:**
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinCard.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx`
- Modify: `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx`
- Modify: `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

- [ ] **Step 1: Write integration UI tests**

Add:

```tsx
it('renders Playwright MCP as a built-in advanced integration with raw tools locked off', async () => {
  renderWithProviders(<IntegrationsModule />)

  expect(await screen.findByText('Playwright MCP')).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'Open Playwright MCP integration' }))

  expect(screen.getByText('Built-in integration')).toBeInTheDocument()
  expect(screen.getByText('Raw MCP tools locked off')).toBeInTheDocument()
  expect(screen.getByText('Wrapped browser actions only')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run failing test**

Run: `cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

Expected: FAIL because the built-in card/detail does not exist.

- [ ] **Step 3: Add built-in card**

Create `PlaywrightMcpBuiltinCard.tsx`:

```tsx
import { Badge } from '@/components/ui/badge'

interface PlaywrightMcpBuiltinCardProps {
  selected: boolean
  onClick: () => void
}

export function PlaywrightMcpBuiltinCard({ selected, onClick }: PlaywrightMcpBuiltinCardProps) {
  return (
    <button
      type="button"
      aria-label="Open Playwright MCP integration"
      onClick={onClick}
      className={[
        'min-h-[88px] rounded-lg border p-4 text-left transition-colors',
        selected ? 'border-foreground bg-muted/60' : 'border-border hover:bg-muted/40',
      ].join(' ')}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="text-sm font-medium">Playwright MCP</div>
        <Badge variant="secondary">Advanced</Badge>
      </div>
      <div className="mt-1 text-xs text-muted-foreground">
        Built-in browser provider · wrapped actions only
      </div>
    </button>
  )
}
```

- [ ] **Step 4: Add detail panel**

Create `PlaywrightMcpBuiltinDetail.tsx`:

```tsx
import { Badge } from '@/components/ui/badge'
import { SettingsCard, SettingsRow, SettingsSection } from '@/components/settings/primitives'

export function PlaywrightMcpBuiltinDetail() {
  return (
    <div className="space-y-4 p-4">
      <SettingsSection title="Playwright MCP" description="Built-in integration">
        <SettingsCard>
          <SettingsRow label="Status" description="Advanced provider, configured through Browser Runtime Control Center">
            <Badge variant="secondary">Built-in integration</Badge>
          </SettingsRow>
          <SettingsRow label="Raw MCP exposure" description="Raw MCP tools locked off" />
          <SettingsRow label="Action boundary" description="Wrapped browser actions only" />
          <SettingsRow label="Runtime pack source" description="uClaw-managed Browser Runtime Pack" />
          <SettingsRow label="Sidecar startup" description="App-managed" />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
```

- [ ] **Step 5: Wire into IntegrationsModule**

Use `selectedBuiltinIntegrationAtom` in `IntegrationsModule.tsx`. Render the built-in card before user MCP servers. When selected, render `PlaywrightMcpBuiltinDetail` in the existing detail area or drawer surface. Do not mix it into `servers` because it is not removable.

- [ ] **Step 6: Run UI tests**

Run: `cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinCard.tsx ui/src/views/Kaleidoscope/modules/Integrations/PlaywrightMcpBuiltinDetail.tsx ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx
git commit -m "feat(kaleidoscope): add Playwright MCP built-in integration" -m "Verification: cd ui && npm test -- --run src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx (expected PASS)"
```

### Task 3: Wire Control Center Configure MCP to Real Route

**Files:**
- Modify: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

- [ ] **Step 1: Write Control Center routing test**

Add:

```tsx
it('routes Configure MCP to Kaleidoscope Integrations built-in detail', async () => {
  renderWithProviders(<BrowserRuntimeSettings />)

  await userEvent.click(await screen.findByRole('button', { name: 'Configure Playwright MCP' }))

  expect(store.get(kaleidoscopeModuleAtom)).toBe('integrations')
  expect(store.get(selectedBuiltinIntegrationAtom)).toBe('playwright_mcp')
})
```

- [ ] **Step 2: Run failing test**

Run: `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`

Expected: FAIL because Configure MCP is not clickable.

- [ ] **Step 3: Mark route ready in Rust report**

Change MCP integration summary:

```rust
mcp_integration_summary: BrowserRuntimeMcpIntegrationSummary {
    built_in: true,
    enabled: config.playwright_mcp_enabled,
    raw_tools_exposed: false,
    configure_route_ready: true,
},
```

- [ ] **Step 4: Wire frontend route**

In `BrowserRuntimeSettings.tsx`, import:

```ts
import { useSetAtom } from 'jotai'
import { kaleidoscopeModuleAtom, selectedBuiltinIntegrationAtom } from '@/atoms/kaleidoscope'
```

Add:

```ts
const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
const setSelectedBuiltinIntegration = useSetAtom(selectedBuiltinIntegrationAtom)

const openPlaywrightMcpIntegration = React.useCallback(() => {
  setKaleidoscopeModule('integrations')
  setSelectedBuiltinIntegration('playwright_mcp')
}, [setKaleidoscopeModule, setSelectedBuiltinIntegration])
```

Render:

```tsx
<Button
  type="button"
  variant="outline"
  size="sm"
  aria-label="Configure Playwright MCP"
  onClick={openPlaywrightMcpIntegration}
>
  Configure MCP
</Button>
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_control_center.rs ui/src/lib/browser-runtime/browser-runtime-control-center.ts ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx
git commit -m "feat(browser-runtime): route MCP configure to Kaleidoscope" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center && cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx (expected PASS)"
```

## Final Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cd ui && npm test -- --run src/atoms/kaleidoscope.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx
npm --prefix ui run build
git diff --check
```

Expected:

- Rust tests PASS.
- Vitest files PASS.
- UI build exits 0.
- `git diff --check` exits 0.
