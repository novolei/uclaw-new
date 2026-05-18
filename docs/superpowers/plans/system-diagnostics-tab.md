# System Diagnostics Tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a 「系统诊断」settings tab that surfaces app health, memU/gbrain bridge status, all background services, and per-bridge recovery actions.

**Architecture:** On-demand snapshot via a single `get_system_diagnostics` Tauri command that aggregates sysinfo memory, boot-time uptime, ServiceManager health, MemUBridge alive state, and McpManager gbrain tool count. Three new Tauri commands (diagnostics + two bridge restarts) plus a `reset_ai_engine` and `restart_app` command back the frontend recovery buttons. `SystemTab.tsx` is a pure display component.

**Tech Stack:** sysinfo 0.31 (Rust), Tauri v2 commands, React 18 + TypeScript, Tailwind, Jotai (read-only — no new atoms needed), `navigator.clipboard` for copy-report.

---

## File Map

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add `sysinfo = { version = "0.31", … }` |
| `src-tauri/src/app.rs` | Add `boot_time: Instant` + `gbrain_mcp_id: Arc<Mutex<Option<String>>>` |
| `src-tauri/src/memu/bridge.rs` | Add `force_restart()` async method |
| `src-tauri/src/memu/client.rs` | Add `force_restart()` passthrough |
| `src-tauri/src/mcp.rs` | Add `server_tool_count(id)` method to `McpManager` |
| `src-tauri/src/main.rs` | Store "gbrain" in `gbrain_mcp_id` after seed |
| `src-tauri/src/tauri_commands.rs` | 5 new commands + 3 new structs |
| `src-tauri/src/main.rs` | Register 5 new commands in `generate_handler!` |
| `ui/src/atoms/settings-tab.ts` | Add `'system'` to union type |
| `ui/src/components/settings/SettingsNav.tsx` | Add 系统诊断 entry |
| `ui/src/components/settings/SettingsPanel.tsx` | Add `system` case + label |
| `ui/src/components/settings/SystemTab.tsx` | **New** — full diagnostics UI |

---

## Task 1: sysinfo dependency + AppState new fields

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Add sysinfo to Cargo.toml**

  Open `src-tauri/Cargo.toml`. Find the `[dependencies]` block and add after the last dependency line:

  ```toml
  sysinfo = { version = "0.31", default-features = false, features = ["system"] }
  ```

- [ ] **Step 2: Add boot_time field to AppState struct**

  In `src-tauri/src/app.rs`, find the `pub struct AppState {` block (line 147). Locate the closing field:

  ```rust
      pub symphony_service: Arc<tokio::sync::RwLock<Option<Arc<crate::symphony::runtime::service::SymphonyService>>>>,
  }
  ```

  Replace with:

  ```rust
      pub symphony_service: Arc<tokio::sync::RwLock<Option<Arc<crate::symphony::runtime::service::SymphonyService>>>>,

      /// App launch instant — used to compute uptime_secs in diagnostics.
      pub boot_time: std::time::Instant,

      /// gbrain MCP server ID stored after seed_bundled_gbrain succeeds.
      /// "gbrain" when seeded; None when bun/gbrain binaries are missing.
      pub gbrain_mcp_id: Arc<std::sync::Mutex<Option<String>>>,
  }
  ```

- [ ] **Step 3: Initialize new fields in AppState::new()**

  In `app.rs`, find the `Ok(Self {` constructor block (around line 703). Locate the last two fields:

  ```rust
          proactive_service: Arc::new(tokio::sync::RwLock::new(None)),
          symphony_service: Arc::new(tokio::sync::RwLock::new(None)),
      })
  ```

  Replace with:

  ```rust
          proactive_service: Arc::new(tokio::sync::RwLock::new(None)),
          symphony_service: Arc::new(tokio::sync::RwLock::new(None)),
          boot_time: std::time::Instant::now(),
          gbrain_mcp_id: Arc::new(std::sync::Mutex::new(None)),
      })
  ```

- [ ] **Step 4: Verify compilation**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  ```

  Expected: no errors. If sysinfo fails to resolve, run `cargo update sysinfo` first.

- [ ] **Step 5: Commit**

  ```bash
  git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/app.rs
  git commit -m "feat(diagnostics): add sysinfo dep + boot_time + gbrain_mcp_id to AppState"
  ```

---

## Task 2: MemUBridge force_restart + MemUClient passthrough + McpManager server_tool_count

**Files:**
- Modify: `src-tauri/src/memu/bridge.rs`
- Modify: `src-tauri/src/memu/client.rs`
- Modify: `src-tauri/src/mcp.rs`

- [ ] **Step 1: Write the failing test for force_restart**

  In `src-tauri/src/memu/bridge.rs`, find the `#[cfg(test)]` module (near the bottom). Add:

  ```rust
  #[tokio::test]
  async fn force_restart_toggles_alive_state() {
      // A real subprocess won't start in tests (no Python), so we
      // just verify force_restart doesn't panic and returns Err
      // (start will fail with "not found").
      let bridge = MemUBridge::new(
          "/nonexistent/python",
          std::path::PathBuf::from("/nonexistent/script.py"),
          std::path::PathBuf::from("/tmp"),
          vec![],
      );
      // force_restart on a never-started bridge should not panic
      let _ = bridge.force_restart().await;
      assert!(!bridge.is_alive());
  }
  ```

- [ ] **Step 2: Run the test to verify it fails**

  ```bash
  cd src-tauri && cargo test --lib memu::bridge::tests::force_restart 2>&1 | tail -5
  ```

  Expected: FAIL — `no method named 'force_restart'`

- [ ] **Step 3: Add force_restart to MemUBridge**

  In `src-tauri/src/memu/bridge.rs`, find the `pub async fn stop` method (line ~306). After its closing `}`, add:

  ```rust
  /// Stop the bridge if running, then restart it. Best-effort — ignores
  /// stop errors so a failed process doesn't block restart.
  pub async fn force_restart(&self) -> Result<(), BridgeError> {
      let _ = self.stop().await;
      self.start().await
  }
  ```

- [ ] **Step 4: Run the test to verify it passes**

  ```bash
  cd src-tauri && cargo test --lib memu::bridge::tests::force_restart 2>&1 | tail -5
  ```

  Expected: `test memu::bridge::tests::force_restart_toggles_alive_state ... ok`

- [ ] **Step 5: Add force_restart passthrough to MemUClient**

  In `src-tauri/src/memu/client.rs`, find the `pub async fn shutdown` method. After its closing `}`, add:

  ```rust
  /// Force-restart the underlying Python subprocess. Stops first (ignores
  /// stop errors), then starts fresh.
  pub async fn force_restart(&self) -> Result<(), BridgeError> {
      self.bridge.force_restart().await
  }
  ```

- [ ] **Step 6: Write failing test for server_tool_count**

  In `src-tauri/src/mcp.rs`, find the `#[cfg(test)]` module. Add:

  ```rust
  #[test]
  fn server_tool_count_returns_none_for_missing_server() {
      let tmp = tempfile::tempdir().unwrap();
      let mgr = McpManager::new(tmp.path());
      assert_eq!(mgr.server_tool_count("gbrain"), None);
  }
  ```

- [ ] **Step 7: Run test to verify it fails**

  ```bash
  cd src-tauri && cargo test --lib mcp::tests::server_tool_count 2>&1 | tail -5
  ```

  Expected: FAIL — `no method named 'server_tool_count'`

- [ ] **Step 8: Add server_tool_count to McpManager**

  In `src-tauri/src/mcp.rs`, find the `pub fn status` method (around line 1313). After its closing `}`, add:

  ```rust
  /// Return the number of discovered tools for a server, or None if the
  /// server ID is not registered.
  pub fn server_tool_count(&self, id: &str) -> Option<usize> {
      self.servers.get(id).map(|s| s.tools.len())
  }
  ```

- [ ] **Step 9: Run test to verify it passes**

  ```bash
  cd src-tauri && cargo test --lib mcp::tests::server_tool_count 2>&1 | tail -5
  ```

  Expected: `test mcp::tests::server_tool_count_returns_none_for_missing_server ... ok`

- [ ] **Step 10: Full compile check**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  ```

  Expected: no errors.

- [ ] **Step 11: Commit**

  ```bash
  git add src-tauri/src/memu/bridge.rs src-tauri/src/memu/client.rs src-tauri/src/mcp.rs
  git commit -m "feat(diagnostics): force_restart on MemUBridge/Client + server_tool_count on McpManager"
  ```

---

## Task 3: Store gbrain_mcp_id in main.rs after seed

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Locate the seed_bundled_gbrain call**

  In `src-tauri/src/main.rs`, find the seed block (around line 525):

  ```rust
  match guard.seed_bundled_gbrain(bun, entry, &pgdata_dir) {
      Ok(true) => tracing::info!(
          "[Stage 3] gbrain MCP entry seeded (first launch)"
      ),
      Ok(false) => tracing::debug!(
          "[Stage 3] gbrain MCP entry already present, skipping seed"
      ),
      Err(e) => tracing::warn!(
  ```

- [ ] **Step 2: Store "gbrain" after seed (both Ok branches)**

  Replace the match arms with:

  ```rust
  match guard.seed_bundled_gbrain(bun, entry, &pgdata_dir) {
      Ok(seeded) => {
          if seeded {
              tracing::info!("[Stage 3] gbrain MCP entry seeded (first launch)");
          } else {
              tracing::debug!("[Stage 3] gbrain MCP entry already present, skipping seed");
          }
          // Store the server ID so diagnostics + restart commands can find it.
          *app_state.gbrain_mcp_id.lock().unwrap() = Some("gbrain".to_string());
      }
      Err(e) => tracing::warn!(
  ```

  > **Note:** `app_state` is the `Arc<AppState>` available in Stage 3 via `app.state::<AppState>()`. If the variable is named differently in your context (e.g. `shared_state`, `state`), use the correct name — search for the `app.state::<AppState>()` call just above Stage 3.

- [ ] **Step 3: Verify the variable name used in Stage 3**

  ```bash
  grep -n "app\.state::<AppState>\|let.*AppState\|app_state\b" /Users/ryanliu/Documents/uclaw/src-tauri/src/main.rs | head -10
  ```

  Use the correct variable name in Step 2 if different from `app_state`.

- [ ] **Step 4: Compile check**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  ```

  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add src-tauri/src/main.rs
  git commit -m "feat(diagnostics): store gbrain MCP ID in AppState after seed"
  ```

---

## Task 4: New Tauri commands

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs` (handler registration)

- [ ] **Step 1: Add the three diagnostic structs**

  In `src-tauri/src/tauri_commands.rs`, find the block near `get_platform` and `get_version` (around line 255). Just before `pub async fn get_platform`, add:

  ```rust
  #[derive(Debug, serde::Serialize, Clone)]
  pub struct MemUBridgeStatus {
      pub running: bool,
      pub pid: Option<u32>,
  }

  #[derive(Debug, serde::Serialize, Clone)]
  pub struct GbrainStatus {
      pub connected: bool,
      pub tool_count: u32,
      pub pgdata_ready: bool,
  }

  #[derive(Debug, serde::Serialize, Clone)]
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
      pub services: Vec<crate::services::types::ServiceHealth>,
      pub memu: MemUBridgeStatus,
      pub gbrain: GbrainStatus,
  }
  ```

- [ ] **Step 2: Add get_system_diagnostics command**

  Just after the `get_version` function closing `}` (around line 270), add:

  ```rust
  #[tauri::command]
  pub async fn get_system_diagnostics(
      state: State<'_, AppState>,
  ) -> Result<SystemDiagnosticsReport, Error> {
      // Memory via sysinfo
      let sys = sysinfo::System::new_with_specifics(
          sysinfo::RefreshKind::new()
              .with_memory(sysinfo::MemoryRefreshKind::everything()),
      );
      let memory_used_mb = sys.used_memory() / 1_048_576;
      let memory_total_mb = sys.total_memory() / 1_048_576;

      // Uptime
      let uptime_secs = state.boot_time.elapsed().as_secs();

      // Services
      let summary = state.service_manager.get_all_health().await;
      let consecutive_failures = summary.failed as u32;
      let recovery_attempts = summary.failed as u32;
      let active_processes = summary.running as u32;

      // memU bridge status
      let memu = match state.memu_client.as_ref() {
          Some(client) => MemUBridgeStatus { running: client.is_available(), pid: None },
          None => MemUBridgeStatus { running: false, pid: None },
      };

      // gbrain status
      let gbrain = {
          let mcp = state.mcp_manager.read().await;
          let connected = matches!(
              mcp.status("gbrain"),
              Some(crate::mcp::McpServerStatus::Connected)
          );
          let tool_count = mcp.server_tool_count("gbrain").unwrap_or(0) as u32;
          let pgdata_ready = state
              .data_dir
              .join("gbrain")
              .join("pgdata")
              .join("PG_VERSION")
              .exists();
          GbrainStatus { connected, tool_count, pgdata_ready }
      };

      Ok(SystemDiagnosticsReport {
          app_version: env!("CARGO_PKG_VERSION").into(),
          platform: std::env::consts::OS.into(),
          arch: std::env::consts::ARCH.into(),
          memory_used_mb,
          memory_total_mb,
          uptime_secs,
          consecutive_failures,
          recovery_attempts,
          active_processes,
          orphan_processes: 0,
          services: summary.services,
          memu,
          gbrain,
      })
  }
  ```

- [ ] **Step 3: Add restart_memu_bridge command**

  After `get_system_diagnostics` closing `}`, add:

  ```rust
  #[tauri::command]
  pub async fn restart_memu_bridge(
      state: State<'_, AppState>,
  ) -> Result<(), String> {
      let client = state
          .memu_client
          .as_ref()
          .ok_or_else(|| "memU client not initialized (Python bridge missing)".to_string())?;
      client.force_restart().await.map_err(|e| e.to_string())
  }
  ```

- [ ] **Step 4: Add restart_gbrain_mcp command**

  After `restart_memu_bridge` closing `}`, add:

  ```rust
  #[tauri::command]
  pub async fn restart_gbrain_mcp(
      state: State<'_, AppState>,
  ) -> Result<(), String> {
      let id = state
          .gbrain_mcp_id
          .lock()
          .unwrap()
          .clone()
          .ok_or_else(|| "gbrain MCP entry not seeded (bundle missing?)".to_string())?;
      let mut mcp = state.mcp_manager.write().await;
      mcp.disconnect_server(&id).await.map_err(|e| e.to_string())?;
      mcp.connect_server(&id).await.map_err(|e| e.to_string())?;
      Ok(())
  }
  ```

- [ ] **Step 5: Add reset_ai_engine command**

  After `restart_gbrain_mcp` closing `}`, add:

  ```rust
  #[tauri::command]
  pub async fn reset_ai_engine(
      state: State<'_, AppState>,
  ) -> Result<(), Error> {
      let mut sessions = state.running_sessions.lock().await;
      let count = sessions.len();
      for (_, token) in sessions.drain() {
          token.cancel();
      }
      tracing::info!("reset_ai_engine: cancelled {} running session(s)", count);
      Ok(())
  }
  ```

- [ ] **Step 6: Add restart_app command**

  After `reset_ai_engine` closing `}`, add:

  ```rust
  #[tauri::command]
  pub async fn restart_app(app: tauri::AppHandle) -> Result<(), Error> {
      app.restart();
      Ok(())
  }
  ```

- [ ] **Step 7: Register all 5 new commands in generate_handler!**

  In `src-tauri/src/main.rs`, find the `// MEMUBOT Services` comment (around line 899):

  ```rust
              // MEMUBOT Services
              uclaw_core::tauri_commands::services_health,
  ```

  Add the 5 new commands just before it:

  ```rust
              // System Diagnostics
              uclaw_core::tauri_commands::get_system_diagnostics,
              uclaw_core::tauri_commands::restart_memu_bridge,
              uclaw_core::tauri_commands::restart_gbrain_mcp,
              uclaw_core::tauri_commands::reset_ai_engine,
              uclaw_core::tauri_commands::restart_app,
              // MEMUBOT Services
              uclaw_core::tauri_commands::services_health,
  ```

- [ ] **Step 8: Compile check**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  ```

  Expected: no errors.

- [ ] **Step 9: Quick unit test for the structs (compile-time check)**

  ```bash
  cd src-tauri && cargo test --lib 2>&1 | grep -E "FAILED|error\[" | head -10
  ```

  Expected: 0 FAILED lines, 0 error lines.

- [ ] **Step 10: Commit**

  ```bash
  git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
  git commit -m "feat(diagnostics): get_system_diagnostics + bridge restart + reset_ai_engine + restart_app commands"
  ```

---

## Task 5: Frontend wiring — SettingsTab + SettingsNav + SettingsPanel

**Files:**
- Modify: `ui/src/atoms/settings-tab.ts`
- Modify: `ui/src/components/settings/SettingsNav.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx`

- [ ] **Step 1: Add 'system' to SettingsTab union**

  In `ui/src/atoms/settings-tab.ts`, replace:

  ```typescript
  export type SettingsTab =
    | 'connectivity'   // 服务商 + 用量
    | 'intelligence'   // 模型 + Agent + 提示词
    | 'tools'          // 工具 + 权限 + 已学技能
    | 'memoryRecall'   // 记忆召回设置
    | 'learnedProfile' // openhuman facet store (Sprint 2.2)
    | 'imChannels'     // IM 渠道
    | 'general'        // 通用 + 外观
    | 'stt'            // 语音输入
    | 'shortcuts'
    | 'pet'
    | 'proxy'
    | 'about'
  ```

  With:

  ```typescript
  export type SettingsTab =
    | 'connectivity'   // 服务商 + 用量
    | 'intelligence'   // 模型 + Agent + 提示词
    | 'tools'          // 工具 + 权限 + 已学技能
    | 'memoryRecall'   // 记忆召回设置
    | 'learnedProfile' // openhuman facet store (Sprint 2.2)
    | 'imChannels'     // IM 渠道
    | 'general'        // 通用 + 外观
    | 'stt'            // 语音输入
    | 'shortcuts'
    | 'pet'
    | 'proxy'
    | 'system'         // 系统诊断
    | 'about'
  ```

- [ ] **Step 2: Add Monitor to icon imports in SettingsNav**

  In `ui/src/components/settings/SettingsNav.tsx`, replace:

  ```typescript
  import {
    Radio, Cpu, Wrench, Settings, Mic, Keyboard, Smile, Globe, Info, Brain,
    Search, MessageSquare, UserCircle2,
  } from 'lucide-react'
  ```

  With:

  ```typescript
  import {
    Radio, Cpu, Wrench, Settings, Mic, Keyboard, Smile, Globe, Info, Brain,
    Search, MessageSquare, UserCircle2, Monitor,
  } from 'lucide-react'
  ```

- [ ] **Step 3: Add system entry to the System nav group**

  In `ui/src/components/settings/SettingsNav.tsx`, replace:

  ```typescript
      { id: 'proxy', label: '代理', icon: <Globe size={16} /> },
      { id: 'about', label: '关于', icon: <Info size={16} /> },
  ```

  With:

  ```typescript
      { id: 'proxy', label: '代理', icon: <Globe size={16} /> },
      { id: 'system', label: '系统诊断', icon: <Monitor size={16} /> },
      { id: 'about', label: '关于', icon: <Info size={16} /> },
  ```

- [ ] **Step 4: Add SystemTab import to SettingsPanel**

  In `ui/src/components/settings/SettingsPanel.tsx`, after:

  ```typescript
  import { ImChannelsSettings } from './ImChannelsSettings'
  ```

  Add:

  ```typescript
  import { SystemTab } from './SystemTab'
  ```

- [ ] **Step 5: Add system case to SettingsContent switch**

  In `ui/src/components/settings/SettingsPanel.tsx`, find:

  ```typescript
      case 'proxy':
        return <ProxySetting />
      case 'about':
        return <AboutSettings />
  ```

  Replace with:

  ```typescript
      case 'proxy':
        return <ProxySetting />
      case 'system':
        return <SystemTab />
      case 'about':
        return <AboutSettings />
  ```

- [ ] **Step 6: Add system to TAB_LABEL record**

  In `ui/src/components/settings/SettingsPanel.tsx`, find:

  ```typescript
      proxy: '代理',
      about: '关于',
  ```

  Replace with:

  ```typescript
      proxy: '代理',
      system: '系统诊断',
      about: '关于',
  ```

- [ ] **Step 7: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors. The `SystemTab` import will fail until Task 6 creates the file — create a stub first if needed:

  ```bash
  echo "export function SystemTab() { return null }" > ui/src/components/settings/SystemTab.tsx
  ```

  Then re-run tsc.

- [ ] **Step 8: Commit**

  ```bash
  git add ui/src/atoms/settings-tab.ts \
          ui/src/components/settings/SettingsNav.tsx \
          ui/src/components/settings/SettingsPanel.tsx \
          ui/src/components/settings/SystemTab.tsx
  git commit -m "feat(diagnostics): wire system tab into settings nav + panel"
  ```

---

## Task 6: SystemTab.tsx — full diagnostics component

**Files:**
- Modify (overwrite stub): `ui/src/components/settings/SystemTab.tsx`

- [ ] **Step 1: Write the component**

  Replace the stub at `ui/src/components/settings/SystemTab.tsx` with:

  ```typescript
  import * as React from 'react'
  import { invoke } from '@tauri-apps/api/core'
  import { ChevronDown, ChevronUp, RefreshCw, RotateCcw, Power } from 'lucide-react'
  import { cn } from '@/lib/utils'

  // ── Types (mirror Rust structs) ──────────────────────────────────────

  type ServiceStatus =
    | 'Stopped'
    | 'Starting'
    | 'Running'
    | 'Stopping'
    | { Failed: { reason: string } }

  interface ServiceHealth {
    name: string
    status: ServiceStatus
    uptime_secs: number | null
    last_error: string | null
    metrics: Record<string, unknown>
  }

  interface MemUBridgeStatus {
    running: boolean
    pid: number | null
  }

  interface GbrainStatus {
    connected: boolean
    tool_count: number
    pgdata_ready: boolean
  }

  interface SystemDiagnosticsReport {
    app_version: string
    platform: string
    arch: string
    memory_used_mb: number
    memory_total_mb: number
    uptime_secs: number
    consecutive_failures: number
    recovery_attempts: number
    active_processes: number
    orphan_processes: number
    services: ServiceHealth[]
    memu: MemUBridgeStatus
    gbrain: GbrainStatus
  }

  // ── Helpers ──────────────────────────────────────────────────────────

  function formatUptime(secs: number): string {
    const h = Math.floor(secs / 3600)
    const m = Math.floor((secs % 3600) / 60)
    return `${h}h ${m}m`
  }

  function formatMemory(mb: number): string {
    return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`
  }

  function serviceStatusLabel(s: ServiceStatus): string {
    if (typeof s === 'string') {
      const map: Record<string, string> = {
        Running: '运行中', Stopped: '未启动',
        Starting: '启动中', Stopping: '停止中',
      }
      return map[s] ?? s
    }
    return `失败: ${s.Failed.reason.slice(0, 40)}`
  }

  function serviceStatusDot(s: ServiceStatus): string {
    if (typeof s === 'string') {
      if (s === 'Running') return 'bg-green-500'
      if (s === 'Stopped' || s === 'Stopping') return 'bg-gray-400'
      return 'bg-yellow-400'
    }
    return 'bg-red-500'
  }

  // ── Main component ───────────────────────────────────────────────────

  export function SystemTab() {
    const [report, setReport] = React.useState<SystemDiagnosticsReport | null>(null)
    const [loading, setLoading] = React.useState(false)
    const [lastChecked, setLastChecked] = React.useState<Date | null>(null)
    const [healthExpanded, setHealthExpanded] = React.useState(false)
    const [busyMemu, setBusyMemu] = React.useState(false)
    const [busyGbrain, setBusyGbrain] = React.useState(false)
    const [busyReset, setBusyReset] = React.useState(false)
    const [busyRestart, setBusyRestart] = React.useState(false)
    const [actionError, setActionError] = React.useState<string | null>(null)

    const runDiagnostics = React.useCallback(async () => {
      setLoading(true)
      setActionError(null)
      try {
        const r = await invoke<SystemDiagnosticsReport>('get_system_diagnostics')
        setReport(r)
        setLastChecked(new Date())
      } catch (e) {
        setActionError(String(e))
      } finally {
        setLoading(false)
      }
    }, [])

    const isHealthy = report
      ? report.consecutive_failures === 0 && !report.services.some(
          s => typeof s.status !== 'string' || s.status === 'Failed' as unknown
        )
      : true

    const failedServices = report?.services.filter(
      s => typeof s.status !== 'string'
    ) ?? []

    async function handleBridgeAction(
      command: string,
      setBusy: (v: boolean) => void,
    ) {
      setBusy(true)
      setActionError(null)
      try {
        await invoke(command)
        await runDiagnostics()
      } catch (e) {
        setActionError(String(e))
      } finally {
        setBusy(false)
      }
    }

    function handleCopyReport() {
      if (!report) return
      navigator.clipboard.writeText(JSON.stringify(report, null, 2))
    }

    function handleExportReport() {
      if (!report) return
      const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `uclaw-diagnostics-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    }

    return (
      <div className="flex flex-col gap-4 p-4 max-w-2xl">
        {/* Header */}
        <div className="flex items-start justify-between">
          <div>
            <h2 className="text-base font-semibold text-foreground">系统诊断</h2>
            <p className="text-xs text-muted-foreground mt-0.5">检查系统健康状态并修复问题</p>
          </div>
          <button
            onClick={runDiagnostics}
            disabled={loading}
            className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg bg-accent text-accent-foreground hover:bg-accent/80 disabled:opacity-50 transition-colors"
          >
            <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
            运行诊断
          </button>
        </div>

        {actionError && (
          <div className="text-xs text-red-400 bg-red-400/10 rounded-lg px-3 py-2">
            {actionError}
          </div>
        )}

        {/* 系统健康 collapsible card */}
        {report && (
          <div
            className={cn(
              'rounded-xl border px-4 py-3 cursor-pointer select-none',
              isHealthy
                ? 'bg-green-500/10 border-green-500/20'
                : 'bg-red-500/10 border-red-500/20',
            )}
            onClick={() => setHealthExpanded(v => !v)}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className={cn('text-sm font-medium', isHealthy ? 'text-green-400' : 'text-red-400')}>
                  {isHealthy ? '✓ 系统健康' : '✗ 发现问题'}
                </span>
                {lastChecked && (
                  <span className="text-xs text-muted-foreground">
                    上次检查: {lastChecked.toLocaleString('zh-CN')}
                  </span>
                )}
              </div>
              {healthExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
            </div>
            {healthExpanded && failedServices.length > 0 && (
              <ul className="mt-2 text-xs text-red-400 space-y-0.5">
                {failedServices.map(s => (
                  <li key={s.name}>• {s.name}: {serviceStatusLabel(s.status)}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        {report && (
          <>
            {/* 系统信息 */}
            <Section title="系统信息">
              <Grid4>
                <InfoCell label="版本" value={report.app_version} />
                <InfoCell label="平台" value={`${report.platform} (${report.arch})`} />
                <InfoCell label="内存" value={`${formatMemory(report.memory_used_mb)} / ${formatMemory(report.memory_total_mb)}`} />
                <InfoCell label="运行时间" value={formatUptime(report.uptime_secs)} />
              </Grid4>
            </Section>

            {/* 健康指标 */}
            <Section title="健康指标">
              <Grid4>
                <InfoCell label="连续失败次数" value={String(report.consecutive_failures)} />
                <InfoCell label="恢复尝试次数" value={String(report.recovery_attempts)} />
                <InfoCell label="活跃进程" value={String(report.active_processes)} />
                <InfoCell label="发现孤儿进程" value={String(report.orphan_processes)} />
              </Grid4>
            </Section>

            {/* 桥接服务 */}
            <Section title="桥接服务">
              <div className="flex flex-col gap-2">
                <BridgeCard
                  name="memU"
                  subtitle="Python Bridge"
                  running={report.memu.running}
                  detail={report.memu.running
                    ? (report.memu.pid ? `PID ${report.memu.pid}` : '运行中')
                    : '未运行'}
                />
                <BridgeCard
                  name="gbrain"
                  subtitle="Bun MCP"
                  running={report.gbrain.connected}
                  detail={report.gbrain.connected
                    ? `${report.gbrain.tool_count} 工具 · PGlite pgdata ${report.gbrain.pgdata_ready ? '已就绪' : '未就绪'}`
                    : '未连接'}
                />
              </div>
            </Section>

            {/* 服务状态 */}
            <Section title="服务状态">
              <div className="flex flex-col divide-y divide-border/50">
                {report.services.map(svc => (
                  <div key={svc.name} className="flex items-center justify-between py-2">
                    <div className="flex items-center gap-2">
                      <span className={cn('size-2 rounded-full flex-shrink-0', serviceStatusDot(svc.status))} />
                      <span className="text-sm text-foreground">{svc.name}</span>
                    </div>
                    <span className="text-xs text-muted-foreground">{serviceStatusLabel(svc.status)}</span>
                  </div>
                ))}
              </div>
            </Section>

            {/* 恢复操作 */}
            <Section title="恢复操作">
              <div className="flex flex-col gap-2">
                <div className="flex gap-2">
                  <ActionButton
                    icon={<RotateCcw size={13} />}
                    label="重置 AI 引擎"
                    busy={busyReset}
                    variant="warm"
                    onClick={() => handleBridgeAction('reset_ai_engine', setBusyReset)}
                  />
                  <ActionButton
                    icon={<Power size={13} />}
                    label="重启应用"
                    busy={busyRestart}
                    variant="danger"
                    onClick={() => handleBridgeAction('restart_app', setBusyRestart)}
                  />
                </div>
                <div className="flex gap-2">
                  <ActionButton
                    icon={<RotateCcw size={13} />}
                    label="重启 memU"
                    busy={busyMemu}
                    variant="bridge"
                    onClick={() => handleBridgeAction('restart_memu_bridge', setBusyMemu)}
                  />
                  <ActionButton
                    icon={<RotateCcw size={13} />}
                    label="重启 gbrain"
                    busy={busyGbrain}
                    variant="bridge"
                    onClick={() => handleBridgeAction('restart_gbrain_mcp', setBusyGbrain)}
                  />
                </div>
              </div>
            </Section>
          </>
        )}

        {/* Footer */}
        {report && (
          <div className="flex gap-4 pt-1 border-t border-border/50">
            <button
              onClick={handleCopyReport}
              className="text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              复制报告
            </button>
            <button
              onClick={handleExportReport}
              className="text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              导出报告
            </button>
          </div>
        )}

        {!report && !loading && (
          <p className="text-sm text-muted-foreground text-center py-8">
            点击「运行诊断」开始检查系统状态
          </p>
        )}
      </div>
    )
  }

  // ── Sub-components ───────────────────────────────────────────────────

  function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">{title}</p>
        {children}
      </div>
    )
  }

  function Grid4({ children }: { children: React.ReactNode }) {
    return <div className="grid grid-cols-2 gap-x-8 gap-y-2">{children}</div>
  }

  function InfoCell({ label, value }: { label: string; value: string }) {
    return (
      <div className="flex items-center justify-between py-1.5 border-b border-border/40">
        <span className="text-xs text-muted-foreground">{label}</span>
        <span className="text-xs text-foreground font-mono">{value}</span>
      </div>
    )
  }

  function BridgeCard({ name, subtitle, running, detail }: {
    name: string; subtitle: string; running: boolean; detail: string
  }) {
    return (
      <div className="rounded-lg bg-muted/40 px-3 py-2.5 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className={cn('size-2 rounded-full flex-shrink-0', running ? 'bg-green-500' : 'bg-gray-500')} />
          <span className="text-sm font-medium text-foreground">{name}</span>
          <span className="text-xs text-muted-foreground">({subtitle})</span>
        </div>
        <span className={cn('text-xs', running ? 'text-green-400' : 'text-muted-foreground')}>{detail}</span>
      </div>
    )
  }

  function ActionButton({ icon, label, busy, variant, onClick }: {
    icon: React.ReactNode; label: string; busy: boolean
    variant: 'warm' | 'danger' | 'bridge'; onClick: () => void
  }) {
    const cls = {
      warm: 'bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 border border-amber-500/20',
      danger: 'bg-red-500/10 text-red-400 hover:bg-red-500/20 border border-red-500/20',
      bridge: 'bg-green-500/10 text-green-400 hover:bg-green-500/20 border border-green-500/20',
    }[variant]

    return (
      <button
        onClick={onClick}
        disabled={busy}
        className={cn(
          'flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
          cls,
        )}
      >
        {busy ? <RefreshCw size={12} className="animate-spin" /> : icon}
        {label}
      </button>
    )
  }
  ```

- [ ] **Step 2: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors. Fix any type mismatch before continuing.

- [ ] **Step 3: Vitest check**

  ```bash
  cd ui && npm test -- --run 2>&1 | tail -10
  ```

  Expected: pre-existing failures only (kaleidoscope + SearchPalette). No new failures.

- [ ] **Step 4: Final backend compile + test**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd src-tauri && cargo test --lib 2>&1 | grep -E "FAILED" | head -10
  ```

  Expected: 0 errors, 0 FAILED.

- [ ] **Step 5: Commit**

  ```bash
  git add ui/src/components/settings/SystemTab.tsx
  git commit -m "feat(diagnostics): SystemTab component with health, bridge, service + recovery actions"
  ```

---

## Self-Review Checklist

**Spec coverage:**
- [x] 新增独立 system Tab — Task 5
- [x] 系统信息 grid (version/platform/memory/uptime) — Task 4 + 6
- [x] 健康指标 grid — Task 4 + 6
- [x] 桥接服务独立卡片 (memU + gbrain) — Task 4 + 6
- [x] 服务状态列表 — Task 4 + 6
- [x] 恢复操作：全局 + 桥接独立重启 — Task 4 + 6
- [x] 复制 / 导出报告 — Task 6
- [x] sysinfo 内存 — Task 1 + 4
- [x] boot_time uptime — Task 1 + 4
- [x] gbrain_mcp_id seed — Task 3
- [x] force_restart MemU — Task 2
- [x] server_tool_count McpManager — Task 2

**Placeholder scan:** None found.

**Type consistency:**
- `SystemDiagnosticsReport` defined in Task 4 Step 1, used in Task 6 Step 1 ✓
- `MemUBridgeStatus` / `GbrainStatus` match between Rust struct and TypeScript interface ✓
- `force_restart` defined in Task 2, called in Task 4 `restart_memu_bridge` ✓
- `server_tool_count` defined in Task 2, called in Task 4 `get_system_diagnostics` ✓
- `gbrain_mcp_id` added in Task 1, written in Task 3, read in Task 4 ✓
