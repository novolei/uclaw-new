# Focus Mode 设计文档

**日期**：2026-05-13
**作者**：uClaw 团队
**状态**：草案 → 待 review

## 目的

当用户在 preview 面板里专注阅读 / 编辑文件时，提供"专注模式"：动画隐藏 LeftSidebar 与 RightSidePanel，把屏幕让给 preview。需要时把鼠标移到屏幕左右边缘可以临时召回任一侧栏（浮岛形式），鼠标离开后自动收起。

设计原则：
- **保留现有 LeftSidebar / RightSidePanel 的设计与代码**。只换"如何显示它"。
- **零后端改动**。纯前端 UI 状态。
- **多主题友好**。辉光颜色随 11 个主题协调。

---

## 用户故事

1. **进入专注**：用户打开一个 markdown / 代码文件预览，按 `Alt+F` 或点 PreviewHeader 上的 Focus 图标 → LeftSidebar 与 RightSidePanel 滑出屏外，preview 区域横向延展。
2. **召回左栏**：用户鼠标靠近屏幕左边缘 → 边缘浮现一条柔和辉光提示"这里可以召唤"→ 鼠标贴近 ≤32px → LeftSidebar 从左滑入，呈现为带圆角 + 阴影 + 磨砂的"浮岛"，悬于 preview 之上。
3. **召回右栏**：同上，右侧边缘 → RightSidePanel 浮岛。
4. **自动收起**：鼠标离开浮岛区域 200ms 后自动滑回屏外。
5. **钉住**：用户在浮岛内点击任意元素（如选会话、点击 tab）→ 浮岛钉住，鼠标走开也不会收起；点击浮岛外才解钉。
6. **退出专注**：再按 `Alt+F` / 再点按钮 / 关闭最后一个 preview tab → 侧栏滑回原位，专注模式退出。

---

## 架构

### 状态层（jotai）

新文件 `ui/src/atoms/focus-mode-atoms.ts`：

```ts
export const focusModeAtom = atom<boolean>(false)
export const focusRevealSideAtom = atom<'left' | 'right' | null>(null)
export const focusRevealPinnedAtom = atom<boolean>(false)

// Actions
export const toggleFocusModeAction = atom(null, (get, set) => {
  const next = !get(focusModeAtom)
  set(focusModeAtom, next)
  // 退出时清理 reveal 状态
  if (!next) {
    set(focusRevealSideAtom, null)
    set(focusRevealPinnedAtom, false)
  }
})

export const exitFocusModeAction = atom(null, (_get, set) => {
  set(focusModeAtom, false)
  set(focusRevealSideAtom, null)
  set(focusRevealPinnedAtom, false)
})
```

**作用域**：全局。整个 app 共享一个 focusMode 状态，切换工作区不重置（但若新工作区无 preview，autoExit 会自动把 focusMode 改为 false）。

### 钩子层（hooks/）

#### `useFocusModeShortcut`

```ts
export function useFocusModeShortcut(): void {
  const toggle = useSetAtom(toggleFocusModeAction)
  useShortcut({
    id: 'toggle-focus-mode',
    handler: () => toggle(),
    // preventDefault 默认 true（useShortcut.ts:80），自动阻止 Mac Option+F 插入 ƒ 字符。
    // useShortcut 不做 editable-skip，所以 Alt+F 在编辑器内也能触发 —— 这正是我们要的。
  })
}
```

无需扩展 `useShortcut` —— 现有 API 已满足需求。

#### `useFocusModeHotzone`

```ts
export function useFocusModeHotzone(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const reveal = useAtomValue(focusRevealSideAtom)
  const pinned = useAtomValue(focusRevealPinnedAtom)
  const setReveal = useSetAtom(focusRevealSideAtom)
  const setMouse = useSetAtom(focusMousePosAtom)  // for glow opacity

  // ref：当前是否在 island 区域内
  const isInsideIslandRef = useRef(false)
  const leaveTimerRef = useRef<number | null>(null)

  useEffect(() => {
    if (!focusMode) return
    const HOT_ZONE = 32
    const TOP_EXCLUDE = 84  // 50px titlebar + 34px TabBar

    const onMove = (e: MouseEvent) => {
      setMouse({ x: e.clientX, y: e.clientY })

      if (pinned) return
      if (e.clientY < TOP_EXCLUDE) return

      const w = window.innerWidth
      const inLeftZone  = e.clientX <= HOT_ZONE
      const inRightZone = e.clientX >= w - HOT_ZONE

      // 检查是否在已展开的浮岛区域内（含 hot zone 联合区）
      const inLeftIsland  = isInsideIslandRect('left', e.clientX, e.clientY)
      const inRightIsland = isInsideIslandRect('right', e.clientX, e.clientY)

      const shouldRevealLeft  = inLeftZone || inLeftIsland
      const shouldRevealRight = inRightZone || inRightIsland

      if (shouldRevealLeft) {
        if (reveal !== 'left') setReveal('left')
        clearLeaveTimer()
      } else if (shouldRevealRight) {
        if (reveal !== 'right') setReveal('right')
        clearLeaveTimer()
      } else if (reveal !== null) {
        // 离开了浮岛与 hot zone 联合区 → 启动 200ms timer
        startLeaveTimer(() => setReveal(null))
      }
    }

    window.addEventListener('mousemove', onMove)
    return () => {
      window.removeEventListener('mousemove', onMove)
      clearLeaveTimer()
    }
  }, [focusMode, reveal, pinned])
}
```

辅助：`isInsideIslandRect(side, x, y)` 根据浮岛的固定布局（width、margin）计算包围盒。包围盒 = 浮岛 box ∪ 该侧 hot zone（让用户能从浮岛走到边再走回）。

`focusMousePosAtom: { x: number; y: number }` 用来驱动辉光的距离响应与 Y 轴 trace。

#### `useFocusModeAutoExit`

```ts
export function useFocusModeAutoExit(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const exit = useSetAtom(exitFocusModeAction)

  useEffect(() => {
    if (!focusMode) return
    if (!previewOpen) exit()
  }, [focusMode, previewOpen, exit])
}
```

`previewPanelOpenAtom`（`preview-panel-atoms.ts`）是 preview 面板的开关信号；关闭最后一个 preview 或切换工作区都会把它置为 false，autoExit 据此触发。

### 组件层（components/focus-mode/）

#### `FocusModeOverlay`

挂在 `AppShell.tsx` 顶层。三件事：
1. 渲染两个 `<FloatingIsland side="left|right" />`，children 为 `<LeftSidebar />` / `<RightSidePanel />`
2. 渲染两条 `<GlowIndicator side="left|right" />`
3. 装载 `useFocusModeHotzone()` 与 `useFocusModeAutoExit()` 副作用

```tsx
export function FocusModeOverlay(): React.ReactElement | null {
  const focusMode = useAtomValue(focusModeAtom)
  useFocusModeHotzone()
  useFocusModeAutoExit()

  if (!focusMode) return null

  return (
    <>
      <GlowIndicator side="left" />
      <GlowIndicator side="right" />
      <FloatingIsland side="left">
        <LeftSidebar />
      </FloatingIsland>
      <FloatingIsland side="right">
        <RightSidePanel />
      </FloatingIsland>
    </>
  )
}
```

#### `FloatingIsland`

```tsx
interface Props {
  side: 'left' | 'right'
  children: React.ReactNode
}

export function FloatingIsland({ side, children }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const setPinned = useSetAtom(focusRevealPinnedAtom)
  const islandRef = useRef<HTMLDivElement>(null)
  const visible = reveal === side

  // 点击内部 → 钉住；点击外部 → 解钉
  useEffect(() => {
    if (!visible) return
    const onDocClick = (e: MouseEvent) => {
      const t = e.target as Element
      // Radix portal 节点不算外部（toast / dropdown 等）
      if (t.closest('[data-radix-portal]')) return
      if (islandRef.current?.contains(t)) {
        setPinned(true)
      } else {
        setPinned(false)
      }
    }
    document.addEventListener('click', onDocClick, true)  // capture
    return () => document.removeEventListener('click', onDocClick, true)
  }, [visible, setPinned])

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          ref={islandRef}
          custom={side}
          variants={islandVariants}
          initial="hidden"
          animate="shown"
          exit="hidden"
          transition={{ duration: 0.26, ease: [0.32, 0.72, 0, 1] }}
          className={cn(
            'fixed top-3 bottom-3 z-[80]',
            side === 'left' ? 'left-3 w-[280px]' : 'right-3 w-[400px]',
            'rounded-xl bg-popover/96 backdrop-blur-md overflow-hidden',
            'shadow-[0_1px_3px_rgba(0,0,0,0.10),0_12px_36px_-8px_rgba(0,0,0,0.25),0_0_0_1px_hsl(var(--border)/0.4)]',
          )}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  )
}

const islandVariants: Variants = {
  hidden: (side: 'left' | 'right') => ({
    x: side === 'left' ? 'calc(-100% - 12px)' : 'calc(100% + 12px)',
    opacity: 0,
    scale: 0.96,
  }),
  shown: { x: 0, opacity: 1, scale: 1 },
}
```

#### `GlowIndicator`

```tsx
interface Props { side: 'left' | 'right' }

export function GlowIndicator({ side }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const mouse = useAtomValue(focusMousePosAtom)
  const isRevealed = reveal === side

  // 距离响应的总体 opacity
  const dist = side === 'left'
    ? mouse.x
    : window.innerWidth - mouse.x
  const proximityOpacity = dist > 80 ? 0 : dist < 16 ? 1 : 1 - (dist - 16) / 64
  const overallOpacity = isRevealed ? 0 : proximityOpacity

  return (
    <motion.div
      aria-hidden
      animate={{ opacity: overallOpacity }}
      transition={{ duration: 0.15 }}
      className={cn(
        'fixed top-0 bottom-0 z-[79] pointer-events-none w-1',
        side === 'left' ? 'left-0' : 'right-0',
      )}
      style={{ '--g': 'hsl(var(--focus-glow))' } as React.CSSProperties}
    >
      {/* 外晕 halo（56px ellipse） */}
      <div className={cn('focus-glow-halo', side === 'right' && 'focus-glow-halo-right')} />
      {/* 软光 soft（24px blurred） */}
      <div className={cn('focus-glow-soft', side === 'right' && 'focus-glow-soft-right')} />
      {/* 内核 core（2px crisp） */}
      <div className="focus-glow-core" />
      {/* Y 轴 trace 高亮（跟随鼠标） */}
      <div
        className={cn('focus-glow-trace', side === 'right' && 'focus-glow-trace-right')}
        style={{ transform: `translateY(${mouse.y}px)` }}
      />
    </motion.div>
  )
}
```

CSS @keyframes 与样式落在 `globals.css`：

```css
.focus-glow-core {
  position: absolute; inset: 0;
  background: linear-gradient(to bottom,
    transparent 0%, var(--g) 18%, var(--g) 82%, transparent 100%);
  border-radius: 2px; filter: blur(0.5px);
  animation: focus-glow-breathe-core 2.4s ease-in-out infinite;
}
.focus-glow-soft {
  position: absolute; top: 0; bottom: 0; left: -8px; width: 24px;
  background: linear-gradient(to bottom,
    transparent 0%, var(--g) 25%, var(--g) 75%, transparent 100%);
  filter: blur(10px); opacity: 0.55;
  animation: focus-glow-breathe-soft 2.4s ease-in-out infinite;
}
.focus-glow-soft-right { left: auto; right: -8px; }
.focus-glow-halo {
  position: absolute; top: 0; bottom: 0; left: -16px; width: 56px;
  background: radial-gradient(ellipse at left center, var(--g) 0%, transparent 70%);
  filter: blur(8px); opacity: 0.28;
  animation: focus-glow-breathe-halo 2.4s ease-in-out infinite;
}
.focus-glow-halo-right {
  left: auto; right: -16px;
  background: radial-gradient(ellipse at right center, var(--g) 0%, transparent 70%);
}
.focus-glow-trace {
  position: absolute; left: -4px; top: 0; width: 12px; height: 80px;
  margin-top: -40px;
  background: radial-gradient(ellipse at left center,
    hsl(var(--focus-glow-bright)) 0%, transparent 65%);
  filter: blur(6px); opacity: 0.75;
  will-change: transform;
  transition: transform 0.06s linear;
}
.focus-glow-trace-right {
  left: auto; right: -4px;
  background: radial-gradient(ellipse at right center,
    hsl(var(--focus-glow-bright)) 0%, transparent 65%);
}
@keyframes focus-glow-breathe-core { 0%,100%{opacity:.85} 50%{opacity:1} }
@keyframes focus-glow-breathe-soft { 0%,100%{opacity:.45} 50%{opacity:.65} }
@keyframes focus-glow-breathe-halo { 0%,100%{opacity:.22} 50%{opacity:.34} }
```

#### `FocusModeButton`

挂在 `PreviewHeader.tsx`，位于现有 Copy/Reveal/Close 三件套**左侧**：

```tsx
export function FocusModeButton(): React.ReactElement {
  const focusMode = useAtomValue(focusModeAtom)
  const toggle = useSetAtom(toggleFocusModeAction)
  return (
    <button
      type="button"
      onClick={() => toggle()}
      title={focusMode ? '退出专注模式 (Alt+F)' : '进入专注模式 (Alt+F)'}
      aria-label={focusMode ? '退出专注模式' : '进入专注模式'}
      className="size-6 inline-flex items-center justify-center rounded-md
                 text-muted-foreground hover:text-foreground
                 hover:bg-foreground/[0.06] transition-colors"
    >
      {focusMode ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
    </button>
  )
}
```

### 主题色 token

在 `ui/src/styles/globals.css` 的每个主题块新增两行：

```css
:root {
  /* ... 现有 token ... */
  --focus-glow: 200 80% 55%;
  --focus-glow-bright: 200 90% 65%;
}
.dark {
  --focus-glow: 200 85% 65%;
  --focus-glow-bright: 200 95% 75%;
}
.theme-ocean-light  { --focus-glow: 205 50% 50%; --focus-glow-bright: 205 70% 60%; }
.theme-ocean-dark   { --focus-glow: 205 70% 58%; --focus-glow-bright: 205 85% 68%; }
.theme-forest-light { --focus-glow: 150 35% 38%; --focus-glow-bright: 150 55% 48%; }
.theme-forest-dark  { --focus-glow: 150 60% 52%; --focus-glow-bright: 150 75% 62%; }
.theme-slate-light  { --focus-glow: 185 60% 50%; --focus-glow-bright: 185 75% 60%; }
.theme-warm-paper   { --focus-glow: 205 55% 55%; --focus-glow-bright: 205 70% 65%; }
.theme-qingye       { --focus-glow: 340 50% 65%; --focus-glow-bright: 340 65% 75%; }
.theme-black        { --focus-glow:  43 70% 58%; --focus-glow-bright:  43 85% 68%; }
.theme-the-finals   { --focus-glow:  44 100% 62%; --focus-glow-bright:  44 100% 72%; }
.theme-slate-dark   { --focus-glow:  30 75% 62%; --focus-glow-bright:  30 90% 72%; }
```

`--focus-glow-bright` 比 base 提亮 10% 用作 trace 跟随高亮。

### AppShell 集成

```tsx
// AppShell.tsx
const focusMode = useAtomValue(focusModeAtom)
useFocusModeShortcut()  // 全局 Alt+F 绑定

return (
  <div className="...">
    {/* titlebar drag region 不变 */}
    <div className="titlebar-drag-region ..." />

    <div className="flex h-full">
      {!focusMode && (
        <div className="z-[60] p-2 pr-0">
          <LeftSidebar />
        </div>
      )}
      <MainArea className="flex-1" />
      {!focusMode && showRightPanel && (
        <div className="z-[60] p-2 pl-0">
          <RightSidePanel />
        </div>
      )}
    </div>

    <FocusModeOverlay />  {/* focus mode 关时返回 null */}
    {/* ... 其他 portals */}
  </div>
)
```

---

## 状态机

```
                  ┌─────────────────────────────────────────┐
                  │             [Focus Off]                  │
                  │   reveal=null, pinned=false              │
                  └──┬──────────────────────────────────┬───┘
                     │ Alt+F / 点 FocusModeButton       │ Alt+F
                     ↓                                  ↑
                  ┌──────────────────────────────────────┐
                  │      [Focus On, reveal=null]         │
                  │            (sidebars hidden)         │
                  └──┬──────────────────────────────────┘
                     │ mouse 进入左侧 hot zone (x≤32, y>84)
                     ↓
                  ┌──────────────────────────────────────┐
   ┌─────────────→│   [reveal='left', pinned=false]      │
   │              └──┬──┬──────────────────────────────┬─┘
   │                 │  │                              │
   │                 │  │ mouse 离开浮岛+hot zone 联合区│
   │                 │  │ → 200ms timer                │
   │                 │  ↓                              │
   │                 │  ┌─────────────────────────┐    │
   │                 │  │ [reveal=null]            │    │
   │                 │  └─────────────────────────┘    │
   │                 │ click 浮岛内部                  │
   │                 ↓                                 │
   │              ┌──────────────────────────────────┐ │
   │              │   [reveal='left', pinned=true]    │ │
   │              └──┬──────────────────────────────┘ │
   │                 │ click 浮岛外（非 Radix portal）│
   │                 │ → 立即 reveal=null, pinned=false│
   │                 ↓                                 │
   └─────────────────┘                                 │
                                                       │
   (同理另一侧右浮岛，状态机对称)                       │
                                                       │
   任意状态:                                            │
     - Alt+F                → 回到 [Focus Off]          │
     - 最后一个 preview 关  → autoExit → [Focus Off]    │
     - 切换工作区          → preview 自动关 → 走 autoExit
```

---

## 边界场景

| 场景 | 处理 |
|---|---|
| 焦点在编辑器内按 Alt+F | useShortcut 加 `allowInsideEditable: true` 配置；`preventDefault()` 阻止 ƒ 字符插入 |
| Preview 关闭瞬间浮岛正在动画 | autoExit 立即设 focusMode=false，AnimatePresence 的 exit 动画继续跑完 |
| Dialog / Popover 开启时鼠标进入 hot zone | dialog z-[100] > 浮岛 z-[80] > glow z-[79]，dialog 自然遮挡命中 |
| 工作区无 RightSidePanel（非 agent 模式 / 无 session） | AppShell 的 `showRightPanel` 条件依然适用；FocusModeOverlay 内 `<FloatingIsland side="right">` 也通过同一条件守卫 |
| 鼠标快速划过边缘但不停留 | hot zone 命中即触发 reveal；如想抑制，可加 50ms intent timer，但 YAGNI 暂不加 |
| 浏览器窗口尺寸变化 | `window.innerWidth` 在 mousemove 内直接读取，自动适应 |
| 鼠标移出窗口后再回来 | `mouseleave` window 不触发额外清理；下次 mousemove 重新判定 |
| 鼠标在窗口左上角（接近 titlebar） | `y < 84` 守卫跳过左浮岛触发，防止与 traffic light / drag region 抢手势 |

---

## 测试

UI 测试目标：**363 → ~378（+15）**。零后端测试改动。

| 文件 | 用例数 | 覆盖 |
|---|---|---|
| `focus-mode-atoms.test.ts` | 4 | 默认值 / toggle / reveal 切换 / pin 路径 / exit action 清理 |
| `useFocusModeHotzone.test.ts` | 6 | 左 hot zone 命中 / 右 hot zone 命中 / 离开 200ms timer / timer 内回到 hot zone 取消 / pinned 锁定 / y<84 不触发 |
| `useFocusModeAutoExit.test.ts` | 3 | preview 全关时退出 / 还有 preview 时不退出 / mount 时纠正残留 |
| `FocusModeButton.test.tsx` | 2 | 渲染图标根据 atom 切换 / 点击触发 toggle |

工具方面 vitest 已有 `userEvent` + `fireEvent.mouseMove`，足够测 hot zone。Window size 通过 jsdom 的 `window.innerWidth` mock。

---

## 文件清单

**新建（12 个 — 8 实现 + 4 测试）**

| 文件 | 行数估计 |
|---|---|
| `ui/src/atoms/focus-mode-atoms.ts` | ~30 |
| `ui/src/hooks/useFocusModeShortcut.ts` | ~20 |
| `ui/src/hooks/useFocusModeHotzone.ts` | ~80 |
| `ui/src/hooks/useFocusModeAutoExit.ts` | ~20 |
| `ui/src/components/focus-mode/FocusModeOverlay.tsx` | ~30 |
| `ui/src/components/focus-mode/FloatingIsland.tsx` | ~60 |
| `ui/src/components/focus-mode/GlowIndicator.tsx` | ~50 |
| `ui/src/components/focus-mode/FocusModeButton.tsx` | ~30 |
| `ui/src/atoms/focus-mode-atoms.test.ts` | ~70 |
| `ui/src/hooks/useFocusModeHotzone.test.ts` | ~120 |
| `ui/src/hooks/useFocusModeAutoExit.test.ts` | ~60 |
| `ui/src/components/focus-mode/FocusModeButton.test.tsx` | ~40 |

**修改（4 个）**

| 文件 | 改动 |
|---|---|
| `ui/src/components/app-shell/AppShell.tsx` | 条件渲染 LeftSidebar / RightSidePanel；挂 `<FocusModeOverlay />`；调用 `useFocusModeShortcut()` |
| `ui/src/components/preview/PreviewHeader.tsx` | 在 Copy/Reveal/Close 左边挂 `<FocusModeButton />` |
| `ui/src/lib/shortcut-defaults.ts` | 注册 `'toggle-focus-mode'`: `Alt+F` / `Alt+F` |
| `ui/src/styles/globals.css` | 12 主题各加 `--focus-glow` + `--focus-glow-bright`；加 4 个 `.focus-glow-*` class + 3 个 `@keyframes` |

---

## 实现顺序建议

1. atom + 测试（独立单位）
2. shortcut-defaults 注册 `toggle-focus-mode`
3. `useFocusModeShortcut` + `useFocusModeAutoExit` + 测试
4. `useFocusModeHotzone` + 测试（最复杂）
5. globals.css 12 主题 token + CSS @keyframes
6. `FloatingIsland` 组件
7. `GlowIndicator` 组件
8. `FocusModeOverlay` 组件（组装）
9. `FocusModeButton` + PreviewHeader 集成 + 测试
10. AppShell 集成（条件渲染 + 挂 overlay）
11. 全量手测 + tsc + vitest

---

## 不在本期范围（YAGNI）

- 移动端 / 触摸屏的边缘 swipe 唤起（uClaw 是桌面应用）
- Focus Mode 的"用户偏好持久化"（每次启动是否记住 focusMode 状态）—— 默认 false，YAGNI
- 浮岛宽度的用户自定义拖拽 —— 保持 LeftSidebar 现有 resize 行为即可（在 inline 模式下生效），浮岛使用其 LATEST width 值
- 浮岛展开方向的"反向"配置（左浮岛从右侧出现）—— YAGNI
- Hot zone "intent timer"（50ms 停留确认才触发）—— 实际使用感觉确认需求后再加
- 浮岛内部滚动惯性 / 拖拽手势 —— 沿用 LeftSidebar 现有交互
- 在快捷键面板里展示与编辑 Focus Mode 快捷键 —— shortcut-defaults.ts 注册后自动出现

---

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| LeftSidebar unmount/remount 损失 scroll 位置 | 接受：会话列表数据在 atoms，scroll 不算关键状态。如有反馈再加 `scrollPositionRef` 兜底 |
| Alt+F 在某些非 Mac/非 Win 输入法下触发输入法 | `preventDefault()` 已尽力；如果出现可加 IME composition 检测 |
| Y trace 跟随鼠标的 60fps 更新拖累渲染 | 用 CSS transform + `will-change: transform`，由 GPU 合成；mousemove 不触发 React re-render（写到 ref 后通过 imperative DOM 更新） |
| 浮岛圆角与 LeftSidebar 内部第一行/最后一行的 hover 背景冲突 | `overflow-hidden` 在外壳上确保圆角生效；LeftSidebar 内部任何贴边的元素由其自身样式管理 |
| 主题切换时辉光颜色突变 | CSS 变量切换 + `transition: opacity 0.15s` 平滑；色相变化无 transition 但人眼可接受 |

---

## 验收标准

- [ ] `Alt+F` 切换 Focus Mode；按下时编辑器焦点保持，不插入 ƒ 字符
- [ ] PreviewHeader 上的 Focus 图标在 Focus On 时变成 Minimize2，Off 时为 Maximize2
- [ ] Focus On 时 LeftSidebar / RightSidePanel 都不在原 flex 流里（preview 横向延展）
- [ ] 鼠标距屏幕左 ≤ 80px 时，左侧辉光开始可见（三层 + 呼吸 + 跟随 Y）
- [ ] 鼠标距屏幕左 ≤ 32px 且 y > 84px 时，LeftSidebar 浮岛滑入（圆角 / 阴影 / 磨砂背景）
- [ ] 鼠标离开浮岛 + hot zone 联合区 200ms 后浮岛滑出
- [ ] 点击浮岛内部任意元素后，鼠标离开不再触发收起（pinned）
- [ ] 点击浮岛外（不含 Radix portal 节点）解锁 pinned，并立即收起
- [ ] 关闭最后一个 preview tab 自动退出 Focus Mode
- [ ] 11 个主题切换后，辉光颜色保持协调（视觉验收）
- [ ] tsc 干净；UI 测试 363 → ~378 全绿
