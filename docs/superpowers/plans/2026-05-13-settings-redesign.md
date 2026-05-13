# Settings Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate 15 settings tabs → 9, make dialog responsive (1200×800 ceiling), add grouped nav with search + sticky breadcrumb, and beautify density — without rewriting any existing `<Tab>Settings.tsx` component bodies.

**Architecture:** Four new wrapper components compose existing tab bodies as vertically-stacked sub-sections; nav and breadcrumb are extracted into focused components; dialog sizing moves from fixed `w-[900px] h-[600px]` to responsive `min(85vw, 1200px) × min(85vh, 800px)`.

**Tech Stack:** React 18, TypeScript, jotai, Radix Dialog, motion/react, Tailwind + theme tokens.

**Spec:** [`docs/superpowers/specs/2026-05-13-settings-redesign-design.md`](../specs/2026-05-13-settings-redesign-design.md)

**Worktree branch:** `worktree-settings-redesign` (already entered, HEAD on `c953d3b` = spec commit)

---

## Hard rules (every subagent MUST re-read)

1. **Branch hygiene.** Run `git branch --show-current` at task start, before commit, and after commit. Confirm `worktree-settings-redesign` each time. Mismatch → **BLOCKED**, do NOT try to fix.
2. **Zero `<Tab>Settings.tsx` body rewrites.** The 15 existing per-tab files are treated as black boxes. New wrappers compose them. The ONLY exception is `BotDefaultSettings.tsx` which gets deleted entirely.
3. **Zero backend changes.** Pure `ui/` work.
4. **Theme tokens only.** `bg-popover` / `bg-muted` / `bg-muted/60` / `text-foreground` / `text-muted-foreground` / `border-border/50` etc. NO hardcoded hex / `bg-zinc-*` / `bg-gray-*` / `bg-slate-*`.
5. **Every task green before commit.** `npx tsc --noEmit` clean, `npm test -- --run` passes.
6. **Test baseline:** UI = 463 (post-STT) → target ~475 (+12).
7. **Tab id migration map** (no orphan literal strings allowed in `ui/`):
   - `channels` / `usage` → `connectivity`
   - `models` / `agent` / `prompts` → `intelligence`
   - `permissions` / `skills` → `tools`
   - `appearance` → `general`
   - `tutorial` / `bots` → DELETE references entirely

---

## Known jump-link callers (Task 1 will migrate)

```
ui/src/components/settings/PromptsSettings.tsx:109  setSettingsTab('general')          → keep 'general'
ui/src/components/chat/ToolSelectorPopover.tsx:66   setSettingsTab('tools')            → keep 'tools'
ui/src/components/agent/SkillCitationChips.tsx:78   setSettingsTab('skills')           → migrate to 'tools'
ui/src/components/agent/SDKMessageRenderer.tsx:887  setSettingsTab('channels')         → migrate to 'connectivity'
ui/src/components/agent/SkillRecallChips.tsx:35     setSettingsTab('skills')           → migrate to 'tools'
ui/src/components/app-shell/LeftSidebar.tsx:1031    setSettingsTab('agent')            → migrate to 'intelligence'
```

---

## Task 1: Tab type + atom + jump-link migration

**Model:** haiku
**Files:**
- Modify: `ui/src/atoms/settings-tab.ts`
- Modify: `ui/src/components/settings/PromptsSettings.tsx`
- Modify: `ui/src/components/chat/ToolSelectorPopover.tsx`
- Modify: `ui/src/components/agent/SkillCitationChips.tsx`
- Modify: `ui/src/components/agent/SDKMessageRenderer.tsx`
- Modify: `ui/src/components/agent/SkillRecallChips.tsx`
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx`

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Rewrite the type union and default.**

In `ui/src/atoms/settings-tab.ts`, replace the `SettingsTab` type and `settingsTabAtom` default:

```ts
export type SettingsTab =
  | 'connectivity'   // 服务商 + 用量
  | 'intelligence'   // 模型 + Agent + 提示词
  | 'tools'          // 工具 + 权限 + 已学技能
  | 'general'        // 通用 + 外观
  | 'stt'            // 语音输入
  | 'shortcuts'
  | 'pet'
  | 'proxy'
  | 'about'

/** 当前设置标签页（不持久化，每次打开默认显示「服务商与用量」） */
export const settingsTabAtom = atom<SettingsTab>('connectivity')
```

(Leave `channelFormDirtyAtom`, `settingsOpenAtom`, `settingsCloseRequestedAtom` untouched.)

- [ ] **Step 3: Migrate the 4 jump-link callers that use removed ids.**

Edit each file to use the new id:

`ui/src/components/agent/SkillCitationChips.tsx:78`:
```diff
-                  setSettingsTab('skills')
+                  setSettingsTab('tools')
```

`ui/src/components/agent/SkillRecallChips.tsx:35`:
```diff
-    setSettingsTab('skills')
+    setSettingsTab('tools')
```

`ui/src/components/agent/SDKMessageRenderer.tsx:887`:
```diff
-        setSettingsTab('channels')
+        setSettingsTab('connectivity')
```

`ui/src/components/app-shell/LeftSidebar.tsx:1031`:
```diff
-                <button onClick={() => { setSettingsTab('agent'); setSettingsOpen(true) }}
+                <button onClick={() => { setSettingsTab('intelligence'); setSettingsOpen(true) }}
```

The two callers using ids that survive (`'tools'` in ToolSelectorPopover and `'general'` in PromptsSettings) need NO changes — they're already valid.

- [ ] **Step 4: TS check.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -20
```

Expected: errors in `SettingsPanel.tsx` (it still references old ids in TABS array + SettingsContent switch — those are Tasks 2 and 7's job). All OTHER files should be clean. **If a file outside `ui/src/components/settings/SettingsPanel.tsx` has type errors after this task, BLOCKED — that means a jump-link caller was missed.**

- [ ] **Step 5: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/atoms/settings-tab.ts \
        ui/src/components/agent/SkillCitationChips.tsx \
        ui/src/components/agent/SkillRecallChips.tsx \
        ui/src/components/agent/SDKMessageRenderer.tsx \
        ui/src/components/app-shell/LeftSidebar.tsx
git commit -m "$(cat <<'EOF'
feat(settings): SettingsTab union 收敛到 9 个 id + jump-link 迁移

- type union: connectivity / intelligence / tools / general / stt /
  shortcuts / pet / proxy / about；删除 channels/models/appearance/
  usage/agent/prompts/permissions/skills/bots/tutorial 等旧 id
- 默认 tab 由 'channels' 改为 'connectivity'
- 4 处 jump-link 迁移到新 id：
  • SkillCitationChips / SkillRecallChips: 'skills' → 'tools'
  • SDKMessageRenderer: 'channels' → 'connectivity'
  • LeftSidebar: 'agent' → 'intelligence'
- SettingsPanel.tsx 暂时残留 TS 错误（TABS 数组 + switch），由
  Task 2 和 Task 7 修
EOF
)"
git branch --show-current
```

---

## Task 2: Responsive dialog sizing + content max-width

**Model:** haiku
**Files:**
- Modify: `ui/src/components/settings/SettingsDialog.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (right pane max-width only — tab nav left for Task 3)

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Replace fixed sizing in `SettingsDialog.tsx`.**

Find the `motion.div` whose className contains `w-[900px] h-[600px]`. Replace its className/style:

```tsx
// before
<motion.div
  initial={{ opacity: 0, scale: 0.992 }}
  animate={{ opacity: 1, scale: 1 }}
  exit={{ opacity: 0, scale: 0.992 }}
  transition={{ duration: 0.22, ease: DIALOG_EASE }}
  className="w-[900px] h-[600px] bg-background shadow-2xl rounded-2xl overflow-hidden"
>

// after
<motion.div
  initial={{ opacity: 0, scale: 0.992 }}
  animate={{ opacity: 1, scale: 1 }}
  exit={{ opacity: 0, scale: 0.992 }}
  transition={{ duration: 0.22, ease: DIALOG_EASE }}
  style={{ width: 'min(85vw, 1200px)', height: 'min(85vh, 800px)' }}
  className="bg-background shadow-2xl rounded-2xl overflow-hidden"
>
```

- [ ] **Step 3: Bump content area max-width in `SettingsPanel.tsx`.**

Locate the right-side scrollable pane (search for `max-w-[640px]`). Replace with `max-w-[800px]`:

```bash
grep -n 'max-w-\[640' ui/src/components/settings/SettingsPanel.tsx
```

Edit each occurrence (likely 1) to `max-w-[800px]`.

- [ ] **Step 4: TS + render check.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
```
Expected: same SettingsPanel errors as Task 1 (TABS / switch still has old ids). No NEW errors.

- [ ] **Step 5: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/components/settings/SettingsDialog.tsx ui/src/components/settings/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): 响应式 Dialog 尺寸 + 内容区放宽

- SettingsDialog: w-[900px] h-[600px] → min(85vw, 1200px) ×
  min(85vh, 800px)。13" MBA ≈ 1200×720；27" 上限 1200×800
- SettingsPanel 右栏 max-w-[640px] → max-w-[800px]，模型表 / 用量
  图表 / 长输入行有呼吸空间
EOF
)"
git branch --show-current
```

---

## Task 3: Extract SettingsNav with groups + search

**Model:** sonnet
**Files:**
- Create: `ui/src/components/settings/SettingsNav.tsx`
- Create: `ui/src/components/settings/SettingsNav.test.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (replace inline nav with `<SettingsNav />`, fix TABS + switch to 9 ids)

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Read the existing inline nav for context.**

```bash
sed -n '120,160p' ui/src/components/settings/SettingsPanel.tsx
```

Understand the active-item styling pattern (`bg-muted text-foreground font-medium` for active, `text-muted-foreground hover:bg-muted/50 hover:text-foreground` for idle). We extend this.

- [ ] **Step 3: Write failing test.** Create `ui/src/components/settings/SettingsNav.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SettingsNav } from './SettingsNav'

describe('SettingsNav', () => {
  it('renders all 9 tabs grouped under 3 group headers', () => {
    renderWithProviders(<SettingsNav active="connectivity" onChange={() => {}} hasUpdate={false} sttNeedsDownload={false} />)
    // Group headers
    expect(screen.getByText('核心')).not.toBeNull()
    expect(screen.getByText('偏好')).not.toBeNull()
    expect(screen.getByText('系统')).not.toBeNull()
    // Sample tabs from each group
    expect(screen.getByText('服务商与用量')).not.toBeNull()
    expect(screen.getByText('智能')).not.toBeNull()
    expect(screen.getByText('工具与能力')).not.toBeNull()
    expect(screen.getByText('通用与外观')).not.toBeNull()
    expect(screen.getByText('输入（语音）')).not.toBeNull()
    expect(screen.getByText('代理')).not.toBeNull()
    expect(screen.getByText('关于')).not.toBeNull()
  })

  it('clicking a tab calls onChange with its id', () => {
    const onChange = vi.fn()
    renderWithProviders(<SettingsNav active="connectivity" onChange={onChange} hasUpdate={false} sttNeedsDownload={false} />)
    fireEvent.click(screen.getByText('智能'))
    expect(onChange).toHaveBeenCalledWith('intelligence')
  })

  it('search filters tabs (case-insensitive substring on label)', () => {
    renderWithProviders(<SettingsNav active="connectivity" onChange={() => {}} hasUpdate={false} sttNeedsDownload={false} />)
    const search = screen.getByPlaceholderText(/搜索/)
    fireEvent.change(search, { target: { value: '语音' } })
    // 输入框中输入「语音」 → 「输入（语音）」高亮可见，其他半透明
    const inputTab = screen.getByText('输入（语音）').closest('button')
    expect(inputTab?.className).not.toContain('opacity-40')
    const proxyTab = screen.getByText('代理').closest('button')
    expect(proxyTab?.className).toContain('opacity-40')
  })

  it('about shows red dot when hasUpdate=true', () => {
    renderWithProviders(<SettingsNav active="about" onChange={() => {}} hasUpdate={true} sttNeedsDownload={false} />)
    const aboutBtn = screen.getByText('关于').closest('button')
    expect(aboutBtn?.querySelector('.bg-red-500, [data-update-dot]')).not.toBeNull()
  })

  it('stt tab shows red dot when sttNeedsDownload=true', () => {
    renderWithProviders(<SettingsNav active="connectivity" onChange={() => {}} hasUpdate={false} sttNeedsDownload={true} />)
    const sttBtn = screen.getByText('输入（语音）').closest('button')
    expect(sttBtn?.querySelector('[data-stt-dot]')).not.toBeNull()
  })
})
```

Add `import { vi } from 'vitest'` at the top.

- [ ] **Step 4: Verify failure.**

```bash
cd ui && npx vitest run src/components/settings/SettingsNav.test.tsx 2>&1 | tail -8
```
Expected: FAIL — module missing.

- [ ] **Step 5: Implement `SettingsNav.tsx`.** Create with this EXACT content:

```tsx
/**
 * SettingsNav — left rail for the settings dialog.
 *
 * 9 tabs grouped into 3 sections (核心 / 偏好 / 系统), with a top
 * search box that fuzz-filters by label (case-insensitive substring).
 * Non-matching tabs dim to 40% opacity rather than disappear, so the
 * structure stays intact during search.
 */
import * as React from 'react'
import {
  Radio, Cpu, Wrench, Settings, Mic, Keyboard, Smile, Globe, Info,
  Search,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SettingsTab } from '@/atoms/settings-tab'

interface NavItem {
  id: SettingsTab
  label: string
  icon: React.ReactNode
}

interface NavGroup {
  title: string
  items: NavItem[]
}

const GROUPS: NavGroup[] = [
  {
    title: '核心',
    items: [
      { id: 'connectivity', label: '服务商与用量', icon: <Radio size={16} /> },
      { id: 'intelligence', label: '智能', icon: <Cpu size={16} /> },
      { id: 'tools', label: '工具与能力', icon: <Wrench size={16} /> },
    ],
  },
  {
    title: '偏好',
    items: [
      { id: 'general', label: '通用与外观', icon: <Settings size={16} /> },
      { id: 'stt', label: '输入（语音）', icon: <Mic size={16} /> },
      { id: 'shortcuts', label: '快捷键', icon: <Keyboard size={16} /> },
      { id: 'pet', label: '桌面宠物', icon: <Smile size={16} /> },
    ],
  },
  {
    title: '系统',
    items: [
      { id: 'proxy', label: '代理', icon: <Globe size={16} /> },
      { id: 'about', label: '关于', icon: <Info size={16} /> },
    ],
  },
]

interface SettingsNavProps {
  active: SettingsTab
  onChange: (id: SettingsTab) => void
  hasUpdate: boolean
  sttNeedsDownload: boolean
}

export function SettingsNav({
  active,
  onChange,
  hasUpdate,
  sttNeedsDownload,
}: SettingsNavProps): React.ReactElement {
  const [query, setQuery] = React.useState('')
  const q = query.trim().toLowerCase()

  const matches = (label: string): boolean =>
    q === '' || label.toLowerCase().includes(q)

  return (
    <div className="w-[200px] border-r border-border/50 pt-3 px-2 flex-shrink-0 overflow-y-auto">
      {/* Search */}
      <div className="relative mb-3 px-1">
        <Search
          size={12}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/60 pointer-events-none"
        />
        <input
          type="text"
          placeholder="搜索…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className={cn(
            'w-full bg-muted/40 rounded-md pl-7 pr-2 py-1.5 text-xs',
            'text-foreground placeholder:text-muted-foreground/60',
            'border border-transparent focus:border-border/70 focus:bg-muted/60',
            'outline-none transition-colors',
          )}
        />
      </div>

      {/* Groups */}
      <nav className="space-y-3">
        {GROUPS.map((g) => (
          <div key={g.title}>
            <div className="px-3 py-1 text-[10.5px] uppercase tracking-wider text-muted-foreground/70 font-medium">
              {g.title}
            </div>
            <div className="flex flex-col gap-0.5">
              {g.items.map((it) => {
                const dim = !matches(it.label)
                const isActive = active === it.id
                return (
                  <button
                    key={it.id}
                    type="button"
                    onClick={() => onChange(it.id)}
                    className={cn(
                      'relative flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-all',
                      isActive
                        ? 'bg-muted text-foreground font-medium'
                        : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground',
                      dim && 'opacity-40',
                    )}
                  >
                    {/* Active left indicator */}
                    {isActive && (
                      <span
                        aria-hidden
                        className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-r bg-primary"
                      />
                    )}
                    {it.icon}
                    <span className="flex-1 text-left">{it.label}</span>
                    {it.id === 'about' && hasUpdate && (
                      <span
                        data-update-dot
                        className="w-1.5 h-1.5 rounded-full bg-red-500"
                      />
                    )}
                    {it.id === 'stt' && sttNeedsDownload && (
                      <span
                        data-stt-dot
                        className="w-1.5 h-1.5 rounded-full bg-primary"
                      />
                    )}
                  </button>
                )
              })}
            </div>
          </div>
        ))}
      </nav>
    </div>
  )
}
```

- [ ] **Step 6: Replace inline nav + fix TABS + switch in `SettingsPanel.tsx`.**

Open `ui/src/components/settings/SettingsPanel.tsx`. Make these changes in order:

**(a) Strip the old `TABS` array** (around lines 47-63) entirely.

**(b) Strip the old icon imports** that were used only by TABS. Leave only icons still needed (e.g., `X` for the close button).

**(c) Update the `SettingsContent` switch** to use the 9 new ids. Until Tasks 5/6/7 create the wrapper components, route the merged tabs to a placeholder div + the un-merged tabs to their existing component. After Task 7 lands the wrappers, the switch becomes final.

For NOW (Task 3 commit), the switch should map all 9 new ids to existing single-tab components as best effort placeholders:

```tsx
function SettingsContent({ tab }: { tab: SettingsTab }) {
  switch (tab) {
    case 'connectivity':
      // Placeholder — ConnectivityTab wrapper lands in Task 5
      return <ChannelSettings />
    case 'intelligence':
      // Placeholder — IntelligenceTab wrapper lands in Task 5
      return <ModelSettings />
    case 'tools':
      // Placeholder — ToolsTab wrapper lands in Task 6
      return <ToolSettings />
    case 'general':
      // Placeholder — GeneralTab wrapper lands in Task 6
      return <GeneralSettings />
    case 'stt':
      return <SttSettings />
    case 'shortcuts':
      return <ShortcutSettings />
    case 'pet':
      return <PetSettings />
    case 'proxy':
      return <ProxySetting />
    case 'about':
      return <AboutSettings />
    default:
      return <ChannelSettings />
  }
}
```

Add `import { SttSettings } from './SttSettings'` at the top. Remove imports for `BotDefaultSettings` (Task 7 deletes the file, but the import already needs to go now to keep the build green).

**(d) Replace the inline nav `<div className="w-[160px] ...">…</nav></div>` block with**:

```tsx
import { SettingsNav } from './SettingsNav'
// ...
<SettingsNav
  active={activeTab}
  onChange={setActiveTab}
  hasUpdate={hasUpdate}
  sttNeedsDownload={false /* Task 7 wires this from modelStatusAtom */}
/>
```

**(e) Update `activeLabel`** computation: now driven by a static map of 9 labels (since `TABS` is gone). Add inside the component:

```tsx
const TAB_LABEL: Record<SettingsTab, string> = {
  connectivity: '服务商与用量',
  intelligence: '智能',
  tools: '工具与能力',
  general: '通用与外观',
  stt: '输入（语音）',
  shortcuts: '快捷键',
  pet: '桌面宠物',
  proxy: '代理',
  about: '关于',
}
const activeLabel = TAB_LABEL[activeTab] ?? '设置'
```

- [ ] **Step 7: Run tests.**

```bash
cd ui && npx vitest run src/components/settings/SettingsNav.test.tsx 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected:
- SettingsNav: 5 tests pass.
- TS clean.
- Total ~468 (463 + 5).

- [ ] **Step 8: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/components/settings/SettingsNav.tsx \
        ui/src/components/settings/SettingsNav.test.tsx \
        ui/src/components/settings/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): 抽取 SettingsNav — 分组 / 搜索框 / 强 active 视觉

- 新 SettingsNav.tsx: 9 tab 分 3 组（核心 / 偏好 / 系统），顶部搜索
  框模糊过滤 label，命中高亮、未命中 opacity-40（不消失）
- 宽度 160 → 200px；active 加左侧 2px primary 竖线指示
- icon 15 → 16，hover 由 bg-muted/50 → /60，跟 macOS 系统设置语感
- STT 模型未下载时显示红点（params 接入，实际由 Task 7 wire）
- SettingsPanel.TABS 数组删除，用 TAB_LABEL 字典代替；SettingsContent
  switch 用 9 个新 id，merged tab 用单子组件做占位（Task 5/6 替换为
  真正的 wrapper）；SttSettings 现在可见
- 5 单元测试
EOF
)"
git branch --show-current
```

---

## Task 4: Sticky breadcrumb header

**Model:** sonnet
**Files:**
- Create: `ui/src/components/settings/SettingsBreadcrumb.tsx`
- Create: `ui/src/components/settings/SettingsBreadcrumb.test.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (replace existing header)

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Write failing test.** Create `ui/src/components/settings/SettingsBreadcrumb.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SettingsBreadcrumb } from './SettingsBreadcrumb'

// jsdom doesn't ship IntersectionObserver
beforeAll(() => {
  ;(globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
    observe() {}
    disconnect() {}
    unobserve() {}
    takeRecords() {
      return []
    }
    root = null
    rootMargin = ''
    thresholds = []
  } as unknown as typeof IntersectionObserver
})

describe('SettingsBreadcrumb', () => {
  it('renders 「设置 / <tabLabel>」 when no subsection is active', () => {
    renderWithProviders(
      <SettingsBreadcrumb tabLabel="智能" scrollContainerRef={{ current: null }} onClose={() => {}} />,
    )
    expect(screen.getByText('设置')).not.toBeNull()
    expect(screen.getByText('智能')).not.toBeNull()
  })

  it('close button triggers onClose', () => {
    const onClose = vi.fn()
    renderWithProviders(
      <SettingsBreadcrumb tabLabel="智能" scrollContainerRef={{ current: null }} onClose={onClose} />,
    )
    fireEvent.click(screen.getByRole('button', { name: /关闭/ }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 3: Verify failure.**

```bash
cd ui && npx vitest run src/components/settings/SettingsBreadcrumb.test.tsx 2>&1 | tail -8
```
Expected: FAIL — module missing.

- [ ] **Step 4: Implement.** Create `ui/src/components/settings/SettingsBreadcrumb.tsx`:

```tsx
/**
 * SettingsBreadcrumb — sticky top header for the settings dialog.
 *
 * Shows: 设置 / <tab label> / <subsection title?>
 * The subsection segment is driven by IntersectionObserver tracking
 * h4 elements with `data-settings-section` attribute inside the
 * scrollable content container.
 */
import * as React from 'react'
import { ChevronRight, X } from 'lucide-react'

interface SettingsBreadcrumbProps {
  tabLabel: string
  /** Scroll container ref — used to observe section titles inside. */
  scrollContainerRef: React.MutableRefObject<HTMLElement | null>
  onClose: () => void
}

export function SettingsBreadcrumb({
  tabLabel,
  scrollContainerRef,
  onClose,
}: SettingsBreadcrumbProps): React.ReactElement {
  const [activeSection, setActiveSection] = React.useState<string | null>(null)

  React.useEffect(() => {
    setActiveSection(null)
    const root = scrollContainerRef.current
    if (!root) return

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
        if (visible.length > 0) {
          const el = visible[0]!.target as HTMLElement
          setActiveSection(el.dataset.settingsSection ?? null)
        }
      },
      { root, rootMargin: '0px 0px -70% 0px', threshold: 0 },
    )

    // Defer to next frame so the new tab's DOM is mounted.
    const id = requestAnimationFrame(() => {
      const nodes = root.querySelectorAll<HTMLElement>('[data-settings-section]')
      nodes.forEach((n) => observer.observe(n))
    })

    return () => {
      cancelAnimationFrame(id)
      observer.disconnect()
    }
  }, [scrollContainerRef, tabLabel])

  return (
    <div className="h-12 flex items-center justify-between px-5 border-b border-border/50 flex-shrink-0 bg-background/95 backdrop-blur-sm sticky top-0 z-10">
      <div className="flex items-center gap-1.5 text-sm">
        <span className="text-muted-foreground">设置</span>
        <ChevronRight size={12} className="text-muted-foreground/50" />
        <span className="text-foreground font-medium">{tabLabel}</span>
        {activeSection && (
          <>
            <ChevronRight size={12} className="text-muted-foreground/50" />
            <span className="text-foreground/80">{activeSection}</span>
          </>
        )}
      </div>
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="rounded-md p-1.5 text-muted-foreground/60 hover:text-foreground hover:bg-muted transition-colors"
      >
        <X size={16} />
      </button>
    </div>
  )
}
```

- [ ] **Step 5: Wire into `SettingsPanel.tsx`.**

Replace the existing inline header (around lines 110-122 — the `<div className="h-12 flex items-center ...">` block) with:

```tsx
const scrollRef = React.useRef<HTMLDivElement | null>(null)

// ... in JSX:
<SettingsBreadcrumb
  tabLabel={activeLabel}
  scrollContainerRef={scrollRef as React.MutableRefObject<HTMLElement | null>}
  onClose={() => setOpen(false)}
/>
```

And the right-content scrollable container gets `ref={scrollRef}`:

```tsx
<ScrollArea className="flex-1 min-h-0" ref={scrollRef as any}>
  ...
</ScrollArea>
```

If `ScrollArea` doesn't forward refs cleanly, wrap the inner content in a div with the ref instead, or read the ScrollArea internals. Search first:

```bash
grep -n 'forwardRef\|ScrollAreaPrimitive' ui/src/components/ui/scroll-area.tsx 2>&1 | head
```

Adapt accordingly.

Add `import { SettingsBreadcrumb } from './SettingsBreadcrumb'` at the top.

- [ ] **Step 6: Run tests.**

```bash
cd ui && npx vitest run src/components/settings/SettingsBreadcrumb.test.tsx 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected: Breadcrumb 2 tests pass. Total ~470 (468 + 2).

- [ ] **Step 7: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/components/settings/SettingsBreadcrumb.tsx \
        ui/src/components/settings/SettingsBreadcrumb.test.tsx \
        ui/src/components/settings/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): sticky breadcrumb 顶栏 + IntersectionObserver 同步子区块

- SettingsBreadcrumb：「设置 / <tab> / <当前 section>」三段面包屑
- IntersectionObserver 观察 [data-settings-section] 元素，根据视口
  最上方可见 section 自动更新末段（rootMargin 让区块在 30% 视口高
  时切换，避免抖动）
- bg-background/95 backdrop-blur-sm sticky top-0，滚动时贴顶
- SettingsPanel 用 SettingsBreadcrumb 替代原本的 12px header
- 2 单元测试 + IntersectionObserver jsdom mock
EOF
)"
git branch --show-current
```

---

## Task 5: ConnectivityTab + IntelligenceTab wrappers

**Model:** sonnet
**Files:**
- Create: `ui/src/components/settings/ConnectivityTab.tsx`
- Create: `ui/src/components/settings/IntelligenceTab.tsx`
- Create: `ui/src/components/settings/ConnectivityTab.test.tsx`
- Create: `ui/src/components/settings/IntelligenceTab.test.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (route 'connectivity' / 'intelligence' to wrappers)

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Write failing smoke tests.**

`ui/src/components/settings/ConnectivityTab.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ConnectivityTab } from './ConnectivityTab'

describe('ConnectivityTab', () => {
  it('renders without throwing', () => {
    const { container } = renderWithProviders(<ConnectivityTab />)
    // Smoke: at least one section header should appear
    expect(container.querySelectorAll('h4').length).toBeGreaterThan(0)
  })
})
```

`ui/src/components/settings/IntelligenceTab.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { IntelligenceTab } from './IntelligenceTab'

describe('IntelligenceTab', () => {
  it('renders without throwing', () => {
    const { container } = renderWithProviders(<IntelligenceTab />)
    expect(container.querySelectorAll('h4').length).toBeGreaterThan(0)
  })
})
```

- [ ] **Step 3: Verify failures.**

```bash
cd ui && npx vitest run src/components/settings/ConnectivityTab.test.tsx src/components/settings/IntelligenceTab.test.tsx 2>&1 | tail -10
```
Expected: 2 failures — modules missing.

- [ ] **Step 4: Implement wrappers.**

`ui/src/components/settings/ConnectivityTab.tsx`:

```tsx
/**
 * ConnectivityTab — composes ChannelSettings + UsageSettings as
 * vertically-stacked sub-sections within a single tab.
 *
 * Each child component already provides its own SettingsSection
 * headings, so this wrapper is pure composition + data-section
 * anchor markers for the breadcrumb's IntersectionObserver.
 */
import * as React from 'react'
import { ChannelSettings } from './ChannelSettings'
import { UsageSettings } from './UsageSettings'

export function ConnectivityTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="服务商">
        <ChannelSettings />
      </section>
      <section data-settings-section="用量与预算">
        <UsageSettings />
      </section>
    </div>
  )
}
```

`ui/src/components/settings/IntelligenceTab.tsx`:

```tsx
/**
 * IntelligenceTab — composes ModelSettings + AgentSettings + PromptsSettings
 * as vertically-stacked sub-sections within a single tab.
 */
import * as React from 'react'
import { ModelSettings } from './ModelSettings'
import { AgentSettings } from './AgentSettings'
import { PromptsSettings } from './PromptsSettings'

export function IntelligenceTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="模型分配">
        <ModelSettings />
      </section>
      <section data-settings-section="Agent 行为">
        <AgentSettings />
      </section>
      <section data-settings-section="提示词">
        <PromptsSettings />
      </section>
    </div>
  )
}
```

If `ChannelSettings`, `UsageSettings`, `ModelSettings`, `AgentSettings`, or `PromptsSettings` are default-exported, adapt the import to `import ChannelSettings from './ChannelSettings'`. Check with:

```bash
grep -E 'export (default |const |function )' ui/src/components/settings/{Channel,Usage,Model,Agent,Prompts}Settings.tsx | head
```

- [ ] **Step 5: Route in `SettingsPanel.tsx`.**

In `SettingsContent`, replace:
```tsx
case 'connectivity': return <ChannelSettings />
case 'intelligence': return <ModelSettings />
```
with:
```tsx
case 'connectivity': return <ConnectivityTab />
case 'intelligence': return <IntelligenceTab />
```

Add imports:
```tsx
import { ConnectivityTab } from './ConnectivityTab'
import { IntelligenceTab } from './IntelligenceTab'
```

Remove now-unused `ChannelSettings`, `ModelSettings`, `AgentSettings`, `PromptsSettings`, `UsageSettings` imports (TS will flag them).

- [ ] **Step 6: Run tests.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected: TS clean. Total ~472 (470 + 2).

- [ ] **Step 7: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/components/settings/ConnectivityTab.tsx \
        ui/src/components/settings/IntelligenceTab.tsx \
        ui/src/components/settings/ConnectivityTab.test.tsx \
        ui/src/components/settings/IntelligenceTab.test.tsx \
        ui/src/components/settings/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): ConnectivityTab + IntelligenceTab wrapper

- ConnectivityTab: 服务商 + 用量与预算（2 个 sub-section）
- IntelligenceTab: 模型分配 + Agent 行为 + 提示词（3 个 sub-section）
- 子区块用 <section data-settings-section> 标注，给 breadcrumb
  IntersectionObserver 跟踪
- 不重写原 *Settings.tsx，纯组合
- 各加 1 个 smoke 测试
EOF
)"
git branch --show-current
```

---

## Task 6: ToolsTab + GeneralTab wrappers

**Model:** sonnet
**Files:**
- Create: `ui/src/components/settings/ToolsTab.tsx`
- Create: `ui/src/components/settings/GeneralTab.tsx`
- Create: `ui/src/components/settings/ToolsTab.test.tsx`
- Create: `ui/src/components/settings/GeneralTab.test.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx`

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Write failing smoke tests** (same shape as Task 5):

`ui/src/components/settings/ToolsTab.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ToolsTab } from './ToolsTab'

describe('ToolsTab', () => {
  it('renders without throwing', () => {
    const { container } = renderWithProviders(<ToolsTab />)
    expect(container.querySelectorAll('h4').length).toBeGreaterThan(0)
  })
})
```

`ui/src/components/settings/GeneralTab.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { GeneralTab } from './GeneralTab'

describe('GeneralTab', () => {
  it('renders without throwing', () => {
    const { container } = renderWithProviders(<GeneralTab />)
    expect(container.querySelectorAll('h4').length).toBeGreaterThan(0)
  })
})
```

- [ ] **Step 3: Verify failures.**

```bash
cd ui && npx vitest run src/components/settings/ToolsTab.test.tsx src/components/settings/GeneralTab.test.tsx 2>&1 | tail -10
```
Expected: 2 failures.

- [ ] **Step 4: Implement.**

`ui/src/components/settings/ToolsTab.tsx`:

```tsx
/**
 * ToolsTab — composes ToolSettings + PermissionsSettings + SkillsSettings.
 */
import * as React from 'react'
import { ToolSettings } from './ToolSettings'
import { PermissionsSettings } from './PermissionsSettings'
import { SkillsSettings } from './SkillsSettings'

export function ToolsTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="工具与 MCP">
        <ToolSettings />
      </section>
      <section data-settings-section="工具权限">
        <PermissionsSettings />
      </section>
      <section data-settings-section="已学技能">
        <SkillsSettings />
      </section>
    </div>
  )
}
```

`ui/src/components/settings/GeneralTab.tsx`:

```tsx
/**
 * GeneralTab — composes GeneralSettings + AppearanceSettings.
 */
import * as React from 'react'
import { GeneralSettings } from './GeneralSettings'
import { AppearanceSettings } from './AppearanceSettings'

export function GeneralTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="通用偏好">
        <GeneralSettings />
      </section>
      <section data-settings-section="主题与字体">
        <AppearanceSettings />
      </section>
    </div>
  )
}
```

(Confirm export shapes first with `grep` if uncertain.)

- [ ] **Step 5: Route in `SettingsPanel.tsx`.**

Replace `case 'tools'` and `case 'general'` returns with `<ToolsTab />` and `<GeneralTab />`. Add imports, remove now-unused single-tab imports.

- [ ] **Step 6: Run tests.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected: TS clean. Total ~474 (472 + 2).

- [ ] **Step 7: Branch check + commit.**

```bash
git branch --show-current
git add ui/src/components/settings/ToolsTab.tsx \
        ui/src/components/settings/GeneralTab.tsx \
        ui/src/components/settings/ToolsTab.test.tsx \
        ui/src/components/settings/GeneralTab.test.tsx \
        ui/src/components/settings/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): ToolsTab + GeneralTab wrapper

- ToolsTab: 工具与 MCP + 工具权限 + 已学技能（3 sub-section）
- GeneralTab: 通用偏好 + 主题与字体（2 sub-section）
- 子区块 data-settings-section 标注
- 各加 1 个 smoke 测试
EOF
)"
git branch --show-current
```

---

## Task 7: Delete Bot stub + density tweaks + STT nav wire + final routing

**Model:** sonnet
**Files:**
- Delete: `ui/src/components/settings/BotDefaultSettings.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (sttNeedsDownload wire-up, final clean)
- Modify: `ui/src/components/settings/primitives/SettingsUIConstants.ts` (ROW_CLASS density)

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Confirm Bot stub has no other importers.**

```bash
grep -rn 'BotDefaultSettings\|BotDefault' ui/src/ 2>&1 | grep -v node_modules
```
Expected: only `BotDefaultSettings.tsx` itself + maybe a stale `SettingsPanel.tsx` import (if not already removed in Task 3 cleanup).

If there ARE other importers, BLOCKED — report.

- [ ] **Step 3: Delete the file.**

```bash
rm ui/src/components/settings/BotDefaultSettings.tsx
```

If `SettingsPanel.tsx` still has an `import { BotDefaultSettings }` line, remove it.

- [ ] **Step 4: Wire STT nav red-dot to modelStatusAtom.**

In `SettingsPanel.tsx`, add an import:

```tsx
import { useAtomValue } from 'jotai'
import { modelStatusAtom } from '@/atoms/stt-atoms'
```

(Note: `useAtomValue` may already be imported; if `useAtom` is being used for `settingsTabAtom`, you can either add `useAtomValue` separately or use `useAtomValue` for the STT status.)

Inside the component:

```tsx
const modelStatus = useAtomValue(modelStatusAtom)
const sttNeedsDownload = modelStatus.kind === 'not-downloaded'
```

And pass to `<SettingsNav>`:

```tsx
<SettingsNav
  active={activeTab}
  onChange={setActiveTab}
  hasUpdate={hasUpdate}
  sttNeedsDownload={sttNeedsDownload}
/>
```

- [ ] **Step 5: Density tweaks.**

In `ui/src/components/settings/primitives/SettingsUIConstants.ts`:

```ts
// before
export const ROW_CLASS = 'flex items-center justify-between px-4 py-3'

// after
export const ROW_CLASS = 'flex items-center justify-between px-4 py-3.5'
```

(Just `py-3` → `py-3.5`. Don't touch other constants — section spacing is handled per-wrapper via `space-y-8`.)

- [ ] **Step 6: TS + tests.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected:
- TS clean.
- Total ~474 (same as Task 6 — no new tests; existing tests still pass).

- [ ] **Step 7: Branch check + commit.**

```bash
git branch --show-current
git add -A ui/src/components/settings/ ui/src/components/settings/primitives/SettingsUIConstants.ts
git commit -m "$(cat <<'EOF'
feat(settings): 删 Bot stub + STT 红点 wire + 行密度微调

- 删除 BotDefaultSettings.tsx（占位 stub，无实现）
- SettingsPanel: useAtomValue(modelStatusAtom) → sttNeedsDownload
  传给 SettingsNav，nav 上「输入（语音）」会红点提示
- ROW_CLASS py-3 → py-3.5（跟苹果系统设置的 row 节奏对齐）
EOF
)"
git branch --show-current
```

---

## Task 8: Final verification + manual smoke

**Model:** sonnet
**Files:** none

- [ ] **Step 1: Branch check.** `git branch --show-current` → `worktree-settings-redesign`.

- [ ] **Step 2: Full TS + test sweep.**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected:
- TS clean.
- UI: ~475 passed (was 463; +12 from Tasks 3/4/5/6 tests).
- Rust: 556 passed (unchanged).

- [ ] **Step 3: Sanity checks via grep.**

```bash
# No old tab id literals remain
grep -rn "'channels'\|'models'\|'agent'\|'prompts'\|'permissions'\|'skills'\|'bots'\|'tutorial'\|'appearance'\|'usage'" \
  ui/src/components/ ui/src/atoms/ 2>&1 \
  | grep -v node_modules \
  | grep -v "\.test\." \
  | grep -v "// " \
  | grep -E "(setSettingsTab|settingsTabAtom)"
```
Expected: empty (all `setSettingsTab(...)` callers use new ids).

```bash
# 9 cases in switch
grep -c "case '" ui/src/components/settings/SettingsPanel.tsx
```
Expected: 9.

```bash
# Bot stub really gone
ls ui/src/components/settings/BotDefaultSettings.tsx 2>&1
```
Expected: "No such file or directory".

```bash
# data-settings-section markers present
grep -rn 'data-settings-section' ui/src/components/settings/ 2>&1 | wc -l
```
Expected: >= 10 (4 wrappers × ~2-3 sub-sections each).

- [ ] **Step 4: Manual smoke test (10 steps).**

Run `cargo tauri dev` from `src-tauri/`, then in the running app:

1. Open settings (`Cmd+,` or sidebar gear).
2. Confirm dialog appears at ~85vw × 85vh, capped at 1200×800.
3. Left nav shows 9 items grouped under 3 headers (核心 / 偏好 / 系统).
4. Default active tab is 「服务商与用量」 — content shows both Channel + Usage sections stacked.
5. Click 「智能」 → see 模型分配 → Agent 行为 → 提示词 stacked. Scroll down — the breadcrumb末段 should update through the three section names.
6. Click 「输入（语音）」 → SttSettings renders (model status, language, mic, autoSend, shortcut). If model isn't downloaded, the nav item should have a small primary-color dot beside the label.
7. Type "宠物" into the search box at top of nav → only 「桌面宠物」 stays opaque; others dim to 40%.
8. Click 「关于」 — if `hasUpdate` is set, a red dot appears.
9. Resize the app window narrower (e.g. drag to ~1100px wide) → dialog shrinks responsively, no overflow.
10. From the agent, trigger a skill citation chip → click 「去设置」 — confirm it opens settings on 「工具与能力」 (was previously `'skills'`).

- [ ] **Step 5: Branch check + commit (only if something needed touching).**

If everything is green and no fixes are needed, no commit is required for Task 8.

If a small follow-up fix surfaces, include it as:

```bash
git commit -m "$(cat <<'EOF'
chore(settings): final verification — UI 475 / Rust 556 / TS clean / 手测 10 步通过
EOF
)"
```

- [ ] **Step 6: Push branch (controller decides PR shape).**

```bash
git push -u origin worktree-settings-redesign
```

---

## Self-review checklist (after all 8 tasks)

1. **Spec coverage:**
   - § 4 (9 tabs): Tasks 1 (type) + 3 (nav) + 5/6 (wrappers) + 7 (cleanup) ✓
   - § 5 (nav groups + search + active indicator): Task 3 ✓
   - § 6 (sticky breadcrumb): Task 4 ✓
   - § 7 (responsive dialog + max-w-800): Task 2 ✓
   - § 8 (density tweaks): Task 7 ✓
   - § 9 (type + atom): Task 1 ✓
   - § 11 (test target ~475): Tasks 3/4/5/6/8 ✓
   - STT漏挂补救: Tasks 1+3 (added to nav + routing) ✓
   - Bot deletion: Task 7 ✓

2. **Placeholder scan:** No "TBD" / "TODO" / "similar to Task N" in this plan.

3. **Type consistency:**
   - `SettingsTab` union (9 values) → consistent across Tasks 1/3/7.
   - `SettingsNav` props `{ active, onChange, hasUpdate, sttNeedsDownload }` → consistent in Tasks 3/7.
   - `SettingsBreadcrumb` props `{ tabLabel, scrollContainerRef, onClose }` → consistent in Task 4.
   - Wrapper components all return `React.ReactElement` and use `<section data-settings-section="...">` markers.
