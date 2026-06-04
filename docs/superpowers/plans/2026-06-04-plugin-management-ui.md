# Plugin Management UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** A lightweight Settings panel ("插件") listing installed plugins with an enable/disable toggle + MCP-connection status, calling the already-shipped `list_plugins`/`set_plugin_enabled` commands.

**Spec:** `docs/superpowers/specs/2026-06-04-plugin-management-ui-design.md`

**Stack:** React + TS + Vite (`ui/`), Radix + Tailwind, jotai, sonner toast, vitest + @testing-library.

---

## Pinned facts (verbatim — do not re-derive)

- **Backend `PluginInfo` is snake_case** (`tauri_commands.rs:17742`, `#[derive(serde::Serialize)]`, NO rename_all): `{ id, display_name, version, enabled, mcp_connected }`. So the TS interface uses `display_name` + `mcp_connected` (snake). `list_plugins` → `Result<Vec<PluginInfo>,Error>`; `set_plugin_enabled(id,enabled)` → `Result<(),Error>` → TS `Promise<void>`.
- `ui/src/atoms/settings-tab.ts:9-24` — `SettingsTab` union (add `| 'plugins'`).
- `ui/src/components/settings/SettingsNav.tsx`: lucide import block (line ~10-13, add `Puzzle`); `GROUPS` (28-59); 核心 group items end at `{ id: 'imChannels', ... }` — insert `{ id: 'plugins', label: '插件', icon: <Puzzle size={16} /> }` right after the `{ id: 'tools', ... }` item (adjacent to 工具与能力).
- `ui/src/components/settings/SettingsPanel.tsx`: imports end ~line 23 (add `import { PluginsSettings } from './PluginsSettings'`); `SettingsContent` switch (26-61, add `case 'plugins': return <PluginsSettings />`); `TAB_LABEL` (70-86, add `plugins: '插件',`).
- `tauri-bridge.ts`: `import { invoke } from '@tauri-apps/api/core'` (line 11); bindings like `export const toggleMcpServer = (id, enabled): Promise<boolean> => invoke('toggle_mcp_server', { id, enabled })` (~1204-1227); types imported from `./types`.
- `lib/types.ts`: `McpServerInfo` interface (~856) — add `PluginInfo` here in the same style.
- Primitives: `SettingsSection { title?, description?, action?, children, className? }`; `SettingsCard { children, className?, divided? }`; `SettingsRow { label, icon?, description?, children?, className? }`; `Switch` (Radix) takes `checked` / `onCheckedChange` / `disabled`; `cn` from `@/lib/utils`.
- ToolSettings pattern: `import { toast } from 'sonner'`; `useState<T|null>(null)` + `useState(false)` loading; `useCallback(async()=>{ setLoading(true); try{...}catch(e){toast.error('…',{description:String(e)})}finally{setLoading(false)} },[])`; `useEffect(()=>{ void refresh() },[refresh])`.
- Test precedent `ui/src/components/settings/ImChannelsSettings.test.tsx`: `import { describe,it,expect,vi,beforeEach } from 'vitest'`; `import { fireEvent, waitFor } from '@testing-library/react'`; `import { renderWithProviders, screen } from '@/test-utils/render'`; `const invokeMock = vi.fn(); vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a:unknown[]) => invokeMock(...a) })); vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))`. Verify: `cd ui && npx tsc --noEmit` + `cd ui && npm test -- --run`.
- **NEW file `PluginsSettings.tsx` needs explicit `git add`.**

---

## Task 1: Type + bridge bindings + `PluginsSettings.tsx` + test

**Files:** Modify `ui/src/lib/types.ts`, `ui/src/lib/tauri-bridge.ts`; Create `ui/src/components/settings/PluginsSettings.tsx` + `PluginsSettings.test.tsx`

- [ ] **Step 1: `PluginInfo` type** — in `ui/src/lib/types.ts`, near `McpServerInfo`:
```ts
export interface PluginInfo {
  id: string
  display_name: string
  version: string
  enabled: boolean
  mcp_connected: boolean
}
```

- [ ] **Step 2: bridge bindings** — in `ui/src/lib/tauri-bridge.ts`, add `PluginInfo` to the `import type { … } from './types'` list, and add a Plugins section near the MCP bindings:
```ts
// ─── Plugins (Pi-3b) ───
export const listPlugins = (): Promise<PluginInfo[]> => invoke('list_plugins')
export const setPluginEnabled = (id: string, enabled: boolean): Promise<void> =>
  invoke('set_plugin_enabled', { id, enabled })
```

- [ ] **Step 3: failing test** — `ui/src/components/settings/PluginsSettings.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { PluginsSettings } from './PluginsSettings'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

const PLUGIN = { id: 'p1', display_name: 'Hello Plugin', version: '0.1.0', enabled: true, mcp_connected: true }

beforeEach(() => { invokeMock.mockReset() })

describe('PluginsSettings', () => {
  it('lists plugins from list_plugins', async () => {
    invokeMock.mockImplementation((cmd: string) => cmd === 'list_plugins' ? Promise.resolve([PLUGIN]) : Promise.resolve())
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('Hello Plugin')).toBeInTheDocument())
    expect(screen.getByText(/0\.1\.0/)).toBeInTheDocument()
  })

  it('toggles via set_plugin_enabled', async () => {
    invokeMock.mockImplementation((cmd: string) => cmd === 'list_plugins' ? Promise.resolve([PLUGIN]) : Promise.resolve())
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('Hello Plugin')).toBeInTheDocument())
    const sw = screen.getByRole('switch')
    fireEvent.click(sw)
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('set_plugin_enabled', { id: 'p1', enabled: false }))
  })

  it('shows empty state when no plugins', async () => {
    invokeMock.mockImplementation((cmd: string) => cmd === 'list_plugins' ? Promise.resolve([]) : Promise.resolve())
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('未安装插件')).toBeInTheDocument())
  })
})
```
(If `renderWithProviders`/`screen` import path differs, match `ImChannelsSettings.test.tsx` exactly. If the Switch lacks `role="switch"`, Radix Switch has it by default; otherwise query by `aria-label="启用"`.)

- [ ] **Step 4: implement `PluginsSettings.tsx`**
```tsx
import { useState, useEffect, useCallback } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { Switch } from '@/components/ui/switch'
import { listPlugins, setPluginEnabled } from '@/lib/tauri-bridge'
import type { PluginInfo } from '@/lib/types'
import { cn } from '@/lib/utils'
import { toast } from 'sonner'

export function PluginsSettings() {
  const [plugins, setPlugins] = useState<PluginInfo[] | null>(null)
  const [loading, setLoading] = useState(false)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      setPlugins(await listPlugins())
    } catch (e) {
      toast.error('加载插件失败', { description: String(e) })
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void refresh() }, [refresh])

  const onToggle = async (p: PluginInfo, next: boolean) => {
    setPlugins((prev) => prev?.map((x) => (x.id === p.id ? { ...x, enabled: next } : x)) ?? prev)
    try {
      await setPluginEnabled(p.id, next)
    } catch (e) {
      toast.error('切换插件状态失败', { description: String(e) })
      setPlugins((prev) => prev?.map((x) => (x.id === p.id ? { ...x, enabled: !next } : x)) ?? prev)
    }
  }

  return (
    <SettingsSection
      title="插件"
      description="管理已安装的插件（启用 / 停用在下次会话或重连后生效）"
    >
      <SettingsCard>
        {plugins == null ? (
          <div className="px-4 py-3.5 text-sm text-muted-foreground">加载中…</div>
        ) : plugins.length === 0 ? (
          <div className="px-4 py-3.5 text-sm text-muted-foreground">未安装插件</div>
        ) : (
          plugins.map((p) => (
            <SettingsRow key={p.id} label={p.display_name} description={`v${p.version}`}>
              <div className="flex items-center gap-3">
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span
                    className={cn(
                      'size-2 rounded-full flex-shrink-0',
                      p.mcp_connected ? 'bg-emerald-500' : 'bg-muted-foreground/40',
                    )}
                  />
                  {p.mcp_connected ? 'MCP 已连接' : 'MCP 未连接'}
                </span>
                <Switch
                  checked={p.enabled}
                  onCheckedChange={(next) => onToggle(p, next)}
                  disabled={loading}
                  aria-label="启用"
                />
              </div>
            </SettingsRow>
          ))
        )}
      </SettingsCard>
    </SettingsSection>
  )
}
```

- [ ] **Step 5: run + commit**
`cd ui && npx tsc --noEmit 2>&1 | head` → clean. `cd ui && npm test -- --run PluginsSettings 2>&1 | tail -15` → green.
```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts ui/src/components/settings/PluginsSettings.tsx ui/src/components/settings/PluginsSettings.test.tsx
git commit -m "feat(ui): PluginsSettings panel + listPlugins/setPluginEnabled bindings + PluginInfo type"
```
Verify `git show HEAD --stat` lists the new `PluginsSettings.tsx` + `.test.tsx`.

---

## Task 2: Wire the `plugins` tab into Settings

**Files:** Modify `ui/src/atoms/settings-tab.ts`, `ui/src/components/settings/SettingsNav.tsx`, `ui/src/components/settings/SettingsPanel.tsx`

- [ ] **Step 1: union** — `settings-tab.ts`: add `  | 'plugins'        // 已安装插件管理` after `'imChannels'` (or anywhere in the union).

- [ ] **Step 2: nav** — `SettingsNav.tsx`: add `Puzzle` to the lucide-react import; in `GROUPS` 核心 group, insert right after the `{ id: 'tools', label: '工具与能力', icon: <Wrench size={16} /> }` line:
```tsx
      { id: 'plugins', label: '插件', icon: <Puzzle size={16} /> },
```

- [ ] **Step 3: panel** — `SettingsPanel.tsx`: add `import { PluginsSettings } from './PluginsSettings'` to the import block; add `    case 'plugins':\n      return <PluginsSettings />` to the `SettingsContent` switch (after the `'tools'` case); add `  plugins: '插件',` to `TAB_LABEL`.

- [ ] **Step 4: verify + commit**
`cd ui && npx tsc --noEmit 2>&1 | head` → clean (the `TAB_LABEL: Record<SettingsTab,string>` will fail to compile if you missed the union or the label — good guard). `cd ui && npm test -- --run 2>&1 | tail -8` → green.
```bash
git add ui/src/atoms/settings-tab.ts ui/src/components/settings/SettingsNav.tsx ui/src/components/settings/SettingsPanel.tsx
git commit -m "feat(ui): wire 插件 (plugins) tab into Settings nav + panel router"
```

---

## Task 3: Whole-slice verification + ship

- [ ] **Step 1**: `cd ui && npx tsc --noEmit` clean; `cd ui && npm test -- --run` green (no regressions; PluginsSettings tests pass).
- [ ] **Step 2**: grep gates — `'plugins'` in the union; nav item present; switch case + `TAB_LABEL.plugins` present; `Puzzle` imported; bindings use snake-case command names (`list_plugins`/`set_plugin_enabled`); `PluginInfo` fields snake_case.
- [ ] **Step 3**: PR with `## Commits (bisectable)` table. Note: snake_case serde-matched fields; lightweight Option-A list; toggle takes effect next session (backend design); status pill = mcp_connected.
- [ ] **Step 4**: rebase onto latest origin/main, rebase-merge, sync main, cleanup worktree+branch, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: plugin management UI shipped; next 3b = install-from-registry/marketplace, commands-dispatch, sandbox v2).

---

## Self-Review

**Spec coverage:** §1 tab register → T2; §2 bindings+type → T1; §3 component → T1. ✓
**Placeholder scan:** test import-path / Switch-role fallbacks are flagged-with-fallback (match ImChannels test), not TODOs. ✓
**Type consistency:** `PluginInfo` snake_case (matches backend serde, the #1 risk — pinned); `setPluginEnabled → Promise<void>` (matches `Result<(),Error>`); command names `list_plugins`/`set_plugin_enabled` (exact). ✓
**Two-edit completeness:** adding a tab = union + nav + switch + label — all four in T2; `TAB_LABEL: Record<SettingsTab,string>` makes tsc enforce the label (miss it → compile error). ✓
**House style:** reuses SettingsSection/Card/Row + Switch + cn + sonner + ToolSettings data pattern; no new styling. ✓
