# Kaleidoscope Phase 1 (骨架) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭起"万花筒"作为并行于 workspace 的第二个顶层 surface 的骨架 —— 顶层视图状态、`WorkspaceShell` 提取、`KaleidoscopeShell` + `KaleidoscopeRail` + Lottie 入口图标，外加 1 个真实模块（数字人，wrap 现有 `AutomationHub`），其余 6 个模块走占位。

**Architecture:** 引入 `topLevelViewAtom`（`'workspace' | 'kaleidoscope'`）作为最高层视图状态。`MainArea.tsx` 现有的 chat/agent/automation/homeOffice/preview 全部分支整体搬进新建的 `WorkspaceShell`，`MainArea` 只保留 `<Panel>` 外壳 + `<SettingsDialog>` + 用 `motion` 的 `AnimatePresence` 在两个 surface 间做 200ms cross-dissolve。`KaleidoscopeShell` = 120px 窄轨 `KaleidoscopeRail` + 主区，主区按 `kaleidoscopeModuleAtom` 渲染模块。入口图标 `KaleidoscopeIcon` 进 `WorkspaceSwitcherBar` 最左位置。

**Tech Stack:** React 18 + TypeScript、Jotai、`motion ^12.38.0`（已依赖）、`lottie-react`（本期新增）、Tailwind + theme tokens、Vitest + React Testing Library。

**Spec:** `docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`

---

## File Structure

| 文件 | 职责 | 操作 |
|---|---|---|
| `ui/src/atoms/top-level-view.ts` | 顶层视图枚举 atom | 新建 |
| `ui/src/atoms/kaleidoscope.ts` | 万花筒模块 id atom + 模块元数据表 | 新建 |
| `ui/src/views/Workspace/WorkspaceShell.tsx` | 收编 MainArea 现有全部 surface 分支与 hooks | 新建（move-only） |
| `ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.tsx` | 入口图标静态 SVG 兜底（彩色小篮子） | 新建 |
| `ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx` | 入口图标：Lottie 包装 + ErrorBoundary → fallback | 新建 |
| `ui/src/views/Kaleidoscope/shared/ModuleHeader.tsx` | 7 模块共用 header（分组标签+标题+副标题+CTA） | 新建 |
| `ui/src/views/Kaleidoscope/modules/Humans/HumansModule.tsx` | 数字人模块（wrap `AutomationHub`） | 新建 |
| `ui/src/views/Kaleidoscope/modules/ComingSoonModule.tsx` | 其余 6 模块的占位 | 新建 |
| `ui/src/views/Kaleidoscope/KaleidoscopeRail.tsx` | 120px 窄轨导航 + 底部三段 | 新建 |
| `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx` | rail + 主区组合 | 新建 |
| `ui/src/components/tabs/MainArea.tsx` | 顶层 switch + cross-dissolve | 修改 |
| `ui/src/components/workspace/WorkspaceSwitcherBar.tsx` | 最左插入入口图标 | 修改 |
| `ui/vite.config.ts` | `lottie-react` 进 manualChunk | 修改 |
| `ui/package.json` | 新增 `lottie-react` 依赖 | 修改 |

**已知 Phase 1 范围限制（写进 PR 描述）：** 入口图标放在 `WorkspaceSwitcherBar` 内，而该组件仅在 agent 模式渲染（`LeftSidebar.tsx:1087`）。因此 Phase 1 的万花筒入口仅在 agent 模式可见。`appModeAtom` 默认 `'agent'`，覆盖主路径；若后续要在 chat 模式也能进入，把 `KaleidoscopeIcon` 上提到 `LeftSidebar` 直接渲染 —— 留作 Phase 2 评估，不在本期。

---

## Task 1: 顶层视图与万花筒模块 atoms

**Files:**
- Create: `ui/src/atoms/top-level-view.ts`
- Create: `ui/src/atoms/kaleidoscope.ts`
- Test: `ui/src/atoms/kaleidoscope.test.ts`

- [ ] **Step 1: 写失败测试**

`ui/src/atoms/kaleidoscope.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { topLevelViewAtom } from './top-level-view'
import {
  kaleidoscopeModuleAtom,
  KALEIDOSCOPE_MODULES,
} from './kaleidoscope'

describe('top-level-view / kaleidoscope atoms', () => {
  it('topLevelViewAtom defaults to "workspace"', () => {
    const store = createStore()
    expect(store.get(topLevelViewAtom)).toBe('workspace')
  })

  it('kaleidoscopeModuleAtom defaults to "humans"', () => {
    const store = createStore()
    expect(store.get(kaleidoscopeModuleAtom)).toBe('humans')
  })

  it('KALEIDOSCOPE_MODULES lists 7 modules: 4 asset + 3 capability', () => {
    expect(KALEIDOSCOPE_MODULES).toHaveLength(7)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'asset')).toHaveLength(4)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'capability')).toHaveLength(3)
  })

  it('every module id is unique and humans is first', () => {
    const ids = KALEIDOSCOPE_MODULES.map((m) => m.id)
    expect(new Set(ids).size).toBe(7)
    expect(ids[0]).toBe('humans')
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/atoms/kaleidoscope.test.ts`
Expected: FAIL — `Cannot find module './top-level-view'`

- [ ] **Step 3: 实现 `top-level-view.ts`**

`ui/src/atoms/top-level-view.ts`:

```ts
/**
 * Top-Level View Atom — 最高层视图状态
 *
 * uClaw 有两个并行的顶层 surface：
 *  - 'workspace'    : 任务流（chat / agent / 文件 / artifact），由 WorkspaceShell 承载
 *  - 'kaleidoscope' : 配置流（数字人 / 应用 / 技能 / 集成 / 记忆 / 产出），由 KaleidoscopeShell 承载
 *
 * 不持久化 —— 应用重启回到 'workspace'（任务流是默认入口）。
 * `appModeAtom`（chat/agent）只在 topLevelView === 'workspace' 时生效。
 */
import { atom } from 'jotai'

export type TopLevelView = 'workspace' | 'kaleidoscope'

export const topLevelViewAtom = atom<TopLevelView>('workspace')
```

- [ ] **Step 4: 实现 `kaleidoscope.ts`**

`ui/src/atoms/kaleidoscope.ts`:

```ts
/**
 * Kaleidoscope Atoms — 万花筒内部状态
 *
 *  - kaleidoscopeModuleAtom : 当前选中的模块
 *  - KALEIDOSCOPE_MODULES   : 7 个模块的元数据表（id / 标签 / 分组），
 *                             Rail 的导航顺序与分组分隔以此为准
 *
 * 分组语义：
 *  - 'asset'      资产 —— 你拥有 / 积累的东西（数字人 / 应用商店 / 我的应用 / 产出）
 *  - 'capability' 能力 —— 你 Agent 的内功（技能 / 集成 / 记忆）
 *
 * 不持久化 —— Phase 1 每次进万花筒都从 'humans' 开始。
 */
import { atom } from 'jotai'

export type KaleidoscopeModuleId =
  | 'humans' | 'store' | 'apps' | 'artifacts'
  | 'skills' | 'integrations' | 'memory'

export type KaleidoscopeGroup = 'asset' | 'capability'

export interface KaleidoscopeModuleMeta {
  id: KaleidoscopeModuleId
  /** Rail 上显示的中文标签 */
  label: string
  group: KaleidoscopeGroup
}

export const KALEIDOSCOPE_MODULES: KaleidoscopeModuleMeta[] = [
  { id: 'humans', label: '数字人', group: 'asset' },
  { id: 'store', label: '应用商店', group: 'asset' },
  { id: 'apps', label: '我的应用', group: 'asset' },
  { id: 'artifacts', label: '产出', group: 'asset' },
  { id: 'skills', label: '技能', group: 'capability' },
  { id: 'integrations', label: '集成', group: 'capability' },
  { id: 'memory', label: '记忆', group: 'capability' },
]

export const kaleidoscopeModuleAtom = atom<KaleidoscopeModuleId>('humans')
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cd ui && npx vitest run src/atoms/kaleidoscope.test.ts`
Expected: PASS — 4 tests

- [ ] **Step 6: 提交**

```bash
git add ui/src/atoms/top-level-view.ts ui/src/atoms/kaleidoscope.ts ui/src/atoms/kaleidoscope.test.ts
git commit -m "feat(kaleidoscope): add top-level-view and kaleidoscope module atoms"
```

---

## Task 2: 提取 `WorkspaceShell`（move-only 重构）

把 `MainArea.tsx` 当前 `<Panel>` 内的全部内容和相关 hooks 整体搬进新建的 `WorkspaceShell.tsx`，`MainArea.tsx` 暂时只渲染 `<WorkspaceShell />`（行为零变化）。顶层 switch 在 Task 8 接入。

**这是 move-only 重构：不写新单测，回归保障 = 现有 vitest 全绿 + tsc 通过。**

**Files:**
- Create: `ui/src/views/Workspace/WorkspaceShell.tsx`
- Modify: `ui/src/components/tabs/MainArea.tsx`

- [ ] **Step 1: 创建 `WorkspaceShell.tsx`**

`ui/src/views/Workspace/WorkspaceShell.tsx` —— 内容为当前 `MainArea.tsx` 第 12-211 行去掉 `Panel` / `SettingsDialog` import 与外层 `<Panel>`/`<SettingsDialog>` 包裹后的产物：

```tsx
/**
 * WorkspaceShell — 任务流 surface。
 *
 * 由 MainArea.tsx move-only 提取（2026-05-14 Kaleidoscope Phase 1）。
 * 承载 chat / agent / automation / homeOffice / preview 全部分支与
 * 相关 hooks。MainArea 现在只负责顶层 surface 切换 + Panel 外壳 +
 * SettingsDialog。
 *
 * W4a-followup: when the preview panel is open, the body is split
 * horizontally (chat ↔ preview) with a draggable resize handle between
 * them. The split ratio is persisted via `previewPanelSplitRatioAtom`.
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { visibleTabsAtom, activeTabIdAtom } from '@/atoms/tab-atoms'
import {
  previewPanelOpenAtom,
  previewPanelSplitRatioAtom,
  selectedPreviewFileAtom,
} from '@/atoms/preview-panel-atoms'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import { PreviewPanel } from '@/components/preview/PreviewPanel'
import WelcomeView from '@/views/WelcomeView'
import { TabBar } from '@/components/tabs/TabBar'
import { TabContent } from '@/components/tabs/TabContent'
import { automationPanelOpenAtom } from '@/atoms/automation'
import { AutomationsView } from '@/views/AutomationsView'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeView } from '@/components/home-office/HomeOfficeView'

const MIN_CHAT_RATIO = 0.30
const MAX_CHAT_RATIO = 0.80

export function WorkspaceShell(): React.ReactElement {
  const tabs = useAtomValue(visibleTabsAtom)
  const activeTabId = useAtomValue(activeTabIdAtom)
  const setActiveTabId = useSetAtom(activeTabIdAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const setPreviewOpen = useSetAtom(previewPanelOpenAtom)
  const setSelectedPreviewFile = useSetAtom(selectedPreviewFileAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const [splitRatio, setSplitRatio] = useAtom(previewPanelSplitRatioAtom)
  const [automationOpen, setAutomationOpen] = useAtom(automationPanelOpenAtom)
  const [homeOfficeOpen, setHomeOfficeOpen] = useAtom(homeOfficePanelOpenAtom)
  const draggingRef = React.useRef(false)

  // Auto-close AutomationsView when the user picks a different session/tab
  // in LeftSidebar (Bug 3 from 2026-05-14 polish round). Watching activeTabId
  // covers chat / agent / browser tabs — they're the only routes a user
  // explicitly navigates to from the sidebar conversation list. AutomationsView
  // remains open while the user moves within it (humans/apps/store sub-tabs
  // don't touch activeTabId).
  const prevTabIdRef = React.useRef<string | null>(activeTabId)
  React.useEffect(() => {
    if (prevTabIdRef.current !== activeTabId && automationOpen) {
      setAutomationOpen(false)
    }
    prevTabIdRef.current = activeTabId
  }, [activeTabId, automationOpen, setAutomationOpen])

  // Reset the preview panel when the active workspace changes. The previously
  // selected file lived in the OLD workspace's mount tree — keeping the panel
  // open after switch surfaces a stale file (or a noisy "loading" state for a
  // path that the new workspace can't resolve). Future opens (via openPreview
  // from the rail) re-mount the panel with the new file.
  const prevWorkspaceRef = React.useRef(currentWorkspaceId)
  React.useEffect(() => {
    if (prevWorkspaceRef.current === currentWorkspaceId) return
    prevWorkspaceRef.current = currentWorkspaceId
    setPreviewOpen(false)
    setSelectedPreviewFile(null)
  }, [currentWorkspaceId, setPreviewOpen, setSelectedPreviewFile])

  // [FLASH-DEBUG] 监控 tabs 变化，如果 tabs.length 变为 0 说明所有标签被卸载
  React.useEffect(() => {
    if (tabs.length === 0) {
      console.warn('[FLASH-DEBUG] WorkspaceShell: tabs.length === 0, showing WelcomeView!', new Error().stack)
    }
  }, [tabs.length])

  // 兜底：tabs 存在但 activeTabId 为空时，自动激活第一个标签。
  // 正常路径（openTab/closeTab/持久化恢复）都会维护 activeTabId，此分支只为防御
  // 异常状态（如外部原子被误清空），避免渲染 WelcomeView 触发重复 openTab 循环。
  React.useEffect(() => {
    if (tabs.length > 0 && !activeTabId) {
      setActiveTabId(tabs[0]!.id)
    }
  }, [tabs, activeTabId, setActiveTabId])

  const onResizeStart = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      draggingRef.current = true
      const startX = e.clientX
      const startRatio = splitRatio
      const containerEl = (e.currentTarget as HTMLElement).closest(
        '[data-preview-split]',
      ) as HTMLElement | null
      const containerWidth = containerEl?.clientWidth ?? 1
      let rafId = 0

      document.body.style.userSelect = 'none'
      document.body.style.cursor = 'col-resize'
      // Lock iframes during the drag so they don't swallow mouse events.
      document.querySelectorAll('iframe').forEach((f) => {
        ;(f as HTMLElement).style.pointerEvents = 'none'
      })

      const onMove = (ev: MouseEvent) => {
        if (!draggingRef.current) return
        if (rafId) return
        rafId = requestAnimationFrame(() => {
          rafId = 0
          const delta = ev.clientX - startX
          const next = Math.max(
            MIN_CHAT_RATIO,
            Math.min(MAX_CHAT_RATIO, startRatio + delta / containerWidth),
          )
          setSplitRatio(next)
        })
      }
      const onUp = () => {
        draggingRef.current = false
        if (rafId) cancelAnimationFrame(rafId)
        document.body.style.userSelect = ''
        document.body.style.cursor = ''
        document.querySelectorAll('iframe').forEach((f) => {
          ;(f as HTMLElement).style.pointerEvents = ''
        })
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
      }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [splitRatio, setSplitRatio],
  )

  const chatBody = (
    <>
      <TabBar />
      {/* Both body branches sit inside their own titlebar-no-drag so
          clicks land on chat/agent/welcome UI; the TabBar above stays
          in the drag region. We removed the previous broad
          titlebar-no-drag wrapper at AppShell-level because WKWebView
          won't subtract a child drag-region from a parent no-drag. */}
      {tabs.length === 0 ? (
        <div className="flex-1 min-h-0 titlebar-no-drag">
          <WelcomeView />
        </div>
      ) : activeTabId ? (
        <div className="flex-1 min-h-0 titlebar-no-drag">
          <TabContent tabId={activeTabId} />
        </div>
      ) : null}
    </>
  )

  React.useEffect(() => {
    if (!homeOfficeOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setHomeOfficeOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [homeOfficeOpen, setHomeOfficeOpen])

  if (homeOfficeOpen) return <HomeOfficeView />
  if (automationOpen) return <AutomationsView />
  if (previewOpen) {
    return (
      <div className="flex flex-1 min-h-0" data-preview-split>
        {/* Left: chat (TabBar + TabContent) */}
        <div
          className="flex flex-col min-w-0 h-full"
          style={{ flex: `0 0 calc(${splitRatio * 100}% - 4px)` }}
        >
          {chatBody}
        </div>

        {/* Drag handle */}
        <button
          type="button"
          onMouseDown={onResizeStart}
          aria-label="拖动调整预览面板宽度"
          title="拖动调整宽度"
          className="w-[8px] cursor-col-resize flex-shrink-0 self-stretch bg-border/40 hover:bg-foreground/20 active:bg-foreground/30 transition-colors"
        />

        {/* Right: preview */}
        <div className="flex-1 min-w-0 h-full overflow-hidden">
          <PreviewPanel />
        </div>
      </div>
    )
  }
  return chatBody
}
```

> 注意：原 `MainArea` 用三元嵌套渲染分支，提取后改为 early-return（行为完全等价，可读性更好）。`chatBody` 是 `React.ReactElement` 片段，`return chatBody` 合法。

- [ ] **Step 2: 改写 `MainArea.tsx` 仅渲染 `WorkspaceShell`**

`ui/src/components/tabs/MainArea.tsx` 全文替换为：

```tsx
/**
 * MainArea — 主内容区域
 *
 * 顶层 surface 切换：'workspace'（任务流，WorkspaceShell）↔ 'kaleidoscope'
 * （配置流，KaleidoscopeShell）。两个 surface 用 motion 的 AnimatePresence
 * 做 200ms cross-dissolve。设置以浮窗形式叠加显示。
 *
 * Task 8 会把 topLevelViewAtom 的 switch 接进来；本次（Task 2）只渲染
 * WorkspaceShell，等价于重构前行为。
 */

import * as React from 'react'
import { Panel } from '@/components/app-shell/Panel'
import { SettingsDialog } from '@/components/settings/SettingsDialog'
import { WorkspaceShell } from '@/views/Workspace/WorkspaceShell'

export function MainArea(): React.ReactElement {
  return (
    <>
      <Panel
        variant="grow"
        className="bg-content-area rounded-2xl shadow-xl"
      >
        <WorkspaceShell />
      </Panel>
      <SettingsDialog />
    </>
  )
}
```

- [ ] **Step 3: tsc 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无输出（无类型错误）

- [ ] **Step 4: 跑全量 vitest 确认回归绿**

Run: `cd ui && npm test -- --run 2>&1 | tail -10`
Expected: 全部通过，无新增 failure（move-only 不改变任何行为）

- [ ] **Step 5: 提交**

```bash
git add ui/src/views/Workspace/WorkspaceShell.tsx ui/src/components/tabs/MainArea.tsx
git commit -m "refactor(kaleidoscope): extract WorkspaceShell from MainArea (move-only)"
```

---

## Task 3: 新增 `lottie-react` 依赖 + 入口图标静态兜底 `KaleidoscopeIconFallback`

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/vite.config.ts`
- Create: `ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.tsx`
- Test: `ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx`

- [ ] **Step 1: 安装 `lottie-react`**

Run: `cd ui && npm install lottie-react@^2.4.1`
Expected: `package.json` 的 `dependencies` 新增 `"lottie-react": "^2.4.1"`，`package-lock.json` 更新

- [ ] **Step 2: 把 `lottie-react` 加进 vite manualChunk**

`ui/vite.config.ts` —— 在现有 vendor 判断块（`id.includes('node_modules/jotai')...` 那段）后追加一条：

找到：

```ts
          if (
            id.includes('node_modules/jotai') ||
            id.includes('node_modules/clsx') ||
            id.includes('node_modules/tailwind-merge')
          ) {
            return 'vendor'
          }
```

在它后面新增：

```ts
          // Lottie runtime — only the Kaleidoscope entry icon uses it.
          // Own chunk so the ~50KB gzip cost is isolated from the main bundle.
          if (id.includes('node_modules/lottie-react') || id.includes('node_modules/lottie-web')) {
            return 'lottie'
          }
```

- [ ] **Step 3: 写失败测试**

`ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { KaleidoscopeIconFallback } from './KaleidoscopeIconFallback'

describe('KaleidoscopeIconFallback', () => {
  it('renders an svg with the kaleidoscope aria-label', () => {
    render(<KaleidoscopeIconFallback />)
    const el = screen.getByLabelText('万花筒')
    expect(el).toBeInTheDocument()
  })

  it('honours the size prop', () => {
    const { container } = render(<KaleidoscopeIconFallback size={48} />)
    const wrapper = container.firstElementChild as HTMLElement
    expect(wrapper.style.width).toBe('48px')
    expect(wrapper.style.height).toBe('48px')
  })
})
```

- [ ] **Step 4: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx`
Expected: FAIL — `Cannot find module './KaleidoscopeIconFallback'`

- [ ] **Step 5: 实现 `KaleidoscopeIconFallback.tsx`**

`ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.tsx`:

```tsx
/**
 * KaleidoscopeIconFallback — 万花筒入口图标的静态 SVG 兜底。
 *
 * 当 Lottie 动画数据缺失或运行时加载失败时渲染这个。彩色小篮子 + sparkle，
 * 渐变用 theme token（primary → accent），SVG 内描用 primary-foreground，
 * 保证每个主题下都有对比度。
 *
 * 纯展示组件 —— 不绑定点击；交互由父组件（KaleidoscopeIcon）处理。
 */
import * as React from 'react'

export interface KaleidoscopeIconFallbackProps {
  /** 外框边长（含渐变背板），单位 px。默认 30。 */
  size?: number
}

export function KaleidoscopeIconFallback({
  size = 30,
}: KaleidoscopeIconFallbackProps): React.ReactElement {
  const svgSize = Math.round(size * 0.6)
  return (
    <div
      aria-label="万花筒"
      role="img"
      style={{ width: size, height: size }}
      className="inline-flex items-center justify-center rounded-[8px]
                 bg-gradient-to-br from-primary to-accent
                 shadow-[0_1px_3px_hsl(var(--primary)/0.35)]"
    >
      <svg
        viewBox="0 0 24 24"
        width={svgSize}
        height={svgSize}
        fill="none"
        className="text-primary-foreground"
        aria-hidden
      >
        {/* basket body */}
        <path
          d="M5 10 Q5 9 6 9 H18 Q19 9 19 10 L18 19 Q17.8 20 17 20 H7 Q6.2 20 6 19 Z"
          fill="currentColor"
          opacity="0.95"
        />
        <path d="M5 10 H19" stroke="currentColor" strokeWidth="1.4" opacity="0.6" />
        <ellipse cx="12" cy="9" rx="5.5" ry="0.9" fill="currentColor" opacity="0.4" />
        {/* sparkle */}
        <path
          d="M16.5 4 L17.2 5.5 L18.8 6.2 L17.2 6.9 L16.5 8.4 L15.8 6.9 L14.2 6.2 L15.8 5.5 Z"
          fill="currentColor"
        />
        <circle cx="19.5" cy="3.5" r="0.7" fill="currentColor" opacity="0.85" />
      </svg>
    </div>
  )
}
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx`
Expected: PASS — 2 tests

- [ ] **Step 7: 提交**

```bash
git add ui/package.json ui/package-lock.json ui/vite.config.ts ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.tsx ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx
git commit -m "feat(kaleidoscope): add lottie-react dep and static entry-icon fallback"
```

---

## Task 4: `KaleidoscopeIcon`（Lottie 包装 + ErrorBoundary → fallback）

入口图标主组件。接收可选的 `animationData`（Lottie JSON）：
- 没有 `animationData`（Phase 1 默认状态，Lottie 文件尚未到位）→ 直接渲染 `KaleidoscopeIconFallback`
- 有 `animationData` → 渲染 `<Lottie>`，并用 ErrorBoundary 包裹，运行时报错回退到 `KaleidoscopeIconFallback`

**Files:**
- Create: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx`
- Test: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`

- [ ] **Step 1: 写失败测试**

`ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { KaleidoscopeIcon } from './KaleidoscopeIcon'

// Mock lottie-react so the test doesn't need a real canvas/animation runtime.
vi.mock('lottie-react', () => ({
  default: () => <div data-testid="lottie-stub" />,
}))

describe('KaleidoscopeIcon', () => {
  it('renders the static fallback when no animationData is provided', () => {
    render(<KaleidoscopeIcon />)
    expect(screen.getByLabelText('万花筒')).toBeInTheDocument()
    expect(screen.queryByTestId('lottie-stub')).not.toBeInTheDocument()
  })

  it('renders the Lottie player when animationData is provided', () => {
    render(<KaleidoscopeIcon animationData={{ v: '5.0.0', fr: 30, layers: [] }} />)
    expect(screen.getByTestId('lottie-stub')).toBeInTheDocument()
  })

  it('is a button with an accessible label and fires onClick', async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(<KaleidoscopeIcon onClick={onClick} />)
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(onClick).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
Expected: FAIL — `Cannot find module './KaleidoscopeIcon'`

- [ ] **Step 3: 实现 `KaleidoscopeIcon.tsx`**

`ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx`:

```tsx
/**
 * KaleidoscopeIcon — 万花筒入口图标（放在 WorkspaceSwitcherBar 最左）。
 *
 * 行为：
 *  - 无 animationData（Phase 1 默认，Lottie 文件尚未到位）→ 渲染静态
 *    KaleidoscopeIconFallback。
 *  - 有 animationData → 渲染 Lottie；hover 播放、leave 倒回 frame 0、
 *    active 定格结尾帧。Lottie 渲染被 ErrorBoundary 包裹，运行时报错
 *    回退到 KaleidoscopeIconFallback。
 *
 * Lottie JSON 到位后，调用方传入 `animationData={import('...json')}` 即可。
 * active 态的结尾帧号待 Lottie 文件到位后由 props 补充（见 spec §6.1）。
 */
import * as React from 'react'
import Lottie, { type LottieRefCurrentProps } from 'lottie-react'
import { cn } from '@/lib/utils'
import { KaleidoscopeIconFallback } from './KaleidoscopeIconFallback'

export interface KaleidoscopeIconProps {
  /** Lottie 动画 JSON。缺省时走静态 SVG 兜底。 */
  animationData?: object
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
  /** 外框边长 px，默认 30。 */
  size?: number
}

/** 包裹 Lottie 渲染，运行时异常时回退到静态 SVG。 */
class LottieErrorBoundary extends React.Component<
  { fallback: React.ReactNode; children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false }
  static getDerivedStateFromError(): { hasError: boolean } {
    return { hasError: true }
  }
  componentDidCatch(err: unknown): void {
    console.warn('[KaleidoscopeIcon] Lottie render failed, using static fallback:', err)
  }
  render(): React.ReactNode {
    return this.state.hasError ? this.props.fallback : this.props.children
  }
}

export function KaleidoscopeIcon({
  animationData,
  active = false,
  onClick,
  size = 30,
}: KaleidoscopeIconProps): React.ReactElement {
  const lottieRef = React.useRef<LottieRefCurrentProps>(null)

  const handleEnter = React.useCallback(() => {
    lottieRef.current?.setDirection(1)
    lottieRef.current?.play()
  }, [])
  const handleLeave = React.useCallback(() => {
    lottieRef.current?.setDirection(-1)
    lottieRef.current?.play()
  }, [])

  const fallback = <KaleidoscopeIconFallback size={size} />

  const inner = animationData ? (
    <LottieErrorBoundary fallback={fallback}>
      <Lottie
        lottieRef={lottieRef}
        animationData={animationData}
        autoplay={false}
        loop
        style={{ width: size, height: size }}
      />
    </LottieErrorBoundary>
  ) : (
    fallback
  )

  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
      className={cn(
        'titlebar-no-drag inline-flex items-center justify-center rounded-[8px]',
        'transition-transform duration-200 ease-out shrink-0',
        'hover:scale-[1.06] active:scale-[0.92]',
        active && 'ring-2 ring-primary/40',
      )}
    >
      {inner}
    </button>
  )
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
Expected: PASS — 3 tests

- [ ] **Step 5: 提交**

```bash
git add ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx
git commit -m "feat(kaleidoscope): add KaleidoscopeIcon with Lottie wrapper and fallback"
```

---

## Task 5: `ModuleHeader` 共享组件 + `HumansModule` + `ComingSoonModule`

**Files:**
- Create: `ui/src/views/Kaleidoscope/shared/ModuleHeader.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Humans/HumansModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/ComingSoonModule.tsx`
- Test: `ui/src/views/Kaleidoscope/shared/ModuleHeader.test.tsx`

- [ ] **Step 1: 写失败测试**

`ui/src/views/Kaleidoscope/shared/ModuleHeader.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ModuleHeader } from './ModuleHeader'

describe('ModuleHeader', () => {
  it('renders group label, title and subtitle', () => {
    render(
      <ModuleHeader group="asset" title="数字人" subtitle="5 个 · 本周 18 次执行" />,
    )
    expect(screen.getByText('资产')).toBeInTheDocument()
    expect(screen.getByText('数字人')).toBeInTheDocument()
    expect(screen.getByText('5 个 · 本周 18 次执行')).toBeInTheDocument()
  })

  it('renders the capability group label', () => {
    render(<ModuleHeader group="capability" title="技能" />)
    expect(screen.getByText('能力')).toBeInTheDocument()
  })

  it('renders action node when provided', () => {
    render(
      <ModuleHeader
        group="asset"
        title="数字人"
        actions={<button type="button">+ 新建</button>}
      />,
    )
    expect(screen.getByRole('button', { name: '+ 新建' })).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/shared/ModuleHeader.test.tsx`
Expected: FAIL — `Cannot find module './ModuleHeader'`

- [ ] **Step 3: 实现 `ModuleHeader.tsx`**

`ui/src/views/Kaleidoscope/shared/ModuleHeader.tsx`:

```tsx
/**
 * ModuleHeader — 万花筒 7 个模块主区共用的顶部 header。
 *
 * 结构：分组标签（资产 / 能力，小号大写灰字）+ 模块标题（22px/600）+
 * 可选状态副标题（12px muted）+ 右上角可选操作区（搜索 / CTA）。
 *
 * 全部走 theme token，不写死颜色。
 */
import * as React from 'react'
import type { KaleidoscopeGroup } from '@/atoms/kaleidoscope'

const GROUP_LABEL: Record<KaleidoscopeGroup, string> = {
  asset: '资产',
  capability: '能力',
}

export interface ModuleHeaderProps {
  group: KaleidoscopeGroup
  title: string
  subtitle?: string
  /** 右上角操作区（搜索框 / 主 CTA）。 */
  actions?: React.ReactNode
}

export function ModuleHeader({
  group,
  title,
  subtitle,
  actions,
}: ModuleHeaderProps): React.ReactElement {
  return (
    <div className="flex items-start justify-between gap-4 px-8 pt-7 pb-4">
      <div className="min-w-0">
        <div className="text-[11px] uppercase tracking-[0.5px] text-muted-foreground">
          {GROUP_LABEL[group]}
        </div>
        <h1 className="mt-0.5 text-[22px] font-semibold text-foreground truncate">
          {title}
        </h1>
        {subtitle && (
          <div className="mt-0.5 text-[12px] text-muted-foreground truncate">
            {subtitle}
          </div>
        )}
      </div>
      {actions && <div className="flex items-center gap-2 shrink-0">{actions}</div>}
    </div>
  )
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/shared/ModuleHeader.test.tsx`
Expected: PASS — 3 tests

- [ ] **Step 5: 实现 `HumansModule.tsx`**

`ui/src/views/Kaleidoscope/modules/Humans/HumansModule.tsx` —— Phase 1 直接 wrap 现有 `AutomationHub`（`AutomationsView` 的 'humans' 子视图组件），套上 `ModuleHeader`：

```tsx
/**
 * HumansModule — 万花筒「数字人」模块。
 *
 * Phase 1：直接复用现有 AutomationHub（AutomationsView 的 'humans' 子视图），
 * 套一层统一的 ModuleHeader。Phase 2 再按画廊规格细化卡片与详情抽屉。
 */
import * as React from 'react'
import { AutomationHub } from '@/components/automation/AutomationHub'
import { ModuleHeader } from '../../shared/ModuleHeader'

export function HumansModule(): React.ReactElement {
  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader group="asset" title="数字人 · Automations" />
      <div className="flex-1 min-h-0">
        <AutomationHub />
      </div>
    </div>
  )
}
```

- [ ] **Step 6: 实现 `ComingSoonModule.tsx`**

`ui/src/views/Kaleidoscope/modules/ComingSoonModule.tsx` —— Phase 1 里 humans 之外 6 个模块的占位：

```tsx
/**
 * ComingSoonModule — Phase 1 占位。
 *
 * 万花筒 Rail 在 Phase 1 就展示全部 7 个模块入口（导航结构是骨架的一部分），
 * 但只有「数字人」有真实实现。其余 6 个点进来渲染这个占位，Phase 2 逐个替换。
 */
import * as React from 'react'
import { KALEIDOSCOPE_MODULES, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import { ModuleHeader } from '../shared/ModuleHeader'

export interface ComingSoonModuleProps {
  moduleId: KaleidoscopeModuleId
}

export function ComingSoonModule({
  moduleId,
}: ComingSoonModuleProps): React.ReactElement {
  const meta = KALEIDOSCOPE_MODULES.find((m) => m.id === moduleId)
  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader group={meta?.group ?? 'asset'} title={meta?.label ?? '模块'} />
      <div className="flex-1 min-h-0 flex items-center justify-center">
        <div className="text-[13px] text-muted-foreground">即将到来 · Phase 2</div>
      </div>
    </div>
  )
}
```

- [ ] **Step 7: tsc 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无输出

- [ ] **Step 8: 提交**

```bash
git add ui/src/views/Kaleidoscope/shared/ModuleHeader.tsx ui/src/views/Kaleidoscope/shared/ModuleHeader.test.tsx ui/src/views/Kaleidoscope/modules/Humans/HumansModule.tsx ui/src/views/Kaleidoscope/modules/ComingSoonModule.tsx
git commit -m "feat(kaleidoscope): add ModuleHeader, HumansModule and ComingSoon placeholder"
```

---

## Task 6: `KaleidoscopeRail`（120px 窄轨导航 + 底部三段）

**Files:**
- Create: `ui/src/views/Kaleidoscope/KaleidoscopeRail.tsx`
- Test: `ui/src/views/Kaleidoscope/KaleidoscopeRail.test.tsx`

- [ ] **Step 1: 写失败测试**

`ui/src/views/Kaleidoscope/KaleidoscopeRail.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
}))

describe('KaleidoscopeRail', () => {
  it('renders all 7 module nav buttons', () => {
    renderWithProviders(<KaleidoscopeRail />)
    for (const label of ['数字人', '应用商店', '我的应用', '产出', '技能', '集成', '记忆']) {
      expect(screen.getByRole('button', { name: new RegExp(label) })).toBeInTheDocument()
    }
  })

  it('clicking a module button updates kaleidoscopeModuleAtom', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<KaleidoscopeRail />, { store })
    await user.click(screen.getByRole('button', { name: /技能/ }))
    expect(store.get(kaleidoscopeModuleAtom)).toBe('skills')
  })

  it('clicking the return button sets topLevelViewAtom back to "workspace"', async () => {
    const store = createStore()
    store.set(topLevelViewAtom, 'kaleidoscope')
    const { user } = renderWithProviders(<KaleidoscopeRail />, { store })
    await user.click(screen.getByRole('button', { name: '返回主窗口' }))
    expect(store.get(topLevelViewAtom)).toBe('workspace')
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeRail.test.tsx`
Expected: FAIL — `Cannot find module './KaleidoscopeRail'`

- [ ] **Step 3: 实现 `KaleidoscopeRail.tsx`**

`ui/src/views/Kaleidoscope/KaleidoscopeRail.tsx`:

```tsx
/**
 * KaleidoscopeRail — 万花筒的 120px 窄轨导航（Arc Library 风格）。
 *
 * 结构（从上至下）：
 *  - 36px 红绿灯让位区
 *  - 资产组：数字人 / 应用商店 / 我的应用 / 产出
 *  - 36px hairline 分组分隔
 *  - 能力组：技能 / 集成 / 记忆
 *  - 1px 分割线
 *  - 44px 行：← 返回主窗口（左）+ ✦ 装饰标识（右，不可交互）
 *  - 1px 分割线
 *  - 48px User / Settings 行（沿用 LeftSidebar 底部规格）
 *
 * 每个模块条目 = lucide 图标 + 中文标签竖排。选中态：资产组用 primary tint，
 * 能力组用 accent tint。全部走 theme token。
 */
import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import {
  ArrowLeft, Sparkles, Settings,
  Bot, Store, LayoutGrid, FileText, Zap, Plug, Brain,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import {
  kaleidoscopeModuleAtom,
  KALEIDOSCOPE_MODULES,
  type KaleidoscopeModuleId,
  type KaleidoscopeGroup,
} from '@/atoms/kaleidoscope'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import { userProfileAtom } from '@/atoms/user-profile'
import { UserAvatar } from '@/components/chat/UserAvatar'

const MODULE_ICON: Record<KaleidoscopeModuleId, LucideIcon> = {
  humans: Bot,
  store: Store,
  apps: LayoutGrid,
  artifacts: FileText,
  skills: Zap,
  integrations: Plug,
  memory: Brain,
}

interface RailItemProps {
  id: KaleidoscopeModuleId
  label: string
  group: KaleidoscopeGroup
  active: boolean
  onSelect: (id: KaleidoscopeModuleId) => void
}

function RailItem({ id, label, group, active, onSelect }: RailItemProps): React.ReactElement {
  const Icon = MODULE_ICON[id]
  return (
    <button
      type="button"
      onClick={() => onSelect(id)}
      aria-current={active ? 'true' : undefined}
      className={cn(
        'titlebar-no-drag flex flex-col items-center gap-1.5 w-[88%] py-2 rounded-[10px]',
        'transition-colors',
        active
          ? group === 'asset'
            ? 'bg-primary/[0.18] border border-primary/35 text-foreground'
            : 'bg-accent/[0.18] border border-accent/35 text-foreground'
          : 'border border-transparent text-muted-foreground hover:bg-muted/30',
      )}
    >
      <Icon className={cn('size-[22px]', !active && 'opacity-70')} aria-hidden />
      <span className="text-[11px] font-semibold leading-none">{label}</span>
    </button>
  )
}

export function KaleidoscopeRail(): React.ReactElement {
  const [moduleId, setModuleId] = useAtom(kaleidoscopeModuleAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const userProfile = useAtomValue(userProfileAtom)

  const assetModules = KALEIDOSCOPE_MODULES.filter((m) => m.group === 'asset')
  const capabilityModules = KALEIDOSCOPE_MODULES.filter((m) => m.group === 'capability')

  return (
    <div className="w-[120px] shrink-0 flex flex-col bg-background border-r border-border">
      {/* 红绿灯让位 */}
      <div className="h-9 shrink-0" />

      {/* 主导航 */}
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col items-center gap-[18px] pt-3">
        {assetModules.map((m) => (
          <RailItem
            key={m.id}
            id={m.id}
            label={m.label}
            group={m.group}
            active={moduleId === m.id}
            onSelect={setModuleId}
          />
        ))}

        {/* 分组分隔 */}
        <div className="w-9 h-px bg-border my-0.5" />

        {capabilityModules.map((m) => (
          <RailItem
            key={m.id}
            id={m.id}
            label={m.label}
            group={m.group}
            active={moduleId === m.id}
            onSelect={setModuleId}
          />
        ))}
      </div>

      {/* ── 底部三段（沿用 chat 窗口结构） ── */}

      {/* 分割线 */}
      <div className="h-px bg-border mx-2.5 shrink-0" />

      {/* ① 返回 + 装饰行 (44px) */}
      <div className="h-11 shrink-0 px-3 flex items-center justify-between">
        <button
          type="button"
          onClick={() => setTopLevelView('workspace')}
          aria-label="返回主窗口"
          className="titlebar-no-drag inline-flex items-center justify-center
                     size-7 rounded-[7px] bg-primary/15 border border-primary/35
                     text-primary hover:bg-primary/25 transition-colors"
        >
          <ArrowLeft className="size-3.5" />
        </button>
        {/* 装饰标识 —— 非交互 */}
        <Sparkles
          className="size-[18px] text-primary/35
                     drop-shadow-[0_0_6px_hsl(var(--primary)/0.35)]"
          aria-hidden
        />
      </div>

      {/* 分割线 */}
      <div className="h-px bg-border mx-2.5 shrink-0" />

      {/* ② User / Settings 行 (48px) */}
      <div className="h-12 shrink-0 px-2 flex items-center">
        <button
          type="button"
          onClick={() => setSettingsOpen(true)}
          className="titlebar-no-drag w-full flex items-center gap-1.5 px-1.5 py-2
                     rounded-[10px] text-foreground/70 hover:bg-foreground/[0.04]
                     hover:text-foreground transition-colors"
        >
          <UserAvatar avatar={userProfile.avatar} size={22} />
          <span className="flex-1 min-w-0 text-[11px] truncate text-left">
            {userProfile.userName}
          </span>
          <Settings className="size-3.5 shrink-0 text-foreground/40" />
        </button>
      </div>
    </div>
  )
}
```

> `userProfileAtom` 从 `@/atoms/user-profile` 引入 —— 与 `LeftSidebar.tsx` 第 369 行同源。`UserAvatar` 从 `@/components/chat/UserAvatar` 引入。若实现时发现 `userProfileAtom` 实际路径不同，以 `LeftSidebar.tsx` 顶部 import 为准（grep `userProfileAtom`）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeRail.test.tsx`
Expected: PASS — 3 tests

- [ ] **Step 5: 提交**

```bash
git add ui/src/views/Kaleidoscope/KaleidoscopeRail.tsx ui/src/views/Kaleidoscope/KaleidoscopeRail.test.tsx
git commit -m "feat(kaleidoscope): add KaleidoscopeRail navigation with grouped modules"
```

---

## Task 7: `KaleidoscopeShell`（rail + 主区组合）

**Files:**
- Create: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`
- Test: `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`

- [ ] **Step 1: 写失败测试**

`ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeShell } from './KaleidoscopeShell'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
}))

// AutomationHub pulls a heavy subtree — stub it; this test only cares that
// KaleidoscopeShell routes the right module into the main area.
vi.mock('@/components/automation/AutomationHub', () => ({
  AutomationHub: () => <div data-testid="automation-hub" />,
}))

describe('KaleidoscopeShell', () => {
  it('renders the rail and the humans module by default', () => {
    renderWithProviders(<KaleidoscopeShell />)
    expect(screen.getByRole('button', { name: /数字人/ })).toBeInTheDocument()
    expect(screen.getByTestId('automation-hub')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a non-humans module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`
Expected: FAIL — `Cannot find module './KaleidoscopeShell'`

- [ ] **Step 3: 实现 `KaleidoscopeShell.tsx`**

`ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`:

```tsx
/**
 * KaleidoscopeShell — 万花筒 surface 的根组件。
 *
 * 布局：左侧 120px KaleidoscopeRail + 右侧主区。主区按 kaleidoscopeModuleAtom
 * 渲染对应模块；Phase 1 只有 humans 是真实实现，其余走 ComingSoonModule。
 *
 * 模块间切换用 motion 的 AnimatePresence 做 80ms slide-fade（与
 * AutomationsView.tsx 的子视图切换同模式）。key={moduleId} 触发重挂载。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { HumansModule } from './modules/Humans/HumansModule'
import { ComingSoonModule } from './modules/ComingSoonModule'

export function KaleidoscopeShell(): React.ReactElement {
  const moduleId = useAtomValue(kaleidoscopeModuleAtom)

  return (
    <div className="flex h-full min-h-0 bg-background">
      <KaleidoscopeRail />
      <div className="flex-1 min-w-0 min-h-0 relative">
        <AnimatePresence mode="wait">
          <motion.div
            key={moduleId}
            initial={{ opacity: 0, x: 12 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.08, ease: [0.32, 0.72, 0, 1] }}
            className="absolute inset-0"
          >
            {moduleId === 'humans' ? (
              <HumansModule />
            ) : (
              <ComingSoonModule moduleId={moduleId} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`
Expected: PASS — 2 tests

- [ ] **Step 5: 提交**

```bash
git add ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx
git commit -m "feat(kaleidoscope): add KaleidoscopeShell composing rail and module area"
```

---

## Task 8: 接入 `MainArea` 顶层 switch + `WorkspaceSwitcherBar` 入口图标

**Files:**
- Modify: `ui/src/components/tabs/MainArea.tsx`
- Modify: `ui/src/components/workspace/WorkspaceSwitcherBar.tsx`
- Test: `ui/src/components/tabs/MainArea.test.tsx`
- Test: `ui/src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx`

- [ ] **Step 1: 写失败测试（MainArea 顶层切换）**

`ui/src/components/tabs/MainArea.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { MainArea } from './MainArea'
import { topLevelViewAtom } from '@/atoms/top-level-view'

// Stub the two surfaces — MainArea's only job is to pick between them.
vi.mock('@/views/Workspace/WorkspaceShell', () => ({
  WorkspaceShell: () => <div data-testid="workspace-shell" />,
}))
vi.mock('@/views/Kaleidoscope/KaleidoscopeShell', () => ({
  KaleidoscopeShell: () => <div data-testid="kaleidoscope-shell" />,
}))
vi.mock('@/components/settings/SettingsDialog', () => ({
  SettingsDialog: () => null,
}))

describe('MainArea — top-level surface switch', () => {
  it('renders WorkspaceShell when topLevelView is "workspace" (default)', () => {
    renderWithProviders(<MainArea />)
    expect(screen.getByTestId('workspace-shell')).toBeInTheDocument()
    expect(screen.queryByTestId('kaleidoscope-shell')).not.toBeInTheDocument()
  })

  it('renders KaleidoscopeShell when topLevelView is "kaleidoscope"', () => {
    const store = createStore()
    store.set(topLevelViewAtom, 'kaleidoscope')
    renderWithProviders(<MainArea />, { store })
    expect(screen.getByTestId('kaleidoscope-shell')).toBeInTheDocument()
    expect(screen.queryByTestId('workspace-shell')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/components/tabs/MainArea.test.tsx`
Expected: FAIL — MainArea 当前只渲染 WorkspaceShell，第二个用例失败（找不到 `kaleidoscope-shell`）

- [ ] **Step 3: 改写 `MainArea.tsx` 接入顶层 switch**

`ui/src/components/tabs/MainArea.tsx` 全文替换为：

```tsx
/**
 * MainArea — 主内容区域
 *
 * 顶层 surface 切换：'workspace'（任务流，WorkspaceShell）↔ 'kaleidoscope'
 * （配置流，KaleidoscopeShell）。两个 surface 用 motion 的 AnimatePresence
 * 做 200ms cross-dissolve（与 AutomationsView.tsx 的子视图切换同模式）。
 * 设置以浮窗形式叠加显示。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Panel } from '@/components/app-shell/Panel'
import { SettingsDialog } from '@/components/settings/SettingsDialog'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { WorkspaceShell } from '@/views/Workspace/WorkspaceShell'
import { KaleidoscopeShell } from '@/views/Kaleidoscope/KaleidoscopeShell'

export function MainArea(): React.ReactElement {
  const topLevelView = useAtomValue(topLevelViewAtom)

  return (
    <>
      <Panel
        variant="grow"
        className="bg-content-area rounded-2xl shadow-xl"
      >
        <div className="relative flex-1 min-h-0 flex flex-col">
          <AnimatePresence mode="wait">
            <motion.div
              key={topLevelView}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2, ease: [0.32, 0.72, 0, 1] }}
              className="absolute inset-0 flex flex-col min-h-0"
            >
              {topLevelView === 'kaleidoscope' ? (
                <KaleidoscopeShell />
              ) : (
                <WorkspaceShell />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </Panel>
      <SettingsDialog />
    </>
  )
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/components/tabs/MainArea.test.tsx`
Expected: PASS — 2 tests

- [ ] **Step 5: 写失败测试（WorkspaceSwitcherBar 入口图标）**

`ui/src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { WorkspaceSwitcherBar } from './WorkspaceSwitcherBar'
import { workspacesAtom, activeWorkspaceIdAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { topLevelViewAtom } from '@/atoms/top-level-view'

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn(),
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
}))

function makeWs(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: 'Folder', path: `/tmp/${id}`, attachedDirs: [], sortOrder: 0,
    createdAt: '2026-05-14T00:00:00Z', updatedAt: '2026-05-14T00:00:00Z',
  }
}

describe('WorkspaceSwitcherBar — Kaleidoscope entry', () => {
  it('renders the Kaleidoscope entry icon', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one')])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithProviders(<WorkspaceSwitcherBar />, { store })
    expect(screen.getByRole('button', { name: '打开万花筒' })).toBeInTheDocument()
  })

  it('clicking the entry icon sets topLevelViewAtom to "kaleidoscope"', async () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one')])
    store.set(activeWorkspaceIdAtom, 'w1')
    const { user } = renderWithProviders(<WorkspaceSwitcherBar />, { store })
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(store.get(topLevelViewAtom)).toBe('kaleidoscope')
  })
})
```

- [ ] **Step 6: 跑测试确认失败**

Run: `cd ui && npx vitest run src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx`
Expected: FAIL — 找不到 `打开万花筒` 按钮

- [ ] **Step 7: 修改 `WorkspaceSwitcherBar.tsx` 插入入口图标**

在 `ui/src/components/workspace/WorkspaceSwitcherBar.tsx` 顶部 import 区追加：

```tsx
import { useSetAtom } from 'jotai'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { KaleidoscopeIcon } from '@/views/Kaleidoscope/KaleidoscopeIcon'
```

> 注意：文件第 30 行已有 `import { useAtomValue, useSetAtom } from 'jotai'` —— 若 `useSetAtom` 已在其中，不要重复 import；只补 `topLevelViewAtom` 与 `KaleidoscopeIcon` 两行。

在 `WorkspaceSwitcherBar` 组件函数体内（第 304 行 `export function WorkspaceSwitcherBar` 之后、现有 `const workspaces = ...` 附近）追加：

```tsx
  const setTopLevelView = useSetAtom(topLevelViewAtom)
```

在 return 的 toolbar 容器里 —— 找到第 484 行：

```tsx
        <div className="flex items-center gap-1.5 px-3 py-2 border-t border-border/40">
          {/* Workspace icons or dots */}
```

在 `{/* Workspace icons or dots */}` 注释之前插入入口图标 + 竖向分隔线：

```tsx
        <div className="flex items-center gap-1.5 px-3 py-2 border-t border-border/40">
          {/* Kaleidoscope 入口 —— 它不是一个 workspace，所以跟 workspace
              dots 之间用一条竖 hairline 隔开。 */}
          <KaleidoscopeIcon
            size={28}
            onClick={() => setTopLevelView('kaleidoscope')}
          />
          <div className="w-px h-[18px] bg-border/60 shrink-0" aria-hidden />

          {/* Workspace icons or dots */}
```

- [ ] **Step 8: 跑测试确认通过**

Run: `cd ui && npx vitest run src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx`
Expected: PASS — 2 tests

- [ ] **Step 9: 全量回归 + tsc**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10 && npm test -- --run 2>&1 | tail -10`
Expected: tsc 无输出；vitest 全绿

- [ ] **Step 10: 提交**

```bash
git add ui/src/components/tabs/MainArea.tsx ui/src/components/tabs/MainArea.test.tsx ui/src/components/workspace/WorkspaceSwitcherBar.tsx ui/src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx
git commit -m "feat(kaleidoscope): wire top-level switch into MainArea and add entry icon"
```

---

## Task 9: 手动验证 + PR

- [ ] **Step 1: 启动 dev 跑通黄金路径**

Run: `cd src-tauri && cargo tauri dev`

手测清单：
- agent 模式下，左栏底部 switcher bar 最左出现彩色小篮子入口图标，与 workspace dots 之间有竖 hairline
- hover 入口图标 → scale 1.06（Phase 1 走静态 SVG fallback，无 Lottie 动画属正常）
- 点击入口图标 → 200ms cross-dissolve 切到万花筒，左侧 120px 窄轨显示 7 个模块（资产 4 + 能力 3，中间 hairline 分组），主区显示「数字人」（现有 AutomationHub 内容）
- 点 Rail 其它 6 个模块 → 主区 80ms slide-fade 切到「即将到来 · Phase 2」占位
- 点 Rail 底部 ← 返回按钮 → cross-dissolve 切回原 chat/agent surface，且 chat/agent 子模式保持切走前的状态
- 点 Rail 底部 User 行 → 打开 Settings 弹窗
- 切到 chat 模式：switcher bar 不渲染（入口图标在 Phase 1 仅 agent 模式可见，属已知范围限制）

- [ ] **Step 2: 主题抽查**

在 Settings 里切到 `warm-paper` 与 `qingye` 两个主题，各自重复进出万花筒一次，确认 rail / 卡片 / 入口图标渐变随主题换色，无写死颜色残留。（完整 11 主题走查留到 Phase 3。）

- [ ] **Step 3: 创建 PR**

```bash
git push -u origin HEAD
gh pr create --title "feat(kaleidoscope): Phase 1 — 万花筒 surface 骨架" --body "$(cat <<'EOF'
## Summary
- 新增 `topLevelViewAtom`（workspace ↔ kaleidoscope）作为最高层视图状态
- `MainArea` 现有全部 surface 分支 move-only 提取到 `WorkspaceShell`
- 新建 `KaleidoscopeShell` + `KaleidoscopeRail`（120px Arc 风窄轨，7 模块分两组）
- `KaleidoscopeIcon` 入口图标（Lottie 包装 + 静态 SVG 兜底，本期 Lottie JSON 未到位走 fallback）
- 1 个真实模块「数字人」（wrap 现有 `AutomationHub`），其余 6 个走 `ComingSoonModule` 占位
- 入口图标接入 `WorkspaceSwitcherBar` 最左

## Spec
`docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`

## 已知范围限制
- 入口图标仅在 agent 模式可见（`WorkspaceSwitcherBar` 本身 agent-only）。`appModeAtom` 默认 agent，覆盖主路径；chat 模式入口留作 Phase 2 评估。
- Lottie 动画文件尚未到位，入口图标走静态 SVG 兜底（hover scale 生效，无篮子摆动 / sparkle 闪烁）。

## Commits (bisectable)
| Commit | What |
|---|---|
| feat(kaleidoscope): add top-level-view and kaleidoscope module atoms | Task 1 |
| refactor(kaleidoscope): extract WorkspaceShell from MainArea (move-only) | Task 2 |
| feat(kaleidoscope): add lottie-react dep and static entry-icon fallback | Task 3 |
| feat(kaleidoscope): add KaleidoscopeIcon with Lottie wrapper and fallback | Task 4 |
| feat(kaleidoscope): add ModuleHeader, HumansModule and ComingSoon placeholder | Task 5 |
| feat(kaleidoscope): add KaleidoscopeRail navigation with grouped modules | Task 6 |
| feat(kaleidoscope): add KaleidoscopeShell composing rail and module area | Task 7 |
| feat(kaleidoscope): wire top-level switch into MainArea and add entry icon | Task 8 |

## Test plan
- [ ] `cd ui && npx tsc --noEmit` 无输出
- [ ] `cd ui && npm test -- --run` 全绿（含新增 6 个测试文件）
- [ ] 手测黄金路径（见 plan Task 9 Step 1）
- [ ] warm-paper / qingye 主题抽查无写死颜色
EOF
)"
```

---

## Self-Review

**Spec coverage（对照 spec 第 12 节 Phase 1 清单）：**
- `topLevelViewAtom` + `kaleidoscopeModuleAtom` → Task 1 ✓
- `WorkspaceShell` 提取 → Task 2 ✓
- `KaleidoscopeShell` + `KaleidoscopeRail` + 底部三段 → Task 6 + Task 7 ✓
- `KaleidoscopeIcon`（Lottie + SVG fallback）→ Task 3 + Task 4 ✓
- 入口图标接入 `WorkspaceSwitcherBar` → Task 8 ✓
- 占位模块 `HumansModule`（wrap `AutomationHub`）→ Task 5 ✓
- 测试：状态切换 + 入口点击 + fallback 渲染 → Task 1/4/6/7/8 测试覆盖 ✓
- spec §6.3 `lottie-react` 进 vendor chunk → Task 3 Step 2（独立 `lottie` chunk，比塞进 vendor 更干净）✓

**Placeholder 扫描：** 无 TBD / TODO / "实现细节略"。每个改代码的 step 都有完整代码块。`ComingSoonModule` 的"即将到来"是产品占位文案、非计划占位，属 Phase 1 有意设计。

**类型一致性：**
- `TopLevelView` / `topLevelViewAtom` —— Task 1 定义，Task 6/8 一致引用
- `KaleidoscopeModuleId` / `KaleidoscopeGroup` / `KaleidoscopeModuleMeta` / `KALEIDOSCOPE_MODULES` / `kaleidoscopeModuleAtom` —— Task 1 定义，Task 5/6/7 一致引用
- `KaleidoscopeIconFallbackProps.size` / `KaleidoscopeIconProps.{animationData,active,onClick,size}` —— Task 3/4 定义，Task 8 用 `size` + `onClick`，一致
- `ModuleHeaderProps.{group,title,subtitle,actions}` —— Task 5 定义，`HumansModule` / `ComingSoonModule` 一致使用
- `ComingSoonModuleProps.moduleId` —— Task 5 定义，Task 7 `KaleidoscopeShell` 一致传入

**待实现者注意的不确定点：**
- `userProfileAtom` 的 import 路径：plan 写的是 `@/atoms/user-profile`，实现时以 `LeftSidebar.tsx` 顶部实际 import 为准（已在 Task 6 注明）。
- `lottie-react` 版本号 `^2.4.1`：若 npm 上最新 major 已变，取当前稳定 major 即可，组件 API（`<Lottie animationData lottieRef autoplay loop />` + `LottieRefCurrentProps`）在 2.x 稳定。

---

## Addendum: Lottie → CSS 入口图标替换（2026-05-14）

**背景：** 用户下载的 Lottie 文件（`space boy developer.json`）评估后不适用 —— 颜色 baked-in 无法适配 11 主题、5s 连续角色动画与 hover 微交互模型不契合、视觉语义不符。决定改为纯 CSS 动画，取消 `lottie-react` 依赖。Tasks 3/4 的 Lottie 部分由下面两个任务取代（spec §6 已同步更新）。

### Task A: 重写 `KaleidoscopeIcon` 为纯 CSS 动画 + 删除 `KaleidoscopeIconFallback`

**Files:**
- Modify: `ui/tailwind.config.js`（加 3 个 keyframes + animation utilities）
- Rewrite: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx`
- Rewrite: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
- Delete: `ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.tsx`
- Delete: `ui/src/views/Kaleidoscope/KaleidoscopeIconFallback.test.tsx`

`tailwind.config.js` —— 在 `theme.extend.keyframes` 追加：

```js
        'kaleido-idle-breath': {
          '0%, 100%': { filter: 'drop-shadow(0 1px 2px hsl(var(--primary) / 0.35))' },
          '50%': { filter: 'drop-shadow(0 1px 2px hsl(var(--primary) / 0.35)) drop-shadow(0 0 8px hsl(var(--primary) / 0.3))' },
        },
        'kaleido-basket-wobble': {
          '0%, 100%': { transform: 'rotate(0deg)' },
          '25%': { transform: 'rotate(-3deg)' },
          '50%': { transform: 'rotate(0deg)' },
          '75%': { transform: 'rotate(3deg)' },
        },
        'kaleido-sparkle-twinkle': {
          '0%, 100%': { transform: 'scale(1)', opacity: '1' },
          '30%': { transform: 'scale(1.4)', opacity: '1' },
          '60%': { transform: 'scale(0.85)', opacity: '0.7' },
        },
```

在 `theme.extend.animation` 追加：

```js
        'kaleido-idle-breath': 'kaleido-idle-breath 3.5s ease-in-out infinite',
        'kaleido-basket-wobble': 'kaleido-basket-wobble 600ms ease-in-out',
        'kaleido-sparkle-twinkle': 'kaleido-sparkle-twinkle 800ms ease-in-out infinite',
```

`KaleidoscopeIcon.tsx` 全文（自包含，内联 SVG，无 Lottie/ErrorBoundary）：

```tsx
/**
 * KaleidoscopeIcon — 万花筒入口图标（WorkspaceSwitcherBar 最左）。
 *
 * 彩色小篮子 + sparkle，纯 SVG + CSS 动画（无 Lottie 依赖）：
 *  - idle：渐变背板 3.5s 一呼一吸的 glow
 *  - hover：整体 scale 1.06，idle glow 停止；篮子 600ms 摆头一次；sparkle 800ms 闪烁循环
 *  - active（身处万花筒 surface）：ring-2 外圈
 *  - 全部 transform/opacity/filter —— GPU 加速；颜色走 theme token，11 主题自适应
 */
import * as React from 'react'
import { cn } from '@/lib/utils'

export interface KaleidoscopeIconProps {
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
  /** 外框边长 px，默认 30。 */
  size?: number
}

/** SVG <g> 的 transform 必须绕自身中心 —— SVG transform-origin 默认是坐标原点。 */
const G_TRANSFORM: React.CSSProperties = {
  transformBox: 'fill-box',
  transformOrigin: 'center',
}

export function KaleidoscopeIcon({
  active = false,
  onClick,
  size = 30,
}: KaleidoscopeIconProps): React.ReactElement {
  const svgSize = Math.round(size * 0.6)
  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      style={{ width: size, height: size }}
      className={cn(
        'group titlebar-no-drag inline-flex items-center justify-center rounded-[8px] shrink-0',
        'bg-gradient-to-br from-primary to-accent',
        'transition-transform duration-200 ease-out',
        'hover:scale-[1.06] active:scale-[0.92]',
        'animate-kaleido-idle-breath hover:animate-none',
        active && 'ring-2 ring-primary/40',
      )}
    >
      <svg
        viewBox="0 0 24 24"
        width={svgSize}
        height={svgSize}
        fill="none"
        className="text-primary-foreground"
        aria-hidden
      >
        {/* basket body —— hover 时 600ms 摆头一次 */}
        <g
          style={G_TRANSFORM}
          className="group-hover:animate-kaleido-basket-wobble"
        >
          <path
            d="M5 10 Q5 9 6 9 H18 Q19 9 19 10 L18 19 Q17.8 20 17 20 H7 Q6.2 20 6 19 Z"
            fill="currentColor"
            opacity="0.95"
          />
          <path d="M5 10 H19" stroke="currentColor" strokeWidth="1.4" opacity="0.6" />
          <ellipse cx="12" cy="9" rx="5.5" ry="0.9" fill="currentColor" opacity="0.4" />
        </g>
        {/* sparkle —— hover 时 800ms 闪烁循环 */}
        <g
          style={G_TRANSFORM}
          className="group-hover:animate-kaleido-sparkle-twinkle"
        >
          <path
            d="M16.5 4 L17.2 5.5 L18.8 6.2 L17.2 6.9 L16.5 8.4 L15.8 6.9 L14.2 6.2 L15.8 5.5 Z"
            fill="currentColor"
          />
          <circle cx="19.5" cy="3.5" r="0.7" fill="currentColor" opacity="0.85" />
        </g>
      </svg>
    </button>
  )
}
```

`KaleidoscopeIcon.test.tsx` 全文（无 lottie mock，测组件契约）：

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { KaleidoscopeIcon } from './KaleidoscopeIcon'

describe('KaleidoscopeIcon', () => {
  it('renders a button with the accessible label', () => {
    render(<KaleidoscopeIcon />)
    expect(screen.getByRole('button', { name: '打开万花筒' })).toBeInTheDocument()
  })

  it('fires onClick when clicked', async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(<KaleidoscopeIcon onClick={onClick} />)
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('applies the active ring only when active', () => {
    const { rerender } = render(<KaleidoscopeIcon active={false} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    expect(btn.className).not.toMatch(/ring-2/)
    rerender(<KaleidoscopeIcon active />)
    expect(btn.className).toMatch(/ring-2/)
  })

  it('honours the size prop', () => {
    render(<KaleidoscopeIcon size={48} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    expect(btn.style.width).toBe('48px')
    expect(btn.style.height).toBe('48px')
  })
})
```

Steps: write the new test → run (old Lottie tests gone, new fail to compile until impl) → update tailwind.config.js → rewrite KaleidoscopeIcon.tsx → `git rm` the two Fallback files → `npx tsc --noEmit` clean → `npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx` 4 pass → commit `feat(kaleidoscope): pure-CSS entry-icon animation, drop Lottie wrapper`.

Note: `WorkspaceSwitcherBar.tsx` already calls `<KaleidoscopeIcon size={28} active={...} onClick={...} />` with NO `animationData` — so it needs **no change**. After Task A, nothing in `src/` imports `lottie-react` except `setup.ts` (cleaned in Task B).

### Task B: 移除 `lottie-react` 依赖 + vite chunk + setup.ts mock

**Files:**
- Modify: `ui/package.json` + `ui/package-lock.json`（`npm uninstall lottie-react`）
- Modify: `ui/vite.config.ts`（删除 `lottie` chunk 块）
- Modify: `ui/src/test-utils/setup.ts`（删除 `vi.mock('lottie-react', …)` 块 + 不再使用的 `createElement` import）

Steps: `cd ui && npm uninstall lottie-react` → 删除 vite.config.ts 里的 `lottie` chunk 判断块（Task 3 加的那段注释 + if）→ 删除 setup.ts 末尾的 lottie-react mock 块和顶部 `import { createElement } from 'react'`（确认 `createElement` 在 setup.ts 别处未使用）→ `npx tsc --noEmit` clean → `npm test -- --run` 全绿（0 失败，无新 error）→ commit `chore(kaleidoscope): remove lottie-react dependency`。

顺序：必须 Task A 先（让 KaleidoscopeIcon 不再 import lottie），再 Task B（卸依赖）。否则 Task B 卸了依赖 Task A 还没改 KaleidoscopeIcon，编译会断。
