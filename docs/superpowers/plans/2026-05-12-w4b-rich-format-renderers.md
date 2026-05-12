# W4b — Rich Format Renderers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add PDF / DOCX / XLSX / PPTX rendering + a friendly hint for legacy `.doc`/`.xls`/`.ppt` to uClaw's preview engine. All parsing runs in the renderer (no new Rust); pdfjs's worker is split into its own chunk for lazy load.

**Architecture:** New `ui/src/components/preview/office-parsers/` directory ports Proma v0.9.27's pure-JS Office parsers (XLSX + PPTX = JSZip + `@xmldom/xmldom` to unzip the docx/xlsx/pptx archive and walk its XML; DOCX = mammoth.browser → HTML). A new `PdfRenderer.tsx` lazy-imports `pdfjs-dist` and renders pages to canvas with a zoom toolbar. `ext-classifier.ts` gains 5 new `RendererKind` values; `PreviewSurface.tsx` dispatches to the new renderers. Office output styles are ported from Proma's `globals.css` with theme-token swaps.

**Tech Stack:** React 18 + TypeScript · `jszip` ^3.10 · `@xmldom/xmldom` ^0.8 · `mammoth` ^1.12 (browser build) · `pdfjs-dist` ^4 · existing Tailwind theme tokens.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6 rich-formats subset.

**Proma reference (read-only)**: `/Users/ryanliu/Documents/Proma/apps/electron/src/main/lib/file-preview-service.ts` (full file) — pure-JS implementation we're porting. `apps/electron/src/renderer/styles/globals.css` lines 848–965 — Office CSS.

**Out of W4b scope** (deferred to W4c / later):
- Inline editing for any format
- File-path chips in agent messages
- Detached preview window
- Diff renderer

---

## Pre-flight

- [ ] **Branch setup** (already done at plan-writing time; confirm)

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git checkout -b claude/w4b-rich-formats   # or reuse if already created
```

- [ ] **Baseline**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3    # 391 baseline
cd ../ui && npx tsc --noEmit 2>&1 | tail -3
cd ui && npm test -- --run 2>&1 | tail -3          # 275 baseline (or more if polish PR #109 merged)
```

- [ ] **Note**: PR #109 (preview polish) is open against `main`. W4b touches **different files** for the most part — only `ext-classifier.ts` and `PreviewSurface.tsx` overlap. If #109 merges first, W4b auto-picks up the polish; if W4b merges first, #109 will rebase cleanly. Either order works.

---

## File Structure

### New TypeScript modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `ui/src/components/preview/office-parsers/xml-utils.ts` | ~80 | shared helpers — `parseXml`, `getElementsByLocalName`, `getFirstTextByLocalName`, `readZipText`, `escapeHtml`, `parseRelationships` |
| `ui/src/components/preview/office-parsers/docx.ts` | ~50 | thin mammoth wrapper, returns `{ html: string; messages: string[] }` |
| `ui/src/components/preview/office-parsers/xlsx.ts` | ~220 | port of Proma's `convertXlsxToHtml` — shared strings + date styles + sheet rows + table HTML emit |
| `ui/src/components/preview/office-parsers/pptx.ts` | ~120 | port of Proma's `convertPptxToHtml` — slide path resolution + per-slide text extraction + slide HTML emit |
| `ui/src/components/preview/renderers/DocxRenderer.tsx` | ~70 | consumes `docx.ts`, scoped sanitized HTML render |
| `ui/src/components/preview/renderers/XlsxRenderer.tsx` | ~70 | consumes `xlsx.ts`, scoped sanitized HTML render |
| `ui/src/components/preview/renderers/PptxRenderer.tsx` | ~70 | consumes `pptx.ts`, scoped sanitized HTML render |
| `ui/src/components/preview/renderers/PdfRenderer.tsx` | ~160 | lazy-imports pdfjs, renders pages to canvas, zoom toolbar |
| `ui/src/components/preview/renderers/LegacyOfficeHint.tsx` | ~70 | friendly placeholder for `.doc` / `.xls` / `.ppt` |

### Modified TypeScript files

| Path | Edit |
|---|---|
| `ui/package.json` | add `jszip`, `@xmldom/xmldom`, `mammoth`, `pdfjs-dist` to `dependencies` |
| `ui/vite.config.ts` | add `manualChunks` for `pdfjs-worker` + `office-parsers` |
| `ui/src/components/preview/utils/ext-classifier.ts` | extend `RendererKind` with `'pdf' \| 'docx' \| 'xlsx' \| 'pptx' \| 'legacyOffice'`; add ext entries |
| `ui/src/components/preview/utils/ext-classifier.test.ts` | add tests for the new classifications |
| `ui/src/components/preview/PreviewSurface.tsx` | dispatch new kinds |
| `ui/src/styles/globals.css` | port Proma's `.office-preview*` / `.office-table-wrap*` / `.office-sheet` / `.office-slide` rules (~120 lines, theme-token-based) |

**Module size budget**: every new file ≤ 250 lines target. Largest is `xlsx.ts` at ~220 LOC.

**Total new code**: ~10 new files, ~1000 LoC + ~120 CSS lines. 6 modified files.

---

## Task 1: New deps + Vite chunking + classifier extension

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/vite.config.ts`
- Modify: `ui/src/components/preview/utils/ext-classifier.ts`
- Modify: `ui/src/components/preview/utils/ext-classifier.test.ts`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w4b-rich-formats && git branch --show-current
```
Output MUST print `claude/w4b-rich-formats`.

- [ ] **Step 2: Add deps**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm install --save jszip@^3.10 @xmldom/xmldom@^0.8 mammoth@^1.12 pdfjs-dist@^4
```

Wait for install to complete. The `pdfjs-dist` package is ~6 MB but the worker chunk will be code-split out so it only loads when a PDF is opened.

- [ ] **Step 3: Vite manual-chunks**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "manualChunks" ui/vite.config.ts | head -3
```

Find the existing `manualChunks` definition. It's likely already a function or an object mapping. Add two new chunks:

If `manualChunks` is an **object** literal:

```ts
manualChunks: {
  // ...existing entries...
  'pdfjs-worker': ['pdfjs-dist/build/pdf.worker.min.mjs'],
  'office-parsers': ['jszip', '@xmldom/xmldom', 'mammoth'],
},
```

If it's a **function**:

```ts
manualChunks(id) {
  // ...existing rules first...
  if (id.includes('pdfjs-dist/build/pdf.worker')) return 'pdfjs-worker'
  if (id.includes('node_modules/jszip')
      || id.includes('node_modules/@xmldom')
      || id.includes('node_modules/mammoth')) return 'office-parsers'
  // ...rest of existing function...
}
```

Pick the form matching uClaw's existing style. If `manualChunks` doesn't exist at all in vite.config.ts, add the object form at the top level of `build.rollupOptions.output.manualChunks`.

- [ ] **Step 4: Extend `ext-classifier.ts`**

Use the `Edit` tool. Find the `RendererKind` type:

```ts
export type RendererKind = 'image' | 'markdown' | 'code' | 'binary'
```

Replace with:

```ts
export type RendererKind =
  | 'image'
  | 'markdown'
  | 'code'
  | 'pdf'
  | 'docx'
  | 'xlsx'
  | 'pptx'
  | 'legacyOffice'
  | 'binary'
```

Find the `classifyExtension` function. Replace its body with:

```ts
export function classifyExtension(filename: string): ClassificationResult {
  const ext = getExtension(filename)
  if (ext && IMAGE_EXTS.has(ext)) return { kind: 'image', ext }
  if (ext && MD_EXTS.has(ext)) return { kind: 'markdown', ext }
  if (ext === 'pdf') return { kind: 'pdf', ext }
  if (ext === 'docx') return { kind: 'docx', ext }
  if (ext === 'xlsx') return { kind: 'xlsx', ext }
  if (ext === 'pptx') return { kind: 'pptx', ext }
  if (ext === 'doc' || ext === 'xls' || ext === 'ppt') {
    return { kind: 'legacyOffice', ext }
  }
  if (ext && CODE_EXTS.has(ext)) {
    return { kind: 'code', ext, language: CODE_EXTS.get(ext) }
  }
  return { kind: 'binary', ext }
}
```

- [ ] **Step 5: Add tests for new kinds**

In `ui/src/components/preview/utils/ext-classifier.test.ts`, find the `classifyExtension` describe block. Add these tests at the end (before the `'exports immutable sets'` test):

```ts
    it('routes pdf', () => {
      expect(classifyExtension('a.pdf').kind).toBe('pdf')
    })

    it('routes docx / xlsx / pptx', () => {
      expect(classifyExtension('a.docx').kind).toBe('docx')
      expect(classifyExtension('a.xlsx').kind).toBe('xlsx')
      expect(classifyExtension('a.pptx').kind).toBe('pptx')
    })

    it('routes legacy office (.doc / .xls / .ppt) to legacyOffice', () => {
      expect(classifyExtension('a.doc').kind).toBe('legacyOffice')
      expect(classifyExtension('a.xls').kind).toBe('legacyOffice')
      expect(classifyExtension('a.ppt').kind).toBe('legacyOffice')
    })

    it('routes uppercase extensions case-insensitively', () => {
      expect(classifyExtension('a.PDF').kind).toBe('pdf')
      expect(classifyExtension('a.DOCX').kind).toBe('docx')
      expect(classifyExtension('a.DOC').kind).toBe('legacyOffice')
    })
```

- [ ] **Step 6: Build + test**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: TS clean. Test count up by 4 (baseline + 4 new ext-classifier tests).

- [ ] **Step 7: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/package.json
 M ui/package-lock.json   (or pnpm-lock.yaml)
 M ui/vite.config.ts
 M ui/src/components/preview/utils/ext-classifier.ts
 M ui/src/components/preview/utils/ext-classifier.test.ts
```

If any unrelated file appears, STOP and report BLOCKED.

- [ ] **Step 8: Commit**

```bash
git add ui/package.json ui/package-lock.json ui/vite.config.ts ui/src/components/preview/utils/ext-classifier.ts ui/src/components/preview/utils/ext-classifier.test.ts
git commit -m "feat(preview): add office + pdf deps + classifier extensions (pdf/docx/xlsx/pptx/legacyOffice)"
```

(If lockfile is `pnpm-lock.yaml` instead, substitute accordingly.)

---

## Task 2: Office CSS port

**Files:**
- Modify: `ui/src/styles/globals.css`

Port Proma's office CSS rules. They already use `hsl(var(--foreground))` / `hsl(var(--border))` / `hsl(var(--muted))` / `hsl(var(--muted-foreground))` — same token names as uClaw — so the port is verbatim.

- [ ] **Step 1: Find an insertion point in globals.css**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "sidebar-window-drag-strip\|titlebar-drag-region" ui/src/styles/globals.css | head -3
```

The W1 sidebar drag-strip rule is around line 565. We'll add the office rules in their own clearly-labeled section. Find the end of the file or a section divider to anchor onto. The safest insertion point is **right after** the last existing rule. To find it:

```bash
cd /Users/ryanliu/Documents/uclaw && tail -30 ui/src/styles/globals.css
```

Note the last selector. Append after the last `}`.

- [ ] **Step 2: Append the office rules**

Use the `Edit` tool. Anchor on a unique identifier at the end of the file (last selector you found). Append the block AFTER it:

```css

/* ===== W4b: Office preview rules (DOCX / XLSX / PPTX) ===== */

.office-preview-host {
  min-height: 100%;
  padding: 12px;
  color: hsl(var(--foreground));
  font-size: 12px;
}

.office-preview {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.office-preview-title {
  color: hsl(var(--foreground) / 0.72);
  font-weight: 600;
  line-height: 1.4;
  word-break: break-word;
}

.office-preview-notice,
.office-empty {
  color: hsl(var(--muted-foreground));
  font-size: 11px;
  line-height: 1.5;
}

.office-preview-notice {
  border: 1px solid hsl(var(--border) / 0.45);
  border-radius: 8px;
  background: hsl(var(--muted) / 0.24);
  padding: 8px 10px;
}

.office-sheet,
.office-slide {
  border: 1px solid hsl(var(--border) / 0.5);
  border-radius: 8px;
  background: hsl(var(--background) / 0.72);
  overflow: hidden;
}

.office-sheet h3,
.office-slide h3 {
  margin: 0;
  color: hsl(var(--foreground));
  font-size: 13px;
  font-weight: 600;
  line-height: 1.45;
}

.office-sheet h3 {
  padding: 10px 12px;
  border-bottom: 1px solid hsl(var(--border) / 0.45);
}

.office-table-wrap {
  overflow: auto;
  max-width: 100%;
}

.office-table-wrap table {
  width: max-content;
  min-width: 100%;
  border-collapse: separate;
  border-spacing: 0;
  font-variant-numeric: tabular-nums;
}

.office-table-wrap th,
.office-table-wrap td {
  max-width: 280px;
  min-width: 72px;
  border-right: 1px solid hsl(var(--border) / 0.35);
  border-bottom: 1px solid hsl(var(--border) / 0.35);
  padding: 6px 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}

.office-table-wrap thead th,
.office-row-heading {
  position: sticky;
  z-index: 1;
  background: hsl(var(--muted) / 0.36);
  color: hsl(var(--muted-foreground));
  font-weight: 500;
}

.office-table-wrap thead th {
  top: 0;
}

.office-row-heading {
  left: 0;
  min-width: 44px;
  text-align: right;
}

.office-slide {
  padding: 12px;
}

.office-slide-index {
  margin-bottom: 6px;
  color: hsl(var(--muted-foreground));
  font-size: 11px;
  line-height: 1.4;
}

.office-slide ul {
  margin: 8px 0 0;
  padding-left: 18px;
  color: hsl(var(--foreground) / 0.82);
  line-height: 1.6;
}

.office-slide li + li {
  margin-top: 4px;
}

/* DOCX uses mammoth's plain HTML — give it minimal padding + theme typography */
.office-docx-host {
  padding: 24px;
  color: hsl(var(--foreground) / 0.88);
  font-size: 13px;
  line-height: 1.7;
  max-width: 880px;
  margin: 0 auto;
}

.office-docx-host h1,
.office-docx-host h2,
.office-docx-host h3 {
  color: hsl(var(--foreground));
  font-weight: 600;
  margin: 1.4em 0 0.6em;
}

.office-docx-host h1 { font-size: 20px; }
.office-docx-host h2 { font-size: 17px; }
.office-docx-host h3 { font-size: 14px; }

.office-docx-host p { margin: 0.6em 0; }
.office-docx-host ul,
.office-docx-host ol { padding-left: 1.4em; }
.office-docx-host li { margin: 0.2em 0; }
.office-docx-host strong { font-weight: 600; color: hsl(var(--foreground)); }
.office-docx-host em { font-style: italic; }
.office-docx-host a {
  color: hsl(var(--primary));
  text-decoration: underline;
  text-underline-offset: 2px;
}
.office-docx-host table {
  border-collapse: collapse;
  width: 100%;
  margin: 1em 0;
  font-size: 12px;
}
.office-docx-host th,
.office-docx-host td {
  border: 1px solid hsl(var(--border) / 0.5);
  padding: 6px 10px;
  text-align: left;
}
.office-docx-host th {
  background: hsl(var(--muted) / 0.4);
  font-weight: 600;
}
```

- [ ] **Step 3: Build + spot-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | grep -E "^error|Error" | head -3
```

Expected: build completes without errors (CSS additions don't affect build success).

- [ ] **Step 4: Commit**

```bash
git add ui/src/styles/globals.css
git commit -m "feat(preview): port Proma office CSS rules (.office-preview, .office-table, .office-slide)"
```

---

## Task 3: XML utilities

**Files:**
- Create: `ui/src/components/preview/office-parsers/xml-utils.ts`

Shared helpers for the XLSX/PPTX parsers. Port from Proma's `file-preview-service.ts` lines 150–225, adapting `AdmZip` → `JSZip` (async API).

- [ ] **Step 1: Create the file**

```ts
/**
 * xml-utils — Shared helpers for the Office parsers.
 *
 * Ports Proma v0.9.27's pure-XML helpers (`file-preview-service.ts:150-225`)
 * to the browser, replacing `adm-zip` with `jszip` (async API).
 *
 * Why @xmldom/xmldom instead of DOMParser?
 * The browser's native DOMParser auto-handles XML namespaces, but emits
 * different tag names depending on the document (e.g. `c:sld` vs `p:sld`).
 * Proma walks XLSX/PPTX trees by LOCAL name only, which native DOMParser
 * doesn't expose cleanly. @xmldom/xmldom gives us `Element.localName` + raw
 * namespace prefixes, matching Proma's tree-walking logic verbatim.
 */

import { DOMParser } from '@xmldom/xmldom'
import type JSZip from 'jszip'

export function parseXml(xml: string): Document {
  // suppress error/warning console spam from @xmldom on malformed XML
  return new DOMParser({
    errorHandler: { warning: () => {}, error: () => {}, fatalError: () => {} },
  }).parseFromString(xml, 'text/xml') as unknown as Document
}

/** All descendant elements with the given local name (namespace-agnostic). */
export function getElementsByLocalName(root: Node, localName: string): Element[] {
  const out: Element[] = []
  const walk = (node: Node) => {
    if (node.nodeType === 1 /* ELEMENT_NODE */) {
      const el = node as Element
      if (el.localName === localName) out.push(el)
    }
    for (let i = 0; i < node.childNodes.length; i++) {
      walk(node.childNodes[i]!)
    }
  }
  walk(root)
  return out
}

/** Direct children only (not grandchildren) with the given local name. */
export function getDirectChildElementsByLocalName(
  root: Element | Document,
  localName: string,
): Element[] {
  const out: Element[] = []
  for (let i = 0; i < root.childNodes.length; i++) {
    const node = root.childNodes[i]!
    if (node.nodeType === 1 && (node as Element).localName === localName) {
      out.push(node as Element)
    }
  }
  return out
}

/** Concatenated text of the first descendant element with the given local name. */
export function getFirstTextByLocalName(root: Element, localName: string): string {
  for (let i = 0; i < root.childNodes.length; i++) {
    const node = root.childNodes[i]!
    if (node.nodeType === 1 && (node as Element).localName === localName) {
      return (node.textContent ?? '').trim()
    }
  }
  // Fall through to descendant search if no direct child match.
  const found = getElementsByLocalName(root, localName)[0]
  return found ? (found.textContent ?? '').trim() : ''
}

/** Read a file inside the zip as utf-8 text. Returns null if the entry is missing. */
export async function readZipText(zip: JSZip, path: string): Promise<string | null> {
  const file = zip.file(path)
  if (!file) return null
  return file.async('string')
}

/** Normalize a relationship target relative to a base dir within the zip. */
export function normalizeZipTarget(baseDir: string, target: string): string {
  if (target.startsWith('/')) return target.slice(1)
  // ".." segments collapse against baseDir.
  const parts = `${baseDir}/${target}`.split('/')
  const stack: string[] = []
  for (const p of parts) {
    if (p === '' || p === '.') continue
    if (p === '..') stack.pop()
    else stack.push(p)
  }
  return stack.join('/')
}

/** Parse a `_rels/*.rels` file into a Map<rId, target-path>. */
export async function parseRelationships(
  zip: JSZip,
  relsPath: string,
  baseDir: string,
): Promise<Map<string, string>> {
  const out = new Map<string, string>()
  const xml = await readZipText(zip, relsPath)
  if (!xml) return out
  const doc = parseXml(xml)
  for (const rel of getElementsByLocalName(doc, 'Relationship')) {
    const id = rel.getAttribute('Id')
    const target = rel.getAttribute('Target')
    if (id && target) out.set(id, normalizeZipTarget(baseDir, target))
  }
  return out
}

/** Minimal HTML escape — same behavior as Proma's helper. */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
```

- [ ] **Step 2: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/preview/office-parsers/xml-utils.ts
git commit -m "feat(preview): xml-utils for office parsers (JSZip + @xmldom/xmldom)"
```

---

## Task 4: DOCX parser

**Files:**
- Create: `ui/src/components/preview/office-parsers/docx.ts`

Thin wrapper over mammoth's browser build. Mammoth turns DOCX → HTML in one call.

- [ ] **Step 1: Create**

```ts
/**
 * docx — Convert a DOCX file buffer to HTML using mammoth's browser build.
 *
 * Mammoth handles paragraph styles, headings, lists, tables, basic
 * bold/italic, and inline images (as base64 data URIs). Footnotes /
 * comments / complex layouts are dropped; this is a preview, not a fidelity
 * tool.
 *
 * Output: { html, messages } — messages are mammoth's warnings about
 * unsupported elements. We surface them in the renderer's banner.
 */

interface ConvertResult {
  html: string
  messages: { type: string; message: string }[]
}

export async function convertDocxToHtml(bytes: Uint8Array): Promise<ConvertResult> {
  // mammoth's browser build accepts ArrayBuffer via `arrayBuffer`.
  const mammoth = await import('mammoth/mammoth.browser')
  const result = await mammoth.convertToHtml({ arrayBuffer: bytes.buffer })
  return {
    html: result.value,
    messages: (result.messages ?? []).map((m: { type: string; message: string }) => ({
      type: m.type,
      message: m.message,
    })),
  }
}
```

If `import('mammoth/mammoth.browser')` doesn't resolve (the path is package-version-specific), fall back to plain `import('mammoth')` — mammoth's main entry auto-resolves to the browser build under Vite. Try both; check `node_modules/mammoth/package.json`'s `browser` / `exports` field to confirm.

- [ ] **Step 2: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
```

Expected: clean. If TS complains about missing types for `mammoth`, install `@types/mammoth`. As of 2024, mammoth ships its own types — should be unnecessary.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/preview/office-parsers/docx.ts
git commit -m "feat(preview): docx parser (mammoth.browser wrapper)"
```

---

## Task 5: XLSX parser

**Files:**
- Create: `ui/src/components/preview/office-parsers/xlsx.ts`

Port of Proma's `convertXlsxToHtml` (file-preview-service.ts lines 221–449). Replaces `adm-zip` with JSZip (async). Replaces `parseXml` etc. with imports from `xml-utils.ts`.

Constants from Proma (verified):
- `MAX_XLSX_SHEETS = 8`
- `MAX_XLSX_ROWS = 100`
- `MAX_XLSX_COLUMNS = 40`

- [ ] **Step 1: Reference the Proma source**

```bash
sed -n '221,450p' /Users/ryanliu/Documents/Proma/apps/electron/src/main/lib/file-preview-service.ts
```

Read the function bodies carefully. The port is mechanical — every Node-specific call has a JSZip equivalent.

- [ ] **Step 2: Create the file**

```ts
/**
 * xlsx — Convert a .xlsx file buffer to themed HTML.
 *
 * Pure-JS port of Proma v0.9.27's `convertXlsxToHtml`
 * (apps/electron/src/main/lib/file-preview-service.ts:396-449). Walks the
 * workbook XML, shared-strings table, and per-sheet rows using JSZip +
 * @xmldom/xmldom.
 *
 * Limits (match Proma):
 *   - MAX_XLSX_SHEETS = 8
 *   - MAX_XLSX_ROWS   = 100
 *   - MAX_XLSX_COLUMNS = 40
 *
 * Output HTML is class-tagged for the .office-* rules in globals.css.
 */

import JSZip from 'jszip'
import {
  escapeHtml,
  getDirectChildElementsByLocalName,
  getElementsByLocalName,
  getFirstTextByLocalName,
  parseRelationships,
  parseXml,
  readZipText,
} from './xml-utils'

const MAX_XLSX_SHEETS = 8
const MAX_XLSX_ROWS = 100
const MAX_XLSX_COLUMNS = 40

export interface XlsxResult {
  html: string
  /** Plain-text fallback (joined by `\n`) for accessibility / copy. */
  text: string
}

// ---- Shared strings -------------------------------------------------------

async function parseSharedStrings(zip: JSZip): Promise<string[]> {
  const xml = await readZipText(zip, 'xl/sharedStrings.xml')
  if (!xml) return []
  const doc = parseXml(xml)
  return getElementsByLocalName(doc, 'si').map((si) => {
    return getElementsByLocalName(si, 't')
      .map((t) => t.textContent ?? '')
      .join('')
  })
}

// ---- Date style detection -------------------------------------------------

function isDateNumFmtId(id: number): boolean {
  // Standard Excel format IDs that represent dates / times.
  return (id >= 14 && id <= 22) || (id >= 27 && id <= 36) || (id >= 45 && id <= 47)
}

function isDateFormatCode(code: string): boolean {
  const upper = code.toUpperCase()
  if (/AM\/PM|A\/P/.test(upper)) return true
  return /[YMDHS]/i.test(upper)
}

async function parseXlsxDateStyleIndexes(zip: JSZip): Promise<Set<number>> {
  const out = new Set<number>()
  const stylesXml = await readZipText(zip, 'xl/styles.xml')
  if (!stylesXml) return out
  const doc = parseXml(stylesXml)
  const customFormats = new Map<number, string>()
  for (const numFmt of getElementsByLocalName(doc, 'numFmt')) {
    const id = Number(numFmt.getAttribute('numFmtId'))
    const code = numFmt.getAttribute('formatCode') ?? ''
    if (Number.isFinite(id) && code) customFormats.set(id, code)
  }
  const cellXfs = getElementsByLocalName(doc, 'cellXfs')[0]
  if (!cellXfs) return out
  getDirectChildElementsByLocalName(cellXfs, 'xf').forEach((xf, index) => {
    const numFmtId = Number(xf.getAttribute('numFmtId'))
    if (!Number.isFinite(numFmtId)) return
    const customCode = customFormats.get(numFmtId)
    if (isDateNumFmtId(numFmtId) || (customCode && isDateFormatCode(customCode))) {
      out.add(index)
    }
  })
  return out
}

function formatExcelSerialDate(raw: string): string {
  const serial = Number(raw)
  if (!Number.isFinite(serial)) return raw
  // 25569 = days from 1900-01-00 to 1970-01-01.
  const ms = Math.round((serial - 25569) * 86400 * 1000)
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return raw
  const year = date.getUTCFullYear()
  if (year < 1900 || year > 9999) return raw
  const pad = (n: number) => String(n).padStart(2, '0')
  const datePart = `${year}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`
  const hasTime = Math.abs(serial - Math.floor(serial)) > 0.000001
  if (!hasTime) return datePart
  return `${datePart} ${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}`
}

// ---- Cell ref helpers -----------------------------------------------------

function columnIndexFromCellRef(cellRef: string): number {
  const letters = cellRef.match(/[A-Za-z]+/)?.[0]?.toUpperCase()
  if (!letters) return 0
  let index = 0
  for (const char of letters) {
    index = index * 26 + (char.charCodeAt(0) - 64)
  }
  return Math.max(0, index - 1)
}

function columnNameFromIndex(index: number): string {
  let value = index + 1
  let name = ''
  while (value > 0) {
    const rem = (value - 1) % 26
    name = String.fromCharCode(65 + rem) + name
    value = Math.floor((value - 1) / 26)
  }
  return name
}

function getXlsxCellText(
  cell: Element,
  sharedStrings: string[],
  dateStyleIndexes: Set<number>,
): string {
  const type = cell.getAttribute('t')
  if (type === 'inlineStr') {
    return getElementsByLocalName(cell, 't')
      .map((node) => node.textContent ?? '')
      .join('')
  }
  const value = getFirstTextByLocalName(cell, 'v')
  if (!value) return ''
  if (type === 's') {
    const idx = Number(value)
    return Number.isInteger(idx) ? sharedStrings[idx] ?? '' : ''
  }
  if (type === 'b') return value === '1' ? 'TRUE' : 'FALSE'
  const styleIndex = Number(cell.getAttribute('s'))
  if (!type && Number.isInteger(styleIndex) && dateStyleIndexes.has(styleIndex)) {
    return formatExcelSerialDate(value)
  }
  return value
}

// ---- Sheet rows -----------------------------------------------------------

interface SheetRows {
  rows: string[][]
  truncatedRows: boolean
  truncatedColumns: boolean
}

async function parseXlsxSheetRows(
  zip: JSZip,
  sheetPath: string,
  sharedStrings: string[],
  dateStyleIndexes: Set<number>,
): Promise<SheetRows> {
  const xml = await readZipText(zip, sheetPath)
  if (!xml) return { rows: [], truncatedRows: false, truncatedColumns: false }
  const doc = parseXml(xml)
  const rows: string[][] = []
  let truncatedRows = false
  let truncatedColumns = false

  for (const row of getElementsByLocalName(doc, 'row')) {
    if (rows.length >= MAX_XLSX_ROWS) {
      truncatedRows = true
      break
    }
    const values: string[] = []
    for (const cell of getDirectChildElementsByLocalName(row, 'c')) {
      const ref = cell.getAttribute('r') ?? ''
      const col = columnIndexFromCellRef(ref)
      if (col >= MAX_XLSX_COLUMNS) {
        truncatedColumns = true
        continue
      }
      values[col] = getXlsxCellText(cell, sharedStrings, dateStyleIndexes)
    }
    while (values.length > 0 && !values[values.length - 1]) values.pop()
    if (values.some((v) => v.trim().length > 0)) rows.push(values)
  }
  return { rows, truncatedRows, truncatedColumns }
}

// ---- HTML emission --------------------------------------------------------

function renderXlsxTable(rows: string[][]): string {
  if (rows.length === 0) {
    return '<div class="office-empty">这个工作表没有可预览的数据</div>'
  }
  const cols = Math.max(...rows.map((r) => r.length), 1)
  const headerCells = Array.from(
    { length: cols },
    (_, i) => `<th>${escapeHtml(columnNameFromIndex(i))}</th>`,
  ).join('')
  const bodyRows = rows
    .map((row, rowIdx) => {
      const cells = Array.from(
        { length: cols },
        (_, i) => `<td>${escapeHtml(row[i] ?? '')}</td>`,
      ).join('')
      return `<tr><th class="office-row-heading">${rowIdx + 1}</th>${cells}</tr>`
    })
    .join('')
  return `<div class="office-table-wrap"><table><thead><tr><th></th>${headerCells}</tr></thead><tbody>${bodyRows}</tbody></table></div>`
}

// ---- Entry point ----------------------------------------------------------

export async function convertXlsxToHtml(bytes: Uint8Array, filename: string): Promise<XlsxResult> {
  const zip = await JSZip.loadAsync(bytes)
  const workbookXml = await readZipText(zip, 'xl/workbook.xml')
  if (!workbookXml) throw new Error('Invalid XLSX: workbook.xml missing')

  const workbookDoc = parseXml(workbookXml)
  const relationships = await parseRelationships(zip, 'xl/_rels/workbook.xml.rels', 'xl')
  const sharedStrings = await parseSharedStrings(zip)
  const dateStyleIndexes = await parseXlsxDateStyleIndexes(zip)
  const sheets = getElementsByLocalName(workbookDoc, 'sheet')

  let truncatedRows = false
  let truncatedColumns = false
  const textParts: string[] = []
  const htmlParts: string[] = []

  // XML namespaces vary — get r:id OR id.
  for (const sheet of sheets.slice(0, MAX_XLSX_SHEETS)) {
    const name = sheet.getAttribute('name') || `Sheet ${htmlParts.length + 1}`
    const relId = sheet.getAttribute('r:id') ?? sheet.getAttribute('id')
    const sheetPath = relId ? relationships.get(relId) : undefined
    if (!sheetPath) continue
    const parsed = await parseXlsxSheetRows(zip, sheetPath, sharedStrings, dateStyleIndexes)
    truncatedRows ||= parsed.truncatedRows
    truncatedColumns ||= parsed.truncatedColumns
    textParts.push(`[${name}]`)
    textParts.push(...parsed.rows.map((r) => r.join('\t')))
    htmlParts.push(
      `<section class="office-sheet"><h3>${escapeHtml(name)}</h3>${renderXlsxTable(parsed.rows)}</section>`,
    )
  }

  if (htmlParts.length === 0) {
    throw new Error('Invalid XLSX: no worksheet data resolved')
  }

  const truncatedSheets = sheets.length > MAX_XLSX_SHEETS
  const notices: string[] = []
  if (truncatedSheets) notices.push(`仅显示前 ${MAX_XLSX_SHEETS} 个工作表`)
  if (truncatedRows) notices.push(`每个工作表最多显示 ${MAX_XLSX_ROWS} 行`)
  if (truncatedColumns) notices.push(`每行最多显示 ${MAX_XLSX_COLUMNS} 列`)
  const noticeHtml =
    notices.length > 0
      ? `<div class="office-preview-notice">${escapeHtml(notices.join('，'))}</div>`
      : ''

  const html = `<div class="office-preview office-preview-spreadsheet"><div class="office-preview-title">${escapeHtml(filename)}</div>${noticeHtml}${htmlParts.join('')}</div>`
  return { html, text: textParts.join('\n').trim() }
}
```

- [ ] **Step 3: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/preview/office-parsers/xlsx.ts
git commit -m "feat(preview): xlsx parser (port of Proma convertXlsxToHtml, JSZip + xmldom)"
```

---

## Task 6: PPTX parser

**Files:**
- Create: `ui/src/components/preview/office-parsers/pptx.ts`

Port of Proma's `convertPptxToHtml` (file-preview-service.ts lines 451–520).

Constant: `MAX_PPTX_SLIDES = 80`

- [ ] **Step 1: Reference**

```bash
sed -n '451,520p' /Users/ryanliu/Documents/Proma/apps/electron/src/main/lib/file-preview-service.ts
```

- [ ] **Step 2: Create the file**

```ts
/**
 * pptx — Convert a .pptx file buffer to themed HTML.
 *
 * Pure-JS port of Proma v0.9.27's `convertPptxToHtml`
 * (apps/electron/src/main/lib/file-preview-service.ts:483-520). Walks
 * the presentation.xml + per-slide XML to extract text only (no shapes,
 * no images, no transitions — it's a preview, not a Keynote replacement).
 *
 * Output HTML is class-tagged for the .office-slide / .office-empty rules
 * in globals.css.
 */

import JSZip from 'jszip'
import {
  escapeHtml,
  getElementsByLocalName,
  parseRelationships,
  parseXml,
  readZipText,
} from './xml-utils'

const MAX_PPTX_SLIDES = 80

export interface PptxResult {
  html: string
  /** Plain-text fallback (joined by \n) for accessibility / copy. */
  text: string
}

async function getPptxSlidePaths(zip: JSZip): Promise<string[]> {
  const presentationXml = await readZipText(zip, 'ppt/presentation.xml')
  const relationships = await parseRelationships(zip, 'ppt/_rels/presentation.xml.rels', 'ppt')
  if (presentationXml) {
    const doc = parseXml(presentationXml)
    const paths = getElementsByLocalName(doc, 'sldId')
      .map((s) => s.getAttribute('r:id') ?? s.getAttribute('id'))
      .map((rid) => (rid ? relationships.get(rid) : undefined))
      .filter((p): p is string => Boolean(p))
    if (paths.length > 0) return paths
  }
  // Fallback: enumerate slides directly.
  const out: string[] = []
  zip.forEach((path) => {
    if (/^ppt\/slides\/slide\d+\.xml$/.test(path)) out.push(path)
  })
  return out.sort((a, b) => {
    const numA = Number(a.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
    const numB = Number(b.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
    return numA - numB
  })
}

async function getPptxSlideText(zip: JSZip, slidePath: string): Promise<string[]> {
  const xml = await readZipText(zip, slidePath)
  if (!xml) return []
  const doc = parseXml(xml)
  return getElementsByLocalName(doc, 'p')
    .map((paragraph) =>
      getElementsByLocalName(paragraph, 't')
        .map((textNode) => textNode.textContent ?? '')
        .join('')
        .trim(),
    )
    .filter(Boolean)
}

export async function convertPptxToHtml(bytes: Uint8Array, filename: string): Promise<PptxResult> {
  const zip = await JSZip.loadAsync(bytes)
  const slidePaths = await getPptxSlidePaths(zip)
  const visible = slidePaths.slice(0, MAX_PPTX_SLIDES)

  const textParts: string[] = []
  const slideHtmlParts: string[] = []

  for (let i = 0; i < visible.length; i++) {
    const slidePath = visible[i]!
    const lines = await getPptxSlideText(zip, slidePath)
    textParts.push(`幻灯片 ${i + 1}`)
    textParts.push(...lines)
    const title = lines[0] || '（无标题）'
    const body =
      lines.length > 1
        ? `<ul>${lines.slice(1).map((line) => `<li>${escapeHtml(line)}</li>`).join('')}</ul>`
        : '<div class="office-empty">这页没有更多可提取文本</div>'
    slideHtmlParts.push(
      `<section class="office-slide"><div class="office-slide-index">幻灯片 ${i + 1}</div><h3>${escapeHtml(title)}</h3>${body}</section>`,
    )
  }

  if (slideHtmlParts.length === 0) {
    throw new Error('Invalid PPTX: no slides resolved')
  }

  const noticeHtml =
    slidePaths.length > MAX_PPTX_SLIDES
      ? `<div class="office-preview-notice">${escapeHtml(`仅显示前 ${MAX_PPTX_SLIDES} 页幻灯片`)}</div>`
      : ''

  const html = `<div class="office-preview office-preview-presentation"><div class="office-preview-title">${escapeHtml(filename)}</div>${noticeHtml}${slideHtmlParts.join('')}</div>`
  return { html, text: textParts.join('\n').trim() }
}
```

- [ ] **Step 3: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/preview/office-parsers/pptx.ts
git commit -m "feat(preview): pptx parser (port of Proma convertPptxToHtml, JSZip + xmldom)"
```

---

## Task 7: DOCX / XLSX / PPTX renderers

**Files:**
- Create: `ui/src/components/preview/renderers/DocxRenderer.tsx`
- Create: `ui/src/components/preview/renderers/XlsxRenderer.tsx`
- Create: `ui/src/components/preview/renderers/PptxRenderer.tsx`

All three follow the same shape: take `bytes` + `name`, call the parser, render the HTML inside a host div with the right class. Show loading / error states.

- [ ] **Step 1: `DocxRenderer.tsx`**

```tsx
import * as React from 'react'
import { Loader2, AlertTriangle } from 'lucide-react'

interface DocxRendererProps {
  bytes: Uint8Array
  name: string
}

export function DocxRenderer({ bytes, name }: DocxRendererProps): React.ReactElement {
  const [state, setState] = React.useState<
    { kind: 'loading' } | { kind: 'ready'; html: string } | { kind: 'error'; message: string }
  >({ kind: 'loading' })

  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    void (async () => {
      try {
        const { convertDocxToHtml } = await import('@/components/preview/office-parsers/docx')
        const result = await convertDocxToHtml(bytes)
        if (cancelled) return
        setState({ kind: 'ready', html: result.html })
      } catch (err) {
        if (cancelled) return
        setState({ kind: 'error', message: err instanceof Error ? err.message : String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes])

  if (state.kind === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
        <div className="text-[12.5px] text-foreground/70">正在转换 {name}…</div>
      </div>
    )
  }
  if (state.kind === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-destructive">DOCX 解析失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
          {state.message}
        </div>
      </div>
    )
  }
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div
        className="office-docx-host"
        // mammoth output is HTML-escaped via mammoth's own sanitization; we render it directly.
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  )
}
```

- [ ] **Step 2: `XlsxRenderer.tsx`**

```tsx
import * as React from 'react'
import { Loader2, AlertTriangle } from 'lucide-react'

interface XlsxRendererProps {
  bytes: Uint8Array
  name: string
}

export function XlsxRenderer({ bytes, name }: XlsxRendererProps): React.ReactElement {
  const [state, setState] = React.useState<
    { kind: 'loading' } | { kind: 'ready'; html: string } | { kind: 'error'; message: string }
  >({ kind: 'loading' })

  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    void (async () => {
      try {
        const { convertXlsxToHtml } = await import('@/components/preview/office-parsers/xlsx')
        const result = await convertXlsxToHtml(bytes, name)
        if (cancelled) return
        setState({ kind: 'ready', html: result.html })
      } catch (err) {
        if (cancelled) return
        setState({ kind: 'error', message: err instanceof Error ? err.message : String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes, name])

  if (state.kind === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
        <div className="text-[12.5px] text-foreground/70">正在解析 {name}…</div>
      </div>
    )
  }
  if (state.kind === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-destructive">XLSX 解析失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
          {state.message}
        </div>
      </div>
    )
  }
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div
        className="office-preview-host"
        // Output is HTML-escaped at the parser level (every cell goes through escapeHtml).
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  )
}
```

- [ ] **Step 3: `PptxRenderer.tsx`**

Same shape as XlsxRenderer but imports the pptx parser:

```tsx
import * as React from 'react'
import { Loader2, AlertTriangle } from 'lucide-react'

interface PptxRendererProps {
  bytes: Uint8Array
  name: string
}

export function PptxRenderer({ bytes, name }: PptxRendererProps): React.ReactElement {
  const [state, setState] = React.useState<
    { kind: 'loading' } | { kind: 'ready'; html: string } | { kind: 'error'; message: string }
  >({ kind: 'loading' })

  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    void (async () => {
      try {
        const { convertPptxToHtml } = await import('@/components/preview/office-parsers/pptx')
        const result = await convertPptxToHtml(bytes, name)
        if (cancelled) return
        setState({ kind: 'ready', html: result.html })
      } catch (err) {
        if (cancelled) return
        setState({ kind: 'error', message: err instanceof Error ? err.message : String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes, name])

  if (state.kind === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
        <div className="text-[12.5px] text-foreground/70">正在提取 {name} 文本…</div>
      </div>
    )
  }
  if (state.kind === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-destructive">PPTX 解析失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
          {state.message}
        </div>
      </div>
    )
  }
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div
        className="office-preview-host"
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  )
}
```

- [ ] **Step 4: Type-check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
git add ui/src/components/preview/renderers/DocxRenderer.tsx ui/src/components/preview/renderers/XlsxRenderer.tsx ui/src/components/preview/renderers/PptxRenderer.tsx
git commit -m "feat(preview): DocxRenderer + XlsxRenderer + PptxRenderer (consume office parsers)"
```

---

## Task 8: LegacyOfficeHint

**Files:**
- Create: `ui/src/components/preview/renderers/LegacyOfficeHint.tsx`

A pure presentation component — no parsing, just guidance.

- [ ] **Step 1: Create**

```tsx
import * as React from 'react'
import { FileWarning } from 'lucide-react'

interface LegacyOfficeHintProps {
  name: string
  ext: string
}

const FORMAT_LABEL: Record<string, string> = {
  doc: 'Word 97-2003 文档 (.doc)',
  xls: 'Excel 97-2003 工作簿 (.xls)',
  ppt: 'PowerPoint 97-2003 演示文稿 (.ppt)',
}

const NEW_FORMAT: Record<string, string> = {
  doc: '.docx',
  xls: '.xlsx',
  ppt: '.pptx',
}

export function LegacyOfficeHint({ name, ext }: LegacyOfficeHintProps): React.ReactElement {
  const label = FORMAT_LABEL[ext] ?? `Legacy ${ext.toUpperCase()}`
  const newFmt = NEW_FORMAT[ext] ?? '.docx / .xlsx / .pptx'
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center select-none bg-popover">
      <div className="size-14 rounded-full bg-amber-500/10 flex items-center justify-center mb-4">
        <FileWarning className="size-7 text-amber-700 dark:text-amber-300" aria-hidden />
      </div>
      <div className="text-[13px] font-medium text-foreground/85 mb-1">
        暂不支持预览此格式
      </div>
      <div className="text-[11.5px] text-muted-foreground max-w-[320px] leading-relaxed mb-3">
        {label}
      </div>
      <div className="text-[11.5px] text-muted-foreground/80 max-w-[320px] leading-relaxed">
        请使用 Microsoft Office、Pages/Numbers/Keynote 或 LibreOffice 将文件
        另存为 <span className="font-mono text-foreground/75">{newFmt}</span> 格式
        后再次预览。
      </div>
      <div
        className="mt-4 text-[10.5px] text-muted-foreground/60 font-mono tabular-nums max-w-[320px] break-words"
        title={name}
      >
        {name}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Type-check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
git add ui/src/components/preview/renderers/LegacyOfficeHint.tsx
git commit -m "feat(preview): LegacyOfficeHint for .doc / .xls / .ppt"
```

---

## Task 9: PDF renderer

**Files:**
- Create: `ui/src/components/preview/renderers/PdfRenderer.tsx`

Lazy-imports `pdfjs-dist`, renders all pages to canvases, with a zoom toolbar.

- [ ] **Step 1: Create**

```tsx
/**
 * PdfRenderer — Renders a PDF file to a stack of canvas elements via pdfjs.
 *
 * Lazy-imports pdfjs-dist so the ~6 MB worker chunk only loads when a PDF is
 * opened. Worker URL uses Vite's `?url` import to get a hashed path under
 * `assets/`.
 *
 * Zoom: discrete steps 50% → 300%. Re-renders on every step change.
 * Big PDFs (50+ pages) are still rendered eagerly — if this becomes a
 * perf issue we can virtualise per-page later.
 */

import * as React from 'react'
import { Loader2, AlertTriangle, ZoomIn, ZoomOut } from 'lucide-react'
import { cn } from '@/lib/utils'

interface PdfRendererProps {
  bytes: Uint8Array
  name: string
}

const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3] as const
const DEFAULT_STEP_IDX = 2 // 1.0x

export function PdfRenderer({ bytes, name }: PdfRendererProps): React.ReactElement {
  const containerRef = React.useRef<HTMLDivElement>(null)
  const [stepIdx, setStepIdx] = React.useState<number>(DEFAULT_STEP_IDX)
  const [state, setState] = React.useState<
    | { kind: 'loading' }
    | { kind: 'ready'; numPages: number }
    | { kind: 'error'; message: string }
  >({ kind: 'loading' })
  // Hold the loaded document across zoom changes.
  const pdfDocRef = React.useRef<PDFDocumentProxy | null>(null)

  // Load the document once on `bytes` change.
  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    pdfDocRef.current = null
    void (async () => {
      try {
        // Lazy import + worker URL.
        const pdfjs = await import('pdfjs-dist')
        // Use ?url so Vite emits a hashed worker file we can pass to pdfjs.
        const workerUrl = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default
        pdfjs.GlobalWorkerOptions.workerSrc = workerUrl

        const doc = await pdfjs.getDocument({ data: bytes }).promise
        if (cancelled) return
        pdfDocRef.current = doc as PDFDocumentProxy
        setState({ kind: 'ready', numPages: doc.numPages })
      } catch (err) {
        if (cancelled) return
        setState({
          kind: 'error',
          message: err instanceof Error ? err.message : String(err),
        })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes])

  // Re-render all pages whenever the document loads OR zoom changes.
  React.useEffect(() => {
    if (state.kind !== 'ready' || !pdfDocRef.current || !containerRef.current) return
    let cancelled = false
    const doc = pdfDocRef.current
    const container = containerRef.current
    container.innerHTML = ''

    void (async () => {
      const scale = ZOOM_STEPS[stepIdx]!
      const dpr = window.devicePixelRatio || 1
      for (let pageNum = 1; pageNum <= doc.numPages; pageNum++) {
        if (cancelled) return
        const page = await doc.getPage(pageNum)
        const viewport = page.getViewport({ scale: scale * dpr })
        const canvas = document.createElement('canvas')
        canvas.width = viewport.width
        canvas.height = viewport.height
        canvas.style.width = `${viewport.width / dpr}px`
        canvas.style.height = `${viewport.height / dpr}px`
        canvas.style.display = 'block'
        canvas.style.margin = '0 auto 12px'
        canvas.style.borderRadius = '4px'
        canvas.style.boxShadow = '0 2px 8px hsl(var(--foreground) / 0.08)'
        const ctx = canvas.getContext('2d')
        if (!ctx) continue
        await page.render({ canvasContext: ctx, viewport }).promise
        if (!cancelled) container.appendChild(canvas)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [state, stepIdx])

  const handleZoomIn = React.useCallback(() => {
    setStepIdx((idx) => Math.min(ZOOM_STEPS.length - 1, idx + 1))
  }, [])
  const handleZoomOut = React.useCallback(() => {
    setStepIdx((idx) => Math.max(0, idx - 1))
  }, [])

  return (
    <div className="flex flex-col h-full bg-popover">
      {state.kind === 'ready' && (
        <div className="flex-shrink-0 flex items-center gap-2 h-[32px] px-3 border-b border-border text-[11px] text-muted-foreground">
          <span>共 {state.numPages} 页</span>
          <span className="ml-auto" />
          <ToolbarButton aria-label="缩小" onClick={handleZoomOut} disabled={stepIdx === 0}>
            <ZoomOut size={13} />
          </ToolbarButton>
          <span className="font-mono tabular-nums text-foreground/70 min-w-[42px] text-center">
            {Math.round(ZOOM_STEPS[stepIdx]! * 100)}%
          </span>
          <ToolbarButton
            aria-label="放大"
            onClick={handleZoomIn}
            disabled={stepIdx === ZOOM_STEPS.length - 1}
          >
            <ZoomIn size={13} />
          </ToolbarButton>
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-auto p-4">
        {state.kind === 'loading' && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
            <div className="text-[12.5px] text-foreground/70">正在加载 PDF…</div>
          </div>
        )}
        {state.kind === 'error' && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
            <div className="text-[12.5px] font-medium text-destructive">PDF 加载失败</div>
            <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
              {name}：{state.message}
            </div>
          </div>
        )}
        {state.kind === 'ready' && <div ref={containerRef} />}
      </div>
    </div>
  )
}

function ToolbarButton({
  children,
  onClick,
  disabled,
  ariaLabel,
}: {
  children: React.ReactNode
  onClick: () => void
  disabled?: boolean
  ariaLabel?: string
} & { 'aria-label'?: string }): React.ReactElement {
  return (
    <button
      type="button"
      aria-label={ariaLabel}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'size-6 inline-flex items-center justify-center rounded',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        disabled
          ? 'text-foreground/25 cursor-not-allowed'
          : 'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.06]',
      )}
    >
      {children}
    </button>
  )
}

// Minimal pdfjs typing — avoid pulling in `@types/pdfjs-dist` to keep
// the dep surface small. We only use `numPages` + `getPage().getViewport()`
// + `page.render()`.
interface PDFPageViewport {
  width: number
  height: number
}
interface PDFPageProxy {
  getViewport(opts: { scale: number }): PDFPageViewport
  render(opts: { canvasContext: CanvasRenderingContext2D; viewport: PDFPageViewport }): {
    promise: Promise<void>
  }
}
interface PDFDocumentProxy {
  numPages: number
  getPage(num: number): Promise<PDFPageProxy>
}
```

The `ToolbarButton` props are slightly awkward (TS doesn't allow `aria-label` directly as a prop name without `&`). The trick used here works — `aria-label` is forwarded as an attribute via `ariaLabel`. If TS complains, simplify by just using a `title` attribute instead of `aria-label`.

- [ ] **Step 2: Type-check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -10
```

If `import('pdfjs-dist/build/pdf.worker.min.mjs?url')` fails to typecheck (Vite-specific `?url` suffix), add a global `vite-env.d.ts` declaration:

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "?url" ui/src/vite-env.d.ts | head -3
```

If `?url` types aren't declared, append to `ui/src/vite-env.d.ts`:

```ts
declare module '*?url' {
  const url: string
  export default url
}
```

```bash
git add ui/src/components/preview/renderers/PdfRenderer.tsx
# If vite-env.d.ts was modified, add it too:
git add ui/src/vite-env.d.ts
git commit -m "feat(preview): PdfRenderer (lazy pdfjs + zoom toolbar)"
```

---

## Task 10: PreviewSurface dispatch

**Files:**
- Modify: `ui/src/components/preview/PreviewSurface.tsx`

Wire the new renderers into `usePreviewRouter`'s dispatch.

- [ ] **Step 1: Read current dispatcher**

```bash
cd /Users/ryanliu/Documents/uclaw && cat ui/src/components/preview/PreviewSurface.tsx
```

The current switch covers `image` / `markdown` / `code` / `binary`. We need to extend it for the new 5 kinds.

- [ ] **Step 2: Replace the dispatch section**

Use the `Edit` tool. Find the existing `if (route.kind === 'image')` / `if (route.kind === 'markdown')` / `if (route.kind === 'code')` / final fallback block. Replace with:

```tsx
  if (route.kind === 'image') {
    return <ImageRenderer resolvedPath={state.resolvedPath} name={target.name} />
  }
  if (route.kind === 'markdown') {
    return <MarkdownRenderer text={text} />
  }
  if (route.kind === 'code') {
    return (
      <CodeRenderer
        code={text}
        language={route.language ?? 'text'}
        cacheScope={state.resolvedPath}
        refreshVersion={refreshVersion}
        truncated={state.truncated}
      />
    )
  }
  if (route.kind === 'pdf') {
    return <PdfRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'docx') {
    return <DocxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'xlsx') {
    return <XlsxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'pptx') {
    return <PptxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'legacyOffice') {
    return <LegacyOfficeHint name={target.name} ext={route.ext} />
  }
  return <BinaryFallback name={target.name} size={state.size} ext={route.ext} />
```

- [ ] **Step 3: Add imports**

Near the top of `PreviewSurface.tsx`, after the existing renderer imports, add:

```tsx
import { PdfRenderer } from './renderers/PdfRenderer'
import { DocxRenderer } from './renderers/DocxRenderer'
import { XlsxRenderer } from './renderers/XlsxRenderer'
import { PptxRenderer } from './renderers/PptxRenderer'
import { LegacyOfficeHint } from './renderers/LegacyOfficeHint'
```

- [ ] **Step 4: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean. UI test count unchanged.

- [ ] **Step 5: Pre-commit check**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/src/components/preview/PreviewSurface.tsx
```

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/preview/PreviewSurface.tsx
git commit -m "feat(preview): wire PDF / DOCX / XLSX / PPTX / legacyOffice into PreviewSurface dispatch"
```

---

## Task 11: Final verification + push + PR

- [ ] **Step 1: Full Rust + UI suites**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | tail -10
```

Expected: 391 Rust tests pass (W4b doesn't touch Rust). UI tests = baseline + 4 (new ext-classifier). `npm run build` succeeds and you can verify the chunked output contains `pdfjs-worker-*.js` and `office-parsers-*.js` files.

- [ ] **Step 2: Color audit**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -rnE 'bg-\[#|text-\[#|border-\[#|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/preview/office-parsers/ \
  ui/src/components/preview/renderers/PdfRenderer.tsx \
  ui/src/components/preview/renderers/DocxRenderer.tsx \
  ui/src/components/preview/renderers/XlsxRenderer.tsx \
  ui/src/components/preview/renderers/PptxRenderer.tsx \
  ui/src/components/preview/renderers/LegacyOfficeHint.tsx 2>/dev/null | head
```

Expected: empty (the amber color in LegacyOfficeHint uses Tailwind named colors, which are fine — only raw hex is flagged).

- [ ] **Step 3: Git log**

```bash
git log --oneline main..HEAD
```

Expected ~10 commits in order. Each task is one commit, plus the plan-write at the top.

- [ ] **Step 4: Push + open PR**

```bash
git push -u origin claude/w4b-rich-formats
gh pr create --title "W4b: Rich format renderers — PDF / DOCX / XLSX / PPTX + legacy office hint" --body "$(cat <<'EOF'
## Summary

Wave 4b of the [Proma v0.9.27 preview port](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md). Layers PDF + Office rendering on top of W4a's preview core. All parsing runs in the renderer — no new Rust.

## What lands

| Format | Renderer | Parser | Strategy |
|---|---|---|---|
| PDF | `PdfRenderer.tsx` | `pdfjs-dist` (lazy worker chunk) | Render all pages to canvas; 7-step zoom |
| DOCX | `DocxRenderer.tsx` | `mammoth/mammoth.browser` | DOCX → HTML in one call |
| XLSX | `XlsxRenderer.tsx` | `office-parsers/xlsx.ts` (port of Proma) | JSZip + xmldom: shared strings + date styles + table HTML |
| PPTX | `PptxRenderer.tsx` | `office-parsers/pptx.ts` (port of Proma) | JSZip + xmldom: extract text per slide |
| .doc / .xls / .ppt | `LegacyOfficeHint.tsx` | none | Friendly "convert to new format" guidance |

## New deps (`ui/package.json`)

- `jszip ^3.10` — read .xlsx / .pptx as zip archives
- `@xmldom/xmldom ^0.8` — local-name based XML walking (matches Proma's tree walks)
- `mammoth ^1.12` — DOCX → HTML
- `pdfjs-dist ^4` — PDF rendering

`vite.config.ts` adds `manualChunks` for `pdfjs-worker` and `office-parsers` so the heavy bits split out of the main bundle. The PDF worker only loads when a user opens a PDF.

## Theme integration

Office output uses Proma's CSS class names (`.office-preview`, `.office-table-wrap`, `.office-sheet`, `.office-slide`, etc.) — those rules are ported into `ui/src/styles/globals.css` using uClaw's existing theme tokens (`hsl(var(--foreground))`, `hsl(var(--border))`, etc.) so every theme renders consistently.

## Commits (bisectable)

11 commits — one per plan task. See `git log main..HEAD`.

## Test plan

- [x] `cd ui && npx tsc --noEmit` — clean
- [x] `cd ui && npm test -- --run` — passes (+4 new ext-classifier tests)
- [x] `cd ui && npm run build` — chunks emitted: `pdfjs-worker-*.js`, `office-parsers-*.js`
- [x] `cd src-tauri && cargo test --lib` — 391 pass (no Rust changes)
- [x] No hardcoded colors in new files
- [ ] Manual: click a `.pdf` → loads, zoom toolbar works, all pages render
- [ ] Manual: click a `.docx` → renders as formatted HTML (headings, paragraphs, lists, tables)
- [ ] Manual: click a `.xlsx` with 2+ sheets → all sheets rendered as tables; rows/cols truncation banner appears if over limits
- [ ] Manual: click a `.pptx` → slide text extracted, slide-by-slide layout
- [ ] Manual: click a `.doc` / `.xls` / `.ppt` → LegacyOfficeHint with conversion guidance
- [ ] Manual: 11-theme spot-check — XLSX tables / PPTX slides adapt; PDF chrome adapts
- [ ] Manual: large XLSX (>10 sheets, >500 rows) — truncation banner shows correctly

## What's out of scope (deferred to W4c)

- Inline editing for any format (DOCX/XLSX/PPTX are explicitly preview-only — W4 spec)
- File-path chips in agent messages + remark plugin
- Restore Shift-click "add to chat" from rail

## Branch base

Branched from main at `8c5df57` (post-W4a-followup). If polish PR #109 merges first, this branch rebases cleanly — they touch different files (#109 = chrome polish, W4b = new renderers).
EOF
)"
```

---

## Self-Review

Spec-coverage sweep (§6 rich-formats subset):

| Spec | Task |
|---|---|
| §6.4 Office parsing: pure-renderer approach | Tasks 3–7 |
| §6.4 XLSX/PPTX constants (8/100/40, 80 slides) | Tasks 5, 6 |
| §6.5 PDF strategy: lazy pdfjs worker | Task 9 + Task 1 (vite chunk) |
| §6.6 Edit scope = M1 (read-only for DOCX/XLSX/PPTX/PDF) | All renderers — no write path |
| §6.6 Legacy office hint | Task 8 |
| §6 Office CSS theme-token port | Task 2 |

Type-consistency check:
- `XlsxResult`, `PptxResult` — defined in their respective parser files; consumed by their renderers only. No cross-task naming drift.
- `LegacyOfficeHintProps` takes `{ name, ext }` — `route.kind === 'legacyOffice'` passes `target.name` + `route.ext`, matches.
- `PdfRendererProps`, `DocxRendererProps`, `XlsxRendererProps`, `PptxRendererProps` — all take `{ bytes: Uint8Array, name: string }`. `state.bytes` from `useFileBytes` is `Uint8Array`. Consistent.
- `RendererKind` extended in Task 1; consumed in Task 10. Strings match exactly.

Placeholder scan: none — every step contains real code.

Module-size cap: every file ≤ 250 LOC target. Largest is `xlsx.ts` at ~220.

Known risks:
1. **Mammoth's browser entry path** may differ by version. Task 4 has a fallback (`import('mammoth')` instead of `import('mammoth/mammoth.browser')`) — verify at install time.
2. **pdfjs `?url` import** requires a Vite dev type. Task 9 covers the `vite-env.d.ts` declaration if missing.
3. **DOMParser in @xmldom/xmldom** vs native browser DOMParser have slightly different attribute behavior on namespaced attributes (`r:id` vs `id`). Tasks 5 + 6 explicitly check both via `getAttribute('r:id') ?? getAttribute('id')` — matches Proma's defensive pattern.
4. **Sanitization**: mammoth's HTML is trusted (it's a known sanitizer). xlsx/pptx output is escaped at the cell level via the parser's `escapeHtml`. DocxRenderer / XlsxRenderer / PptxRenderer use `dangerouslySetInnerHTML` knowingly.
