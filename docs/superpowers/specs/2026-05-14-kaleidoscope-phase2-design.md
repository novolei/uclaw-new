# 万花筒 Phase 2 · 能力组三模块设计

> 子 spec —— 父 spec:`docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`(§7.5 / §7.6 / §7.7 / §12 Phase 2)。
> 本文聚焦「能力」组三个模块的实现:**记忆 / 技能 / 集成**,外加集成模块新增的 **MCP 可视化编辑器**(父 spec 之后追加的需求)。

**日期**:2026-05-14
**状态**:已定稿,待写 plan

---

## 1. 背景

资产组(数字人 / 应用商店 / 我的应用)已在 Phase 1 + 1.1 完成迁移。万花筒 Rail 的「能力」组三个模块——记忆 / 技能 / 集成——目前点进去都渲染 `ComingSoonModule` 占位。本期把这三个换成真实实现。

一个结构难题(父 spec §7.5/§7.6 已点出):现有代码里「技能」和「MCP」是混在一起的——

- `ui/src/components/settings/SkillsSettings.tsx`(474 LOC)= **学得技能**
- `ui/src/components/settings/ToolSettings.tsx`(246 LOC)= **MCP servers + 内置技能 + workspace skill tags + active manifest 调试**,四件事混在一个文件

万花筒 Rail 却是「技能」「集成」两个独立模块。本期按**语义拆分**重组:技能相关(学得 + 内置)归技能模块,MCP 相关归集成模块,workspace skill tags + active manifest 调试**留在 Settings**。

### 1.1 与父 spec 的有意偏离

| 父 spec | 本期实际 | 原因 |
|---|---|---|
| §7.7 记忆:wrap + `ModuleHeader` + 类型筛选 + 搜索 + 右抽屉 | 仅 full-bleed wrap,无 `ModuleHeader`、无新增交互 | `MemoryGraphView` 自包含且交互完整;筛选/搜索是它内部的事,套壳不应加层。与 Phase 1.1 迁移模块(Humans/Store/Apps)同款最小 wrap |
| §11.3「本次设计不引入新的 Rust 命令」 | 引入 1 个新 Tauri 命令 + 扩 2 个 IPC 结构体 | MCP 可视化编辑器是父 spec 之后追加的需求,后端当前不支持 transport 选择 / 编辑现有 server |
| §12 Phase 2 含 `EmptyState` / `AssetCard` 共享组件 | 不做这两个抽象 | YAGNI——三个模块的空态/卡片各自内聚就够,过早抽象反而增加耦合 |
| §12 Phase 2 含 StoreModule / AppsModule | 不含——已在 Phase 1.1 完成 | 父 spec 写于 Phase 1.1 之前 |

---

## 2. 目标与非目标

### 目标

- 记忆 / 技能 / 集成三个模块从 `ComingSoonModule` 占位换成真实实现
- 技能模块:左可折叠分组列表(学得 / 内置)+ 右详情双栏
- 集成模块:MCP server 富卡片网格 + 详情抽屉 + 模板库 + **可视化编辑器(居中模态框)**
- `ToolSettings.tsx` 语义拆分:MCP 段 + 内置技能段移出,只留 workspace skill tags + active manifest 调试
- 后端补齐 MCP 编辑器所需:transport 选择、编辑现有 server
- 全程 theme token 纪律(11 主题),无硬编码颜色

### 非目标

- 不动 `MemoryGraphView` 内部一行代码
- 不做 `artifacts`(产出)模块——仍是 `ComingSoonModule`,留给后续
- 不做「+ 自定义技能」的真实功能——按钮 disabled 占位(父 spec §7.5 原意)
- 不做 MCP 连接日志的时间戳流式展示——详情抽屉显示当前状态 + 最近一次错误串即可
- 不引入 DB migration、不新增 LLM provider(CSP 不动)

---

## 3. 文件组织

```
ui/src/views/Kaleidoscope/
├── KaleidoscopeShell.tsx          # 修改:路由加 skills/integrations/memory 三分支
├── modules/
│   ├── Memory/
│   │   └── MemoryModule.tsx       # 新建:full-bleed wrap MemoryGraphView
│   ├── Skills/
│   │   ├── SkillsModule.tsx       # 新建:双栏 shell + 数据 merge
│   │   ├── SkillsList.tsx         # 新建:左栏可折叠分组列表
│   │   └── SkillDetail.tsx        # 新建:右栏详情(复用 SkillsSettings 渲染 + SkillEvolutionTab)
│   └── Integrations/
│       ├── IntegrationsModule.tsx # 新建:卡片网格 shell + 抽屉/模态框状态
│       ├── McpServerCard.tsx      # 新建:富卡片(变体 B)
│       ├── McpDetailDrawer.tsx    # 新建:右侧详情抽屉
│       ├── McpEditorModal.tsx     # 新建:可视化编辑器(居中模态框)
│       └── McpTemplateLibrary.tsx # 新建:模板库选择(GitHub/Notion/Slack/Custom)

ui/src/components/settings/
├── ToolSettings.tsx               # 修改:删 MCP 段 + 内置技能段,只留 tags + manifest 调试
└── SkillsSettings.tsx             # 修改:导出可被 SkillDetail 复用的渲染部分(或整体迁入 Skills/)

src-tauri/src/
├── ipc.rs                         # 修改:McpServerInput / McpServerInfo 扩字段
├── tauri_commands.rs              # 修改:add_mcp_server 读 transport;新增 update_mcp_server
└── main.rs                        # 修改:invoke_handler! 注册 update_mcp_server
```

---

## 4. 模块 1 · 记忆(Memory)

最薄的一个。

### 4.1 实现

```tsx
// ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx
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

- `MemoryGraphView` 根是 `relative w-full h-full min-h-[300px]`,自包含——自己 fetch `memory_graph_get_full_graph`,自带筛选/缩放/节点详情。
- `absolute inset-0` 给它一个确定尺寸的父容器(`KaleidoscopeShell` 主区卡片是 `relative`)。与 `HumansModule` 同款。
- 无 `ModuleHeader`(full-bleed)。

### 4.2 降级

`MemoryGraphView` 自己处理 memU 未连接 / 空图的情况(本期不改它的降级逻辑)。

### 4.3 测试

`MemoryModule.test.tsx`:mock `MemoryGraphView`,断言渲染在 `absolute inset-0` 容器内。

---

## 5. 模块 2 · 技能(Skills)

左分类列表 + 右详情双栏(父 spec §7.5)。左栏组织方式采用**变体 A:可折叠分组**。

### 5.1 布局

```
┌─ SkillsModule (flex h-full) ──────────────────────────────┐
│ ┌─ SkillsList (w-60, border-r) ─┐ ┌─ SkillDetail (flex-1)─┐│
│ │ [搜索框] [+ 自定义(disabled)] │ │  选中技能的详情        ││
│ │ ▾ 学得 · N                    │ │  - 学得:context/      ││
│ │   ⚡ skill-a                   │ │    principles/steps/   ││
│ │   📝 skill-b                   │ │    pitfalls + 演进时间线││
│ │ ▾ 内置 · M                    │ │  - 内置:description/   ││
│ │   🛠 skill-c                   │ │    version/author/     ││
│ │   🧪 skill-d                   │ │    category + Fork     ││
│ │                               │ │  顶部:「Agent 可调用」  ││
│ │                               │ │  开关                  ││
│ └───────────────────────────────┘ └────────────────────────┘│
└────────────────────────────────────────────────────────────┘
```

### 5.2 数据模型

两个来源在 `SkillsModule` 内 merge 成统一列表:

```ts
type UnifiedSkill =
  | { kind: 'learned'; id: string; raw: LearnedSkill }
  | { kind: 'builtin'; id: string; raw: SkillInfo }   // id = SkillInfo.name
```

| 来源 | 列表数据 | 切换启用 | 删除/Fork |
|---|---|---|---|
| 学得 | `listLearnedSkills('default')` → `LearnedSkill[]` | `toggleLearnedSkill(id, enabled)` | `deleteLearnedSkill(id)` |
| 内置 | `listSkills()` → `SkillInfo[]` | `toggleSkill({ name, enabled })` | `forkSkillToUser(name)`(仅 `provenance === 'bundled'`) |

- 两个 fetch 在 `SkillsModule` mount 时并行发起。任一失败 → sonner toast,另一组仍渲染。
- `reloadSkills()` 接到「重新加载」按钮(放在内置分组的 header 行)。
- `proposeSkillConsolidation` / `backfillSkillKeywords` 是 `SkillsSettings.tsx` 现有的高级操作——本期**保留在哪由实现决定**:若 `SkillsSettings.tsx` 整体迁入 `Skills/`,它们跟着走;若只复用渲染片段,这两个操作暂不进万花筒(不是回归,Settings 里本就没有独立入口)。plan 阶段二选一。

### 5.3 左栏 `SkillsList`

- 顶部:搜索框(本地过滤,匹配 `name`)+「+ 自定义」按钮(`disabled`,`title="未来扩展"`)。
- 两个可折叠 `<section>`:「学得 · N」「内置 · M」,各带计数,点 header 折叠/展开(本地 `useState`,不持久化)。
- 列表项:图标 + 名称 + 次要行。次要行——学得显示 `context` 首行截断,内置显示 `category`。(mockup 里的关键词 chips 是示意;`LearnedSkill` 类型当前无 keywords 字段,本期不为此扩后端。)
- 选中项:`bg-accent/15 border-accent/35`(能力组高亮,父 spec §8.1)。

### 5.4 右栏 `SkillDetail`

- 复用 `SkillsSettings.tsx` 现有的卡片渲染逻辑展示学得技能的 context/principles/steps/pitfalls。
- 学得技能额外有「查看演进时间线」——展开渲染现有 `SkillEvolutionTab`(props:`{ skillId }`)。
- 内置技能显示 description/version/author/category/provenance 徽章 + Fork 按钮(仅 bundled)。
- 顶部右侧「Agent 可调用」开关 → 对应来源的 toggle 命令。
- 未选中任何技能 → 空态:「选择左侧一个技能查看详情」。
- 列表为空(两个来源都空)→ 空态卡(父 spec §10):「你的 Agent 还没学到技能 — 让它处理几次任务就会积累」。

### 5.5 语义拆分:`ToolSettings.tsx`

删除 `ToolSettings.tsx` 中:
- MCP Servers 段(约 line 85-113)→ 移到集成模块
- 内置技能段(约 line 183-233)→ 逻辑归技能模块

保留:
- workspace skill tags
- active manifest 调试面板

`ToolSettings.tsx` 顶部加一行提示卡:「技能与集成的完整管理已移至万花筒 → 技能 / 集成」(纯文案,不做跳转按钮——避免 Settings 依赖 `topLevelViewAtom`,保持 Settings 自包含)。

### 5.6 测试

`SkillsModule.test.tsx`:
- 两来源 merge:mock 两个 fetch,断言学得/内置都进列表且计数正确
- 分组折叠:点 header 折叠后该组列表项不可见
- 选中切详情:点列表项,右栏渲染对应详情
- 空态:两 fetch 都返回 `[]` → 渲染空态卡

---

## 6. 模块 3 · 集成(Integrations / MCP)

富卡片网格(变体 B)+ 详情抽屉 + 模板库 + 可视化编辑器(居中模态框,变体 A)。

### 6.1 布局

```
┌─ IntegrationsModule ──────────────────────────────────────┐
│  [ModuleHeader: 能力 · 集成 · MCP]      [+ 添加集成]        │
│  3 个 MCP server · 2 个已连接 · 共 21 个工具                │
│  ┌──────────┐ ┌──────────┐                                │
│  │McpServer │ │McpServer │   ← 富卡片网格 (grid 2 列)       │
│  │  Card    │ │  Card    │                                │
│  └──────────┘ └──────────┘                                │
│  ┌──────────┐ ┌ + 从模板 ┐                                │
│  │   ...    │ │   添加   ┘ (dashed)                        │
│  └──────────┘                                             │
└────────────────────────────────────────────────────────────┘
   点卡片 → McpDetailDrawer 从右侧滑入
   + 添加 / + 从模板 → McpTemplateLibrary → McpEditorModal(居中)
   抽屉内「编辑」→ McpEditorModal(居中,预填)
```

### 6.2 富卡片 `McpServerCard`(变体 B)

每张卡 = 一个 MCP server:
- icon + 名称 + 状态点(绿=connected / 红=error / 灰=disconnected/connecting)
- 「`transport` · N 个工具」一行
- 工具名 chips 预览(取前 2-3 个 + 「+N」)
- `error` 状态时副标题变红:「连接失败 · 点击查看日志」
- 选中态:`bg-accent/15 border-accent/35`

工具数 / 工具名:`listMcpTools()` 返回全部工具(带 `serverId`),在 Module 内按 server 分组。

### 6.3 详情抽屉 `McpDetailDrawer`

从右侧滑入(父 spec §5.5 抽屉规范):
- 头部:icon + 名称 + 状态 + transport
- 工具列表(该 server 的全部工具名)
- 状态区:`connected` 显示「已连接」;`error` 显示最近一次错误串(来自 `list_mcp_servers` 新增的 `errorMessage` 字段,见 §7)。**不做时间戳日志流。**
- 操作:重启(`restartMcpServer`)/ 移除(`removeMcpServer`,二次确认)/ **编辑**(打开 `McpEditorModal` 预填)
- 启用开关:`toggleMcpServer(id, enabled)`

### 6.4 模板库 `McpTemplateLibrary`

「+ 添加集成」/「+ 从模板添加」触发。一个小弹层,4 个模板:
- **GitHub** → 预填 `command: "npx"`, `args: ["-y", "@modelcontextprotocol/server-github"]`, `env: { GITHUB_TOKEN: "" }`
- **Notion** → 预填对应 npx 包
- **Slack** → 预填对应 npx 包
- **Custom** → 全空表单

选中模板 → 关闭模板库,打开 `McpEditorModal` 并预填。模板定义是前端常量表(`MCP_TEMPLATES`),不需要后端。

### 6.5 可视化编辑器 `McpEditorModal`(变体 A · 居中模态框)

居中 modal,背景压暗。表单字段(对齐后端 `McpServerConfig`):

| 字段 | 控件 | 适用 transport |
|---|---|---|
| 名称 | text input | 全部 |
| 描述 | text input | 全部 |
| 传输方式 | 二选一切换(stdio / http) | 全部 |
| 命令 | text input(monospace) | stdio |
| 参数 | chips 编辑器(可增删的字符串数组) | stdio |
| 环境变量 | key-value 行编辑器(可增删) | stdio |
| URL | text input | http |
| 自动批准工具调用 | toggle | 全部 |

- 切换 transport 时,stdio 字段组 ↔ http 字段组互相替换显示(未填的值保留在 state,不丢)。
- 底部:「测试连接并保存」+「取消」。
- **「测试连接并保存」流程**:
  1. 新建走 `addMcpServer(input)`;编辑走 `updateMcpServer(id, input)`(新命令,见 §7)
  2. 落库成功后调 `connectMcpServer(id)`
  3. `connect` 成功 → 关闭 modal,toast「已连接」,刷新列表
  4. `connect` 失败 → modal 不关,内联显示错误串,server 已落库(用户可改了再试,或直接关掉——它会以 disconnected 留在列表)
- 校验:名称非空;stdio 时命令非空;http 时 URL 非空。前端校验,不满足时「保存」disabled。

### 6.6 空态 / 错误

- server 列表为空 → 空态卡(父 spec §10):「没有集成 — 点 + 添加,让 Agent 接入 Slack/GitHub/Notion」
- 任一 Tauri 命令失败 → sonner toast,UI 不崩

### 6.7 测试

`IntegrationsModule.test.tsx`:
- 卡片渲染:mock `listMcpServers` + `listMcpTools`,断言每 server 一张卡、状态点颜色、工具 chips
- 抽屉开合:点卡片 → 抽屉出现;点关闭 → 消失
- 编辑器表单:打开 modal,切 transport → stdio 字段组 ↔ http 字段组替换;切回不丢已填值
- 校验:stdio 命令为空时「保存」disabled
- 模板预填:选 GitHub 模板 → modal 命令/参数已预填

---

## 7. 后端改动(只为 MCP 编辑器)

当前后端缺口与改动:

### 7.1 `add_mcp_server` 硬编码 transport

`tauri_commands.rs::add_mcp_server`(约 line 2079)当前硬编码 `transport_type: Stdio`、`auto_approve: false`、`url: None`。改成读 `input`:

```rust
transport_type: input.transport_type.unwrap_or_default(),  // 默认 Stdio
command: input.command,
args: input.args.unwrap_or_default(),
env: input.env.unwrap_or_default(),
url: input.url,
auto_approve: input.auto_approve.unwrap_or(false),
```

### 7.2 `McpServerInput` 扩字段(`ipc.rs`)

```rust
pub struct McpServerInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub transport_type: Option<TransportType>,  // 新增
    pub url: Option<String>,                    // 新增
    pub auto_approve: Option<bool>,             // 新增
}
```

全部 `Option` —— 向后兼容,现有调用方不传也能编译/运行。

### 7.3 `McpServerInfo` 扩字段(`ipc.rs`)

```rust
pub struct McpServerInfo {
    // ...现有字段...
    pub transport_type: TransportType,    // 新增:前端卡片显示 transport、编辑器回填
    pub url: Option<String>,              // 新增:http server 编辑器回填
    pub error_message: Option<String>,    // 新增:error 状态时详情抽屉显示
}
```

`list_mcp_servers` 当前已从 `all_server_statuses()` 拿到 `(status, err)` 但把 `err` 丢了(`let (status_enum, _err)`)——改成填进 `error_message`。`transport_type` / `url` 从 `McpServerConfig` 直接取。同步更新 `add_mcp_server` 返回的 `McpServerInfo` 构造。

### 7.4 新增 `update_mcp_server` Tauri 命令

`McpManager::update_server(id, config)` 已存在(`mcp.rs` line 870)——只缺 Tauri 命令包装:

```rust
#[tauri::command]
pub async fn update_mcp_server(
    state: State<'_, AppState>,
    id: String,
    input: McpServerInput,
) -> Result<McpServerInfo, Error> {
    let config = McpServerConfig { id: id.clone(), /* 从 input 构造,同 add */ };
    let mut mgr = state.mcp_manager.write().await;
    mgr.update_server(&id, config.clone()).map_err(Error::InvalidInput)?;
    Ok(/* McpServerInfo from config, status "disconnected" */)
}
```

在 `main.rs` 的 `invoke_handler!` 注册(MCP 命令块,约 line 330 后)。

### 7.5 前端 bridge + 类型

- `ui/src/lib/types.ts`:`McpServerInfo` / `McpServerInput` 补对应字段;新增 `type McpTransportType = 'stdio' | 'http'`
- `ui/src/lib/tauri-bridge.ts`:新增 `updateMcpServer(id, input)`

### 7.6 后端测试

`src-tauri/src/mcp.rs` 的 `#[cfg(test)]` 块新增:
- `add_server` 尊重 `transport_type` —— 传 `Http` + `url`,断言落库的 config transport 是 Http
- `update_server` round-trip —— add 一个 stdio server,update 成 http,断言字段已变、`mcp_servers.json` 已重写

---

## 8. 路由接线(`KaleidoscopeShell`)

`KaleidoscopeShell.tsx` 的模块路由扩三分支:

```tsx
{moduleId === 'humans' ? <HumansModule />
 : moduleId === 'store' ? <StoreModule />
 : moduleId === 'apps' ? <AppsModule />
 : moduleId === 'memory' ? <MemoryModule />
 : moduleId === 'skills' ? <SkillsModule />
 : moduleId === 'integrations' ? <IntegrationsModule />
 : <ComingSoonModule moduleId={moduleId} />}
```

Phase 2 完成后 `ComingSoonModule` 只剩 `artifacts` 命中。

---

## 9. 主题适配

父 spec §8.1 硬约束全部适用:

- 能力组高亮统一用 `bg-accent/15 border-accent/35`(技能选中项、集成选中卡)
- 抽屉 / 模态框背景 `bg-popover`,边框 `border-border`
- 状态点颜色:`connected` 用 `bg-emerald-500` 这类语义色——**例外说明**:连接状态点是功能语义色(红/绿/灰),不是装饰色,可用固定语义色;但卡片/文字/边框一律 token
- mockup 里的 `hsl(...)` 绿色调只是 brainstorm 示意,实现一律换 token

走查:warm-paper / qingye / forest-dark / the-finals 四主题人工逐屏过技能 + 集成两模块(记忆模块复用 `MemoryGraphView`,其主题已在 `MemoryPanel` 验证过)。

---

## 10. PR 形态

一条分支,bisectable commits,一个 PR。建议提交粒度:

| # | commit | 范围 |
|---|---|---|
| 1 | 后端:MCP transport + update 命令 | `ipc.rs` / `tauri_commands.rs` / `main.rs` + cargo test |
| 2 | 前端 bridge + 类型 | `types.ts` / `tauri-bridge.ts` |
| 3 | MemoryModule | `modules/Memory/` + 路由接线 |
| 4 | SkillsModule | `modules/Skills/` + 路由接线 |
| 5 | IntegrationsModule | `modules/Integrations/`(卡片/抽屉/编辑器/模板库)+ 路由接线 |
| 6 | 语义拆分清理 | `ToolSettings.tsx` 删段 + 提示卡;`SkillsSettings.tsx` 复用/迁移 |

提交 2 在 1 之后(类型依赖后端结构体)。3/4/5 互相独立,顺序不限。6 放最后(依赖 4/5 已把功能搬走)。

---

## 11. 风险与依赖

| 风险 | 缓解 |
|---|---|
| `SkillsSettings.tsx` 渲染逻辑耦合较深,复用 vs 整体迁移不好选 | plan 阶段先读 `SkillsSettings.tsx` 全文再定;两条路都可接受,不阻塞 |
| `add_mcp_server` 现有调用方(Settings 的添加流程)在语义拆分后被删,但命令本身仍被新编辑器用 | 命令保留,只是调用点从 `ToolSettings.tsx` 换成 `McpEditorModal`;`McpServerInput` 全 Option 字段保证不破坏其他潜在调用方 |
| `list_mcp_tools` 返回 `Vec<serde_json::Value>`,前端按 `serverId` 分组依赖字段名 | plan 阶段确认 `McpToolDef` 序列化后的字段名(`serverId` camelCase) |
| MCP 迁移版本号 | 本期**无 DB migration** —— `mcp_servers.json` 是文件持久化,不碰 SQLite |

---

## 12. 验收清单

- [ ] 记忆模块:进入显示 `MemoryGraphView`,full-bleed,无空白边
- [ ] 技能模块:学得 + 内置都列出,分组可折叠,点项切详情,演进时间线可展开
- [ ] 集成模块:每 MCP server 一张富卡,状态点正确,点卡出抽屉
- [ ] MCP 编辑器:新建 stdio / http server 均可;编辑现有 server 字段预填正确;transport 切换字段组替换;「测试连接并保存」连得上的 server 显示已连接
- [ ] 模板库:GitHub/Notion/Slack 模板预填正确,Custom 空表单
- [ ] `ToolSettings.tsx`:MCP 段 + 内置技能段已移除,workspace skill tags + manifest 调试仍在,提示卡文案正确
- [ ] 后端:`cargo test` MCP 相关单测通过;`cargo build` 无 error
- [ ] 前端:`npx tsc --noEmit` 无 error;`npm test -- --run` 三个新模块测试通过
- [ ] 主题:warm-paper / qingye / forest-dark / the-finals 四主题人工过技能 + 集成
- [ ] `ComingSoonModule` 现在只有 `artifacts` 命中
