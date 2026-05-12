# W4c — File-Path Chips + Shift-Click Attach Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every file reference in agent messages a clickable chip that opens the preview panel, and make Shift-click on chips OR rail tree entries add the file to the composer's pending-attachments list — uniform semantics across both surfaces.

**Architecture:** A new `remark` plugin walks the markdown AST and rewrites file-reference nodes (markdown links, single-filename inline code, slash-bearing path tokens) into custom HAST nodes that `react-markdown` renders via a new `<FilePathChip>` component. A `useFileChipResolver` hook batches existence checks across all visible chips into one `preview_resolve_chips` Tauri call per ~50 ms tick, caches results in a jotai atom, and busts entries on W3 `Created`/`Removed` file-change events. Shift-click on a chip OR on a rail `FileTreeNode` dispatches a new `addPendingAttachmentAction` atom write that eagerly fetches bytes via the existing `preview_read_bytes` command and pushes a `PendingAttachment` onto the shared composer atom.

**Tech Stack:** React 18 + TypeScript · `react-markdown` 9 / `remark-gfm` (already in tree) · `unified` / mdast-util-to-hast `data.hName` pattern for custom nodes · `jotai` `atomWithStorage` for cache · Rust `tokio::fs::metadata` for existence checks · `sonner` for toast feedback.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6.8 (file-path chips), plus inline-approved W4c brainstorming decisions captured in the conversation that produced this plan.

**Branch base:** Start `claude/w4c-chips-and-shift-attach` from `origin/main` **after Task 1 merges PR #109**, so the polish chrome is already in the base.

**Out of W4c scope** (deferred to a possible W4d / W5):
- Inline editing for any format (CodeMirror 6 / TipTap / `preview_write_text`)
- DiffRenderer
- Detached preview window
- Cmd/Ctrl-click semantics (placeholder no-op)
- Bare-filename detection in prose (rejected as too noisy)

---

## Pre-flight

- [ ] **Confirm starting state**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git log --oneline -3
# Expect: 3acb23a W4b: Rich format renderers ...
```

- [ ] **Baselines (must record before starting)**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: test result: ok. 391 passed; 0 failed
cd ../ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
cd ui && npm test -- --run 2>&1 | tail -3
# Expect: Test Files 41 passed (41), Tests 279 passed (279)
```

Record these. After Task 1 the Rust baseline is unchanged; the UI baseline may bump (#109 added ~6 polish files but no new tests). Re-record after Task 1.

- [ ] **Branch hygiene note** — the harness silently flips branches between commands. **Every subagent prompt in this plan must verify the branch at start, before commit, and after commit.** If a subagent finds itself on a different branch, it must STOP and report — not push, not continue.

---

## File Structure

### New TypeScript modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `ui/src/components/preview/chips/line-col-parser.ts` | ~30 | Parse `path:42` / `path:42:15` → `{ path, line?, col? }`. Pure. |
| `ui/src/components/preview/chips/file-type-colors.ts` | ~60 | `getChipColors(ext)` → `{ icon, bg, fg }` mapped to theme tokens. |
| `ui/src/components/preview/chips/FilePathChip.tsx` | ~120 | Visual chip + click + Shift-click handlers. Pure of resolver logic. |
| `ui/src/components/preview/chips/markdownFileChipPlugin.ts` | ~110 | Remark plugin: 3 detection patterns, emits custom HAST nodes via `data.hName`. |
| `ui/src/components/preview/chips/useFileChipResolver.ts` | ~110 | Hook: batched async existence check, LRU cache, file-change invalidation. |
| `ui/src/atoms/preview-chip-atoms.ts` | ~80 | `chipResolutionCacheAtom` + `addPendingAttachmentAction`. |

### Modified files

| Path | Change | Lines (approx) |
|---|---|---|
| `ui/src/components/preview/utils/ext-classifier.ts` | Export `ALL_PREVIEWABLE_EXTS` + `isPreviewableExt` helper | +20 |
| `ui/src/components/ai-elements/message.tsx` | Add plugin to `REMARK_PLUGINS` + `'file-path-chip'` to `MARKDOWN_COMPONENTS` | +6 |
| `ui/src/components/agent/ContentBlock.tsx` | Add plugin to `THINKING_REMARK_PLUGINS` + entry to `THINKING_MD_COMPONENTS` | +6 |
| `ui/src/components/files-rail/workspace/FileTreeNode.tsx` | Thread `MouseEvent` to `onFileClick` so callers can read `shiftKey` | +5 |
| `ui/src/components/files-rail/workspace/MountSection.tsx` | Same prop signature change | +3 |
| `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx` | Same prop signature change | +3 |
| `ui/src/components/files-rail/index.tsx` | Same prop signature change | +2 |
| `ui/src/components/agent/SidePanel.tsx` | Branch on `event.shiftKey` in `onFileClick` callback | +12 |
| `src-tauri/src/preview/commands.rs` | Add `preview_resolve_chips` | +60 |
| `src-tauri/src/preview/resolver.rs` | Add `resolve_chip_candidate` helper | +40 |
| `src-tauri/src/preview/types.rs` | Add `ChipResolution` type | +15 |
| `src-tauri/src/tauri_commands.rs` | Re-export `preview_resolve_chips` | +1 |
| `src-tauri/src/main.rs` | Register in `invoke_handler!` | +1 |
| `src-tauri/src/preview/tests.rs` | Tests for `resolve_chip_candidate` | +60 |

Total: 6 new TS + 1 new Rust types entry; ~13 modified files. Largest single file is `FilePathChip.tsx` at ~120 LOC — well inside the 300-line target.

---

## Task 1: Rebase + merge PR #109 (operational, no code)

**Files:** none new. Operates on PR #109's branch `claude/w4a-polish-preview-ui`.

This is a **manual operational task** done by the human (per CLAUDE.md "no push without explicit ask"). The subagent-driven flow cannot do it. If you're using subagent-driven-development, this is the one task the controller handles directly with the user's approval.

- [ ] **Step 1: Confirm #109 is still polish-only and clean**

```bash
cd /Users/ryanliu/Documents/uclaw
gh pr view 109 --json files,additions,deletions,headRefName,baseRefName
# Expect: ~6 files, all under ui/src/components/preview/, head = claude/w4a-polish-preview-ui
```

If file count or paths look unexpected (e.g. it now touches `ext-classifier.ts` or `PreviewSurface.tsx`), STOP and inspect — that means it overlaps W4b. Resolve before continuing.

- [ ] **Step 2: Rebase #109 onto current main**

```bash
git fetch origin
git checkout claude/w4a-polish-preview-ui
git rebase origin/main
# Expect: no conflicts (PR #109 touches polish chrome; W4b added new sibling files)
```

If conflicts surface, they are most likely in `ext-classifier.ts` (W4b extended `RendererKind`) or `PreviewSurface.tsx` (W4b added dispatch arms) — resolve by preserving BOTH sides (W4b's additions stay, #109's polish stays).

- [ ] **Step 3: Force-push the rebased branch**

```bash
git push --force-with-lease origin claude/w4a-polish-preview-ui
```

`--force-with-lease` aborts if someone else pushed in the meantime — safer than `--force`.

- [ ] **Step 4: Verify CI green and merge**

```bash
gh pr checks 109            # wait for any checks to pass
gh pr view 109 --json mergeable,mergeStateStatus
# Expect: mergeable=MERGEABLE, mergeStateStatus=CLEAN
gh pr merge 109 --squash --delete-branch
```

- [ ] **Step 5: Update local main and confirm**

```bash
git checkout main
git pull --ff-only
git log --oneline -3
# Expect: <#109 squash sha> ... | 3acb23a W4b ...
```

- [ ] **Step 6: Start W4c branch from new main**

```bash
git checkout -b claude/w4c-chips-and-shift-attach
git push -u origin claude/w4c-chips-and-shift-attach
# pushing the empty branch upfront avoids any surprise about which branch CI sees later
```

- [ ] **Step 7: Re-record baselines on the new branch**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3   # still 391
cd ../ui && npx tsc --noEmit 2>&1 | tail -3       # clean
cd ui && npm test -- --run 2>&1 | tail -3         # 279 (or new baseline if #109 added tests)
```

If the UI test count differs, **record the new baseline** — every later "expect baseline + N" must use this number.

---

## Task 2: Extend ext-classifier with previewable-extension set

**Files:**
- Modify: `ui/src/components/preview/utils/ext-classifier.ts`
- Test: `ui/src/components/preview/utils/ext-classifier.test.ts`

The chip plugin needs a fast "is this ext renderable?" predicate so it can reject patterns like `arr.map` (where `map` would otherwise look like an extension). Today `classifyExtension` returns `'binary'` for anything we don't render — we expose that distinction as a tiny helper.

- [ ] **Step 1: Write the failing tests**

Append to `ui/src/components/preview/utils/ext-classifier.test.ts`:

```ts
describe('isPreviewableExt', () => {
  it('returns true for code/image/markdown/office extensions', () => {
    expect(isPreviewableExt('ts')).toBe(true)
    expect(isPreviewableExt('rs')).toBe(true)
    expect(isPreviewableExt('png')).toBe(true)
    expect(isPreviewableExt('md')).toBe(true)
    expect(isPreviewableExt('pdf')).toBe(true)
    expect(isPreviewableExt('docx')).toBe(true)
    expect(isPreviewableExt('xlsx')).toBe(true)
    expect(isPreviewableExt('pptx')).toBe(true)
    expect(isPreviewableExt('doc')).toBe(true)  // legacy office still rendered
  })

  it('returns false for unknown extensions', () => {
    expect(isPreviewableExt('exe')).toBe(false)
    expect(isPreviewableExt('map')).toBe(false)
    expect(isPreviewableExt('')).toBe(false)
  })

  it('is case-insensitive (callers pass lowercased but be defensive)', () => {
    expect(isPreviewableExt('TS')).toBe(true)
  })
})
```

Add the import at the top of the test file:

```ts
import { isPreviewableExt } from './ext-classifier'
```

- [ ] **Step 2: Run tests, confirm failure**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run ext-classifier 2>&1 | tail -10
# Expect: 3 failures referencing `isPreviewableExt is not a function`
```

- [ ] **Step 3: Implement**

In `ui/src/components/preview/utils/ext-classifier.ts`, append after `CODE_EXTS`:

```ts
/**
 * Every extension that produces something other than `kind: 'binary'`
 * from `classifyExtension`. Used by the chip plugin to gate ambiguous
 * tokens like `arr.map` (where the suffix isn't a real ext).
 */
export const ALL_PREVIEWABLE_EXTS: ReadonlySet<string> = new Set<string>([
  ...IMAGE_EXTS,
  ...MD_EXTS,
  'pdf',
  'docx', 'xlsx', 'pptx',
  'doc', 'xls', 'ppt',
  ...Array.from(CODE_EXTS.keys()),
])

/** True if the extension would route to a non-binary renderer. */
export function isPreviewableExt(ext: string): boolean {
  if (!ext) return false
  return ALL_PREVIEWABLE_EXTS.has(ext.toLowerCase())
}
```

- [ ] **Step 4: Run tests + tsc, confirm pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run ext-classifier 2>&1 | tail -5
# Expect: all pass
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git status --short
# Expect:
#  M ui/src/components/preview/utils/ext-classifier.ts
#  M ui/src/components/preview/utils/ext-classifier.test.ts
git add ui/src/components/preview/utils/ext-classifier.ts ui/src/components/preview/utils/ext-classifier.test.ts
git commit -m "feat(preview): isPreviewableExt + ALL_PREVIEWABLE_EXTS for chip gating"
git branch --show-current   # must show claude/w4c-chips-and-shift-attach
```

---

## Task 3: line-col-parser

**Files:**
- Create: `ui/src/components/preview/chips/line-col-parser.ts`
- Test: `ui/src/components/preview/chips/line-col-parser.test.ts`

Pure utility: takes a chip-candidate string, splits off any `:42` / `:42:15` suffix. Used by the plugin and by `FilePathChip` (the chip stores `line`/`col` separately so a future jump-to-line works).

- [ ] **Step 1: Create the test file (failing)**

Path: `ui/src/components/preview/chips/line-col-parser.test.ts`

```ts
import { describe, it, expect } from 'vitest'
import { parseLineCol } from './line-col-parser'

describe('parseLineCol', () => {
  it('returns input unchanged when no :line:col suffix', () => {
    expect(parseLineCol('src/main.rs')).toEqual({ path: 'src/main.rs' })
    expect(parseLineCol('foo.ts')).toEqual({ path: 'foo.ts' })
  })

  it('strips :line', () => {
    expect(parseLineCol('src/main.rs:42')).toEqual({ path: 'src/main.rs', line: 42 })
  })

  it('strips :line:col', () => {
    expect(parseLineCol('src/main.rs:42:15')).toEqual({
      path: 'src/main.rs',
      line: 42,
      col: 15,
    })
  })

  it('rejects bogus :suffixes (non-numeric)', () => {
    expect(parseLineCol('src/main.rs:foo')).toEqual({ path: 'src/main.rs:foo' })
    expect(parseLineCol('http://example.com')).toEqual({ path: 'http://example.com' })
  })

  it('rejects negative or zero line/col', () => {
    expect(parseLineCol('src/main.rs:0')).toEqual({ path: 'src/main.rs:0' })
    expect(parseLineCol('src/main.rs:-1')).toEqual({ path: 'src/main.rs:-1' })
  })

  it('handles Windows-style paths conservatively (treats colon after drive letter as non-line)', () => {
    // We're a Tauri/Mac+Linux app; Windows paths are not a hard requirement,
    // but we should not crash. `C:\foo` is left as `C:\foo` (one-char "line" rejected).
    expect(parseLineCol('C:foo')).toEqual({ path: 'C:foo' })
  })
})
```

- [ ] **Step 2: Run, confirm failure**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run line-col-parser 2>&1 | tail -8
# Expect: 'Cannot find module ./line-col-parser'
```

- [ ] **Step 3: Implement**

Path: `ui/src/components/preview/chips/line-col-parser.ts`

```ts
/**
 * Parse a chip-candidate path that may carry a `:line` or `:line:col` suffix.
 *
 * Used by the markdown plugin (after extension matching) and by FilePathChip
 * (to preserve the suffix for future jump-to-line wiring).
 */

export interface ParsedLineCol {
  /** Bare path, never empty if input was non-empty. */
  path: string
  /** 1-indexed line number when present. */
  line?: number
  /** 1-indexed column when present. */
  col?: number
}

const LINE_COL_RE = /^(.*?):(\d+)(?::(\d+))?$/

export function parseLineCol(input: string): ParsedLineCol {
  const m = LINE_COL_RE.exec(input)
  if (!m) return { path: input }
  const path = m[1]!
  const line = Number(m[2])
  const colRaw = m[3]
  if (!Number.isInteger(line) || line < 1) return { path: input }
  if (colRaw !== undefined) {
    const col = Number(colRaw)
    if (!Number.isInteger(col) || col < 1) return { path: input }
    return { path, line, col }
  }
  return { path, line }
}
```

- [ ] **Step 4: Run, confirm pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run line-col-parser 2>&1 | tail -5
# Expect: 6 tests, all passing
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/preview/chips/line-col-parser.ts ui/src/components/preview/chips/line-col-parser.test.ts
git commit -m "feat(preview): line-col-parser for chip path:line:col suffixes"
git branch --show-current   # must show claude/w4c-chips-and-shift-attach
```

---

## Task 4: file-type-colors

**Files:**
- Create: `ui/src/components/preview/chips/file-type-colors.ts`

Maps an extension to chip color tokens. Theme-token only (no raw hex). Falls back to neutral muted for unknown but previewable extensions.

- [ ] **Step 1: Create**

Path: `ui/src/components/preview/chips/file-type-colors.ts`

```ts
/**
 * file-type-colors — Per-extension chip color tokens.
 *
 * Single source of truth (per master spec §2.2). Theme-token classes only —
 * no raw hex — so chips adapt to every uClaw theme.
 *
 * `icon` is the inline-SVG color class; `bg` is the chip background;
 * `border` is the chip border. The chip uses bg-foreground/[0.04]
 * by default and only the icon color shifts per ext, so we keep this
 * deliberately small.
 */

export interface ChipColors {
  /** Tailwind class applied to the leading icon (color only). */
  icon: string
}

const TYPE_COLOR_MAP: Record<string, ChipColors> = {
  // typescript / javascript
  ts:   { icon: 'text-sky-600 dark:text-sky-300' },
  tsx:  { icon: 'text-sky-600 dark:text-sky-300' },
  js:   { icon: 'text-amber-600 dark:text-amber-300' },
  jsx:  { icon: 'text-amber-600 dark:text-amber-300' },
  mjs:  { icon: 'text-amber-600 dark:text-amber-300' },
  // systems
  rs:   { icon: 'text-orange-700 dark:text-orange-300' },
  go:   { icon: 'text-cyan-600 dark:text-cyan-300' },
  py:   { icon: 'text-emerald-600 dark:text-emerald-300' },
  // web
  html: { icon: 'text-orange-600 dark:text-orange-300' },
  css:  { icon: 'text-blue-600 dark:text-blue-300' },
  scss: { icon: 'text-pink-600 dark:text-pink-300' },
  // data / markup
  json: { icon: 'text-yellow-600 dark:text-yellow-300' },
  yaml: { icon: 'text-violet-600 dark:text-violet-300' },
  yml:  { icon: 'text-violet-600 dark:text-violet-300' },
  toml: { icon: 'text-violet-600 dark:text-violet-300' },
  md:   { icon: 'text-slate-600 dark:text-slate-300' },
  // images
  png:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  jpg:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  jpeg: { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  gif:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  svg:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  webp: { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  // documents
  pdf:  { icon: 'text-rose-600 dark:text-rose-300' },
  docx: { icon: 'text-blue-700 dark:text-blue-300' },
  xlsx: { icon: 'text-emerald-700 dark:text-emerald-300' },
  pptx: { icon: 'text-orange-700 dark:text-orange-300' },
}

const FALLBACK: ChipColors = { icon: 'text-foreground/55' }

export function getChipColors(ext: string): ChipColors {
  return TYPE_COLOR_MAP[ext.toLowerCase()] ?? FALLBACK
}
```

- [ ] **Step 2: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
```

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/preview/chips/file-type-colors.ts
git commit -m "feat(preview): chip color map keyed on file extension"
git branch --show-current
```

---

## Task 5: FilePathChip component

**Files:**
- Create: `ui/src/components/preview/chips/FilePathChip.tsx`

The visual chip. Pure of resolver logic — it accepts `state: 'ok' | 'pending' | 'missing'` and resolution metadata as props. The resolver hook (Task 9) and the plugin (Task 6) wire those props.

- [ ] **Step 1: Create**

Path: `ui/src/components/preview/chips/FilePathChip.tsx`

```tsx
/**
 * FilePathChip — Inline file reference in agent messages.
 *
 * Visual states:
 *   - ok       : full opacity, hover background
 *   - pending  : 70% opacity, no spinner (too noisy with many chips)
 *   - missing  : 45% opacity + strikethrough label, tooltip "文件未找到"
 *
 * Click semantics (uniform with FileTreeNode after Task 11):
 *   - Click       → openPreviewAction
 *   - Shift-click → addPendingAttachmentAction
 *   - Cmd/Ctrl    → reserved for W5; no-op today
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { File } from 'lucide-react'
import { cn } from '@/lib/utils'
import { openPreviewAction } from '@/atoms/preview-panel-atoms'
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'
import { getChipColors } from './file-type-colors'
import { getExtension } from '@/components/preview/utils/ext-classifier'

export type ChipState = 'ok' | 'pending' | 'missing'

export interface FilePathChipProps {
  /** Input string the plugin matched (with or without :line:col stripped). */
  rawPath: string
  /** Display label — usually basename or the markdown link text. */
  label: string
  /** Resolution state (from useFileChipResolver). */
  state: ChipState
  /** Mount id (from resolver). Empty string when state !== 'ok'. */
  mountId: string
  /** Path inside the mount (from resolver). Empty string when state !== 'ok'. */
  relPath: string
  /** Absolute path when resolved; empty string otherwise. */
  absolutePath: string
  /** Active session id (used when re-opening the preview). */
  sessionId?: string | null
  /** Optional line/col from parser, preserved for future jump-to-line. */
  line?: number
  col?: number
}

export function FilePathChip(props: FilePathChipProps): React.ReactElement {
  const openPreview = useSetAtom(openPreviewAction)
  const addAttachment = useSetAtom(addPendingAttachmentAction)

  const ext = getExtension(props.label) || getExtension(props.rawPath)
  const colors = getChipColors(ext)
  const isMissing = props.state === 'missing'

  const handleClick = React.useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      // Cmd/Ctrl reserved for W5 — no-op today.
      if (e.metaKey || e.ctrlKey) {
        e.preventDefault()
        return
      }
      e.preventDefault()
      if (e.shiftKey) {
        if (isMissing) return  // Task 7 toast covers the user-facing miss
        void addAttachment({
          mountId: props.mountId,
          relPath: props.relPath,
          name: props.label,
          sessionId: props.sessionId ?? null,
          absolutePath: props.absolutePath,
        })
        return
      }
      // Plain click: open in preview panel.
      if (props.state !== 'ok') {
        // Still open so user sees the "not found" surface (master spec §6.8).
        openPreview({
          mountId: props.mountId || 'workspace:default',
          relPath: props.relPath || props.rawPath,
          name: props.label,
          sessionId: props.sessionId ?? null,
          absolutePath: props.absolutePath,
        })
        return
      }
      openPreview({
        mountId: props.mountId,
        relPath: props.relPath,
        name: props.label,
        sessionId: props.sessionId ?? null,
        absolutePath: props.absolutePath,
      })
    },
    [openPreview, addAttachment, isMissing, props],
  )

  const stateOpacity =
    props.state === 'ok'
      ? 'opacity-100'
      : props.state === 'pending'
        ? 'opacity-70'
        : 'opacity-45'

  return (
    <button
      type="button"
      onClick={handleClick}
      title={isMissing ? `文件未找到：${props.rawPath}` : props.rawPath}
      data-chip-state={props.state}
      className={cn(
        'inline-flex items-center gap-1 align-baseline',
        'h-[20px] px-1.5 mx-0.5 rounded-md',
        'text-[11.5px] font-mono tabular-nums leading-none',
        'bg-foreground/[0.04] hover:bg-foreground/[0.08]',
        'border border-border/60',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        stateOpacity,
      )}
    >
      <File size={11} className={cn('shrink-0', colors.icon)} aria-hidden />
      <span className={cn(isMissing && 'line-through')}>{props.label}</span>
      {props.line !== undefined && (
        <span className="text-foreground/45">
          :{props.line}
          {props.col !== undefined ? `:${props.col}` : ''}
        </span>
      )}
    </button>
  )
}
```

- [ ] **Step 2: tsc — expect failure (atoms not created yet)**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | grep -E "(FilePathChip|preview-chip-atoms)" | head -5
# Expect: error about Cannot find module '@/atoms/preview-chip-atoms'
```

This is expected — Task 7 creates that module. We commit the chip now so the diff stays small per commit; Task 7 closes the loop.

- [ ] **Step 3: Commit (with known temporary type error)**

The plan deliberately introduces a 2-commit gap (chip + atoms file) so each commit stays focused. To avoid CI breaking mid-PR, we re-order if necessary at execution time. **If the executing agent prefers, swap Task 5 ↔ Task 7 so atoms ship first.** Both orderings produce identical final state.

The recommended execution order for the rest of this plan is: **do Task 7 first, then Task 5**. Tasks remain numbered as-is for plan readability.

```bash
# After also completing Task 7 (atoms), tsc should be clean.
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/preview/chips/FilePathChip.tsx
git commit -m "feat(preview): FilePathChip visual component (3 states: ok/pending/missing)"
git branch --show-current
```

---

## Task 6: markdownFileChipPlugin

**Files:**
- Create: `ui/src/components/preview/chips/markdownFileChipPlugin.ts`
- Test: `ui/src/components/preview/chips/markdownFileChipPlugin.test.ts`

A `unified` remark plugin. Walks the mdast and rewrites file-reference nodes into custom HAST emission via `data.hName` + `data.hProperties`. `react-markdown` (used by `MessageResponse`) then renders the unknown tag name through `components` mapping (Task 10).

**Detection patterns:**
1. **Markdown link** (mdast `type: 'link'`) — URL ends in a previewable extension, **not** a protocol URL (`http(s)://`, `file://`, `mailto:`).
2. **Inline code** (mdast `type: 'inlineCode'`) — value matches `^[\w.-]+\.([a-z0-9]+)$` and ext is previewable.
3. **Slash-bearing path token in text** (mdast `type: 'text'`) — regex finds path-like substrings with at least one `/`, ending in a previewable extension, optional `:line:col` suffix.

**Exclusions:** the visitor never descends into mdast `type: 'code'` (fenced blocks) — handled automatically because we use `unist-util-visit` and skip recursion into `code` nodes via the visitor return.

- [ ] **Step 1: Write the failing tests**

Path: `ui/src/components/preview/chips/markdownFileChipPlugin.test.ts`

```ts
import { describe, it, expect } from 'vitest'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import { markdownFileChipPlugin } from './markdownFileChipPlugin'

function findChipNodes(tree: any): any[] {
  const out: any[] = []
  function walk(node: any) {
    if (node?.data?.hName === 'file-path-chip') out.push(node)
    if (Array.isArray(node?.children)) node.children.forEach(walk)
  }
  walk(tree)
  return out
}

function run(md: string) {
  const tree = unified().use(remarkParse).use(remarkGfm).use(markdownFileChipPlugin).parse(md)
  return unified().use(remarkParse).use(remarkGfm).use(markdownFileChipPlugin).runSync(tree)
}

describe('markdownFileChipPlugin', () => {
  it('converts a markdown link to a chip', () => {
    const tree = run('See [the entry](src/main.rs) for details.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({
      rawPath: 'src/main.rs',
      label: 'the entry',
    })
  })

  it('converts inline-code single filename to a chip', () => {
    const tree = run('Check `style.css` for the rules.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({ rawPath: 'style.css', label: 'style.css' })
  })

  it('does NOT convert inline code that is not a filename', () => {
    const tree = run('Call `arr.map((x) => x + 1)` here.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })

  it('converts slash-bearing path tokens in text', () => {
    const tree = run('Open src/main.rs to see the entry.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties.rawPath).toBe('src/main.rs')
  })

  it('strips :line:col from path tokens', () => {
    const tree = run('Bug at ui/src/atoms.ts:42:15 in the reducer.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({
      rawPath: 'ui/src/atoms.ts',
      line: 42,
      col: 15,
    })
  })

  it('does NOT match http/https URLs', () => {
    const tree = run('See https://example.com/foo.ts for context.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })

  it('does NOT descend into fenced code blocks', () => {
    const tree = run('Outer src/a.ts here.\n\n```\ninner src/b.ts inside\n```\n')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties.rawPath).toBe('src/a.ts')
  })

  it('rejects extensions not in ALL_PREVIEWABLE_EXTS', () => {
    const tree = run('Run foo.exe then bar.map there.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run, confirm failure**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run markdownFileChipPlugin 2>&1 | tail -10
# Expect: 'Cannot find module ./markdownFileChipPlugin'
```

- [ ] **Step 3: Implement**

Path: `ui/src/components/preview/chips/markdownFileChipPlugin.ts`

```ts
/**
 * markdownFileChipPlugin — Remark plugin that rewrites file references
 * into custom HAST nodes the renderer maps to <FilePathChip>.
 *
 * Three detection patterns (in AST visitor order):
 *   1. mdast `link` whose URL ends in a previewable extension and is not
 *      a protocol URL.
 *   2. mdast `inlineCode` whose value is a single filename
 *      (`/^[\w.-]+\.([a-z0-9]+)$/` with previewable ext).
 *   3. mdast `text` containing slash-bearing path tokens with optional
 *      `:line:col` suffix.
 *
 * Fenced code (mdast `code`) is naturally skipped because the visitor only
 * matches `link` / `inlineCode` / `text` types and `unist-util-visit`
 * descends into all children. Text inside `code` does not appear as a
 * separate `text` node — it lives in the code node's `value` string, so
 * it's never visited.
 *
 * Custom node emission uses the unified ecosystem's `data.hName` /
 * `data.hProperties` convention: mdast-util-to-hast converts those into
 * a HAST element of the given tag name with the given properties.
 * react-markdown then renders the unknown tag name via its `components`
 * map (Task 10).
 */

import type { Plugin } from 'unified'
import type { Root, Link, InlineCode, Text, RootContent, PhrasingContent } from 'mdast'
import { visit, SKIP } from 'unist-util-visit'
import { isPreviewableExt, getExtension } from '@/components/preview/utils/ext-classifier'
import { parseLineCol } from './line-col-parser'

const PROTOCOL_RE = /^[a-z][a-z0-9+.-]*:\/\//i

// Single inline-code filename — no slashes, ending in a recognized ext.
const INLINE_CODE_FILENAME_RE = /^[\w.@-]+\.([a-z0-9]+)$/i

// Slash-bearing path token with optional :line:col suffix.
// Capture group 1 is the full match (path + suffix).
// We require at least one '/' so single-word filenames in prose don't trigger.
const PATH_TOKEN_RE =
  /(?:^|[\s\(])((?:[\w.@-]+\/)+[\w.-]+\.[a-z0-9]+(?::\d+(?::\d+)?)?)(?=[\s\)\.,;:!?]|$)/g

interface ChipHProperties {
  rawPath: string
  label: string
  line?: number
  col?: number
}

function makeChipNode(rawPath: string, label: string): RootContent {
  const parsed = parseLineCol(rawPath)
  const hProperties: ChipHProperties = {
    rawPath: parsed.path,
    label,
  }
  if (parsed.line !== undefined) hProperties.line = parsed.line
  if (parsed.col !== undefined) hProperties.col = parsed.col
  // We emit it as a `text` mdast node with hName/hProperties — the runner
  // (mdast-util-to-hast) honors those over the node's own type.
  return {
    type: 'text',
    value: '',
    data: { hName: 'file-path-chip', hProperties },
  } as RootContent
}

function urlIsChip(url: string): boolean {
  if (PROTOCOL_RE.test(url)) return false
  // Strip line/col suffix before extension check.
  const parsed = parseLineCol(url)
  const ext = getExtension(parsed.path)
  return ext.length > 0 && isPreviewableExt(ext)
}

function inlineCodeIsChip(value: string): boolean {
  const m = INLINE_CODE_FILENAME_RE.exec(value)
  if (!m) return false
  return isPreviewableExt(m[1]!)
}

export const markdownFileChipPlugin: Plugin<[], Root> = function plugin() {
  return (tree: Root) => {
    // Pattern 1 + 2: link + inlineCode nodes — substitute in place via parent.
    visit(tree, (node, index, parent) => {
      if (!parent || typeof index !== 'number') return
      if (node.type === 'link') {
        const link = node as Link
        if (!urlIsChip(link.url)) return
        const labelNode = link.children?.[0]
        const label =
          labelNode && 'value' in labelNode && typeof labelNode.value === 'string'
            ? labelNode.value
            : link.url
        parent.children[index] = makeChipNode(link.url, label) as PhrasingContent
        return SKIP
      }
      if (node.type === 'inlineCode') {
        const inline = node as InlineCode
        if (!inlineCodeIsChip(inline.value)) return
        parent.children[index] = makeChipNode(inline.value, inline.value) as PhrasingContent
        return SKIP
      }
    })

    // Pattern 3: split text nodes around path-like tokens.
    visit(tree, 'text', (node: Text, index, parent) => {
      if (!parent || typeof index !== 'number') return
      const value = node.value
      const matches: { start: number; end: number; raw: string }[] = []
      PATH_TOKEN_RE.lastIndex = 0
      let m: RegExpExecArray | null
      while ((m = PATH_TOKEN_RE.exec(value)) !== null) {
        const raw = m[1]!
        const parsed = parseLineCol(raw)
        const ext = getExtension(parsed.path)
        if (!ext || !isPreviewableExt(ext)) continue
        const start = m.index + (m[0].length - raw.length)
        matches.push({ start, end: start + raw.length, raw })
      }
      if (matches.length === 0) return

      // Replace this text node with [text, chip, text, chip, text...].
      const replacement: PhrasingContent[] = []
      let cursor = 0
      for (const m of matches) {
        if (m.start > cursor) {
          replacement.push({ type: 'text', value: value.slice(cursor, m.start) } as PhrasingContent)
        }
        // Label is the basename of the path (post-:line:col strip).
        const parsed = parseLineCol(m.raw)
        const label = parsed.path.split('/').pop() ?? parsed.path
        replacement.push(makeChipNode(m.raw, label) as PhrasingContent)
        cursor = m.end
      }
      if (cursor < value.length) {
        replacement.push({ type: 'text', value: value.slice(cursor) } as PhrasingContent)
      }
      parent.children.splice(index, 1, ...replacement)
      return [SKIP, index + replacement.length]
    })
  }
}
```

- [ ] **Step 4: Run, confirm pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run markdownFileChipPlugin 2>&1 | tail -10
# Expect: 8 tests, all passing
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean (other than potentially the FilePathChip→atoms issue if Task 7 hasn't run yet)
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/preview/chips/markdownFileChipPlugin.ts ui/src/components/preview/chips/markdownFileChipPlugin.test.ts
git commit -m "feat(preview): remark plugin — file path chips (links + inline code + slash tokens)"
git branch --show-current
```

---

## Task 7: preview-chip-atoms — cache + addPendingAttachmentAction

**Files:**
- Create: `ui/src/atoms/preview-chip-atoms.ts`

Owns:
1. `chipResolutionCacheAtom` — `Map<rawPath, ChipResolutionEntry>` (LRU-capped at 500).
2. `addPendingAttachmentAction` — write atom that fetches bytes via `preview_read_bytes` and pushes a `PendingAttachment`. Dedupes by absolute path. Toasts via `sonner`.

- [ ] **Step 1: Create**

Path: `ui/src/atoms/preview-chip-atoms.ts`

```ts
/**
 * preview-chip-atoms — Shared state for file-path chips (W4c).
 *
 * `chipResolutionCacheAtom` is read by every <FilePathChip> through
 * `useFileChipResolver`. It's a bounded LRU (cap 500) to keep memory
 * stable across long sessions.
 *
 * `addPendingAttachmentAction` is dispatched by Shift-click on a chip OR
 * on a FileTreeNode row. It eagerly fetches bytes (so the resulting
 * PendingAttachment carries `localPath` for downstream send paths) and
 * surfaces a sonner toast.
 */

import { atom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import type { PendingAttachment } from './chat-atoms'
import { pendingAttachmentsAtom } from './chat-atoms'

export type ChipResolutionState = 'pending' | 'ok' | 'missing'

export interface ChipResolutionEntry {
  state: ChipResolutionState
  mountId: string
  relPath: string
  absolutePath: string
}

const CACHE_MAX = 500

/**
 * The cache is a Map (insertion order doubles as LRU order). When the
 * cache exceeds CACHE_MAX, callers (writers) re-create the map with the
 * oldest entries dropped. Reads are O(1).
 */
export const chipResolutionCacheAtom = atom<Map<string, ChipResolutionEntry>>(new Map())

/**
 * Set or update a cache entry, evicting the oldest entry when over cap.
 * Write-only atom; reads return null.
 */
export const setChipResolutionAction = atom(
  null,
  (get, set, payload: { rawPath: string; entry: ChipResolutionEntry }) => {
    const current = get(chipResolutionCacheAtom)
    const next = new Map(current)
    // Re-insert to move to most-recent position.
    next.delete(payload.rawPath)
    next.set(payload.rawPath, payload.entry)
    while (next.size > CACHE_MAX) {
      const oldest = next.keys().next().value as string | undefined
      if (oldest === undefined) break
      next.delete(oldest)
    }
    set(chipResolutionCacheAtom, next)
  },
)

/** Bust specific cache entries (called by Task 9 on Created/Removed events). */
export const invalidateChipResolutionsAction = atom(
  null,
  (get, set, paths: string[]) => {
    if (paths.length === 0) return
    const current = get(chipResolutionCacheAtom)
    const next = new Map(current)
    let mutated = false
    for (const p of paths) {
      if (next.delete(p)) mutated = true
    }
    if (mutated) set(chipResolutionCacheAtom, next)
  },
)

interface PreviewBytesPayload {
  resolvedPath: string
  bytes: number[]
  size: number
  truncated: boolean
  mtimeMs: number
}

interface PreviewBytesIpcPayload {
  resolved_path: string
  bytes: number[]
  size: number
  truncated: boolean
  mtime_ms: number
}

interface AddAttachmentPayload {
  mountId: string
  relPath: string
  name: string
  sessionId: string | null
  absolutePath: string
}

function inferMediaType(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) {
    return `image/${ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext}`
  }
  if (ext === 'pdf') return 'application/pdf'
  return 'text/plain'
}

/**
 * Shift-click handler shared by chip + rail.
 * - dedupes by absolute path (or name, when absolute path missing)
 * - eagerly fetches bytes via preview_read_bytes (50 MB cap enforced server-side)
 * - replaces a single shared toast id to avoid spam on rapid clicks
 */
export const addPendingAttachmentAction = atom(
  null,
  async (get, set, payload: AddAttachmentPayload) => {
    const dedupeKey = payload.absolutePath || `${payload.mountId}::${payload.relPath}`
    const current = get(pendingAttachmentsAtom)
    if (current.some((a) => (a.localPath || a.filename) === dedupeKey || a.localPath === payload.absolutePath)) {
      toast.info('文件已在附件中', { id: 'attachment-added', description: payload.name })
      return
    }
    try {
      const result = await invoke<PreviewBytesIpcPayload>('preview_read_bytes', {
        mountId: payload.mountId,
        relPath: payload.relPath,
        sessionId: payload.sessionId ?? null,
      })
      const next: PendingAttachment = {
        filename: payload.name,
        localPath: result.resolved_path,
        mediaType: inferMediaType(payload.name),
        size: result.size,
      }
      set(pendingAttachmentsAtom, [...current, next])
      toast.success(`已添加 ${payload.name} 到聊天`, { id: 'attachment-added' })
    } catch (err) {
      toast.error('无法添加附件', {
        id: 'attachment-added',
        description: err instanceof Error ? err.message : String(err),
      })
    }
  },
)
```

- [ ] **Step 2: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean (this file resolves the FilePathChip import)
```

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/preview-chip-atoms.ts
git commit -m "feat(preview): chip resolution cache + addPendingAttachmentAction atoms"
git branch --show-current
```

---

## Task 8: preview_resolve_chips Tauri command

**Files:**
- Modify: `src-tauri/src/preview/types.rs`
- Modify: `src-tauri/src/preview/resolver.rs`
- Modify: `src-tauri/src/preview/commands.rs`
- Modify: `src-tauri/src/preview/tests.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

The command takes a list of raw chip strings (possibly with `:line:col` suffixes) plus an optional `session_id` and returns one resolution per input. For each input:
1. Strip the `:line:col` suffix (Rust-side parallel of `parseLineCol`).
2. If absolute → `fs::metadata` directly.
3. If relative → for each W3 mount (workspace first, then attached), try `mount.path.join(rel)` and `fs::metadata`; first hit wins.

- [ ] **Step 1: Add the response type**

In `src-tauri/src/preview/types.rs`, append:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipResolution {
    /// The raw input string, unchanged (so the frontend can key its cache).
    pub input: String,
    /// `true` if the resolved path exists and is a regular file.
    pub exists: bool,
    /// Mount id when the path resolved through a mount. `None` for absolute paths.
    pub mount_id: Option<String>,
    /// Path inside the mount (forward-slash). `None` for absolute paths or misses.
    pub rel_path: Option<String>,
    /// Canonicalised absolute path when resolved; `None` otherwise.
    pub absolute_path: Option<String>,
}
```

(If `types.rs` doesn't already import `serde::Serialize`, ensure it does.)

- [ ] **Step 2: Add the resolver helper**

In `src-tauri/src/preview/resolver.rs`, append:

```rust
use super::types::ChipResolution;

/// Strip an optional `:line:col` suffix from a chip-candidate input.
/// Returns `(bare_path, line, col)`. Mirrors `parseLineCol` in TS.
fn strip_line_col(input: &str) -> (&str, Option<u32>, Option<u32>) {
    // Find the last ':' — there can be at most two trailing numeric segments.
    let bytes = input.as_bytes();
    let mut iter = input.char_indices().rev();
    let mut col: Option<u32> = None;
    let mut line: Option<u32> = None;
    // Try to peel `:N` once or twice.
    while let Some((i, ch)) = iter.next() {
        if ch != ':' {
            continue;
        }
        let tail = &input[i + 1..];
        if let Ok(n) = tail.parse::<u32>() {
            if n == 0 {
                return (input, None, None);
            }
            if col.is_none() {
                col = Some(n);
                // Continue to maybe peel a second suffix.
                continue;
            } else {
                line = col;
                col = Some(n);
                return (&input[..i], line.or(col), if line.is_some() { col } else { None });
            }
        }
        // Non-numeric suffix found before we got a line — leave input as-is.
        break;
    }
    // If we peeled exactly one numeric suffix, it's the line.
    if let Some(n) = col {
        // Strip from the rightmost `:` we matched.
        if let Some(colon_idx) = input.rfind(':') {
            return (&input[..colon_idx], Some(n), None);
        }
    }
    (input, None, None)
}

/// Resolve a single chip candidate against mounts + absolute-path fallback.
///
/// Returns a fully-populated `ChipResolution`. Never errors — missing or
/// invalid inputs yield `exists: false`.
pub async fn resolve_chip_candidate(
    state: &AppState,
    raw: &str,
    session_id: Option<String>,
) -> ChipResolution {
    let (bare, _line, _col) = strip_line_col(raw);

    // Absolute paths take the express lane.
    if bare.starts_with('/') {
        let p = Path::new(bare);
        let exists = fs::metadata(p).map(|m| m.is_file()).unwrap_or(false);
        return ChipResolution {
            input: raw.to_string(),
            exists,
            mount_id: None,
            rel_path: None,
            absolute_path: if exists {
                p.canonicalize().ok().map(|c| c.to_string_lossy().into_owned())
            } else {
                None
            },
        };
    }

    // Relative path — must not contain '..' segments.
    if bare.split('/').any(|seg| seg == "..") {
        return ChipResolution {
            input: raw.to_string(),
            exists: false,
            mount_id: None,
            rel_path: None,
            absolute_path: None,
        };
    }

    let mounts = match state.files_rail_list_mounts(session_id).await {
        Ok(m) => m,
        Err(_) => Vec::new(),
    };
    for mount in mounts {
        let candidate = mount.path.join(bare);
        if let Ok(meta) = fs::metadata(&candidate) {
            if meta.is_file() {
                let abs = candidate
                    .canonicalize()
                    .ok()
                    .map(|c| c.to_string_lossy().into_owned());
                return ChipResolution {
                    input: raw.to_string(),
                    exists: true,
                    mount_id: Some(mount.id.clone()),
                    rel_path: Some(bare.to_string()),
                    absolute_path: abs,
                };
            }
        }
    }
    ChipResolution {
        input: raw.to_string(),
        exists: false,
        mount_id: None,
        rel_path: None,
        absolute_path: None,
    }
}
```

- [ ] **Step 3: Add the command**

In `src-tauri/src/preview/commands.rs`, append:

```rust
use super::resolver::resolve_chip_candidate;
use super::types::ChipResolution;

#[tauri::command]
pub async fn preview_resolve_chips(
    state: State<'_, AppState>,
    paths: Vec<String>,
    session_id: Option<String>,
) -> Result<Vec<ChipResolution>, Error> {
    // Cap input length to prevent abuse — a normal chat message has ≪ 100 chips.
    const MAX_PATHS: usize = 256;
    let take = paths.len().min(MAX_PATHS);
    let mut out = Vec::with_capacity(take);
    for raw in paths.into_iter().take(MAX_PATHS) {
        out.push(resolve_chip_candidate(&state, &raw, session_id.clone()).await);
    }
    Ok(out)
}
```

- [ ] **Step 4: Register the command**

In `src-tauri/src/tauri_commands.rs`, append next to the existing `preview_read_bytes` re-export:

```rust
pub use crate::preview::commands::preview_resolve_chips;
```

In `src-tauri/src/main.rs`, locate the `tauri::generate_handler!` (or `invoke_handler!`) macro and add `preview_resolve_chips` to the list.

- [ ] **Step 5: Add tests**

In `src-tauri/src/preview/tests.rs`, append:

```rust
#[cfg(test)]
mod chip_tests {
    use super::super::resolver::strip_line_col;

    #[test]
    fn strips_line_only() {
        let (p, line, col) = strip_line_col("src/main.rs:42");
        assert_eq!(p, "src/main.rs");
        assert_eq!(line, Some(42));
        assert_eq!(col, None);
    }

    #[test]
    fn strips_line_and_col() {
        let (p, line, col) = strip_line_col("src/main.rs:42:15");
        assert_eq!(p, "src/main.rs");
        assert_eq!(line, Some(42));
        assert_eq!(col, Some(15));
    }

    #[test]
    fn leaves_input_when_no_suffix() {
        let (p, line, col) = strip_line_col("src/main.rs");
        assert_eq!(p, "src/main.rs");
        assert_eq!(line, None);
        assert_eq!(col, None);
    }

    #[test]
    fn leaves_input_when_suffix_non_numeric() {
        let (p, line, col) = strip_line_col("src/main.rs:foo");
        assert_eq!(p, "src/main.rs:foo");
        assert_eq!(line, None);
        assert_eq!(col, None);
    }
}
```

If `strip_line_col` is not `pub`, mark it `pub(super)` so the test module can call it.

- [ ] **Step 6: Build + run Rust tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib chip_tests 2>&1 | tail -8
# Expect: 4 passed
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 395 passed (391 baseline + 4 new)
```

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/preview/types.rs src-tauri/src/preview/resolver.rs src-tauri/src/preview/commands.rs src-tauri/src/preview/tests.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(preview): preview_resolve_chips Tauri command (mount-aware existence check)"
git branch --show-current
```

---

## Task 9: useFileChipResolver hook

**Files:**
- Create: `ui/src/components/preview/chips/useFileChipResolver.ts`

The hook that bridges chips → cache → backend. Every mounted `FilePathChip` calls `useFileChipResolver(rawPath, sessionId)` once. The hook:
1. Reads the cache entry for `rawPath`.
2. If absent: appends `rawPath` to a module-scope batch queue; on first append in this tick, schedules a debounced flush (50 ms).
3. Returns `{ state, mountId, relPath, absolutePath }` synchronously from the current cache value (or `'pending'` placeholders).
4. Subscribes to `files_rail_change` events; on `Created`/`Removed`, busts matching cache entries.

- [ ] **Step 1: Create**

Path: `ui/src/components/preview/chips/useFileChipResolver.ts`

```ts
/**
 * useFileChipResolver — Batched async existence check for FilePathChip.
 *
 * Module-scope batching: every chip that mounts and hits an empty cache
 * entry queues its rawPath. The first queued path schedules a 50 ms
 * setTimeout to flush. The flush issues a single `preview_resolve_chips`
 * invoke and seeds the cache.
 *
 * The hook returns synchronously from the cache (or 'pending' placeholders),
 * relying on react-redraw when the cache atom updates.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  chipResolutionCacheAtom,
  setChipResolutionAction,
  invalidateChipResolutionsAction,
  type ChipResolutionEntry,
} from '@/atoms/preview-chip-atoms'

interface ChipResolutionIpcPayload {
  input: string
  exists: boolean
  mountId: string | null
  relPath: string | null
  absolutePath: string | null
}

interface BatchEntry {
  rawPath: string
  sessionId: string | null
  resolve: (entry: ChipResolutionEntry) => void
}

const BATCH_WINDOW_MS = 50
const queue: BatchEntry[] = []
let scheduled = false

function flush(setEntry: (p: { rawPath: string; entry: ChipResolutionEntry }) => void): void {
  if (queue.length === 0) {
    scheduled = false
    return
  }
  const batch = queue.splice(0, queue.length)
  scheduled = false
  // Group by sessionId — typically a single value, but be defensive.
  const groups = new Map<string | null, BatchEntry[]>()
  for (const item of batch) {
    const arr = groups.get(item.sessionId) ?? []
    arr.push(item)
    groups.set(item.sessionId, arr)
  }
  for (const [sessionId, items] of groups) {
    const paths = items.map((it) => it.rawPath)
    void invoke<ChipResolutionIpcPayload[]>('preview_resolve_chips', {
      paths,
      sessionId,
    })
      .then((results) => {
        const byInput = new Map(results.map((r) => [r.input, r]))
        for (const item of items) {
          const r = byInput.get(item.rawPath)
          const entry: ChipResolutionEntry = r
            ? {
                state: r.exists ? 'ok' : 'missing',
                mountId: r.mountId ?? '',
                relPath: r.relPath ?? '',
                absolutePath: r.absolutePath ?? '',
              }
            : { state: 'missing', mountId: '', relPath: '', absolutePath: '' }
          setEntry({ rawPath: item.rawPath, entry })
          item.resolve(entry)
        }
      })
      .catch(() => {
        for (const item of items) {
          const entry: ChipResolutionEntry = {
            state: 'missing',
            mountId: '',
            relPath: '',
            absolutePath: '',
          }
          setEntry({ rawPath: item.rawPath, entry })
          item.resolve(entry)
        }
      })
  }
}

function enqueue(
  rawPath: string,
  sessionId: string | null,
  setEntry: (p: { rawPath: string; entry: ChipResolutionEntry }) => void,
): void {
  queue.push({ rawPath, sessionId, resolve: () => {} })
  if (!scheduled) {
    scheduled = true
    setTimeout(() => flush(setEntry), BATCH_WINDOW_MS)
  }
}

const PENDING_ENTRY: ChipResolutionEntry = {
  state: 'pending',
  mountId: '',
  relPath: '',
  absolutePath: '',
}

/**
 * Subscribe to a single chip's resolution. Returns synchronously.
 * `'pending'` for un-cached paths; the hook triggers a backend resolve
 * and the cache update will re-render this chip.
 */
export function useFileChipResolver(
  rawPath: string,
  sessionId: string | null,
): ChipResolutionEntry {
  const cache = useAtomValue(chipResolutionCacheAtom)
  const setEntry = useSetAtom(setChipResolutionAction)
  const existing = cache.get(rawPath)

  React.useEffect(() => {
    if (!rawPath) return
    if (cache.has(rawPath)) return
    // Mark as pending immediately so other chips with the same path don't re-queue.
    setEntry({ rawPath, entry: PENDING_ENTRY })
    enqueue(rawPath, sessionId, setEntry)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawPath, sessionId])

  return existing ?? PENDING_ENTRY
}

/**
 * Top-level mount in MessageResponse: subscribes to files_rail_change events
 * once per session and invalidates Created/Removed cache entries.
 * The hook only needs to run once per host page; MessageResponse calls it.
 */
export function useChipCacheInvalidator(): void {
  const invalidate = useSetAtom(invalidateChipResolutionsAction)
  React.useEffect(() => {
    let cancelled = false
    let unlisten: undefined | (() => void)
    void listen<{ kind: string; path: string }>('files_rail_change', (event) => {
      if (cancelled) return
      const kind = event.payload?.kind
      const path = event.payload?.path
      if (!path) return
      if (kind === 'Created' || kind === 'Removed') {
        invalidate([path])
      }
    }).then((fn) => {
      if (cancelled) {
        fn()
      } else {
        unlisten = fn
      }
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [invalidate])
}
```

- [ ] **Step 2: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
```

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/preview/chips/useFileChipResolver.ts
git commit -m "feat(preview): useFileChipResolver — batched existence checks + LRU cache"
git branch --show-current
```

> **Note on test coverage:** the hook has time-dependent batching that is awkward to unit-test under jsdom + vitest fake timers without flake. The cache-LRU behavior is exercised through `setChipResolutionAction` (already covered indirectly by Task 6's plugin tests if expanded; we don't add a unit test here to keep scope tight). Manual verification in the final task ensures the batched fetch produces visible chip state transitions.

---

## Task 10: Wire chips into MessageResponse + ContentBlock

**Files:**
- Modify: `ui/src/components/ai-elements/message.tsx`
- Modify: `ui/src/components/agent/ContentBlock.tsx`

Two integration points: the standard assistant-response markdown render, and the agent "thinking" panel render.

**Strategy:** A small wrapper component `<FileChipFromHast>` consumes the HAST properties react-markdown forwards (camelCase) and calls `useFileChipResolver` to get resolution state, then renders `<FilePathChip>`. Mount `useChipCacheInvalidator` once at the top of `MessageResponse` so the file-change subscription is registered.

The hook + resolver need a `sessionId`. `MessageResponse` is generic (no session prop). We thread it via a new optional `sessionId` prop that defaults to `null`. ContentBlock can pass the agent session id; vanilla chat passes `null` (in which case the resolver uses workspace mounts only).

- [ ] **Step 1: Create the small adapter component inline**

In `ui/src/components/ai-elements/message.tsx`, add **above** `MARKDOWN_COMPONENTS`:

```tsx
import { useFileChipResolver, useChipCacheInvalidator } from '@/components/preview/chips/useFileChipResolver'
import { FilePathChip } from '@/components/preview/chips/FilePathChip'
import { markdownFileChipPlugin } from '@/components/preview/chips/markdownFileChipPlugin'

interface FileChipHastProps {
  rawPath: string
  label: string
  line?: number
  col?: number
}

function FileChipFromHast(props: FileChipHastProps & { sessionId: string | null }): React.ReactElement {
  const resolution = useFileChipResolver(props.rawPath, props.sessionId)
  return (
    <FilePathChip
      rawPath={props.rawPath}
      label={props.label}
      state={resolution.state}
      mountId={resolution.mountId}
      relPath={resolution.relPath}
      absolutePath={resolution.absolutePath}
      sessionId={props.sessionId}
      line={props.line}
      col={props.col}
    />
  )
}
```

- [ ] **Step 2: Add the plugin + custom tag to the components map**

In the same file, modify `REMARK_PLUGINS` + `REMARK_PLUGINS_WITH_BREAKS`:

```ts
const REMARK_PLUGINS: PluggableList = [remarkGfm, markdownFileChipPlugin]
const REMARK_PLUGINS_WITH_BREAKS: PluggableList = [remarkGfm, remarkPreserveBreaks, markdownFileChipPlugin]
```

`MARKDOWN_COMPONENTS` already exists. We DON'T add `'file-path-chip'` to it directly because react-markdown's `components` map types only allow known HTML tag names. We pass it via a wrapper using `react-markdown`'s `components` extension. The simplest approach: extend `MARKDOWN_COMPONENTS` with a type-erased entry inside `MessageResponse`:

In `MessageResponse`:

```tsx
export const MessageResponse = React.memo(
  function MessageResponse({
    children,
    className,
    preserveBreaks = false,
    sessionId = null,
  }: MessageResponseProps): React.ReactElement {
    useChipCacheInvalidator()
    const remarkPlugins = preserveBreaks ? REMARK_PLUGINS_WITH_BREAKS : REMARK_PLUGINS
    const content = typeof children === 'string' ? children : ''

    // We must inject the chip renderer per-call because it closes over sessionId.
    const components = React.useMemo(
      () => ({
        ...MARKDOWN_COMPONENTS,
        'file-path-chip': (chipProps: FileChipHastProps) => (
          <FileChipFromHast {...chipProps} sessionId={sessionId} />
        ),
      }),
      [sessionId],
    )

    return (
      <div
        className={cn(
          'chat-content prose prose-sm dark:prose-invert max-w-none',
          'prose-p:my-1.5 prose-p:leading-[1.65]',
          'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
          'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
          '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
          className,
        )}
      >
        {content ? (
          <Markdown
            remarkPlugins={remarkPlugins}
            rehypePlugins={REHYPE_PLUGINS}
            urlTransform={defaultUrlTransform}
            // react-markdown's `components` type rejects custom tag names;
            // cast for the custom 'file-path-chip' entry.
            components={components as ComponentProps<typeof Markdown>['components']}
          >
            {content}
          </Markdown>
        ) : (
          typeof children !== 'string' ? children : null
        )}
      </div>
    )
  },
  (prev, next) =>
    prev.children === next.children &&
    prev.preserveBreaks === next.preserveBreaks &&
    prev.className === next.className &&
    prev.sessionId === next.sessionId,
)
```

Also update `MessageResponseProps`:

```ts
interface MessageResponseProps {
  children: React.ReactNode
  className?: string
  preserveBreaks?: boolean
  sessionId?: string | null
}
```

(Remove the now-unused `basePath` / `basePaths` placeholder props from the existing interface — they were placeholders and the W4c chip is the real implementation. **Check call sites first** with `grep`. If any caller passes them, leave the props for one cycle and just unwire them inside; otherwise delete.)

Add the missing import at the top of the file:

```ts
import type { ComponentProps } from 'react'
```

- [ ] **Step 3: Mirror in ContentBlock for agent thinking**

In `ui/src/components/agent/ContentBlock.tsx`:

```ts
// Existing:
const THINKING_REMARK_PLUGINS = [remarkGfm]

// Change to:
import { markdownFileChipPlugin } from '@/components/preview/chips/markdownFileChipPlugin'
import { FilePathChip } from '@/components/preview/chips/FilePathChip'
import { useFileChipResolver } from '@/components/preview/chips/useFileChipResolver'

const THINKING_REMARK_PLUGINS = [remarkGfm, markdownFileChipPlugin]
```

In `THINKING_MD_COMPONENTS` add the wrapper. Since `ContentBlock` already has a session context (look up how it gets sessionId — pass through the call site), wire the chip with whatever sessionId is in scope. If `ContentBlock` lacks a sessionId, pass `null` (works against workspace mounts only):

```tsx
interface ThinkingChipProps {
  rawPath: string
  label: string
  line?: number
  col?: number
}

function ThinkingFileChip(props: ThinkingChipProps & { sessionId: string | null }): React.ReactElement {
  const resolution = useFileChipResolver(props.rawPath, props.sessionId)
  return (
    <FilePathChip
      rawPath={props.rawPath}
      label={props.label}
      state={resolution.state}
      mountId={resolution.mountId}
      relPath={resolution.relPath}
      absolutePath={resolution.absolutePath}
      sessionId={props.sessionId}
      line={props.line}
      col={props.col}
    />
  )
}
```

Then where the existing `<Markdown remarkPlugins={THINKING_REMARK_PLUGINS} components={THINKING_MD_COMPONENTS}>` call lives, replace the `components` prop with a merged object including the chip entry. If `ContentBlock` has access to a `sessionId` (look at the surrounding component props), use it; otherwise pass `null`.

- [ ] **Step 4: Verify tests + tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: tests pass at baseline + new (line-col + plugin + ext-classifier added)
```

If `message.fixtures.test.tsx` snapshots/regressions changed because the plugin now adds chip nodes, **read those tests** before changing — they document existing behavior. The plugin should only ADD chip nodes for paths with `/` + previewable extension; if any fixture coincidentally contains such a path, the test must be updated to expect the chip rather than the bare text/link.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/ai-elements/message.tsx ui/src/components/agent/ContentBlock.tsx
git commit -m "feat(preview): wire chip plugin into MessageResponse + ContentBlock"
git branch --show-current
```

---

## Task 11: Shift-click on FileTreeNode (rail prop signature change)

**Files:**
- Modify: `ui/src/components/files-rail/workspace/FileTreeNode.tsx`
- Modify: `ui/src/components/files-rail/workspace/MountSection.tsx`
- Modify: `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx`
- Modify: `ui/src/components/files-rail/index.tsx`
- Modify: `ui/src/components/agent/SidePanel.tsx`

The rail's `onFileClick` is currently `(node) => void`. We thread `React.MouseEvent` through so the SidePanel consumer can branch on `event.shiftKey`.

- [ ] **Step 1: Update FileTreeNode**

In `ui/src/components/files-rail/workspace/FileTreeNode.tsx`:

Change the prop type and call site:

```tsx
interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}
```

```tsx
  const handleClick = React.useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      if (isDir) void onToggle(node.relPath, true)
      else onFileClick(node, event)
    },
    [isDir, node, onToggle, onFileClick],
  )
```

- [ ] **Step 2: Update MountSection**

In `ui/src/components/files-rail/workspace/MountSection.tsx`:

```tsx
interface MountSectionProps {
  mount: MountRoot
  sessionId: string | null
  onFileClick: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}
```

And:

```tsx
  const handleFileClick = React.useCallback(
    (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) =>
      onFileClick(mount, node, event),
    [mount, onFileClick],
  )
```

- [ ] **Step 3: Update WorkspaceFilesPanel**

In `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx`:

```tsx
interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}
```

And:

```tsx
  const handleClick = React.useCallback(
    (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => {
      onFileClick?.(mount, node, event)
    },
    [onFileClick],
  )
```

- [ ] **Step 4: Update FilesRail index**

In `ui/src/components/files-rail/index.tsx`:

```tsx
interface FilesRailProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}
```

- [ ] **Step 5: Update SidePanel — branch on shiftKey**

In `ui/src/components/agent/SidePanel.tsx`, change the `onFileClick` body:

```tsx
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'

// inside the component:
const addAttachment = useSetAtom(addPendingAttachmentAction)

// and the FilesRail JSX block:
<FilesRail
  sessionId={sessionId}
  onFileClick={(mount, node, event) => {
    if (node.kind === 'directory') return
    if (event.shiftKey) {
      void addAttachment({
        mountId: mount.id,
        relPath: node.relPath,
        name: node.name,
        sessionId,
        absolutePath: `${mount.path}/${node.relPath}`,
      })
      return
    }
    if (event.metaKey || event.ctrlKey) return  // reserved for W5
    openPreview({
      mountId: mount.id,
      relPath: node.relPath,
      name: node.name,
      sessionId,
      absolutePath: `${mount.path}/${node.relPath}`,
    })
  }}
/>
```

Add `useSetAtom` to the imports if not already present.

- [ ] **Step 6: tsc + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: all tests pass
```

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/files-rail/workspace/FileTreeNode.tsx ui/src/components/files-rail/workspace/MountSection.tsx ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx ui/src/components/files-rail/index.tsx ui/src/components/agent/SidePanel.tsx
git commit -m "feat(rail): Shift-click in FileTreeNode dispatches addPendingAttachmentAction"
git branch --show-current
```

---

## Task 12: Final verification + open PR

- [ ] **Step 1: Run full local check matrix**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 395 passed (391 + 4 chip_tests)

cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: baseline + ~14 (3 isPreviewableExt + 6 line-col-parser + 8 plugin = 17, allow ±2 for snapshot updates)

cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | tail -10
# Expect: build succeeds; no new large chunks (chip code is tiny)
```

- [ ] **Step 2: Color audit on new files**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -rnE 'bg-\[#|text-\[#|border-\[#|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/preview/chips/ \
  ui/src/atoms/preview-chip-atoms.ts \
  2>/dev/null
echo "---color audit done---"
```

Expected: empty. (Tailwind named colors like `text-sky-600` are fine — only raw hex / zinc / gray are flagged.)

- [ ] **Step 3: Commit log check**

```bash
git log --oneline main..HEAD
```

Expected ~10 commits (one per task except Task 1 which lives on `main` via #109 squash + the W4c branch creation):
- `feat(preview): isPreviewableExt + ALL_PREVIEWABLE_EXTS for chip gating` (Task 2)
- `feat(preview): line-col-parser for chip path:line:col suffixes` (Task 3)
- `feat(preview): chip color map keyed on file extension` (Task 4)
- `feat(preview): chip resolution cache + addPendingAttachmentAction atoms` (Task 7)
- `feat(preview): FilePathChip visual component (3 states: ok/pending/missing)` (Task 5)
- `feat(preview): remark plugin — file path chips (links + inline code + slash tokens)` (Task 6)
- `feat(preview): preview_resolve_chips Tauri command (mount-aware existence check)` (Task 8)
- `feat(preview): useFileChipResolver — batched existence checks + LRU cache` (Task 9)
- `feat(preview): wire chip plugin into MessageResponse + ContentBlock` (Task 10)
- `feat(rail): Shift-click in FileTreeNode dispatches addPendingAttachmentAction` (Task 11)

- [ ] **Step 4: Manual checklist**

Manual verification — these can't be unit-tested:

```text
[ ] Open the app; open a chat with a recent agent message containing a markdown link to a real file in the workspace. Expect: chip rendered with full opacity, click opens preview.
[ ] Agent message containing inline code with a single filename (e.g. `style.css`). Expect: chip rendered; click opens preview if file exists.
[ ] Agent message containing slash-bearing path (e.g. "see src/main.rs"). Expect: chip rendered; click opens preview.
[ ] Agent message containing a path with :42:15 suffix. Expect: chip shows ":42:15" suffix; click opens preview.
[ ] Agent message referencing a non-existent file. Expect: chip rendered with 45% opacity + strikethrough; tooltip says "文件未找到"; click still opens preview surface in not-found state.
[ ] Click on a real file's chip → preview panel opens.
[ ] Shift-click on a real file's chip → toast "已添加 X 到聊天"; file appears in composer attachments.
[ ] Shift-click again on the same chip → toast "文件已在附件中"; no duplicate.
[ ] Cmd-click (mac) or Ctrl-click (linux/windows) on a chip → nothing happens (reserved for W5).
[ ] Open the rail (right side, workspace tab). Click a file → preview opens (unchanged behavior).
[ ] Shift-click a file in the rail → toast "已添加 X 到聊天"; file appears in composer attachments.
[ ] Theme spot check: switch to qingye, warm-paper, forest-night themes. Chips remain readable; colors don't break.
[ ] Streaming check: send a chat message that produces an agent reply containing 10+ file references mid-stream. Chips should appear as the text streams; no perf hitch.
```

- [ ] **Step 5: Working tree clean check**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
# Expect: empty
git branch --show-current   # claude/w4c-chips-and-shift-attach
```

- [ ] **Step 6: (CONDITIONAL — only after user explicitly approves push)** Push + open PR

Per CLAUDE.md: do **not** push or open a PR until the user explicitly asks. When approved:

```bash
git push origin claude/w4c-chips-and-shift-attach

gh pr create --title "W4c: File-path chips in agent messages + Shift-click attach" --body "$(cat <<'EOF'
## Summary

Final wave of the [Proma v0.9.27 preview port](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md). Adds clickable file-path chips inside agent messages and a uniform Shift-click "add to attachment" semantics across both chips and the rail's FileTreeNode.

Inline editing (CodeMirror 6 / TipTap) and DiffRenderer remain deferred — they can land as a follow-up W4d.

## What lands

| Surface | Behavior |
|---|---|
| Agent markdown link `[label](path.ext)` | Renders as a chip |
| Agent inline code with a single filename `` `style.css` `` | Renders as a chip |
| Agent text containing slash-bearing path `src/main.rs` (with optional `:42:15`) | Renders as a chip |
| Chip click | Opens preview panel |
| Chip Shift-click | Adds file to composer attachments + sonner toast |
| FileTreeNode click | Opens preview (unchanged) |
| FileTreeNode Shift-click | Adds file to composer attachments + sonner toast |

## Backend

One new Tauri command, no new module:
\`preview_resolve_chips(paths, session_id) -> Vec<ChipResolution>\` walks the existing W3 mount registry (workspace first, then attached_dirs, first hit wins). 256-input cap. Used by \`useFileChipResolver\` with 50 ms debounced batching.

## New deps

None. Uses existing \`unified\` / \`unist-util-visit\` (already pulled in by \`react-markdown\` + \`remark-gfm\`).

## Theme integration

Chip backgrounds use \`bg-foreground/[0.04]\` + \`border-border\` — pure theme tokens. Icon colors live in \`file-type-colors.ts\` using Tailwind named colors (sky/amber/rose/etc.), which adapt to dark mode via the \`dark:\` variant.

## Commits (bisectable)

10 commits, one per task. See \`git log main..HEAD\`.

## Test plan

- [x] \`cd ui && npx tsc --noEmit\` — clean
- [x] \`cd ui && npm test -- --run\` — baseline + ~17 new (ext + line-col + plugin)
- [x] \`cd src-tauri && cargo test --lib\` — 395 pass (391 baseline + 4 chip_tests)
- [x] \`cd ui && npm run build\` — succeeds, no new large chunks
- [x] Color audit on new files — clean
- [ ] Manual: chip rendering for each of 3 patterns
- [ ] Manual: chip click → preview opens
- [ ] Manual: chip Shift-click → attachment added + toast
- [ ] Manual: missing chip visual state (45% opacity + strikethrough)
- [ ] Manual: rail FileTreeNode Shift-click → attachment + toast
- [ ] Manual: 11-theme spot check
- [ ] Manual: streaming-message chip rendering (no perf hitch)

## What's out of scope (deferred)

- Inline editing for any format
- DiffRenderer
- Detached preview window (W5)
- Cmd/Ctrl-click semantics (placeholder no-op today)
- Bare-filename detection in prose (rejected as too noisy)
EOF
)"
```

---

## Self-Review

After drafting the plan, the controller (this agent) ran the spec-coverage / placeholder / type-consistency checklist inline. Recorded results:

**Spec coverage:**
- §6.8 file-path chips: 3 detection patterns → Task 6 (plugin) + Task 2 (ext gating) + Task 3 (line-col parser).
- §6.8 visual states (ok/pending/missing, broken-chip stays clickable): Task 5 (FilePathChip).
- §6.8 existence check + cache: Task 7 (atoms) + Task 9 (hook).
- §6.8 click → openPreviewAction: Task 5.
- W4c brainstorming: Shift-click on rail: Task 11. Shift-click on chip: Task 5 + Task 7.
- W4c brainstorming: workspace + attached mounts resolver: Task 8.
- W4c brainstorming: PR #109 rebase + merge: Task 1.
- W4c brainstorming: cache invalidation on Created/Removed: Task 9 (`useChipCacheInvalidator`) + Task 7 (`invalidateChipResolutionsAction`).
- W4c brainstorming: Cmd/Ctrl reserved for W5: Task 5 (explicit no-op branch in `handleClick`) + Task 11 (same branch in `SidePanel` consumer).

**Placeholder scan:** No "TBD", "TODO", or "implement later" remain. The "if any tests turn red, read them" guidance in Task 10 Step 4 is intentional human-judgement, not a placeholder.

**Type consistency:**
- `ChipResolutionEntry` (atoms) ↔ `ChipResolution` (Rust) — TS shape mirrors the Rust `serde(rename_all = "camelCase")` projection.
- `FilePathChipProps.rawPath` / `.label` / `.line` / `.col` ↔ `markdownFileChipPlugin`'s `hProperties` emission ↔ `FileChipFromHast`'s incoming props — all four sites use the same field names.
- `onFileClick(node, event)` signature: same in `FileTreeNode` → `MountSection` → `WorkspaceFilesPanel` → `FilesRail` → `SidePanel`.
- `addPendingAttachmentAction` payload `{ mountId, relPath, name, sessionId, absolutePath }` — same shape produced by chip click and rail Shift-click.

**Module-size cap:** every new TS file ≤ 120 LOC target; every modified file's diff stays under +20 LOC. Well within the 300-line house rule.

**Known risks called out inline:**
1. Task 5/7 ordering: chip imports atoms; we recommend executing Task 7 first to keep tsc green at each commit.
2. Task 10 may need to update `message.fixtures.test.tsx` if a fixture coincidentally contains a recognizable path token — that's not a regression, it's the plugin doing its job.
3. Task 9 has no unit test (debounced batched fetch is awkward under fake timers); manual verification in Task 12 covers it.
