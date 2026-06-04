# Plugin Management UI Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b plugin system. Frontend slice making the (backend-complete) plugin system user-visible + controllable. After lifecycle (#667), skills (#668), sandbox (#669) — all backend. This is the user-facing capstone of 3b's first wave.

## Problem

The plugin system is functionally complete on the backend (discovery, lifecycle enable/disable + persistence, tools/skills contribution, OS sandbox) and exposes Tauri commands `list_plugins() -> Vec<PluginInfo>` + `set_plugin_enabled(id, enabled)` — but there is NO UI. A user cannot see what plugins are installed or turn one off without editing files. This slice adds a Settings panel that lists installed plugins with an enable/disable toggle + MCP connection status.

## Decision (approved 2026-06-04)

- **Lightweight list (Option A)** — mirror `ToolSettings.tsx`: `SettingsSection` + `SettingsCard`, one row per plugin (name + version + MCP-status badge + enable `Switch`). Optimistic toggle, toast-on-error revert, empty + loading states. (Card-grid + detail drawer = deferred v2; install-from-registry = separate slice.)
- **Placement: 核心 group, adjacent to 工具与能力 (`tools`)** — plugins are capability extensions (tools/skills/MCP), so they belong next to tools. Icon: `Puzzle` (lucide-react).

## Design

### §1 Register the `plugins` settings tab
- `ui/src/atoms/settings-tab.ts` — add `'plugins'` to the `SettingsTab` union.
- `ui/src/components/settings/SettingsNav.tsx` — add `{ id: 'plugins', label: '插件', icon: <Puzzle size={16} /> }` to the **核心** group, right after the `tools` item.
- `ui/src/components/settings/SettingsPanel.tsx` — add `case 'plugins': return <PluginsSettings />` to the switch router + `plugins: '插件'` to `TAB_LABEL` (and any title map).

### §2 Frontend command bindings + type
- `ui/src/lib/tauri-bridge.ts`:
  ```ts
  export const listPlugins = (): Promise<PluginInfo[]> => invoke('list_plugins')
  export const setPluginEnabled = (id: string, enabled: boolean): Promise<void> =>
    invoke('set_plugin_enabled', { id, enabled })
  ```
- `PluginInfo` TS interface — **field names MUST match the backend serde output** (plan pins whether the Rust `PluginInfo` uses `#[serde(rename_all="camelCase")]` → `displayName`/`mcpConnected`, or stays snake → `display_name`/`mcp_connected`). Home: `ui/src/lib/types.ts` (or alongside the bridge fn, matching how `McpServerInfo` is declared).

### §3 `PluginsSettings.tsx` (new component)
Mirror `ToolSettings.tsx` data pattern:
- State: `plugins: PluginInfo[] | null`, `loading: boolean`.
- `refresh = useCallback(async () => { setLoading(true); try { setPlugins(await listPlugins()) } catch(e){ toast.error('加载插件失败', {description:String(e)}) } finally { setLoading(false) } }, [])`; `useEffect(() => { void refresh() }, [refresh])`.
- Render `SettingsSection title="插件" description="管理已安装的插件（启用/停用在下次会话生效）"` → `SettingsCard` → for each plugin a row:
  - left: `display_name` (LABEL_CLASS) + `v{version}` (DESCRIPTION_CLASS, subtle).
  - right: an MCP-status pill (a small colored dot + label) — `mcp_connected ? 已连接(emerald) : 未连接(muted)`; only meaningful for plugins that contribute an MCP server, so show a neutral dash for non-MCP plugins is acceptable (plan decides the exact rule from PluginInfo — v1 can simply show 已连接/未连接 from `mcp_connected`).
  - far right: `Switch checked={enabled} onCheckedChange={...}`.
- Toggle handler (mirror IntegrationsModule `onToggleEnabled`): optimistic local update → `setPluginEnabled(id, next)` → on error `toast.error` + revert.
- Empty state: when `plugins` is `[]`, render a muted "未安装插件" row/note.
- Loading: a spinner/skeleton on first load (match ToolSettings' loading treatment).
- Use the existing settings primitives (`SettingsSection`, `SettingsCard`, `SettingsRow`/`SettingsToggle`, `Switch`, `Badge`) — match house style (`SettingsUIConstants` classes); no new styling system.

## Data flow

```
mount → listPlugins() → render rows (name/version/status/toggle)
toggle → optimistic setEnabled → setPluginEnabled(id,next) → err? toast+revert
(backend: set_plugin_enabled persists V59 + updates live map + mcp set_enabled;
 takes effect next agent session / MCP reconnect — UI hint notes this)
```

## Out of scope

Card-grid + per-plugin detail drawer (v2); install-from-registry / marketplace (separate slice); per-plugin permission editing; showing contributed tools/skills counts or sandbox status per plugin (v2 detail); uninstall/delete; live mid-session re-render of the agent's tool set (backend applies next session by design). No backend changes — `list_plugins`/`set_plugin_enabled` already exist.

## Error handling

`listPlugins` failure → toast + keep prior state (or empty). `setPluginEnabled` failure → toast + revert the optimistic toggle (the canonical pattern). Loading guard so the toggle isn't shown mid-fetch. No backend/migration risk (read + one existing mutation command).

## Testing

Frontend (uClaw uses Vitest + jsdom — `cd ui && npm test`):
1. `PluginsSettings` renders a fetched list (mock `listPlugins` → rows with name/version/status/toggle).
2. Toggling a row calls `setPluginEnabled(id, next)` (mock) + optimistically flips the Switch.
3. On `setPluginEnabled` reject → the Switch reverts + an error toast fires (mock toast).
4. Empty list → "未安装插件" state.
5. `npx tsc --noEmit` clean (the new `PluginInfo` type + component typecheck).
(If the existing settings components have no test precedent, at minimum ship the tsc-clean component + a render+toggle test mirroring any existing settings test; plan checks for a test precedent.)

## Scope / files

| File | Change |
|---|---|
| `ui/src/atoms/settings-tab.ts` | add `'plugins'` to `SettingsTab` |
| `ui/src/components/settings/SettingsNav.tsx` | nav item in 核心 after `tools` (Puzzle icon) |
| `ui/src/components/settings/SettingsPanel.tsx` | switch case + `TAB_LABEL` entry |
| `ui/src/components/settings/PluginsSettings.tsx` (new) | the panel (list + toggle + status) |
| `ui/src/lib/tauri-bridge.ts` | `listPlugins` + `setPluginEnabled` bindings |
| `ui/src/lib/types.ts` (or bridge) | `PluginInfo` interface (serde-matched fields) |

## Risk

Low. Pure frontend, reuses existing settings primitives + the ToolSettings/Integrations patterns, calls two already-shipped commands, no backend/migration. Main risks: (1) **serde field-name match** — the TS `PluginInfo` field names must match the Rust struct's serialized output (camelCase vs snake_case); plan pins it from the backend struct (a mismatch = undefined fields rendering blank). (2) **`set_plugin_enabled` return type** — backend returns `Result<(), Error>`; the TS binding is `Promise<void>` (confirm it's not `Promise<bool>` like `toggle_mcp_server`). (3) match the exact `SettingsNav` GROUPS shape + `SettingsPanel` switch/label maps (plan quotes them). (4) the two-edit nature of adding a tab (union + nav + switch + label — miss one and the tab is dead/unlabeled). Bisectable: bindings+type → component → wire into settings (tab/nav/switch) → verify. After this slice, a user can see installed plugins and enable/disable them from Settings — the plugin system becomes visible + controllable, completing 3b's first user-facing surface.
