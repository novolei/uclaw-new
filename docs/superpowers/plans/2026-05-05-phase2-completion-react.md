# Phase 2 Completion (React) + Phase 3 Kickoff — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Phase 2 所有遗留 TODO（后端 space_id 硬编码、stopAgent IPC、设置加载），补全前端 workspace 持久化、更新器集成等缺失连接，为 Phase 3 AI Browser 建立骨架。

**Architecture:** 后端补全三处关键 Rust TODO（space_id 推导、stopAgent、DB 查询），前端通过现有 Jotai + Tauri invoke 桥接补全所有 stub；Phase 3 新增 `browser/` Rust 模块和 CDP 工具注册（不起动真实 Chromium，只建架构）。

**Tech Stack:** Rust (Tauri v2, tokio, rusqlite, chromiumoxide预装), React 18, TypeScript, Jotai, @tauri-apps/api

---

## 重要背景：Phase 2 计划 vs 实际代码的差异

Phase 2 计划文件（`docs/superpowers/plans/2026-04-30-phase2-core-ux.md`）是用 **Svelte 5** 写的，但实际代码库已经是 **React 18 + TypeScript**。本计划基于当前 React 实现，不引用任何 `.svelte` 文件。

---

## 文件结构

### 后端修改文件
- `src-tauri/src/tauri_commands.rs` — 修复 space_id 硬编码 + 新增 stop_agent_session
- `src-tauri/src/agent/session.rs` — 新增 get_conversation_space_id 方法
- `src-tauri/src/agent/types.rs` — 若需新增 StopSignal 类型
- `src-tauri/src/main.rs` — 注册 stop_agent_session 命令
- `src-tauri/src/browser/mod.rs` — (新建) BrowserService 骨架
- `src-tauri/src/browser/tools.rs` — (新建) CDP 工具声明
- `src-tauri/src/browser/types.rs` — (新建) 浏览器内部类型
- `src-tauri/src/lib.rs` — 注册 browser 模块

### 前端修改文件
- `ui/src/App.tsx` — 启动时加载设置（修复 TODO:22）
- `ui/src/hooks/useCloseTab.ts` — 接入真实 stop_agent_session IPC（修复 TODO:81）
- `ui/src/hooks/useOpenSession.ts` — 持久化 workspace 选择（修复 TODO:55）
- `ui/src/components/tabs/TabBar.tsx` — 持久化 agentWorkspaceId（修复 TODO:83）
- `ui/src/atoms/ui-preferences.ts` — 持久化主题等 UI 偏好（修复 TODO:38）
- `ui/src/atoms/theme.ts` — 持久化 themeStyle（修复 TODO:170）
- `ui/src/atoms/updater.ts` — 接入 Tauri updater 插件（修复 TODO:45）
- `ui/src/components/settings/UpdateDialog.tsx` — 接入 updater（修复 TODO:16）
- `ui/src/components/environment/EnvironmentCheckDialog.tsx` — 接入环境检测（修复 TODO:65）
- `ui/src/components/agent/AgentMessages.tsx` — 移除 Legacy Path 1 分支（修复 TODO:544）
- `ui/src/lib/capabilities-toast.ts` — 接入 sonner toast（修复 TODO:95）

---

## Task 1: 后端 — 修复 space_id 硬编码

**背景：** `tauri_commands.rs` 第 178、276 行 `space_id = "default"` 是 TODO。需要从会话关联的 Space 推导真实的 space_id。

**Files:**
- Read: `src-tauri/src/agent/session.rs`
- Read: `src-tauri/src/db/migrations.rs` (确认 conversations 表结构)
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 确认 conversations 表有 space_id 列**

```bash
grep -n "space_id\|CREATE TABLE conv" /Users/ryanliu/Documents/uclaw/src-tauri/src/db/migrations.rs
```

预期输出：看到 `space_id TEXT` 列定义。若无，后续步骤需先加迁移。

- [ ] **Step 2: 查看 session.rs 的 get_conversation 方法**

```bash
grep -n "fn get\|space_id\|conversation_id" /Users/ryanliu/Documents/uclaw/src-tauri/src/agent/session.rs
```

- [ ] **Step 3: 在 session.rs 新增 get_conversation_space_id 方法**

在 `src-tauri/src/agent/session.rs` 的 `SessionManager` impl 块内添加：
```rust
pub fn get_space_id(&self, conversation_id: &str) -> Option<String> {
    self.db
        .lock()
        .ok()?
        .query_row(
            "SELECT space_id FROM conversations WHERE id = ?1",
            rusqlite::params![conversation_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
}
```

- [ ] **Step 4: 修复 tauri_commands.rs 第 178 行的 space_id**

找到 `send_message` 命令中的：
```rust
let space_id = "default"; // TODO: derive from conversation's space
```

替换为：
```rust
let space_id = {
    let session_mgr = state.session_manager.read().await;
    session_mgr
        .get_space_id(&input.conversation_id)
        .unwrap_or_else(|| "default".to_string())
};
```

- [ ] **Step 5: 修复第 276 行的 reflection_space_id**

找到：
```rust
let reflection_space_id = "default".to_string(); // TODO: derive from conversation's space
```

替换为：
```rust
let reflection_space_id = {
    let session_mgr = state.session_manager.read().await;
    session_mgr
        .get_space_id(&input.conversation_id)
        .unwrap_or_else(|| "default".to_string())
};
```

- [ ] **Step 6: 验证编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | tail -8
```

预期：`Finished` 无错误。修复任何借用或类型错误。

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/agent/session.rs src-tauri/src/tauri_commands.rs && git commit -m "fix: derive space_id from conversation in send_message and reflection"
```

---

## Task 2: 后端 — 实现 stop_agent_session IPC

**背景：** 前端 `useCloseTab.ts:81` 有一个 stub，等待 Tauri IPC `stop_agent`/`stop_agent_session`。当用户关闭 agent 标签时需要真正停止正在运行的 agentic loop。

**Files:**
- Read: `src-tauri/src/app.rs` (查看 AppState 结构)
- Read: `src-tauri/src/agent/agentic_loop.rs` (查看取消机制)
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 查看当前 AppState 有无 cancel token 机制**

```bash
grep -n "cancel\|abort\|CancellationToken\|HashMap\|running" /Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs | head -30
```

- [ ] **Step 2: 查看 agentic_loop 的中断机制**

```bash
grep -n "cancel\|abort\|CancellationToken\|break\|stop" /Users/ryanliu/Documents/uclaw/src-tauri/src/agent/agentic_loop.rs | head -20
```

- [ ] **Step 3: 在 AppState 添加 running_sessions 注册表**

在 `src-tauri/src/app.rs` 的 AppState 结构体中添加字段（在 `pending_approvals` 附近）：
```rust
pub running_sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>,
```

在 `AppState::new()` 中初始化：
```rust
running_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
```

若 `tokio-util` 未引入，先在 `Cargo.toml` 加：
```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

- [ ] **Step 4: 在 send_message 命令中注册 cancel token**

在 `tauri_commands.rs` 的 `send_message` 命令函数内，创建并注册 cancel token：
```rust
let cancel_token = tokio_util::sync::CancellationToken::new();
{
    let mut sessions = state.running_sessions.lock().await;
    sessions.insert(input.conversation_id.clone(), cancel_token.clone());
}

// 在 agentic loop 调用结束后移除
let result = /* ... 原有的 agentic loop 调用 ... */;
{
    let mut sessions = state.running_sessions.lock().await;
    sessions.remove(&input.conversation_id);
}
result
```

（具体插入位置需看 agentic_loop::run_agentic_loop 的签名 — 若支持 cancel_token 参数则传入；若不支持则先只注册/移除不传入，后续 Task 再实现真正取消）

- [ ] **Step 5: 新增 stop_agent_session 命令**

在 `tauri_commands.rs` 的命令区块末尾添加：
```rust
#[tauri::command]
pub async fn stop_agent_session(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<bool, Error> {
    let mut sessions = state.running_sessions.lock().await;
    if let Some(token) = sessions.remove(&conversation_id) {
        token.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}
```

- [ ] **Step 6: 注册命令到 invoke_handler**

在 `src-tauri/src/main.rs` 的 `invoke_handler!` 宏中添加：
```rust
uclaw_core::tauri_commands::stop_agent_session,
```

- [ ] **Step 7: 验证编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | tail -10
```

- [ ] **Step 8: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/app.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/Cargo.toml && git commit -m "feat: add stop_agent_session IPC command with CancellationToken registry"
```

---

## Task 3: 后端 — 修复 tauri_commands.rs 第 427 行 DB TODO

**Files:**
- Read: `src-tauri/src/tauri_commands.rs` (查看 427 行上下文)
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 查看 427 行的 TODO 上下文**

```bash
sed -n '420,445p' /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs
```

- [ ] **Step 2: 根据上下文实现 DB 查询**

根据 Step 1 看到的代码，用实际的 rusqlite 查询替换 TODO 占位符。典型模式：
```rust
let db = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
let result: SomeType = db.query_row(
    "SELECT ... FROM ... WHERE id = ?1",
    rusqlite::params![some_id],
    |row| row.get(0),
).map_err(Error::Database)?;
```

- [ ] **Step 3: 验证编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | tail -5
```

- [ ] **Step 4: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/tauri_commands.rs && git commit -m "fix: implement DB query at tauri_commands.rs line 427"
```

---

## Task 4: 前端 — App.tsx 启动时加载设置

**背景：** `ui/src/App.tsx:22` 有 `// TODO: 从 Tauri 后端加载初始设置`，导致每次启动都用默认值，不读取用户已保存的配置。

**Files:**
- Read: `ui/src/App.tsx`
- Read: `ui/src/lib/tauri-bridge.ts` 或 `ui/src/lib/api.ts` (查看现有设置读取 API)
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: 查看 App.tsx 中 TODO 的上下文**

```bash
sed -n '1,60p' /Users/ryanliu/Documents/uclaw/ui/src/App.tsx
```

- [ ] **Step 2: 查看可用的设置读取命令**

```bash
grep -n "get_settings\|load_settings\|getSettings\|loadSettings\|invoke.*settings" /Users/ryanliu/Documents/uclaw/ui/src/lib/*.ts 2>/dev/null | head -20
grep -n "get_settings\|load_settings" /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs | head -10
```

- [ ] **Step 3: 修复 App.tsx，在 useEffect 中加载设置**

找到 TODO 处，替换为真实的设置加载逻辑：
```tsx
useEffect(() => {
    const loadInitialSettings = async () => {
        try {
            const settings = await invoke<AppSettings>('get_settings');
            // 将 settings 分发到对应的 Jotai atoms
            // 例如：setTheme(settings.theme); setLanguage(settings.language);
        } catch (err) {
            console.error('[App] Failed to load initial settings:', err);
        }
    };
    loadInitialSettings();
}, []);
```

具体 invoke 命令名称根据 Step 2 的结果确认。

- [ ] **Step 4: 验证 TypeScript 类型检查**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -20
```

预期：无新增类型错误。

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/App.tsx && git commit -m "fix: load initial settings from Tauri backend on app startup"
```

---

## Task 5: 前端 — 修复 useCloseTab.ts stop_agent stub

**背景：** `ui/src/hooks/useCloseTab.ts:81` 有明确的 stub，等待 `stop_agent_session` IPC（Task 2 已实现）。

**Files:**
- Read: `ui/src/hooks/useCloseTab.ts`
- Modify: `ui/src/hooks/useCloseTab.ts`

- [ ] **Step 1: 查看 useCloseTab.ts 的 stub 上下文**

```bash
sed -n '70,100p' /Users/ryanliu/Documents/uclaw/ui/src/hooks/useCloseTab.ts
```

- [ ] **Step 2: 替换 stub 为真实 IPC 调用**

找到 stub 代码，替换为：
```typescript
import { invoke } from '@tauri-apps/api/core';

// 在 stub 位置替换：
try {
    await invoke('stop_agent_session', { conversationId: tab.sessionId });
} catch (err) {
    console.error('[useCloseTab] Failed to stop agent session:', err);
}
```

- [ ] **Step 3: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 4: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/hooks/useCloseTab.ts && git commit -m "fix: wire stop_agent_session IPC in useCloseTab, remove stub"
```

---

## Task 6: 前端 — 持久化 workspace 选择

**背景：** `useOpenSession.ts:55` 和 `useSyncActiveTabSideEffects.ts:64` 以及 `TabBar.tsx:83` 都有 `// TODO: Persist workspace selection via Tauri bridge`。

**Files:**
- Read: `ui/src/hooks/useOpenSession.ts`
- Read: `ui/src/hooks/useSyncActiveTabSideEffects.ts`
- Read: `ui/src/components/tabs/TabBar.tsx`
- Modify: 三个上述文件

- [ ] **Step 1: 查看现有 workspace 相关 IPC 命令**

```bash
grep -n "workspace\|agent_workspace\|agentWorkspace" /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs | head -20
```

- [ ] **Step 2: 查看 useOpenSession.ts 的 TODO 上下文**

```bash
sed -n '48,75p' /Users/ryanliu/Documents/uclaw/ui/src/hooks/useOpenSession.ts
```

- [ ] **Step 3: 替换三处 TODO**

在每个 TODO 处，添加实际的持久化调用。若后端已有 `set_agent_workspace` 或 `update_session_workspace` 命令：
```typescript
await invoke('set_agent_workspace', { sessionId, workspaceId });
```

若没有对应命令，用 localStorage 作为临时方案（并添加注释说明待后端实现）：
```typescript
// 临时方案：localStorage 持久化，待 Tauri IPC 命令实现后替换
localStorage.setItem(`workspace:${sessionId}`, workspaceId);
```

- [ ] **Step 4: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/hooks/useOpenSession.ts ui/src/hooks/useSyncActiveTabSideEffects.ts ui/src/components/tabs/TabBar.tsx && git commit -m "fix: persist workspace selection (Tauri IPC or localStorage fallback)"
```

---

## Task 7: 前端 — UI 偏好和主题持久化

**背景：** `ui-preferences.ts:38` 和 `theme.ts:170` 都有 TODO 关于持久化到后端。

**Files:**
- Read: `ui/src/atoms/ui-preferences.ts`
- Read: `ui/src/atoms/theme.ts`
- Modify: 两个上述文件

- [ ] **Step 1: 查看现有设置保存 IPC**

```bash
grep -n "patch_settings\|save_settings\|patchSettings\|saveSettings\|update_settings" /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs | head -10
grep -n "patch_settings\|save_settings\|patchSettings" /Users/ryanliu/Documents/uclaw/ui/src/lib/*.ts 2>/dev/null | head -10
```

- [ ] **Step 2: 查看 ui-preferences.ts 的 TODO 上下文**

```bash
sed -n '30,55p' /Users/ryanliu/Documents/uclaw/ui/src/atoms/ui-preferences.ts
```

- [ ] **Step 3: 修复 ui-preferences.ts，接入持久化**

在 TODO 处，调用已有的设置保存 IPC（如 `patch_settings`）：
```typescript
import { invoke } from '@tauri-apps/api/core';

// 替换 TODO:38
try {
    await invoke('patch_settings', { key: 'sidebarWidth', value: newValue });
} catch (err) {
    console.warn('[ui-preferences] Failed to persist preference:', err);
}
```

- [ ] **Step 4: 修复 theme.ts:170，取消注释持久化代码**

```bash
sed -n '163,180p' /Users/ryanliu/Documents/uclaw/ui/src/atoms/theme.ts
```

根据看到的代码，取消注释或实现 themeStyle 持久化。

- [ ] **Step 5: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/atoms/ui-preferences.ts ui/src/atoms/theme.ts && git commit -m "fix: persist UI preferences and theme style via Tauri IPC"
```

---

## Task 8: 前端 — Tauri Updater 集成

**背景：** `atoms/updater.ts:45` 和 `components/settings/UpdateDialog.tsx:16` 都等待接入 `@tauri-apps/plugin-updater`。

**Files:**
- Read: `ui/src/atoms/updater.ts`
- Read: `ui/src/components/settings/UpdateDialog.tsx`
- Read: `ui/package.json` (确认 plugin-updater 是否已安装)
- Modify: `ui/src/atoms/updater.ts`
- Modify: `ui/src/components/settings/UpdateDialog.tsx`

- [ ] **Step 1: 确认 plugin-updater 是否已安装**

```bash
grep "plugin-updater" /Users/ryanliu/Documents/uclaw/ui/package.json
grep "tauri-plugin-updater" /Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml
```

- [ ] **Step 2: 若未安装，安装插件**

若 `grep` 无结果：
```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm install @tauri-apps/plugin-updater
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo add tauri-plugin-updater
```

- [ ] **Step 3: 查看 updater.ts 的当前实现**

```bash
cat /Users/ryanliu/Documents/uclaw/ui/src/atoms/updater.ts
```

- [ ] **Step 4: 修复 updater.ts，接入 plugin-updater**

替换 stub 为真实实现：
```typescript
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export async function checkForUpdates(): Promise<UpdateInfo | null> {
    try {
        const update = await check();
        if (update) {
            return { version: update.version, body: update.body ?? '' };
        }
        return null;
    } catch (err) {
        console.error('[updater] Failed to check updates:', err);
        return null;
    }
}

export async function installUpdate(): Promise<void> {
    const update = await check();
    if (update) {
        await update.downloadAndInstall();
        await relaunch();
    }
}
```

- [ ] **Step 5: 修复 UpdateDialog.tsx，接入真实 checkForUpdates**

找到 TODO 处，将 stub 替换为调用上述 `checkForUpdates()` 和 `installUpdate()`。

- [ ] **Step 6: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/atoms/updater.ts ui/src/components/settings/UpdateDialog.tsx ui/package.json ui/package-lock.json && git commit -m "feat: integrate @tauri-apps/plugin-updater for in-app updates"
```

---

## Task 9: 前端 — 环境检测 IPC + capabilities-toast

**背景：** `EnvironmentCheckDialog.tsx:65` 等待 Tauri 环境检测命令；`capabilities-toast.ts:95` 等待 sonner toast 集成。

**Files:**
- Read: `ui/src/components/environment/EnvironmentCheckDialog.tsx`
- Read: `ui/src/lib/capabilities-toast.ts`
- Modify: 两个上述文件

- [ ] **Step 1: 查看现有环境检测命令**

```bash
grep -n "check_env\|environment\|python\|memu" /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs | head -20
```

- [ ] **Step 2: 查看 EnvironmentCheckDialog.tsx 的 TODO 上下文**

```bash
sed -n '55,90p' /Users/ryanliu/Documents/uclaw/ui/src/components/environment/EnvironmentCheckDialog.tsx
```

- [ ] **Step 3: 修复 EnvironmentCheckDialog.tsx**

根据 Step 1 找到的命令名，替换 TODO：
```typescript
const result = await invoke<EnvironmentStatus>('check_environment');
// 或已有的命令名，如 'get_memu_status', 'check_python_env' 等
```

若后端无对应命令，先用 mock 并添加注释：
```typescript
// 临时 mock — 待后端实现 check_environment 命令
const result: EnvironmentStatus = { pythonAvailable: false, memuAvailable: false };
```

- [ ] **Step 4: 修复 capabilities-toast.ts，接入 sonner**

```bash
sed -n '88,110p' /Users/ryanliu/Documents/uclaw/ui/src/lib/capabilities-toast.ts
```

查看 TODO 上下文后，将 `console.log` 或占位符替换为：
```typescript
import { toast } from 'sonner';

// 替换 TODO:95
toast.success(message, { description: details });
// 或
toast.warning(message);
```

- [ ] **Step 5: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/components/environment/EnvironmentCheckDialog.tsx ui/src/lib/capabilities-toast.ts && git commit -m "fix: wire environment check IPC and sonner toast in capabilities-toast"
```

---

## Task 10: 前端 — 清理 AgentMessages Legacy Path 1 代码

**背景：** `AgentMessages.tsx:544` 有明确注释，Legacy Path 1 分支将在未来版本移除。现在清理它以减少包体积和维护负担。

**Files:**
- Read: `ui/src/components/agent/AgentMessages.tsx` (重点看 540-600 行)
- Modify: `ui/src/components/agent/AgentMessages.tsx`

- [ ] **Step 1: 查看 Legacy Path 1 代码范围**

```bash
sed -n '530,600p' /Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentMessages.tsx
```

- [ ] **Step 2: 确认 Path 1 代码不被其他地方依赖**

```bash
grep -rn "Path 1\|path1\|legacyPath\|useLegacy" /Users/ryanliu/Documents/uclaw/ui/src/ --include="*.tsx" --include="*.ts" | grep -v "node_modules" | grep -v "AgentMessages.tsx"
```

预期：无其他引用。

- [ ] **Step 3: 删除 Legacy Path 1 分支**

根据 Step 1 看到的代码边界，删除整个 Legacy 分支（`if (isLegacyPath1)` 或类似条件分支）。保留 Path 2（新路径）的代码。

- [ ] **Step 4: 验证 TypeScript 编译和无未使用变量**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/components/agent/AgentMessages.tsx && git commit -m "refactor: remove legacy Path 1 code from AgentMessages"
```

---

## Task 11: 后端 Phase 3 — Browser 模块骨架

**背景：** Phase 3 目标是 AI Browser，基于 `chromiumoxide` + CDP。本 Task 只建立模块结构和类型，不启动真实 Chromium。

**Files:**
- Create: `src-tauri/src/browser/mod.rs`
- Create: `src-tauri/src/browser/types.rs`
- Create: `src-tauri/src/browser/tools.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 添加 chromiumoxide 依赖**

在 `src-tauri/Cargo.toml` 中，添加（features 按需调整）：
```toml
# Browser automation (Phase 3)
chromiumoxide = { version = "0.7", features = ["tokio-runtime"], optional = true }

[features]
default = []
ai-browser = ["chromiumoxide"]
```

使用 optional feature 避免影响普通编译。

运行：`cd src-tauri && cargo check 2>&1 | tail -5`
预期：`Finished` 无错误（optional dep 不实际编译）。

- [ ] **Step 2: 创建 browser/types.rs**

新建 `src-tauri/src/browser/types.rs`：
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub tab_id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub data: String, // base64 encoded PNG
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateInput {
    pub tab_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickInput {
    pub tab_id: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillInput {
    pub tab_id: String,
    pub selector: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateInput {
    pub tab_id: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub running: bool,
    pub tabs: Vec<BrowserTab>,
    pub active_tab_id: Option<String>,
}
```

- [ ] **Step 3: 创建 browser/mod.rs — BrowserService 骨架**

新建 `src-tauri/src/browser/mod.rs`：
```rust
pub mod types;
pub mod tools;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::Error;
use self::types::BrowserState;

/// BrowserService manages the lifecycle of a headless Chromium instance via CDP.
/// Phase 3 implementation — currently provides state management only.
pub struct BrowserService {
    state: Arc<RwLock<BrowserState>>,
}

impl BrowserService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(BrowserState {
                running: false,
                tabs: vec![],
                active_tab_id: None,
            })),
        }
    }

    pub async fn get_state(&self) -> BrowserState {
        self.state.read().await.clone()
    }

    /// Launch headless Chromium and establish CDP connection.
    /// Full implementation in Phase 3 using chromiumoxide feature flag.
    pub async fn launch(&self) -> Result<(), Error> {
        Err(Error::Internal("AI Browser not yet implemented (Phase 3)".into()))
    }

    /// Stop Chromium and clean up all CDP connections.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.running = false;
        state.tabs.clear();
        state.active_tab_id = None;
        Ok(())
    }
}

impl Default for BrowserService {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 创建 browser/tools.rs — 14 个 CDP 工具声明**

新建 `src-tauri/src/browser/tools.rs`：
```rust
/// CDP tool registry for the AI Browser.
/// Tools are registered into the agent's ToolRegistry when BrowserService is active.
/// Full CDP implementation added in Phase 3.

pub const BROWSER_TOOLS: &[(&str, &str)] = &[
    ("browser_navigate",    "Navigate to a URL in the browser tab"),
    ("browser_screenshot",  "Capture a screenshot of the current page"),
    ("browser_click",       "Click an element by CSS selector or coordinates"),
    ("browser_fill",        "Fill an input field with text"),
    ("browser_hover",       "Hover over an element"),
    ("browser_press_key",   "Press a keyboard key"),
    ("browser_scroll",      "Scroll the page or a specific element"),
    ("browser_drag",        "Drag from one element to another"),
    ("browser_evaluate",    "Execute JavaScript in the page context"),
    ("browser_inspect",     "Get the accessibility tree / DOM snapshot"),
    ("browser_snapshot",    "Get an accessibility snapshot of the page"),
    ("browser_wait_for",    "Wait for an element or condition"),
    ("browser_download",    "Download a file from the current URL"),
    ("browser_tab",         "Open, close, or switch between browser tabs"),
];

/// Returns the tool schema for all browser tools (stub — returns JSON schema array).
pub fn browser_tool_schemas() -> serde_json::Value {
    serde_json::json!(
        BROWSER_TOOLS.iter().map(|(name, desc)| {
            serde_json::json!({
                "name": name,
                "description": desc,
                "input_schema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            })
        }).collect::<Vec<_>>()
    )
}
```

- [ ] **Step 5: 注册 browser 模块到 lib.rs**

在 `src-tauri/src/lib.rs` 中添加：
```rust
pub mod browser;
```

- [ ] **Step 6: 验证编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | tail -10
```

预期：`Finished` 无错误。

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/browser/ src-tauri/src/lib.rs src-tauri/Cargo.toml && git commit -m "feat(phase3): add browser module skeleton with CDP tool registry"
```

---

## Task 12: 后端 Phase 3 — Browser IPC 命令 + AppState 集成

**Files:**
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 将 BrowserService 添加到 AppState**

在 `app.rs` 的 AppState 结构体中添加：
```rust
pub browser_service: Arc<crate::browser::BrowserService>,
```

在 `AppState::new()` 中初始化：
```rust
browser_service: Arc::new(crate::browser::BrowserService::new()),
```

- [ ] **Step 2: 添加 browser IPC 命令**

在 `tauri_commands.rs` 末尾添加：
```rust
// ─── Browser Commands (Phase 3) ─────────────────────────────────────────

#[tauri::command]
pub async fn browser_get_state(
    state: State<'_, AppState>,
) -> Result<crate::browser::types::BrowserState, Error> {
    Ok(state.browser_service.get_state().await)
}

#[tauri::command]
pub async fn browser_launch(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    state.browser_service.launch().await?;
    Ok(true)
}

#[tauri::command]
pub async fn browser_shutdown(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    state.browser_service.shutdown().await?;
    Ok(true)
}
```

- [ ] **Step 3: 注册命令**

在 `main.rs` 的 `invoke_handler!` 中添加：
```rust
uclaw_core::tauri_commands::browser_get_state,
uclaw_core::tauri_commands::browser_launch,
uclaw_core::tauri_commands::browser_shutdown,
```

- [ ] **Step 4: 验证编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | tail -10
```

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/app.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs && git commit -m "feat(phase3): integrate BrowserService into AppState and expose IPC commands"
```

---

## Task 13: 前端 Phase 3 — Browser Jotai atoms + BrowserViewer 骨架

**Files:**
- Create: `ui/src/atoms/browser-atoms.ts`
- Create: `ui/src/components/canvas/BrowserViewer.tsx`
- Modify: `ui/src/components/tabs/TabPreviewPanel.tsx` 或 Canvas 路由组件

- [ ] **Step 1: 创建 browser-atoms.ts**

新建 `ui/src/atoms/browser-atoms.ts`：
```typescript
import { atom } from 'jotai';

export interface BrowserTab {
  tabId: string;
  url: string;
  title: string;
}

export interface BrowserState {
  running: boolean;
  tabs: BrowserTab[];
  activeTabId: string | null;
}

export const browserStateAtom = atom<BrowserState>({
  running: false,
  tabs: [],
  activeTabId: null,
});

export const isBrowserLoadingAtom = atom(false);
```

- [ ] **Step 2: 创建 BrowserViewer.tsx 骨架**

新建 `ui/src/components/canvas/BrowserViewer.tsx`：
```tsx
import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAtom } from 'jotai';
import { browserStateAtom, isBrowserLoadingAtom } from '../../atoms/browser-atoms';

export const BrowserViewer: React.FC = () => {
    const [browserState, setBrowserState] = useAtom(browserStateAtom);
    const [isLoading, setIsLoading] = useAtom(isBrowserLoadingAtom);
    const [error, setError] = useState<string | null>(null);

    const handleLaunch = async () => {
        setIsLoading(true);
        setError(null);
        try {
            await invoke('browser_launch');
            const state = await invoke<typeof browserState>('browser_get_state');
            setBrowserState(state);
        } catch (err: any) {
            setError(err?.message || String(err));
        } finally {
            setIsLoading(false);
        }
    };

    if (!browserState.running) {
        return (
            <div className="flex flex-col items-center justify-center h-full gap-4 text-sm text-muted-foreground">
                <div className="text-4xl">🌐</div>
                <p>AI Browser (Phase 3)</p>
                {error && <p className="text-red-500 text-xs">{error}</p>}
                <button
                    onClick={handleLaunch}
                    disabled={isLoading}
                    className="px-4 py-2 rounded-lg bg-primary text-primary-foreground text-xs disabled:opacity-50"
                >
                    {isLoading ? '启动中...' : '启动 AI 浏览器'}
                </button>
                <p className="text-xs text-muted-foreground opacity-60">
                    完整 CDP 功能将在 Phase 3 实现
                </p>
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full">
            {/* 地址栏 — Phase 3 实现 */}
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
                <span className="text-xs text-muted-foreground">
                    已连接 Chromium • {browserState.tabs.length} 个标签
                </span>
            </div>
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
                CDP 工具界面 — Phase 3 开发中
            </div>
        </div>
    );
};
```

- [ ] **Step 3: 在 Canvas 路由中注册 BrowserViewer**

查看 Canvas 的内容路由文件（可能是 `TabPreviewPanel.tsx` 或类似）：
```bash
grep -n "viewer\|Viewer\|canvas\|Canvas\|preview\|Preview" /Users/ryanliu/Documents/uclaw/ui/src/components/tabs/*.tsx | head -20
```

在路由条件中添加：
```tsx
} else if (tab.type === 'browser') {
    return <BrowserViewer />;
}
```

- [ ] **Step 4: 验证 TypeScript 编译**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -20
```

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw && git add ui/src/atoms/browser-atoms.ts ui/src/components/canvas/BrowserViewer.tsx && git commit -m "feat(phase3): add browser atoms and BrowserViewer skeleton component"
```

---

## Task 14: 集成验证

- [ ] **Step 1: 完整后端编译**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | tail -15
```

预期：`Finished` 无错误。修复所有编译错误。

- [ ] **Step 2: 完整前端类型检查**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1
```

预期：零类型错误（或仅预有的已知错误，无新增）。

- [ ] **Step 3: 前端构建**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | tail -15
```

预期：构建成功，输出到 `../static/`。

- [ ] **Step 4: 回归验证 TODO 修复清单**

运行以下检查，确认 TODO 数量减少：
```bash
echo "=== Backend TODOs ===" && grep -rn "TODO\|FIXME" /Users/ryanliu/Documents/uclaw/src-tauri/src/ --include="*.rs" | wc -l
echo "=== Frontend TODOs ===" && grep -rn "TODO\|FIXME" /Users/ryanliu/Documents/uclaw/ui/src/ --include="*.ts" --include="*.tsx" | wc -l
```

预期：后端 ≤5 个，前端 ≤8 个（从当前 8+15 减少约一半）。

---

## Self-Review

### 规格覆盖检查

| 目标 | 对应 Task | 状态 |
|------|-----------|------|
| space_id 硬编码修复 | Task 1 | 覆盖 |
| stopAgent IPC 实现 | Task 2 | 覆盖 |
| DB TODO 修复 | Task 3 | 覆盖 |
| 启动加载设置 | Task 4 | 覆盖 |
| 关闭标签停止 agent | Task 5 | 覆盖 |
| Workspace 持久化 | Task 6 | 覆盖 |
| UI 偏好/主题持久化 | Task 7 | 覆盖 |
| Updater 集成 | Task 8 | 覆盖 |
| 环境检测 + Toast | Task 9 | 覆盖 |
| Legacy Path 1 清理 | Task 10 | 覆盖 |
| Phase 3 Browser 后端骨架 | Tasks 11-12 | 覆盖 |
| Phase 3 Browser 前端骨架 | Task 13 | 覆盖 |
| 集成验证 | Task 14 | 覆盖 |

### Placeholder 扫描

- Task 3 的具体实现依赖读取第 427 行上下文 — Step 1 强制先读 ✅
- Task 4/5/6 的具体 invoke 命令名依赖 grep 步骤确认 ✅
- browser/tools.rs 使用 `serde_json::json!` macro，需确认 serde_json 在 dep 中（它已在 Cargo.toml） ✅

### 类型一致性

- `BrowserState` 在 `browser/types.rs`（Rust）和 `browser-atoms.ts`（TS）中字段命名使用 `camelCase`（serde `rename_all` 处理转换） ✅
- `stop_agent_session` 命令在后端 Task 2 定义，前端 Task 5 使用相同命令名 ✅
