# Plugin Management UI v2 Design (card grid + detail drawer)

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b. Upgrades the Settings 插件 tab from a flat list into a **2-column card grid + a per-plugin detail drawer** showing the plugin's contributed tools/skills/commands, permissions/sandbox, and preflight findings. After install (#675). In-place in Settings (approved: Option A — keeps plugin management unified, reuses the Sheet drawer pattern).

## Problem

`PluginsSettings.tsx` lists plugins as flat rows (name + version + MCP status + enable toggle) and an install section. `list_plugins` → `PluginInfo { id, display_name, version, enabled, mcp_connected }` — no per-plugin detail (what it contributes, what it's permitted/sandboxed to do, any preflight warnings). Users can't see what a plugin actually does.

## Decision (approved 2026-06-04)

- **Option A — in-place in Settings**: keep the install section; replace the flat list with a 2-col **PluginCard** grid; clicking a card opens a **PluginDetailDrawer** (`Sheet`, 340px, z-50 overlay — fits the 800px Settings panel). Mirrors the proven `McpServerCard` + `McpDetailDrawer` pattern. Unified plugin management (install + grid + detail) in one tab; less code than a new Kaleidoscope module; no fragmentation.
- **Detail data on demand**: a new `get_plugin_detail(id)` Tauri command re-runs `PluginDiscovery` (same as `list_plugins`), reads `manifest.contributes` + `manifest.permissions`, computes preflight fresh (`PluginPreflightReport::for_loaded_plugin`), derives sandbox flags, + live MCP status + enabled state. (The boot `PluginLifecycleReport` isn't stored; re-scan is consistent with `list_plugins`.) **Declared contributions** (from `manifest.contributes`) are shown; preflight surfaces registration problems.

## Design

### §1 Backend `get_plugin_detail(id) -> PluginDetail` (tauri_commands.rs + main.rs)
```rust
#[derive(serde::Serialize)]
pub struct PluginDetail {
    // base (mirrors PluginInfo)
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
    pub mcp_connected: bool,
    // manifest
    pub description: Option<String>,
    pub author_name: String,
    pub contributes: crate::plugin_manifest::schema::PluginContribution, // tools/skills/commands/mcp_servers/themes (Serialize)
    pub permissions: crate::plugin_manifest::schema::PluginPermissions,  // network/fs_read/fs_write/run_subprocess/... (Serialize)
    pub preflight: Option<crate::plugins::runtime::PluginPreflightReport>, // verdict + findings (Serialize)
}

#[tauri::command]
pub async fn get_plugin_detail(state: State<'_, AppState>, id: String) -> Result<PluginDetail, Error> {
    // re-run discovery, find the LoadedPlugin whose manifest.id == id (NotFound else)
    // enabled from state.plugin_enabled; mcp_connected from mcp_manager.status(&id)
    // preflight = PluginPreflightReport::for_loaded_plugin(&loaded)
    // build PluginDetail from loaded.manifest (+ author_name = manifest.author.name)
}
```
Embeds the existing `Serialize` types (`PluginContribution`, `PluginPermissions`, `PluginPreflightReport`) directly — no DTO duplication; the TS side declares matching interfaces. Register in `main.rs` `generate_handler!`. Plan pins: `PluginAuthor.name` field; `PluginPreflightSeverity`/`Category` are `Serialize` (confirm); the discovery-find-by-id pattern (mirror `list_plugins`'s loop, match on `loaded.manifest.id == id`).

### §2 Frontend types + binding (types.ts + tauri-bridge.ts)
```ts
export interface PluginContribution { tools: string[]; skills: string[]; commands: string[]; mcp_servers: string[]; themes: string[] }
export interface PluginPermissions { network: boolean; filesystem_read: boolean; filesystem_write: boolean; memory_read: boolean; memory_write: boolean; run_subprocess: boolean; additional: string[] }
export interface PluginPreflightFinding { severity: 'info'|'warning'|'error'; category: string; message: string }
export interface PluginPreflightReport { verdict: 'pass'|'warn'|'fail'; findings: PluginPreflightFinding[]; summary: { errors: number; warnings: number; info: number } }
export interface PluginDetail {
  id: string; display_name: string; version: string; enabled: boolean; mcp_connected: boolean
  description?: string; author_name: string
  contributes: PluginContribution; permissions: PluginPermissions; preflight?: PluginPreflightReport
}
```
(Field casing matches the Rust serde output: `PluginContribution`/`PluginPermissions` are snake-free single words except `mcp_servers`/`filesystem_read` etc. — snake_case; preflight `#[serde(rename_all="snake_case")]` on severity/category/verdict. Plan pins exact casing.)
`tauri-bridge.ts`: `export const getPluginDetail = (id: string): Promise<PluginDetail> => invoke('get_plugin_detail', { id })`.

### §3 `PluginCard.tsx` (new — mirror McpServerCard)
A clickable button card: icon (first letter of display_name), `display_name`, `v{version}`, an MCP status dot (emerald connected / muted not), and the enable `Switch` (inline quick-toggle, stop-propagation so it doesn't open the drawer). `selected` highlight when its drawer is open. `onClick` opens the detail drawer. (The card shows base info; the drawer shows contributions — the card doesn't need contribution counts, avoiding a `list_plugins` change.)

### §4 `PluginDetailDrawer.tsx` (new — mirror McpDetailDrawer)
`Sheet` (340px, right). On open, fetch `getPluginDetail(id)` (loading state). Sections:
- Header: `display_name` + `v{version}`; `description` (muted).
- Row: 启用 `Switch` (calls the same `onToggle`) + MCP 状态 dot/label.
- **贡献**: tools / skills / commands / mcp_servers as labeled chip lists (empty → "无"). Each section a heading + chips (reuse the McpDetailDrawer chip style).
- **权限 / 沙箱**: badges for `network` / `filesystem_read` / `filesystem_write` / `run_subprocess` (granted = colored, else muted) — these drive the sandbox (#669) policy.
- **预检 (preflight)**: if `verdict != pass`, list findings colored by severity (error=red, warning=amber, info=muted) — surfaces problems (e.g. missing run_subprocess).
- Dismiss via Sheet's built-in close.

### §5 `PluginsSettings.tsx` restructure
Keep the 安装插件 section. Replace the `SettingsRow` list with a 2-col card grid (`grid grid-cols-2 gap-3`) of `PluginCard`. Add `selectedId: string|null` + `drawerOpen: boolean` state; clicking a card sets them + renders `PluginDetailDrawer` (sibling, z-50 overlay). Keep `refresh` + `onToggle` (shared by card + drawer). Loading/empty states preserved.

## Data flow

```
open 插件 tab → list_plugins → render PluginCard grid (name/version/MCP/toggle)
click card → drawerOpen + selectedId → PluginDetailDrawer → get_plugin_detail(id)
  → re-discover + manifest.contributes/permissions + fresh preflight + mcp status
  → drawer shows contributions + permissions/sandbox + preflight findings
toggle (card or drawer) → setPluginEnabled (existing) → optimistic + refresh
```

## Out of scope

Enriching `list_plugins` with contribution counts (card stays base-info; drawer fetches detail); editing permissions / plugin config in the UI (read-only; findings informational); uninstall/upgrade buttons (could be a quick follow); the runtime-registered set (`AgentApi.plugin_index`) — declared `manifest.contributes` is shown (preflight flags mismatches); marketplace browse / remote registry (separate); a full Kaleidoscope module (Option B rejected).

## Error handling

`get_plugin_detail` for an unknown id → `Error::NotFound` → drawer shows an error state (toast). Discovery/parse failure for that plugin → the plugin won't be in the list (consistent with `list_plugins`). Preflight is pure (no I/O) — always computes. Drawer fetch failure → toast + closeable. The card grid degrades to the existing list behavior if detail fails (cards still render from `list_plugins`).

## Testing

1. **backend get_plugin_detail**: install/seed a temp plugin (plugin.toml with contributes + permissions) under a temp data_dir, call the underlying builder → assert contributes/permissions/preflight/author surfaced; unknown id → NotFound. (If the command is hard to unit-test with `State`, extract a pure `build_plugin_detail(loaded, enabled, mcp_connected) -> PluginDetail` helper + unit-test that.)
2. **frontend**: `PluginCard` renders name/version/status/toggle (mock); clicking calls onSelect. `PluginDetailDrawer` renders a mocked `getPluginDetail` result (tools/skills/commands chips, permission badges, preflight findings). PluginsSettings renders the card grid + opens the drawer on card click (mock getPluginDetail). `tsc --noEmit` clean.
`cargo build`/clippy + `cargo test --lib plugins` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `tauri_commands.rs` + `main.rs` | `get_plugin_detail` + `PluginDetail` + invoke_handler |
| `ui/lib/types.ts` + `lib/tauri-bridge.ts` | `PluginDetail`/contributes/permissions/preflight types + `getPluginDetail` |
| `ui/.../settings/PluginCard.tsx` (new) | card (mirror McpServerCard) |
| `ui/.../settings/PluginDetailDrawer.tsx` (new) | Sheet detail drawer (mirror McpDetailDrawer) |
| `ui/.../settings/PluginsSettings.tsx` | list → card grid + drawer wiring |

## Risk

Med (frontend-heavy; reuses proven patterns). Main risks: (1) **serde field casing** — `PluginContribution`/`PluginPermissions`/`PluginPreflightReport` wire shapes must match the TS interfaces (snake_case fields like `mcp_servers`, `filesystem_read`; `#[serde(rename_all="snake_case")]` enums); plan pins each from the structs. (2) **Sheet overlay in the 800px Settings panel** — the drawer is a z-50 portal overlay (not side-by-side), so the width is fine; verify it renders above the panel. (3) **card toggle vs card click** — the inline Switch must stopPropagation so toggling doesn't open the drawer. (4) **detail-on-open fetch** — drawer fetches `get_plugin_detail` when opened (loading state); a slow/failed fetch shows loading/error, doesn't break the grid. (5) embedding internal Serialize types on the wire couples them to the TS types — acceptable (they're stable manifest types); a future rename would need both sides. (6) **two-edit Tauri** — register `get_plugin_detail` in main.rs. Bisectable: backend endpoint → frontend card+drawer+restructure → verify. After this slice, the Settings 插件 tab shows a card grid; clicking a plugin reveals its full contribution surface (tools/skills/commands), permissions/sandbox, and preflight health — the plugin system is fully inspectable.
