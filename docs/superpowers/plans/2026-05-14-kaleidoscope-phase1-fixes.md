# Kaleidoscope Phase 1.1 — 反馈修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 修复 Phase 1 上线后用户反馈的 4 个问题：①入口图标（换 Aperture + motion 动画）②万花筒主区布局塌缩 ③automation 内容完整迁移进万花筒 ④rail 圆角浮卡样式。

**Architecture:** `KaleidoscopeShell` 根 div 缺宽度指令导致整个主区塌缩 —— 修宽度并重构成「rail 浮卡 + 主区浮卡」布局（对齐 chat 窗口的 `shell-bg` + 圆角卡片视觉系统）。入口图标从彩色渐变篮子换成单色 lucide `Aperture`（与 workspace 图标同款），hover 用 `motion` 缓慢旋转。automation 完整迁移：万花筒的 数字人/应用商店/我的应用 3 个模块直接渲染现有 `AutomationHub`/`StoreView`/`AppsTab`（与 `AutomationsView` 的 3 个子视图 1:1），chat 窗口「Automations」入口改为跳转万花筒，`AutomationsView` + `automationPanelOpenAtom` 退役。

**Tech Stack:** React 18 + TS + Jotai + Tailwind + `motion ^12.38.0` + `lucide-react`，Vitest。

**Spec:** `docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`（本计划同步更新 §4.1 / §5.1 / §6 / §7）

---

## 关键事实（实现者必读）

- `AutomationHub` 根 = `<div className="flex flex-col h-full">`；`AppsTab` 根 = `<div className="flex flex-col h-full overflow-y-auto px-6 py-4">`；`StoreView` 根 = `<div className="flex flex-col h-full overflow-hidden">`。三者都 `h-full` —— **必须有确定高度的父容器**。`AutomationsView` 用 `<div className="flex-1 min-h-0 relative"><motion.div className="absolute inset-0">{component}</motion.div></div>` 给它们撑开 —— 迁移模块复刻此模式。
- `automationsSubviewAtom`（在 `@/atoms/marketplace`）值为 `'humans' | 'apps' | 'store' | 'store-detail'`。`StoreView` → `StoreDetail` 的进出靠它。`StoreModule` 需读它来在 StoreView/StoreDetail 间切换。**这个 atom 保留**（StoreModule 依赖）。
- chat 窗口 LeftSidebar 浮卡样式（`LeftSidebar.tsx:918`）：`relative h-full flex flex-col bg-background rounded-2xl shadow-xl`，外层 AppShell 包 `<div className="sidebar-wrapper p-2 pr-0 relative z-[60]">`。
- 现有 `KaleidoscopeIcon` 是彩色渐变 SVG 篮子 + `animate-kaleido-*` tailwind 动画。workspace 图标样式（`WorkspaceSwitcherBar.tsx` 的 `WorkspaceIcon`）：`size-7 rounded-md`，默认 `text-foreground/55`，hover `text-foreground hover:bg-foreground/[0.05]`，active `bg-primary/15 text-primary`，图标 `size-4`。

---

## Task A: KaleidoscopeShell 宽度修复 + 圆角浮卡布局 + rail 样式

**根因：** `KaleidoscopeShell` 根 `<div className="flex h-full min-h-0 bg-background">` 没有宽度指令；`AppShell` 把它放进 `<div className="flex-1 min-w-0 flex">` 当唯一 flex 子节点，但它自己没 `flex-1` → 塌缩成内容宽度 → 右侧 `flex-1` 主区无宽度可撑 → 整体崩坏（"资产"竖排、内容挤成小框都是连带症状）。

**Files:**
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeRail.tsx`
- Modify: `ui/src/components/app-shell/AppShell.tsx`

- [ ] **Step 1: 改 `AppShell.tsx` 的 kaleidoscope 分支**

读 `AppShell.tsx`，找到（约 302 行）：
```tsx
        {topLevelView === 'kaleidoscope' ? (
          <div className="flex-1 min-w-0 flex">
            <KaleidoscopeShell />
          </div>
        ) : (
```
把 wrapper 简化掉，直接渲染 `<KaleidoscopeShell />`（让 KaleidoscopeShell 自己 `flex-1`）：
```tsx
        {topLevelView === 'kaleidoscope' ? (
          <KaleidoscopeShell />
        ) : (
```

- [ ] **Step 2: 改 `KaleidoscopeRail.tsx` 根 div 为圆角浮卡**

读 `KaleidoscopeRail.tsx`，找到根 div（约 86 行）：
```tsx
    <div className="w-[120px] shrink-0 flex flex-col bg-background border-r border-border">
```
改为（圆角 + 阴影，对齐 chat 窗口 LeftSidebar 的 `rounded-2xl shadow-xl`，去掉 `border-r`）：
```tsx
    <div className="w-[120px] shrink-0 flex flex-col bg-background rounded-2xl shadow-xl overflow-hidden">
```
其余内容不动。

- [ ] **Step 3: 重构 `KaleidoscopeShell.tsx` 为「rail 浮卡 + 主区浮卡」布局**

全文替换为：
```tsx
/**
 * KaleidoscopeShell — 万花筒 surface 的根组件。
 *
 * 布局对齐 chat 窗口的 shell-bg 视觉系统：根容器无背景（让 AppShell 的
 * shell-bg 渐变透出 padding 间隙），rail 与主区各自是 rounded-2xl 浮卡。
 *
 * rail 在 p-2 pr-0 包裹里（与 chat 窗口 sidebar-wrapper 同款）；主区在 p-2
 * 包裹里、内层一张 rounded-2xl 卡片。主区按 kaleidoscopeModuleAtom 渲染模块。
 *
 * humaneSpecsAtom 在此加载一次（替代已退役的 AutomationsView 的同名 effect）
 * —— StoreView/StoreGrid 依赖它算"已安装"徽章，用户可能直接进应用商店模块。
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { humaneSpecsAtom } from '@/atoms/automation'
import { listAutomationsHumane } from '@/lib/tauri-bridge'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { HumansModule } from './modules/Humans/HumansModule'
import { StoreModule } from './modules/Store/StoreModule'
import { AppsModule } from './modules/Apps/AppsModule'
import { ComingSoonModule } from './modules/ComingSoonModule'

export function KaleidoscopeShell(): React.ReactElement {
  const moduleId = useAtomValue(kaleidoscopeModuleAtom)
  const setHumaneSpecs = useSetAtom(humaneSpecsAtom)

  // 加载已安装 specs 一次（替代 AutomationsView 退役后丢失的同名 effect）。
  React.useEffect(() => {
    listAutomationsHumane()
      .then(setHumaneSpecs)
      .catch((err) => console.warn('[KaleidoscopeShell] failed to load installed specs:', err))
  }, [setHumaneSpecs])

  return (
    <div className="flex flex-1 min-w-0 min-h-0">
      {/* rail 浮卡 —— p-2 pr-0 对齐 chat 窗口 sidebar-wrapper */}
      <div className="p-2 pr-0 shrink-0">
        <KaleidoscopeRail />
      </div>
      {/* 主区浮卡 */}
      <div className="flex-1 min-w-0 min-h-0 p-2">
        <div className="h-full rounded-2xl shadow-xl bg-content-area overflow-hidden relative">
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
              ) : moduleId === 'store' ? (
                <StoreModule />
              ) : moduleId === 'apps' ? (
                <AppsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </div>
    </div>
  )
}
```
> 注意：本 Step 引入了 `StoreModule` / `AppsModule` 的 import —— 这两个文件由 Task C 创建。**Task A 与 Task C 必须一起完成才能编译通过**（见下方"任务顺序"）。实现 Task A 时如果先做，会有 2 个 import 报错 —— 这是预期的，Task C 补上文件后即解决。建议实现顺序：A 的 Step1-2 → 直接进 Task C 建好 3 个 module → 回来做 A 的 Step3 → 一起验证 → 一起提交。**或**：把 Task A Step3 和 Task C 合并成一个提交。本计划按"合并提交"执行：Task A 与 Task C 共用一个提交，见 Task C 的提交步骤。

- [ ] **Step 4（与 Task C 合并验证 + 提交，见 Task C Step 6）**

---

## Task B: 入口图标换 Aperture + motion hover 旋转动画

**Files:**
- Rewrite: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx`
- Rewrite: `ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
- Modify: `ui/tailwind.config.js`（删除 3 个 `kaleido-*` keyframes + animation）
- Modify: `ui/src/components/workspace/WorkspaceSwitcherBar.tsx`（调用点去掉 `size`）

- [ ] **Step 1: 写新测试**

`ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx` 全文替换：
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

  it('applies the active style only when active', () => {
    const { rerender } = render(<KaleidoscopeIcon active={false} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    expect(btn.className).not.toMatch(/bg-primary\/15/)
    rerender(<KaleidoscopeIcon active />)
    expect(btn.className).toMatch(/bg-primary\/15/)
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
Expected: FAIL（旧组件的 active 态是 `ring-2 ring-primary/40` 不是 `bg-primary/15`，第 3 个用例失败）

- [ ] **Step 3: 重写 `KaleidoscopeIcon.tsx`**

全文替换：
```tsx
/**
 * KaleidoscopeIcon — 万花筒入口图标（WorkspaceSwitcherBar 最左）。
 *
 * 单色 lucide Aperture（光圈），与 workspace 图标同款处理（size-7 rounded-md、
 * 默认 text-foreground/55、hover 提亮+底色、active 主色 tint）。
 * hover 时用 motion 让光圈缓慢旋转（万花筒转动的隐喻），离开平滑归位。
 * 无常驻动画、无新依赖（motion 已在栈内）。
 */
import * as React from 'react'
import { Aperture } from 'lucide-react'
import { motion } from 'motion/react'
import { cn } from '@/lib/utils'

export interface KaleidoscopeIconProps {
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
}

export function KaleidoscopeIcon({
  active = false,
  onClick,
}: KaleidoscopeIconProps): React.ReactElement {
  const [hovered, setHovered] = React.useState(false)
  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={cn(
        'titlebar-no-drag relative inline-flex items-center justify-center',
        'size-7 rounded-md transition-colors shrink-0',
        active
          ? 'bg-primary/15 text-primary'
          : 'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.05]',
      )}
    >
      <motion.span
        className="inline-flex"
        animate={{ rotate: hovered ? 360 : 0 }}
        transition={
          hovered
            ? { repeat: Infinity, duration: 2.6, ease: 'linear' }
            : { duration: 0.4, ease: 'easeOut' }
        }
      >
        <Aperture className="size-4" aria-hidden />
      </motion.span>
    </button>
  )
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx`
Expected: PASS — 3 tests

- [ ] **Step 5: 删除 `tailwind.config.js` 里的 `kaleido-*` keyframes**

读 `ui/tailwind.config.js`。在 `theme.extend.keyframes` 里删除这 3 个键：`'kaleido-idle-breath'`、`'kaleido-basket-wobble'`、`'kaleido-sparkle-twinkle'`（连同它们的值对象）。在 `theme.extend.animation` 里删除对应的 3 行：`'kaleido-idle-breath': ...`、`'kaleido-basket-wobble': ...`、`'kaleido-sparkle-twinkle': ...`。保留所有其它既有的 keyframes/animation（`slide-in-from-top` 等）。

- [ ] **Step 6: 改 `WorkspaceSwitcherBar.tsx` 调用点**

读 `WorkspaceSwitcherBar.tsx`，找到（约 491 行）：
```tsx
          <KaleidoscopeIcon
            size={28}
            active={topLevelView === 'kaleidoscope'}
            onClick={() => setTopLevelView('kaleidoscope')}
          />
```
去掉 `size={28}` 那行（新图标固定 `size-7`，与 workspace 图标一致）：
```tsx
          <KaleidoscopeIcon
            active={topLevelView === 'kaleidoscope'}
            onClick={() => setTopLevelView('kaleidoscope')}
          />
```

- [ ] **Step 7: tsc + 提交**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` — 必须干净
Run: `cd ui && npx vitest run src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx src/components/workspace/WorkspaceSwitcherBar.kaleidoscope.test.tsx` — 全过

```bash
git add ui/src/views/Kaleidoscope/KaleidoscopeIcon.tsx ui/src/views/Kaleidoscope/KaleidoscopeIcon.test.tsx ui/tailwind.config.js ui/src/components/workspace/WorkspaceSwitcherBar.tsx
git commit -m "feat(kaleidoscope): replace entry icon with Aperture glyph + motion hover spin"
```

---

## Task C: automation 完整迁移（Humans / Store / Apps 模块）

把万花筒的 数字人/应用商店/我的应用 3 个模块接到现有 `AutomationHub`/`StoreView`+`StoreDetail`/`AppsTab`。迁移后的模块**不套 `ModuleHeader`** —— 这些 automation 组件自带 header/controls，rail 已经是跨模块导航，再叠一个 ModuleHeader 会双 header（图1 已暴露）。`ModuleHeader` 保留给 `ComingSoonModule`（其余 4 个未做模块）继续用。

**Files:**
- Rewrite: `ui/src/views/Kaleidoscope/modules/Humans/HumansModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Store/StoreModule.tsx`
- Create: `ui/src/views/Kaleidoscope/modules/Apps/AppsModule.tsx`
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`（已在 Task A Step 3 写好，含 Store/Apps import）
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.test.tsx`

- [ ] **Step 1: 重写 `HumansModule.tsx`**

全文替换（去掉 ModuleHeader，用 `relative` + `absolute inset-0` 给 `h-full` 的 AutomationHub 撑出确定高度）：
```tsx
/**
 * HumansModule — 万花筒「数字人」模块。
 *
 * 渲染现有 AutomationHub（原 AutomationsView 的 'humans' 子视图）。
 * AutomationHub 根是 h-full，需确定高度的父容器 —— 用 relative + absolute
 * inset-0（复刻 AutomationsView 的撑高方式）。AutomationHub 自带 header，
 * 不再叠 ModuleHeader。
 */
import * as React from 'react'
import { AutomationHub } from '@/components/automation/AutomationHub'

export function HumansModule(): React.ReactElement {
  return (
    <div className="absolute inset-0">
      <AutomationHub />
    </div>
  )
}
```
> KaleidoscopeShell 的主区卡片已是 `relative`（见 Task A Step 3），module 用 `absolute inset-0` 即可铺满。

- [ ] **Step 2: 创建 `StoreModule.tsx`**

`ui/src/views/Kaleidoscope/modules/Store/StoreModule.tsx`:
```tsx
/**
 * StoreModule — 万花筒「应用商店」模块。
 *
 * 渲染现有 StoreView；当 automationsSubviewAtom 进入 'store-detail' 时渲染
 * StoreDetail（复刻原 AutomationsView 的 store/store-detail 切换）。
 * StoreDetail 的"返回"会把 automationsSubviewAtom 设回 'store'。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { automationsSubviewAtom } from '@/atoms/marketplace'
import { StoreView } from '@/components/automation/StoreView'
import { StoreDetail } from '@/components/automation/StoreDetail'

export function StoreModule(): React.ReactElement {
  const subview = useAtomValue(automationsSubviewAtom)
  return (
    <div className="absolute inset-0">
      {subview === 'store-detail' ? <StoreDetail /> : <StoreView />}
    </div>
  )
}
```

- [ ] **Step 3: 创建 `AppsModule.tsx`**

`ui/src/views/Kaleidoscope/modules/Apps/AppsModule.tsx`:
```tsx
/**
 * AppsModule — 万花筒「我的应用」模块。
 *
 * 渲染现有 AppsTab（原 AutomationsView 的 'apps' 子视图）。AppsTab 根是
 * h-full overflow-y-auto，用 absolute inset-0 撑出确定高度。
 */
import * as React from 'react'
import { AppsTab } from '@/components/automation/AppsTab'

export function AppsModule(): React.ReactElement {
  return (
    <div className="absolute inset-0">
      <AppsTab />
    </div>
  )
}
```

- [ ] **Step 4: 完成 Task A Step 3**

如果还没做 Task A Step 3（重写 `KaleidoscopeShell.tsx`），现在做 —— 它的 import 现在能解析了（Store/Apps module 已创建）。

- [ ] **Step 5: 更新 `KaleidoscopeShell.test.tsx`**

全文替换（新增 store/apps 路由测试，mock 3 个 automation 组件 + tauri-bridge）：
```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeShell } from './KaleidoscopeShell'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
  listAutomationsHumane: vi.fn().mockResolvedValue([]),
}))

// automation 组件子树重 —— stub 掉，本测试只关心 KaleidoscopeShell 的模块路由。
vi.mock('@/components/automation/AutomationHub', () => ({
  AutomationHub: () => <div data-testid="automation-hub" />,
}))
vi.mock('@/components/automation/StoreView', () => ({
  StoreView: () => <div data-testid="store-view" />,
}))
vi.mock('@/components/automation/StoreDetail', () => ({
  StoreDetail: () => <div data-testid="store-detail" />,
}))
vi.mock('@/components/automation/AppsTab', () => ({
  AppsTab: () => <div data-testid="apps-tab" />,
}))

describe('KaleidoscopeShell', () => {
  it('renders the rail and the humans module (AutomationHub) by default', () => {
    renderWithProviders(<KaleidoscopeShell />)
    expect(screen.getByRole('button', { name: /数字人/ })).toBeInTheDocument()
    expect(screen.getByTestId('automation-hub')).toBeInTheDocument()
  })

  it('renders StoreView for the store module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'store')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('store-view')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })

  it('renders AppsTab for the apps module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'apps')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('apps-tab')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a not-yet-built module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 6: 验证 + 提交（Task A + Task C 合并提交）**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15` — 必须干净
Run: `cd ui && npx vitest run src/views/Kaleidoscope/ 2>&1 | tail -10` — 万花筒目录全部测试通过
Run: `cd ui && npm test -- --run 2>&1 | tail -8` — 全量：0 失败，无新 error（baseline ~104 文件 / ~595 测试 / ~10 个 pre-existing error）

```bash
git add ui/src/views/Kaleidoscope/ ui/src/components/app-shell/AppShell.tsx
git commit -m "feat(kaleidoscope): full-window layout fix + migrate automation views into modules"
```
> 这一个提交同时包含 Task A（布局/样式）与 Task C（迁移）—— 因为 KaleidoscopeShell.tsx 同时被两者改、且 import 互相依赖，拆开无法编译。提交信息覆盖两者。

---

## Task D: chat 窗口入口改跳转 + WorkspaceShell 移除 automation 分支 + 清理

**Files:**
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx`
- Modify: `ui/src/views/Workspace/WorkspaceShell.tsx`
- Modify: `ui/src/atoms/automation.ts`
- Delete: `ui/src/views/AutomationsView.tsx`（迁移后无人引用 —— Step 4 grep 确认）

- [ ] **Step 1: 改 `LeftSidebar.tsx` 的 Automations 入口**

读 `LeftSidebar.tsx`。改动点：
1. import：删除 `import { automationPanelOpenAtom } from '@/atoms/automation'`（约 74 行）；新增 `import { topLevelViewAtom } from '@/atoms/top-level-view'` 和 `import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'`。
2. 组件体内：删除 `const [automationPanelOpen, setAutomationPanelOpen] = useAtom(automationPanelOpenAtom)`（约 357 行）；新增 `const setTopLevelView = useSetAtom(topLevelViewAtom)` 和 `const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)`（`useSetAtom` 已 import）。
3. Automations 按钮（约 1055 行）：`onClick={() => setAutomationPanelOpen(true)}` → `onClick={() => { setKaleidoscopeModule('humans'); setTopLevelView('kaleidoscope') }}`。按钮其它属性（className/title/`<Bot>` 图标/文案）不动。
4. 删除底部的 ESLint hack（约 1104-1108 行）：
   ```tsx
   {/* Automation Hub is rendered by MainArea ... */}
   {void automationPanelOpen}
   ```
   整块删掉（`automationPanelOpen` 变量已不存在）。
5. 顺手修正 stale 注释（约 72 行）`// AutomationHub is rendered by MainArea (see atoms/automation::automationPanelOpenAtom)` —— 删掉这行或改为 `// Kaleidoscope surface is rendered by AppShell (see atoms/top-level-view)`.

- [ ] **Step 2: 改 `WorkspaceShell.tsx` 移除 automation 分支**

读 `WorkspaceShell.tsx`。改动点：
1. import：删除 `import { automationPanelOpenAtom } from '@/atoms/automation'`（约 27 行）和 `import { AutomationsView } from '@/views/AutomationsView'`（约 28 行）。
2. 删除 `const [automationOpen, setAutomationOpen] = useAtom(automationPanelOpenAtom)`（约 44 行）。
3. 删除整个 auto-close effect（约 48-60 行，那段 `// Auto-close AutomationsView when the user picks a different session/tab ...` + `React.useEffect(() => { if (prevTabIdRef.current !== activeTabId && automationOpen) {...} }, [...])`）连同 `prevTabIdRef`（如果 `prevTabIdRef` 只被这个 effect 用 —— grep 确认；只此一处则一并删）。
4. 删除渲染分支 `if (automationOpen) return <AutomationsView />`（约 170 行）。
5. 确认 `useAtom` 如果因此变成未使用则从 jotai import 里移除（grep `useAtom(` 在文件内还有没有别的用处 —— `previewPanelSplitRatioAtom` 等可能还在用 `useAtom`，多半还需要，谨慎处理）。

- [ ] **Step 3: 从 `atoms/automation.ts` 删除 `automationPanelOpenAtom`**

读 `ui/src/atoms/automation.ts`。先 `grep -rn "automationPanelOpenAtom" ui/src` 确认除了已改的 LeftSidebar/WorkspaceShell 再无引用。确认后删除 `export const automationPanelOpenAtom = atom(false)` 那行。其它 atom（`humaneSpecsAtom` 等）保留 —— 它们仍被 KaleidoscopeShell / automation 组件使用。

- [ ] **Step 4: 删除 `AutomationsView.tsx`**

`grep -rn "AutomationsView" ui/src` —— 确认 Step 1/2 改完后已无任何 import（注释里提到不算）。确认后：
```bash
git rm ui/src/views/AutomationsView.tsx
```
> 若 grep 仍有引用，STOP 并报告 —— 说明有遗漏的消费点。

- [ ] **Step 5: 验证**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15` — 必须干净（最容易在这步暴露遗漏的 import / 未使用变量）
Run: `cd ui && npm test -- --run 2>&1 | tail -8` — 0 失败；测试数可能比 baseline 少（删了 AutomationsView 如果它有测试 —— grep `AutomationsView.test` 确认有没有；有的话一并 `git rm`）；无新 error

- [ ] **Step 6: 提交**

```bash
git add ui/src/components/app-shell/LeftSidebar.tsx ui/src/views/Workspace/WorkspaceShell.tsx ui/src/atoms/automation.ts ui/src/views/AutomationsView.tsx
git commit -m "refactor(kaleidoscope): retire AutomationsView, chat entry now opens Kaleidoscope"
```

---

## Task E: spec 同步 + 最终验证

**Files:**
- Modify: `docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`

- [ ] **Step 1: 更新 spec**

读 spec，按实际落地改：
- **§6（入口图标）**：改为「单色 lucide `Aperture`，与 workspace 图标同款处理（`size-7 rounded-md`），hover 用 `motion` 缓慢旋转，离开平滑归位；无常驻动画、无 tailwind keyframes」。删掉之前 CSS keyframes 那套描述（idle-breath/basket-wobble/sparkle-twinkle 已从 tailwind.config.js 移除）。
- **§5.1（KaleidoscopeShell 布局）**：补充「rail 与主区各为 rounded-2xl shadow-xl 浮卡，根容器无背景让 shell-bg 渐变透出 padding 间隙，对齐 chat 窗口视觉系统」。
- **§4.1**：`automationPanelOpenAtom` 已删除；chat 窗口「Automations」入口改为 `setTopLevelView('kaleidoscope') + setKaleidoscopeModule('humans')`；`AutomationsView.tsx` 已退役删除。
- **§7.1 / §7.2 / §7.3**：数字人/应用商店/我的应用 3 个模块 Phase 1.1 已完成迁移 —— 直接渲染 `AutomationHub` / `StoreView`+`StoreDetail` / `AppsTab`，不套 `ModuleHeader`（automation 组件自带 header）。
- 顶部加一行变更说明：`> 设计变更（2026-05-14 Phase 1.1）：入口图标换 Aperture+motion；automation 三模块完成迁移；rail 改圆角浮卡。详见 plans/2026-05-14-kaleidoscope-phase1-fixes.md`

- [ ] **Step 2: 最终全量验证**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` — 干净
Run: `cd ui && npm test -- --run 2>&1 | tail -8` — 0 失败，无新 error
Run: `cd ui && npx vitest run src/views/Kaleidoscope/ src/components/workspace/ 2>&1 | tail -6` — 万花筒 + workspace 相关全过

- [ ] **Step 3: 提交**

```bash
git add docs/superpowers/specs/2026-05-14-kaleidoscope-design.md
git commit -m "docs(kaleidoscope): spec sync for Phase 1.1 feedback fixes"
```

- [ ] **Step 4: 手动验证清单（交用户跑 `cargo tauri dev`）**

- agent 模式底栏最左是单色 Aperture 图标（不再是彩色篮子），平时静止，hover 缓慢旋转
- 点击 → 全窗口万花筒，**主区正常铺满**（不再塌缩），rail 是圆角浮卡、主区也是圆角浮卡，间隙透出 shell-bg
- 数字人 / 应用商店 / 我的应用 3 个模块分别正常显示 AutomationHub / StoreView / AppsTab 内容（铺满、不挤）
- 应用商店里点一个应用 → StoreDetail；返回 → StoreView
- 其它 4 个模块（产出/技能/集成/记忆）→ "即将到来 · Phase 2"
- chat 窗口左栏「Automations」按钮 → 跳转万花筒（数字人模块）
- 点 ← 返回 → 回 workspace
- warm-paper / qingye 两主题各扫一眼

---

## 任务顺序

A（Step 1-2）→ C（建 3 个 module + 完成 A Step 3 + 测试）→ **A+C 合并提交** → B → D → E。
A 与 C 因 `KaleidoscopeShell.tsx` 互相依赖，合并为一个提交。B 独立。D 依赖 C（迁移完成后才能退役 AutomationsView）。E 最后。

## Self-Review

- **覆盖反馈 4 点**：① Task B（Aperture+motion）② Task A（宽度修复）③ Task C+D（完整迁移）④ Task A（rail 圆角）✓
- **Placeholder 扫描**：无 TBD；每个改代码的 step 给了完整代码或精确改动描述 ✓
- **类型一致性**：`KaleidoscopeIconProps` 去掉 `size`（Task B）→ `WorkspaceSwitcherBar` 调用点同步去掉 `size`（Task B Step 6）✓；`StoreModule`/`AppsModule` 导出名与 `KaleidoscopeShell` import 一致 ✓
- **编译依赖**：Task A Step 3 的 import 依赖 Task C 的文件 —— 已明确要求 A+C 合并提交，不会留下不可编译的中间态 ✓
