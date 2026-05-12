# W4d — Preview Inline Editing + DiffRenderer — Design

> **Status**: spec. Implementation plan to follow in `docs/superpowers/plans/2026-05-12-w4d-preview-inline-editing.md`.

## 0. Goal

Add inline editing to the preview panel for plain-text formats (txt/log/csv/ini/env + CODE_EXTS + markdown). Save with optimistic mtime-based concurrency control; surface external changes via a non-modal banner with a side-by-side diff view. Ship `DiffRenderer` as both the conflict-resolution surface AND a standalone renderer for `.diff` / `.patch` files.

This is the largest remaining wave of the Proma v0.9.27 preview port (the only one materially bigger than W4b). Closes out the "M1 editing scope" from the master spec §6.6.

## 1. Master-spec anchor

This spec implements `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6.6 (editing scope), §6.7 (panel layout — already shipped by W4a), and the DiffRenderer slot listed in §6.1. Defers everything else from the master spec to later waves (see §10).

## 2. M1 editable scope (mirror of master spec §6.6)

| Format | Read | Edit |
|---|---|---|
| `txt` / `log` / `csv` / `ini` / `env` | ✓ | ✓ (CodeMirror 6 / `language='text'`) |
| CODE_EXTS (`ts/tsx/js/jsx/py/rs/go/...`) | ✓ | ✓ (CodeMirror 6 / shiki language) |
| `md` / `markdown` | ✓ | ✓ (TipTap rich by default + CM6 raw toggle) |
| `diff` / `patch` | ✓ | ✗ (rendered by DiffRenderer) |
| `docx` / `xlsx` / `pptx` | ✓ | ✗ (defer to M2 / never) |
| `doc` / `xls` / `ppt` | hint only | ✗ |
| `pdf` / image | ✓ | ✗ |

## 3. Editor stack decision

**CodeMirror 6 + TipTap** (matches master spec). Rationale:

- CM6 for raw text/code/markdown — universal, themed via shiki tokens, lazy per-language loading
- TipTap for markdown rich mode — gives the "feels like Notion" experience users expect for note-taking
- Toggle persisted globally via `atomWithStorage('uclaw-md-editor-mode', 'rich')`
- Bundle impact: ~350 KB minified, split into a single `editors` chunk via `vite.config.ts manualChunks`; read-only sessions pay 0

**Rejected**: Monaco (~500 KB, overkill for small-file editing).

## 4. Save mechanics — hybrid

| File type | Save trigger |
|---|---|
| Code / text (CODE_EXTS, txt/log/csv/ini/env) | **Explicit Cmd/Ctrl-S**. `useDirtyBuffer` registers; file-switch / panel-close / window-close intercepts. |
| Markdown (raw OR rich) | **Auto-save**, 300 ms debounce after last keystroke. No dirty-state registry; transient buffer only. |

Rationale: code editing has a "save = commit" mental model where the user owns the moment; markdown is closer to note-taking where auto-save matches Notion/Bear/Obsidian. Auto-save pauses while a conflict banner is showing (§7).

## 5. Backend — `preview_write_text`

### 5.1 Wire format (`src-tauri/src/preview/types.rs`)

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WriteResult {
    Saved { mtime_ms: i64, size: u64 },
    Conflict { current_mtime_ms: i64, current_content: String },
    NeedsApproval { approval_id: String },
}
```

`current_content` returned on Conflict avoids a follow-up read roundtrip — frontend can render the diff banner immediately.

### 5.2 Tauri command

```rust
#[tauri::command]
pub async fn preview_write_text(
    state: State<'_, AppState>,
    mount_id: String,
    rel_path: String,
    session_id: Option<String>,
    content: String,
    expected_mtime_ms: i64,   // -1 if no expectation (new file or first save)
) -> Result<WriteResult, Error>
```

Flow:
1. Resolve `(mount_id, rel_path)` via existing `resolve_path` (already canonicalises and rejects `..` segments — W4a infrastructure).
2. Check `mount.editable` (W3 `MountRoot.editable: bool`):
   - `true` → proceed.
   - `false` AND `mount.kind == AttachedDir` → queue a `PendingApproval`, return `WriteResult::NeedsApproval { approval_id }`. Frontend opens an approval dialog; on Allow, calls sibling command `approve_preview_write(approval_id, allowed)`.
3. `stat` the resolved path. If file exists AND `expected_mtime_ms != current_mtime`:
   - Read current content (capped at `MAX_PREVIEW_BYTES = 50 MB`), return `Conflict { current_mtime_ms, current_content }`.
4. Write atomically: write to a tempfile in the **same directory** as the target, then `fs::rename`. Preserves Unix perms; atomic on POSIX. Fallback for cross-filesystem rename failure: write-then-fsync directly to target.
5. Return `Saved { mtime_ms, size }`.

### 5.3 Approval flow

Reuses existing `state.pending_approvals: Arc<PendingApprovals>` (already powers tool-call approvals). New helper in `src-tauri/src/preview/approval.rs` (~60 LOC):

```rust
pub async fn request_write_approval(
    state: &AppState,
    abs_path: &Path,
    reason: &str,
) -> Result<bool, Error>  // resolves when user clicks Allow / Deny
```

Tauri event `preview:write_approval_request` carries `{ approval_id, path, reason }`. Frontend's `<WriteApprovalDialog>` consumes the event, presents Allow / Deny, calls `approve_preview_write(approval_id, allowed)` which dispatches the oneshot resolution.

### 5.4 Safety constraints

- 50 MB hard cap on `content.len()` — mirror the read cap. Reject `BadRequest("file too large")`.
- Post-write `stat` verifies size matches input length; detects partial writes.
- UTF-8 only at this layer; `content: String` already validated by serde.
- Path traversal already rejected by `resolve_path`.

### 5.5 Backend tests (`src-tauri/src/preview/tests.rs` write submodule)

6 unit tests + 2 approval-flow tests:

```rust
mod write_tests {
    #[tokio::test] async fn writes_to_workspace_mount_succeeds() { ... }
    #[tokio::test] async fn conflict_when_mtime_mismatch() { ... }
    #[tokio::test] async fn rejects_path_traversal() { ... }
    #[tokio::test] async fn rejects_non_editable_mount_with_needs_approval() { ... }
    #[tokio::test] async fn atomic_rename_preserves_unix_perms() { ... }
    #[tokio::test] async fn write_creates_new_file_when_expected_mtime_is_minus_one() { ... }
}

mod approval_tests {
    #[tokio::test] async fn approval_allow_proceeds_with_write() { ... }
    #[tokio::test] async fn approval_deny_returns_permission_denied() { ... }
}
```

All tests use `tempfile::TempDir` for the workspace; an `AppState` builder helper mocks `files_rail_list_mounts` to return a tempdir.

## 6. Frontend — editors

### 6.1 Module layout

```
ui/src/components/preview/editors/
├── TextEditor.tsx              (~180 LOC) — CodeMirror 6 host
├── MarkdownRichEditor.tsx      (~200)     — TipTap host
├── MarkdownEditor.tsx          (~80)      — Wrapper: routes by mode toggle
├── EditorToolbar.tsx           (~110)     — Save state pill + MD toggle + format actions
├── ConflictBanner.tsx          (~90)      — External-change banner with 3 actions
├── WriteApprovalDialog.tsx     (~80)      — Modal for outside-mount write approval
├── codemirror-theme.ts         (~120)     — uClaw tokens → CM6 EditorView theme
├── codemirror-langs.ts         (~80)      — Lazy language imports keyed on ext
└── useDirtyBuffer.ts           (~100)     — Hook: dirty state + intercept
```

### 6.2 Common editor API

```ts
type SaveOutcome =
  | { kind: 'saved'; mtimeMs: number }
  | { kind: 'conflict'; externalContent: string; externalMtimeMs: number }
  | { kind: 'needs-approval'; approvalId: string }
  | { kind: 'error'; message: string }

interface EditorProps {
  initialContent: string
  language?: string              // shiki id; TipTap ignores
  mtimeMs: number                // baseline for conflict detection
  filePath: string               // identity key for dirty-buffer registry
  saveMode: 'explicit' | 'auto'
  onSave: (content: string) => Promise<SaveOutcome>
  onContentChange: (content: string, isDirty: boolean) => void
  readOnly?: boolean
}
```

Both `TextEditor` and `MarkdownRichEditor` conform. `MarkdownEditor` wraps:

```tsx
function MarkdownEditor(props: EditorProps) {
  const mode = useAtomValue(markdownEditorModeAtom)
  return mode === 'rich'
    ? <MarkdownRichEditor {...props} />
    : <TextEditor {...props} language="markdown" />
}
```

### 6.3 useDirtyBuffer (explicit-save only)

```ts
export interface DirtyBuffer {
  filePath: string
  content: string
  baselineMtimeMs: number
}

export const dirtyBuffersAtom = atom<Map<string, DirtyBuffer>>(new Map())
```

Auto-save mode never registers. Explicit-save mode:
- Registers on first change after baseline
- Clears on successful save
- Intercept atoms (`openPreviewAction`, `closePreviewAction`) read `dirtyBuffersAtom` and dispatch a confirm dialog if dirty
- `beforeunload` window event for browser/Tauri close

### 6.4 TipTap rich-mode caveat

Round-trip from rich markdown back to source can lose fidelity on raw HTML blocks, footnote syntax, and some GFM table edge cases. Mitigation: one-time toast on first edit in rich mode per session — *"富文本编辑可能简化部分原始 Markdown 语法 — 切换到「源码」可保留所有原文"*. Suppressible via `localStorage` key `uclaw-tiptap-fidelity-warning-shown`.

### 6.5 Read-only file types

When `classifyExtension` returns a non-editable kind (image/pdf/docx/xlsx/pptx/legacyOffice/binary/diff), `PreviewSurface` routes to the existing renderer (CodeRenderer / DiffRenderer / image / etc.) — no editor mounts. Header doesn't show editor toolbar slots.

## 7. Frontend — conflict banner

```
┌────────────────────────────────────────────────────────────────────┐
│ ⚠  File changed on disk · [View diff] [Overwrite] [Discard mine] ✕ │
├────────────────────────────────────────────────────────────────────┤
│ (editor content stays visible; banner is sticky at top)            │
└────────────────────────────────────────────────────────────────────┘
```

State:
- `conflictAtom`: `Map<filePath, { externalContent: string, externalMtimeMs: number }>`
- For **markdown auto-save**: auto-save pauses while conflict exists for this filePath; user must resolve.
- For **code explicit-save**: editor stays interactive; user can keep typing and re-save (will re-conflict if mtime still stale).

Actions:
- **View diff** → opens a modal containing `<DiffRenderer left={localContent} right={externalContent} />`
- **Overwrite** → re-save with `expected_mtime_ms = externalMtimeMs`. Banner clears.
- **Discard mine** → set editor content to `externalContent`, update baseline mtime, banner clears.
- **✕** → dismiss banner only; editor keeps user's edits, mtime stays stale, next save will conflict again.

### 7.1 Self-write echo guard

The file watcher fires `Modified` events on our own writes, which could be mis-interpreted as external changes. Mitigation: editor tracks `lastSelfWriteMtimeMs`; the watcher subscription in the editor host ignores `Modified` events whose mtime matches exactly.

## 8. DiffRenderer

### 8.1 Component shape

```tsx
interface DiffRendererProps {
  left:  { content: string; label: string }   // e.g. "Your edits"
  right: { content: string; label: string }   // e.g. "On disk"
  language?: string                            // for shiki highlighting of unchanged lines
}
```

Pure component, no editor logic. Used in two surfaces:
1. **Conflict banner "View diff"** modal
2. **`.diff` / `.patch` file preview** via a new `RendererKind: 'diff'` in `ext-classifier.ts`

### 8.2 Hunk-collapse pattern (ported from if2Ai)

Borrowed wholesale from `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/WriteToolDiffCard.tsx:40-244`:

```ts
function buildRenderHunks(patch: Hunk[]): DiffLine[] {
  // Flatten structuredPatch hunks into render-ready lines with
  // sticky oldNo/newNo. Unchanged regions between hunks become
  // collapse buttons; user clicks to slice & reveal context.
}

function gapLineCount(line: DiffLine, next: DiffLine): number {
  // Distance between hunks in original-file line numbers.
}

function GapExpansion({ oldContent, fromLine, toLine, onExpand }) {
  // Lazy: only slices oldContent when expanded.
}
```

State:
- `showFull: boolean` toggle — when true, re-runs `structuredPatch` with `context = Number.MAX_SAFE_INTEGER` (clever reuse: full view = zero-collapse hunked view)
- `expandedGaps: Set<string>` — per-hunk-gap reveal

### 8.3 Density-cells header

12-cell `+`/`-` ratio bar at top:

```
[++++++--___---] 23 additions, 8 deletions
```

~20 LOC. Adopted from `WriteToolDiffCard:85-101`.

### 8.4 Layout — side-by-side + shiki

Two-column rendering (vs if2Ai's unified). Each column:
- Line numbers in a sticky gutter
- Line content rendered through `useShikiHighlight` (existing W1 infrastructure) for syntax color on unchanged lines
- Added/removed lines get background tint over the highlighted spans (Tailwind `bg-emerald-100/60` / `bg-rose-100/60` + `dark:bg-emerald-900/30` / `dark:bg-rose-900/30`)

### 8.5 Truncation banner for large diffs

Match the existing CodeRenderer/MarkdownRenderer pattern: cap rendered diff at 5,000 lines, show a banner above the content:

```
ℹ 差异超过 5000 行 · 仅显示前 5000 行
```

Reject virtualization (`react-window`) at this stage — adds complexity for an unproven need. Revisit if a real user hits the cap.

### 8.6 LOC budget

Revised from 150 → **250–350 LOC**. Hunk-collapse math + 2-column layout + shiki integration + density cells together push past my initial estimate. The component splits into:

```
DiffRenderer.tsx                ~120 — orchestration, props, modal/standalone surfaces
useDiffHunks.ts                 ~80  — buildRenderHunks + gapLineCount + showFull state
DiffLineRow.tsx                 ~70  — single line render with shiki spans + add/del tint
DiffDensityCells.tsx            ~30  — 12-cell summary bar
```

## 9. PreviewSurface dispatch + ext-classifier additions

`ui/src/components/preview/utils/ext-classifier.ts`:
- Add `'diff'` to `RendererKind`
- Route `diff` / `patch` extensions to `kind: 'diff'` (currently routed to code with language="diff")

`PreviewSurface.tsx` gains:
```tsx
if (route.kind === 'diff') return <DiffRenderer .../>
if (isEditableKind(route.kind)) return <EditorSurface .../>  // routes to TextEditor or MarkdownEditor
```

`<EditorSurface>` is a thin wrapper around the editor + ConflictBanner + EditorToolbar (~80 LOC, replaces the current `route.kind === 'code'` / `'markdown'` arms in PreviewSurface).

## 10. Out of scope (deferred)

- **Editing DOCX/XLSX/PPTX** (still preview-only via W4b renderers)
- **Multi-cursor / column selection** (CM6 supports it; UI not exposed)
- **Find/replace toolbar** (Cmd-F still triggers browser find; CM6's search panel not surfaced)
- **Snippets / autocomplete / IntelliSense** (Monaco-like)
- **Per-file format settings** (line ending detection, indentation auto-detect)
- **DiffRenderer virtualization** — truncation banner first; revisit if real user hits the cap
- **Git-aware file rail decorations** — if uClaw later adds M/A/D badges or status bar widgets, the implementer should NOT follow if2Ai's "shell out fresh every call" pattern. Status decorations need a **debounced cache keyed on `files_rail/` notify events** to avoid hammering git on every render. Spec note for future planner.

## 11. Test plan

### 11.1 Rust (`src-tauri/src/preview/tests.rs`)

- 6 write tests (§5.5)
- 2 approval-flow tests (§5.5)
- **Total**: 8 new tests. Baseline 395 → 403.

### 11.2 TypeScript

- `useDirtyBuffer` — 3 tests under vitest fake timers (register, clear-on-save, intercept-on-switch)
- `DiffRenderer` hunk-collapse — 4 fixture tests (identical, add-only, remove-only, mixed)
- `MarkdownRichEditor` round-trip — 3 tests (basic md, GFM table, code-block-with-language)
- **Total**: 10 new tests. Baseline 296 → 306.

### 11.3 Manual checklist

- 11-theme spot-check on editor + banner + diff
- Cmd-S on a `.ts` file while agent edits same → conflict banner
- Edit `.md`, observe auto-save (within 1 s), agent edits same → banner
- Edit a file in an `editable: false` AttachedDir → approval dialog
- Discard / Overwrite / View diff actions all reset state correctly
- File-switch with dirty buffer in code → intercept confirms
- Window close with dirty buffer → intercept confirms
- TipTap rich → raw toggle preserves content
- DiffRenderer hunk-collapse: click "show full" reveals all; expandedGaps work independently per hunk

## 12. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| TipTap → markdown round-trip loss on edge-case features | medium | low | One-time toast; user switches to raw mode permanently |
| Bundle size +350 KB | low | medium | Manual chunk `editors`, lazy-loaded on first edit. Read-only sessions pay 0 |
| Cross-filesystem `fs::rename` failure | very low | low | Fall back to direct write-then-fsync |
| Auto-save IPC volume | low | low | 300 ms debounce; backend write <5 ms typical; raise to 500 ms if profiling shows issue |
| Self-write echo masquerading as external change | medium | medium | Editor tracks `lastSelfWriteMtimeMs`; watcher subscription filters matching events |
| DiffRenderer + shiki on very large files | low | low | Truncate at 5,000 lines with banner |
| TipTap deps + transitive size bloat | medium | medium | Use `@tiptap/starter-kit` (curated) + 2–3 specific extensions; avoid `@tiptap/extension-table` (heavy, edge-case-heavy round-trip) until needed |

## 13. Reference borrows summary

From `/Users/ryanliu/Documents/IfAI/if2Ai`:
- `src/components/chat/WriteToolDiffCard.tsx:40-244` — `buildRenderHunks` + `gapLineCount` + `GapExpansion` pattern. Drop-in for our DiffRenderer's hunk-collapse logic.
- `src/components/chat/WriteToolDiffCard.tsx:85-101` — `densityCells` 12-cell summary. Drop-in for our DiffRenderer header.
- `src-tauri/src/modules/git/status.rs:50-67` `DiffMode::{Stat, Full}` discipline — applied to "external version" payload in our ConflictBanner. Default to stat-style summary in agent context, full diff only in editor surface.

Explicitly NOT borrowed:
- if2Ai's fixed `max-h-[420px]` no-virtualization scroll container — we use a truncation banner pattern instead
- if2Ai's "shell out fresh every git call, no cache" backend — fine for them, but uClaw will need cached status decorations eventually (see §10 out-of-scope note)
- if2Ai's `assert_cwd_in_registered_projects` sandbox — already covered by W3's mount-id-based `resolve_path` pipeline

## 14. Spec self-review

(Inline checklist; fix issues in place.)

**Placeholder scan**: no "TBD", "TODO", or vague requirements remain.

**Internal consistency**: `WriteResult` enum keys (`Saved` / `Conflict` / `NeedsApproval`) are referenced consistently across §5.1 backend types, §6.2 frontend `SaveOutcome` union, and §11 tests. `MountRoot.editable` flag use is consistent across §5.2 (W3 source) and §10 (deferred git decorations note). The hybrid save matrix (§4) is consistent with §6.3 (`useDirtyBuffer` engages only for explicit save) and §7 (auto-save pauses on conflict).

**Scope check**: single implementation plan-sized. ~16 tasks, similar shape to W4b's 11-task plan. Estimate 4–6 days of subagent-driven work.

**Ambiguity check**: `expected_mtime_ms = -1` for new files is explicit (§5.2 step 3 covers the "file doesn't exist" branch). The "auto-save pauses while conflicted" rule (§7) applies per-filePath, not globally — already explicit. Read-only kinds (§6.5) are exhaustively enumerated.
