# 万花筒 Phase 2 · 能力组三模块 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把万花筒「能力」组的三个模块——记忆 / 技能 / 集成——从 `ComingSoonModule` 占位换成真实实现,并给集成模块加 MCP 可视化编辑器。

**Architecture:** 三个新模块挂在 `KaleidoscopeShell` 的模块路由下。记忆模块 full-bleed wrap 现有 `MemoryGraphView`;技能模块是左可折叠分组列表 + 右详情双栏(数据来自学得 + 内置两个来源,在容器组件 merge);集成模块是 MCP server 富卡片网格 + 详情抽屉(Sheet)+ 可视化编辑器(Dialog 模态框)+ 模板库。后端补齐 MCP 的 transport 选择与编辑现有 server 能力。语义拆分:`ToolSettings.tsx` 的 MCP 段和内置技能段移除,`SkillsSettings.tsx` 整体迁入技能模块。

**Tech Stack:** React 18 + TypeScript + Jotai + Tailwind(theme token)、Radix UI primitives(`Dialog` / `Sheet` / `AlertDialog`)、Vitest + RTL + jsdom;Rust + Tauri v2(`uclaw_core` crate)。

**Spec:** `docs/superpowers/specs/2026-05-14-kaleidoscope-phase2-design.md`

---

## File Structure

**后端(Task 1):**
- Modify `src-tauri/src/ipc.rs` — `McpServerInput` / `McpServerInfo` 扩字段
- Modify `src-tauri/src/tauri_commands.rs` — `add_mcp_server` 读 transport;新增 `update_mcp_server`;`list_mcp_servers` + `get_workspace_capabilities` 填新字段
- Modify `src-tauri/src/main.rs` — `invoke_handler!` 注册 `update_mcp_server`
- Modify `src-tauri/src/mcp.rs` — 文件末尾新增 `#[cfg(test)] mod tests`
- Modify `src-tauri/Cargo.toml` — `tempfile` 进 `[dev-dependencies]`

**前端类型 / bridge(Task 2):**
- Modify `ui/src/lib/types.ts` — `McpTransportType` 新增;`McpServerInfo` / `McpServerInput` 扩字段
- Modify `ui/src/lib/tauri-bridge.ts` — 新增 `updateMcpServer`

**记忆模块(Task 3):**
- Create `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx`
- Create `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.test.tsx`

**技能模块(Task 4):**
- Create `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.tsx` — 容器:数据 fetch + merge + 选中态 + 维护操作
- Create `ui/src/views/Kaleidoscope/modules/Skills/SkillsList.tsx` — 左栏:搜索 + 可折叠分组列表(presentational)
- Create `ui/src/views/Kaleidoscope/modules/Skills/SkillDetail.tsx` — 右栏:选中技能详情(presentational)
- Create `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.test.tsx`

**集成模块(Task 5):**
- Create `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx` — 容器
- Create `ui/src/views/Kaleidoscope/modules/Integrations/McpServerCard.tsx` — 富卡片(presentational)
- Create `ui/src/views/Kaleidoscope/modules/Integrations/McpDetailDrawer.tsx` — 详情抽屉(Sheet)
- Create `ui/src/views/Kaleidoscope/modules/Integrations/McpEditorModal.tsx` — 可视化编辑器(Dialog)
- Create `ui/src/views/Kaleidoscope/modules/Integrations/McpTemplateLibrary.tsx` — 模板库 + `MCP_TEMPLATES` 常量
- Create `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`

**路由 + 清理(Task 3/4/5/6):**
- Modify `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx` — 三个新分支(逐 task 加)
- Modify `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx` — 路由测试更新(逐 task 改)
- Modify `ui/src/components/settings/ToolSettings.tsx` — 删 MCP 段 + 内置技能段(Task 6)
- Modify `ui/src/components/settings/ToolsTab.tsx` — 删 SkillsSettings 段(Task 6)
- Delete `ui/src/components/settings/SkillsSettings.tsx`(Task 6)
- Delete `ui/src/components/settings/McpServerForm.tsx`(Task 6)

---

## Task 1: 后端 — MCP transport 选择 + update 命令

**Files:**
- Modify: `src-tauri/Cargo.toml`(`[dev-dependencies]` 段)
- Modify: `src-tauri/src/ipc.rs:399-422`(`McpServerInfo` / `McpServerInput`)
- Modify: `src-tauri/src/tauri_commands.rs`(`list_mcp_servers` ~2046、`add_mcp_server` ~2078、`get_workspace_capabilities` 内 ~2210;新增 `update_mcp_server`)
- Modify: `src-tauri/src/main.rs:330`(`invoke_handler!` MCP 块)
- Modify: `src-tauri/src/mcp.rs`(文件末尾新增测试模块)

### 背景

当前后端三个缺口:`add_mcp_server` 硬编码 `transport_type: Stdio` / `auto_approve: false` / `url: None`;`McpServerInput` 无 transport/url/auto_approve 字段;无 `update_mcp_server` 命令(虽然 `McpManager::update_server` 已存在,`mcp.rs:870`)。`McpManager` 的 `add_server`/`update_server` 都同步写盘到 `mcp_servers.json`。

- [ ] **Step 1: 确保 `tempfile` 在测试可用**

`src-tauri/Cargo.toml` 里 `tempfile = "3"` 当前在 `[target.'cfg(target_os = "macos")'.dependencies]` 下(macOS-only)。测试要跨平台可编译,加进 `[dev-dependencies]`。先查是否已有 `[dev-dependencies]` 段:

Run: `grep -n "\[dev-dependencies\]" src-tauri/Cargo.toml`

若有该段,在其下加一行 `tempfile = "3"`;若没有,在文件末尾追加:

```toml
[dev-dependencies]
tempfile = "3"
```

(cargo 会去重,与 macOS target 段并存无害。)

- [ ] **Step 2: 写后端失败测试**

在 `src-tauri/src/mcp.rs` 文件**末尾**追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(id: &str, transport: TransportType) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: format!("srv-{id}"),
            description: String::new(),
            transport_type: transport,
            command: "npx".into(),
            args: vec!["-y".into()],
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_approve: false,
        }
    }

    #[test]
    fn add_server_preserves_transport_type_and_url() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let mut http = cfg("a", TransportType::Http);
        http.url = Some("https://example.com/mcp".into());
        mgr.add_server(http).unwrap();
        let stored = mgr.all_servers().into_iter().find(|c| c.id == "a").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/mcp"));
    }

    #[test]
    fn update_server_rewrites_config_and_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            mgr.add_server(cfg("b", TransportType::Stdio)).unwrap();
            let mut updated = cfg("b", TransportType::Http);
            updated.url = Some("https://example.com/b".into());
            updated.auto_approve = true;
            mgr.update_server("b", updated).unwrap();
        }
        // Re-open from disk — confirms save_config persisted the update.
        let mgr2 = McpManager::new(dir.path());
        let stored = mgr2.all_servers().into_iter().find(|c| c.id == "b").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/b"));
        assert!(stored.auto_approve);
    }

    #[test]
    fn update_server_missing_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let err = mgr
            .update_server("nope", cfg("nope", TransportType::Stdio))
            .unwrap_err();
        assert!(err.contains("not found"));
    }
}
```

- [ ] **Step 3: 运行测试确认通过(这几个测的是已存在的 manager 方法)**

Run: `cd src-tauri && cargo test --lib mcp::tests 2>&1 | tail -15`
Expected: 3 个测试 PASS。`McpManager::add_server` / `update_server` 已存在且行为正确,这一步是给后续命令改动加安全网。若 FAIL on `tempfile` 未找到 → 回 Step 1 确认 `[dev-dependencies]`。

- [ ] **Step 4: 扩 `McpServerInput` 和 `McpServerInfo`(`ipc.rs`)**

把 `src-tauri/src/ipc.rs:399-422` 整段替换为:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub transport_type: crate::mcp::TransportType,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub url: Option<String>,
    pub enabled: bool,
    pub auto_approve: bool,
    pub error_message: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub transport_type: Option<crate::mcp::TransportType>,
    pub url: Option<String>,
    pub auto_approve: Option<bool>,
}
```

- [ ] **Step 5: 改 `add_mcp_server` 读 input 的 transport(`tauri_commands.rs`)**

把 `add_mcp_server` 函数体(`tauri_commands.rs` ~2078-2105)整个替换为:

```rust
#[tauri::command]
pub async fn add_mcp_server(state: State<'_, AppState>, input: McpServerInput) -> Result<McpServerInfo, Error> {
    let config = crate::mcp::McpServerConfig {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.clone(),
        description: input.description.clone(),
        transport_type: input.transport_type.clone().unwrap_or_default(),
        command: input.command.clone(),
        args: input.args.clone().unwrap_or_default(),
        env: input.env.clone().unwrap_or_default(),
        url: input.url.clone(),
        enabled: true,
        auto_approve: input.auto_approve.unwrap_or(false),
    };
    let mut mgr = state.mcp_manager.write().await;
    mgr.add_server(config.clone()).map_err(Error::InvalidInput)?;
    Ok(McpServerInfo {
        id: config.id,
        name: config.name,
        description: config.description,
        transport_type: config.transport_type,
        command: config.command,
        args: config.args,
        env: Some(config.env),
        url: config.url,
        enabled: config.enabled,
        auto_approve: config.auto_approve,
        error_message: None,
        status: "disconnected".into(),
    })
}
```

- [ ] **Step 6: 新增 `update_mcp_server` 命令(`tauri_commands.rs`)**

紧跟在 `add_mcp_server` 之后插入:

```rust
#[tauri::command]
pub async fn update_mcp_server(
    state: State<'_, AppState>,
    id: String,
    input: McpServerInput,
) -> Result<McpServerInfo, Error> {
    let mut mgr = state.mcp_manager.write().await;
    // 保留 enabled —— 编辑表单不拥有这个状态(卡片/抽屉的开关才管它)。
    let enabled = mgr
        .all_servers()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.enabled)
        .ok_or_else(|| Error::NotFound(format!("MCP server '{}' not found", id)))?;
    let config = crate::mcp::McpServerConfig {
        id: id.clone(),
        name: input.name.clone(),
        description: input.description.clone(),
        transport_type: input.transport_type.clone().unwrap_or_default(),
        command: input.command.clone(),
        args: input.args.clone().unwrap_or_default(),
        env: input.env.clone().unwrap_or_default(),
        url: input.url.clone(),
        enabled,
        auto_approve: input.auto_approve.unwrap_or(false),
    };
    mgr.update_server(&id, config.clone()).map_err(Error::InvalidInput)?;
    Ok(McpServerInfo {
        id: config.id,
        name: config.name,
        description: config.description,
        transport_type: config.transport_type,
        command: config.command,
        args: config.args,
        env: Some(config.env),
        url: config.url,
        enabled: config.enabled,
        auto_approve: config.auto_approve,
        error_message: None,
        status: "disconnected".into(),
    })
}
```

- [ ] **Step 7: 更新 `list_mcp_servers` 填新字段(`tauri_commands.rs`)**

`list_mcp_servers`(~2046)的 `.map(|c| {...})` 闭包里:把 `let (status_enum, _err)` 改成 `let (status_enum, err)`,并把闭包返回的 `McpServerInfo {...}` 替换为:

```rust
        McpServerInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            transport_type: c.transport_type.clone(),
            command: c.command.clone(),
            args: c.args.clone(),
            env: Some(c.env.clone()),
            url: c.url.clone(),
            enabled: c.enabled,
            auto_approve: c.auto_approve,
            error_message: err,
            status: status.into(),
        }
```

- [ ] **Step 8: 更新 `get_workspace_capabilities` 填新字段(`tauri_commands.rs`)**

`get_workspace_capabilities` 内的 `mcp_servers` 构造(~2200-2221):把 `let (status_enum, _err)` 改成 `let (status_enum, err)`,并把闭包里的 `McpServerInfo {...}` 替换为:

```rust
            McpServerInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                transport_type: c.transport_type.clone(),
                command: c.command.clone(),
                args: c.args.clone(),
                env: Some(c.env.clone()),
                url: c.url.clone(),
                enabled: c.enabled,
                auto_approve: c.auto_approve,
                error_message: err,
                status: status.into(),
            }
```

- [ ] **Step 9: 注册 `update_mcp_server`(`main.rs`)**

`src-tauri/src/main.rs:324`,在 `add_mcp_server,` 那一行下面插入:

```rust
            uclaw_core::tauri_commands::update_mcp_server,
```

- [ ] **Step 10: 编译 + 跑测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出(无 error)。

Run: `cd src-tauri && cargo test --lib mcp::tests 2>&1 | tail -8`
Expected: 3 passed。

若报 `TransportType` 未实现某 trait:`TransportType` 已 derive `Debug/Clone/Serialize/Deserialize/PartialEq`(`mcp.rs:211`),`Default`(`mcp.rs:218`)。`McpServerInfo` 用到的 `Serialize` 它都有,无需额外 derive。

- [ ] **Step 11: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/mcp.rs
git commit -m "$(cat <<'EOF'
feat(mcp): transport selection + update_mcp_server command

McpServerInput/Info 扩 transportType/url/autoApprove/errorMessage;
add_mcp_server 不再硬编码 Stdio;新增 update_mcp_server(wrap 已存在的
McpManager::update_server,保留 enabled 状态)。为万花筒集成模块的
可视化编辑器铺路。

adjacent: main.rs invoke_handler! 注册 update_mcp_server;mcp.rs 新增
#[cfg(test)] 覆盖 transport round-trip + 磁盘持久化。
EOF
)"
```

---

## Task 2: 前端 bridge + 类型

**Files:**
- Modify: `ui/src/lib/types.ts:848-869`(MCP 类型段)
- Modify: `ui/src/lib/tauri-bridge.ts:629-651`(MCP bridge 段)

依赖 Task 1(类型形状要对齐后端结构体)。

- [ ] **Step 1: 扩 MCP 类型(`types.ts`)**

把 `ui/src/lib/types.ts:848-869` 整段(`// ─── MCP ───` 到 `McpServerInput` 结束)替换为:

```typescript
// ─── MCP ────────────────────────────────────────────────────────────────

export type McpTransportType = 'stdio' | 'http';

export interface McpServerInfo {
  id: string;
  name: string;
  description: string;
  transportType: McpTransportType;
  command: string;
  args: string[];
  env?: Record<string, string>;
  url?: string | null;
  enabled: boolean;
  autoApprove: boolean;
  errorMessage?: string | null;
  status: string;
}

export interface McpServerInput {
  id?: string;
  name: string;
  description: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  transportType?: McpTransportType;
  url?: string | null;
  autoApprove?: boolean;
}
```

- [ ] **Step 2: 新增 `updateMcpServer` bridge(`tauri-bridge.ts`)**

`ui/src/lib/tauri-bridge.ts:632-633`,在 `addMcpServer` 定义之后插入:

```typescript
export const updateMcpServer = (id: string, input: McpServerInput): Promise<McpServerInfo> =>
  invoke('update_mcp_server', { id, input });
```

- [ ] **Step 3: TS 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: 可能报 `ToolSettings.tsx` / `McpServerForm.tsx` 里 `McpServerInfo` 用法相关的 error —— 这些文件 Task 6 会删/改,**本 task 不修**。只确认没有 `types.ts` / `tauri-bridge.ts` 自身的语法 error。若 `McpServerForm.tsx` 报 `addMcpServer` 入参缺字段:不会,新字段全是可选的。预期:0 个新增 error(已有文件因新字段不报错,因为旧字段都还在 + 新字段可选)。

- [ ] **Step 4: Commit**

```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(mcp): frontend types + updateMcpServer bridge

McpServerInfo/Input 对齐后端新字段(transportType/url/autoApprove/
errorMessage);新增 McpTransportType 与 updateMcpServer bridge。
EOF
)"
```

---

## Task 3: 记忆模块 + 路由接线

**Files:**
- Create: `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.test.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`

`MemoryGraphView` 自包含(根 `relative w-full h-full min-h-[300px]`,自己 fetch `memory_graph_get_full_graph`),只需 full-bleed wrap,与 `HumansModule` 同款。

- [ ] **Step 1: 写失败测试 `MemoryModule.test.tsx`**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { MemoryModule } from './MemoryModule'

vi.mock('@/components/memory/MemoryGraphView', () => ({
  MemoryGraphView: () => <div data-testid="memory-graph-view" />,
}))

describe('MemoryModule', () => {
  it('renders MemoryGraphView full-bleed', () => {
    const { container } = renderWithProviders(<MemoryModule />)
    expect(screen.getByTestId('memory-graph-view')).toBeInTheDocument()
    // full-bleed wrapper —— absolute inset-0,与 HumansModule 同款
    expect(container.querySelector('.absolute.inset-0')).not.toBeNull()
  })
})
```

- [ ] **Step 2: 运行确认失败**

Run: `cd ui && npm test -- --run MemoryModule 2>&1 | tail -10`
Expected: FAIL —— `Cannot find module './MemoryModule'`。

- [ ] **Step 3: 实现 `MemoryModule.tsx`**

```tsx
/**
 * MemoryModule — 万花筒「记忆」模块。
 *
 * full-bleed wrap 现有 MemoryGraphView。MemoryGraphView 自包含
 * (根 relative w-full h-full,自己 fetch memory_graph_get_full_graph、
 * 自带筛选/缩放/节点详情),需要一个确定尺寸的父容器 —— 用 absolute
 * inset-0(KaleidoscopeShell 主区卡片是 relative)。与 HumansModule 同款,
 * 不叠 ModuleHeader。
 */
import * as React from 'react'
import { MemoryGraphView } from '@/components/memory/MemoryGraphView'

export function MemoryModule(): React.ReactElement {
  return (
    <div className="absolute inset-0">
      <MemoryGraphView />
    </div>
  )
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd ui && npm test -- --run MemoryModule 2>&1 | tail -10`
Expected: PASS(1 test)。

- [ ] **Step 5: 路由接线 `KaleidoscopeShell.tsx`**

加 import(在 `import { AppsModule } ...` 之后):

```tsx
import { MemoryModule } from './modules/Memory/MemoryModule'
```

把路由三元里的 `: (` 之前加一个 `memory` 分支 —— 找到:

```tsx
              ) : moduleId === 'apps' ? (
                <AppsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
```

替换为:

```tsx
              ) : moduleId === 'apps' ? (
                <AppsModule />
              ) : moduleId === 'memory' ? (
                <MemoryModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
```

- [ ] **Step 6: 更新 `KaleidoscopeShell.test.tsx`**

在 `vi.mock('@/components/automation/AppsTab', ...)` 之后加一条 mock:

```tsx
vi.mock('@/components/memory/MemoryGraphView', () => ({
  MemoryGraphView: () => <div data-testid="memory-graph-view" />,
}))
```

在 `it('renders AppsTab for the apps module', ...)` 之后插入新测试:

```tsx
  it('renders MemoryGraphView for the memory module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'memory')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('memory-graph-view')).toBeInTheDocument()
  })
```

- [ ] **Step 7: 运行 shell 测试 + TS 检查**

Run: `cd ui && npm test -- --run KaleidoscopeShell 2>&1 | tail -12`
Expected: PASS(原 5 + 新 1 = 6 tests)。

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "Memory|Kaleidoscope" | head`
Expected: 无输出。

- [ ] **Step 8: Commit**

```bash
git add ui/src/views/Kaleidoscope/modules/Memory/ ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx
git commit -m "$(cat <<'EOF'
feat(kaleidoscope): Memory module — full-bleed wrap MemoryGraphView

记忆模块 = absolute inset-0 包 MemoryGraphView,与 HumansModule 同款
最小 wrap。KaleidoscopeShell 路由加 memory 分支。
EOF
)"
```

---

## Task 4: 技能模块 — 双栏(可折叠分组列表 + 详情)

**Files:**
- Create: `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Skills/SkillsList.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Skills/SkillDetail.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.test.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`

技能模块整体迁入 `SkillsSettings.tsx` 的能力:学得技能的详情渲染(场景/原则/步骤/陷阱 + 演化历史)、维护操作(回填关键词 / 整合技能)。`SkillsSettings.tsx` 本身在 Task 6 删除。`SkillEvolutionTab` / `SkillConsolidationDialog` 留在 `components/settings/`,由技能模块 import 复用。

数据模型:学得(`LearnedSkill`)+ 内置(`SkillInfo`)在 `SkillsModule` merge 成 `UnifiedSkill`。内置技能的 `id` 用 `name`(`SkillInfo` 无独立 id)。

- [ ] **Step 1: 写失败测试 `SkillsModule.test.tsx`**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import userEvent from '@testing-library/user-event'
import { SkillsModule } from './SkillsModule'

const learnedFixture = [
  {
    id: 'L1', name: 'systematic-debugging', context: '修复流式 bug 的场景',
    principles: '先复现', steps: '1. 复现', pitfalls: '别猜',
    enabled: true, usageCount: 3, createdAt: '2026-05-12T10:00:00Z',
  },
]
const builtinFixture = [
  {
    name: 'brainstorming', version: '1.0.0', description: '把想法变成设计',
    author: 'uclaw', enabled: true, category: 'design', provenance: 'bundled' as const,
  },
]

const listLearnedSkills = vi.fn()
const listSkills = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  listLearnedSkills: (...a: unknown[]) => listLearnedSkills(...a),
  toggleLearnedSkill: vi.fn().mockResolvedValue(undefined),
  deleteLearnedSkill: vi.fn().mockResolvedValue(undefined),
  proposeSkillConsolidation: vi.fn().mockResolvedValue({ clusters: [] }),
  backfillSkillKeywords: vi.fn().mockResolvedValue({ backfilledSkills: 0, totalLearnedSkills: 0, keywordsInserted: 0 }),
  listSkills: (...a: unknown[]) => listSkills(...a),
  toggleSkill: vi.fn().mockResolvedValue(true),
  forkSkillToUser: vi.fn().mockResolvedValue('~/.uclaw/skills/x'),
  reloadSkills: vi.fn().mockResolvedValue([]),
}))

// 重子树 —— stub 掉,本测试只关心 SkillsModule 的 merge / 分组 / 选中。
vi.mock('@/components/settings/SkillEvolutionTab', () => ({
  SkillEvolutionTab: () => <div data-testid="skill-evolution-tab" />,
}))
vi.mock('@/components/settings/SkillConsolidationDialog', () => ({
  SkillConsolidationDialog: () => null,
}))
vi.mock('react-markdown', () => ({ default: ({ children }: { children: string }) => <span>{children}</span> }))

describe('SkillsModule', () => {
  beforeEach(() => {
    listLearnedSkills.mockReset().mockResolvedValue(learnedFixture)
    listSkills.mockReset().mockResolvedValue(builtinFixture)
  })

  it('merges learned + builtin into the two groups with counts', async () => {
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    expect(screen.getByText('brainstorming')).toBeInTheDocument()
    expect(screen.getByText(/学得 · 1/)).toBeInTheDocument()
    expect(screen.getByText(/内置 · 1/)).toBeInTheDocument()
  })

  it('collapses a group when its header is clicked', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    await user.click(screen.getByText(/学得 · 1/))
    expect(screen.queryByText('systematic-debugging')).not.toBeInTheDocument()
  })

  it('shows the detail pane when a skill is selected', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    await user.click(screen.getByText('systematic-debugging'))
    // 学得技能详情显示「场景」段
    expect(screen.getByText('修复流式 bug 的场景')).toBeInTheDocument()
  })

  it('renders the empty state when both sources are empty', async () => {
    listLearnedSkills.mockResolvedValue([])
    listSkills.mockResolvedValue([])
    renderWithProviders(<SkillsModule />)
    await waitFor(() =>
      expect(screen.getByText(/还没学到技能/)).toBeInTheDocument(),
    )
  })
})
```

- [ ] **Step 2: 运行确认失败**

Run: `cd ui && npm test -- --run SkillsModule 2>&1 | tail -10`
Expected: FAIL —— `Cannot find module './SkillsModule'`。

- [ ] **Step 3: 实现 `SkillDetail.tsx`(右栏,presentational)**

```tsx
/**
 * SkillDetail — 技能模块右栏:选中技能的详情。
 *
 * 学得技能:场景 / 原则 / 步骤 / 陷阱 + 可展开的演化历史(SkillEvolutionTab)。
 * 内置技能:描述 / 版本 / 作者 / 分类 / provenance 徽章 + Fork(仅 bundled)。
 * 顶部右侧「Agent 可调用」开关。渲染逻辑迁自原 SkillsSettings 的 SkillCard 展开体。
 */
import * as React from 'react'
import Markdown from 'react-markdown'
import { History } from 'lucide-react'
import { Switch } from '@/components/ui/switch'
import { Button } from '@/components/ui/button'
import { SkillEvolutionTab } from '@/components/settings/SkillEvolutionTab'
import { cn } from '@/lib/utils'
import type { UnifiedSkill } from './SkillsModule'

function formatDate(s: string): string {
  if (!s) return ''
  const d = new Date(s)
  if (isNaN(d.getTime())) return s
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function Section({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div>
      <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/70">
        {label}
      </div>
      {children}
    </div>
  )
}

function MarkdownBlock({ text }: { text: string }): React.ReactElement {
  return (
    <div className="prose prose-sm max-w-none text-[12.5px] text-foreground/90
                    prose-p:my-1 prose-ul:my-1 prose-ol:my-1 prose-li:my-0
                    prose-headings:text-foreground prose-strong:text-foreground
                    prose-code:text-foreground prose-code:bg-muted prose-code:px-1 prose-code:rounded">
      <Markdown>{text}</Markdown>
    </div>
  )
}

export interface SkillDetailProps {
  skill: UnifiedSkill | null
  forking: boolean
  onToggleEnabled: (skill: UnifiedSkill, next: boolean) => void
  onRequestDelete: (skill: UnifiedSkill) => void
  onFork: (name: string) => void
}

export function SkillDetail({
  skill,
  forking,
  onToggleEnabled,
  onRequestDelete,
  onFork,
}: SkillDetailProps): React.ReactElement {
  const [showTimeline, setShowTimeline] = React.useState(false)

  // 切换选中技能时收起演化历史。
  React.useEffect(() => {
    setShowTimeline(false)
  }, [skill?.id])

  if (!skill) {
    return (
      <div className="flex-1 min-w-0 flex items-center justify-center bg-content-area">
        <div className="text-[13px] text-muted-foreground">选择左侧一个技能查看详情</div>
      </div>
    )
  }

  return (
    <div className="flex-1 min-w-0 overflow-y-auto px-7 py-6 bg-content-area">
      {/* 头部:名称 + 类型徽章 + 启用开关 */}
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[18px] font-semibold text-foreground truncate">{skill.name}</span>
            <span className="rounded-full bg-accent/15 border border-accent/35 px-2 py-0.5 text-[10px] text-accent-foreground">
              {skill.kind === 'learned' ? '学得' : '内置'}
            </span>
          </div>
          {skill.kind === 'learned' ? (
            <div className="mt-1 text-[11px] text-muted-foreground tabular-nums">
              使用 {skill.raw.usageCount} 次 · 创建于 {formatDate(skill.raw.createdAt)}
            </div>
          ) : (
            <div className="mt-1 text-[11px] text-muted-foreground">
              v{skill.raw.version} · {skill.raw.author} · {skill.raw.category || '未分类'}
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {skill.kind === 'builtin' && skill.raw.provenance === 'bundled' && (
            <Button
              size="sm"
              variant="outline"
              disabled={forking}
              onClick={() => onFork(skill.name)}
              className="h-7 px-2 text-[11.5px]"
            >
              {forking ? 'Fork 中…' : 'Fork 到我的'}
            </Button>
          )}
          <span className="text-[11px] text-muted-foreground">Agent 可调用</span>
          <Switch
            checked={skill.enabled}
            onCheckedChange={(next) => onToggleEnabled(skill, next)}
            aria-label="Agent 可调用"
          />
        </div>
      </div>

      {/* 主体 */}
      {skill.kind === 'builtin' ? (
        <div className="mt-5 space-y-3 text-[12.5px] text-foreground/90">
          {skill.raw.description && (
            <Section label="描述">
              <p className="leading-relaxed text-muted-foreground">{skill.raw.description}</p>
            </Section>
          )}
        </div>
      ) : (
        <div className="mt-5 space-y-3 text-[12.5px] text-foreground/90">
          <div className="flex items-center justify-between">
            <button
              type="button"
              onClick={() => setShowTimeline((v) => !v)}
              className={cn(
                'flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[11.5px] transition-colors',
                showTimeline
                  ? 'border-border bg-muted/60 text-foreground'
                  : 'border-border/40 text-muted-foreground hover:border-border hover:text-foreground',
              )}
            >
              <History className="size-3.5" />
              演化历史
            </button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => onRequestDelete(skill)}
              className="h-7 px-2 text-[11.5px] text-destructive hover:text-destructive"
            >
              删除
            </Button>
          </div>
          {showTimeline ? (
            <SkillEvolutionTab skillId={skill.raw.id} />
          ) : (
            <>
              {skill.raw.context && (
                <Section label="场景">
                  <p className="leading-relaxed text-muted-foreground">{skill.raw.context}</p>
                </Section>
              )}
              {skill.raw.principles && (
                <Section label="原则">
                  <MarkdownBlock text={skill.raw.principles} />
                </Section>
              )}
              {skill.raw.steps && (
                <Section label="步骤">
                  <MarkdownBlock text={skill.raw.steps} />
                </Section>
              )}
              {skill.raw.pitfalls && (
                <Section label="陷阱">
                  <MarkdownBlock text={skill.raw.pitfalls} />
                </Section>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: 实现 `SkillsList.tsx`(左栏,presentational)**

```tsx
/**
 * SkillsList — 技能模块左栏:搜索 + 两个可折叠分组(学得 / 内置)。
 *
 * 纯展示组件:数据、选中态、维护操作回调都由 SkillsModule 传入。
 * 分组折叠状态是本地 useState,不持久化。
 */
import * as React from 'react'
import { Search, RefreshCw, Combine, KeyRound, ChevronDown, ChevronRight } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { UnifiedSkill } from './SkillsModule'

interface GroupProps {
  label: string
  count: number
  open: boolean
  onToggle: () => void
  children: React.ReactNode
}

function Group({ label, count, open, onToggle, children }: GroupProps): React.ReactElement {
  return (
    <div>
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-1 px-1.5 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground"
      >
        {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        {label} · {count}
      </button>
      {open && <div className="space-y-0.5">{children}</div>}
    </div>
  )
}

function Row({
  skill,
  selected,
  onSelect,
}: {
  skill: UnifiedSkill
  selected: boolean
  onSelect: () => void
}): React.ReactElement {
  const secondary =
    skill.kind === 'learned'
      ? skill.raw.context.split('\n')[0]
      : skill.raw.category || skill.raw.description
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'w-full rounded-md border px-2.5 py-1.5 text-left transition-colors',
        selected
          ? 'border-accent/35 bg-accent/15'
          : 'border-transparent hover:bg-muted/40',
        !skill.enabled && 'opacity-60',
      )}
    >
      <div className="text-[12px] font-medium text-foreground truncate">{skill.name}</div>
      {secondary && (
        <div className="mt-0.5 text-[10px] text-muted-foreground truncate">{secondary}</div>
      )}
    </button>
  )
}

export interface SkillsListProps {
  learned: UnifiedSkill[]
  builtin: UnifiedSkill[]
  selectedId: string | null
  query: string
  loading: boolean
  canPropose: boolean
  proposing: boolean
  backfilling: boolean
  onSelect: (id: string) => void
  onQueryChange: (q: string) => void
  onReload: () => void
  onPropose: () => void
  onBackfill: () => void
}

export function SkillsList({
  learned,
  builtin,
  selectedId,
  query,
  loading,
  canPropose,
  proposing,
  backfilling,
  onSelect,
  onQueryChange,
  onReload,
  onPropose,
  onBackfill,
}: SkillsListProps): React.ReactElement {
  const [learnedOpen, setLearnedOpen] = React.useState(true)
  const [builtinOpen, setBuiltinOpen] = React.useState(true)

  return (
    <div className="flex w-64 shrink-0 flex-col border-r border-border bg-background">
      {/* header:搜索 + 自定义 */}
      <div className="border-b border-border/60 p-3 space-y-2">
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground pointer-events-none" />
          <Input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="搜索技能…"
            className="h-8 pl-8 text-[12px]"
          />
        </div>
        <Button
          size="sm"
          variant="outline"
          disabled
          title="未来扩展"
          className="h-7 w-full text-[11.5px]"
        >
          + 自定义技能
        </Button>
      </div>

      {/* 列表 */}
      <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-2">
        <Group label="学得" count={learned.length} open={learnedOpen} onToggle={() => setLearnedOpen((v) => !v)}>
          <div className="flex gap-1 px-1 pb-1">
            <Button
              size="sm"
              variant="ghost"
              onClick={onBackfill}
              disabled={backfilling || loading || learned.length === 0}
              title="为缺关键词索引的旧技能补全索引"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <KeyRound className={cn('size-3', backfilling && 'animate-pulse')} />
              回填关键词
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onPropose}
              disabled={proposing || loading || !canPropose}
              title="用 LLM 分析并合并概念重复的技能"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <Combine className={cn('size-3', proposing && 'animate-pulse')} />
              整合技能
            </Button>
          </div>
          {learned.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} />
          ))}
        </Group>

        <Group label="内置" count={builtin.length} open={builtinOpen} onToggle={() => setBuiltinOpen((v) => !v)}>
          <div className="flex px-1 pb-1">
            <Button
              size="sm"
              variant="ghost"
              onClick={onReload}
              disabled={loading}
              title="重新加载内置技能"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
              重新加载
            </Button>
          </div>
          {builtin.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} />
          ))}
        </Group>
      </div>
    </div>
  )
}
```

- [ ] **Step 5: 实现 `SkillsModule.tsx`(容器)**

```tsx
/**
 * SkillsModule — 万花筒「技能」模块。
 *
 * 左可折叠分组列表(SkillsList)+ 右详情(SkillDetail)双栏。数据来自两个
 * 来源 —— 学得技能(listLearnedSkills)+ 内置技能(listSkills)—— 在此 merge
 * 成 UnifiedSkill 列表。维护操作(回填关键词 / 整合技能 / 重新加载)与删除确认
 * 也由本容器持有。整体迁自原 components/settings/SkillsSettings.tsx。
 */
import * as React from 'react'
import { toast } from 'sonner'
import {
  listLearnedSkills,
  toggleLearnedSkill,
  deleteLearnedSkill,
  proposeSkillConsolidation,
  backfillSkillKeywords,
  listSkills,
  toggleSkill,
  forkSkillToUser,
  reloadSkills,
  type SkillConsolidationProposal,
} from '@/lib/tauri-bridge'
import type { LearnedSkill, SkillInfo } from '@/lib/types'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { SkillsList } from './SkillsList'
import { SkillDetail } from './SkillDetail'
import { SkillConsolidationDialog } from '@/components/settings/SkillConsolidationDialog'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

export type UnifiedSkill =
  | { kind: 'learned'; id: string; name: string; enabled: boolean; raw: LearnedSkill }
  | { kind: 'builtin'; id: string; name: string; enabled: boolean; raw: SkillInfo }

export function SkillsModule(): React.ReactElement {
  const [learnedRaw, setLearnedRaw] = React.useState<LearnedSkill[]>([])
  const [builtinRaw, setBuiltinRaw] = React.useState<SkillInfo[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState('')
  const [pendingDelete, setPendingDelete] = React.useState<UnifiedSkill | null>(null)
  const [forkingName, setForkingName] = React.useState<string | null>(null)
  const [proposing, setProposing] = React.useState(false)
  const [backfilling, setBackfilling] = React.useState(false)
  const [proposal, setProposal] = React.useState<SkillConsolidationProposal | null>(null)
  const [consolidationOpen, setConsolidationOpen] = React.useState(false)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    const [l, b] = await Promise.allSettled([listLearnedSkills(), listSkills()])
    if (l.status === 'fulfilled') setLearnedRaw(l.value)
    else toast.error('加载学得技能失败', { description: String(l.reason) })
    if (b.status === 'fulfilled') setBuiltinRaw(b.value)
    else toast.error('加载内置技能失败', { description: String(b.reason) })
    setLoading(false)
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  const learned: UnifiedSkill[] = React.useMemo(
    () => learnedRaw.map((s) => ({ kind: 'learned', id: s.id, name: s.name, enabled: s.enabled, raw: s })),
    [learnedRaw],
  )
  const builtin: UnifiedSkill[] = React.useMemo(
    () => builtinRaw.map((s) => ({ kind: 'builtin', id: s.name, name: s.name, enabled: s.enabled, raw: s })),
    [builtinRaw],
  )

  const filterFn = React.useCallback(
    (s: UnifiedSkill) => {
      const q = query.trim().toLowerCase()
      return !q || s.name.toLowerCase().includes(q)
    },
    [query],
  )
  const learnedFiltered = learned.filter(filterFn)
  const builtinFiltered = builtin.filter(filterFn)

  const selected =
    [...learned, ...builtin].find((s) => s.id === selectedId) ?? null

  const onToggleEnabled = async (skill: UnifiedSkill, next: boolean) => {
    if (skill.kind === 'learned') {
      setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleLearnedSkill(skill.id, next)
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: !next } : s)))
      }
    } else {
      setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleSkill({ name: skill.name, enabled: next })
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: !next } : s)))
      }
    }
  }

  const onConfirmDelete = async () => {
    if (!pendingDelete || pendingDelete.kind !== 'learned') return
    const target = pendingDelete
    setPendingDelete(null)
    const snapshot = learnedRaw
    setLearnedRaw((prev) => prev.filter((s) => s.id !== target.id))
    if (selectedId === target.id) setSelectedId(null)
    try {
      await deleteLearnedSkill(target.id)
      toast.success(`已删除「${target.name}」`)
    } catch (err) {
      toast.error('删除失败', { description: String(err) })
      setLearnedRaw(snapshot)
    }
  }

  const onFork = async (name: string) => {
    setForkingName(name)
    try {
      const destPath = await forkSkillToUser(name)
      toast.success(`已 Fork 到 ${destPath}`, {
        description: '现在可以在 ~/.uclaw/skills/ 下编辑这份 skill。',
      })
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('Fork 失败', { description: String(err) })
    } finally {
      setForkingName(null)
    }
  }

  const onReload = async () => {
    setLoading(true)
    try {
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('重新加载失败', { description: String(err) })
    } finally {
      setLoading(false)
    }
  }

  const onPropose = async () => {
    setProposing(true)
    try {
      const result = await proposeSkillConsolidation()
      if (!result.clusters || result.clusters.length === 0) {
        toast.info('暂无可合并的重复技能')
        return
      }
      setProposal(result)
      setConsolidationOpen(true)
    } catch (err) {
      toast.error('无法分析技能整合方案', { description: String(err) })
    } finally {
      setProposing(false)
    }
  }

  const onBackfill = async () => {
    setBackfilling(true)
    try {
      const result = await backfillSkillKeywords()
      if (result.backfilledSkills === 0) {
        toast.info('关键词索引已完整', {
          description: `${result.totalLearnedSkills} 条技能全部已索引`,
        })
      } else {
        toast.success('关键词回填完成', {
          description: `${result.backfilledSkills}/${result.totalLearnedSkills} 条新增 · 共 ${result.keywordsInserted} 个关键词`,
        })
      }
    } catch (err) {
      toast.error('回填关键词失败', { description: String(err) })
    } finally {
      setBackfilling(false)
    }
  }

  const isEmpty = !loading && learned.length === 0 && builtin.length === 0
  const enabledLearnedCount = learned.filter((s) => s.enabled).length

  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader
        group="capability"
        title="技能"
        subtitle={`学得 ${learned.length} · 内置 ${builtin.length}`}
      />
      {isEmpty ? (
        <div className="flex-1 min-h-0 flex items-center justify-center">
          <div className="rounded-lg border border-dashed border-border bg-muted/10 px-8 py-10 text-center">
            <div className="text-[13px] text-foreground/80">你的 Agent 还没学到技能</div>
            <div className="mt-1 text-[11.5px] text-muted-foreground">
              让它处理几次任务就会积累。
            </div>
          </div>
        </div>
      ) : (
        <div className="flex flex-1 min-h-0">
          <SkillsList
            learned={learnedFiltered}
            builtin={builtinFiltered}
            selectedId={selectedId}
            query={query}
            loading={loading}
            canPropose={enabledLearnedCount >= 2}
            proposing={proposing}
            backfilling={backfilling}
            onSelect={setSelectedId}
            onQueryChange={setQuery}
            onReload={() => void onReload()}
            onPropose={() => void onPropose()}
            onBackfill={() => void onBackfill()}
          />
          <SkillDetail
            skill={selected}
            forking={selected?.kind === 'builtin' && forkingName === selected.name}
            onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
            onRequestDelete={(s) => setPendingDelete(s)}
            onFork={(name) => void onFork(name)}
          />
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除技能？</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-2 text-sm text-muted-foreground">
                <p>即将删除「{pendingDelete?.name ?? ''}」，连同它的版本、关键词和关联边都会被清除。</p>
                <p>这个操作无法撤销。</p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void onConfirmDelete()}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <SkillConsolidationDialog
        open={consolidationOpen}
        proposal={proposal}
        onOpenChange={(next) => {
          setConsolidationOpen(next)
          if (!next) setProposal(null)
        }}
        onApplied={() => {
          void refetch()
        }}
      />
    </div>
  )
}
```

- [ ] **Step 6: 运行确认通过**

Run: `cd ui && npm test -- --run SkillsModule 2>&1 | tail -12`
Expected: PASS(4 tests)。

若 `Promise.allSettled` mock 相关报错:确认测试里 `listLearnedSkills` / `listSkills` 用 `mockResolvedValue`。若 `SkillConsolidationProposal` 类型 import 报错:它从 `@/lib/tauri-bridge` 导出(`SkillsSettings.tsx` 原样 import 过,确认存在)。

- [ ] **Step 7: 路由接线 `KaleidoscopeShell.tsx`**

加 import(在 `MemoryModule` import 之后):

```tsx
import { SkillsModule } from './modules/Skills/SkillsModule'
```

路由三元里,在 `memory` 分支之后加 `skills` 分支:

```tsx
              ) : moduleId === 'memory' ? (
                <MemoryModule />
              ) : moduleId === 'skills' ? (
                <SkillsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
```

- [ ] **Step 8: 更新 `KaleidoscopeShell.test.tsx`**

`KaleidoscopeShell` 渲染 `SkillsModule` 会触发它的 tauri-bridge 调用。在测试文件顶部的 `vi.mock('@/lib/tauri-bridge', ...)` 里补齐 SkillsModule 用到的函数 —— 把现有那条 mock 替换为:

```tsx
vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
  listAutomationsHumane: vi.fn().mockResolvedValue([]),
  listLearnedSkills: vi.fn().mockResolvedValue([]),
  toggleLearnedSkill: vi.fn().mockResolvedValue(undefined),
  deleteLearnedSkill: vi.fn().mockResolvedValue(undefined),
  proposeSkillConsolidation: vi.fn().mockResolvedValue({ clusters: [] }),
  backfillSkillKeywords: vi.fn().mockResolvedValue({ backfilledSkills: 0, totalLearnedSkills: 0, keywordsInserted: 0 }),
  listSkills: vi.fn().mockResolvedValue([]),
  toggleSkill: vi.fn().mockResolvedValue(true),
  forkSkillToUser: vi.fn().mockResolvedValue(''),
  reloadSkills: vi.fn().mockResolvedValue([]),
}))
```

补 stub(在 `vi.mock('@/components/memory/MemoryGraphView', ...)` 之后):

```tsx
vi.mock('@/components/settings/SkillConsolidationDialog', () => ({
  SkillConsolidationDialog: () => null,
}))
```

把原来那条 `it('renders the ComingSoon placeholder ...')` 用的 `'skills'` 改成 `'artifacts'` —— 整条替换为:

```tsx
  it('renders SkillsModule for the skills module', async () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(await screen.findByText('技能')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a not-yet-built module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'artifacts')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
```

- [ ] **Step 9: 跑测试 + TS 检查**

Run: `cd ui && npm test -- --run KaleidoscopeShell SkillsModule 2>&1 | tail -14`
Expected: 全 PASS(KaleidoscopeShell 7 tests + SkillsModule 4 tests)。

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "Skills|Kaleidoscope" | head`
Expected: 无输出。

- [ ] **Step 10: Commit**

```bash
git add ui/src/views/Kaleidoscope/modules/Skills/ ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx
git commit -m "$(cat <<'EOF'
feat(kaleidoscope): Skills module — 2-pane collapsible list + detail

技能模块:左可折叠分组列表(学得/内置)+ 右详情双栏。数据 merge 自
listLearnedSkills + listSkills。详情渲染、维护操作(回填关键词/整合技能)
整体迁自 SkillsSettings.tsx(该文件 Task 6 删除)。SkillEvolutionTab /
SkillConsolidationDialog 留在 settings/ 被复用。KaleidoscopeShell 加
skills 分支。
EOF
)"
```

---

## Task 5: 集成模块 — MCP 卡片网格 + 抽屉 + 可视化编辑器

**Files:**
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/McpServerCard.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/McpDetailDrawer.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/McpTemplateLibrary.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/McpEditorModal.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`

富卡片网格(变体 B)+ 详情抽屉(`Sheet`)+ 可视化编辑器(`Dialog` 居中模态框,变体 A)+ 模板库。`listMcpTools()` 返回全部已连接 server 的工具,在容器按 `serverId` 分组。

- [ ] **Step 1: 写失败测试 `IntegrationsModule.test.tsx`**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import userEvent from '@testing-library/user-event'
import { IntegrationsModule } from './IntegrationsModule'

const serversFixture = [
  {
    id: 'gh', name: 'github', description: 'GitHub 操作',
    transportType: 'stdio' as const, command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-github'], env: { GITHUB_TOKEN: 'x' },
    url: null, enabled: true, autoApprove: false, errorMessage: null, status: 'connected',
  },
  {
    id: 'sl', name: 'slack', description: '',
    transportType: 'stdio' as const, command: 'npx', args: [], env: {},
    url: null, enabled: true, autoApprove: false,
    errorMessage: 'spawn failed', status: 'error',
  },
]
const toolsFixture = [
  { serverId: 'gh', name: 'create_pull_request', description: '', parameters: {} },
  { serverId: 'gh', name: 'list_issues', description: '', parameters: {} },
]

const listMcpServers = vi.fn()
const listMcpTools = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  listMcpServers: (...a: unknown[]) => listMcpServers(...a),
  listMcpTools: (...a: unknown[]) => listMcpTools(...a),
  addMcpServer: vi.fn().mockResolvedValue(serversFixture[0]),
  updateMcpServer: vi.fn().mockResolvedValue(serversFixture[0]),
  removeMcpServer: vi.fn().mockResolvedValue(true),
  toggleMcpServer: vi.fn().mockResolvedValue(true),
  restartMcpServer: vi.fn().mockResolvedValue(true),
  connectMcpServer: vi.fn().mockResolvedValue(true),
}))

describe('IntegrationsModule', () => {
  beforeEach(() => {
    listMcpServers.mockReset().mockResolvedValue(serversFixture)
    listMcpTools.mockReset().mockResolvedValue(toolsFixture)
  })

  it('renders one card per MCP server', async () => {
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    expect(screen.getByText('slack')).toBeInTheDocument()
  })

  it('opens the detail drawer when a card is clicked', async () => {
    const user = userEvent.setup()
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    await user.click(screen.getByText('github'))
    // 抽屉显示工具列表
    await waitFor(() => expect(screen.getByText('create_pull_request')).toBeInTheDocument())
  })

  it('opens the editor modal from the add button', async () => {
    const user = userEvent.setup()
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: /添加集成/ }))
    // 模板库出现
    expect(await screen.findByText('从模板新建')).toBeInTheDocument()
  })

  it('renders the empty state when there are no servers', async () => {
    listMcpServers.mockResolvedValue([])
    listMcpTools.mockResolvedValue([])
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText(/没有集成/)).toBeInTheDocument())
  })
})
```

- [ ] **Step 2: 运行确认失败**

Run: `cd ui && npm test -- --run IntegrationsModule 2>&1 | tail -10`
Expected: FAIL —— `Cannot find module './IntegrationsModule'`。

- [ ] **Step 3: 实现 `McpServerCard.tsx`(富卡片,presentational)**

```tsx
/**
 * McpServerCard — 集成模块的富卡片(变体 B)。
 *
 * icon + 名称 + 状态点 + 「transport · N 工具」+ 工具名 chips 预览。
 * 纯展示;点击由父级处理。状态点用功能语义色(绿/红/灰),非装饰色。
 */
import * as React from 'react'
import { cn } from '@/lib/utils'
import type { McpServerInfo } from '@/lib/types'

const STATUS_DOT: Record<string, string> = {
  connected: 'bg-emerald-500',
  error: 'bg-red-500',
  connecting: 'bg-amber-500',
  disconnected: 'bg-muted-foreground/40',
}

export interface McpServerCardProps {
  server: McpServerInfo
  toolNames: string[]
  selected: boolean
  onClick: () => void
}

export function McpServerCard({ server, toolNames, selected, onClick }: McpServerCardProps): React.ReactElement {
  const dot = STATUS_DOT[server.status] ?? STATUS_DOT.disconnected
  const isError = server.status === 'error'
  const previewChips = toolNames.slice(0, 3)
  const moreCount = toolNames.length - previewChips.length

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'rounded-xl border p-3.5 text-left transition-colors',
        selected
          ? 'border-accent/35 bg-accent/15'
          : 'border-border bg-card hover:bg-muted/40',
      )}
    >
      <div className="flex items-center gap-2">
        <div className="flex size-7 items-center justify-center rounded-lg bg-muted text-[13px]">
          {server.name.charAt(0).toUpperCase()}
        </div>
        <div className="text-[13px] font-semibold text-foreground truncate">{server.name}</div>
        <span className={cn('ml-auto size-1.5 rounded-full', dot)} title={server.status} />
      </div>
      {isError ? (
        <div className="mt-2 text-[11px] text-red-500">连接失败 · 点击查看详情</div>
      ) : (
        <div className="mt-2 text-[11px] text-muted-foreground">
          {server.transportType} · {toolNames.length} 个工具
        </div>
      )}
      {previewChips.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {previewChips.map((t) => (
            <span key={t} className="rounded bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground">
              {t}
            </span>
          ))}
          {moreCount > 0 && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground">
              +{moreCount}
            </span>
          )}
        </div>
      )}
    </button>
  )
}
```

- [ ] **Step 4: 实现 `McpDetailDrawer.tsx`(详情抽屉,Sheet)**

```tsx
/**
 * McpDetailDrawer — 集成模块的详情抽屉(右侧 Sheet)。
 *
 * 展示选中 MCP server 的工具列表、状态/错误、操作(重启/移除/编辑/启用开关)。
 * 不做时间戳日志流 —— error 状态显示 errorMessage 即可(spec §6.3)。
 */
import * as React from 'react'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import type { McpServerInfo } from '@/lib/types'

export interface McpDetailDrawerProps {
  server: McpServerInfo | null
  toolNames: string[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onToggleEnabled: (server: McpServerInfo, next: boolean) => void
  onRestart: (server: McpServerInfo) => void
  onRemove: (server: McpServerInfo) => void
  onEdit: (server: McpServerInfo) => void
}

export function McpDetailDrawer({
  server,
  toolNames,
  open,
  onOpenChange,
  onToggleEnabled,
  onRestart,
  onRemove,
  onEdit,
}: McpDetailDrawerProps): React.ReactElement {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-[340px] sm:max-w-[340px] bg-popover">
        {server && (
          <>
            <SheetHeader>
              <SheetTitle className="flex items-center gap-2">
                <span className="truncate">{server.name}</span>
                <span className="text-[11px] font-normal text-muted-foreground">
                  {server.transportType}
                </span>
              </SheetTitle>
            </SheetHeader>

            <div className="mt-4 flex items-center justify-between">
              <span className="text-[11px] text-muted-foreground">Agent 可调用</span>
              <Switch
                checked={server.enabled}
                onCheckedChange={(next) => onToggleEnabled(server, next)}
                aria-label="启用"
              />
            </div>

            {server.status === 'error' && server.errorMessage && (
              <div className="mt-4">
                <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                  最近错误
                </div>
                <pre className="mt-1 whitespace-pre-wrap break-words rounded-md bg-destructive/10 p-2 text-[11px] text-destructive">
                  {server.errorMessage}
                </pre>
              </div>
            )}

            <div className="mt-4">
              <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                工具（{toolNames.length}）
              </div>
              <div className="mt-1.5 flex flex-col gap-1">
                {toolNames.length === 0 ? (
                  <div className="text-[11px] text-muted-foreground">
                    {server.status === 'connected' ? '此 server 未暴露工具' : '连接后显示工具'}
                  </div>
                ) : (
                  toolNames.map((t) => (
                    <div key={t} className="rounded bg-muted px-2 py-1 text-[11px] text-foreground">
                      {t}
                    </div>
                  ))
                )}
              </div>
            </div>

            <div className="mt-5 flex gap-2">
              <Button size="sm" variant="outline" className="flex-1" onClick={() => onEdit(server)}>
                编辑
              </Button>
              <Button size="sm" variant="outline" className="flex-1" onClick={() => onRestart(server)}>
                重启
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="flex-1 text-destructive hover:text-destructive"
                onClick={() => onRemove(server)}
              >
                移除
              </Button>
            </div>
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}
```

- [ ] **Step 5: 实现 `McpTemplateLibrary.tsx`(模板库 + 常量)**

```tsx
/**
 * McpTemplateLibrary — 「+ 添加集成」弹出的模板库。
 *
 * 4 个模板(GitHub / Notion / Slack / Custom)。选中 → 关闭本弹层、把预填
 * McpServerInput 交给父级打开 McpEditorModal。模板定义是纯前端常量。
 */
import * as React from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { McpServerInput } from '@/lib/types'

export interface McpTemplate {
  key: string
  label: string
  description: string
  prefill: McpServerInput
}

export const MCP_TEMPLATES: McpTemplate[] = [
  {
    key: 'github',
    label: 'GitHub',
    description: '仓库 / PR / issue 操作',
    prefill: {
      name: 'github',
      description: 'GitHub 仓库 / PR / issue 操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-github'],
      env: { GITHUB_TOKEN: '' },
    },
  },
  {
    key: 'notion',
    label: 'Notion',
    description: 'Notion 页面 / 数据库',
    prefill: {
      name: 'notion',
      description: 'Notion 页面 / 数据库操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-notion'],
      env: { NOTION_API_KEY: '' },
    },
  },
  {
    key: 'slack',
    label: 'Slack',
    description: '消息 / 频道',
    prefill: {
      name: 'slack',
      description: 'Slack 消息 / 频道操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-slack'],
      env: { SLACK_BOT_TOKEN: '' },
    },
  },
  {
    key: 'custom',
    label: 'Custom',
    description: '从空白表单开始',
    prefill: {
      name: '',
      description: '',
      transportType: 'stdio',
      command: '',
      args: [],
      env: {},
    },
  },
]

export interface McpTemplateLibraryProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onPick: (prefill: McpServerInput) => void
}

export function McpTemplateLibrary({ open, onOpenChange, onPick }: McpTemplateLibraryProps): React.ReactElement {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-popover">
        <DialogHeader>
          <DialogTitle>从模板新建</DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-2">
          {MCP_TEMPLATES.map((tpl) => (
            <button
              key={tpl.key}
              type="button"
              onClick={() => onPick(tpl.prefill)}
              className="rounded-lg border border-border bg-card p-3 text-left transition-colors hover:bg-muted/40"
            >
              <div className="text-[13px] font-semibold text-foreground">{tpl.label}</div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">{tpl.description}</div>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 6: 实现 `McpEditorModal.tsx`(可视化编辑器,Dialog)**

```tsx
/**
 * McpEditorModal — MCP server 可视化编辑器(居中模态框,变体 A)。
 *
 * 表单字段对齐后端 McpServerConfig:名称 / 描述 / 传输方式(stdio↔http)/
 * stdio 字段(命令 / 参数 chips / 环境变量 key-value)或 http 字段(URL)/
 * 自动批准开关。「测试连接并保存」= 先 add/update 落库,再 connect 读状态。
 */
import * as React from 'react'
import { toast } from 'sonner'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { addMcpServer, updateMcpServer, connectMcpServer } from '@/lib/tauri-bridge'
import type { McpServerInfo, McpServerInput, McpTransportType } from '@/lib/types'

/** 编辑器模式:新建(带预填)或编辑现有 server。 */
export type McpEditorTarget =
  | { mode: 'add'; prefill: McpServerInput }
  | { mode: 'edit'; server: McpServerInfo }

export interface McpEditorModalProps {
  target: McpEditorTarget | null
  onOpenChange: (open: boolean) => void
  /** 保存 + 连接成功后回调,父级用来刷新列表。 */
  onSaved: () => void
}

interface FormState {
  name: string
  description: string
  transportType: McpTransportType
  command: string
  args: string[]
  env: Array<{ key: string; value: string }>
  url: string
  autoApprove: boolean
}

function emptyForm(): FormState {
  return { name: '', description: '', transportType: 'stdio', command: '', args: [], env: [], url: '', autoApprove: false }
}

function fromInput(input: McpServerInput): FormState {
  return {
    name: input.name,
    description: input.description,
    transportType: input.transportType ?? 'stdio',
    command: input.command,
    args: input.args ?? [],
    env: Object.entries(input.env ?? {}).map(([key, value]) => ({ key, value })),
    url: input.url ?? '',
    autoApprove: input.autoApprove ?? false,
  }
}

function fromServer(server: McpServerInfo): FormState {
  return {
    name: server.name,
    description: server.description,
    transportType: server.transportType,
    command: server.command,
    args: server.args,
    env: Object.entries(server.env ?? {}).map(([key, value]) => ({ key, value })),
    url: server.url ?? '',
    autoApprove: server.autoApprove,
  }
}

function toInput(form: FormState): McpServerInput {
  return {
    name: form.name.trim(),
    description: form.description.trim(),
    transportType: form.transportType,
    command: form.command.trim(),
    args: form.args,
    env: Object.fromEntries(form.env.filter((e) => e.key.trim()).map((e) => [e.key.trim(), e.value])),
    url: form.transportType === 'http' ? form.url.trim() : null,
    autoApprove: form.autoApprove,
  }
}

export function McpEditorModal({ target, onOpenChange, onSaved }: McpEditorModalProps): React.ReactElement {
  const [form, setForm] = React.useState<FormState>(emptyForm)
  const [submitting, setSubmitting] = React.useState(false)
  const [connectError, setConnectError] = React.useState<string | null>(null)
  const [newArg, setNewArg] = React.useState('')

  // 打开/切换 target 时重置表单。
  React.useEffect(() => {
    if (!target) return
    setForm(target.mode === 'add' ? fromInput(target.prefill) : fromServer(target.server))
    setConnectError(null)
    setNewArg('')
  }, [target])

  const valid =
    form.name.trim().length > 0 &&
    (form.transportType === 'stdio' ? form.command.trim().length > 0 : form.url.trim().length > 0)

  const handleSave = async () => {
    if (!target || !valid) return
    setSubmitting(true)
    setConnectError(null)
    const input = toInput(form)
    try {
      const saved =
        target.mode === 'add'
          ? await addMcpServer(input)
          : await updateMcpServer(target.server.id, input)
      try {
        await connectMcpServer(saved.id)
        toast.success(`「${saved.name}」已连接`)
        onSaved()
        onOpenChange(false)
      } catch (connErr) {
        // 已落库但连不上 —— modal 不关,内联显示错误。
        setConnectError(String(connErr))
        onSaved()
      }
    } catch (saveErr) {
      toast.error('保存失败', { description: String(saveErr) })
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onOpenChange(false)}>
      <DialogContent className="bg-popover max-w-[440px]">
        <DialogHeader>
          <DialogTitle>{target?.mode === 'edit' ? `编辑集成 · ${target.server.name}` : '新建集成'}</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <Field label="名称">
            <Input
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="github"
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="描述">
            <Input
              value={form.description}
              onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
              placeholder="服务器用途说明"
              className="h-8 text-[12px]"
            />
          </Field>

          <Field label="传输方式">
            <div className="flex gap-1.5">
              {(['stdio', 'http'] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => setForm((f) => ({ ...f, transportType: t }))}
                  className={cn(
                    'flex-1 rounded-md border px-2 py-1 text-[11.5px] transition-colors',
                    form.transportType === t
                      ? 'border-accent/35 bg-accent/15 text-accent-foreground'
                      : 'border-border text-muted-foreground hover:bg-muted/40',
                  )}
                >
                  {t === 'stdio' ? 'stdio（子进程）' : 'http'}
                </button>
              ))}
            </div>
          </Field>

          {form.transportType === 'stdio' ? (
            <>
              <Field label="命令">
                <Input
                  value={form.command}
                  onChange={(e) => setForm((f) => ({ ...f, command: e.target.value }))}
                  placeholder="npx"
                  className="h-8 font-mono text-[11.5px]"
                />
              </Field>
              <Field label="参数">
                <div className="flex flex-wrap gap-1.5">
                  {form.args.map((arg, i) => (
                    <span
                      key={`${arg}-${i}`}
                      className="flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]"
                    >
                      {arg}
                      <button
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, args: f.args.filter((_, j) => j !== i) }))}
                        className="text-muted-foreground hover:text-destructive"
                        aria-label={`移除参数 ${arg}`}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <Input
                    value={newArg}
                    onChange={(e) => setNewArg(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && newArg.trim()) {
                        e.preventDefault()
                        setForm((f) => ({ ...f, args: [...f.args, newArg.trim()] }))
                        setNewArg('')
                      }
                    }}
                    placeholder="+ 参数后回车"
                    className="h-6 w-32 font-mono text-[10px]"
                  />
                </div>
              </Field>
              <Field label="环境变量">
                <div className="space-y-1">
                  {form.env.map((row, i) => (
                    <div key={i} className="flex gap-1.5">
                      <Input
                        value={row.key}
                        onChange={(e) =>
                          setForm((f) => ({
                            ...f,
                            env: f.env.map((r, j) => (j === i ? { ...r, key: e.target.value } : r)),
                          }))
                        }
                        placeholder="KEY"
                        className="h-7 flex-[0_0_130px] font-mono text-[10px]"
                      />
                      <Input
                        value={row.value}
                        onChange={(e) =>
                          setForm((f) => ({
                            ...f,
                            env: f.env.map((r, j) => (j === i ? { ...r, value: e.target.value } : r)),
                          }))
                        }
                        placeholder="value"
                        className="h-7 flex-1 font-mono text-[10px]"
                      />
                      <button
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, env: f.env.filter((_, j) => j !== i) }))}
                        className="text-[11px] text-muted-foreground hover:text-destructive"
                        aria-label="移除环境变量"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() => setForm((f) => ({ ...f, env: [...f.env, { key: '', value: '' }] }))}
                    className="w-full rounded border border-dashed border-border py-1 text-[10px] text-muted-foreground hover:bg-muted/40"
                  >
                    + 添加环境变量
                  </button>
                </div>
              </Field>
            </>
          ) : (
            <Field label="URL">
              <Input
                value={form.url}
                onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
                placeholder="https://example.com/mcp"
                className="h-8 font-mono text-[11.5px]"
              />
            </Field>
          )}

          <div className="flex items-center justify-between rounded-md bg-muted/40 px-3 py-2">
            <div>
              <div className="text-[11.5px] font-medium text-foreground">自动批准工具调用</div>
              <div className="text-[10px] text-muted-foreground">Agent 调用此 server 的工具时不再逐次确认</div>
            </div>
            <Switch
              checked={form.autoApprove}
              onCheckedChange={(next) => setForm((f) => ({ ...f, autoApprove: next }))}
              aria-label="自动批准工具调用"
            />
          </div>

          {connectError && (
            <pre className="whitespace-pre-wrap break-words rounded-md bg-destructive/10 p-2 text-[11px] text-destructive">
              已保存，但连接失败：{connectError}
            </pre>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={submitting}>
            取消
          </Button>
          <Button onClick={() => void handleSave()} disabled={!valid || submitting}>
            {submitting ? '保存中…' : '测试连接并保存'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div>
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      {children}
    </div>
  )
}
```

- [ ] **Step 7: 实现 `IntegrationsModule.tsx`(容器)**

```tsx
/**
 * IntegrationsModule — 万花筒「集成」模块。
 *
 * MCP server 富卡片网格 + 详情抽屉(Sheet)+ 可视化编辑器(Dialog)+ 模板库。
 * 数据:listMcpServers + listMcpTools(后者按 serverId 分组)。
 */
import * as React from 'react'
import { toast } from 'sonner'
import {
  listMcpServers,
  listMcpTools,
  toggleMcpServer,
  restartMcpServer,
  removeMcpServer,
} from '@/lib/tauri-bridge'
import type { McpServerInfo, McpServerInput } from '@/lib/types'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { Button } from '@/components/ui/button'
import { McpServerCard } from './McpServerCard'
import { McpDetailDrawer } from './McpDetailDrawer'
import { McpTemplateLibrary } from './McpTemplateLibrary'
import { McpEditorModal, type McpEditorTarget } from './McpEditorModal'

interface McpToolRow {
  serverId: string
  name: string
}

export function IntegrationsModule(): React.ReactElement {
  const [servers, setServers] = React.useState<McpServerInfo[]>([])
  const [tools, setTools] = React.useState<McpToolRow[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [drawerOpen, setDrawerOpen] = React.useState(false)
  const [templateOpen, setTemplateOpen] = React.useState(false)
  const [editorTarget, setEditorTarget] = React.useState<McpEditorTarget | null>(null)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    const [s, t] = await Promise.allSettled([listMcpServers(), listMcpTools()])
    if (s.status === 'fulfilled') setServers(s.value)
    else toast.error('加载 MCP server 失败', { description: String(s.reason) })
    if (t.status === 'fulfilled') {
      setTools(
        (t.value as Array<{ serverId?: string; name?: string }>).map((row) => ({
          serverId: row.serverId ?? '',
          name: row.name ?? '',
        })),
      )
    }
    setLoading(false)
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  const toolsByServer = React.useMemo(() => {
    const map = new Map<string, string[]>()
    for (const row of tools) {
      const arr = map.get(row.serverId) ?? []
      arr.push(row.name)
      map.set(row.serverId, arr)
    }
    return map
  }, [tools])

  const selected = servers.find((s) => s.id === selectedId) ?? null
  const connectedCount = servers.filter((s) => s.status === 'connected').length

  const openCard = (server: McpServerInfo) => {
    setSelectedId(server.id)
    setDrawerOpen(true)
  }

  const onToggleEnabled = async (server: McpServerInfo, next: boolean) => {
    setServers((prev) => prev.map((s) => (s.id === server.id ? { ...s, enabled: next } : s)))
    try {
      await toggleMcpServer(server.id, next)
    } catch (err) {
      toast.error('切换状态失败', { description: String(err) })
      setServers((prev) => prev.map((s) => (s.id === server.id ? { ...s, enabled: !next } : s)))
    }
  }

  const onRestart = async (server: McpServerInfo) => {
    try {
      await restartMcpServer(server.id)
      toast.success(`已重启「${server.name}」`)
      await refetch()
    } catch (err) {
      toast.error('重启失败', { description: String(err) })
    }
  }

  const onRemove = async (server: McpServerInfo) => {
    try {
      await removeMcpServer(server.id)
      toast.success(`已移除「${server.name}」`)
      setDrawerOpen(false)
      setSelectedId(null)
      await refetch()
    } catch (err) {
      toast.error('移除失败', { description: String(err) })
    }
  }

  const onPickTemplate = (prefill: McpServerInput) => {
    setTemplateOpen(false)
    setEditorTarget({ mode: 'add', prefill })
  }

  const onEdit = (server: McpServerInfo) => {
    setDrawerOpen(false)
    setEditorTarget({ mode: 'edit', server })
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader
        group="capability"
        title="集成 · MCP"
        subtitle={
          loading
            ? '加载中…'
            : `${servers.length} 个 MCP server · ${connectedCount} 个已连接`
        }
        actions={
          <Button size="sm" onClick={() => setTemplateOpen(true)}>
            + 添加集成
          </Button>
        }
      />

      <div className="flex-1 min-h-0 overflow-y-auto px-8 pb-8">
        {!loading && servers.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="rounded-lg border border-dashed border-border bg-muted/10 px-8 py-10 text-center">
              <div className="text-[13px] text-foreground/80">还没有集成</div>
              <div className="mt-1 text-[11.5px] text-muted-foreground">
                点「+ 添加集成」，让 Agent 接入 Slack / GitHub / Notion。
              </div>
            </div>
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-3">
            {servers.map((server) => (
              <McpServerCard
                key={server.id}
                server={server}
                toolNames={toolsByServer.get(server.id) ?? []}
                selected={server.id === selectedId && drawerOpen}
                onClick={() => openCard(server)}
              />
            ))}
            <button
              type="button"
              onClick={() => setTemplateOpen(true)}
              className="flex min-h-[88px] items-center justify-center rounded-xl border border-dashed border-border text-[12px] text-muted-foreground hover:bg-muted/40"
            >
              + 从模板添加
            </button>
          </div>
        )}
      </div>

      <McpDetailDrawer
        server={selected}
        toolNames={selected ? toolsByServer.get(selected.id) ?? [] : []}
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
        onRestart={(s) => void onRestart(s)}
        onRemove={(s) => void onRemove(s)}
        onEdit={onEdit}
      />

      <McpTemplateLibrary
        open={templateOpen}
        onOpenChange={setTemplateOpen}
        onPick={onPickTemplate}
      />

      <McpEditorModal
        target={editorTarget}
        onOpenChange={(open) => !open && setEditorTarget(null)}
        onSaved={() => void refetch()}
      />
    </div>
  )
}
```

- [ ] **Step 8: 运行确认通过**

Run: `cd ui && npm test -- --run IntegrationsModule 2>&1 | tail -12`
Expected: PASS(4 tests)。

若 `Sheet` / `Dialog` 在 jsdom 下报 portal/`ResizeObserver` 相关 warning:无妨,只要断言通过。若 `screen.getByText('从模板新建')` 找不到 —— 确认 `McpTemplateLibrary` 的 `DialogTitle` 文案是「从模板新建」。

- [ ] **Step 9: 路由接线 `KaleidoscopeShell.tsx`**

加 import(在 `SkillsModule` import 之后):

```tsx
import { IntegrationsModule } from './modules/Integrations/IntegrationsModule'
```

路由三元里,在 `skills` 分支之后加 `integrations` 分支:

```tsx
              ) : moduleId === 'skills' ? (
                <SkillsModule />
              ) : moduleId === 'integrations' ? (
                <IntegrationsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
```

- [ ] **Step 10: 更新 `KaleidoscopeShell.test.tsx`**

把 Task 4 改过的 `vi.mock('@/lib/tauri-bridge', ...)` 再补上 MCP 函数 —— 在该 mock 工厂的返回对象里追加:

```tsx
  listMcpServers: vi.fn().mockResolvedValue([]),
  listMcpTools: vi.fn().mockResolvedValue([]),
  addMcpServer: vi.fn().mockResolvedValue({}),
  updateMcpServer: vi.fn().mockResolvedValue({}),
  removeMcpServer: vi.fn().mockResolvedValue(true),
  toggleMcpServer: vi.fn().mockResolvedValue(true),
  restartMcpServer: vi.fn().mockResolvedValue(true),
  connectMcpServer: vi.fn().mockResolvedValue(true),
```

在 `it('renders SkillsModule ...')` 之后插入:

```tsx
  it('renders IntegrationsModule for the integrations module', async () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'integrations')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(await screen.findByText('集成 · MCP')).toBeInTheDocument()
  })
```

- [ ] **Step 11: 跑测试 + TS 检查**

Run: `cd ui && npm test -- --run KaleidoscopeShell IntegrationsModule 2>&1 | tail -16`
Expected: 全 PASS(KaleidoscopeShell 8 tests + IntegrationsModule 4 tests)。

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "Integrations|Kaleidoscope" | head`
Expected: 无输出。

- [ ] **Step 12: Commit**

```bash
git add ui/src/views/Kaleidoscope/modules/Integrations/ ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx
git commit -m "$(cat <<'EOF'
feat(kaleidoscope): Integrations module — MCP card grid + visual editor

集成模块:MCP server 富卡片网格(变体 B,工具名 chips 预览)+ 详情抽屉
(Sheet)+ 模板库 + 可视化编辑器(Dialog 居中模态框,变体 A)。编辑器
表单对齐后端 McpServerConfig,「测试连接并保存」先 add/update 落库再
connect。KaleidoscopeShell 加 integrations 分支。
EOF
)"
```

---

## Task 6: 语义拆分清理 — Settings 瘦身

**Files:**
- Modify: `ui/src/components/settings/ToolSettings.tsx`
- Modify: `ui/src/components/settings/ToolsTab.tsx`
- Delete: `ui/src/components/settings/SkillsSettings.tsx`
- Delete: `ui/src/components/settings/McpServerForm.tsx`

技能 / 集成已搬进万花筒,Settings 里的对应区块退役。`ToolSettings.tsx` 只留 workspace skill tags + active manifest 调试。`SkillsSettings.tsx` / `McpServerForm.tsx` 整体删除(`SkillEvolutionTab.tsx` / `SkillConsolidationDialog.tsx` 保留 —— 被技能模块复用)。

- [ ] **Step 1: 重写 `ToolSettings.tsx` —— 只留 tags + manifest 调试**

把 `ui/src/components/settings/ToolSettings.tsx` 整个文件替换为:

```tsx
import { useState, useEffect, useCallback } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { listActiveManifestSkills } from '@/lib/tauri-bridge'
import type { ActiveManifestSkill } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { WorkspaceSkillTagsEditor } from './WorkspaceSkillTagsEditor'
import { toast } from 'sonner'
import { RefreshCw } from 'lucide-react'

type ProvenanceKey = ActiveManifestSkill['provenance']

const PROVENANCE_BADGE: Record<ProvenanceKey, { label: string; className: string }> = {
  bundled: { label: 'Bundled', className: 'bg-primary/10 text-primary border-primary/20' },
  user:    { label: 'User',    className: 'bg-emerald-500/10 text-emerald-600 border-emerald-500/20 dark:text-emerald-400' },
  project: { label: 'Project', className: 'bg-muted text-muted-foreground border-border' },
  learned: { label: 'Learned', className: 'bg-amber-500/10 text-amber-600 border-amber-500/20 dark:text-amber-400' },
}

export function ToolSettings() {
  const [activeManifest, setActiveManifest] = useState<ActiveManifestSkill[] | null>(null)
  const [manifestLoading, setManifestLoading] = useState(false)

  const refreshActiveManifest = useCallback(async () => {
    setManifestLoading(true)
    try {
      const rows = await listActiveManifestSkills()
      setActiveManifest(rows)
    } catch (e) {
      toast.error('加载活动技能清单失败', { description: String(e) })
    } finally {
      setManifestLoading(false)
    }
  }, [])

  useEffect(() => {
    refreshActiveManifest()
  }, [refreshActiveManifest])

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">工具设置</h2>

      <div className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5 text-[12px] text-muted-foreground">
        技能与集成（MCP）的完整管理已移至 <span className="text-foreground font-medium">万花筒 → 技能 / 集成</span>。
      </div>

      <SettingsSection
        title="工作区 Skill 标签 (V19+)"
        description="按标签过滤当前工作区可用的 Skill — 留空 = 默认全部可见；填写后只有匹配标签的 Skill 进入 manifest。未打标的 Skill 默认视为全局（始终可见），保护新抽取学得技能的冷启动。"
      >
        <WorkspaceSkillTagsEditor />
      </SettingsSection>

      <SettingsSection
        title="活动技能（调试）"
        description="此刻**会被注入到 Agent system prompt** 的技能清单 — 顺序与 Agent 看到的完全一致。用于排查「为什么这条 skill 没被召回」之类的问题。包含 builtin (Bundled/User/Project) + Learned (已 promoted)。如果配置了上方的工作区标签，此处显示的是过滤后的结果。"
      >
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <div className="text-xs text-muted-foreground">
              {activeManifest == null
                ? '加载中…'
                : activeManifest.length === 0
                ? '当前 manifest 为空 — 没有 enabled 的 builtin 技能且没有 promoted 的 learned 技能'
                : `共 ${activeManifest.length} 条按 E3 排序`}
            </div>
            <Button
              size="sm"
              variant="ghost"
              disabled={manifestLoading}
              onClick={refreshActiveManifest}
            >
              <RefreshCw className={`size-3.5 mr-1 ${manifestLoading ? 'animate-spin' : ''}`} />
              刷新
            </Button>
          </div>
          {activeManifest && activeManifest.length > 0 && (
            <div className="space-y-1">
              {activeManifest.map((row) => {
                const badge = PROVENANCE_BADGE[row.provenance]
                return (
                  <div
                    key={`${row.rank}-${row.name}`}
                    className="flex items-start gap-2 px-2 py-1.5 rounded border border-border/40 bg-muted/30 hover:bg-muted/50 transition-colors"
                  >
                    <span className="text-[10px] text-muted-foreground/60 tabular-nums w-5 flex-shrink-0 text-right pt-0.5">
                      {row.rank}.
                    </span>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="text-xs font-medium truncate">{row.name}</span>
                        <span className={`text-[9px] px-1 py-px rounded border ${badge.className}`}>
                          {badge.label}
                        </span>
                        {row.provenance === 'learned' && row.citedCount > 0 && (
                          <span className="text-[9px] text-muted-foreground/60">
                            ✓ {row.citedCount}
                          </span>
                        )}
                      </div>
                      {row.summary && (
                        <div className="text-[11px] text-muted-foreground mt-0.5 line-clamp-1">
                          {row.summary}
                        </div>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </SettingsSection>
    </div>
  )
}
```

- [ ] **Step 2: 改 `ToolsTab.tsx` —— 删 SkillsSettings 段**

把 `ui/src/components/settings/ToolsTab.tsx` 整个文件替换为:

```tsx
/**
 * ToolsTab — composes ToolSettings + PermissionsSettings.
 *
 * 学得技能与 MCP 已迁至万花筒（技能 / 集成模块）。本 tab 只剩工作区
 * skill 标签、活动技能调试面板、工具权限。
 */
import * as React from 'react'
import { ToolSettings } from './ToolSettings'
import { PermissionsSettings } from './PermissionsSettings'

export function ToolsTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="工具与 MCP">
        <ToolSettings />
      </section>
      <section data-settings-section="工具权限">
        <PermissionsSettings />
      </section>
    </div>
  )
}
```

- [ ] **Step 3: 删除退役文件**

```bash
git rm ui/src/components/settings/SkillsSettings.tsx ui/src/components/settings/McpServerForm.tsx
```

- [ ] **Step 4: 确认无悬空引用**

Run: `cd ui && grep -rn "SkillsSettings\|McpServerForm" src/ --include="*.tsx" --include="*.ts"`
Expected: 无输出。若有命中:

- `SkillConsolidationDialog.tsx` 里若只是注释提到 SkillsSettings —— 改注释或留着无害(注释不影响编译);若是 import —— 不应该有。
- 其它命中都要清掉。

- [ ] **Step 5: TS 检查 + 全量测试**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15`
Expected: 无输出(0 error)。

Run: `cd ui && npm test -- --run 2>&1 | tail -12`
Expected: 全绿。若有 `SkillsSettings.test.tsx` 之类的遗留测试文件报找不到模块 —— 该测试文件也要 `git rm`(先 `ls ui/src/components/settings/*.test.tsx` 确认是否存在 `SkillsSettings.test.tsx` / `ToolSettings.test.tsx` / `McpServerForm.test.tsx`,存在且引用已删组件就一并删除或更新)。

- [ ] **Step 6: 后端编译确认(无回归)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出。

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/settings/ToolSettings.tsx ui/src/components/settings/ToolsTab.tsx
git commit -m "$(cat <<'EOF'
refactor(settings): semantic split — 技能/MCP 迁出 Settings

技能与集成已搬进万花筒,Settings 对应区块退役:ToolSettings 只留
workspace skill tags + active manifest 调试 + 一张跳转提示卡;
ToolsTab 删 SkillsSettings 段。删除 SkillsSettings.tsx / McpServerForm.tsx
(SkillEvolutionTab / SkillConsolidationDialog 保留 —— 技能模块复用)。
EOF
)"
```

---

## Self-Review

**1. Spec coverage:**

| Spec 章节 | 对应 Task |
|---|---|
| §3 文件组织 | Task 1-6 的 File Structure |
| §4 记忆模块 | Task 3 |
| §5 技能模块(双栏 / 可折叠分组 / 数据 merge / 详情 / SkillEvolutionTab) | Task 4 |
| §5.5 语义拆分 ToolSettings | Task 6 |
| §6 集成模块(富卡片 / 抽屉 / 模板库 / 编辑器) | Task 5 |
| §7 后端改动(transport / update / McpServerInfo 三字段) | Task 1 |
| §7.5 前端 bridge + 类型 | Task 2 |
| §7.6 后端测试 | Task 1 Step 2-3 / Step 10 |
| §8 路由接线 | Task 3/4/5 各自 Step |
| §10 PR 形态(6 commits) | Task 1-6 各一 commit |
| §12 验收清单 | 见下方 Final Verification |

覆盖完整,无遗漏 spec 要求。

**2. Placeholder scan:** 无 TBD / TODO / "类似 Task N" / 无代码的步骤。每个改代码的步骤都给了完整代码。

**3. Type consistency:**
- `UnifiedSkill` 在 `SkillsModule.tsx` 定义并 export,`SkillsList.tsx` / `SkillDetail.tsx` import —— 一致。
- `McpEditorTarget` 在 `McpEditorModal.tsx` 定义并 export,`IntegrationsModule.tsx` import —— 一致。
- `McpServerInput` 字段(`transportType` / `url` / `autoApprove`)在 Task 2 types.ts 定义,Task 5 `toInput()` / `MCP_TEMPLATES` 使用 —— 一致。
- 后端 `McpServerInfo` 序列化字段 camelCase(`transportType` / `errorMessage` / `autoApprove`)对齐 Task 2 TS 类型 —— 一致。
- `connectMcpServer` / `addMcpServer` / `updateMcpServer` bridge 签名:Task 2 定义,Task 5 `McpEditorModal` 使用 —— 一致。

---

## Final Verification(全部 Task 完成后)

- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` —— 无输出
- [ ] `cd src-tauri && cargo test --lib mcp 2>&1 | tail -5` —— mcp 测试全绿
- [ ] `cd ui && npx tsc --noEmit` —— 0 error
- [ ] `cd ui && npm test -- --run 2>&1 | tail -10` —— 全绿
- [ ] 手动:`cargo tauri dev`,进万花筒 → 记忆 / 技能 / 集成三个模块都渲染真实内容,`artifacts` 仍是占位
- [ ] 手动:集成模块「+ 添加集成」→ 模板库 → 选 GitHub → 编辑器预填正确;切 stdio↔http 字段组替换;新建一个 server 能落库
- [ ] 手动:Settings → 工具 tab 不再有 MCP 段 / 内置技能段 / 已学技能段,提示卡文案正确
- [ ] 手动主题走查:warm-paper / qingye / forest-dark / the-finals 四主题过技能 + 集成两模块
