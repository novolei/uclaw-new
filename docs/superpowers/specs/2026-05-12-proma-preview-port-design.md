# Proma v0.9.27 Preview & Files Port — Design

**Date**: 2026-05-12
**Status**: Brainstorming complete, ready for plans
**Owner**: Ryan
**Sources**:
- Proma v0.9.27 tag `80e21f8` on github.com/ErlichLiu/Proma
- 8 underlying PRs: #408 #409 #414 #415 #416 #417 #419 #420
- uClaw `main` @ commit `07fc793`

---

## 0. Background

Port Proma v0.9.27's preview + agent-IO improvements into uClaw, with three additional uClaw-specific requirements that emerged during brainstorming:

1. **Files-tab in the right rail** with live filesystem refresh (notify-based)
2. **Multi-format preview with inline editing** (not just rendering)
3. **File-path chips** in agent message renderer that open the preview on click

### Architecture delta

| Layer | Proma | uClaw |
|---|---|---|
| Shell | Electron + Node main process | Tauri v2 + Rust |
| Multi-window | `BrowserWindow` | `WebviewWindow` |
| File I/O | Node `fs`/`path` direct | Tauri command + Rust |
| Custom protocol | `proma-file://` via `protocol.handle` | `assetProtocol` scope `**` (already enabled in `src-tauri/tauri.conf.json`) |
| Office parsing | `mammoth` + `adm-zip` + `@xmldom/xmldom` in main | **Pure renderer** (jszip + @xmldom/xmldom + mammoth.browser) — decision in §6.4 |
| Agent loop retry | `agent-orchestrator.ts` JS | `agent/agentic_loop.rs` + `llm/stream_error.rs` |

**Implication**: no file-level copy from Proma is possible end-to-end. JS parsing logic for XLSX/PPTX can be directly transliterated since the npm packages (`jszip`, `@xmldom/xmldom`) are browser-friendly. Everything else is reference, not source.

### v0.9.27 PR → wave mapping

| PR | Title | Commit | Lands in |
|---|---|---|---|
| #408 | Sidebar drag strip | `fe93553` | W1 |
| #409 | File preview refresh | `be74e4e` | W1 (mechanism) + W4 (consumer) |
| #414 | Office file previews | `ff9fe01` | W4 |
| #415 | Long-text paste → attachment | `d3b5e75` | W1 |
| #416 | TypeScript highlight cache | `2103451` | W1 (cache) + W4 (consumer) |
| #417 | Detached preview window | `07bf0a7` | W5 |
| #419 | Agent retry budget (5min) | `4161e54` | W2 |
| #420 | Migration backup warnings | `f68fe0c` | W2 (conditional — see §4.2) |

---

## 1. Phasing — 5 Waves

Each wave is one branch / one PR / one bisectable commit per task.

| Wave | Theme | Risk | Est. commits | Tauri-cmd registrations | New deps |
|---|---|---|---|---|---|
| **W1** | Renderer quick wins (cache + paste + drag + refresh atom) | Low | 4–5 | 0 | 0 |
| **W2** | Agent retry budget (+ optional backup tolerance) | Med | 2–4 | 0 | 0 |
| **W3** | Files Rail v2 (live tree, attached dirs, tabs) | Med | 6–8 | 4 | 0 |
| **W4** | Preview Engine v2 (multi-format + edit + chips) | High | 8–12 | 2–3 | jszip, @xmldom/xmldom, mammoth, pdfjs-dist, @react-symbols/icons, @codemirror/* or monaco, @tiptap/* or md-editor |
| **W5** | Detached preview window | High | 5–7 | 4 | (none — uses already-present Tauri webview API) |

**Bisectability rule**: one commit per implementation plan task. Match PR shape of #29 / #31 / #33 / #35 / #36 (each PR has a `## Commits (bisectable)` table).

---

## 2. Cross-Cutting Constraints

These apply to every wave.

### 2.1 Module size (hard rule from user)

- Single `.tsx` / `.ts` / `.rs` file: **≤ 300 lines target**, **≤ 400 lines hard cap**
- Component entry files: ≤ 200 lines; logic splits into `hooks/`, `renderers/`, `utils/`
- No "god file" — if approaching the cap during implementation, **stop and split first** before adding more
- Each feature lives in its own directory (`components/preview/`, `components/files-rail/`, `src-tauri/src/preview/`)

### 2.2 UI/UX design language

Senior-designer-grade fit-and-finish. Each W3/W4/W5 PR must pass this checklist before being marked ready:

- [ ] 11 themes verified (warm-paper, qingye, forest-*, etc.) — no hardcoded `#abc` / `bg-zinc-*` / `text-gray-*`
- [ ] System dark + `prefers-reduced-motion` verified — no broken layout, no jarring transitions
- [ ] 5 interaction states (default / hover / focus-visible / active / disabled) implemented for every interactive element
- [ ] 3 state-template (empty / loading / error) implemented for every panel
- [ ] Keyboard reachable, focus ring visible
- [ ] Wraps/truncates correctly at 320px width
- [ ] `aria-label` / `role` set
- [ ] grep for `bg-\[#` / `text-\[#` returns empty for new code

#### Visual rhythm

- Spacing grid: 4 / 8 / 12 / 16 / 24 / 32 (Tailwind steps only)
- Radii: `rounded-md` chips/buttons, `rounded-lg` panels, `rounded-xl` modals
- Borders: 1px default, `border-border` token; `border-2` reserved for focus

#### Color tokens

All colors via theme variables: `bg-popover`, `text-foreground`, `text-muted-foreground`, `bg-muted`, `border-border`, `ring-ring`, `--success`, `--warning`, `--destructive`. File-type colors live in `ui/src/components/preview/chips/file-type-colors.ts` as the single source of truth.

#### Motion

- Panel slide-in: `transition-transform duration-200 ease-out`
- Hover/focus: `transition-colors duration-150`
- Never `transition-all`
- `motion-reduce:transition-none` honored throughout

#### Typography

- `font-mono tabular-nums` for paths/sizes/line numbers
- Path truncation with `dir="rtl"` to preserve filename end

#### Iconography

- Primary library: `lucide-react` (already in uClaw)
- File-type icons: `@react-symbols/icons` (W4 new dep) — only inside `FileTypeIcon` / `FilePathChip`, not elsewhere
- Toast: `sonner` (already in uClaw)

#### Keyboard

- `Cmd/Ctrl+B`: toggle FilesRail
- `Cmd/Ctrl+Shift+P`: toggle Preview
- `Cmd/Ctrl+S`: save edits
- `Esc`: close preview (or focus chat)
- Tree: `↑/↓` move, `→/←` expand/collapse, `Enter` open, `Shift+Enter` edit

#### Responsive

- < 1100px: FilesRail folds to icon-only / drawer
- < 1280px: Preview switches to full-window sheet (no triple-column)
- > 1800px: Preview width caps at 800px, extra space goes to chat

### 2.3 Registration discipline (CLAUDE.md restated)

- Every new Tauri command: defined in `tauri_commands.rs` **AND** registered in `main.rs invoke_handler!`. Compile-clean but runtime-fail if missed.
- Every new background service: registered in `main.rs` Stage 3 block.
- Mention in commit body so reviewer doesn't flag as scope creep.

### 2.4 Migration version pinning

W3 + W4 may need new SQLite columns (e.g. preview width per session, recent-files list). Before claiming a `V` number, check the **Active migration registry** in `CLAUDE.md` (currently V17 is claimed by open PR `claude/workspace-phase2`). Next free → V18+.

---

## 3. W1 — Renderer Quick Wins

Pure `ui/` changes, 0 Rust touched, 0 new deps. Establishes infrastructure (cache + refresh atom) that W4 will consume.

### 3.1 Files

```
ui/src/
├── lib/clipboard-attachment.ts            (~70 lines, new)
├── lib/clipboard-attachment.test.ts       (new)
├── components/preview/codeHighlightCache.ts        (~80 lines, new)
├── components/preview/codeHighlightCache.test.ts   (new)
├── hooks/usePreviewRefresh.ts             (~50 lines, new)
├── atoms/previewAtoms.ts                  (+20 lines)
├── components/chat/ChatInput.tsx          (+20 lines)
├── components/app-shell/<LeftSidebar>.tsx (+15 lines; exact name resolved during plan)
└── styles/globals.css                     (+10 lines)
```

### 3.2 Decisions

- **Highlight cache**: LRU Map, max 50 entries, key `gitRoot:filePath:refreshVersion`, stores `{ oldContent, newContent, highlightedHtml, highlightedLanguage, highlightedTheme }`. `MAX_HIGHLIGHT_CHARS = 200_000` → render plain `<pre>` if exceeded.
- **Refresh atom**: `atomFamily<filePath, number>` bumped on (a) agent file-write event, (b) `tauri://focus`, (c) manual trigger. W4 will additionally bump on (d) files_rail change events.
- **Paste threshold**: 500 chars (matches Proma exactly). MD detection: 8 regex patterns (Proma's set). Filename: `clipboard-YYYYMMDD-HHMMSS.{md|txt}`. MIME: `text/markdown` vs `text/plain`.
- **Drag strip**: `28px` height top region with `data-tauri-drag-region`, present in both collapsed and expanded sidebar states.

### 3.3 Commits

1. `feat(preview): add code highlight cache module`
2. `feat(preview): add usePreviewRefresh hook + refreshVersion atom`
3. `feat(chat): paste long text as attachment`
4. `feat(app-shell): add sidebar top drag strip`
5. `test: cover W1 modules`

### 3.4 Verification

```
cd ui && npx tsc --noEmit
cd ui && npm test -- --run
```
Manual: paste ≥500 chars → toast + attachment; collapse sidebar → top still draggable; agent edits file → cached preview key invalidates (cache miss next read).

---

## 4. W2 — Agent Backend

### 4.1 Retry budget extension (PR #419 transliterated to Rust)

Numbers from Proma diff:

| Constant | New value |
|---|---|
| `MAX_AUTO_RETRIES` | 25 |
| `MAX_AUTO_RETRY_WAIT_MS` | 300_000 (5 min) |
| `RETRY_MAX_DELAY_MS` | 15_000 |
| Backoff base | `min(1000 * 2^(attempt-1), 15_000)` |
| Jitter | ±20% |

#### Modules

```
src-tauri/src/agent/retry/
├── mod.rs              (~40 lines)
├── budget.rs           (~120 lines)
├── backoff.rs          (~80 lines)
└── tests.rs            (~150 lines)
```

#### API

```rust
pub struct RetryBudget { /* internal */ }
impl RetryBudget {
    pub fn for_agent_loop() -> Self;       // 25 / 5min
    pub fn next_delay(&mut self) -> Option<Duration>;  // None = exhausted
    pub fn attempts(&self) -> u32;
    pub fn elapsed_wait(&self) -> Duration;
}
```

#### Wiring

In `agent/agentic_loop.rs`: when `classify_stream_error` returns retryable, consult `budget.next_delay()`. Sleep with `tokio::select!` between the duration and the session-abort signal channel.

Emit IPC event `agent:retry` for every retry: `{ type: 'retry', status: 'starting'|'attempt'|'exhausted', attempt, maxAttempts, delaySeconds, reason }`. UI consumption deferred to W4.

#### Coexistence with existing timeouts

- `STREAM_STALL_TIMEOUT=45s` is **per-chunk**; on stall, classify_stream_error decides retryable → budget governs.
- `COMPLETE_TIMEOUT=120s` is **per-attempt overall**; on hit, same classification.
- `MAX_AUTO_RETRY_WAIT_MS=5min` is **cumulative across all retries** (sum of sleeps, not sum of attempt durations).

Spec note: budget tracks sleep time, not attempt time. So actual wall clock can exceed 5 min if individual attempts run long — accepted (matches Proma).

### 4.2 Migration backup warnings (conditional)

W2 task 0: grep uClaw for backup/export functionality.

```bash
grep -rln "backup\|export" /Users/ryanliu/Documents/uclaw/src-tauri/src/ \
  | grep -v test | grep -v target
```

- **If exists** → mirror Proma PR #420: replace `?`-propagated errors during directory walk with `Result<Vec<Entry>, Vec<SkippedEntry>>`. Surface warnings to UI via toast list. Module: `<existing>/walker.rs`.
- **If absent** → **drop this scope from W2**. Spec records "YAGNI: uClaw has no backup feature; not building one to match Proma." Update spec § 4.2 accordingly during plan.

### 4.3 Commits

1. `feat(agent): add retry budget + backoff modules`
2. `feat(agent): wire retry budget into agentic_loop with 5min/25-attempt window`
3. `feat(agent): emit agent:retry IPC events`
4. (conditional) `fix(backup): tolerate unreadable entries with warnings`

### 4.4 Verification

```
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::retry
```
Manual: mock 429 provider → 25 retries over ~5 min then fail; abort mid-retry → stops immediately; devtools shows `agent:retry` event stream.

---

## 5. W3 — Files Rail v2

Right-rail file panel: workspace files + attached dirs, two tabs (workspace / changes), live notify-based refresh.

### 5.1 Backend modules

```
src-tauri/src/files_rail/
├── mod.rs              (~40)
├── types.rs            (~80)   — FileNode, MountRoot, NodeKind, MountKind
├── walker.rs           (~150)  — single-layer dir read + ignore + sort
├── ignore.rs           (~80)   — SKIP_DIRS + .gitignore read
├── watcher.rs          (~200)  — notify watcher + path→mount reverse lookup
├── service.rs          (~150)  — FilesRailService (ServiceManager trait)
├── commands.rs         (~120)
└── tests.rs            (~200)
```

Total ≤ 1020 lines across 8 files. Max single file 200.

### 5.2 Tauri commands

```
files_rail_list_mounts(session_id?) -> Vec<MountRoot>
files_rail_read_dir(mount_id, rel_path) -> Vec<FileNode>
files_rail_watch_start(mount_id) -> ()
files_rail_watch_stop(mount_id) -> ()
```

Plus IPC event `files_rail:change` batched 16ms or 100 events.

### 5.3 Frontend modules

```
ui/src/components/files-rail/
├── index.tsx                    (~120)
├── FilesRailTabs.tsx            (~60)
├── workspace/
│   ├── WorkspaceFilesPanel.tsx  (~200)
│   ├── MountSection.tsx         (~150)
│   ├── FileTreeNode.tsx         (~180)
│   ├── AddFileButton.tsx        (~50)
│   └── AttachDirButton.tsx      (~60)
├── changes/
│   ├── FileChangesPanel.tsx     (~200)
│   └── ChangeRow.tsx            (~80)
├── hooks/
│   ├── useFilesRail.ts          (~150)
│   ├── useFileTree.ts           (~120)
│   ├── useFilesRailWatcher.ts   (~100)
│   └── useDirectoryActions.ts   (~80)
└── utils/
    ├── tree-patch.ts            (~120)  — apply change events to in-memory tree
    └── tree-patch.test.ts
```

### 5.4 Performance decision

- Lazy tree: load one level at a time on expand; collapse does **not** discard expansion state — re-expand restores from cache without re-fetching.
- `tree-patch.ts` applies notify events incrementally; **unexpanded directories ignore child changes** (re-fetched lazily on next expand). This is uClaw-specific (Proma doesn't optimize this).
- ignore rules: SKIP_DIRS = `node_modules .git dist .next __pycache__ .venv build .cache target` + `.DS_Store`.

### 5.5 Integration

- W3 mounts `<FilesRail />` as always-on right column in `AgentView` (or equivalent shell).
- Click → emits `selectFileForPreview` Jotai action atom (W4 consumes).
- Right-click menu reuses existing `rename_attached_file`/`move_attached_file`/`delete_workspace_file` commands.

### 5.6 Commits

1. `feat(files-rail): data types + walker + ignore module`
2. `feat(files-rail): notify-based watcher service`
3. `feat(files-rail): Tauri commands + invoke_handler registration`
4. `feat(ui/files-rail): scaffold panel + tabs + workspace tree`
5. `feat(ui/files-rail): integrate watcher events with tree-patch`
6. `feat(ui/files-rail): attached-dir / add-file actions`
7. `feat(ui/files-rail): file-changes tab`
8. `test: cover files-rail Rust + UI`

### 5.7 Verification

```
cd src-tauri && cargo build && cargo test --lib files_rail
cd ui && npx tsc --noEmit && npm test -- --run files-rail
```
Manual: `touch` new file → appears in 1s; expand 10k+-file dir → no UI freeze; attached dir appears as separate section.

---

## 6. W4 — Preview Engine v2

The largest wave. Multi-format preview + inline editing + file chips. Sits between FilesRail and chat as a slide-in panel.

### 6.1 Frontend modules

```
ui/src/components/preview/
├── PreviewPanel.tsx              (~180)
├── PreviewHeader.tsx             (~120)
├── PreviewSurface.tsx            (~150)
├── PreviewEmpty.tsx              (~50)
├── renderers/
│   ├── CodeRenderer.tsx          (~180)
│   ├── MarkdownRenderer.tsx      (~150)
│   ├── ImageRenderer.tsx         (~80)
│   ├── PdfRenderer.tsx           (~140)
│   ├── DocxRenderer.tsx          (~120)
│   ├── XlsxRenderer.tsx          (~200)
│   ├── PptxRenderer.tsx          (~180)
│   ├── DiffRenderer.tsx          (~180)
│   ├── LegacyOfficeHint.tsx      (~60)
│   └── BinaryFallback.tsx        (~60)
├── editors/
│   ├── TextEditor.tsx            (~200)   — CodeMirror 6
│   ├── MarkdownRichEditor.tsx    (~240)   — TipTap
│   ├── EditorToolbar.tsx         (~80)
│   └── useDirtyBuffer.ts         (~80)
├── chips/
│   ├── FilePathChip.tsx          (~140)
│   ├── useFileChipResolver.ts    (~120)
│   ├── line-col-parser.ts        (~40)
│   ├── file-type-colors.ts       (~80)
│   └── markdownFileChipPlugin.ts (~120)
├── office-parsers/
│   ├── xlsx.ts                   (~200)
│   ├── pptx.ts                   (~150)
│   ├── docx.ts                   (~50)
│   └── xml-utils.ts              (~80)
├── hooks/
│   ├── useFileBytes.ts           (~80)
│   ├── usePreviewState.ts        (~120)
│   ├── usePreviewRouter.ts       (~100)
│   └── useEditorPersistence.ts   (~120)
└── utils/
    ├── ext-classifier.ts         (~80)
    ├── ext-classifier.test.ts
    └── file-size-formatter.ts    (~30)
```

~32 files, ~3400 lines, max single file 240.

### 6.2 Backend modules

```
src-tauri/src/preview/
├── mod.rs
├── commands.rs       (~150)  — preview_read_bytes, preview_write_text
├── resolver.rs       (~120)  — port of Proma resolveTargetPath
└── tests.rs
```

### 6.3 Tauri commands

```
preview_read_bytes(path, max_bytes?) -> PreviewBytes { bytes, size, truncated, mtime_ms }
preview_write_text(path, content, expected_mtime_ms) -> WriteResult::Saved{mtime}|::Conflict{mtime}
```

50 MB hard cap on read. Write goes through `SafetyManager`:
- Inside workspace → allow
- Inside attached_dirs with `editable=true` → allow
- Otherwise → reject; require manual approval via existing pending_approvals flow

### 6.4 Office parsing — pure renderer

Decision (user-chosen): mirror Proma's TS logic into renderer modules using browser-friendly packages.

New deps:
- `jszip` (replaces `adm-zip`)
- `@xmldom/xmldom` (same package as Proma)
- `mammoth` (browser build)
- `pdfjs-dist` (worker as separate chunk)

Constants from Proma `file-preview-service.ts` (transliterated exactly):
- `MAX_XLSX_SHEETS = 20`, `MAX_XLSX_ROWS = 500`, `MAX_XLSX_COLUMNS = 50`
- `MAX_PPTX_SLIDES = 100`

(Confirm exact values during W4 task 1; spec to be updated if Proma values differ.)

CSS adaptation: Proma's `office.css` (124 lines, hardcoded grays) is **re-authored** with theme tokens before merging. No hex left.

### 6.5 PDF strategy

uClaw's `assetProtocol` scope `**` is already enabled — no protocol shim needed. `pdfjs-dist` worker is lazy-loaded via `import()` and split out in `vite.config.ts`:

```ts
manualChunks: {
  'pdfjs-worker': ['pdfjs-dist/build/pdf.worker.min.mjs'],
  'office-parsers': ['jszip', '@xmldom/xmldom', 'mammoth'],
}
```

Pass `useFileBytes` result as `Uint8Array` to `getDocument({ data: bytes })`.

Zoom levels (mirror Proma): `[0.5, 0.75, 1, 1.25, 1.5, 2, 3]`.

### 6.6 Editing scope — M1

| Format | Read | Edit |
|---|---|---|
| txt / log / csv / ini / env | ✓ | ✓ (TextEditor) |
| CODE_EXTS (ts/js/py/rs/go/...) | ✓ | ✓ (TextEditor) |
| md / markdown | ✓ | ✓ (TextEditor raw + TipTap rich, toggle) |
| docx | ✓ | ✗ M2 |
| xlsx | ✓ | ✗ M2 |
| pptx | ✓ | ✗ M2 |
| doc/xls/ppt | hint only | ✗ |
| pdf | ✓ | ✗ |
| image | ✓ | ✗ |
| diff | ✓ | ✗ |

Save (`Cmd+S` / `Ctrl+S`) calls `preview_write_text`. If `expected_mtime_ms` mismatches actual (external change while editing), surface "File changed externally — Overwrite / Discard / View diff".

Save does **not** bump `refreshVersion` (avoids self-flush of dirty state).

`useDirtyBuffer` intercepts: file switch / panel close / window close → "Unsaved changes — Save / Discard / Cancel".

### 6.7 PreviewPanel layout

Slide-in from chat area's right edge, **does not overlap FilesRail**.

- Width: `42% min 420 max 800`, draggable handle, persisted to `previewWidthAtom` (`atomWithStorage`)
- Z-order: chat < preview < modal
- Header: filename + truncated path + actions (refresh, pop-out [disabled until W5], close)
- Body: `<PreviewSurface />` via `usePreviewRouter`
- Esc closes; focus returns to chat input

### 6.8 File-path chips

Renders inside agent message text whenever a file reference is detected. Strategy:

- Implementation: **remark plugin** transforms AST text/link nodes → custom chip nodes
- Triggers (need at least one):
  - Markdown link `[label](path.ext)` where `ext ∈ ALL_PREVIEWABLE_EXTS`
  - Inline code containing a recognizable filename: `` `style.css` ``
  - Path-like token with at least one `/` and recognizable extension: `src/main.rs`
- Skipped: bare bare-words like `style.css` (too ambiguous); inside code fences
- Line/col suffix `:42` / `:42:15` stripped via `stripLineCol`; preserved for jump-to-line in W4 task editing

Click → `openPreviewAction` Jotai write atom → preview panel opens with that file.

Existence check is async (cached map), driven by `useFileChipResolver`. Broken chips render with reduced opacity + tooltip, **still clickable** (lets user see broken state instead of silent).

Visual:
```
[</> index.html]   ← orange/HTML
[ JS sounds.js]   ← yellow/JS
[{} style.css]    ← blue/CSS
```

Colors live in `file-type-colors.ts`. Icons from `@react-symbols/icons` (autoAssign by filename). Chip itself: `rounded-md bg-muted/60 hover:bg-muted h-[22px] px-1.5 inline-flex items-center gap-1`.

### 6.9 Refresh integration

`useFileBytes(path)` depends on `usePreviewRefresh(path)` version. Bumped by:
1. Agent file-write IPC event
2. FilesRail change events (W3) where `mountId/path` matches
3. `tauri://focus`
4. Manual refresh button in header

### 6.10 Commits

1. `feat(preview): scaffold panel + router + read_bytes command`
2. `feat(preview): CodeRenderer + highlight-cache integration`
3. `feat(preview): MarkdownRenderer + ImageRenderer`
4. `feat(preview): PdfRenderer (lazy pdfjs worker)`
5. `feat(preview): Office parsers (xlsx/pptx/docx)`
6. `feat(preview): Office renderers + legacy-format hint`
7. `feat(preview): DiffRenderer + file-changes tab integration`
8. `feat(preview): TextEditor (CodeMirror) + write_text command`
9. `feat(preview): MarkdownRichEditor + dirty buffer`
10. `feat(preview): FilePathChip + remark plugin`
11. `feat(preview): wire chips into agent message renderer`
12. `test: cover preview modules`

### 6.11 Verification

```
cd src-tauri && cargo build && cargo test --lib preview
cd ui && npx tsc --noEmit && npm test -- --run preview
```

Manual coverage:
- code/md/image/pdf/docx/xlsx/pptx render correctly
- 200k+ chars file: highlight skipped, raw renders fine
- xlsx multi-sheet, pptx multi-slide, doc/xls/ppt → legacy hint
- agent writes file → preview auto-reloads
- edit + Cmd+S → saved; external change → conflict modal
- close while dirty → guard prompt
- chip click in agent message → preview slides in with that file
- chip inside code fence → not transformed
- chip with `:42` suffix → opens preview scrolled to line 42

---

## 7. W5 — Detached Preview Window

Pop preview out into a standalone `WebviewWindow`.

### 7.1 Backend modules

```
src-tauri/src/detached_preview/
├── mod.rs              (~30)
├── types.rs            (~80)
├── registry.rs         (~150)  — id↔data, id↔label, signature↔id
├── window_factory.rs   (~200)  — WebviewWindowBuilder + bounds + URL
├── commands.rs         (~120)
└── tests.rs            (~120)
```

### 7.2 Tauri commands

```
detached_preview_open(input) -> previewId
detached_preview_get_data(preview_id) -> DetachedPreviewData
detached_preview_close(preview_id) -> ()
detached_preview_open_external(url) -> ()
```

### 7.3 Capability isolation

New `src-tauri/capabilities/detached-preview.json` with allowlist:
- `core:webview:allow-close`, `core:window:allow-show/hide/unminimize/set-title`
- `event:allow-listen`
- `detached_preview_get_data`, `detached_preview_open_external`
- `preview_read_bytes` (read-only)
- **NOT** `preview_write_text` — detached windows are read-only

Capability matches `windows: ["preview_*"]` (label prefix).

### 7.4 Window factory

- Dedup by signature: `serde_json::to_string({session_id, file_path, dir_path, git_root, preview_only, base_paths})`
- Existing window → `unminimize + show + set_focus + return existing id`
- New window: `WebviewWindowBuilder` with label `preview_<id_sanitized>`, `visible(false)`, position centered on monitor matching source window
- Front-end emits `detached_preview:ready` after first effect → Rust calls `show + set_focus` (avoids white flash)

### 7.5 Frontend modules

```
ui/src/components/preview/detached/
├── DetachedPreviewApp.tsx       (~150)
├── DetachedShell.tsx            (~100)
├── useDetachedBoot.ts           (~120)
└── DetachedFatalError.tsx       (~50)
```

URL routing in `ui/src/main.tsx`:
```ts
const kind = new URLSearchParams(location.search).get('window')
if (kind === 'detached-preview') {
  root.render(<DetachedPreviewApp />)
} else {
  root.render(<App />)
}
```

`DetachedPreviewApp` reuses W4's `<PreviewSurface />` and a compact `<PreviewHeader />`. No FilesRail / Chat / SidePanel.

### 7.6 Theme sync — option C (URL init + IPC updates)

- `detached_preview_open` accepts `theme: string` → passed in URL `?theme=warm-paper`
- `DetachedShell` reads URL on boot, applies theme to body class
- Subscribes to `theme:changed` IPC event for runtime updates (main window broadcasts when user switches theme)

Eliminates first-frame flash and keeps detached in sync.

### 7.7 External links

Global `click` capture in `DetachedShell`: any `<a href="http(s)://...">` → `preventDefault` + `invoke('detached_preview_open_external', { url })` → `tauri_plugin_shell::ShellExt::open`.

### 7.8 Keyboard

- `Esc` → `window.close()`
- `Cmd+W` / `Ctrl+W` → `window.close()`

### 7.9 Main-window UX contract

Clicking pop-out button in main PreviewPanel:
1. Open detached window for current file
2. **Close the main inline preview** (avoid two-view confusion)
3. Re-clicking the file later reopens inline

Editing remains exclusive to the main window (detached is read-only — capability enforced).

### 7.10 Commits

1. `feat(detached-preview): registry + types + signature dedup`
2. `feat(detached-preview): window factory + Tauri commands`
3. `feat(ui): detached preview app entrypoint + URL routing`
4. `feat(ui): theme propagation via URL + theme:changed event`
5. `feat(ui): wire pop-out button in PreviewPanel header`
6. `feat(detached-preview): isolated capability file`
7. `test: detached preview registry + window factory`

### 7.11 Verification

`cargo test --lib detached_preview` + manual:
- Same file double-popped → focus existing, no second window
- Different files → multiple windows coexist
- Theme switch in main → detached follows
- Close main → detached survives
- Cmd+W / Esc closes
- External link → system browser
- Pop-out hides main inline preview (UX contract)
- detached window: no save button visible; capability blocks write_text invoke

---

## 8. Open Questions

These need decisions during plan-writing or first wave:

1. **W2 backup feature existence**: confirm via grep, then either implement or YAGNI-out.
2. **Diff library selection** (W4 task 7): `@pierre/diffs` (Proma's, license TBD), `react-diff-view`, or hand-rolled with `diff` (npm). Decision before writing W4 plan.
3. **Editor selection** (W4 task 8): CodeMirror 6 vs Monaco. CodeMirror is lighter (~150KB) and lazy-loadable per-language; Monaco gives VSCode parity (~1.5MB). Default proposal: **CodeMirror 6** unless a strong reason emerges.
4. **Rich markdown editor** (W4 task 9): TipTap (Proma's, full extension ecosystem) vs `@uiw/react-md-editor` (simpler, less malleable). Default proposal: **TipTap**.
5. **Filesystem watcher cross-platform footprint**: notify v7 with `macos_kqueue` feature is set in uClaw Cargo.toml. Linux uses inotify, Windows uses ReadDirectoryChangesW automatically. Spec assumes no platform-specific code needed; verify during W3.
6. **Theme broadcast mechanism**: confirm uClaw already has a `theme:changed` IPC event or equivalent. If not, W5 adds it.

---

## 9. Out of Scope

- Office file *editing* (docx/xlsx/pptx round-trip writing) — M2 or later
- Diff *editing* (resolve conflicts inline) — M2 or later
- Multi-user / collaborative editing
- Building a backup/export system from scratch in W2
- PDF annotation / form filling
- Video / audio preview (Proma supports mp4/webm/mov; defer to M2)
- Workspace tabs UI restructure (separate workstream — see `docs/superpowers/specs/2026-05-11-per-workspace-tabs-design.md`)

---

## 10. Spec Self-Review Checklist

- [x] No TBDs in implementation paths — open questions §8 are decision points, not unspecified work
- [x] §3 (W1) doesn't depend on §4 (W2), §4 doesn't depend on §3 — waves can ship in any order if needed (though §6 W4 depends on §3 W1 cache+refresh-atom and §5 W3 mount data)
- [x] Each wave specifies new Tauri commands AND notes invoke_handler registration
- [x] Each wave has bisectable commit list
- [x] Module size cap (≤300 line target / ≤400 hard) baked into every file budget
- [x] UI/UX checklist (§2.2) referenced from each UI-bearing wave
- [x] No hardcoded colors in spec examples (all token-based)
- [x] Migration version registry rule re-stated (§2.4)
- [x] Architecture delta documented up-front (§0)
- [x] Out-of-scope explicitly enumerated (§9)
