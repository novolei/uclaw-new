# Tool result beautification — Phase 1 design

**Status:** Design v1, brainstorming gate passed 2026-05-18.
**Base:** `main` at `e34b74e`.
**Spec scope:** uClaw chat trajectory — replace the stub `ToolResultRenderer` with a Pierre-powered fleet covering write/edit/read/bash + the "preview" affordance on tool activity rows. Sets the foundation for Phase 2 (grep/glob/web).

## Problem

uClaw's `ui/src/components/agent/tool-result-renderers.tsx` is a 21-line stub: every tool's result renders as raw text inside `<pre>`. For file-write operations (`write_file`, `edit`) this is especially poor — users see giant blobs of unsyntax-highlighted code with no diff coloring, no line numbers, no "preview in panel" affordance.

Proma (uClaw's predecessor) ships a complete fleet of specialized renderers built on the **[@pierre/diffs](https://www.npmjs.com/package/@pierre/diffs)** React library (Apache 2.0, by the Bootstrap team). Pierre wraps Shiki for syntax highlighting and the `diff` library for diff computation, exposing three components: `MultiFileDiff` (Write), `FileDiff` (Edit), `File` (Read). Each Proma renderer is 30–150 lines because Pierre handles 90% of the work.

User reference screenshot (2026-05-18) shows the target UX: a one-line tool activity row `[icon] 写入 server.js +176 [v]                                       [预览]` with the **"preview" button always visible** on the right, and (when expanded) a beautiful diff with line numbers, syntax highlight, and a green stripe down the added lines.

This spec ports Phase 1: the 4 highest-traffic tools (`write_file`, `edit`, `read_file`, `bash`) + the preview-button affordance + foundation primitives.

## Goals

1. Replace the stub `tool-result-renderers.tsx` with a Pierre-powered dispatcher.
2. Ship per-tool renderers for `write_file` / `edit` / `read_file` / `bash`.
3. Add an always-visible **"预览"** button on the tool activity row for file-write tools, wired into the multi-tab preview panel (PR #190 infra).
4. Make `write_file` content render incrementally as the tool input streams in (matches Proma).
5. Ship a `CollapsibleResult` shared primitive that long-output renderers can wrap with.
6. Set up the directory + dispatcher infrastructure so Phase 2 (grep / glob / web / MCP default) is a plug-in operation.

## Non-goals

- Renderers for `grep` / `glob` / `web_fetch` / `web_search` / MCP tools (Phase 2).
- Task / sub-agent nested rendering (Phase 2+).
- Editing in-place from the tool result panel (the existing preview panel handles edits).
- Per-tool TypeScript input shape validation (renderers extract fields defensively; runtime mismatches degrade gracefully).
- Replacing the `closePreviewAction` confirm flow (covered by PR #190).

## Architecture

```
ui/src/components/agent/
├── tool-result-renderers.tsx   ← DELETE (current 21-line stub)
└── tool-renderers/             ← NEW directory
    ├── index.tsx               ← dispatcher (switch by snake_case tool name)
    ├── collapsible-result.tsx  ← shared primitive
    ├── default-result.tsx      ← fallback (MCP / unmatched)
    ├── write-result.tsx        ← uses Pierre MultiFileDiff
    ├── edit-result.tsx         ← uses Pierre FileDiff (loops over uClaw's edits[] array)
    ├── read-result.tsx         ← uses Pierre File (code-only, no diff)
    ├── bash-result.tsx         ← terminal style + stderr highlight
    └── pierre-theme.ts         ← CSS-var bridge from uClaw themes to Pierre vars
```

The dispatcher's public surface (props + name) stays compatible with the old stub so callers don't break:

```ts
export interface ToolResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
  basePath?: string  // for resolving relative file paths in renderers
}
export function ToolResultRenderer(props: ToolResultRendererProps): React.ReactElement
```

## Data model

### Pierre integration

Add to `ui/package.json`:
```json
"@pierre/diffs": "^1.1.22"
```

Pierre brings its own deps (`@pierre/theme`, `@shikijs/transformers`, `diff`, `hast-util-to-html`, `lru_map`, `shiki`). uClaw already has `shiki`, so the net bundle add is ~3–4 MB. Acceptable for desktop.

Pierre exports React components: `MultiFileDiff`, `FileDiff`, `File`. Each takes `theme: 'one-light' | 'one-dark-pro' | …` plus per-component props (file content, diff hunks, etc.). CSS is imported via `@pierre/diffs/dist/index.css` (or similar — implementer verifies actual path).

Theme prop wired to the existing `resolvedThemeAtom`:
```tsx
const theme = useAtomValue(resolvedThemeAtom)
const pierreTheme = theme === 'dark' || theme.includes('dark') ? 'one-dark-pro' : 'one-light'
<MultiFileDiff theme={pierreTheme} ... />
```

### Tool name dispatcher

Proma uses Claude Code SDK PascalCase (`Write`, `Edit`, `Read`, `Bash`); uClaw uses snake_case (`write_file`, `edit`, `read_file`, `bash`). The dispatcher switches on uClaw's names:

```tsx
switch (toolName) {
  case 'write_file': return <WriteResultRenderer input={input} result={result} isError={isError} />
  case 'edit':       return <EditResultRenderer input={input} result={result} isError={isError} basePath={basePath} />
  case 'read_file':  return <ReadResultRenderer input={input} result={result} isError={isError} />
  case 'bash':       return <BashResultRenderer input={input} result={result} isError={isError} />
  default:           return <DefaultResultRenderer toolName={toolName} input={input} result={result} isError={isError} />
}
```

### Per-tool input shape (verified at implementation time)

The implementer for each renderer task **must grep `src-tauri/src/agent/tools/builtin/<tool>.rs`** to confirm the actual field names. Working assumptions (subject to verification):

| Tool | Likely input fields |
|---|---|
| `write_file` | `{ path: string, content: string }` |
| `edit` | `{ path: string, edits: Array<{ old_string: string, new_string: string, replace_all?: boolean }> }` (likely batch, unlike Proma's single-edit `Edit`) |
| `read_file` | `{ path: string, offset?: number, limit?: number }` |
| `bash` | `{ command: string, timeout_ms?: number }` |

If `edit` is indeed batch (multiple edits per call), the EditResultRenderer iterates and renders one `<FileDiff>` per edit entry (Pierre handles it cleanly).

## Renderers — concrete designs

### `WriteResultRenderer`

Renders new-file creation as a Pierre `MultiFileDiff` with empty "old" and full "new" content. Shows ALL lines as additions (green stripe down the left edge, matching the user's screenshot).

```tsx
export function WriteResultRenderer({ input, result, isError }: Props): ReactElement {
  if (isError) return <ErrorBlock result={result} />
  const path = (input.path ?? input.file_path) as string
  const content = (input.content ?? '') as string
  const theme = usePierreTheme()
  return (
    <div className="max-h-[400px] overflow-auto rounded-md border border-border bg-content-area">
      <MultiFileDiff
        theme={theme}
        files={[{ path, oldContent: '', newContent: content, language: detectLang(path) }]}
      />
    </div>
  )
}
```

`detectLang(path)` — small helper inferring Shiki language from file extension. Lives in `pierre-theme.ts` or a shared `lang-detect.ts`.

**Streaming behavior:** if `result === ''` and `input.content` is non-empty but `done === false` (passed via parent), still render — Pierre handles partial input gracefully (it's just less content). The parent shows a `<Loader2 className="animate-spin" />` overlay separately via the existing ToolActivityItem spinner mechanic.

### `EditResultRenderer`

uClaw's `edit` tool takes a batch `edits: { old_string, new_string, replace_all? }[]`. Render one `<FileDiff>` per entry, vertically stacked.

```tsx
export function EditResultRenderer({ input, isError }: Props): ReactElement {
  if (isError) return <ErrorBlock result={input.error ?? 'Edit failed'} />
  const path = (input.path ?? input.file_path) as string
  const edits = (input.edits ?? []) as Array<{ old_string: string; new_string: string }>
  const theme = usePierreTheme()
  return (
    <div className="space-y-2 max-h-[500px] overflow-auto">
      {edits.map((edit, i) => (
        <FileDiff
          key={i}
          theme={theme}
          path={path}
          oldContent={edit.old_string}
          newContent={edit.new_string}
          language={detectLang(path)}
        />
      ))}
    </div>
  )
}
```

Line-number context (Proma's "smart offset" feature using `electronAPI.resolveAndReadFile`) is **deferred to Phase 2** — adds complexity and most edits are small enough that the immediate-surroundings view from Pierre is sufficient.

### `ReadResultRenderer`

Uses Pierre's `File` component (code-only, no diff). Wraps in `CollapsibleResult` for files > 3000 chars.

```tsx
export function ReadResultRenderer({ input, result, isError }: Props): ReactElement {
  if (isError) return <ErrorBlock result={result} />
  const path = (input.path ?? input.file_path) as string
  const offset = (input.offset as number | undefined) ?? 1
  const content = stripLinePrefixes(result)  // remove "    1\t" / etc. if SDK injects them
  const theme = usePierreTheme()
  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <File
        theme={theme}
        path={path}
        content={content}
        lineNumberStart={offset}
        language={detectLang(path)}
      />
    </CollapsibleResult>
  )
}
```

### `BashResultRenderer`

Terminal-style output: black bg, monospace, command echo with `$` prefix, stderr lines auto-highlighted via simple pattern match (`error:`, `traceback`, `failed`, etc.).

```tsx
export function BashResultRenderer({ input, result, isError }: Props): ReactElement {
  const command = (input.command as string) ?? ''
  return (
    <CollapsibleResult charThreshold={2000} previewLines={20}>
      <div className="rounded-md bg-zinc-950 dark:bg-zinc-900 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
        <div className="text-emerald-400 mb-1.5">$ {command}</div>
        <pre className="whitespace-pre-wrap break-all">
          {result.split('\n').map((line, i) => {
            const lower = line.toLowerCase()
            const isErrorLine = isError || /error|exception|traceback|failed|fatal/i.test(lower)
            return (
              <div key={i} className={isErrorLine ? 'text-red-400' : ''}>{line}</div>
            )
          })}
        </pre>
      </div>
    </CollapsibleResult>
  )
}
```

Black bg here is intentional — it's terminal semantics, not a theme violation. The user's existing themes accept this convention (Proma does the same).

### `DefaultResultRenderer`

Fallback for MCP tools and anything not matched. Tries JSON parse → key-value table; falls back to plain text in `CollapsibleResult`.

```tsx
export function DefaultResultRenderer({ toolName, result, isError }: Props): ReactElement {
  let parsed: Record<string, unknown> | null = null
  try { parsed = JSON.parse(result) } catch { /* not JSON */ }
  if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
    return (
      <div className="rounded-md bg-muted/30 p-2 text-xs font-mono">
        <table className="w-full">
          <tbody>
            {Object.entries(parsed).map(([k, v]) => (
              <tr key={k}>
                <td className="font-medium text-foreground pr-2 align-top">{k}</td>
                <td className="text-muted-foreground break-all">{typeof v === 'string' ? v : JSON.stringify(v)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }
  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <pre className={cn(
        'whitespace-pre-wrap break-all text-xs px-3 py-2',
        isError ? 'text-destructive bg-destructive/5' : 'text-muted-foreground bg-muted/20',
      )}>{result}</pre>
    </CollapsibleResult>
  )
}
```

### `CollapsibleResult` primitive

```tsx
interface Props {
  charThreshold?: number  // default 3000
  previewLines?: number   // default 15
  children: React.ReactNode
}

export function CollapsibleResult({ charThreshold = 3000, previewLines = 15, children }: Props): ReactElement {
  const childString = React.useMemo(() => extractText(children), [children])
  const exceedsThreshold = childString.length > charThreshold
  const [expanded, setExpanded] = React.useState(false)
  if (!exceedsThreshold) return <>{children}</>
  return (
    <div>
      <div className={cn(!expanded && 'max-h-[calc(var(--line-height)*15)] overflow-hidden')}>
        {children}
      </div>
      <button
        onClick={() => setExpanded(v => !v)}
        className="mt-1.5 text-xs text-muted-foreground hover:text-foreground inline-flex items-center gap-1"
      >
        {expanded ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />}
        {expanded ? '收起' : `展开全部 (${childString.length} 字符, ${childString.split('\n').length} 行)`}
      </button>
    </div>
  )
}
```

`extractText(children)` is a helper that walks the React node tree extracting plain text — used only for the threshold check + the footer label. Renderers don't need to know about this.

## "预览" button on tool activity row

`ui/src/components/agent/ToolActivityItem.tsx` currently renders the icon + name + status + duration + chevron. Add a small "预览" button on the **right side** of the row, always visible (not hover-only — matches user's screenshot), only for tools that have a `previewTarget`:

- `write_file`: previewTarget = input.path
- `edit`: previewTarget = input.path
- `plan_write`: previewTarget = the plan filename returned in result
- `read_file`: NO preview button — Read doesn't need a preview, the result IS the preview

```tsx
function shouldShowPreviewButton(toolName: string, input: Record<string, unknown>): boolean {
  return ['write_file', 'edit', 'plan_write'].includes(toolName)
    && Boolean(input.path ?? input.file_path)
}

// In ActivityRow JSX, before the chevron:
{shouldShowPreviewButton(toolName, input) && (
  <button
    onClick={(e) => { e.stopPropagation(); handlePreview(input.path ?? input.file_path) }}
    className="shrink-0 px-2 py-0.5 text-[11px] text-muted-foreground hover:text-foreground hover:bg-muted/60 rounded border border-border/40 transition-colors"
    aria-label={`Preview ${input.path}`}
  >
    预览
  </button>
)}
```

`handlePreview` calls the existing `openPreviewTabAction({ target, source: 'agent' })` from PR #190.

Even when auto-preview is OFF (settings toggle), this button is the always-accessible "show me this file NOW" affordance. When auto-preview is ON, the button is functionally redundant but visually consistent — the user always sees the same affordance regardless of settings.

## Streaming `write_file` rendering

uClaw's backend streams tool inputs (see `tool_start` → progressive `input` field updates → `tool_result`). The existing `agentStreamingStatesAtom` / `agentToolActivitiesAtom` chain already captures partial input. Each tool activity has a `done: boolean` flag.

For `WriteResultRenderer`, the parent (`ContentBlock` / `ToolActivityItem`) passes the current `input.content` regardless of `done` status. Pierre's `MultiFileDiff` happily re-renders as `content` grows. The visual loading state (spinner overlay, "writing…" label) stays driven by the existing `done` flag in `ToolActivityItem`.

No new streaming infrastructure needed.

## Testing strategy

- **Unit (RTL)** per renderer: feed `{ input, result, isError }` fixtures → assert rendered DOM contains expected file path / content / language class. Pierre's internals are mocked or rendered as test-double in vitest's jsdom env. Each renderer file gets its own `.test.tsx`.
- **Dispatcher test**: for each tool name, assert the right child component is rendered (use `screen.getByText` or test-id markers per renderer).
- **`CollapsibleResult` test**: threshold not crossed → no collapse UI; crossed → preview-only + footer; click footer → expand.
- **`ToolActivityItem` preview button test**: button renders for the 3 file-write tools, NOT for read_file/bash; click fires `openPreviewTabAction`.
- **Streaming integration** (deferred to manual smoke): in dev build, ask the agent to write a long file → verify content appears incrementally.

## Edge cases

- **Tool input field name mismatch**: each renderer defensively checks both Proma-style (`file_path`) and uClaw-style (`path`) field names so it survives either schema. (Recovery is graceful: if both are undefined, render a "Missing path" placeholder.)
- **`edit` input shape uncertainty**: uClaw may store edits as `[{old_string, new_string}, …]` OR as a single `{old_string, new_string}`. Renderer handles both via `Array.isArray(edits) ? edits : [edits]`.
- **Pierre fails to load language**: if `detectLang(path)` returns unknown, fall back to `'text'`. Pierre handles gracefully.
- **Result is empty string** (tool ran but produced no output): show "(no output)" placeholder instead of empty box.
- **`isError` true with empty result**: show error icon + "Tool failed (no output)" rather than blank.

## File map (implementation order)

| # | File | Action |
|---|---|---|
| 1 | `ui/package.json` | ADD `@pierre/diffs` dep |
| 1 | `ui/src/components/agent/tool-renderers/pierre-theme.ts` | NEW |
| 2 | `ui/src/components/agent/tool-renderers/collapsible-result.tsx` | NEW |
| 2 | `ui/src/components/agent/tool-renderers/collapsible-result.test.tsx` | NEW |
| 3 | `ui/src/components/agent/tool-renderers/index.tsx` | NEW (dispatcher) |
| 3 | `ui/src/components/agent/tool-renderers/default-result.tsx` | NEW |
| 3 | `ui/src/components/agent/tool-renderers/index.test.tsx` | NEW (dispatcher test) |
| 3 | `ui/src/components/agent/tool-result-renderers.tsx` | DELETE; migrate callers |
| 4 | `ui/src/components/agent/tool-renderers/write-result.tsx` + test | NEW |
| 5 | `ui/src/components/agent/tool-renderers/edit-result.tsx` + test | NEW |
| 6 | `ui/src/components/agent/tool-renderers/read-result.tsx` + test | NEW |
| 7 | `ui/src/components/agent/tool-renderers/bash-result.tsx` + test | NEW |
| 8 | `ui/src/components/agent/ToolActivityItem.tsx` | MODIFY (preview button) |
| 9 | `ui/src/components/agent/ChatToolBlock.tsx` + chat callers | MODIFY (use new dispatcher) |

## Implementation order (input to writing-plans)

1. **Pierre dependency + theme bridge** — install, verify import, write theme adapter
2. **`CollapsibleResult` primitive** — TDD'd standalone, no Pierre dep
3. **Dispatcher + `DefaultResultRenderer`** — foundation; deletes stub, migrates callers
4. **`WriteResultRenderer`** — first real Pierre integration; tests with synthesized streaming input
5. **`EditResultRenderer`** — array iteration over uClaw's batch edits
6. **`ReadResultRenderer`** — single-file Pierre File component + Collapsible wrap
7. **`BashResultRenderer`** — terminal styling, stderr heuristic, no Pierre dep
8. **`ToolActivityItem` preview button** — small surgical change, hooks to PR #190's `openPreviewTabAction`
9. **End-to-end verification** — manual smoke test against a real agent session (write/edit/read/bash all visible)

writing-plans will turn these into bisectable per-commit tasks.

## Risks / open questions

1. **`@pierre/diffs` CSS injection mechanism unknown** — the package may export CSS as a separate import (`@pierre/diffs/dist/styles.css`), inject via `<style>` tag at runtime, or rely on Tailwind classes. Implementer verifies at install time. If CSS missing, port `pierre-styles.ts` constants manually.
2. **`@pierre/theme` sub-dep** — Pierre depends on its own theme package. If that depends on something incompatible with uClaw (unlikely), may need to pin versions or fork.
3. **uClaw `edit` tool batch shape** — if it's single-edit not batch, the renderer is simpler; if batch, iterate. Verified at implementation time.
4. **Streaming performance** — re-rendering Pierre's `MultiFileDiff` on every token chunk might be expensive. Acceptable for v1 (files are typically < 5k lines); optimization deferred until measured.
5. **11 themes × Pierre defaults** — Pierre's `one-light`/`one-dark-pro` themes may not look great in uClaw's warm-paper / qingye themes. Phase 1 ships with Pierre defaults; per-theme tuning is Phase 2 polish.
6. **Existing tool-result rendering callers** — grep at implementation time to confirm all callers are migrated (currently: ToolActivityItem L418, ContentBlock L429, ChatToolBlock L129 per prior audit).

## Phase 2 scope (preview, not in this spec)

- `GrepResultRenderer` — per-file panels, line-by-line matches, pattern highlight
- `GlobResultRenderer` — file list with FileTypeIcon, grouped by directory
- `WebSearchResultRenderer` — card grid with title/URL/snippet
- `WebFetchResultRenderer` — markdown-rendered fetched content
- `TaskResultRenderer` / `AgentResultRenderer` — collapsible nested message tree
- MCP tool special-casing
- Per-theme Pierre var tuning (warm-paper / qingye / forest etc.)
- Line-number smart offset for `edit` (Proma's `resolveAndReadFile` equivalent)
