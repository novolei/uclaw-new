# Agent Response Typography Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the visual rendering of agent assistant markdown responses (B-style structured cards + T1 Apple Editorial typography) without touching tool blocks, message containers, or backend.

**Architecture:** Two files only. [`ui/src/styles/globals.css`](../../../ui/src/styles/globals.css) gains semantic color tokens (success / warning / danger × 11 themes), updated `.chat-content` typography, and structural CSS rules for ordered-list cards, table card framing, and font-size scaling. [`ui/src/components/ai-elements/message.tsx`](../../../ui/src/components/ai-elements/message.tsx) gains custom react-markdown components for `h1/h2/h3`, `table/thead/tr/th/td`, `blockquote`, `hr` — wired into `MARKDOWN_COMPONENTS`. Status badges in table cells are auto-detected via regex on the cell's plain text.

**Tech Stack:** React 18, TypeScript, react-markdown, Tailwind CSS, Vitest + RTL + jsdom.

**Spec:** [2026-05-10-agent-response-typography-design.md](../specs/2026-05-10-agent-response-typography-design.md)

---

## File Structure

| File | Responsibility | Type |
|---|---|---|
| `ui/src/styles/globals.css` | Theme tokens, base typography, table/list structural CSS, font-size scaling | Modify |
| `ui/src/components/ai-elements/message.tsx` | Custom markdown components + helper (`detectStatusBadge`, `extractText`) | Modify |
| `ui/src/components/ai-elements/message.test.tsx` | Vitest tests for new markdown components | Create |

No new directories. No new dependencies.

---

## Task 1: CSS Foundation — Tokens, Typography, Structural Rules

**Files:**
- Modify: `ui/src/styles/globals.css` — add semantic tokens to 11 theme blocks; update `.chat-content`; add `.chat-content`-scoped rules for ordered list cards, table cells, font-size scaling

This task is CSS-only. No JS change yet. The added tokens won't be visibly used until later tasks wire them up via the `MarkdownTd` badge.

- [ ] **Step 1: Add semantic color tokens to `:root` block**

Locate the existing `:root { ... }` block in `ui/src/styles/globals.css` (starts around line 51, contains `--background`, `--foreground`, etc.). Add these six lines just before the closing `}` of `:root`, after the existing tokens:

```css
    /* Semantic status tokens — used by chat-content table badges */
    --success:    142 65% 35%;
    --success-bg: 142 70% 94%;
    --warning:    35 85% 38%;
    --warning-bg: 40 95% 92%;
    --danger:     0 70% 45%;
    --danger-bg:  0 80% 96%;
```

- [ ] **Step 2: Add semantic tokens to `.dark` block**

Locate the `.dark { ... }` block (starts around line 91). Add at the end of the block, before its closing `}`:

```css
    --success:    142 50% 60%;
    --success-bg: 142 35% 18%;
    --warning:    40 80% 65%;
    --warning-bg: 35 40% 18%;
    --danger:     0 65% 65%;
    --danger-bg:  0 40% 18%;
```

- [ ] **Step 3: Add semantic tokens to all 9 named theme blocks**

For each of these blocks (located by `grep -n "^\s*\.theme-" ui/src/styles/globals.css`), append the matching token set before the closing `}`. Concrete values per theme:

`.theme-ocean-light`:
```css
    --success: 175 50% 35%; --success-bg: 175 50% 94%;
    --warning: 35 80% 40%;  --warning-bg: 35 85% 92%;
    --danger:  350 65% 45%; --danger-bg:  350 70% 95%;
```

`.theme-ocean-dark`:
```css
    --success: 175 45% 55%; --success-bg: 175 30% 18%;
    --warning: 35 75% 65%;  --warning-bg: 35 35% 18%;
    --danger:  350 60% 60%; --danger-bg:  350 35% 18%;
```

`.theme-forest-light`:
```css
    --success: 145 60% 32%; --success-bg: 145 50% 92%;
    --warning: 35 85% 38%;  --warning-bg: 40 90% 92%;
    --danger:  0 70% 45%;   --danger-bg:  0 75% 95%;
```

`.theme-forest-dark`:
```css
    --success: 150 50% 60%; --success-bg: 150 30% 18%;
    --warning: 40 75% 60%;  --warning-bg: 40 35% 18%;
    --danger:  0 60% 60%;   --danger-bg:  0 35% 18%;
```

`.theme-slate-light`:
```css
    --success: 145 50% 35%; --success-bg: 145 40% 93%;
    --warning: 35 80% 40%;  --warning-bg: 35 80% 92%;
    --danger:  0 65% 45%;   --danger-bg:  0 70% 95%;
```

`.theme-slate-dark`:
```css
    --success: 145 45% 60%; --success-bg: 145 25% 18%;
    --warning: 35 70% 65%;  --warning-bg: 35 30% 18%;
    --danger:  0 60% 60%;   --danger-bg:  0 30% 18%;
```

`.theme-warm-paper`:
```css
    --success: 95 35% 35%;  --success-bg: 95 30% 92%;
    --warning: 30 65% 45%;  --warning-bg: 30 60% 90%;
    --danger:  10 60% 45%;  --danger-bg:  10 60% 94%;
```

`.theme-qingye`:
```css
    --success: 165 40% 55%; --success-bg: 165 30% 18%;
    --warning: 35 65% 60%;  --warning-bg: 35 35% 18%;
    --danger:  5 60% 60%;   --danger-bg:  5 35% 18%;
```

`.theme-black`:
```css
    --success: 142 45% 55%; --success-bg: 142 25% 14%;
    --warning: 40 70% 60%;  --warning-bg: 40 30% 14%;
    --danger:  0 60% 60%;   --danger-bg:  0 30% 14%;
```

`.theme-the-finals`:
```css
    --success: 145 60% 55%; --success-bg: 145 30% 12%;
    --warning: 40 85% 60%;  --warning-bg: 40 35% 12%;
    --danger:  0 70% 60%;   --danger-bg:  0 35% 12%;
```

- [ ] **Step 4: Update `.chat-content` base typography**

Locate the existing `.chat-content { ... }` block (around line 1438, defines `font-size: 15px; line-height: 1.6;`). Replace with:

```css
.chat-content {
  font-family:
    -apple-system, BlinkMacSystemFont, "SF Pro Text",
    "PingFang SC", "Helvetica Neue", "Microsoft YaHei", sans-serif;
  font-size: 15px;
  line-height: 1.65;
  letter-spacing: 0.005em;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  font-feature-settings: "tnum", "ss01";
  text-rendering: optimizeLegibility;
}
```

- [ ] **Step 5: Add structural rules for ordered-list cards and table internals**

Append at the end of `globals.css` (after the existing `.chat-content code, .chat-content pre` block):

```css
/* ===== Agent response: ordered-list numbered cards (B structure) ===== */
.chat-content ol {
  list-style: none;
  padding-left: 0;
  margin: 12px 0;
  counter-reset: chat-ordered;
}
.chat-content ol > li {
  counter-increment: chat-ordered;
  display: flex;
  gap: 12px;
  align-items: flex-start;
  padding: 14px;
  margin: 8px 0;
  background: hsl(var(--muted) / 0.45);
  border-radius: 10px;
  line-height: 1.6;
}
.chat-content ol > li::before {
  content: counter(chat-ordered);
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  background: hsl(var(--foreground));
  color: hsl(var(--background));
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12.5px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  margin-top: 1px;
}
.chat-content ol > li > :first-child { margin-top: 0; }
.chat-content ol > li > :last-child  { margin-bottom: 0; }
/* nested ordered lists revert to default decimal numbering, no card */
.chat-content ol ol {
  list-style: decimal;
  padding-left: 20px;
  counter-reset: none;
  margin: 4px 0;
}
.chat-content ol ol > li {
  counter-increment: none;
  display: list-item;
  background: transparent;
  padding: 0;
  margin: 2px 0;
  border-radius: 0;
}
.chat-content ol ol > li::before { content: none; }

/* ===== Agent response: unordered list (kept simple) ===== */
.chat-content ul {
  list-style: disc;
  padding-left: 20px;
  margin: 8px 0;
}
.chat-content ul > li { margin: 2px 0; line-height: 1.7; }
```

- [ ] **Step 6: Add font-size scaling for new typography**

Locate the existing `html[data-chat-font-size="sm"] .chat-content.prose p, ...` rules (around line 1462). Append after them:

```css
/* sm: ~0.85x of base */
html[data-chat-font-size="sm"] .chat-content h1 { font-size: 19px; }
html[data-chat-font-size="sm"] .chat-content h2 { font-size: 17px; }
html[data-chat-font-size="sm"] .chat-content h3 { font-size: 14.5px; }
html[data-chat-font-size="sm"] .chat-content th { font-size: 10.5px; }
html[data-chat-font-size="sm"] .chat-content td { font-size: 13px; }

/* lg: ~1.13x of base */
html[data-chat-font-size="lg"] .chat-content h1 { font-size: 25px; }
html[data-chat-font-size="lg"] .chat-content h2 { font-size: 22px; }
html[data-chat-font-size="lg"] .chat-content h3 { font-size: 18px; }
html[data-chat-font-size="lg"] .chat-content th { font-size: 13px; }
html[data-chat-font-size="lg"] .chat-content td { font-size: 16.5px; }
```

- [ ] **Step 7: Verify build**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors (CSS-only change shouldn't affect TS).

Run: `cd ui && npm run build 2>&1 | tail -5`
Expected: build succeeds. Vite prints CSS bundle size.

- [ ] **Step 8: Commit**

```bash
git add ui/src/styles/globals.css
git commit -m "style(chat): add semantic color tokens + Apple-editorial typography foundation

- Add --success/--warning/--danger HSL token pairs to all 11 theme blocks
- Update .chat-content font stack to SF Pro Text + PingFang SC with negative tracking
- Add ordered-list-as-numbered-card CSS using CSS counters
- Add font-size scaling for new h1/h2/h3/th/td under data-chat-font-size sm/lg

CSS-only change. Tokens used by upcoming MarkdownTd badge logic."
```

---

## Task 2: Custom Heading Components (h1/h2/h3)

**Files:**
- Modify: `ui/src/components/ai-elements/message.tsx`
- Create: `ui/src/components/ai-elements/message.test.tsx`

- [ ] **Step 1: Create the test file with failing tests**

Create `ui/src/components/ai-elements/message.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { MessageResponse } from './message'

describe('MessageResponse — headings', () => {
  it('renders h2 with accent bar wrapper', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'## Project Overview'}</MessageResponse>,
    )
    const h2 = container.querySelector('h2')
    expect(h2).not.toBeNull()
    expect(h2!.textContent).toContain('Project Overview')
    // Custom h2 has flex layout with an accent-bar span and a text span
    expect(h2!.classList.toString()).toContain('flex')
    const accentBar = h2!.querySelector('span[aria-hidden]')
    expect(accentBar).not.toBeNull()
  })

  it('renders h1 with accent bar wrapper', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'# Top Title'}</MessageResponse>,
    )
    const h1 = container.querySelector('h1')
    expect(h1).not.toBeNull()
    expect(h1!.textContent).toContain('Top Title')
    expect(h1!.querySelector('span[aria-hidden]')).not.toBeNull()
  })

  it('renders h3 without accent bar', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'### Subhead'}</MessageResponse>,
    )
    const h3 = container.querySelector('h3')
    expect(h3).not.toBeNull()
    expect(h3!.textContent).toContain('Subhead')
    // h3 is plain — no aria-hidden accent bar inside
    expect(h3!.querySelector('span[aria-hidden]')).toBeNull()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -25`
Expected: 3 failing tests. h2 has no `flex` class yet; no `span[aria-hidden]` accent bar exists.

- [ ] **Step 3: Add heading components to message.tsx**

Open `ui/src/components/ai-elements/message.tsx`. Locate the `MARKDOWN_COMPONENTS` const (around line 188). Add these component definitions ABOVE the `MARKDOWN_COMPONENTS` declaration (after `MarkdownInlineCode`):

```tsx
/** 标题渲染器：h1/h2 带左侧实心条做视觉锚点；h3 纯文本 */
const MarkdownH1 = React.memo(function MarkdownH1({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h1 className="flex items-center gap-2.5 text-[22px] font-semibold tracking-[-0.015em] mt-7 mb-3.5 first:mt-0 leading-[1.3]">
      <span className="w-[3px] h-[18px] bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h1>
  )
})

const MarkdownH2 = React.memo(function MarkdownH2({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h2 className="flex items-center gap-2.5 text-[19px] font-semibold tracking-[-0.012em] mt-[22px] mb-3 first:mt-0 leading-[1.35]">
      <span className="w-[3px] h-4 bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h2>
  )
})

const MarkdownH3 = React.memo(function MarkdownH3({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h3 className="text-[16px] font-semibold mt-[18px] mb-2 first:mt-0 leading-[1.4] tracking-[-0.005em]">
      {children}
    </h3>
  )
})
```

Then update the `MARKDOWN_COMPONENTS` map to wire them in:

```tsx
const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
} as const
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -10`
Expected: 3 passing tests.

- [ ] **Step 5: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/ai-elements/message.tsx ui/src/components/ai-elements/message.test.tsx
git commit -m "feat(chat): custom h1/h2/h3 markdown renderers with accent-bar layout

- h1/h2 use flex + 3px accent bar at left for visual hierarchy
- h3 is plain semibold with negative letter-spacing
- All wired into MARKDOWN_COMPONENTS for react-markdown"
```

---

## Task 3: Table Structure Components (table / thead / tr / th)

**Files:**
- Modify: `ui/src/components/ai-elements/message.tsx`
- Modify: `ui/src/components/ai-elements/message.test.tsx`

`td` with badge detection comes in Task 4 — this task just frames the table.

- [ ] **Step 1: Add failing table tests to message.test.tsx**

Append to `ui/src/components/ai-elements/message.test.tsx`:

```tsx
describe('MessageResponse — tables', () => {
  const tableMd = [
    '| Project | Status |',
    '|---------|--------|',
    '| Alpha   | done   |',
    '| Beta    | wip    |',
  ].join('\n')

  it('wraps table in a card container', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const table = container.querySelector('table')
    expect(table).not.toBeNull()
    const wrapper = table!.parentElement
    expect(wrapper).not.toBeNull()
    expect(wrapper!.className).toContain('rounded-')
    expect(wrapper!.className).toContain('border')
  })

  it('renders thead with muted background', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const thead = container.querySelector('thead')
    expect(thead).not.toBeNull()
    expect(thead!.className).toContain('bg-muted')
  })

  it('renders th with uppercase + tracking + muted-foreground', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const th = container.querySelector('th')
    expect(th).not.toBeNull()
    expect(th!.className).toContain('uppercase')
    expect(th!.className).toContain('tracking-')
    expect(th!.className).toContain('text-muted-foreground')
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -25`
Expected: 3 failures (the 3 new tests). Default react-markdown table has no card wrapper, no `bg-muted` thead, no `uppercase` th.

- [ ] **Step 3: Add table components to message.tsx**

Open `ui/src/components/ai-elements/message.tsx`. Add ABOVE the `MARKDOWN_COMPONENTS` declaration (after the heading components):

```tsx
/** 表格渲染器：包一层 card 容器，让表格有清晰边界 */
const MarkdownTable = React.memo(function MarkdownTable({
  children,
}: React.HTMLAttributes<HTMLTableElement>): React.ReactElement {
  return (
    <div className="my-3 rounded-[10px] border border-border overflow-hidden bg-card">
      <table className="w-full border-collapse">{children}</table>
    </div>
  )
})

const MarkdownThead = React.memo(function MarkdownThead({
  children,
}: React.HTMLAttributes<HTMLTableSectionElement>): React.ReactElement {
  return <thead className="bg-muted/50">{children}</thead>
})

const MarkdownTr = React.memo(function MarkdownTr({
  children,
}: React.HTMLAttributes<HTMLTableRowElement>): React.ReactElement {
  return (
    <tr className="[&:not(:last-child)>td]:border-b [&>td]:border-border/40">
      {children}
    </tr>
  )
})

const MarkdownTh = React.memo(function MarkdownTh({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  return (
    <th className="text-left px-3.5 py-2.5 text-[11.5px] font-semibold uppercase tracking-[0.06em] text-muted-foreground border-b border-border">
      {children}
    </th>
  )
})
```

Update `MARKDOWN_COMPONENTS`:

```tsx
const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
  table: MarkdownTable,
  thead: MarkdownThead,
  tr: MarkdownTr,
  th: MarkdownTh,
} as const
```

- [ ] **Step 4: Run tests**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -10`
Expected: all 6 tests pass (3 from Task 2 + 3 from Task 3).

- [ ] **Step 5: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/ai-elements/message.tsx ui/src/components/ai-elements/message.test.tsx
git commit -m "feat(chat): wrap markdown tables in rounded card + ALLCAPS th"
```

---

## Task 4: TD with Auto-Detected Status Badges

**Files:**
- Modify: `ui/src/components/ai-elements/message.tsx`
- Modify: `ui/src/components/ai-elements/message.test.tsx`

- [ ] **Step 1: Add failing badge tests to message.test.tsx**

Append:

```tsx
describe('MessageResponse — status badges in table cells', () => {
  function tableWithStatus(status: string): string {
    return [
      '| Project | Status |',
      '|---------|--------|',
      `| Alpha   | ${status} |`,
    ].join('\n')
  }

  it('detects success badge for "✅ 已完成"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('✅ 已完成')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const statusCell = cells[1]!
    const badge = statusCell.querySelector('span[data-status]')
    expect(badge).not.toBeNull()
    expect(badge!.getAttribute('data-status')).toBe('success')
  })

  it('detects warning badge for "⏳ 未完成"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('⏳ 未完成')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const badge = cells[1]!.querySelector('span[data-status="warning"]')
    expect(badge).not.toBeNull()
  })

  it('detects danger badge for "❌ 尚未开始"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('❌ 尚未开始')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const badge = cells[1]!.querySelector('span[data-status="danger"]')
    expect(badge).not.toBeNull()
  })

  it('renders cell content unchanged when no status pattern matches', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('HTML/CSS/JS')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const statusCell = cells[1]!
    expect(statusCell.querySelector('span[data-status]')).toBeNull()
    expect(statusCell.textContent).toBe('HTML/CSS/JS')
  })

  it('badge uses success token classes', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('✅ done')}</MessageResponse>,
    )
    const badge = container.querySelector('span[data-status="success"]')!
    expect(badge.className).toContain('hsl(var(--success-bg))')
    expect(badge.className).toContain('hsl(var(--success))')
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -25`
Expected: 5 new failures — there's no `MarkdownTd` yet, so cells render as default `<td>` with no `span[data-status]`.

- [ ] **Step 3: Add `extractText` helper and `MarkdownTd` to message.tsx**

In `ui/src/components/ai-elements/message.tsx`, add ABOVE the `MARKDOWN_COMPONENTS` declaration (after `MarkdownTh`):

```tsx
/** 递归收集 React 子树里的文本节点（用于 td 内的状态识别） */
function extractText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (React.isValidElement(node)) {
    const props = node.props as { children?: React.ReactNode }
    return extractText(props.children)
  }
  return ''
}

type StatusVariant = 'success' | 'warning' | 'danger'

const STATUS_PATTERNS: Array<{ re: RegExp; variant: StatusVariant }> = [
  { re: /✅|✓|已完成|completed|done\b|success\b/i, variant: 'success' },
  { re: /⏳|未完成|in[\s-]?progress|pending|wip\b/i, variant: 'warning' },
  { re: /❌|✗|未开始|尚未|failed|error\b/i, variant: 'danger' },
]

function detectStatus(text: string): StatusVariant | null {
  for (const { re, variant } of STATUS_PATTERNS) {
    if (re.test(text)) return variant
  }
  return null
}

const STATUS_CLASS: Record<StatusVariant, string> = {
  success: 'bg-[hsl(var(--success-bg))] text-[hsl(var(--success))]',
  warning: 'bg-[hsl(var(--warning-bg))] text-[hsl(var(--warning))]',
  danger:  'bg-[hsl(var(--danger-bg))] text-[hsl(var(--danger))]',
}

/** 单元格渲染器：检测状态文本，自动包成 badge */
const MarkdownTd = React.memo(function MarkdownTd({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  const text = extractText(children).trim()
  const variant = text ? detectStatus(text) : null
  if (variant) {
    return (
      <td className="px-3.5 py-3 text-[14.5px]">
        <span
          data-status={variant}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[12px] font-medium',
            STATUS_CLASS[variant],
          )}
        >
          {text}
        </span>
      </td>
    )
  }
  return <td className="px-3.5 py-3 text-[14.5px]">{children}</td>
})
```

Update `MARKDOWN_COMPONENTS`:

```tsx
const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
  table: MarkdownTable,
  thead: MarkdownThead,
  tr: MarkdownTr,
  th: MarkdownTh,
  td: MarkdownTd,
} as const
```

- [ ] **Step 4: Run tests**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -10`
Expected: 11 tests pass total (3 + 3 + 5).

- [ ] **Step 5: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/ai-elements/message.tsx ui/src/components/ai-elements/message.test.tsx
git commit -m "feat(chat): auto-detect status badges in table cells

- Regex match on extracted cell text (✅/⏳/❌ + CN/EN status words)
- Render matched cells as colored pills using --success/--warning/--danger tokens
- Falls through to default rendering when no pattern matches"
```

---

## Task 5: Blockquote, HR, and Final Wiring

**Files:**
- Modify: `ui/src/components/ai-elements/message.tsx`
- Modify: `ui/src/components/ai-elements/message.test.tsx`

This task adds the remaining elements and simplifies the outer prose className that's been carrying the load until now.

- [ ] **Step 1: Add failing tests for blockquote and ordered list**

Append to `message.test.tsx`:

```tsx
describe('MessageResponse — blockquote and lists', () => {
  it('renders blockquote with left border + non-italic', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'> A quoted line.'}</MessageResponse>,
    )
    const bq = container.querySelector('blockquote')
    expect(bq).not.toBeNull()
    expect(bq!.className).toContain('border-l-2')
    expect(bq!.className).toContain('not-italic')
  })

  it('renders hr with subtle border', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'before\n\n---\n\nafter'}</MessageResponse>,
    )
    const hr = container.querySelector('hr')
    expect(hr).not.toBeNull()
    expect(hr!.className).toContain('border-t')
  })

  it('renders ordered list with li children (CSS handles card visual)', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'1. First step\n2. Second step'}</MessageResponse>,
    )
    const ol = container.querySelector('ol')
    expect(ol).not.toBeNull()
    const items = ol!.querySelectorAll(':scope > li')
    expect(items).toHaveLength(2)
    expect(items[0]!.textContent).toContain('First step')
  })
})
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -25`
Expected: 2 failures on blockquote/hr (no custom components yet — the prose default has italic blockquote). Ordered-list test should already pass (default `<ol>` structure is fine).

- [ ] **Step 3: Add blockquote/hr components and simplify outer className**

In `ui/src/components/ai-elements/message.tsx`, add ABOVE `MARKDOWN_COMPONENTS`:

```tsx
const MarkdownBlockquote = React.memo(function MarkdownBlockquote({
  children,
}: React.HTMLAttributes<HTMLQuoteElement>): React.ReactElement {
  return (
    <blockquote className="my-3 pl-3 border-l-2 border-foreground/20 text-foreground/75 not-italic [&>p]:my-1">
      {children}
    </blockquote>
  )
})

const MarkdownHr = React.memo(function MarkdownHr(): React.ReactElement {
  return <hr className="my-6 border-0 border-t border-border/60" />
})
```

Update `MARKDOWN_COMPONENTS` to its final form:

```tsx
const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
  table: MarkdownTable,
  thead: MarkdownThead,
  tr: MarkdownTr,
  th: MarkdownTh,
  td: MarkdownTd,
  blockquote: MarkdownBlockquote,
  hr: MarkdownHr,
} as const
```

Now simplify the outer wrapper className inside `MessageResponse`. Replace the existing:

```tsx
className={cn(
  'chat-content prose prose-sm dark:prose-invert max-w-none text-[15px] leading-relaxed',
  'prose-p:my-1.5 prose-p:leading-[1.6] prose-li:leading-[1.6]',
  'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
  'prose-headings:my-2 prose-headings:font-semibold',
  'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
  'prose-blockquote:border-l-2 prose-blockquote:border-foreground/20 prose-blockquote:text-foreground/70',
  'prose-table:my-2 prose-th:bg-muted/40 prose-th:font-semibold',
  '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
  className,
)}
```

with:

```tsx
className={cn(
  'chat-content prose prose-sm dark:prose-invert max-w-none',
  'prose-p:my-1.5 prose-p:leading-[1.65]',
  'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
  'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
  'prose-strong:font-semibold prose-strong:text-foreground',
  '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
  className,
)}
```

The dropped prose classes are now handled by the custom components and `globals.css` rules. `prose prose-sm` is kept as fallback for `<em>`, `<strong>`, paragraph margins, and any unhandled element.

- [ ] **Step 4: Run tests**

Run: `cd ui && npx vitest run src/components/ai-elements/message.test.tsx 2>&1 | tail -10`
Expected: all 14 tests pass.

- [ ] **Step 5: Run the full UI test suite to check for regressions**

Run: `cd ui && npm test -- --run 2>&1 | tail -15`
Expected: all tests pass. If any pre-existing test snapshots include `MessageResponse` output, they may need updating — open the failures and verify the diff matches the new structure (table card wrapper, h2 with span).

- [ ] **Step 6: TypeScript check + build**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

Run: `cd ui && npm run build 2>&1 | tail -5`
Expected: build succeeds.

- [ ] **Step 7: Manual visual smoke test**

Start dev server: `cd src-tauri && cargo tauri dev`

In the running app, open an existing agent session that contains markdown tables / numbered lists / status emojis (or paste this prompt to the agent and have it echo a structured response):

> "Show me a project status table with three rows (Alpha done, Beta in progress, Gamma not started) and a numbered list of next steps."

Verify:
- Table is wrapped in a rounded card.
- `th` is uppercase, light tracking, muted color.
- `✅` / `⏳` / `❌` cells render as colored pills.
- Numbered list items appear as gray-bg cards with black numeric chips on the left.
- `## Heading` has a black accent bar at left.

Switch theme via Settings: cycle through `warm-paper`, `qingye`, `ocean-dark`, `forest-light`. Confirm the badges remain readable (no harsh red on warm paper, etc.) and accent bars adapt.

Switch chat font size (sm/md/lg) — typography scales.

If anything looks off, note which theme + element, fix in `globals.css` token values, re-test. Don't commit broken visuals.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/ai-elements/message.tsx ui/src/components/ai-elements/message.test.tsx
git commit -m "feat(chat): blockquote/hr renderers + simplify outer prose wrapper

- Custom blockquote: 2px left bar, no italic, foreground/75 text
- Custom hr: subtle border-t, generous my-6 spacing
- Drop prose-table/prose-th/prose-blockquote/prose-headings overrides — now handled by custom components and globals.css

Closes typography redesign — agent responses now render with B-style
structured cards + T1 Apple Editorial typography across all 11 themes."
```

---

## Final Verification Checklist

After Task 5 commits, sanity-check:

- [ ] `cd ui && npm test -- --run` — all tests pass
- [ ] `cd ui && npx tsc --noEmit` — no errors
- [ ] `cd ui && npm run build` — builds clean
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — no Rust errors (no Rust changes expected, sanity)
- [ ] `git log --oneline main..HEAD` shows 5 commits, each with focused subject
- [ ] Manual smoke test passes in light + 2 dark themes + warm-paper + qingye

---

## Notes for the Implementer

- **Where the components live in `message.tsx`:** add new component definitions in source order: `MarkdownH1` → `MarkdownH2` → `MarkdownH3` → `MarkdownTable` → `MarkdownThead` → `MarkdownTr` → `MarkdownTh` → `extractText` → `STATUS_PATTERNS` / `detectStatus` / `STATUS_CLASS` → `MarkdownTd` → `MarkdownBlockquote` → `MarkdownHr`. Then `MARKDOWN_COMPONENTS` references all of them.
- **`React.memo` is the existing pattern** — every existing custom component (`MarkdownLink`, `MarkdownPre`, `MarkdownInlineCode`) is wrapped. Stay consistent.
- **`cn` is already imported** at top of file — no new import needed.
- **Don't touch `UserMessageContent`, `MessageHeader`, or any other export** — the spec is explicit about scope.
- **No `dangerouslySetInnerHTML`** anywhere. All content flows through react-markdown components.
- **Theme HSL values are the implementer's call to refine.** The starting values in Task 1 are reasonable but each theme's color owner may want to nudge by ±5% lightness for harmony. Don't ship values you haven't eyeballed against the actual theme.
- **CSS counter scope:** `chat-content ol > li::before` only applies to direct children, so nested `<ol>` will not double-render. Nested ordered lists fall back to native `decimal` numbering via the `chat-content ol ol` override.
- **Status detection is best-effort** — it's a UX nicety. False positives (a cell that legitimately says "done" without status meaning) will just show as a green pill. If this becomes a problem, future work can add a markdown convention (e.g. `:done:` shortcode) but YAGNI for now.
