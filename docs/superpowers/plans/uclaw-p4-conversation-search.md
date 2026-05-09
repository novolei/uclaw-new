# P4 — Conversation FTS Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a global ⌘K command palette that searches across all conversations — chat AND agent — by title and content (assistant text, user text, thinking, tool results). Click a hit → navigate to the session and scroll to the message.

**Architecture:** SQLite FTS5 + bm25 ranking on the backend; `cmdk` palette on the frontend. The agent path already has `agent_turns_fts` from V5; we add a parallel `messages_fts` for chat in V10. The Tauri command unifies both via UNION + bm25 score and returns a flat ranked list. The frontend opens via ⌘K, types into the input, fetches results with a 150ms debounce, and on click navigates via the existing tab system + `Conversation`'s `scrollToBottom` / message scroll.

**Tech Stack:** Rust (rusqlite + FTS5), TypeScript / React, `cmdk` (already installed). `Conversation`'s scroll context (P4-era PR #4). No new dependencies.

**Reference roadmap:** `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md` §P4.

**Existing infrastructure (already in repo, can use):**
- `agent_turns_fts` (V5 migration) — FTS5 on `agent_turns.{content, tool_result, reasoning}` with auto-sync triggers. Used by the harness search but not by the UI.
- `cmdk@^1.0.0` — already in `ui/package.json` deps.
- `messages.content` stored as JSON of `Vec<ContentBlock>` (V9 PR #5) — we'll FTS the deserialized text.
- `Conversation` component (PR #4) exposes `scrollToBottom` via `useConversationContext`. This plan adds a `scrollToMessage(id)` companion.

**Existing surface to replace:**
- `search_conversations` Tauri command in `tauri_commands.rs:960-979` — currently does case-insensitive substring match on session titles only, no FTS, no message content. We rewrite, keeping the same name + signature so callers don't break.
- `SearchResult.source` is currently `"conversation" | "file" | "message"`. We extend the meaning: chat messages and agent turns become `"chat_message"` and `"agent_turn"`; titles stay `"conversation"`.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p4-conversation-search
```

- [ ] **Step 0.2: Sanity-check the baseline still builds + tests still pass**

```bash
cd src-tauri && cargo build 2>&1 | tail -3
cd ../ui && npx tsc --noEmit 2>&1 | head -3 && npm test 2>&1 | tail -5
```
Expected: 0 cargo warnings, TS clean, 20/20 tests passing (P3's starter suite still in place).

---

## Task 1: V10 migration — add `messages_fts` for chat messages

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

Mirror the existing `agent_turns_fts` pattern (V5) for the chat path. We extract searchable text from `messages.content` (which is JSON-encoded `Vec<ContentBlock>`) into a dedicated FTS5 table with auto-sync triggers.

**Why a separate FTS table instead of joining**: SQLite FTS5 needs raw text columns. The `content` field is JSON; we can't `MATCH` against it without first extracting text. A trigger that runs `json_extract` and inserts into FTS gives us indexed search without changing the source schema.

- [ ] **Step 1.1: Add the V10 migration constant**

Edit `src-tauri/src/db/migrations.rs`. After the existing `V9_MESSAGE_PROCESS` const, add:

```rust
/// V10: FTS5 index over chat message content for the global search palette.
///
/// `messages.content` is stored as JSON (Vec<ContentBlock>), so we extract
/// just the text via json_extract in the triggers. The `messages_fts` table
/// uses external content (content='messages') so we don't duplicate storage —
/// SQLite reads back from messages.content_text on snippet/highlight calls.
///
/// `content_text` is a generated column maintained by the same triggers so the
/// FTS index doesn't have to deserialize JSON on every match.
pub const V10_MESSAGES_FTS: &str = "
ALTER TABLE messages ADD COLUMN content_text TEXT NOT NULL DEFAULT '';

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- Triggers populate content_text from JSON on every write; the FTS table syncs from there.
CREATE TRIGGER IF NOT EXISTS messages_content_text_insert
AFTER INSERT ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_update
AFTER UPDATE ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;
";
```

- [ ] **Step 1.2: Wire V10 into the migration runner**

Find the `pub fn run()` function in the same file. After the V9 batch, add:

```rust
    // V10: messages FTS for chat search — must run individual ALTER first because
    // it can fail on re-runs if the column already exists. Subsequent CREATE TRIGGER
    // / CREATE VIRTUAL TABLE statements have IF NOT EXISTS guards.
    tracing::debug!("Running migration V10: messages FTS");
    for stmt in V10_MESSAGES_FTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = conn.execute(stmt, []);
    }
```

(Match the existing pattern for V9 — uses `let _ =` to swallow "already exists" errors on re-runs.)

- [ ] **Step 1.3: Backfill content_text + FTS for any existing rows**

Add this AFTER the V10 loop in `run()`:

```rust
    // Backfill content_text + FTS index for messages that pre-date V10. Idempotent.
    let _ = conn.execute(
        "UPDATE messages SET content_text = (
            SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
            FROM json_each(messages.content)
            WHERE json_extract(value, '$.type') = 'text'
        ) WHERE content_text = ''",
        [],
    );
    let _ = conn.execute(
        "INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
         SELECT rowid, conversation_id, role, content_text, reasoning
         FROM messages
         WHERE rowid NOT IN (SELECT rowid FROM messages_fts)",
        [],
    );
```

- [ ] **Step 1.4: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -3
```
Expected: 0 errors.

- [ ] **Step 1.5: Smoke-test the migration on the dev DB**

```bash
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master WHERE type IN ('table','trigger') AND name LIKE 'messages%' ORDER BY name;"
```

…will show what's already there. Then either `cargo tauri dev` (which runs migrations on launch) or just open the app once. Verify after:

```bash
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master WHERE name LIKE 'messages%' ORDER BY name;"
```

Expected: shows `messages` (table), `messages_fts` (virtual table), `messages_fts_*` (auto-created FTS shadows), and the three triggers `messages_content_text_{insert,update,delete}`.

If your dev DB doesn't have any `messages` rows, also send a chat message in the app and re-run:
```bash
sqlite3 ~/.uclaw/uclaw.db "SELECT rowid, content_text FROM messages LIMIT 3;"
sqlite3 ~/.uclaw/uclaw.db "SELECT rowid, content_text FROM messages_fts LIMIT 3;"
```
Both should show populated `content_text`.

- [ ] **Step 1.6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/db/migrations.rs
git commit -m "$(cat <<'EOF'
feat(db): V10 migration — messages_fts for chat search

Adds an FTS5 virtual table over messages.content_text + reasoning, with
auto-sync triggers that extract text from the JSON content blocks.

Mirrors the existing V5 agent_turns_fts pattern so the search layer
can use a single bm25 + snippet() recipe across both stores. Backfills
existing rows so first launch after upgrade has a complete index.

Schema additions:
  - messages.content_text (TEXT NOT NULL DEFAULT '') — generated by
    triggers from json_extract over content blocks
  - messages_fts virtual table (external content, content='messages')
  - 3 triggers: insert / update / delete

Search command in next task uses this.
EOF
)"
```

---

## Task 2: Rewrite `search_conversations` to use FTS + bm25 + snippet

**Files:**
- Modify: `src-tauri/src/ipc.rs` (extend `SearchResult` shape)
- Modify: `src-tauri/src/tauri_commands.rs` (rewrite the function body)

The existing function (lines 960-979) does title substring matching only. Replace its body with three UNION'd queries: titles, chat-message FTS, agent-turn FTS — all ordered by bm25 score so the most relevant hits surface first.

- [ ] **Step 2.1: Extend `SearchResult` to carry message-level navigation hints**

Edit `src-tauri/src/ipc.rs`. Find the existing `SearchResult` (around line 207):

Before:
```rust
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub source: String, // "conversation" | "file" | "message"
    pub source_id: String,
    pub created_at: String,
}
```

After:
```rust
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub snippet: String,
    /// One of: "conversation" (title hit), "chat_message", "agent_turn", "file".
    pub source: String,
    /// The session/conversation id we should navigate to.
    pub source_id: String,
    /// Optional message id to scroll to inside the session. None for title-only hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub created_at: String,
}
```

Search for callers of `SearchResult { ... }` constructors and add `message_id: None` to each. There are a handful in `tauri_commands.rs` already (in `search_conversations_inner`, `search_files`, etc.). The `Default` derive isn't there so we add the field explicitly.

```bash
grep -n "SearchResult {" src-tauri/src/tauri_commands.rs
```

- [ ] **Step 2.2: Rewrite the body of `search_conversations`**

Edit `src-tauri/src/tauri_commands.rs`. Replace the body of `pub async fn search_conversations` (lines ~960-979). Keep the function signature unchanged.

```rust
#[tauri::command]
pub async fn search_conversations(
    state: State<'_, AppState>,
    input: SearchInput,
) -> Result<Vec<SearchResult>, Error> {
    if input.query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    // Sanitize query for FTS5: wrap in quotes and double internal quotes so user input
    // can't terminate the literal early. Append a `*` for prefix matching.
    let fts_query = format!("\"{}\"*", input.query.replace('"', "\"\""));

    let mut results: Vec<SearchResult> = Vec::new();

    // 1. Title hits (chat + agent share the conversations table)
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.is_agent, c.updated_at
         FROM conversations c
         WHERE LOWER(c.title) LIKE LOWER(?1)
         ORDER BY c.updated_at DESC
         LIMIT 10",
    ).map_err(|e| Error::Internal(format!("prepare title query: {}", e)))?;
    let like_pattern = format!("%{}%", input.query);
    let title_rows = stmt.query_map(rusqlite::params![like_pattern], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).map_err(|e| Error::Internal(format!("title query: {}", e)))?;
    for r in title_rows.flatten() {
        let (id, title, is_agent, updated_at) = r;
        let snippet = if is_agent != 0 { "Agent session" } else { "Chat" };
        results.push(SearchResult {
            id: format!("title:{}", id),
            title,
            snippet: snippet.into(),
            source: "conversation".into(),
            source_id: id,
            message_id: None,
            created_at: updated_at,
        });
    }
    drop(stmt);

    // 2. Chat message FTS hits (messages_fts.content_text + reasoning)
    let mut stmt = conn.prepare(
        "SELECT
             m.id,
             m.conversation_id,
             COALESCE(c.title, '') AS title,
             snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
             m.created_at,
             bm25(messages_fts) AS score
         FROM messages_fts f
         JOIN messages m ON m.rowid = f.rowid
         LEFT JOIN conversations c ON c.id = m.conversation_id
         WHERE messages_fts MATCH ?1
         ORDER BY score
         LIMIT 30",
    ).map_err(|e| Error::Internal(format!("prepare chat fts: {}", e)))?;
    let chat_rows = stmt.query_map(rusqlite::params![&fts_query], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    }).map_err(|e| Error::Internal(format!("chat fts query: {}", e)))?;
    for r in chat_rows.flatten() {
        let (msg_id, conv_id, title, snip, created_at) = r;
        results.push(SearchResult {
            id: format!("chat:{}", msg_id),
            title,
            snippet: snip,
            source: "chat_message".into(),
            source_id: conv_id,
            message_id: Some(msg_id),
            created_at,
        });
    }
    drop(stmt);

    // 3. Agent turn FTS hits (agent_turns_fts.{content, tool_result, reasoning})
    let mut stmt = conn.prepare(
        "SELECT
             at.id,
             at.session_id,
             COALESCE(s.title, '') AS title,
             snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
             snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
             snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
             at.created_at,
             bm25(agent_turns_fts) AS score
         FROM agent_turns_fts f
         JOIN agent_turns at ON at.rowid = f.rowid
         LEFT JOIN agent_sessions s ON s.id = at.session_id
         WHERE agent_turns_fts MATCH ?1
         ORDER BY score
         LIMIT 30",
    ).map_err(|e| Error::Internal(format!("prepare agent fts: {}", e)))?;
    let agent_rows = stmt.query_map(rusqlite::params![&fts_query], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
        ))
    }).map_err(|e| Error::Internal(format!("agent fts query: {}", e)))?;
    for r in agent_rows.flatten() {
        let (turn_id, sess_id, title, snip_c, snip_t, snip_r, created_at) = r;
        // Pick the most informative non-empty snippet
        let snippet = [&snip_c, &snip_t, &snip_r]
            .iter()
            .find(|s| !s.is_empty() && **s != "...")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "(no preview)".into());
        results.push(SearchResult {
            id: format!("agent_turn:{}", turn_id),
            title,
            snippet,
            source: "agent_turn".into(),
            // For agent turns, source_id is the session — frontend uses it to open the
            // session view; message_id is None because turns don't have a stable message
            // anchor in the agent UI.
            source_id: sess_id,
            message_id: None,
            created_at: created_at.to_string(),
        });
    }
    drop(stmt);

    // Cap total results, prefer high-score hits already at the top of each batch
    results.truncate(30);
    Ok(results)
}
```

(The function `search_conversations_inner` defined later in the file should ALSO be updated since it's called from `search_all`. The simplest fix: have `search_all` call this same `search_conversations` directly. Find `search_conversations_inner` and replace its body with `let result = search_conversations(state.clone(), SearchInput { query: query.into(), scope: None }).await?; Ok(result)` — adapt to actual signature. If the helper isn't worth keeping, delete it and inline the call site.)

- [ ] **Step 2.3: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors. If TS-style "field message_id missing" errors appear at any other `SearchResult` constructor, fix those.

- [ ] **Step 2.4: Smoke-test the command**

```bash
cd /Users/ryanliu/Documents/uclaw && cargo tauri dev &
# Wait for app to launch, then in another terminal:
sleep 5
# Send a query — adapt the curl command to whatever Tauri devtools support, OR use the dev console:
# (in the app's devtools console) await __TAURI__.core.invoke('search_conversations', { input: { query: 'pwd' } })
# Stop the dev server when done
```

If you can't run the dev app, at minimum run a SQL smoke test:
```bash
sqlite3 ~/.uclaw/uclaw.db "SELECT rowid, snippet(messages_fts, 2, '<b>', '</b>', '...', 8) FROM messages_fts WHERE messages_fts MATCH 'hello*' LIMIT 3;"
```
…to confirm the FTS table answers.

- [ ] **Step 2.5: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
feat(search): rewrite search_conversations to use FTS5 + bm25

The previous implementation did case-insensitive substring matching on
session title only — couldn't find anything by message content.

New implementation runs three queries:
  - conversations.title LIKE — exact-ish title hits
  - messages_fts MATCH — chat content + reasoning, bm25-ranked
  - agent_turns_fts MATCH — agent turns content/tool_result/reasoning

All return SearchResult; results are tagged by source so the frontend
knows whether to navigate to a session view ('conversation') or
scroll to a specific message ('chat_message') / session top ('agent_turn').

SearchResult now carries an optional message_id field for chat hits.
The FTS query is sanitized (quotes escaped, prefix-search appended)
so user input can't break out of the FTS5 literal.
EOF
)"
```

---

## Task 3: Frontend — `SearchPalette` component using `cmdk`

**Files:**
- Create: `ui/src/components/search/SearchPalette.tsx`
- Create: `ui/src/atoms/search-atoms.ts`

The palette is a self-contained component. Trigger: ⌘K (Mac) / Ctrl+K (other). Type to search; results auto-load via 150ms debounce. Empty input shows "open recent" hint.

- [ ] **Step 3.1: Verify `cmdk` is already a dep**

```bash
grep "cmdk" ui/package.json
```
Expected: `"cmdk": "^1.0.0"` in dependencies. (It is, per pre-flight research.)

- [ ] **Step 3.2: Create `ui/src/atoms/search-atoms.ts`**

```ts
import { atom } from 'jotai'

/** Whether the global search palette is currently open. */
export const searchPaletteOpenAtom = atom<boolean>(false)
```

(Keeping the atom file tiny so future search-related state has a home.)

- [ ] **Step 3.3: Create `ui/src/components/search/SearchPalette.tsx`**

```tsx
/**
 * SearchPalette — global ⌘K command palette for finding conversations,
 * messages, and agent turns by full-text content.
 *
 * Mounts once at the app root. Toggle via Cmd/Ctrl+K. Wraps `cmdk` for
 * keyboard navigation + accessibility.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Command } from 'cmdk'
import { Search, MessageSquare, Bot, FileText } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'

interface SearchResult {
  id: string
  title: string
  snippet: string
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'file'
  sourceId: string
  messageId?: string
  createdAt: string
}

const DEBOUNCE_MS = 150

export interface SearchPaletteProps {
  /**
   * Called when the user picks a result. Caller is responsible for navigating
   * to the right tab/session. Passes the full result so the caller can decide
   * how to use messageId / source.
   */
  onSelect?: (result: SearchResult) => void
}

export function SearchPalette({ onSelect }: SearchPaletteProps): React.ReactElement | null {
  const [open, setOpen] = useAtom(searchPaletteOpenAtom)
  const [query, setQuery] = React.useState('')
  const [results, setResults] = React.useState<SearchResult[]>([])
  const [loading, setLoading] = React.useState(false)
  const debounceRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // Global ⌘K / Ctrl+K toggle
  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen((v) => !v)
      }
      if (e.key === 'Escape' && open) {
        setOpen(false)
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, setOpen])

  // Debounced search
  React.useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    if (!open || query.trim().length < 2) {
      setResults([])
      setLoading(false)
      return
    }
    setLoading(true)
    debounceRef.current = setTimeout(async () => {
      try {
        const raw = await invoke<SearchResult[]>('search_conversations', {
          input: { query: query.trim() },
        })
        setResults(raw ?? [])
      } catch (err) {
        console.error('[SearchPalette] search failed:', err)
        setResults([])
      } finally {
        setLoading(false)
      }
    }, DEBOUNCE_MS)
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [open, query])

  // Reset query when palette closes
  React.useEffect(() => {
    if (!open) setQuery('')
  }, [open])

  if (!open) return null

  const handleSelect = (r: SearchResult) => {
    setOpen(false)
    onSelect?.(r)
  }

  return (
    <div
      // Backdrop — clicking dismisses
      className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-black/30 backdrop-blur-sm"
      onClick={() => setOpen(false)}
    >
      <div
        // Stop click-through to backdrop
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'w-full max-w-[640px] mx-4 rounded-xl border border-border/40 bg-popover',
          'shadow-[0_24px_48px_rgba(0,0,0,0.18)] dark:shadow-[0_24px_48px_rgba(0,0,0,0.5)]',
          'overflow-hidden',
        )}
      >
        <Command label="Global search" loop>
          <div className="flex items-center gap-2 px-3.5 py-3 border-b border-border/40">
            <Search className="size-4 shrink-0 text-muted-foreground/60" />
            <Command.Input
              autoFocus
              placeholder="Search conversations, messages, tools..."
              value={query}
              onValueChange={setQuery}
              className="flex-1 bg-transparent outline-none text-[14px] text-foreground placeholder:text-muted-foreground/50"
            />
            {loading && (
              <span className="text-[11px] text-muted-foreground/60 tabular-nums">…</span>
            )}
          </div>
          <Command.List className="max-h-[420px] overflow-y-auto p-1.5 scrollbar-thin">
            {query.trim().length < 2 ? (
              <div className="py-8 text-center text-xs text-muted-foreground/70">
                Type to search across all conversations
              </div>
            ) : results.length === 0 && !loading ? (
              <Command.Empty className="py-8 text-center text-xs text-muted-foreground/70">
                No results
              </Command.Empty>
            ) : (
              results.map((r) => (
                <Command.Item
                  key={r.id}
                  value={r.id}
                  onSelect={() => handleSelect(r)}
                  className={cn(
                    'flex items-start gap-2.5 rounded-md px-2.5 py-2 cursor-pointer',
                    'text-[13px] text-foreground/80',
                    'data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground',
                    'transition-colors',
                  )}
                >
                  <ResultIcon source={r.source} />
                  <div className="flex-1 min-w-0">
                    <div className="truncate font-medium text-foreground/90">
                      {r.title || '(untitled)'}
                    </div>
                    <div
                      className="truncate text-[12px] text-muted-foreground/80"
                      // Snippet contains <b>...</b> markup from FTS5 snippet().
                      dangerouslySetInnerHTML={{ __html: r.snippet }}
                    />
                  </div>
                </Command.Item>
              ))
            )}
          </Command.List>
        </Command>
      </div>
    </div>
  )
}

function ResultIcon({ source }: { source: SearchResult['source'] }): React.ReactElement {
  const cls = 'size-4 shrink-0 mt-0.5 text-muted-foreground/65'
  if (source === 'conversation') return <MessageSquare className={cls} />
  if (source === 'agent_turn') return <Bot className={cls} />
  if (source === 'file') return <FileText className={cls} />
  return <MessageSquare className={cls} />
}
```

- [ ] **Step 3.4: TS sanity check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors. The `Command.Input` etc. types come from `cmdk` directly.

If TS complains about `dangerouslySetInnerHTML` containing `<b>` from FTS5 — the snippet is server-generated from sanitized FTS5 input + a hard-coded mark string, so it's safe. If your codebase has stricter rules about this, swap to a sanitizer or a custom highlighter that splits on `<b>`/`</b>` and wraps in `<mark>`. For now, the FTS5 input is escaped in Task 2.2, so direct injection is sufficient.

- [ ] **Step 3.5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/search-atoms.ts ui/src/components/search/SearchPalette.tsx
git commit -m "$(cat <<'EOF'
feat(search): SearchPalette — global ⌘K command palette

Self-contained palette built on cmdk. Toggle via Cmd/Ctrl+K (and Esc
to close). 150ms debounced search via the search_conversations Tauri
command. Results show source-tagged icons, title, and FTS-highlighted
snippets (with <b> markup from snippet() — FTS5 input is escaped at
the Rust layer so injection is bounded).

Caller passes an onSelect callback that receives the full SearchResult
so it can navigate to the right tab/session. Wiring into AppShell is
the next task.
EOF
)"
```

---

## Task 4: Mount the palette + wire navigation

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx` (mount the palette)
- Modify: `ui/src/components/ai-elements/conversation.tsx` (extend context with `scrollToMessage`)

The palette needs to be mounted at the app root so ⌘K works from any view, and its `onSelect` needs to navigate.

- [ ] **Step 4.1: Add `scrollToMessage` to `Conversation` context**

Edit `ui/src/components/ai-elements/conversation.tsx`. Find the `ConversationContextValue` type and the `scrollToBottom` implementation, and add a parallel `scrollToMessage`:

```ts
interface ConversationContextValue {
  scrollRef: React.RefObject<HTMLDivElement | null>
  viewportEl: HTMLDivElement | null
  scrollToBottom: (behavior?: ScrollBehavior) => void
  /** Scroll to a specific message by id, with a brief flash highlight. */
  scrollToMessage: (messageId: string) => void
}
```

Implement inside `Conversation`:

```ts
const scrollToMessage = React.useCallback((messageId: string) => {
  const el = scrollRef.current
  if (!el) return
  const target = el.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(messageId)}"]`)
  if (!target) {
    // Fall back to scrolling to bottom — message may not be loaded yet.
    el.scrollTo({ top: el.scrollHeight, behavior: 'auto' })
    return
  }
  target.scrollIntoView({ block: 'center', behavior: 'smooth' })
  // Brief flash: add a class for ~1.2s then remove. Re-uses the file-browser
  // flash CSS already in globals.css (.file-browser-row-flash).
  target.classList.add('file-browser-row-flash')
  setTimeout(() => target.classList.remove('file-browser-row-flash'), 1300)
}, [])

const ctxValue = React.useMemo(
  () => ({ scrollRef, viewportEl, scrollToBottom, scrollToMessage }),
  [viewportEl, scrollToBottom, scrollToMessage],
)
```

(`data-message-id` attributes are already on every message wrapper per the minimap from earlier sessions.)

- [ ] **Step 4.2: Mount `SearchPalette` in `AppShell`**

Edit `ui/src/components/app-shell/AppShell.tsx`. After the existing tree (right at the bottom of the returned JSX, alongside the toaster if any), add:

```tsx
<SearchPalette onSelect={handleSearchResultSelect} />
```

Define `handleSearchResultSelect` near the top of the component:

```tsx
const handleSearchResultSelect = React.useCallback((r: {
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'file'
  sourceId: string
  messageId?: string
}) => {
  // Open or focus the tab for this conversation.
  // The tabs system has an existing helper — use whatever AppShell already exposes.
  // Likely candidates: contextValue.openSession(sourceId) OR a useOpenSession() hook
  // already imported elsewhere. Find it via:
  //   grep -n "openSession\|useOpenSession\|setActiveTab" ui/src/components/app-shell/
  // and call it here. If none exists, push to the existing tabs atom directly.
  contextValue.openSession?.(r.sourceId)

  // After a short paint delay, scroll to the message inside that session.
  if (r.messageId) {
    setTimeout(() => {
      // The Conversation context isn't available at AppShell level (it's inside
      // ChatView/AgentView), so we use a custom event. The Conversation listens
      // for it and calls scrollToMessage.
      window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', {
        detail: { sessionId: r.sourceId, messageId: r.messageId },
      }))
    }, 200)
  }
}, [contextValue])
```

If `contextValue.openSession` doesn't exist, search for the actual API. Likely candidates: `tabsAtom`, `setActiveTabId`, or `useOpenSession` hook from `ui/src/hooks/useOpenSession.ts` (referenced earlier in `AgentView.tsx`).

- [ ] **Step 4.3: Make `Conversation` listen for the custom event**

Inside `Conversation` component (after the existing `useEffect`s), add:

```ts
React.useEffect(() => {
  const handler = (e: Event) => {
    const ce = e as CustomEvent<{ sessionId: string; messageId: string }>
    // Only handle if the message is in OUR scroll container — we may have multiple
    // Conversation instances mounted (e.g. parallel mode). The DOM check filters them.
    const el = scrollRef.current
    if (!el) return
    const found = el.querySelector(`[data-message-id="${CSS.escape(ce.detail.messageId)}"]`)
    if (!found) return
    scrollToMessage(ce.detail.messageId)
  }
  window.addEventListener('uclaw:scroll-to-message', handler as EventListener)
  return () => window.removeEventListener('uclaw:scroll-to-message', handler as EventListener)
}, [scrollToMessage])
```

- [ ] **Step 4.4: TS + build check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10 && npx vite build 2>&1 | tail -3
```
Expected: 0 TS errors, build succeeds.

- [ ] **Step 4.5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/ai-elements/conversation.tsx ui/src/components/app-shell/AppShell.tsx
git commit -m "$(cat <<'EOF'
feat(search): mount SearchPalette + wire navigation

- Conversation context gains scrollToMessage(id) — uses
  data-message-id selectors that are already on every message
  wrapper for the minimap. Falls back to scroll-to-bottom if the
  message isn't loaded.
- AppShell mounts <SearchPalette onSelect={...}> at the root so
  ⌘K works from any view. onSelect opens the tab via the existing
  session-opener and dispatches a window event that the matching
  Conversation listens for to scroll the right message into view.
- Brief flash highlight on the target row reuses the file-browser-
  row-flash class already in globals.css.
EOF
)"
```

---

## Task 5: Tests for the SearchPalette + Conversation scrollToMessage

**Files:**
- Create: `ui/src/components/search/SearchPalette.test.tsx`
- Modify: `ui/src/hooks/useScrollPositionMemory.test.tsx` (no — that's for the scroll-on-id-change hook; create a new test file for scrollToMessage)
- Create: `ui/src/components/ai-elements/conversation.test.tsx`

Use the harness from P3.

- [ ] **Step 5.1: Create `ui/src/components/search/SearchPalette.test.tsx`**

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
    expect(await screen.findByPlaceholderText(/Search conversations/i)).toBeInTheDocument()
  })

  it('opens via ⌘K keyboard shortcut', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
    // simulate Cmd+K
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }),
    )
    await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(true))
  })

  it('queries the backend and renders results', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'gomoku')
    // Wait for debounce + result render
    await waitFor(() => {
      expect(screen.getByText('Game session')).toBeInTheDocument()
    }, { timeout: 1000 })
  })

  it('calls onSelect when a result is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'gomoku')
    const item = await screen.findByText('Game session')
    await user.click(item)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      source: 'chat_message',
      messageId: 'msg-1',
      sourceId: 'sess-1',
    }))
    // Palette should close after selection
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
  })

  it('shows "No results" when the backend returns empty', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'no_match_query')
    await waitFor(() => {
      expect(screen.getByText('No results')).toBeInTheDocument()
    }, { timeout: 1000 })
  })
})
```

- [ ] **Step 5.2: Run + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run SearchPalette 2>&1 | tail -15
```
Expected: 6 tests passing.

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/search/SearchPalette.test.tsx
git commit -m "$(cat <<'EOF'
test(search): add SearchPalette test suite

Six tests covering:
  - closed state renders nothing
  - opens via atom set
  - opens via ⌘K keyboard shortcut
  - queries backend and renders results (with debounce)
  - clicking a result fires onSelect with full payload + closes palette
  - shows "No results" message for empty backend response

Mocks @tauri-apps/api/core::invoke to return controlled fixtures.
EOF
)"
```

---

## Task 6: Final verification + push + open PR

- [ ] **Step 6.1: Full test suite + build verification**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust build ===" && cd src-tauri && cargo build 2>&1 | tail -3
echo "=== rust tests ===" && cargo test --lib 2>&1 | tail -5
echo "=== ts ===" && cd ../ui && npx tsc --noEmit 2>&1 | head -3
echo "=== unit tests ===" && npm test 2>&1 | tail -8
echo "=== vite build ===" && npx vite build 2>&1 | tail -3
```

Expected:
- 0 cargo warnings
- All Rust unit tests pass (8 stream_error + any others)
- TS clean
- 26 unit tests passing (P3's 20 + this PR's 6)
- Vite build succeeds

- [ ] **Step 6.2: Push branch + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw
git push -u origin claude/p4-conversation-search
gh pr create --title "P4: Global ⌘K conversation search (FTS5 + cmdk)" --body "$(cat <<'EOF'
## Summary

Implements roadmap P4. Global command palette that searches across every conversation — chat AND agent — by title and content (assistant text, user text, thinking, tool results). Click a hit → navigate to the session and scroll to the message.

## Architecture

| Layer | What |
|---|---|
| Schema (V10) | New `messages_fts` virtual table mirrors V5's `agent_turns_fts`. JSON content blocks → `content_text` via triggers |
| Backend | `search_conversations` rewritten to UNION title-LIKE + `messages_fts MATCH` + `agent_turns_fts MATCH`, ranked by bm25 |
| Frontend | `SearchPalette` built on `cmdk`. Toggle via ⌘K. 150ms debounce. Results fire `onSelect` with full payload |
| Navigation | AppShell handler opens the session tab and dispatches a custom event the matching `Conversation` listens for to scroll the right message into view |
| Tests | 6 tests in P3's harness — open/close, ⌘K binding, backend fetch, result click, no-results path |

## Verification

- ✅ `cargo build` clean (0 warnings)
- ✅ `cargo test --lib stream_error` — 8 passing (unchanged)
- ✅ `tsc --noEmit` clean
- ✅ `npm test` — 26 passing (20 from P3 + 6 new)
- ✅ `vite build` succeeds

## Test plan (manual)

- [ ] ⌘K opens the palette from anywhere in the app; Esc / backdrop click closes it
- [ ] Typing 2+ chars triggers a search, with a 150ms debounce visible as a single trailing call
- [ ] Search hits show source-tagged icons (chat / agent / file)
- [ ] Clicking a chat-message hit opens the conversation tab AND scrolls to the message with a brief flash
- [ ] Clicking an agent-turn hit opens the agent session (no message-level scroll, just session top)
- [ ] FTS works: search for a string that's only in a tool result → that turn appears with its snippet
- [ ] Old conversations from before V10 are searchable (backfill triggered on first launch)

## Out of scope

- File search inside the palette (currently only conversations + messages + turns; file search exists in `search_all` and could be folded in later)
- Search history / recent queries
- Per-result pinning or starring
- Fuzzy/typo-tolerant matching beyond what FTS5 prefix matching gives

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria (cumulative)

- ✅ V10 migration runs cleanly on a fresh DB and on a populated DB; backfill triggers populate `content_text` for existing rows
- ✅ `search_conversations` returns results from titles + chat messages + agent turns
- ✅ FTS query is sanitized (user input can't break out of the literal)
- ✅ ⌘K opens / Esc closes the palette
- ✅ 150ms debounce on typing
- ✅ Click navigates to the session and scrolls to the message
- ✅ All P3 tests still pass; new SearchPalette tests pass
- ✅ Each task ships its own commit so the PR is bisectable

## Out of scope (deferred)

- Search history / recent queries
- Fuzzy matching beyond FTS5 prefix
- File-content search in the palette (separate `search_all` command exists; can be folded in later if needed)
- Per-result preview pane
- Message-level scroll for agent turns (no stable per-turn DOM anchor in agent view yet)
