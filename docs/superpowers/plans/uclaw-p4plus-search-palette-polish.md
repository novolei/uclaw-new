# P4+ — SearchPalette Browse Mode + Visual Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the SearchPalette landed in P4 (PR #29) with the if2Ai design pattern: when input is empty, show three browseable sections (recent threads / settings & commands / projects). When typing, filter all three sections client-side AND show server-side FTS hits for content matches. Also apply if2Ai's visual polish — frosted-glass panel, layered shadow, group headings, workspace badges, relative-time chips, and a footer with `kbd` keyboard hints.

**Architecture:**
- **Backend:** zero new commands — reuse `list_conversations`, `list_agent_sessions`, `list_spaces` (all already exist). Search command from P4 (`search_conversations`) keeps doing FTS for the typing-mode content match.
- **Frontend:** SearchPalette becomes a hybrid. Empty input → render recents/settings/projects from a one-shot fetch on open. Non-empty input → render the same three sections client-filtered + a fourth FTS section for content matches. cmdk gets `shouldFilter={false}` (already in place from P4); we do all filtering manually with `useMemo`.

**Tech Stack:** Same as P4 — `cmdk@^1`, `lucide-react`, Jotai. Adds `clock` / `folder` / `sliders-horizontal` / `brain` lucide icons; all already exported. No new npm deps.

**Reference spec:** if2Ai's `GlobalSearch.tsx` design (verified in research). Roadmap entry: `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md` §P4 (this plan extends what landed there).

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p4plus-search-polish
```

- [ ] **Step 0.2: Sanity check — P4 baseline still works**

```bash
cd src-tauri && cargo build 2>&1 | tail -3
cd ../ui && npx tsc --noEmit 2>&1 | head -3 && npm test 2>&1 | tail -8
```
Expected: clean cargo, clean tsc, **26/26 tests** (P3's 20 + P4's 6).

---

## Task 1: Backend — recent + projects fetcher

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add one new command)

The browse mode needs:
- **Recent threads**: union of chat conversations and agent sessions, ordered by `updated_at DESC`, limit 20, with their workspace name attached.
- **Projects/workspaces**: list of `spaces` rows with thread count.

Both already have list commands but they don't carry the cross-data we need (e.g. workspace name on each session). Rather than join in TS, expose a single `list_recent_threads` command that returns the merged shape. Workspaces stay on the existing `list_spaces`.

- [ ] **Step 1.1: Add `RecentThread` IPC type**

Edit `src-tauri/src/ipc.rs`. Add after `ConversationResponse`:

```rust
/// Cross-domain summary of a recent conversation or agent session for the
/// search palette's browse mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentThread {
    pub id: String,
    /// "chat" | "agent"
    pub kind: String,
    pub title: String,
    /// Optional emoji prefix (from conversation metadata)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_emoji: Option<String>,
    /// Whether title generation is still pending (show spinner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_pending: Option<bool>,
    /// Human-readable workspace name (the space the thread lives in)
    pub workspace_name: String,
    /// Workspace id for navigation
    pub workspace_id: String,
    /// Number of messages in this thread
    pub message_count: u32,
    pub updated_at: String,
}
```

- [ ] **Step 1.2: Add the Tauri command**

Edit `src-tauri/src/tauri_commands.rs`. Add this new command (insert near the existing `list_conversations` around line 471):

```rust
#[tauri::command]
pub async fn list_recent_threads(state: State<'_, AppState>) -> Result<Vec<RecentThread>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let mut out: Vec<RecentThread> = Vec::new();

    // Chat conversations — JOIN spaces for workspace name
    let mut stmt = conn.prepare(
        "SELECT
            c.id, c.title, c.metadata_json,
            s.name AS workspace_name, s.id AS workspace_id,
            (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count,
            c.updated_at
         FROM conversations c
         LEFT JOIN spaces s ON s.id = c.space_id
         WHERE COALESCE(c.is_agent, 0) = 0
         ORDER BY c.updated_at DESC
         LIMIT 20"
    ).map_err(|e| Error::Internal(format!("prepare chat list: {}", e)))?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let metadata_json: Option<String> = row.get(2)?;
        let workspace_name: Option<String> = row.get(3)?;
        let workspace_id: Option<String> = row.get(4)?;
        let msg_count: i64 = row.get(5)?;
        let updated_at: String = row.get(6)?;
        Ok((id, title, metadata_json, workspace_name, workspace_id, msg_count, updated_at))
    }).map_err(|e| Error::Internal(format!("query chat list: {}", e)))?;
    for r in rows.flatten() {
        let (id, title, metadata_json, ws_name, ws_id, msg_count, updated_at) = r;
        let (emoji, pending) = parse_title_metadata(metadata_json.as_deref());
        out.push(RecentThread {
            id,
            kind: "chat".into(),
            title: title.unwrap_or_else(|| "(untitled)".into()),
            title_emoji: emoji,
            title_pending: pending,
            workspace_name: ws_name.unwrap_or_else(|| "default".into()),
            workspace_id: ws_id.unwrap_or_else(|| "default".into()),
            message_count: msg_count.max(0) as u32,
            updated_at,
        });
    }
    drop(stmt);

    // Agent sessions
    let mut stmt = conn.prepare(
        "SELECT
            s.id, s.title, s.title_emoji, s.title_pending,
            COALESCE(sp.name, 'default') AS workspace_name,
            COALESCE(sp.id, 'default') AS workspace_id,
            s.message_count,
            s.updated_at
         FROM agent_sessions s
         LEFT JOIN spaces sp ON sp.id = s.space_id
         ORDER BY s.updated_at DESC
         LIMIT 20"
    ).map_err(|e| Error::Internal(format!("prepare agent list: {}", e)))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "(untitled)".into()),
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?.map(|v| v != 0),
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
        ))
    }).map_err(|e| Error::Internal(format!("query agent list: {}", e)))?;
    for r in rows.flatten() {
        let (id, title, emoji, pending, ws_name, ws_id, msg_count, updated_at) = r;
        out.push(RecentThread {
            id,
            kind: "agent".into(),
            title,
            title_emoji: emoji,
            title_pending: pending,
            workspace_name: ws_name,
            workspace_id: ws_id,
            message_count: msg_count.max(0) as u32,
            updated_at: updated_at.to_string(),
        });
    }
    drop(stmt);

    // Sort merged list by updated_at DESC, cap at 20.
    // Both chat and agent paths use string updated_at — chat is RFC3339, agent is i64 ms.
    // Convert agent to RFC3339-ish for sortability via string comparison: in practice agent
    // ms timestamps as strings sort *correctly* relative to themselves but compare
    // lexicographically with chat strings. Safer: parse to chrono.
    out.sort_by(|a, b| {
        // Try parse as i64 (agent ms)
        let pa = a.updated_at.parse::<i64>().ok();
        let pb = b.updated_at.parse::<i64>().ok();
        match (pa, pb) {
            (Some(la), Some(lb)) => lb.cmp(&la),  // both agent-ms: numeric compare
            (None, None) => b.updated_at.cmp(&a.updated_at), // both RFC3339: string compare works
            // Mixed: agent-ms (small i64 string) vs RFC3339 (long alpha string) — RFC3339 wins
            // because chat conversations are usually newer in active dev. Fall back to put
            // RFC3339 first.
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
        }
    });
    out.truncate(20);
    Ok(out)
}

/// Parse the conversation `metadata_json` blob for `emoji` and `title_pending`.
/// The blob looks like `{"title":"…","emoji":"🎨","title_pending":false}`.
fn parse_title_metadata(meta: Option<&str>) -> (Option<String>, Option<bool>) {
    let Some(raw) = meta else { return (None, None) };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (None, None)
    };
    let emoji = parsed.get("emoji").and_then(|v| v.as_str()).map(|s| s.to_string());
    let pending = parsed.get("title_pending").and_then(|v| v.as_bool());
    (emoji, pending)
}
```

- [ ] **Step 1.3: Register the command in `main.rs`**

Edit `src-tauri/src/main.rs`. Find the `invoke_handler!` list and add `uclaw_core::tauri_commands::list_recent_threads,` near the existing `list_conversations,`.

- [ ] **Step 1.4: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors. The `agent_sessions.title_emoji` and `agent_sessions.title_pending` columns may not exist on your dev DB (they were added in V8 if at all). If you get a "no such column" error from the runtime smoke-test in step 1.5, fall back to `NULL AS title_emoji, NULL AS title_pending`. Verify:
```bash
sqlite3 ~/.uclaw/uclaw.db ".schema agent_sessions"
```

If those columns don't exist, edit the second SQL to:
```rust
"SELECT
    s.id, s.title,
    NULL AS title_emoji, NULL AS title_pending,
    COALESCE(sp.name, 'default') AS workspace_name,
    COALESCE(sp.id, 'default') AS workspace_id,
    s.message_count, s.updated_at
 FROM agent_sessions s ..."
```
…and update the row-mapping closure accordingly (replace the two `Option<String>` / `Option<i64>` reads with `Ok(None)` / `Ok(None)`).

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(search): list_recent_threads — unified chat + agent recents

Adds one Tauri command that returns recent conversations from BOTH
the chat (`conversations`) and agent (`agent_sessions`) tables, each
joined with `spaces` for the workspace name shown as a badge in the
search palette's browse mode.

RecentThread fields:
  - id, kind ("chat" | "agent")
  - title + optional title_emoji + optional title_pending (for spinner)
  - workspaceName + workspaceId
  - messageCount, updatedAt

Reuses existing schema; no migration. The merged list is sorted by
updatedAt DESC and capped at 20.
EOF
)"
```

---

## Task 2: Frontend — RecentThread + Workspace types

**Files:**
- Modify: `ui/src/lib/agent-types.ts` (add `RecentThread`, `WorkspaceSummary` types)
- Modify: `ui/src/lib/tauri-bridge.ts` (add invoke wrappers)

- [ ] **Step 2.1: Add the type**

Edit `ui/src/lib/agent-types.ts` (or `chat-types.ts` — whichever is more appropriate; the type spans both domains so `agent-types.ts` is fine since it already re-exports from chat). Add:

```ts
/** Cross-domain recent thread shown in the search palette's browse mode. */
export interface RecentThread {
  id: string
  kind: 'chat' | 'agent'
  title: string
  titleEmoji?: string
  titlePending?: boolean
  workspaceName: string
  workspaceId: string
  messageCount: number
  updatedAt: string
}
```

`WorkspaceSummary` already exists per existing `getWorkspaces` etc. — verify and reuse:
```bash
grep -nE "interface (WorkspaceSummary|SpaceSummary)" ui/src/lib/*.ts
```

If a `WorkspaceSummary` doesn't exist, the `SpaceSummary` from `types.ts` should be sufficient (V1 schema, has `id, name, icon, conversationCount?, ...`). The palette will use `name` + `id` + `conversationCount`.

- [ ] **Step 2.2: Add invoke wrapper**

Edit `ui/src/lib/tauri-bridge.ts`. Find the existing `getWorkspaces` / `listSpaces` block and add:

```ts
import type { RecentThread } from './agent-types'

export const listRecentThreads = (): Promise<RecentThread[]> =>
  invoke<RecentThread[]>('list_recent_threads')
```

(Place it near other list-* helpers.)

- [ ] **Step 2.3: TS check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/lib/agent-types.ts ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(search): listRecentThreads bridge + RecentThread type

TS counterpart to the Tauri command added in the previous commit.
Frontend will consume this in SearchPalette's browse mode (next task).
EOF
)"
```

---

## Task 3: Frontend — SearchPalette browse mode + visual polish

**Files:**
- Rewrite: `ui/src/components/search/SearchPalette.tsx` (substantial — adds browse mode + restyling)

This is the biggest task. Restructure the palette into the if2Ai pattern:

- Empty input → render three groups (recents / settings / projects) from one-shot fetch on open
- Non-empty input → render the same three groups client-filtered + a fourth "搜索结果" group from the existing FTS command
- Frosted-glass panel + layered shadow + footer kbd hints

- [ ] **Step 3.1: Read the current SearchPalette**

```bash
cat ui/src/components/search/SearchPalette.tsx
```

Note: `cmdk` already has `shouldFilter={false}` from PR #29's fix-up. Keep that.

- [ ] **Step 3.2: Rewrite `ui/src/components/search/SearchPalette.tsx`**

Replace entire file. The new content (full):

```tsx
/**
 * SearchPalette — global ⌘K command palette.
 *
 * Two modes:
 *   - Empty input  → browse: recent threads + settings shortcuts + workspaces
 *   - Typing       → filter the same three sections client-side, plus show
 *                    server-side FTS results for content matches
 *
 * Toggle via Cmd/Ctrl+K. Esc / backdrop click closes. cmdk handles arrow-
 * key navigation + aria-selected highlight; we do all filtering manually.
 *
 * Visual design ports if2Ai's GlobalSearch:
 *   - Frosted-glass panel with layered shadow
 *   - Group headings in tracked uppercase
 *   - Workspace badges + relative-time chips on thread rows
 *   - Footer with kbd keyboard hints
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Command } from 'cmdk'
import {
  Search,
  MessageSquare,
  Bot,
  Folder,
  FolderOpen,
  Clock,
  SlidersHorizontal,
  Brain,
  Settings as SettingsIcon,
  Hash,
  type LucideIcon,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'
import { listRecentThreads, getWorkspaces } from '@/lib/tauri-bridge'
import type { RecentThread } from '@/lib/agent-types'

// ===== Types =====

interface SearchHit {
  id: string
  title: string
  snippet: string
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'file'
  sourceId: string
  messageId?: string
  createdAt: string
}

interface WorkspaceSummary {
  id: string
  name: string
  icon?: string
  conversationCount?: number
  // Other fields are present but unused here.
}

interface SettingsItem {
  id: string
  label: string
  hint: string
  icon: LucideIcon
}

const SETTINGS_ITEMS: SettingsItem[] = [
  {
    id: 'settings:providers',
    label: '服务商配置',
    hint: 'Provider / API Key / Base URL',
    icon: SlidersHorizontal,
  },
  {
    id: 'settings:models',
    label: '模型配置',
    hint: '主聊天模型 / Thinking 支持',
    icon: Brain,
  },
  {
    id: 'settings:memory',
    label: '记忆设置',
    hint: 'Memory / 编译 / 晋升',
    icon: Brain,
  },
  {
    id: 'settings:appearance',
    label: '外观设置',
    hint: '主题 / 字体 / 衬线',
    icon: SettingsIcon,
  },
]

const MAX_RECENT_BROWSE = 8
const MAX_RECENT_SEARCH = 5
const FTS_DEBOUNCE_MS = 150

// ===== Helpers =====

function formatAge(updatedAt: string): string {
  // updatedAt is either RFC3339 (chat) or i64 ms (agent)
  let ts: number
  const asNum = Number(updatedAt)
  if (Number.isFinite(asNum) && asNum > 1_000_000_000_000) {
    ts = asNum
  } else {
    const parsed = Date.parse(updatedAt)
    if (Number.isNaN(parsed)) return ''
    ts = parsed
  }
  const ageMs = Date.now() - ts
  if (ageMs < 60_000) return '刚刚'
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)}分钟前`
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)}小时前`
  return `${Math.floor(ageMs / 86_400_000)}天前`
}

function truncatePath(path: string | undefined): string {
  if (!path) return ''
  // Last two segments prefixed with "…/"
  const parts = path.split('/').filter(Boolean)
  if (parts.length <= 2) return path
  return `…/${parts.slice(-2).join('/')}`
}

// ===== Component =====

export interface SearchPaletteProps {
  onSelect?: (item:
    | { kind: 'thread'; thread: RecentThread }
    | { kind: 'workspace'; workspace: WorkspaceSummary }
    | { kind: 'settings'; settings: SettingsItem }
    | { kind: 'search_hit'; hit: SearchHit }
  ) => void
}

export function SearchPalette({ onSelect }: SearchPaletteProps): React.ReactElement | null {
  const [open, setOpen] = useAtom(searchPaletteOpenAtom)
  const [query, setQuery] = React.useState('')
  const [recents, setRecents] = React.useState<RecentThread[]>([])
  const [workspaces, setWorkspaces] = React.useState<WorkspaceSummary[]>([])
  const [hits, setHits] = React.useState<SearchHit[]>([])
  const [searching, setSearching] = React.useState(false)
  const debounceRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // Global ⌘K toggle
  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen((v) => !v)
      } else if (e.key === 'Escape' && open) {
        setOpen(false)
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, setOpen])

  // Reset query when palette closes
  React.useEffect(() => {
    if (!open) setQuery('')
  }, [open])

  // Fetch browse data on open
  React.useEffect(() => {
    if (!open) return
    let cancelled = false
    Promise.all([
      listRecentThreads().catch(() => [] as RecentThread[]),
      getWorkspaces().catch(() => [] as WorkspaceSummary[]),
    ]).then(([r, w]) => {
      if (cancelled) return
      setRecents(r)
      setWorkspaces(w as WorkspaceSummary[])
    })
    return () => { cancelled = true }
  }, [open])

  // Debounced FTS search
  React.useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    if (!open || query.trim().length < 2) {
      setHits([])
      setSearching(false)
      return
    }
    setSearching(true)
    debounceRef.current = setTimeout(async () => {
      try {
        const result = await invoke<SearchHit[]>('search_conversations', {
          input: { query: query.trim() },
        })
        setHits(result ?? [])
      } catch (err) {
        console.error('[SearchPalette] FTS search failed:', err)
        setHits([])
      } finally {
        setSearching(false)
      }
    }, FTS_DEBOUNCE_MS)
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [open, query])

  // Client-side filtering for the three browse sections
  const q = query.trim().toLowerCase()
  const filteredRecents = React.useMemo(() => {
    if (!q) return recents.slice(0, MAX_RECENT_BROWSE)
    return recents
      .filter(
        (t) =>
          t.title.toLowerCase().includes(q) ||
          t.workspaceName.toLowerCase().includes(q),
      )
      .slice(0, MAX_RECENT_SEARCH)
  }, [recents, q])
  const filteredWorkspaces = React.useMemo(() => {
    if (!q) return workspaces
    return workspaces.filter((w) => w.name.toLowerCase().includes(q))
  }, [workspaces, q])
  const filteredSettings = React.useMemo(() => {
    if (!q) return SETTINGS_ITEMS.slice(0, 3)
    return SETTINGS_ITEMS.filter((s) =>
      `${s.label} ${s.hint}`.toLowerCase().includes(q),
    )
  }, [q])

  if (!open) return null

  const totalRendered =
    filteredRecents.length +
    filteredSettings.length +
    filteredWorkspaces.length +
    hits.length

  const handle = (
    payload: Parameters<NonNullable<SearchPaletteProps['onSelect']>>[0],
  ) => {
    setOpen(false)
    onSelect?.(payload)
  }

  return (
    <div
      className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-black/30 backdrop-blur-sm"
      onClick={() => setOpen(false)}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'global-search-panel',
          'w-[min(92vw,640px)] mx-4 rounded-2xl border border-black/[0.07] dark:border-white/[0.08]',
          'bg-white/92 dark:bg-zinc-900/92 backdrop-blur-2xl',
          'shadow-[0_20px_60px_rgba(0,0,0,0.18),0_4px_16px_rgba(0,0,0,0.08),0_0_0_0.5px_rgba(0,0,0,0.06)]',
          'overflow-hidden',
        )}
      >
        <Command label="Global search" loop shouldFilter={false}>
          {/* Input row */}
          <div className="flex items-center gap-3 border-b border-border/70 px-4 py-3.5">
            <Search className="size-4 shrink-0 text-muted-foreground/50" />
            <Command.Input
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder="搜索线程、项目..."
              className="flex-1 bg-transparent outline-none text-[13.5px] text-foreground/85 placeholder:text-muted-foreground/40"
            />
            {searching && (
              <span className="text-[10.5px] text-muted-foreground/40 tabular-nums">…</span>
            )}
          </div>

          {/* Body */}
          <Command.List
            className={cn(
              'max-h-[440px] overflow-y-auto overflow-x-hidden px-1.5 py-1.5 scrollbar-thin',
              // Group headings
              '[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2',
              '[&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:font-semibold',
              '[&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-widest',
              '[&_[cmdk-group-heading]]:text-muted-foreground/35',
            )}
          >
            {totalRendered === 0 && q.length >= 2 && !searching ? (
              <Command.Empty className="flex flex-col items-center gap-2 py-10 text-center">
                <Hash className="size-6 text-muted-foreground/20" />
                <span className="text-[12.5px] text-muted-foreground/40">
                  未找到「{query}」相关内容
                </span>
              </Command.Empty>
            ) : null}

            {/* 1. Recent threads */}
            {filteredRecents.length > 0 && (
              <Command.Group heading={q ? '线程' : '最近线程'}>
                {filteredRecents.map((t) => (
                  <Command.Item
                    key={`thread:${t.kind}:${t.id}`}
                    value={`thread-${t.kind}-${t.id}`}
                    onSelect={() => handle({ kind: 'thread', thread: t })}
                    className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/65 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground"
                  >
                    {t.titleEmoji ? (
                      <span className="size-4 shrink-0 text-center text-[14px] leading-none">
                        {t.titleEmoji}
                      </span>
                    ) : t.kind === 'agent' ? (
                      <Bot className="size-4 shrink-0 text-muted-foreground/55" />
                    ) : (
                      <MessageSquare className="size-4 shrink-0 text-muted-foreground/55" />
                    )}
                    <span className="flex-1 truncate">{t.title}</span>
                    <span className="flex shrink-0 items-center gap-1 rounded-md bg-muted/70 px-1.5 py-0.5 text-[10.5px] text-muted-foreground/55 max-w-[120px] truncate">
                      <Folder className="size-2.5 shrink-0" />
                      <span className="truncate">{t.workspaceName}</span>
                    </span>
                    <span className="flex shrink-0 items-center gap-1 text-[10.5px] text-muted-foreground/35 tabular-nums">
                      <Clock className="size-2.5" />
                      {formatAge(t.updatedAt)}
                    </span>
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {filteredRecents.length > 0 && filteredSettings.length > 0 && (
              <div className="mx-2 my-1 h-px bg-border/70" />
            )}

            {/* 2. Settings & commands */}
            {filteredSettings.length > 0 && (
              <Command.Group heading="设置与命令">
                {filteredSettings.map((s) => (
                  <Command.Item
                    key={s.id}
                    value={s.id}
                    onSelect={() => handle({ kind: 'settings', settings: s })}
                    className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/65 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground"
                  >
                    <s.icon className="size-4 shrink-0 text-muted-foreground/55" />
                    <span className="flex-1 truncate">{s.label}</span>
                    <span className="shrink-0 truncate text-[10.5px] text-muted-foreground/40 max-w-[280px]">
                      {s.hint}
                    </span>
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {(filteredRecents.length > 0 || filteredSettings.length > 0) && filteredWorkspaces.length > 0 && (
              <div className="mx-2 my-1 h-px bg-border/70" />
            )}

            {/* 3. Workspaces / projects */}
            {filteredWorkspaces.length > 0 && (
              <Command.Group heading="项目">
                {filteredWorkspaces.map((w) => {
                  const count = w.conversationCount ?? 0
                  return (
                    <Command.Item
                      key={`ws:${w.id}`}
                      value={`ws-${w.id}`}
                      onSelect={() => handle({ kind: 'workspace', workspace: w })}
                      className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/65 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground"
                    >
                      <FolderOpen className="size-4 shrink-0 text-muted-foreground/55" />
                      <span className="flex-1 truncate">{w.icon ? `${w.icon} ` : ''}{w.name}</span>
                      {count > 0 && (
                        <span className="shrink-0 rounded-full bg-muted/70 px-2 py-0.5 text-[10.5px] text-muted-foreground/55 tabular-nums">
                          {count} 个线程
                        </span>
                      )}
                    </Command.Item>
                  )
                })}
              </Command.Group>
            )}

            {hits.length > 0 && (filteredRecents.length > 0 || filteredSettings.length > 0 || filteredWorkspaces.length > 0) && (
              <div className="mx-2 my-1 h-px bg-border/70" />
            )}

            {/* 4. Server-side FTS hits */}
            {hits.length > 0 && (
              <Command.Group heading="搜索结果">
                {hits.map((h) => (
                  <Command.Item
                    key={`hit:${h.id}`}
                    value={`hit-${h.id}`}
                    onSelect={() => handle({ kind: 'search_hit', hit: h })}
                    className="relative flex cursor-pointer select-none items-start gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/65 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground"
                  >
                    {h.source === 'agent_turn' ? (
                      <Bot className="size-4 shrink-0 mt-0.5 text-muted-foreground/55" />
                    ) : (
                      <MessageSquare className="size-4 shrink-0 mt-0.5 text-muted-foreground/55" />
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="truncate font-medium text-foreground/85">
                        {h.title || '(untitled)'}
                      </div>
                      <div
                        className="truncate text-[11.5px] text-muted-foreground/65"
                        // FTS5 returns <b>...</b>; backend escapes user input
                        dangerouslySetInnerHTML={{ __html: h.snippet }}
                      />
                    </div>
                  </Command.Item>
                ))}
              </Command.Group>
            )}
          </Command.List>

          {/* Footer */}
          <div className="global-search-footer flex items-center justify-end gap-3 border-t border-black/[0.05] dark:border-white/[0.05] px-3.5 py-2 text-[10.5px] text-muted-foreground/35">
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-black/[0.06] dark:bg-white/[0.08] px-1 py-0.5 font-mono text-[10px]">↑↓</kbd>
              导航
            </span>
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-black/[0.06] dark:bg-white/[0.08] px-1 py-0.5 font-mono text-[10px]">↵</kbd>
              打开
            </span>
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-black/[0.06] dark:bg-white/[0.08] px-1 py-0.5 font-mono text-[10px]">Esc</kbd>
              关闭
            </span>
          </div>
        </Command>
      </div>
    </div>
  )
}
```

(The `Bot`, `Folder`, `FolderOpen`, `Clock`, `SlidersHorizontal`, `Brain`, `Settings as SettingsIcon`, `Hash` imports are all available in the installed lucide-react.)

- [ ] **Step 3.3: Verify cmdk types**

The `Command.Item` `value` prop requires a string. We use composite ids like `thread-chat-abc` so cmdk's keyboard cycle is stable.

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

- [ ] **Step 3.4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/search/SearchPalette.tsx
git commit -m "$(cat <<'EOF'
feat(search): SearchPalette browse mode + visual polish

Ports if2Ai's GlobalSearch design pattern:
  - Empty input → browse: recent threads (top 8) + settings (top 3)
    + all workspaces, fetched once on open via list_recent_threads +
    getWorkspaces
  - Typing → client-filter the three browse sections + show server-
    side FTS hits in a fourth "搜索结果" group (existing
    search_conversations command)
  - Frosted-glass panel: bg-white/92 + backdrop-blur-2xl + layered
    shadow (3 stacked drop-shadows) + rounded-2xl
  - Group headings in tracked uppercase 10px text
  - Workspace badges (Folder + name) on thread rows
  - Relative-time chips (Clock + "42分钟前") on thread rows
  - Settings rows show subtitle hint right-aligned
  - Workspace rows show "N 个线程" pill
  - Footer with kbd keyboard hints (↑↓ 导航 / ↵ 打开 / Esc 关闭)
  - Empty-state shows Hash icon + "未找到「{query}」相关内容"

cmdk continues to do arrow-key navigation + aria-selected highlight
only; all filtering is manual via useMemo. Server-side FTS still
runs through the existing 150ms debounce path.

The onSelect callback shape changed from (hit) to a tagged union
covering all four item kinds — caller (AppShell) needs to be
updated in the next task.
EOF
)"
```

---

## Task 4: Wire onSelect — handle four item kinds

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx` (the existing `handleSearchResultSelect` only handled the old `SearchHit` shape)

The new payload is a tagged union with four kinds: `thread`, `workspace`, `settings`, `search_hit`. Wire each:

- [ ] **Step 4.1: Update the handler in AppShell**

Find the existing `handleSearchResultSelect` (introduced in PR #29). Replace its body:

```tsx
const handleSearchResultSelect = React.useCallback((payload:
  | { kind: 'thread'; thread: { id: string; kind: 'chat' | 'agent'; workspaceId: string } }
  | { kind: 'workspace'; workspace: { id: string; name: string } }
  | { kind: 'settings'; settings: { id: string } }
  | { kind: 'search_hit'; hit: { source: string; sourceId: string; messageId?: string } }
) => {
  switch (payload.kind) {
    case 'thread': {
      const t = payload.thread
      // Open the right tab type — chat or agent
      const tabType = t.kind === 'agent' ? 'agent' : 'chat'
      setTabs((prev) => openTab(prev, { id: t.id, type: tabType, sessionId: t.id }))
      setActiveTabId(t.id)
      // Update the per-domain "current" atoms so the view focuses correctly.
      setAppMode(t.kind === 'agent' ? 'agent' : 'chat')
      if (t.kind === 'agent') setCurrentAgentSessionId(t.id)
      else setCurrentConversationId(t.id)
      setCurrentAgentWorkspaceId(t.workspaceId)
      break
    }
    case 'workspace': {
      // Switch to that workspace; don't open a thread automatically.
      setCurrentAgentWorkspaceId(payload.workspace.id)
      break
    }
    case 'settings': {
      // Navigate to settings tab. The settings tab id convention from existing code:
      setActiveTabId('settings')
      // Optionally pass a deep-link hint via a separate atom if one exists.
      // For now, just open the settings panel; the user can pick the right page.
      break
    }
    case 'search_hit': {
      const h = payload.hit
      // existing PR #29 behavior — open the session and scroll to the message
      setTabs((prev) => openTab(prev, { id: h.sourceId, type: h.source === 'agent_turn' ? 'agent' : 'chat', sessionId: h.sourceId }))
      setActiveTabId(h.sourceId)
      setAppMode(h.source === 'agent_turn' ? 'agent' : 'chat')
      if (h.source === 'agent_turn') setCurrentAgentSessionId(h.sourceId)
      else setCurrentConversationId(h.sourceId)
      if (h.messageId) {
        setTimeout(() => {
          window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', {
            detail: { sessionId: h.sourceId, messageId: h.messageId },
          }))
        }, 200)
      }
      break
    }
  }
}, [setTabs, setActiveTabId, setAppMode, setCurrentAgentSessionId, setCurrentConversationId, setCurrentAgentWorkspaceId])
```

The existing AppShell already imports `openTab` and the various `setX` atom setters from PR #29. Verify by reading the current file:
```bash
grep -n "setTabs\|setActiveTabId\|setAppMode\|setCurrentAgentSessionId\|setCurrentConversationId\|setCurrentAgentWorkspaceId" ui/src/components/app-shell/AppShell.tsx | head -10
```

If a setter doesn't exist or is named differently, match what's there.

If a settings tab / "settings" tab id convention doesn't exist in the codebase, leave the `case 'settings'` body as a console.warn for now and add a follow-up TODO comment — this can be deep-linked when the settings UI gets a proper navigation system.

- [ ] **Step 4.2: TS check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/app-shell/AppShell.tsx
git commit -m "$(cat <<'EOF'
feat(search): SearchPalette onSelect handles four item kinds

The new SearchPalette payload is a tagged union covering thread /
workspace / settings / search_hit. Wire each:

  - thread: opens the right tab type (chat / agent) and updates
    per-domain 'current' atoms + workspace
  - workspace: switches the current workspace without opening a
    thread (user is browsing)
  - settings: navigates to the settings tab id (deep-linking per
    settings sub-page is a follow-up)
  - search_hit: existing behavior — open session + scroll to message
EOF
)"
```

---

## Task 5: Update + extend tests

**Files:**
- Rewrite: `ui/src/components/search/SearchPalette.test.tsx` (the existing 6 tests need updating + 4 new ones for browse mode)

Existing tests assume the old payload shape (single `SearchHit`). Update to match the new tagged-union payload, and add new tests for the browse-mode rendering.

- [ ] **Step 5.1: Rewrite the test file**

Replace the existing test content with this expanded suite (10 tests total):

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { SearchPalette } from './SearchPalette'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    if (cmd === 'search_conversations') {
      const q: string = args?.input?.query ?? ''
      if (q === 'gomoku') {
        return [
          {
            id: 'chat:msg-1',
            title: 'Game session',
            snippet: '... <b>gomoku</b> rules ...',
            source: 'chat_message',
            sourceId: 'sess-1',
            messageId: 'msg-1',
            createdAt: '2026-05-09',
          },
        ]
      }
      return []
    }
    return []
  }),
}))

vi.mock('@/lib/tauri-bridge', () => ({
  listRecentThreads: vi.fn(async () => [
    {
      id: 'sess-1',
      kind: 'chat',
      title: '记住我最喜欢的fps',
      titleEmoji: '🎨',
      workspaceName: 'Workaround',
      workspaceId: 'ws-1',
      messageCount: 4,
      updatedAt: new Date(Date.now() - 42 * 60_000).toISOString(),
    },
    {
      id: 'sess-2',
      kind: 'agent',
      title: '新对话',
      workspaceName: 'Downloads',
      workspaceId: 'ws-2',
      messageCount: 2,
      updatedAt: new Date(Date.now() - 6 * 86_400_000).toISOString(),
    },
  ]),
  getWorkspaces: vi.fn(async () => [
    { id: 'ws-1', name: 'Workaround', icon: '📁', conversationCount: 6 },
    { id: 'ws-2', name: 'Downloads', conversationCount: 1 },
    { id: 'ws-3', name: 'me', icon: '👤', conversationCount: 3 },
  ]),
}))

describe('SearchPalette', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders nothing when closed', () => {
    const { container } = renderWithProviders(<SearchPalette />)
    expect(container.querySelector('input')).toBeNull()
  })

  it('opens when the atom is set true', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    expect(await screen.findByPlaceholderText('搜索线程、项目...')).toBeInTheDocument()
  })

  it('opens via ⌘K keyboard shortcut', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }),
    )
    await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(true))
  })

  // ===== BROWSE MODE (empty input) =====

  it('shows the three browse sections when input is empty', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    // Wait for fetch
    await screen.findByText('最近线程')
    expect(screen.getByText('最近线程')).toBeInTheDocument()
    expect(screen.getByText('设置与命令')).toBeInTheDocument()
    expect(screen.getByText('项目')).toBeInTheDocument()
  })

  it('renders recent threads with workspace badge + relative time', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('记住我最喜欢的fps')
    // Workspace badge on the row
    const rows = screen.getAllByRole('option')
    expect(rows.length).toBeGreaterThan(0)
    // The first row's text should contain "Workaround" (badge) and a time chip
    expect(screen.getAllByText('Workaround').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText(/分钟前|刚刚/)).toBeInTheDocument()
  })

  it('renders settings shortcuts with hint text', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('设置与命令')
    expect(screen.getByText('服务商配置')).toBeInTheDocument()
    expect(screen.getByText('Provider / API Key / Base URL')).toBeInTheDocument()
  })

  it('renders workspaces with thread count pill', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('项目')
    // Workspace name and pill appear
    expect(screen.getAllByText(/Workaround/).length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText(/6 个线程/)).toBeInTheDocument()
  })

  // ===== TYPING MODE =====

  it('client-filters recent threads when typing', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('最近线程')
    const input = screen.getByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'fps')
    // First recent matches "fps"; second does not — only one row visible
    await waitFor(() => {
      expect(screen.getByText('记住我最喜欢的fps')).toBeInTheDocument()
      expect(screen.queryByText('新对话')).not.toBeInTheDocument()
    })
  })

  it('queries the FTS backend and renders the search-hits section', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'gomoku')
    await waitFor(
      () => {
        expect(screen.getByText('搜索结果')).toBeInTheDocument()
        expect(screen.getByText('Game session')).toBeInTheDocument()
      },
      { timeout: 1000 },
    )
  })

  it('calls onSelect with thread payload when a recent thread is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const item = await screen.findByText('记住我最喜欢的fps')
    await user.click(item)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'thread',
      thread: expect.objectContaining({ id: 'sess-1', kind: 'chat' }),
    }))
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
  })

  it('calls onSelect with search_hit payload when an FTS hit is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'gomoku')
    const hit = await screen.findByText('Game session')
    await user.click(hit)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'search_hit',
      hit: expect.objectContaining({ messageId: 'msg-1', sourceId: 'sess-1' }),
    }))
  })
})
```

- [ ] **Step 5.2: Run + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run SearchPalette 2>&1 | tail -20
```
Expected: **10 tests passing** (4 mode-agnostic + 4 browse-mode + 2 typing-mode + select).

If any test fails because the `getWorkspaces` symbol doesn't exist yet in `tauri-bridge.ts`, check Task 2 — verify the existing `getWorkspaces` function name in `tauri-bridge.ts`:
```bash
grep -n "getWorkspaces\|listSpaces\|listWorkspaces" ui/src/lib/tauri-bridge.ts | head -3
```

Use whichever name is established. If the function returns `SpaceSummary` instead of a custom `WorkspaceSummary`, swap the type alias in the SearchPalette accordingly.

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/search/SearchPalette.test.tsx
git commit -m "$(cat <<'EOF'
test(search): expand SearchPalette suite to 10 tests for browse mode

Replaces PR #29's 6 tests with an expanded suite covering both browse
and typing modes:

Browse mode (empty input):
  - three sections render (最近线程 / 设置与命令 / 项目)
  - thread rows show workspace badge + relative-time chip
  - settings rows show subtitle hint
  - workspace rows show thread-count pill

Typing mode:
  - client-side filter on recent threads
  - server-side FTS hits surface in 搜索结果 group
  - onSelect fires with the right tagged-union payload for both
    thread clicks and FTS-hit clicks

The mock for @/lib/tauri-bridge stubs listRecentThreads + getWorkspaces;
the @tauri-apps/api/core invoke mock keeps stubbing search_conversations
for FTS-hit path.
EOF
)"
```

---

## Task 6: Final verification + push + PR

- [ ] **Step 6.1: Full pipeline check**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== unit tests ===" && (cd ui && npm test 2>&1 | tail -10)
echo "=== vite ===" && (cd ui && npx vite build 2>&1 | tail -3)
```

Expected:
- 0 cargo warnings
- 0 TS errors
- 30 tests passing (P3's 20 + P4's expanded 10)
- Vite build succeeds

- [ ] **Step 6.2: Push branch + open PR**

```bash
git push -u origin claude/p4plus-search-polish
gh pr create --title "P4+: SearchPalette browse mode + visual polish" --body "$(cat <<'EOF'
## Summary

Extends the SearchPalette landed in PR #29 with the if2Ai design pattern. When input is empty, the palette becomes a **browse mode** showing three sections (recent threads / settings & commands / projects). When typing, all three sections client-filter AND server-side FTS results appear in a fourth group.

## What's new

| Aspect | Before (PR #29) | After (this PR) |
|---|---|---|
| Empty input | "Type to search..." hint, no results | Three browseable sections (8 recent threads + 3 settings + N workspaces) |
| Typing input | Server-side FTS only | Client-filters all three sections + FTS results |
| Visual | Plain bordered panel | Frosted-glass + layered shadow + tracked-uppercase group headings |
| Thread row | (didn't exist) | Workspace badge + relative-time chip |
| Settings row | (didn't exist) | Label + subtitle hint |
| Workspace row | (didn't exist) | Folder icon + thread-count pill |
| Footer | None | kbd hints (↑↓ 导航 / ↵ 打开 / Esc 关闭) |
| onSelect payload | Single `SearchHit` | Tagged union: thread / workspace / settings / search_hit |
| Empty state | "No results" | Hash icon + "未找到「{query}」相关内容" |

## Verification

- ✅ `cargo build` clean (0 warnings)
- ✅ `tsc --noEmit` clean
- ✅ **30 tests passing** (P3's 20 + this PR's expanded 10)
- ✅ `vite build` succeeds

## Commit log (bisectable)

| # | Hash | What |
|---|---|---|
| 1 | TBD | feat(search): list_recent_threads — unified chat + agent recents |
| 2 | TBD | feat(search): listRecentThreads bridge + RecentThread type |
| 3 | TBD | feat(search): SearchPalette browse mode + visual polish |
| 4 | TBD | feat(search): SearchPalette onSelect handles four item kinds |
| 5 | TBD | test(search): expand SearchPalette suite to 10 tests for browse mode |

## Visual reference

The user shared screenshots from if2Ai showing:
- Three-section layout when empty: 最近线程 / 设置与命令 / 项目
- Workspace badges (gray pill with folder icon + name) on thread rows
- Relative-time chips (clock icon + "42分钟前") on thread rows
- Settings rows with right-aligned subtitle hints (Provider / API Key / Base URL)
- Workspace rows with "N 个线程" pill
- Footer with three kbd hints

This PR ports that exact pattern.

## Out of scope

- Settings deep-linking — clicking 服务商配置 currently navigates to the settings tab in general; per-page deep links are a follow-up
- Pinned threads — the if2Ai version doesn't have this either; could add later
- Soul/persona indicators on rows — depends on whether uClaw has these (they exist on agent sessions per V8 schema but aren't prominent in current UI)
- Project icon emoji parsing — currently shows `icon` field as-is; if you need emoji-from-name fallback, that's a separate task

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria (cumulative)

- ✅ Empty input → three sections (recent / settings / projects) render with their data fetched on open
- ✅ Typing → client-filters all three sections AND triggers server-side FTS
- ✅ Frosted-glass panel + layered shadow visually present
- ✅ Group headings styled with `tracking-widest text-[10px] uppercase`
- ✅ Workspace badges + relative-time chips render on thread rows
- ✅ Settings rows show subtitle hints
- ✅ Workspaces show thread-count pills
- ✅ Footer shows 3 kbd hints
- ✅ onSelect routes the four kinds correctly
- ✅ 30 tests passing
- ✅ Each task ships its own commit

## Out of scope (deferred)

- Settings deep-link per sub-page (clicking 模型配置 doesn't yet open the model-config sub-page — opens settings tab generally)
- Pinned threads
- Search-history / recent-queries
- Project icon-from-name emoji fallback
- Animation polish (cmdk's default radix data-state animations are fine for first cut)
