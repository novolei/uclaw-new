# Kaleidoscope · 万花筒设计

- **Date**: 2026-05-14
- **Status**: Draft (awaiting review)
- **Owner**: Ryan
- **Related**: `WorkspaceSwitcherBar.tsx`, `AutomationsView.tsx`, `MemoryGraphView.tsx`, `SettingsPanel.tsx`

---

## 1. 背景与动机

uClaw 当前的主窗口集中承担两种心智：

1. **任务流** — 与 Agent 对话 / Chat / 文件 artifact 管理（高频）
2. **配置流** — 散落在多处：
   - `AutomationsView`：仅在 Agent 模式下从左栏顶部按钮唤起，3 个子 tab（Humans / Apps / Store）
   - `SettingsPanel` → Tools tab：MCP 服务器管理
   - `SettingsPanel` → Skills tab：技能注册表
   - `MemoryGraphView`：组件已写但未接入主导航

这种分布让"配置 / 资产管理"心智被切碎，用户找一个能力需要在 3 处之间跳。本设计引入第二个并行 surface — **万花筒（Kaleidoscope）** — 作为"你拥有 / 积累的资产 + 你 Agent 的内功"的统一家，让主窗口回到纯粹的任务流定位。

类比参考：Arc 浏览器的 Library / macOS Launchpad。

---

## 2. 目标与非目标

### 目标
- 一个独立、可一键进出的"配置 / 资产"surface
- 收编以下现有零散入口：Automations、Marketplace、Memory Graph、Skills（重设计）、MCP（重设计）
- 新增 Artifacts 模块（按 session / 应用分组的产出浏览器）
- 用 Arc 风窄轨设计 + uClaw 主题 token，11 个主题完整自适应
- 入口图标用 Lottie 实现 hover 动画，保留静态 SVG 作为 fallback

### 非目标
- **不**重写 Settings 面板 — Settings 继续负责"参数"（LLM API keys、主题、快捷键、provider 配置）
- **不**引入 react-router — uClaw 当前是 atom-driven，保持一致
- **不**改变 chat / agent 子模式行为 — 它们仍然由 `appModeAtom` 控制，只是被新的 `WorkspaceShell` 包了一层
- **不**实现 Cost Dashboard / Scheduled / Prompts / Harness — 列入未来扩展

---

## 3. 用户故事

- **配置一次性集中**：用户想给 Agent 接一个新 MCP server，不需要先开 Settings 再翻 Tools tab；点万花筒入口 → 集成模块 → "+ 添加集成"。
- **资产可见性**：用户想看自己训练过哪些数字人、装了哪些应用、Memory 里 Agent 记得自己什么 —— 一个 surface 三步看完。
- **任务态心智不被打扰**：在 chat / agent 模式专注对话时，不会被左栏一堆配置按钮分散注意力。

---

## 4. 顶层架构

### 4.1 状态建模

引入"顶层视图枚举"作为最高层 atom，现有 chat/agent 子模式归到 `WorkspaceShell` 内部。

```ts
// ui/src/atoms/top-level-view.ts  (新)
export type TopLevelView = 'workspace' | 'kaleidoscope'
export const topLevelViewAtom = atom<TopLevelView>('workspace')

// ui/src/atoms/kaleidoscope.ts  (新)
export type KaleidoscopeModuleId =
  | 'humans' | 'store' | 'apps' | 'artifacts'   // 资产组
  | 'skills' | 'integrations' | 'memory'         // 能力组
export const kaleidoscopeModuleAtom = atom<KaleidoscopeModuleId>('humans')
```

- `appModeAtom`（`'chat' | 'agent'`）保留不动 — 但只在 `topLevelViewAtom === 'workspace'` 时生效
- `automationPanelOpenAtom` 保留作为遗留入口（agent 模式下的快速跳转），但点击后会**置 `topLevelViewAtom = 'kaleidoscope'` 并选中 'humans' 模块**，原 `AutomationsView` 不再单独渲染

### 4.2 视图切换

```tsx
// MainArea.tsx (重构后)
const view = useAtomValue(topLevelViewAtom)
return view === 'kaleidoscope' ? <KaleidoscopeShell /> : <WorkspaceShell />
```

- `WorkspaceShell` 是 move-only 重构产物 — 容纳当前 chat / agent 切换逻辑
- 切换动画：父容器加 200ms cross-dissolve（CSS class toggle，纯样式无 JS）

### 4.3 文件组织

```
ui/src/
  atoms/
    top-level-view.ts                 (新)
    kaleidoscope.ts                   (新)
  views/
    Kaleidoscope/                     (新)
      KaleidoscopeShell.tsx           ← 整合 rail + main area + 底部三段
      KaleidoscopeRail.tsx            ← 7 模块导航 + 底部 ←/✦
      KaleidoscopeIcon.tsx            ← Lottie 包装 (hover/active/fallback)
      atoms.ts                        ← 模块本地 atoms（详情抽屉、筛选等）
      modules/
        Humans/
          HumansModule.tsx            ← wrap AutomationsView 的 HumansTab
        Store/
          StoreModule.tsx             ← wrap StoreView + StoreDetail
        Apps/
          AppsModule.tsx              ← wrap AppsTab
        Artifacts/
          ArtifactsModule.tsx         ← 新做
          ArtifactsList.tsx
          ArtifactsThumb.tsx
        Skills/
          SkillsModule.tsx            ← 从 SkillsSettings 提取重设计
        Integrations/
          IntegrationsModule.tsx      ← 从 ToolSettings 提取重设计
        Memory/
          MemoryModule.tsx            ← wrap MemoryGraphView
      shared/
        ModuleHeader.tsx              ← 分组标签 + 标题 + 副标题 + CTA
        EmptyState.tsx                ← 7 模块共用空态引导
        AssetCard.tsx                 ← 卡片网格基底
    Workspace/                        (新)
      WorkspaceShell.tsx              ← 容纳现有 chat/agent 切换
```

入口图标接入：
- `ui/src/components/workspace/WorkspaceSwitcherBar.tsx`：最左位置渲染 `<KaleidoscopeIcon />`，紧跟一条 vertical hairline 再接 workspace dots
- 点击 → `setTopLevelView('kaleidoscope')`

---

## 5. 视觉规范

### 5.1 KaleidoscopeShell 布局

- 总宽：fill window；rail = **120px** 固定，main area = `1fr`
- Rail 背景 `bg-background` 叠 `bg-muted/30`（与现有 `LeftSidebar.tsx` 的取色一致），main `bg-background`，分界 1px `border-border`

### 5.2 Rail 结构（从上至下）

| 区段 | 高度 | 内容 |
|---|---|---|
| Traffic light padding | 36px | 留白 |
| 资产组 | auto | 数字人 / 应用商店 / 我的应用 / 产出 |
| 分隔线 | 36×1px hairline `border-border` | margin 2px |
| 能力组 | auto | 技能 / 集成 / 记忆 |
| 分割线 | 1px `border-border`, margin 0 10px | — |
| 返回 + 装饰行 | 44px | 左：← 返回 (28×28, `bg-primary/15` + `border-primary/35`)；右：✦ 装饰 SVG (28×28, `text-primary/35` + `drop-shadow-[0_0_6px_hsl(var(--primary)/0.35)]`)，不可交互 |
| 分割线 | 1px `border-border`, margin 0 10px | — |
| User / Settings | 48px | 复用 `LeftSidebar` 现有 User 行；宽度自适应 |

每个 module 条目：
- 宽度 88% rail（约 106px）
- 图标 22px + 标签 11px/font-weight 600，竖排居中，gap 6px
- 选中态：`bg-primary/18` + `border-primary/35` + `text-foreground`（资产组）或 `bg-accent/18` + `border-accent/35`（能力组）
- 默认态：图标 `opacity-70`，标签 `text-muted-foreground`
- hover：背景 `bg-muted/30`
- 条目间距 18~20px

### 5.3 Main area 通用 header（`ModuleHeader`）

```
┌─────────────────────────────────────────────┐
│ 资产                          [🔍] [+ 新建] │  ← 11px uppercase / 灰 + 右上 CTA
│ 数字人 · Automations                        │  ← 22px / 600
│ 你训练的 5 个数字人 · 本周 18 次执行         │  ← 12px / muted
└─────────────────────────────────────────────┘
```

### 5.4 卡片基底（`AssetCard`）

- 圆角 10px，1px `border-border`，padding 14px，固定高度 96px
- 默认 `bg-card`，hover `bg-card` 叠 `bg-primary/5`，激活 `bg-primary/15 + border-primary/30`
- 网格：grid-cols-3 / gap 12px / 自适应到 cols-2（< 768px main 宽度）

### 5.5 详情抽屉

- 所有模块的"点卡片看详情"统一用**右滑抽屉**（width 420px，高度 100%）
- 抽屉背景 `bg-popover`，1px 左 border `border-border`
- 不用 modal（保持空间连续性）

### 5.6 颜色心智

- **资产组**（数字人 / 应用商店 / 我的应用 / 产出）→ 选中态用 `--primary` tint
- **能力组**（技能 / 集成 / 记忆）→ 选中态用 `--accent` tint
- 11 主题下 `--primary` 偏暖、`--accent` 偏冷的组合大多成立；个别主题（如 the-finals）需要 PR 阶段截图核对

---

## 6. 入口图标 + Lottie 动画

### 6.1 组件契约

```tsx
// ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx
interface KaleidoscopeIconProps {
  active?: boolean              // 当前是否在 Kaleidoscope surface
  onClick?: () => void
  size?: number                 // default 30
}
```

行为：
- 加载 `/src/assets/lottie/kaleidoscope-icon.json`（用户后续提供 Lottie 文件）
- 默认暂停 frame 0
- `onMouseEnter` → `lottieRef.current?.play()`
- `onMouseLeave` → `lottieRef.current?.setDirection(-1); lottieRef.current?.play()` 回到 frame 0
- `active=true` → 跳到结尾帧并 hold；具体帧号在 Lottie 文件到位后由用户提供（约定输入：hover 入场段、循环段、结尾定格帧三段帧号），Phase 1 阶段允许此参数缺失（active 态退化为 fallback SVG 的 ring-2 外圈）
- onClick → `setTopLevelView('kaleidoscope')`，触发 200ms cross-dissolve

### 6.2 Fallback

- Lottie JSON 加载失败 → 渲染内联 SVG（设计稿 v2 的彩色小篮子，30×30，linear-gradient `from-primary to-accent`，内部 fill `--primary-foreground`）
- 用 `<ErrorBoundary>` 包裹 Lottie 部分，捕获到错误回退到 SVG
- Fallback 路径保留 CSS hover 效果（scale 1.06 + glow），保证体验不缩水

### 6.3 依赖

- 新增 `lottie-react` npm 包（~50kb gzip）
- 加入 `vite.config.ts` 的 `vendor` manual chunk（与 `jotai/clsx/tailwind-merge` 同列）

---

## 7. 模块详细规格

### 7.1 数字人（Humans）— 复用
- 直接 wrap 现有 `AutomationsView/HumansTab` 内容
- 替换原 tab 切换为 `KaleidoscopeRail` 导航
- header CTA：`+ 新建数字人` → 调起现有创建向导
- 详情：点卡片 → 右抽屉显示运行历史 / 触发条件 / 编辑

### 7.2 应用商店（Store）— 复用
- 原样吸收 `StoreView` + `StoreDetail` + `InstallWizard`
- 当前 `automationsSubviewAtom` 的 4 个状态 (`humans` / `apps` / `store` / `store-detail`) 中的后两个继续生效，由 `StoreModule` 内部管理
- 不改动现有 marketplace 数据流

### 7.3 我的应用（Apps）— 复用
- Wrap `AppsTab`
- 卡片右上角 1×1 状态徽章：active / idle / error
- 详情抽屉：配置 / 卸载 / 重启

### 7.4 产出（Artifacts）— 新做
- 数据源：调 `list_workspace_files` Tauri 命令（或新增），列出 `~/Documents/workground/` 下文件
- 视图切换：list（默认）/ thumbnail
- 分组维度：按 session（agent_sessions.id 关联） / 按应用 / 按时间
- 右键菜单（contextMenu）：
  - 在 Finder 中显示（调 `shell.openPath`）
  - 复制完整路径
  - 删除（需确认对话框）
- 双击 → 用系统默认应用打开（调 `shell.open`）
- 空态：引导用户开一个 Agent 会话

### 7.5 技能（Skills）— 重设计
- 数据源：现有 `SkillsRegistry` / `list_skills` Tauri 命令
- 布局：左侧分类列表（内置 / 学得，可折叠）+ 右侧详情双栏
- 列表项：图标 + 名称 + 触发关键词预览
- 详情：触发示例 / 来源（learned 的话显示来自哪次对话）/ 启用开关（全局，影响 Agent 是否能调用此技能）
- 顶部 CTA：搜索 + `+ 添加自定义技能`（暂留 disabled，未来扩展）

### 7.6 集成（Integrations / MCP）— 重设计
- 数据源：`mcp_list_servers` Tauri 命令
- 网格卡片：每张 = 一个 MCP server，显示连接状态（绿点 / 红点 / 灰点）+ 暴露的工具数量
- 详情抽屉：连接日志 / 工具列表 / 重启 / 移除
- 头部 CTA：`+ 添加集成` → 弹出模板库（Slack / GitHub / Notion / Custom），与 Settings 现有添加流程共用底层 API
- 在 SettingsPanel→Tools 留一个提示卡（一行说明 + 跳转按钮） "完整管理请到万花筒 → 集成"，保留回退路径

### 7.7 记忆（Memory）— 复用
- Wrap `MemoryGraphView`，套上 `ModuleHeader`
- 左侧筛选：实体类型筛选（Person / Place / Topic / Event ...）
- 主区：force-directed 图谱（保留现有渲染）
- 右抽屉：点节点显示该实体所有 facts
- 头部 CTA：搜索（输入定位高亮节点）
- memU 未连接时显示提示卡 + "去 Settings 启用 memU" 链接

---

## 8. 主题适配

### 8.1 硬约束（CLAUDE.md 强制）

所有颜色 → theme token：

| 用途 | Token |
|---|---|
| 主背景 | `bg-background` |
| 卡片背景 | `bg-card` |
| 弹出 / 抽屉背景 | `bg-popover` |
| Rail 背景 | `bg-background` 叠 `bg-muted/30`（与现有 `LeftSidebar.tsx` 同源） |
| 边框 | `border-border` |
| 主文字 | `text-foreground` |
| 次要文字 | `text-muted-foreground` |
| 资产组高亮 | `bg-primary/15`, `border-primary/35`, `text-primary-foreground` |
| 能力组高亮 | `bg-accent/15`, `border-accent/35`, `text-accent-foreground` |
| 入口图标渐变 | `linear-gradient(135deg, hsl(var(--primary)), hsl(var(--accent)))` |
| 装饰 ✦ glow | `drop-shadow-[0_0_6px_hsl(var(--primary)/0.35)]` |

### 8.2 走查清单

PR #3 阶段，11 主题 × 7 模块的截图归档。最低门槛：以下 4 个有特色的主题必须人工逐屏过：
- **warm-paper**（浅色暖系，最贴 Arc 美学）
- **qingye**（深色冷系，对比测试）
- **forest-dark**（深色绿系）
- **the-finals**（高对比黑底）

---

## 9. 切换动画

### 9.1 全屏 cross-dissolve

- `MainArea` 容器加 CSS class `view-transition`
- 子元素 `WorkspaceShell` / `KaleidoscopeShell` 用 `opacity` + 极轻微 `scale(0.98 → 1)` 200ms `ease-out`
- 切换时 React 通过 `key` 触发卸载 / 重挂载（避免状态串扰）

### 9.2 入口图标点击 → 切换

```
click → 80ms scale(0.92) tap-down
      → 触发 setTopLevelView('kaleidoscope')
      → MainArea 重渲染，旧 surface fade-out + 新 surface fade-in 同时进行 (200ms cross-dissolve)
```

### 9.3 模块间切换（Rail 内）

- main area 子组件用 `key={moduleId}` 强制重挂载
- 80ms slide-fade（从右侧 12px 滑入 + opacity 0→1），不需要全屏 cross-dissolve

---

## 10. 错误处理与降级

| 情况 | 行为 |
|---|---|
| Lottie 加载失败 | ErrorBoundary 回退到静态 SVG，保留 CSS hover |
| memU 未启动（`appState.memu_client === None`） | 记忆模块显示"memU 未连接，依赖此模块需启动" + 跳 Settings 按钮 |
| Skills 数据为空 | 空态卡："你的 Agent 还没学到技能 — 让它处理几次任务就会积累" |
| MCP 服务器列表为空 | 空态卡：" 没有集成 — 点 + 添加，让 Agent 接入 Slack/GitHub/Notion" |
| Artifacts 目录不存在 | 引导："workspace 还是空的 — 开一个 Agent 会话开始创造" |
| Tauri 命令失败 | sonner toast 显示错误，UI 不崩溃 |

---

## 11. 测试策略

### 11.1 单元 / 组件测试（Vitest + RTL）

- `KaleidoscopeShell` 切换模块时 main area key 变更，子组件正确卸载 / 挂载
- `topLevelViewAtom = 'kaleidoscope'` 时不渲染 `WorkspaceShell`，反之亦然
- `appModeAtom` 在 Kaleidoscope 期间值保持（切回 workspace 后还是切走前的 chat 或 agent）
- `KaleidoscopeIcon` 在 Lottie JSON 加载失败时降级到 SVG 渲染（mock `lottie-react` import）
- 7 个模块的空态分支都要测一遍（mock 各自的 atom / Tauri 命令返回空）

### 11.2 不测的内容

- 主题视觉走查：人工 + 截图归档（无法自动化）
- Lottie 实际动画播放：依赖运行时 Canvas，不在 jsdom 范围（仅测组件契约）

### 11.3 Rust 后端

- 本次设计不引入新的 Rust 命令，复用现有 `list_skills` / `mcp_list_servers` 等
- 若 Phase 3 Artifacts 需要新增 `list_workspace_files`，单独写 `#[cfg(test)]` 单元测试覆盖空目录 / 大量文件 / 权限错误 3 个分支

---

## 12. 分期实施（3 PRs）

### Phase 1 — 骨架（PR #1，约 1-1.5 天）
- `topLevelViewAtom` + `kaleidoscopeModuleAtom`
- `WorkspaceShell` 提取：把 `MainArea.tsx` 中现有的 `chat / agent / automationOpen` 三分支条件渲染整体移入 `WorkspaceShell.tsx`，对外只暴露 `<WorkspaceShell />`，行为零变化（move-only ~50 行）
- `KaleidoscopeShell` + `KaleidoscopeRail` + 底部三段
- `KaleidoscopeIcon`（Lottie 包装 + SVG fallback；Lottie JSON 可暂时缺失，纯走 fallback 也能跑）
- 入口图标接入 `WorkspaceSwitcherBar`
- 1 个占位模块 `HumansModule`（只 wrap 现有 `HumansTab`，验证整条链路）
- 测试：状态切换 + 入口点击 + fallback 渲染

### Phase 2 — 现有模块迁移（PR #2，约 1-2 天）
- `StoreModule`（吸收 StoreView/StoreDetail）
- `AppsModule`（wrap AppsTab）
- `MemoryModule`（wrap MemoryGraphView）
- `SkillsModule`（从 Settings 提取重设计）
- `IntegrationsModule`（从 Settings 提取重设计）
- 共享：`ModuleHeader` / `EmptyState` / `AssetCard`
- Settings 留提示卡（一行说明 + 跳转按钮）到对应模块（保留回退路径）

### Phase 3 — 新做 + 走查（PR #3，约 1 天）
- `ArtifactsModule`（含 list / thumb / 右键菜单 / 双击打开）
- 若需要新 Tauri 命令：`list_workspace_files`
- 11 主题逐屏走查 + 截图归档
- 空态 / 错误态打磨
- README 截屏更新

---

## 13. 风险与依赖

| 风险 | 缓解 |
|---|---|
| Lottie 文件尚未到位 | Fallback SVG 完整可用，骨架可独立 ship |
| 主题适配走查工作量大（11 × 7 = 77 张） | Phase 3 集中走查；前两期写代码时严格使用 token，问题会少 |
| 从 Settings 提取 Skills/MCP 改动 Settings 现有 UI | 在 Settings 保留"完整管理请去万花筒"的提示卡（一行说明 + 跳转按钮），不破坏现有用户流 |
| memU 未启动用户记忆模块体验 | 空态卡 + 跳 Settings 引导，不崩 |
| 与正在进行的 V19 (workspace-skill-tags)、V20-21 (humane-automation) 迁移冲突 | 本设计**不引入新的 SQL 迁移**，规避数据层冲突 |

---

## 14. 未来扩展（不在本 spec 范围）

- 费用仪表盘（Cost Dashboard）：基于 V13 `cost_records` 可视化，按模型 / 会话 / 应用拆分
- 定时 / Cron 模块：proactive 场景的调度可视化
- 提示词库（Prompts）：保存 system prompts / 模板
- 评测面板（Harness）：Agent 评测运行
- 万花筒搜索：跨 7 模块的全局搜索
- 深链接 / URL：若未来引入 react-router 可加 `/kaleidoscope/<module>` 路径

---

## 15. 验收清单

- [ ] 点击 `WorkspaceSwitcherBar` 最左入口图标 → 全屏切换到 Kaleidoscope
- [ ] 在 Kaleidoscope 内点 ← 返回 → 回到原 chat / agent 子模式
- [ ] 7 个模块的 Rail 切换正常，main area 内容随 `kaleidoscopeModuleAtom` 更新
- [ ] Lottie 文件不在的情况下，入口图标用静态 SVG 渲染并保留 hover 效果
- [ ] 4 个有特色的主题（warm-paper / qingye / forest-dark / the-finals）下截图无明显视觉问题
- [ ] memU 未启动时记忆模块显示引导卡，不崩
- [ ] Phase 1 PR 能独立 ship 且不破坏现有 chat / agent / Settings 体验
- [ ] CI 全绿（cargo build、cargo test、tsc、vitest）
