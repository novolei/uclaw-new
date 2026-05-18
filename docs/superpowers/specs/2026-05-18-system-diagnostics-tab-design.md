# System Diagnostics Tab — Design Spec

**Date:** 2026-05-18  
**Branch:** TBD (new feature branch)  
**Status:** Approved, pending implementation plan

---

## Overview

Add a new **「系统诊断」(System Diagnostics)** settings tab to the uClaw settings window. The page surfaces app health, bridge service status (memU Python bridge + gbrain Bun MCP), all registered background services, and per-service recovery actions — all on demand via a "运行诊断" button.

---

## Decisions

| Question | Decision |
|----------|----------|
| Nav placement | New independent tab in System group, between 代理 and 关于 |
| Bridge services | Separate card section (not merged into process list) with PID / heartbeat / tool-count per bridge |
| Recovery actions | Global row (reset AI engine + restart app) + bridge row (restart memU + restart gbrain) |
| Refresh strategy | On-demand only — user clicks "运行诊断"; shows last-check timestamp; no auto-polling |

---

## Frontend Changes

### 1. `ui/src/atoms/settings-tab.ts`
Add `'system'` to the `SettingsTab` union type.

### 2. `ui/src/components/settings/SettingsNav.tsx`
Insert a new entry in the System group between `proxy` and `about`:
- Icon: monitor/screen icon (consistent with existing icon style)
- Label: `系统诊断`
- Value: `system`

### 3. `ui/src/components/settings/SettingsPanel.tsx`
Add `system` case that renders `<SystemTab />`.

### 4. `ui/src/components/settings/SystemTab.tsx` (new file)

Seven visual sections rendered top-to-bottom inside a scrollable column:

#### Header Row
- Left: "系统诊断" heading + "检查系统健康状态并修复问题" subtitle
- Right: "运行诊断" button — calls `get_system_diagnostics()`, sets `lastChecked` timestamp, updates state

#### 系统健康 Card (collapsible)
- Collapsed default: green checkmark icon + "系统健康" label + last-check timestamp + chevron
- When any service is Failed: red icon + "发现问题" label
- Expands to show a brief summary of failed service names

#### 系统信息 Grid (2×2)
| 版本 | `app_version` | 平台 | `platform (arch)` |
|------|--------------|------|-------------------|
| 内存 | `used / total GB` | 运行时间 | `Xh Ym` |

#### 健康指标 Grid (2×2)
| 连续失败次数 | `consecutive_failures` | 恢复尝试次数 | `recovery_attempts` |
|-------------|----------------------|-------------|----------------------|
| 活跃进程 | `active_processes` | 发现孤儿进程 | `orphan_processes` |

Derived from `ServicesSummary`:
- `consecutive_failures` = `summary.failed` (count of services in Failed state)
- `recovery_attempts` = `summary.failed` (same value; distinct field for future restart-attempt tracking)
- `active_processes` = `summary.running`
- `orphan_processes` = always 0 in this implementation (field reserved for future process-tree scan)

#### 进程状态 List
Row per process:
- **Claude (AI Sessions)** — green dot + "N 运行中" (session count from AppState, or 0)
- **Cloudflared (Tunnel)** — dot + "运行中" / "未运行" (check if cloudflared process exists via service status)

#### 桥接服务 Cards
Two cards with slightly elevated background:

**memU card**
- Status dot (green=running / grey=stopped)
- "memU (Python Bridge)" label
- PID (if running), alive boolean from `MemUBridgeStatus`

**gbrain card**
- Status dot (green=connected / grey=disconnected)
- "gbrain (Bun MCP)" label
- Tool count: "N 工具"
- pgdata status: "PGlite pgdata 已就绪" / "PGlite pgdata 未就绪"

#### 服务状态 List
Flat list from `SystemDiagnosticsReport.services` (Vec<ServiceHealth>):
- Green dot = Running, grey = Stopped/Stopping, red = Failed
- Show `name` + status label

#### 恢复操作
Row 1 (global):
- `↺ 重置 AI 引擎` — calls existing `reset_ai_engine` command (warm-tinted button)
- `↻ 重启应用` — calls `restart_app` / `process::exit(0)` (red-tinted button)

Row 2 (bridges — only shown when bridge section data loaded):
- `↺ 重启 memU` — calls `restart_memu_bridge()`, shows spinner while in-flight
- `↺ 重启 gbrain` — calls `restart_gbrain_mcp()`, shows spinner while in-flight

Each button has its own `isBusy` boolean; they are independent.

#### Footer Links
- `复制报告` — serializes `SystemDiagnosticsReport` to JSON, copies to clipboard
- `导出报告` — triggers file-save dialog with the same JSON blob

---

## Backend Changes

### `src-tauri/Cargo.toml`
Add:
```toml
sysinfo = { version = "0.31", default-features = false, features = ["system"] }
```

### `src-tauri/src/app.rs`

Add two fields to `AppState`:
```rust
/// App launch time — used to compute uptime_secs in diagnostics.
pub boot_time: std::time::Instant,

/// gbrain MCP server name as registered in McpManager after Sprint 2.1 seed.
/// None if gbrain seed was skipped (binary not found or feature disabled).
pub gbrain_mcp_id: Arc<std::sync::Mutex<Option<String>>>,
```

Initialize `boot_time: std::time::Instant::now()` at top of `AppState::new()`.  
Initialize `gbrain_mcp_id: Arc::new(std::sync::Mutex::new(None))`.

### `src-tauri/src/main.rs`

After `seed_bundled_gbrain` succeeds in Stage 3, store the server name:
```rust
*state.gbrain_mcp_id.lock().unwrap() = Some(gbrain_server_name.clone());
```

### `src-tauri/src/memu/bridge.rs`

Add a public `force_restart` method:
```rust
pub async fn force_restart(&self) -> Result<(), MemUBridgeError> {
    self.stop().await;
    self.start().await
}
```

Expose on `MemUClient` as a passthrough to the inner `MemUBridge`.

### `src-tauri/src/tauri_commands.rs`

#### New structs (Serialize)

```rust
pub struct SystemDiagnosticsReport {
    pub app_version: String,
    pub platform: String,
    pub arch: String,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub uptime_secs: u64,
    pub consecutive_failures: u32,
    pub recovery_attempts: u32,
    pub active_processes: u32,
    pub orphan_processes: u32,
    pub services: Vec<ServiceHealth>,
    pub memu: MemUBridgeStatus,
    pub gbrain: GbrainStatus,
}

pub struct MemUBridgeStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

pub struct GbrainStatus {
    pub connected: bool,
    pub tool_count: u32,
    pub pgdata_ready: bool,
}
```

#### New commands

**`get_system_diagnostics`**  
Aggregates:
- Version/platform from existing helpers
- Memory via `sysinfo::System::new_with_specifics`
- `uptime_secs` from `state.boot_time.elapsed().as_secs()`
- Services from `state.service_manager.health_summary().await`
- memU status from `state.memu_client.as_ref().map(|c| c.is_alive())`
- gbrain status from `state.mcp_manager.read().await.get_server(id)` where id = `gbrain_mcp_id`

**`restart_memu_bridge`**  
Calls `state.memu_client.as_ref()?.force_restart().await`. Returns `Ok(())` or error string.

**`restart_gbrain_mcp`**  
Reads `gbrain_mcp_id`, calls `mcp_manager.disconnect(id).await` + `mcp_manager.connect(id).await`. Returns `Ok(())` or error string.

#### `invoke_handler!` macro
Add all three new commands.

---

## File Checklist

| File | Change |
|------|--------|
| `ui/src/atoms/settings-tab.ts` | Add `'system'` to union |
| `ui/src/components/settings/SettingsNav.tsx` | Add nav entry in System group |
| `ui/src/components/settings/SettingsPanel.tsx` | Add `system` tab case |
| `ui/src/components/settings/SystemTab.tsx` | **New** — full diagnostics UI |
| `src-tauri/Cargo.toml` | Add `sysinfo` dependency |
| `src-tauri/src/app.rs` | Add `boot_time` + `gbrain_mcp_id` |
| `src-tauri/src/main.rs` | Init fields; write `gbrain_mcp_id` after seed |
| `src-tauri/src/memu/bridge.rs` | Add `force_restart()` |
| `src-tauri/src/memu/client.rs` | Expose `force_restart()` passthrough |
| `src-tauri/src/tauri_commands.rs` | 3 new commands + 3 new structs + `invoke_handler!` |

---

## Out of Scope

- Auto-polling / live status (future enhancement)
- Cloudflared process detection beyond service status lookup
- Orphan process tree scan (field present but always 0)
- Per-service log viewer
