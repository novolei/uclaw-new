# W1 — Renderer Quick Wins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port four Proma v0.9.27 renderer-only improvements into uClaw — code-highlight cache, preview-refresh atom, paste-to-attachment, and a stable sidebar drag strip — laying the foundation for W4 to consume.

**Architecture:** Four independent renderer modules under `ui/`. Zero Rust touched. Zero new npm dependencies. Each module is its own file ≤ 200 lines with a colocated `.test.ts(x)`. Two thin edits into existing files (`rich-text-input.tsx`, `ChatInput.tsx`, `LeftSidebar.tsx`).

**Tech Stack:** React 18 · TypeScript · Vite · Jotai · Vitest + RTL · Tailwind (with uClaw theme tokens) · `sonner` for toasts · `@tauri-apps/api` for event listening.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §3

---

## Pre-flight

- [ ] **Branch setup**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git checkout -b claude/w1-renderer-quick-wins
```

Expected: clean checkout at `main`'s tip, new branch created.

- [ ] **Baseline verification**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: existing tests pass, no fresh TS errors. Record the test count — used as a regression baseline.

---

## File Structure

| Path | Action | Purpose |
|---|---|---|
| `ui/src/components/preview/codeHighlightCache.ts` | create | LRU cache module for `(key) → highlighted HTML` |
| `ui/src/components/preview/codeHighlightCache.test.ts` | create | unit tests |
| `ui/src/atoms/preview-atoms.ts` | create | `previewRefreshAtomFamily` + helpers |
| `ui/src/atoms/preview-atoms.test.ts` | create | unit tests |
| `ui/src/hooks/usePreviewRefresh.ts` | create | hook that subscribes to refresh triggers and reads atom |
| `ui/src/hooks/usePreviewRefresh.test.tsx` | create | RTL test with mocked Tauri events |
| `ui/src/lib/clipboard-attachment.ts` | create | pure functions: markdown detection, timestamp, file factory |
| `ui/src/lib/clipboard-attachment.test.ts` | create | unit tests |
| `ui/src/components/ai-elements/rich-text-input.tsx` | modify | add `onPaste` handler + `onPasteLongText` / `longTextPasteThreshold` props; wire previously-dead `onPasteFiles` |
| `ui/src/components/chat/ChatInput.tsx` | modify | pass `onPasteLongText` callback that converts text → attachment + toast |
| `ui/src/components/app-shell/LeftSidebar.tsx` | modify | convert top drag strip to absolutely-positioned, z-index 1 |
| `ui/src/styles/globals.css` | modify | add `.sidebar-window-drag-strip` rule |

**Module size budget**: every new file ≤ 200 lines. The `LeftSidebar.tsx` change is +5 / –1 lines. `ChatInput.tsx` +18 / –1. `rich-text-input.tsx` +25.

---

## Task 1: Code Highlight Cache Module

**Files:**
- Create: `ui/src/components/preview/codeHighlightCache.ts`
- Test: `ui/src/components/preview/codeHighlightCache.test.ts`

This is a pure LRU cache. No React, no Tauri. W4 will consume it from `CodeRenderer.tsx`.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/preview/codeHighlightCache.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import {
  cacheGet,
  cacheSet,
  cacheKey,
  shouldSkipHighlight,
  __resetCacheForTests,
  MAX_HIGHLIGHT_CHARS,
  CACHE_MAX,
} from './codeHighlightCache'

describe('codeHighlightCache', () => {
  beforeEach(() => __resetCacheForTests())

  describe('cacheKey', () => {
    it('joins gitRoot, filePath, refreshVersion with separator', () => {
      expect(cacheKey({ gitRoot: '/repo', filePath: 'a.ts', refreshVersion: 3 }))
        .toBe('/repo\0a.ts\0v3')
    })

    it('treats null gitRoot as empty string', () => {
      expect(cacheKey({ gitRoot: null, filePath: 'a.ts', refreshVersion: 0 }))
        .toBe('\0a.ts\0v0')
    })
  })

  describe('cacheGet / cacheSet', () => {
    it('returns undefined for missing key', () => {
      expect(cacheGet('missing')).toBeUndefined()
    })

    it('returns stored entry', () => {
      cacheSet('k1', { oldContent: 'a', newContent: 'b' })
      expect(cacheGet('k1')).toEqual({ oldContent: 'a', newContent: 'b' })
    })

    it('promotes entry to MRU on access', () => {
      cacheSet('k1', { oldContent: '1', newContent: '1' })
      cacheSet('k2', { oldContent: '2', newContent: '2' })
      cacheGet('k1') // promote k1
      // fill cache to evict LRU (k2)
      for (let i = 0; i < CACHE_MAX; i++) {
        cacheSet(`fill-${i}`, { oldContent: '', newContent: '' })
      }
      expect(cacheGet('k1')).toBeDefined()
      expect(cacheGet('k2')).toBeUndefined()
    })

    it('evicts oldest entry when over CACHE_MAX', () => {
      for (let i = 0; i < CACHE_MAX + 5; i++) {
        cacheSet(`k-${i}`, { oldContent: String(i), newContent: '' })
      }
      // first 5 should have been evicted
      expect(cacheGet('k-0')).toBeUndefined()
      expect(cacheGet('k-4')).toBeUndefined()
      expect(cacheGet(`k-${CACHE_MAX + 4}`)).toBeDefined()
    })

    it('stores optional highlighted html / lang / theme', () => {
      cacheSet('k1', {
        oldContent: 'x',
        newContent: 'x',
        highlightedHtml: '<pre>x</pre>',
        highlightedLanguage: 'ts',
        highlightedTheme: 'github-dark',
      })
      const e = cacheGet('k1')
      expect(e?.highlightedHtml).toBe('<pre>x</pre>')
      expect(e?.highlightedLanguage).toBe('ts')
      expect(e?.highlightedTheme).toBe('github-dark')
    })
  })

  describe('shouldSkipHighlight', () => {
    it('returns false for short content', () => {
      expect(shouldSkipHighlight('a'.repeat(1000))).toBe(false)
    })

    it('returns true at threshold boundary', () => {
      expect(shouldSkipHighlight('a'.repeat(MAX_HIGHLIGHT_CHARS))).toBe(false)
      expect(shouldSkipHighlight('a'.repeat(MAX_HIGHLIGHT_CHARS + 1))).toBe(true)
    })
  })
})
```

- [ ] **Step 2: Run the test, watch it fail**

```bash
cd ui && npx vitest run src/components/preview/codeHighlightCache.test.ts
```

Expected: FAIL with "Cannot find module './codeHighlightCache'".

- [ ] **Step 3: Implement the module**

Create `ui/src/components/preview/codeHighlightCache.ts`:

```ts
/**
 * Code-highlight cache — Wave 1 of the Proma preview port.
 *
 * Pure LRU keyed by gitRoot + filePath + refreshVersion. Stores both raw
 * content and (optionally) the rendered Shiki HTML so W4's CodeRenderer can
 * skip both IPC and tokenization when the same file is re-previewed under
 * the same theme/language.
 */

export const CACHE_MAX = 50
export const MAX_HIGHLIGHT_CHARS = 200_000

export interface CacheEntry {
  oldContent: string
  newContent: string
  highlightedHtml?: string
  highlightedLanguage?: string
  highlightedTheme?: string
}

export interface CacheKeyParts {
  gitRoot: string | null
  filePath: string
  refreshVersion: number
}

const SEP = '\0'

const cache = new Map<string, CacheEntry>()

export function cacheKey(parts: CacheKeyParts): string {
  return `${parts.gitRoot ?? ''}${SEP}${parts.filePath}${SEP}v${parts.refreshVersion}`
}

export function cacheGet(key: string): CacheEntry | undefined {
  const entry = cache.get(key)
  if (entry === undefined) return undefined
  // promote to MRU
  cache.delete(key)
  cache.set(key, entry)
  return entry
}

export function cacheSet(key: string, entry: CacheEntry): void {
  if (cache.has(key)) {
    cache.delete(key)
  } else if (cache.size >= CACHE_MAX) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
  cache.set(key, entry)
}

export function shouldSkipHighlight(content: string): boolean {
  return content.length > MAX_HIGHLIGHT_CHARS
}

export function __resetCacheForTests(): void {
  cache.clear()
}
```

- [ ] **Step 4: Run the test, watch it pass**

```bash
cd ui && npx vitest run src/components/preview/codeHighlightCache.test.ts
```

Expected: all 8 tests pass.

- [ ] **Step 5: Verify whole suite + typecheck**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -5
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: no new errors. Total test count increased by ≥ 8.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/preview/codeHighlightCache.ts ui/src/components/preview/codeHighlightCache.test.ts
git commit -m "feat(preview): add code highlight cache module"
```

---

## Task 2: Preview Refresh Atom

**Files:**
- Create: `ui/src/atoms/preview-atoms.ts`
- Test: `ui/src/atoms/preview-atoms.test.ts`

A Jotai `atomFamily` keyed by file path that exposes a counter; bumping it invalidates the W1 cache key. W4 will also subscribe to it inside `useFileBytes`.

- [ ] **Step 1: Write the failing test**

Create `ui/src/atoms/preview-atoms.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  previewRefreshVersionAtomFamily,
  bumpPreviewRefreshAtom,
  resetAllPreviewRefreshAtom,
} from './preview-atoms'

describe('preview-atoms', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
    store.set(resetAllPreviewRefreshAtom)
  })

  it('defaults to 0 for any new file path', () => {
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(0)
  })

  it('bumps the version for one file', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(1)
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(2)
  })

  it('does not bump siblings', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/b.ts'))).toBe(0)
  })

  it('reset returns all known paths to 0', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    store.set(bumpPreviewRefreshAtom, '/x/b.ts')
    store.set(resetAllPreviewRefreshAtom)
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(0)
    expect(store.get(previewRefreshVersionAtomFamily('/x/b.ts'))).toBe(0)
  })
})
```

- [ ] **Step 2: Run the test, watch it fail**

```bash
cd ui && npx vitest run src/atoms/preview-atoms.test.ts
```

Expected: FAIL with "Cannot find module './preview-atoms'".

- [ ] **Step 3: Implement the atom module**

Create `ui/src/atoms/preview-atoms.ts`:

```ts
/**
 * preview-atoms — Wave 1 of the Proma preview port.
 *
 * A per-file refresh counter. Anything that should trigger preview re-reads
 * (agent file-write events, window focus, manual refresh button, W3 files-rail
 * change events) bumps the relevant file's counter. The Code highlight cache
 * keys include the version, so a bump naturally invalidates cache entries.
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'

export const previewRefreshVersionAtomFamily = atomFamily((_filePath: string) => atom(0))

/** Bump the refresh counter for one file. Pass the file path as the action payload. */
export const bumpPreviewRefreshAtom = atom(null, (get, set, filePath: string) => {
  const a = previewRefreshVersionAtomFamily(filePath)
  set(a, get(a) + 1)
})

/** Reset all known files' versions to 0. Used by tests; also safe at logout/workspace switch. */
export const resetAllPreviewRefreshAtom = atom(null, (_get, _set) => {
  // atomFamily.getParams() lists previously-accessed params
  for (const p of previewRefreshVersionAtomFamily.getParams()) {
    previewRefreshVersionAtomFamily.remove(p)
  }
})
```

- [ ] **Step 4: Run the test, watch it pass**

```bash
cd ui && npx vitest run src/atoms/preview-atoms.test.ts
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/atoms/preview-atoms.ts ui/src/atoms/preview-atoms.test.ts
git commit -m "feat(preview): add per-file refresh atom + bump action"
```

---

## Task 3: usePreviewRefresh Hook

**Files:**
- Create: `ui/src/hooks/usePreviewRefresh.ts`
- Test: `ui/src/hooks/usePreviewRefresh.test.tsx`

A hook that returns the current refresh version for a file path AND subscribes to Tauri events that should cause bumps. W1 covers two trigger sources: **window focus** and a placeholder **agent file-write** channel. (W3 will add `files_rail:change`; W4 will add the manual refresh button.)

- [ ] **Step 1: Write the failing test**

Create `ui/src/hooks/usePreviewRefresh.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { usePreviewRefresh } from './usePreviewRefresh'
import { bumpPreviewRefreshAtom, resetAllPreviewRefreshAtom } from '@/atoms/preview-atoms'

// Tauri event subscription is mocked to capture the registered handler so the
// test can synthesize events without touching the real bus.
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    if (!listeners.has(name)) listeners.set(name, new Set())
    listeners.get(name)!.add(handler)
    return () => listeners.get(name)!.delete(handler)
  }),
}))

function emit(name: string, payload: unknown): void {
  listeners.get(name)?.forEach((h) => h({ payload }))
}

function wrapper(store: ReturnType<typeof createStore>) {
  return function W({ children }: { children: React.ReactNode }) {
    return <Provider store={store}>{children}</Provider>
  }
}

describe('usePreviewRefresh', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    listeners.clear()
    store = createStore()
    store.set(resetAllPreviewRefreshAtom)
  })

  it('returns 0 by default', () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    expect(result.current).toBe(0)
  })

  it('re-renders when the atom is bumped', () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    act(() => store.set(bumpPreviewRefreshAtom, '/x/a.ts'))
    expect(result.current).toBe(1)
  })

  it('bumps version when an agent:file-written event matches the path', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    // Allow the effect that registers the listener to run
    await act(async () => { await Promise.resolve() })
    act(() => emit('agent:file-written', { path: '/x/a.ts' }))
    expect(result.current).toBe(1)
  })

  it('ignores agent:file-written for unrelated paths', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('agent:file-written', { path: '/x/other.ts' }))
    expect(result.current).toBe(0)
  })

  it('bumps version on tauri://focus regardless of path', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('tauri://focus', undefined))
    expect(result.current).toBe(1)
  })
})
```

- [ ] **Step 2: Run the test, watch it fail**

```bash
cd ui && npx vitest run src/hooks/usePreviewRefresh.test.tsx
```

Expected: FAIL with "Cannot find module './usePreviewRefresh'".

- [ ] **Step 3: Implement the hook**

Create `ui/src/hooks/usePreviewRefresh.ts`:

```ts
/**
 * usePreviewRefresh — Wave 1 of the Proma preview port.
 *
 * Returns the current refresh version for a file path and subscribes to the
 * triggers that should bump it: window focus and agent-side file writes.
 * Consumer modules (W4 useFileBytes, codeHighlightCache key) include the
 * returned number so a bump naturally invalidates their state.
 *
 * Triggers added in later waves:
 *   - W3: files_rail:change
 *   - W4: manual refresh button via bumpPreviewRefreshAtom
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import {
  previewRefreshVersionAtomFamily,
  bumpPreviewRefreshAtom,
} from '@/atoms/preview-atoms'

interface FileWrittenPayload {
  path: string
}

export function usePreviewRefresh(filePath: string | null): number {
  const version = useAtomValue(
    previewRefreshVersionAtomFamily(filePath ?? ''),
  )
  const bump = useSetAtom(bumpPreviewRefreshAtom)

  React.useEffect(() => {
    if (!filePath) return
    let unlistenFocus: (() => void) | undefined
    let unlistenWrite: (() => void) | undefined
    let cancelled = false

    void (async () => {
      const u1 = await listen('tauri://focus', () => {
        if (!cancelled) bump(filePath)
      })
      const u2 = await listen<FileWrittenPayload>('agent:file-written', (evt) => {
        if (cancelled) return
        if (evt.payload?.path === filePath) bump(filePath)
      })
      unlistenFocus = u1
      unlistenWrite = u2
    })()

    return () => {
      cancelled = true
      unlistenFocus?.()
      unlistenWrite?.()
    }
  }, [filePath, bump])

  return version
}
```

- [ ] **Step 4: Run the test, watch it pass**

```bash
cd ui && npx vitest run src/hooks/usePreviewRefresh.test.tsx
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/hooks/usePreviewRefresh.ts ui/src/hooks/usePreviewRefresh.test.tsx
git commit -m "feat(preview): add usePreviewRefresh hook + Tauri event listeners"
```

---

## Task 4: Clipboard Attachment Utilities (Pure Functions)

**Files:**
- Create: `ui/src/lib/clipboard-attachment.ts`
- Test: `ui/src/lib/clipboard-attachment.test.ts`

Pure helpers. Wired into the input component in Task 5.

- [ ] **Step 1: Write the failing test**

Create `ui/src/lib/clipboard-attachment.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import {
  LONG_TEXT_ATTACHMENT_THRESHOLD,
  looksLikeMarkdown,
  formatClipboardTimestamp,
  createClipboardTextFile,
} from './clipboard-attachment'

describe('clipboard-attachment', () => {
  describe('LONG_TEXT_ATTACHMENT_THRESHOLD', () => {
    it('is 500 (matches Proma)', () => {
      expect(LONG_TEXT_ATTACHMENT_THRESHOLD).toBe(500)
    })
  })

  describe('looksLikeMarkdown', () => {
    it('detects ATX header', () => {
      expect(looksLikeMarkdown('# Title\nbody')).toBe(true)
    })

    it('detects fenced code block', () => {
      expect(looksLikeMarkdown('hello\n```ts\nconst x = 1\n```')).toBe(true)
    })

    it('detects pipe table', () => {
      expect(looksLikeMarkdown('| a | b |\n|---|---|\n| 1 | 2 |')).toBe(true)
    })

    it('detects YAML frontmatter', () => {
      expect(looksLikeMarkdown('---\ntitle: x\n---\nbody')).toBe(true)
    })

    it('detects blockquote', () => {
      expect(looksLikeMarkdown('hello\n> quoted text')).toBe(true)
    })

    it('detects unordered list', () => {
      expect(looksLikeMarkdown('intro\n- one\n- two')).toBe(true)
    })

    it('detects ordered list', () => {
      expect(looksLikeMarkdown('intro\n1. one\n2. two')).toBe(true)
    })

    it('detects inline link', () => {
      expect(looksLikeMarkdown('see [docs](https://x)')).toBe(true)
    })

    it('rejects plain prose', () => {
      expect(looksLikeMarkdown('Just a plain paragraph with no markup at all.'))
        .toBe(false)
    })
  })

  describe('formatClipboardTimestamp', () => {
    it('zero-pads fields to YYYYMMDD-HHMMSS', () => {
      const ts = formatClipboardTimestamp(new Date(2026, 0, 7, 4, 5, 9))
      // month is 0-indexed so January === '01'
      expect(ts).toBe('20260107-040509')
    })

    it('handles December and 23:59:59', () => {
      const ts = formatClipboardTimestamp(new Date(2026, 11, 31, 23, 59, 59))
      expect(ts).toBe('20261231-235959')
    })
  })

  describe('createClipboardTextFile', () => {
    it('produces .md + text/markdown for markdown-looking text', () => {
      const f = createClipboardTextFile('# heading\nbody')
      expect(f.name).toMatch(/^clipboard-\d{8}-\d{6}\.md$/)
      expect(f.type).toBe('text/markdown')
    })

    it('produces .txt + text/plain for plain text', () => {
      const f = createClipboardTextFile('just some text')
      expect(f.name).toMatch(/^clipboard-\d{8}-\d{6}\.txt$/)
      expect(f.type).toBe('text/plain')
    })

    it('writes the input text into the File', async () => {
      const f = createClipboardTextFile('payload here')
      const text = await f.text()
      expect(text).toBe('payload here')
    })
  })
})
```

- [ ] **Step 2: Run the test, watch it fail**

```bash
cd ui && npx vitest run src/lib/clipboard-attachment.test.ts
```

Expected: FAIL with "Cannot find module './clipboard-attachment'".

- [ ] **Step 3: Implement the helpers**

Create `ui/src/lib/clipboard-attachment.ts`:

```ts
/**
 * clipboard-attachment — Wave 1 of the Proma preview port.
 *
 * Pure helpers that convert pasted long text into a File ready for the
 * existing attachment pipeline. Mirrors Proma's AgentView logic:
 *  - LONG_TEXT_ATTACHMENT_THRESHOLD = 500
 *  - markdown-looking text → clipboard-YYYYMMDD-HHMMSS.md + text/markdown
 *  - otherwise → .txt + text/plain
 */

export const LONG_TEXT_ATTACHMENT_THRESHOLD = 500

const MARKDOWN_PATTERNS: readonly RegExp[] = [
  /^#{1,6}\s+\S/m,
  /```[\s\S]*?```/,
  /^\s*\|.+\|\s*\n\s*\|[\s:-]+\|/m,
  /^---\n[\s\S]*?\n---\n/,
  /^\s*> .+/m,
  /^\s*[-*+]\s+\S/m,
  /^\s*\d+\.\s+\S/m,
  /\[[^\]]+\]\([^)]+\)/,
]

export function looksLikeMarkdown(text: string): boolean {
  return MARKDOWN_PATTERNS.some((p) => p.test(text))
}

export function formatClipboardTimestamp(date: Date = new Date()): string {
  const pad = (n: number): string => String(n).padStart(2, '0')
  return (
    `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}` +
    `-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`
  )
}

export function createClipboardTextFile(text: string): File {
  const isMd = looksLikeMarkdown(text)
  const ext = isMd ? 'md' : 'txt'
  const mediaType = isMd ? 'text/markdown' : 'text/plain'
  const filename = `clipboard-${formatClipboardTimestamp()}.${ext}`
  return new File([text], filename, { type: mediaType })
}
```

- [ ] **Step 4: Run the test, watch it pass**

```bash
cd ui && npx vitest run src/lib/clipboard-attachment.test.ts
```

Expected: all 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/clipboard-attachment.ts ui/src/lib/clipboard-attachment.test.ts
git commit -m "feat(chat): add clipboard-attachment helpers (markdown detection + file factory)"
```

---

## Task 5: Wire onPaste in RichTextInput

**Files:**
- Modify: `ui/src/components/ai-elements/rich-text-input.tsx`

The current placeholder declares `onPasteFiles?: (files: File[]) => void` but **never wires it** — the textarea has no `onPaste`. Task 5 wires the prop AND adds `onPasteLongText` / `longTextPasteThreshold`.

- [ ] **Step 1: Read the current file**

```bash
cat ui/src/components/ai-elements/rich-text-input.tsx
```

Expected: 57 lines, header comment `[PLACEHOLDER] ai-elements/rich-text-input — 待后续任务迁移`. The textarea on line ~47 has no `onPaste`. `onPasteFiles` is declared in the interface but not destructured.

- [ ] **Step 2: Replace the entire file**

Overwrite `ui/src/components/ai-elements/rich-text-input.tsx`:

```tsx
// [PLACEHOLDER] ai-elements/rich-text-input — paste hooks wired in W1.
// A real TipTap port lives in W4's Preview Engine; for now this stays
// a thin textarea that nonetheless honors onPasteFiles + onPasteLongText.
import * as React from 'react'

interface RichTextInputProps {
  value: string
  onChange: (value: string) => void
  onSubmit: () => void
  onPasteFiles?: (files: File[]) => void
  /** Called when pasted plain text is >= longTextPasteThreshold. Receives the text. */
  onPasteLongText?: (text: string) => void
  /** Override the default threshold for onPasteLongText. Defaults to 500. */
  longTextPasteThreshold?: number
  placeholder?: string
  disabled?: boolean
  autoFocusTrigger?: string
  collapsible?: boolean
  workspacePath?: string | null
  workspaceSlug?: string | null
  attachedDirs?: string[]
  htmlValue?: string
  onHtmlChange?: (html: string) => void
  sendWithCmdEnter?: boolean
}

export function RichTextInput({
  value,
  onChange,
  onSubmit,
  onPasteFiles,
  onPasteLongText,
  longTextPasteThreshold = 500,
  placeholder,
  disabled,
  sendWithCmdEnter,
}: RichTextInputProps): React.ReactElement {
  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (sendWithCmdEnter) {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
          e.preventDefault()
          onSubmit()
        }
      } else {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault()
          onSubmit()
        }
      }
    },
    [onSubmit, sendWithCmdEnter],
  )

  const handlePaste = React.useCallback(
    (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const files = Array.from(e.clipboardData?.files ?? [])
      if (files.length > 0 && onPasteFiles) {
        e.preventDefault()
        onPasteFiles(files)
        return
      }
      const text = e.clipboardData?.getData('text/plain') ?? ''
      if (text.length >= longTextPasteThreshold && onPasteLongText) {
        e.preventDefault()
        onPasteLongText(text)
        return
      }
      // fall through to default paste
    },
    [onPasteFiles, onPasteLongText, longTextPasteThreshold],
  )

  return (
    <textarea
      className="w-full resize-none bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground/50 min-h-[44px] max-h-[200px]"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={handleKeyDown}
      onPaste={handlePaste}
      placeholder={placeholder}
      disabled={disabled}
      rows={1}
    />
  )
}
```

- [ ] **Step 3: Type-check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no new errors. (Existing ChatInput.tsx already passes `onPasteFiles` so this strictly enables it.)

- [ ] **Step 4: Run existing tests**

```bash
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: no regressions.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/ai-elements/rich-text-input.tsx
git commit -m "feat(chat): wire onPaste handler in RichTextInput (files + long text)"
```

---

## Task 6: Wire Long-Text Paste in ChatInput

**Files:**
- Modify: `ui/src/components/chat/ChatInput.tsx`

Pass the `onPasteLongText` callback so pasted text ≥ 500 chars becomes an attachment with a toast.

- [ ] **Step 1: Read relevant region**

```bash
sed -n '1,30p;260,280p' ui/src/components/chat/ChatInput.tsx
```

Expected output includes: imports block; the `<RichTextInput ... onPasteFiles={handlePasteFiles} />` JSX around line 265-273.

- [ ] **Step 2: Add the import**

Open `ui/src/components/chat/ChatInput.tsx` and verify the existing toast import. If `sonner` is not yet imported, add at top of imports:

```ts
import { toast } from 'sonner'
```

If `toast` is already imported, skip.

To confirm:

```bash
grep -n "from 'sonner'" ui/src/components/chat/ChatInput.tsx
```

If empty, add the import line just below the React import.

Add the clipboard-attachment import:

```ts
import { createClipboardTextFile } from '@/lib/clipboard-attachment'
```

- [ ] **Step 3: Identify the existing `handlePasteFiles` function**

```bash
grep -n "handlePasteFiles" ui/src/components/chat/ChatInput.tsx
```

Note its definition and what it calls (almost certainly a function that turns `File[]` → pending attachments via an existing util like `addFilesAsAttachments` or similar). The new callback reuses the same downstream call.

- [ ] **Step 4: Add the long-text handler beside `handlePasteFiles`**

Find the line declaring `const handlePasteFiles = React.useCallback(` and immediately after the `useCallback(...)` for it, add:

```tsx
  const handlePasteLongText = React.useCallback(
    (text: string): void => {
      const file = createClipboardTextFile(text)
      handlePasteFiles([file])
      toast.success('已将超长文本转为附件', { description: file.name })
    },
    [handlePasteFiles],
  )
```

The exact insertion line depends on the existing layout; place it directly after the `handlePasteFiles` declaration.

- [ ] **Step 5: Pass the prop to RichTextInput**

Change the JSX from:

```tsx
<RichTextInput
  value={content}
  onChange={setContent}
  onSubmit={handleSend}
  onPasteFiles={handlePasteFiles}
  placeholder={sendWithCmdEnter ? '输入消息... (⌘/Ctrl+Enter 发送，Enter 换行)' : '输入消息... (Enter 发送，Shift+Enter 换行)'}
  autoFocusTrigger={conversationId}
  sendWithCmdEnter={sendWithCmdEnter}
/>
```

to:

```tsx
<RichTextInput
  value={content}
  onChange={setContent}
  onSubmit={handleSend}
  onPasteFiles={handlePasteFiles}
  onPasteLongText={handlePasteLongText}
  placeholder={sendWithCmdEnter ? '输入消息... (⌘/Ctrl+Enter 发送，Enter 换行)' : '输入消息... (Enter 发送，Shift+Enter 换行)'}
  autoFocusTrigger={conversationId}
  sendWithCmdEnter={sendWithCmdEnter}
/>
```

- [ ] **Step 6: Type-check + tests**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: no new errors, no regressions.

- [ ] **Step 7: Manual smoke (recommended; optional if blocked)**

If the dev server is available:

```bash
cd src-tauri && cargo tauri dev
```

In the running app, focus the chat input, then paste a long Markdown blob (≥ 500 chars). Expected: input stays empty, a chip appears in the attachment list named `clipboard-YYYYMMDD-HHMMSS.md`, and a sonner toast pops "已将超长文本转为附件" with the filename in the description.

If you have no dev server access, defer this to the W1 PR's manual-test section.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/chat/ChatInput.tsx
git commit -m "feat(chat): paste long text as attachment in ChatInput"
```

---

## Task 7: Sidebar Drag Strip Stability Fix

**Files:**
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx`
- Modify: `ui/src/styles/globals.css`

uClaw already has a 30px drag strip on line 752 of `LeftSidebar.tsx`, but it lives as a flex child. Proma PR #408 moves it to **absolute positioning at z-1** so WKWebView's text-selection layer can't pre-empt it. uClaw has no "collapsed" sidebar mode today, so only the expanded case applies.

- [ ] **Step 1: Read current strip implementation**

```bash
sed -n '747,755p' ui/src/components/app-shell/LeftSidebar.tsx
```

Expected:
```
  // ===== 展开状态 =====
  return (
    <div className="h-full flex flex-col bg-background rounded-2xl shadow-xl transition-[width] duration-300" style={{ width: width ?? 280, minWidth: 180, flexShrink: 1 }}>
      {/* 顶部独立拖拽条：30px 给红绿灯留位置 + 让用户从此处拖动窗口
          (与 AppShell 的 fixed z-50 拖拽条互补——这里覆盖 sidebar 内部) */}
      <div data-tauri-drag-region className="h-[30px] flex-shrink-0 titlebar-drag-region" />
      <div>
        <div className="flex items-start gap-1.5 px-3">
          <div className="flex-1 min-w-0"><ModeSwitcher /></div>
```

- [ ] **Step 2: Convert the outer container to `relative`**

In `ui/src/components/app-shell/LeftSidebar.tsx`, change the line that begins `<div className="h-full flex flex-col bg-background rounded-2xl shadow-xl transition-[width] duration-300"` to add `relative`:

```tsx
    <div className="relative h-full flex flex-col bg-background rounded-2xl shadow-xl transition-[width] duration-300" style={{ width: width ?? 280, minWidth: 180, flexShrink: 1 }}>
```

- [ ] **Step 3: Convert the drag strip to absolute positioning**

Replace the existing strip JSX:

```tsx
      <div data-tauri-drag-region className="h-[30px] flex-shrink-0 titlebar-drag-region" />
```

with:

```tsx
      {/* 顶部独立拖拽条：absolute + z-1 让它叠在 sidebar 内容之上，
          WKWebView 的文本选择层不会再抢占拖拽。 */}
      <div data-tauri-drag-region aria-hidden="true" className="sidebar-window-drag-strip h-[30px]" />
      {/* 占位高度，让正常 flex 流仍然预留 30px。 */}
      <div className="h-[30px] flex-shrink-0" aria-hidden="true" />
```

- [ ] **Step 4: Add the CSS rule**

Open `ui/src/styles/globals.css`. Find the existing `.titlebar-drag-region` rule (around line 555-565). Below it, add:

```css
/* Left sidebar sits above the global titlebar layer, so it owns a stable top drag strip. */
.sidebar-window-drag-strip {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 1;
  -webkit-app-region: drag;
  app-region: drag;
}
```

- [ ] **Step 5: Type-check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no new errors.

- [ ] **Step 6: Quick manual sanity (optional if dev server unavailable)**

If running `cargo tauri dev`: top 30px of the left sidebar drags the window. Clicking on the area below 30px (mode switcher, new-session button) is unaffected.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/app-shell/LeftSidebar.tsx ui/src/styles/globals.css
git commit -m "fix(app-shell): pin sidebar drag strip absolute + z-1 for WKWebView stability"
```

---

## Task 8: Final Verification

- [ ] **Step 1: Full type-check**

```bash
cd ui && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 2: Full test suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -15
```

Expected: all green. Test count up ≥ 31 (8 cache + 4 atom + 5 hook + 14 clipboard utils).

- [ ] **Step 3: Lint for hardcoded colors in new files (UI/UX gate)**

```bash
grep -nE '#[0-9a-fA-F]{3,8}|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/preview/codeHighlightCache.ts \
  ui/src/atoms/preview-atoms.ts \
  ui/src/hooks/usePreviewRefresh.ts \
  ui/src/lib/clipboard-attachment.ts \
  ui/src/components/ai-elements/rich-text-input.tsx 2>/dev/null
```

Expected: empty output (no matches in new code).

- [ ] **Step 4: Rust side untouched**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: empty / no errors. (We touched no Rust.)

- [ ] **Step 5: Git log review**

```bash
git log --oneline main..HEAD
```

Expected: 7 commits in the exact order:

1. `feat(preview): add code highlight cache module`
2. `feat(preview): add per-file refresh atom + bump action`
3. `feat(preview): add usePreviewRefresh hook + Tauri event listeners`
4. `feat(chat): add clipboard-attachment helpers (markdown detection + file factory)`
5. `feat(chat): wire onPaste handler in RichTextInput (files + long text)`
6. `feat(chat): paste long text as attachment in ChatInput`
7. `fix(app-shell): pin sidebar drag strip absolute + z-1 for WKWebView stability`

Each commit is bisectable: cache works without the hook, hook works without paste, etc.

- [ ] **Step 6: Push and open PR**

```bash
git push -u origin claude/w1-renderer-quick-wins
gh pr create --title "W1: Proma v0.9.27 renderer quick wins (cache + refresh + paste + drag)" --body "$(cat <<'EOF'
## Summary

Wave 1 of the Proma v0.9.27 port (see [spec](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md)). Pure renderer changes, 0 Rust touched, 0 new deps.

- **Code-highlight cache** (`ui/src/components/preview/codeHighlightCache.ts`) — LRU 50, key `gitRoot:filePath:refreshVersion`, skip-large at 200k chars. Consumed by W4.
- **Preview refresh atom** (`ui/src/atoms/preview-atoms.ts`) — `atomFamily<filePath, number>` + `bumpPreviewRefreshAtom`.
- **usePreviewRefresh** (`ui/src/hooks/usePreviewRefresh.ts`) — subscribes to `tauri://focus` and `agent:file-written`.
- **Clipboard-attachment utils** (`ui/src/lib/clipboard-attachment.ts`) — markdown detection (8 regex) + `clipboard-YYYYMMDD-HHMMSS.{md|txt}` factory.
- **RichTextInput onPaste** — wires the previously-dead `onPasteFiles` prop and adds `onPasteLongText` + `longTextPasteThreshold`.
- **ChatInput paste-to-attachment** — pastes ≥ 500 chars become attachment + toast.
- **Sidebar drag strip** — absolute + z-1 for WKWebView stability.

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1 | `feat(preview): add code highlight cache module` | LRU cache module |
| 2 | `feat(preview): add per-file refresh atom + bump action` | Jotai atomFamily |
| 3 | `feat(preview): add usePreviewRefresh hook + Tauri event listeners` | hook wiring |
| 4 | `feat(chat): add clipboard-attachment helpers (markdown detection + file factory)` | pure utils |
| 5 | `feat(chat): wire onPaste handler in RichTextInput (files + long text)` | input plumbing |
| 6 | `feat(chat): paste long text as attachment in ChatInput` | wire callback + toast |
| 7 | `fix(app-shell): pin sidebar drag strip absolute + z-1 for WKWebView stability` | drag stability |

## Test plan

- [x] `cd ui && npx tsc --noEmit` clean
- [x] `cd ui && npm test -- --run` all green (+31 tests)
- [x] `cd src-tauri && cargo build` clean (no Rust touched)
- [ ] Manual: paste 600-char markdown → `clipboard-…md` attachment + toast
- [ ] Manual: paste 600-char plain text → `clipboard-…txt` attachment + toast
- [ ] Manual: paste file via clipboard → file attachment (previously broken, now wired)
- [ ] Manual: drag top 30px of sidebar → window moves
- [ ] Manual: confirm no theme regressions (warm-paper / qingye / forest-*)
- [ ] Manual: `prefers-reduced-motion` honored (no new transitions added)
EOF
)"
```

Expected: PR opens, gh prints the URL.

---

## Self-Review (run mentally before handoff)

**Spec coverage** — each spec §3.x maps to a task:

| Spec | Task |
|---|---|
| §3.2 highlight cache (Proma PR #416) | Task 1 |
| §3.2 refresh atom (Proma PR #409 mechanism) | Tasks 2, 3 |
| §3.2 paste-to-attachment (Proma PR #415) | Tasks 4, 5, 6 |
| §3.2 sidebar drag strip (Proma PR #408) | Task 7 |
| §3.3 commits + verification | Tasks 7, 8 |

**Type consistency:**
- `previewRefreshVersionAtomFamily(filePath: string) → Atom<number>` — used identically in Tasks 2, 3.
- `LONG_TEXT_ATTACHMENT_THRESHOLD = 500` defined in Task 4, default in Task 5 (`longTextPasteThreshold = 500`), threshold also 500 in Task 6 narrative — consistent.
- `createClipboardTextFile(text: string) → File` — same signature in Tasks 4 and 6.

**Placeholder scan:** none — every code step contains complete code.

**Module size:**
- codeHighlightCache.ts: ~70 lines ✓
- preview-atoms.ts: ~35 lines ✓
- usePreviewRefresh.ts: ~60 lines ✓
- clipboard-attachment.ts: ~45 lines ✓
- rich-text-input.tsx after change: ~88 lines ✓
- ChatInput.tsx delta: +~12 lines ✓
- LeftSidebar.tsx delta: +5 / –1 ✓

All ≤ 300 target.

**Deferred to later waves (intentionally not in W1):**
- Cache *consumption* in CodeRenderer → W4 Task 2
- Refresh-atom *additional triggers* (files_rail:change) → W3
- Refresh-atom *additional triggers* (manual button) → W4 Task 1
- A real TipTap RichTextInput → W4 (paragraph 6.6 of spec)
