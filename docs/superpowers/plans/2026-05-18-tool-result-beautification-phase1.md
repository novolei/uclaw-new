# Tool result beautification — Phase 1 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 21-line `ToolResultRenderer` stub with a Pierre-powered (`@pierre/diffs`) renderer fleet covering `write_file` / `edit` / `read_file` / `bash`, plus an always-visible "预览" button on tool activity rows that hands off to the multi-tab preview panel.

**Architecture:** Mirror Proma's `tool-result-renderers/` directory structure — one dispatcher (`index.tsx`) switches by snake_case tool name (uClaw's convention, not Proma's PascalCase) and dispatches to per-tool renderers. Each renderer is a thin wrapper around Pierre's `MultiFileDiff` / `FileDiff` / `File` components. A new `CollapsibleResult` primitive wraps long outputs. `selectedPreviewFileAtom` (from PR #190) gets opened via the new preview button alongside the existing chat:stream-tool-activity auto-open path.

**Tech Stack:** React 18, TypeScript, Jotai, Tailwind (theme tokens), Vitest + React Testing Library, **new dep:** `@pierre/diffs@^1.1.22` (Apache 2.0, public npm, by the Bootstrap team).

Spec: [docs/superpowers/specs/2026-05-18-tool-result-beautification-phase1-design.md](../specs/2026-05-18-tool-result-beautification-phase1-design.md).

---

## Verified tool input schemas (locked at plan time, from grepping `src-tauri/src/agent/tools/builtin/`)

| Tool | Input shape |
|---|---|
| `write_file` | `{ path: string, content: string }` |
| `edit` | `{ path: string, edits: Array<{ old_text: string, new_text: string, insert_line?: number }> }` — note `old_text` / `new_text` (NOT `old_string` / `new_string` like Proma) |
| `read_file` | `{ path: string }` (no offset/limit; Pierre's `File` component renders from line 1) |
| `bash` | `{ command: string }` |

Existing dispatch surface (do NOT break):
```ts
interface ToolResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
}
```

Callers of the existing stub (2 files, both will keep working after the dispatcher swap):
- `ui/src/components/chat/ChatToolBlock.tsx:17, 129`
- `ui/src/components/agent/ToolActivityItem.tsx:25, 387`

Existing theme atom:
- `ui/src/atoms/theme.ts:71` → `resolvedThemeAtom: atom<'light' | 'dark'>`

---

## File map (locked)

**New directory** `ui/src/components/agent/tool-renderers/`:

| File | Responsibility |
|---|---|
| `index.tsx` | Dispatcher: switch by snake_case toolName |
| `default-result.tsx` | Fallback — JSON parse → key-value table, else plain text in Collapsible |
| `collapsible-result.tsx` | Shared primitive: long-output collapse with chevron |
| `write-result.tsx` | Pierre `MultiFileDiff` — full-file additions |
| `edit-result.tsx` | Pierre `FileDiff` per edit — iterates `input.edits` array |
| `read-result.tsx` | Pierre `File` — code-only render, no diff, wrapped in Collapsible |
| `bash-result.tsx` | Terminal-style, monospace, stderr highlight |
| `pierre-theme.ts` | `usePierreTheme()` hook + Shiki language detection |

**Modified:**
- `ui/package.json` — add `@pierre/diffs`
- `ui/src/components/agent/tool-result-renderers.tsx` — DELETE
- `ui/src/components/agent/ToolActivityItem.tsx` — import path swap + add "预览" button
- `ui/src/components/chat/ChatToolBlock.tsx` — import path swap (`tool-renderers` instead of `tool-result-renderers`)

---

## Task 1 — Pierre dependency + theme bridge

**Files:**
- Modify: `ui/package.json` (add dep)
- Create: `ui/src/components/agent/tool-renderers/pierre-theme.ts`

- [ ] **Step 1: Add Pierre to ui/package.json**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1/ui
npm install @pierre/diffs@^1.1.22 --save 2>&1 | tail -5
```

Expected: `added 1 package` or similar. Verify `package.json` now has `"@pierre/diffs": "^1.1.22"` in dependencies (NOT devDependencies — runtime use).

- [ ] **Step 2: Verify Pierre exports + CSS import path**

```bash
ls node_modules/@pierre/diffs/
ls node_modules/@pierre/diffs/dist/ 2>&1 | head
cat node_modules/@pierre/diffs/package.json | grep -E '"main"|"module"|"exports"|"types"' | head
```

Read the `exports` field — Pierre may expose `@pierre/diffs/react` (component) and `@pierre/diffs/styles.css` (stylesheet) separately. Note the EXACT import paths needed.

- [ ] **Step 3: Create `pierre-theme.ts`**

`ui/src/components/agent/tool-renderers/pierre-theme.ts`:

```ts
import { useAtomValue } from 'jotai'
import { resolvedThemeAtom } from '@/atoms/theme'

/**
 * Map uClaw's resolved theme ('light' | 'dark') to Pierre's theme prop name.
 * Pierre ships 'one-light' and 'one-dark-pro' out of the box (Shiki themes).
 * Per-uClaw-theme Pierre customization is deferred to Phase 2.
 */
export function usePierreTheme(): 'one-light' | 'one-dark-pro' {
  const theme = useAtomValue(resolvedThemeAtom)
  return theme === 'dark' ? 'one-dark-pro' : 'one-light'
}

/**
 * Infer Shiki/Pierre language identifier from a file path's extension.
 * Falls back to 'text' for unknown / extensionless paths.
 */
export function detectLang(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'tsx',
    js: 'javascript', jsx: 'jsx',
    json: 'json', md: 'markdown', mdx: 'markdown',
    py: 'python', rs: 'rust', go: 'go',
    java: 'java', kt: 'kotlin', swift: 'swift',
    rb: 'ruby', php: 'php', sh: 'shell', bash: 'shell',
    yaml: 'yaml', yml: 'yaml', toml: 'toml',
    html: 'html', css: 'css', scss: 'scss', sass: 'sass',
    sql: 'sql', xml: 'xml', svg: 'xml',
    dockerfile: 'docker', dockerignore: 'text',
    c: 'c', h: 'c', cpp: 'cpp', hpp: 'cpp', cc: 'cpp',
    cs: 'csharp', vue: 'vue', svelte: 'svelte',
    lua: 'lua', r: 'r', dart: 'dart', zig: 'zig',
  }
  return map[ext] ?? 'text'
}
```

- [ ] **Step 4: Import Pierre CSS in root entry**

Find the app's root CSS imports — likely `ui/src/main.tsx` or `ui/src/styles/globals.css`. Add Pierre's CSS (path from Step 2 — likely `@pierre/diffs/dist/index.css`):

```ts
// ui/src/main.tsx (or wherever globals are imported)
import '@pierre/diffs/dist/index.css'  // adjust path per Step 2 findings
```

If Pierre's CSS path is different, use what Step 2 revealed. If Pierre ships CSS-in-JS (no separate file), skip this step.

- [ ] **Step 5: Sanity build**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1/ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean. If Pierre's types fail to resolve, check `tsconfig.json` `moduleResolution` setting and `@pierre/diffs` package.json `types` field.

- [ ] **Step 6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/package.json ui/package-lock.json \
  ui/src/components/agent/tool-renderers/pierre-theme.ts \
  ui/src/main.tsx  # or whichever file imported Pierre CSS

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(deps): add @pierre/diffs + Pierre theme bridge

@pierre/diffs (Apache 2.0, ^1.1.22) is the React code-rendering
library Proma uses: MultiFileDiff for new files, FileDiff for
unified diffs, File for code-only views. Shiki syntax highlight
is built in. uClaw already has shiki@^3.22 (Pierre needs ^3.0)
so this is bundle-light.

pierre-theme.ts provides:
  - usePierreTheme() hook mapping resolvedThemeAtom → Pierre's
    'one-light' / 'one-dark-pro' theme prop
  - detectLang(path) helper inferring Shiki language identifier
    from file extension (covers 25+ common languages)

Per-uClaw-theme Pierre customization (warm-paper / qingye / etc.)
deferred to Phase 2 per spec."
```

---

## Task 2 — `CollapsibleResult` primitive

**Files:**
- Create: `ui/src/components/agent/tool-renderers/collapsible-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/collapsible-result.test.tsx`

- [ ] **Step 1: Write the failing test**

`ui/src/components/agent/tool-renderers/collapsible-result.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import * as React from 'react'
import { CollapsibleResult } from './collapsible-result'

const SHORT = 'short content'
const LONG = 'x'.repeat(4000)  // exceeds default 3000 threshold

describe('CollapsibleResult', () => {
  it('renders children directly when below threshold', () => {
    render(<CollapsibleResult>{SHORT}</CollapsibleResult>)
    expect(screen.getByText(SHORT)).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('renders collapse toggle when over threshold', () => {
    render(<CollapsibleResult><pre>{LONG}</pre></CollapsibleResult>)
    expect(screen.getByRole('button', { name: /展开全部/ })).toBeInTheDocument()
  })

  it('toggle button expands and collapses content', () => {
    render(<CollapsibleResult><pre>{LONG}</pre></CollapsibleResult>)
    const btn = screen.getByRole('button')
    expect(btn).toHaveTextContent('展开全部')
    fireEvent.click(btn)
    expect(btn).toHaveTextContent('收起')
    fireEvent.click(btn)
    expect(btn).toHaveTextContent('展开全部')
  })

  it('respects custom charThreshold prop', () => {
    render(<CollapsibleResult charThreshold={10}>{SHORT}</CollapsibleResult>)
    expect(screen.getByRole('button')).toBeInTheDocument()  // 13 chars > 10
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL (component doesn't exist yet)**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1/ui && npm test -- --run collapsible-result 2>&1 | tail -10
```

Expected: cannot resolve module / 4 tests fail.

- [ ] **Step 3: Implement `collapsible-result.tsx`**

`ui/src/components/agent/tool-renderers/collapsible-result.tsx`:

```tsx
import * as React from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { cn } from '@/lib/utils'

interface CollapsibleResultProps {
  /** Char-length above which the collapse UI appears. Default 3000. */
  charThreshold?: number
  /** Number of lines visible when collapsed. Default 15. */
  previewLines?: number
  children: React.ReactNode
}

/**
 * Walks a React node tree extracting plain text. Used to measure
 * content length against the collapse threshold. Doesn't try to
 * be perfect — just enough to count chars/lines for the heuristic.
 */
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

export function CollapsibleResult({
  charThreshold = 3000,
  previewLines = 15,
  children,
}: CollapsibleResultProps): React.ReactElement {
  const text = React.useMemo(() => extractText(children), [children])
  const charCount = text.length
  const lineCount = text.split('\n').length
  const exceedsThreshold = charCount > charThreshold
  const [expanded, setExpanded] = React.useState(false)

  if (!exceedsThreshold) {
    return <>{children}</> as React.ReactElement
  }

  return (
    <div>
      <div
        className={cn(
          'transition-all',
          !expanded && `max-h-[${previewLines * 1.5}em] overflow-hidden`,
        )}
        style={!expanded ? { maxHeight: `${previewLines * 1.5}em`, overflow: 'hidden' } : undefined}
      >
        {children}
      </div>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="mt-1.5 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        {expanded ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />}
        {expanded ? '收起' : `展开全部 (${charCount} 字符, ${lineCount} 行)`}
      </button>
    </div>
  )
}
```

- [ ] **Step 4: Run tests — expect GREEN**

```bash
cd ui && npm test -- --run collapsible-result 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/collapsible-result.tsx \
  ui/src/components/agent/tool-renderers/collapsible-result.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): CollapsibleResult primitive

Shared wrapper for long tool outputs. Below charThreshold (default
3000), renders children as-is — no UI chrome. Above threshold, shows
collapsed preview (~15 lines via max-height clamp) with a toggle
chevron + label '展开全部 (N 字符, M 行)' / '收起'.

Uses extractText() to walk the React tree and measure content
length without forcing children into a controlled value type.

4 RTL tests cover: below-threshold passthrough, threshold trigger,
toggle expand/collapse, custom charThreshold prop."
```

---

## Task 3 — Dispatcher + `DefaultResultRenderer`, delete stub, migrate callers

**Files:**
- Create: `ui/src/components/agent/tool-renderers/index.tsx`
- Create: `ui/src/components/agent/tool-renderers/default-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/index.test.tsx`
- Delete: `ui/src/components/agent/tool-result-renderers.tsx`
- Modify: `ui/src/components/chat/ChatToolBlock.tsx` (import path swap)
- Modify: `ui/src/components/agent/ToolActivityItem.tsx` (import path swap)
- Modify: `ui/src/components/chat/ChatToolBlock.test.tsx` (any import path swap)

- [ ] **Step 1: Write failing dispatcher test**

`ui/src/components/agent/tool-renderers/index.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { ToolResultRenderer } from './index'

// Mock each specialized renderer so we can verify dispatch
vi.mock('./write-result', () => ({
  WriteResultRenderer: () => <div data-testid="write-renderer">write</div>,
}))
vi.mock('./edit-result', () => ({
  EditResultRenderer: () => <div data-testid="edit-renderer">edit</div>,
}))
vi.mock('./read-result', () => ({
  ReadResultRenderer: () => <div data-testid="read-renderer">read</div>,
}))
vi.mock('./bash-result', () => ({
  BashResultRenderer: () => <div data-testid="bash-renderer">bash</div>,
}))

const baseProps = { input: {}, result: '', isError: false }

describe('ToolResultRenderer dispatch', () => {
  it('dispatches write_file to WriteResultRenderer', () => {
    render(<ToolResultRenderer toolName="write_file" {...baseProps} />)
    expect(screen.getByTestId('write-renderer')).toBeInTheDocument()
  })

  it('dispatches edit to EditResultRenderer', () => {
    render(<ToolResultRenderer toolName="edit" {...baseProps} />)
    expect(screen.getByTestId('edit-renderer')).toBeInTheDocument()
  })

  it('dispatches read_file to ReadResultRenderer', () => {
    render(<ToolResultRenderer toolName="read_file" {...baseProps} />)
    expect(screen.getByTestId('read-renderer')).toBeInTheDocument()
  })

  it('dispatches bash to BashResultRenderer', () => {
    render(<ToolResultRenderer toolName="bash" {...baseProps} />)
    expect(screen.getByTestId('bash-renderer')).toBeInTheDocument()
  })

  it('falls back to DefaultResultRenderer for unknown tool', () => {
    render(<ToolResultRenderer toolName="some_mcp_tool" {...baseProps} result="result text" />)
    // DefaultResultRenderer renders result text (no mock for it — real component)
    expect(screen.getByText('result text')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Implement `default-result.tsx`**

`ui/src/components/agent/tool-renderers/default-result.tsx`:

```tsx
import * as React from 'react'
import { cn } from '@/lib/utils'
import { CollapsibleResult } from './collapsible-result'

interface DefaultResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * Fallback for MCP tools and any unmatched built-in. Tries to parse
 * `result` as JSON and render as a key-value table; otherwise falls
 * back to plain text wrapped in CollapsibleResult.
 */
export function DefaultResultRenderer({
  result,
  isError,
}: DefaultResultRendererProps): React.ReactElement {
  let parsed: Record<string, unknown> | null = null
  try {
    const candidate = JSON.parse(result) as unknown
    if (
      candidate !== null &&
      typeof candidate === 'object' &&
      !Array.isArray(candidate)
    ) {
      parsed = candidate as Record<string, unknown>
    }
  } catch {
    // not JSON — fall through to plain text
  }

  if (parsed) {
    return (
      <div className="rounded-md bg-muted/30 p-2 text-xs font-mono">
        <table className="w-full">
          <tbody>
            {Object.entries(parsed).map(([k, v]) => (
              <tr key={k} className="align-top">
                <td className="font-medium text-foreground pr-2 whitespace-nowrap">{k}</td>
                <td className="text-muted-foreground break-all">
                  {typeof v === 'string' ? v : JSON.stringify(v)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }

  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <pre
        className={cn(
          'whitespace-pre-wrap break-all text-xs px-3 py-2 rounded-md',
          isError ? 'text-destructive bg-destructive/5' : 'text-muted-foreground bg-muted/20',
        )}
      >
        {result}
      </pre>
    </CollapsibleResult>
  )
}
```

- [ ] **Step 3: Implement dispatcher `index.tsx`**

`ui/src/components/agent/tool-renderers/index.tsx`:

```tsx
import * as React from 'react'
import { WriteResultRenderer } from './write-result'
import { EditResultRenderer } from './edit-result'
import { ReadResultRenderer } from './read-result'
import { BashResultRenderer } from './bash-result'
import { DefaultResultRenderer } from './default-result'

export interface ToolResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * Dispatcher for tool result rendering. Switches by uClaw's
 * snake_case tool names (not Proma's PascalCase). Phase 1 covers
 * the four highest-traffic tools + a JSON-aware fallback.
 * Phase 2 will add grep / glob / web_fetch / web_search.
 */
export function ToolResultRenderer({
  toolName,
  input,
  result,
  isError,
}: ToolResultRendererProps): React.ReactElement {
  const props = { input, result, isError }
  switch (toolName) {
    case 'write_file':
      return <WriteResultRenderer {...props} />
    case 'edit':
      return <EditResultRenderer {...props} />
    case 'read_file':
      return <ReadResultRenderer {...props} />
    case 'bash':
      return <BashResultRenderer {...props} />
    default:
      return <DefaultResultRenderer toolName={toolName} {...props} />
  }
}
```

- [ ] **Step 4: Create stub renderer files (real impls land Tasks 4-7)**

To make TypeScript compile + the dispatcher tests pass, the imported files must exist. Create minimal stubs:

`ui/src/components/agent/tool-renderers/write-result.tsx`:
```tsx
export function WriteResultRenderer(_props: { input: Record<string, unknown>; result: string; isError: boolean }): React.ReactElement {
  return <div>write placeholder</div>
}
// eslint-disable-next-line @typescript-eslint/no-unused-vars
import * as React from 'react'
```

Same one-line stubs for `edit-result.tsx`, `read-result.tsx`, `bash-result.tsx` (substitute renderer name + placeholder text). Tasks 4-7 replace these.

- [ ] **Step 5: Delete the old stub**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1
rm ui/src/components/agent/tool-result-renderers.tsx
```

- [ ] **Step 6: Migrate the 2 importers**

In `ui/src/components/chat/ChatToolBlock.tsx`, find the import at L17:
```tsx
// BEFORE:
import { ToolResultRenderer } from '@/components/agent/tool-result-renderers'
// AFTER:
import { ToolResultRenderer } from '@/components/agent/tool-renderers'
```

In `ui/src/components/agent/ToolActivityItem.tsx`, find the import at L25:
```tsx
// BEFORE:
import { ToolResultRenderer } from './tool-result-renderers'
// AFTER:
import { ToolResultRenderer } from './tool-renderers'
```

Grep for any other importer:
```bash
grep -rn "from.*tool-result-renderers" ui/src 2>&1 | head
```

Expected: empty after the 2 swaps. If anything else surfaces, swap it too.

- [ ] **Step 7: Run tests + tsc**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run "tool-renderers|ChatToolBlock" 2>&1 | tail -15
```

Expected: clean tsc; new dispatcher tests pass (5); existing ChatToolBlock test still passes (the dispatcher's external surface is unchanged).

- [ ] **Step 8: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/index.tsx \
  ui/src/components/agent/tool-renderers/default-result.tsx \
  ui/src/components/agent/tool-renderers/index.test.tsx \
  ui/src/components/agent/tool-renderers/write-result.tsx \
  ui/src/components/agent/tool-renderers/edit-result.tsx \
  ui/src/components/agent/tool-renderers/read-result.tsx \
  ui/src/components/agent/tool-renderers/bash-result.tsx \
  ui/src/components/chat/ChatToolBlock.tsx \
  ui/src/components/agent/ToolActivityItem.tsx
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 rm \
  ui/src/components/agent/tool-result-renderers.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): dispatcher + DefaultResultRenderer foundation

Replaces the 21-line stub at ui/src/components/agent/tool-result-renderers.tsx
with a proper directory tool-renderers/ housing:
  - index.tsx — dispatcher that switches by uClaw's snake_case tool
    names (write_file / edit / read_file / bash) with fallback
  - default-result.tsx — JSON parse → key-value table, else plain
    text wrapped in CollapsibleResult; covers MCP tools and any
    built-in tool not yet specialized
  - write/edit/read/bash-result.tsx — placeholder shells (real
    Pierre-based impls land in tasks 4-7)

Migration: 2 importers (ChatToolBlock, ToolActivityItem) swap from
'tool-result-renderers' to 'tool-renderers'. External
ToolResultRenderer surface (toolName/input/result/isError props)
unchanged so callers don't need other adjustments.

5 dispatcher tests verify each named tool routes to its renderer +
unknown names fall back to default."
```

---

## Task 4 — `WriteResultRenderer` (Pierre `MultiFileDiff`)

**Files:**
- Modify (replace stub): `ui/src/components/agent/tool-renderers/write-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/write-result.test.tsx`

- [ ] **Step 1: Write the failing test**

`ui/src/components/agent/tool-renderers/write-result.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { resolvedThemeAtom } from '@/atoms/theme'
import { WriteResultRenderer } from './write-result'

// Mock Pierre — we don't need its real rendering, just verify props
vi.mock('@pierre/diffs', () => ({
  MultiFileDiff: ({ files, theme }: { files: unknown; theme: string }) => (
    <div data-testid="pierre-multifile" data-theme={theme}>
      {JSON.stringify(files)}
    </div>
  ),
}))

function renderWithTheme(theme: 'light' | 'dark', el: React.ReactElement) {
  const store = createStore()
  store.set(resolvedThemeAtom, theme)
  return render(<Provider store={store}>{el}</Provider>)
}

describe('WriteResultRenderer', () => {
  it('renders Pierre MultiFileDiff with new file content as additions', () => {
    renderWithTheme('light',
      <WriteResultRenderer
        input={{ path: 'src/foo.ts', content: 'console.log("hi")' }}
        result=""
        isError={false}
      />,
    )
    const pierre = screen.getByTestId('pierre-multifile')
    expect(pierre).toHaveAttribute('data-theme', 'one-light')
    expect(pierre.textContent).toContain('src/foo.ts')
    expect(pierre.textContent).toContain('console.log')
    expect(pierre.textContent).toContain('"oldContent":""')  // new-file diff has empty old
  })

  it('uses dark theme when resolved theme is dark', () => {
    renderWithTheme('dark',
      <WriteResultRenderer
        input={{ path: 'a.md', content: '# Hello' }}
        result=""
        isError={false}
      />,
    )
    expect(screen.getByTestId('pierre-multifile')).toHaveAttribute('data-theme', 'one-dark-pro')
  })

  it('renders error state when isError', () => {
    renderWithTheme('light',
      <WriteResultRenderer
        input={{ path: 'a.ts', content: 'x' }}
        result="permission denied"
        isError={true}
      />,
    )
    expect(screen.getByText(/permission denied/)).toBeInTheDocument()
    expect(screen.queryByTestId('pierre-multifile')).not.toBeInTheDocument()
  })

  it('gracefully handles missing path/content (empty input)', () => {
    renderWithTheme('light',
      <WriteResultRenderer input={{}} result="" isError={false} />,
    )
    // Should render a placeholder, not crash
    expect(screen.getByText(/missing path|无路径/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd ui && npm test -- --run write-result 2>&1 | tail -10
```

Expected: tests fail (stub renderer returns "write placeholder").

- [ ] **Step 3: Implement `write-result.tsx`**

```tsx
import * as React from 'react'
import { MultiFileDiff } from '@pierre/diffs'
import { usePierreTheme, detectLang } from './pierre-theme'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

export function WriteResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? ''
  const content = (input.content as string | undefined) ?? ''
  const theme = usePierreTheme()

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '写入失败'}
      </div>
    )
  }

  if (!path) {
    return (
      <div className="rounded-md bg-muted/30 text-muted-foreground text-xs px-3 py-2 italic">
        missing path
      </div>
    )
  }

  return (
    <div className="rounded-md border border-border bg-content-area overflow-auto max-h-[400px]">
      <MultiFileDiff
        theme={theme}
        files={[
          {
            path,
            oldContent: '',
            newContent: content,
            language: detectLang(path),
          },
        ]}
      />
    </div>
  )
}
```

If Pierre's `MultiFileDiff` prop shape differs from `files: Array<{ path, oldContent, newContent, language }>`, inspect the package's TypeScript types and adapt. Common variants:
- `files: Array<{ name: string, oldText: string, newText: string }>`
- Single-file API: just `oldContent` + `newContent` + `path` at top level

Read `node_modules/@pierre/diffs/dist/*.d.ts` and adapt. Whatever the actual shape, the test mock at Step 1 can be updated to match — the assertion is "the renderer passes these fields through", not "the field names are exact".

- [ ] **Step 4: Re-run tests — expect GREEN**

```bash
cd ui && npm test -- --run write-result 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/write-result.tsx \
  ui/src/components/agent/tool-renderers/write-result.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): WriteResultRenderer via Pierre MultiFileDiff

Renders write_file tool input as a new-file diff (empty old → full
new). Pierre handles syntax highlighting via Shiki and adds the
visible left-edge green stripe per added line that Proma users
recognise. Max height 400px with internal scroll.

Streaming: Pierre re-renders gracefully as input.content grows
during tool input streaming (the existing ToolActivityItem spinner
handles the loading state separately).

Error state: shows the result message in destructive-tinted box.
Empty input: shows 'missing path' placeholder (no crash).

4 RTL tests with mocked Pierre verify theme propagation,
add/error/empty states."
```

---

## Task 5 — `EditResultRenderer` (Pierre `FileDiff` per edit)

**Files:**
- Modify (replace stub): `ui/src/components/agent/tool-renderers/edit-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/edit-result.test.tsx`

- [ ] **Step 1: Write the failing test**

`ui/src/components/agent/tool-renderers/edit-result.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { resolvedThemeAtom } from '@/atoms/theme'
import { EditResultRenderer } from './edit-result'

vi.mock('@pierre/diffs', () => ({
  FileDiff: ({ path, oldContent, newContent, theme }: { path: string; oldContent: string; newContent: string; theme: string }) => (
    <div data-testid="pierre-filediff" data-theme={theme} data-path={path}>
      {oldContent}|{newContent}
    </div>
  ),
}))

function renderWithTheme(theme: 'light' | 'dark', el: React.ReactElement) {
  const store = createStore()
  store.set(resolvedThemeAtom, theme)
  return render(<Provider store={store}>{el}</Provider>)
}

describe('EditResultRenderer', () => {
  it('renders one FileDiff per edit in the array', () => {
    renderWithTheme('light',
      <EditResultRenderer
        input={{
          path: 'src/foo.ts',
          edits: [
            { old_text: 'foo', new_text: 'bar' },
            { old_text: 'baz', new_text: 'qux' },
          ],
        }}
        result=""
        isError={false}
      />,
    )
    const diffs = screen.getAllByTestId('pierre-filediff')
    expect(diffs).toHaveLength(2)
    expect(diffs[0]).toHaveTextContent('foo|bar')
    expect(diffs[1]).toHaveTextContent('baz|qux')
    expect(diffs[0]).toHaveAttribute('data-path', 'src/foo.ts')
  })

  it('handles single-edit (non-array) input shape defensively', () => {
    renderWithTheme('light',
      <EditResultRenderer
        input={{
          path: 'a.ts',
          edits: { old_text: 'only', new_text: 'one' }, // not an array
        }}
        result=""
        isError={false}
      />,
    )
    const diffs = screen.getAllByTestId('pierre-filediff')
    expect(diffs).toHaveLength(1)
    expect(diffs[0]).toHaveTextContent('only|one')
  })

  it('renders error state when isError', () => {
    renderWithTheme('light',
      <EditResultRenderer
        input={{ path: 'a.ts', edits: [{ old_text: 'x', new_text: 'y' }] }}
        result="text not found"
        isError={true}
      />,
    )
    expect(screen.getByText(/text not found/)).toBeInTheDocument()
    expect(screen.queryByTestId('pierre-filediff')).not.toBeInTheDocument()
  })

  it('handles missing edits array gracefully', () => {
    renderWithTheme('light',
      <EditResultRenderer input={{ path: 'a.ts' }} result="" isError={false} />,
    )
    expect(screen.getByText(/no edits/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd ui && npm test -- --run edit-result 2>&1 | tail -10
```

- [ ] **Step 3: Implement `edit-result.tsx`**

```tsx
import * as React from 'react'
import { FileDiff } from '@pierre/diffs'
import { usePierreTheme, detectLang } from './pierre-theme'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

interface EditEntry {
  old_text: string
  new_text: string
  insert_line?: number
}

export function EditResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? ''
  const rawEdits = input.edits
  // uClaw's edit tool uses batch edits; defensive: also accept single edit
  const edits: EditEntry[] = Array.isArray(rawEdits)
    ? (rawEdits as EditEntry[])
    : rawEdits && typeof rawEdits === 'object'
      ? [rawEdits as EditEntry]
      : []
  const theme = usePierreTheme()

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '编辑失败'}
      </div>
    )
  }

  if (!path || edits.length === 0) {
    return (
      <div className="rounded-md bg-muted/30 text-muted-foreground text-xs px-3 py-2 italic">
        no edits to display
      </div>
    )
  }

  return (
    <div className="space-y-2 max-h-[500px] overflow-auto">
      {edits.map((edit, i) => (
        <FileDiff
          key={i}
          theme={theme}
          path={path}
          oldContent={edit.old_text ?? ''}
          newContent={edit.new_text ?? ''}
          language={detectLang(path)}
        />
      ))}
    </div>
  )
}
```

(Same caveat as Task 4: if Pierre's `FileDiff` prop shape differs, adapt — the test mock + the component must agree.)

- [ ] **Step 4: Re-run tests — expect GREEN**

```bash
cd ui && npm test -- --run edit-result 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/edit-result.tsx \
  ui/src/components/agent/tool-renderers/edit-result.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): EditResultRenderer via Pierre FileDiff per edit

uClaw's edit tool takes a batch edits array (each with old_text /
new_text / optional insert_line). Renderer iterates and stacks one
FileDiff per edit vertically (max-height 500px scroll). Pierre
handles unified-diff coloring + syntax highlight.

Defensive: also accepts a single edit object (not in array) to
survive future schema drift. Empty edits or missing path render
a 'no edits' placeholder.

Note: Proma uses old_string / new_string field names; uClaw uses
old_text / new_text. The renderer matches uClaw's schema.

Line-number smart offset (Proma's resolveAndReadFile pattern)
deferred to Phase 2 — most edits surface enough context in Pierre's
default unified-diff view.

4 RTL tests cover batch + single-edit shapes, error state,
missing-edits placeholder."
```

---

## Task 6 — `ReadResultRenderer` (Pierre `File`)

**Files:**
- Modify (replace stub): `ui/src/components/agent/tool-renderers/read-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/read-result.test.tsx`

- [ ] **Step 1: Write the failing test**

`ui/src/components/agent/tool-renderers/read-result.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { resolvedThemeAtom } from '@/atoms/theme'
import { ReadResultRenderer } from './read-result'

vi.mock('@pierre/diffs', () => ({
  File: ({ path, content, theme, language }: { path: string; content: string; theme: string; language: string }) => (
    <div data-testid="pierre-file" data-theme={theme} data-path={path} data-lang={language}>
      {content}
    </div>
  ),
}))

function renderWithTheme(theme: 'light' | 'dark', el: React.ReactElement) {
  const store = createStore()
  store.set(resolvedThemeAtom, theme)
  return render(<Provider store={store}>{el}</Provider>)
}

describe('ReadResultRenderer', () => {
  it('renders Pierre File with path, content, detected language, theme', () => {
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'src/foo.ts' }}
        result='console.log("hello")'
        isError={false}
      />,
    )
    const f = screen.getByTestId('pierre-file')
    expect(f).toHaveAttribute('data-theme', 'one-light')
    expect(f).toHaveAttribute('data-path', 'src/foo.ts')
    expect(f).toHaveAttribute('data-lang', 'typescript')
    expect(f).toHaveTextContent('console.log("hello")')
  })

  it('renders error state when isError', () => {
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'missing.ts' }}
        result="ENOENT: no such file"
        isError={true}
      />,
    )
    expect(screen.getByText(/ENOENT/)).toBeInTheDocument()
    expect(screen.queryByTestId('pierre-file')).not.toBeInTheDocument()
  })

  it('strips numbered line prefixes if the result has them', () => {
    // Some SDK outputs prefix lines with "    1\tcontent". Pierre wants raw.
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'a.txt' }}
        result="     1\tline-one\n     2\tline-two"
        isError={false}
      />,
    )
    const f = screen.getByTestId('pierre-file')
    expect(f.textContent).toBe('line-one\nline-two')
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

- [ ] **Step 3: Implement `read-result.tsx`**

```tsx
import * as React from 'react'
import { File as PierreFile } from '@pierre/diffs'
import { usePierreTheme, detectLang } from './pierre-theme'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * If the SDK injects line-number prefixes like "    1\tcontent",
 * strip them so Pierre renders clean source. Tolerant of files
 * that don't have this convention (pass-through).
 */
function stripLinePrefixes(text: string): string {
  const lines = text.split('\n')
  // Detect "padded-num<TAB>" prefix pattern
  const pattern = /^\s*\d+\t/
  if (lines.every((l) => l === '' || pattern.test(l))) {
    return lines.map((l) => l.replace(pattern, '')).join('\n')
  }
  return text
}

export function ReadResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? ''
  const theme = usePierreTheme()

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '读取失败'}
      </div>
    )
  }

  const content = stripLinePrefixes(result)
  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <div className="rounded-md border border-border bg-content-area overflow-auto max-h-[400px]">
        <PierreFile
          theme={theme}
          path={path}
          content={content}
          language={detectLang(path)}
        />
      </div>
    </CollapsibleResult>
  )
}
```

- [ ] **Step 4: Re-run tests — expect GREEN**

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/read-result.tsx \
  ui/src/components/agent/tool-renderers/read-result.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): ReadResultRenderer via Pierre File

Pierre's File component renders code with syntax highlighting +
line numbers + no diff coloring (read = no change). Max height
400px with internal scroll; long files (>3000 chars) get the
CollapsibleResult wrapper for preview + expand.

stripLinePrefixes(): some agent SDKs prefix output with line
numbers like '     1\\tcontent'. The helper detects and strips
this convention to give Pierre clean source, while passing
through files that don't have the prefix.

3 RTL tests cover happy path, error state, line-prefix strip."
```

---

## Task 7 — `BashResultRenderer` (terminal style)

**Files:**
- Modify (replace stub): `ui/src/components/agent/tool-renderers/bash-result.tsx`
- Create: `ui/src/components/agent/tool-renderers/bash-result.test.tsx`

- [ ] **Step 1: Write the failing test**

`ui/src/components/agent/tool-renderers/bash-result.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { BashResultRenderer } from './bash-result'

describe('BashResultRenderer', () => {
  it('renders command echo with $ prefix + stdout body', () => {
    render(
      <BashResultRenderer
        input={{ command: 'ls -la' }}
        result="total 8\ndrwxr-xr-x   2 root root 4096 May 18 10:00 ."
        isError={false}
      />,
    )
    expect(screen.getByText('$ ls -la')).toBeInTheDocument()
    expect(screen.getByText(/total 8/)).toBeInTheDocument()
  })

  it('highlights lines matching error patterns in red', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'cargo build' }}
        result="compiling foo v0.1.0\nerror: cannot find module\nDone."
        isError={false}
      />,
    )
    const errorLine = container.querySelector('.text-red-400')
    expect(errorLine).toBeInTheDocument()
    expect(errorLine?.textContent).toContain('error: cannot find module')
  })

  it('marks all lines red when isError is true', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'false' }}
        result="line 1\nline 2"
        isError={true}
      />,
    )
    const redLines = container.querySelectorAll('.text-red-400')
    expect(redLines.length).toBeGreaterThanOrEqual(2)
  })

  it('handles empty command gracefully', () => {
    render(<BashResultRenderer input={{}} result="output" isError={false} />)
    expect(screen.getByText('$')).toBeInTheDocument()
    expect(screen.getByText('output')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

- [ ] **Step 3: Implement `bash-result.tsx`**

```tsx
import * as React from 'react'
import { cn } from '@/lib/utils'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

const ERROR_PATTERNS = /(error|exception|traceback|failed|fatal|panic|warning)/i

export function BashResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const command = (input.command as string | undefined) ?? ''
  const lines = result.split('\n')

  return (
    <CollapsibleResult charThreshold={2000} previewLines={20}>
      <div className="rounded-md bg-zinc-950 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
        <div className="text-emerald-400 mb-1.5">$ {command}</div>
        <pre className="whitespace-pre-wrap break-all">
          {lines.map((line, i) => {
            const isErrorLine = isError || ERROR_PATTERNS.test(line)
            return (
              <div key={i} className={cn(isErrorLine && 'text-red-400')}>
                {line || ' ' /* preserve blank lines */}
              </div>
            )
          })}
        </pre>
      </div>
    </CollapsibleResult>
  )
}
```

Note: the `bg-zinc-950` + `text-zinc-100` choice is intentional terminal styling — it overrides theme tokens for this specific renderer (terminals are always dark; Proma does the same). This isn't a theme-token violation per CLAUDE.md because the convention applies.

- [ ] **Step 4: Re-run tests — expect GREEN**

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/tool-renderers/bash-result.tsx \
  ui/src/components/agent/tool-renderers/bash-result.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(tool-renderers): BashResultRenderer terminal-style with stderr highlight

Renders bash tool output in terminal aesthetic: dark zinc-950
background, emerald-400 command echo with \$ prefix, monospace
body. Lines matching error/exception/traceback/failed/fatal/panic
patterns get red-400 — heuristic but cheap and catches most
useful cases. When isError is true, ALL lines render red.

Terminal styling deliberately overrides theme tokens (terminals
are always dark by convention) — same approach Proma uses, not a
theme-token violation.

CollapsibleResult wraps long outputs (>2000 chars → preview 20
lines).

4 RTL tests cover happy path, in-result error line highlight,
isError full-red mode, empty command."
```

---

## Task 8 — "预览" button on `ToolActivityItem`

**Files:**
- Modify: `ui/src/components/agent/ToolActivityItem.tsx`
- Modify: `ui/src/components/agent/ToolActivityItem.test.tsx` (if exists; otherwise create)

- [ ] **Step 1: Find the activity row + understand its props**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1
sed -n '176,236p' ui/src/components/agent/ToolActivityItem.tsx
```

Identify: where in `ActivityRow` to inject the preview button (just before the chevron in the right cluster).

- [ ] **Step 2: Add the preview-eligibility helper + button**

In `ui/src/components/agent/ToolActivityItem.tsx`, near the top of the file (after imports), add:

```ts
import { useSetAtom } from 'jotai'
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'

const PREVIEW_ELIGIBLE_TOOLS = new Set(['write_file', 'edit', 'plan_write'])

function shouldShowPreviewButton(
  toolName: string,
  input: Record<string, unknown>,
): boolean {
  if (!PREVIEW_ELIGIBLE_TOOLS.has(toolName)) return false
  const path = (input.path ?? input.file_path) as string | undefined
  return Boolean(path && path.length > 0)
}
```

Inside the `ActivityRow` component body, near the top:

```tsx
const openPreviewTab = useSetAtom(openPreviewTabAction)

const handlePreview = React.useCallback(
  (e: React.MouseEvent) => {
    e.stopPropagation()  // don't trigger the row's open-details
    const path = (input.path ?? input.file_path) as string | undefined
    if (!path) return
    // Best-effort mountId — Phase 1 uses 'workspace:default'; future
    // could thread per-session mount info through tool activity payload
    openPreviewTab({
      target: {
        mountId: 'workspace:default',
        relPath: path,
        name: path.split('/').pop() ?? path,
        absolutePath: path,
        sessionId: undefined,
      },
      source: 'agent',
    })
  },
  [input, openPreviewTab],
)
```

In the JSX of `ActivityRow`, find the spot just BEFORE the existing chevron / expand button. Inject:

```tsx
{shouldShowPreviewButton(toolName, input) && (
  <button
    type="button"
    onClick={handlePreview}
    className="shrink-0 px-2 py-0.5 text-[11px] text-muted-foreground hover:text-foreground hover:bg-muted/60 rounded border border-border/40 transition-colors"
    aria-label={`预览 ${(input.path ?? input.file_path) as string}`}
  >
    预览
  </button>
)}
```

Adjust the surrounding flex layout if needed so the button doesn't break the existing alignment. Likely the existing row is `<div className="flex items-center gap-2 ...">` — the new button slots in cleanly.

- [ ] **Step 3: Write/update test**

Check if `ToolActivityItem.test.tsx` exists:

```bash
ls ui/src/components/agent/ToolActivityItem.test.tsx 2>&1
```

If exists, add a new test case. If not, create one. Either way, the test:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { previewTabsAtom } from '@/atoms/preview-panel-atoms'
// Import ActivityRow if it's exported; otherwise wrap the file's main export
// import { ToolActivityList } from './ToolActivityItem'

vi.mock('@/components/agent/tool-renderers', () => ({
  ToolResultRenderer: () => <div>result</div>,
}))

describe('ToolActivityItem preview button', () => {
  function makeStore() {
    return createStore()
  }

  it('shows 预览 button for write_file tool', () => {
    const store = makeStore()
    // Render minimal ActivityRow / ToolActivityList with a write_file activity
    // Test implementation depends on the actual ActivityRow export surface;
    // if not directly exported, test via the parent ToolActivityList with
    // an activities prop containing a write_file entry.
    // ...
    // Pseudo-assert:
    // expect(screen.getByRole('button', { name: /预览/ })).toBeInTheDocument()
  })

  it('does NOT show 预览 button for read_file tool', () => {
    // Same pattern, with read_file activity → button absent
  })

  it('clicking 预览 opens the preview tab', () => {
    // Click button → assert previewTabsAtom now contains the file
  })
})
```

Adapt the test to the actual `ActivityRow` / `ToolActivityList` export surface. If `ActivityRow` isn't exported separately, the test renders the list with a fixture activities array containing a single `write_file` entry, then asserts on the rendered button.

- [ ] **Step 4: Run tests + tsc**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run ToolActivityItem 2>&1 | tail -15
```

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 add \
  ui/src/components/agent/ToolActivityItem.tsx \
  ui/src/components/agent/ToolActivityItem.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1 commit -m "feat(agent): 预览 button on tool activity rows for file-write tools

Per user reference screenshot (2026-05-18): write/edit tool activity
rows show an always-visible '预览' button on the right that opens
the file in the multi-tab preview panel (PR #190's
openPreviewTabAction with source: 'agent').

Eligibility: write_file, edit, plan_write — tools whose input
includes a file path the user might want to inspect mid-stream.
read_file is excluded (the result IS the preview, no need for a
separate panel).

The button is always-visible (not hover-only) so the affordance
is the same regardless of the user's auto-preview setting. When
auto-preview is ON, the button is functionally redundant — but
visually consistent.

3 RTL tests cover eligibility logic + click-to-open."
```

---

## Task 9 — End-to-end verification + cleanup grep

**Files:**
- No new files; verification only

- [ ] **Step 1: Full test suite**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1/ui && npm test -- --run 2>&1 | tail -10
```

Expected: existing tests pass + new ones added (collapsible-result 4, dispatcher 5, write 4, edit 4, read 3, bash 4, ToolActivityItem 3 = ~27 new tests). Pre-existing failures from earlier (kaleidoscope / SearchPalette / settings) may still fail — only block on NEW regressions.

- [ ] **Step 2: TypeScript clean**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 3: Verify no residual imports of the deleted stub**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-tool-result-beautification-p1
grep -rn "tool-result-renderers" ui/src --include="*.tsx" --include="*.ts" 2>&1 | head
```

Expected: empty (all migrated to `tool-renderers`).

- [ ] **Step 4: Verify Pierre actually imported in renderers (not phantom)**

```bash
grep -rn "from '@pierre/diffs'" ui/src/components/agent/tool-renderers/ 2>&1
```

Expected: 3 hits (`write-result.tsx`, `edit-result.tsx`, `read-result.tsx`).

- [ ] **Step 5: Manual smoke test checklist (for dev build)**

This step is a checklist for whoever runs the dev build after merge — no commit needed unless something fails.

```
Manual verification:
  □ Ask agent to "write a hello.md file" → write_file activity shows '预览' button on right + expanded result shows Pierre MultiFileDiff with green stripes
  □ Click '预览' button → file opens in preview panel as agent-source tab (✨ marker)
  □ Ask agent to "edit hello.md change Hello to Hi" → edit activity shows FileDiff per edit
  □ Ask agent to "read package.json" → read result shows Pierre File with syntax highlight
  □ Ask agent to "run ls -la" → bash result shows terminal style with $ prefix
  □ Ask agent to call any MCP / unknown tool → result falls back to DefaultResultRenderer
  □ Run a long bash command (e.g. `find . -type f`) → result shows '展开全部' collapse footer
  □ Toggle light/dark theme → Pierre re-renders with matching theme
```

- [ ] **Step 6 (optional): If anything from Step 5 fails, fix + commit; otherwise no commit needed for this task**

---

## Self-review

**Spec coverage** — each spec section maps to a task:

| Spec section | Task |
|---|---|
| Pierre integration | Task 1 |
| Tool name dispatcher | Task 3 |
| Per-tool input shape | Locked at plan top + verified in each Task 4-7 |
| WriteResultRenderer | Task 4 |
| EditResultRenderer | Task 5 |
| ReadResultRenderer | Task 6 |
| BashResultRenderer | Task 7 |
| DefaultResultRenderer | Task 3 |
| CollapsibleResult | Task 2 |
| "预览" button | Task 8 |
| Streaming `write_file` | Inherent in Task 4 (Pierre re-renders on input change) |
| Testing strategy | TDD throughout (each task has failing-test step before impl) |
| Edge cases | Each renderer task addresses missing-path / empty-input / error cases |
| File map | Locked at plan top |
| Implementation order | Tasks 1-9 in order |
| Risks (Pierre CSS, theme sub-dep, edit shape) | Surfaced in Task 1 Step 2 + Task 5 defensive code |
| Phase 2 scope | Out of plan (deferred per spec) |

**Placeholder scan** — no TBD/TODO/FIXME inside code blocks. The text mentions "Phase 2" as a deferred scope marker (not a placeholder).

**Type consistency:**
- `ToolResultRendererProps` shape consistent across Tasks 3, 4-7, 8
- `PreviewTabSource = 'agent'` for preview-button task (matches PR #190 contract)
- `openPreviewTabAction({ target, source })` payload matches Task 7 of PR #190
- `usePierreTheme()` returns `'one-light' | 'one-dark-pro'` consistently
- `detectLang()` returns string, consumed as Pierre's `language` prop in 3 renderers

**Cross-task contracts:** dispatcher (Task 3) imports from each per-tool file (Tasks 4-7); the stubs created in Task 3 Step 4 let the dispatcher tests pass before the real impls land — clean dependency order.

**PR shape:** 9 bisectable commits, target `main`. Tasks 1-3 are foundation (no user-visible change); Tasks 4-7 each ship a per-tool visible improvement; Task 8 adds the preview button affordance; Task 9 is verification.
