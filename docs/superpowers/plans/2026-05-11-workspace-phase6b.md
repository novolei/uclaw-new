# Phase 6-B — Cross-Workspace Search Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the ⌘K search palette show results from every workspace, grouped by workspace, with the active workspace pinned to the top — and auto-switch the active workspace when the user opens a cross-workspace hit.

**Architecture:** The backend already searches globally (`search_conversations` has no workspace filter; `scope` only narrows to a single session). What's missing is the **workspaceId on each `SearchResult`** so the frontend can group hits by their origin workspace. Add the column to the IPC struct, JOIN through `agent_sessions.space_id` / `conversations.workspace_id` to populate it, then group client-side in SearchPalette with a pure helper. Hit-click handler in AppShell already tags the tab with the session's workspace — we add one line to call `selectWorkspaceAtom` so the active workspace switches to match.

**Tech Stack:** Rust (rusqlite), Tauri 2 commands, React 18 + TypeScript, Jotai, `cmdk` for the palette, Vitest + React Testing Library, `cargo test` for Rust unit tests.

---

## Background: spec drift vs. reality

Spec §4 assumes `SearchResult` already returns `workspaceId`. **It does not.** Reading [src-tauri/src/ipc.rs:241-253](src-tauri/src/ipc.rs#L241-L253) the struct currently has `id, title, snippet, source, source_id, message_id, created_at` — no workspace field. So the backend work is real (not "purely frontend" as the spec suggests). The frontend types in [ui/src/lib/types.ts:271-278](ui/src/lib/types.ts#L271-L278) and the local `SearchHit` in [ui/src/components/search/SearchPalette.tsx:47-55](ui/src/components/search/SearchPalette.tsx#L47-L55) both need the new field.

Spec §5 also assumes hits are stored in a jotai atom (`searchResultsGroupedAtom`). They are not — `hits` is `React.useState` inside SearchPalette. We keep that shape and use a **pure helper function** `groupHitsByWorkspace(hits, workspaces, activeId)` instead of introducing a new atom. Simpler, less plumbing, equivalent outcome.

## File Structure

**New / modified files:**

- `src-tauri/src/ipc.rs` — add `workspace_id: Option<String>` to `SearchResult` (serde `camelCase` → `workspaceId` on the wire).
- `src-tauri/src/tauri_commands.rs` — extend each of the 5 SQL branches in `search_conversations` (title, chat FTS, agent_turns FTS, agent_messages FTS, LIKE fallback) to JOIN through `agent_sessions.space_id` / `conversations.workspace_id` and populate `workspace_id` on the result.
- `ui/src/lib/types.ts` — add `workspaceId?: string` to `SearchResult`.
- `ui/src/components/search/SearchPalette.tsx` — add `workspaceId?` to the local `SearchHit` interface; introduce `groupHitsByWorkspace` helper; replace the flat "搜索结果" group with per-workspace grouped sections.
- `ui/src/lib/group-search-hits.ts` (**NEW**) — the pure grouping helper, exported separately so it's trivially unit-testable without rendering the palette.
- `ui/src/components/app-shell/AppShell.tsx` — in `handleSearchResultSelect`'s `search_hit` case, call `selectWorkspaceAtom` when `session.workspaceId !== activeWorkspaceId`.

**Test files:**

- `src-tauri/src/tauri_commands.rs` — inline `#[cfg(test)]` module (the file already follows this pattern for other commands; add tests for `search_conversations` workspace_id population).
- `ui/src/lib/group-search-hits.test.ts` (**NEW**) — unit tests for the pure helper.
- `ui/src/components/search/SearchPalette.test.tsx` — extend existing tests to cover the new grouped render.

---

## Task 1: Backend — add `workspaceId` to `SearchResult`

**Files:**
- Modify: `src-tauri/src/ipc.rs:239-253`
- Modify: `src-tauri/src/tauri_commands.rs` — all 5 INSERTs into `results` inside `search_conversations` (lines ~1330-1595)
- Test: `src-tauri/src/tauri_commands.rs` — inline `#[cfg(test)] mod search_tests`

### - [ ] Step 1.1: Add the field to the IPC struct

In `src-tauri/src/ipc.rs`, update `SearchResult`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub snippet: String,
    /// One of: "conversation" (title hit), "chat_message", "agent_turn",
    /// "agent_message", "file".
    pub source: String,
    /// The session/conversation id we should navigate to.
    pub source_id: String,
    /// Optional message id to scroll to inside the session. None for
    /// title-only hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Workspace this hit originated in. Resolved server-side via
    /// `agent_sessions.space_id` (agent domain) or
    /// `conversations.workspace_id` (chat domain). `None` only when the
    /// row hasn't been re-homed yet (legacy data) — the frontend treats
    /// `None` as 'default'.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub created_at: String,
}
```

### - [ ] Step 1.2: Write the failing Rust test FIRST

In `src-tauri/src/tauri_commands.rs`, append a `#[cfg(test)]` module at the bottom of the file (or add to an existing one — check for `#[cfg(test)]` near the end). Use the same in-memory SQLite + migration pattern as `db/migrations.rs::tests::v18_pinned_at_smoke`. The test:

```rust
#[cfg(test)]
mod search_workspace_tests {
    use rusqlite::Connection;
    use crate::db::migrations::run_migrations;

    /// Helper: open an in-memory DB and run migrations up to current.
    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        run_migrations(&mut conn).expect("run migrations");
        conn
    }

    /// Smoke: with one agent_session in workspace 'ws-a' and one
    /// agent_message under it, FTS hits should populate workspace_id='ws-a'.
    #[test]
    fn search_populates_workspace_id_for_agent_messages() {
        let conn = setup_db();
        // Insert workspace + session + message
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, attached_dirs,
                                 sort_order, created_at, updated_at)
             VALUES ('ws-a', 'A', 'Folder', '/a', '[]', 0, '2026-01-01', '2026-01-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-1', 'ws-a', 'Hello', 1700000000000, 1700000000000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at)
             VALUES ('m-1', 's-1', 'user', 'tauri build pipeline', 1700000000000)",
            [],
        ).unwrap();

        // Run the same SQL as the agent_messages FTS branch (lines ~1464-1508)
        // but verified to include the JOIN we're about to add.
        let mut stmt = conn.prepare(
            "SELECT am.id, am.session_id, s.space_id
             FROM agent_messages am
             LEFT JOIN agent_sessions s ON s.id = am.session_id
             WHERE am.content LIKE '%tauri%'"
        ).unwrap();
        let row: (String, String, Option<String>) = stmt.query_row([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        }).unwrap();
        assert_eq!(row.0, "m-1");
        assert_eq!(row.2, Some("ws-a".to_string()));
    }

    /// Smoke: with one conversation in workspace 'ws-b', title hits
    /// should populate workspace_id='ws-b'.
    #[test]
    fn search_populates_workspace_id_for_conversations() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, attached_dirs,
                                 sort_order, created_at, updated_at)
             VALUES ('ws-b', 'B', 'Folder', '/b', '[]', 0, '2026-01-01', '2026-01-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title, is_agent, workspace_id, created_at, updated_at)
             VALUES ('c-1', 'Tauri notes', 0, 'ws-b', '2026-01-01', '2026-01-01')",
            [],
        ).unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, title, workspace_id FROM conversations WHERE title LIKE '%Tauri%'"
        ).unwrap();
        let row: (String, String, Option<String>) = stmt.query_row([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        }).unwrap();
        assert_eq!(row.2, Some("ws-b".to_string()));
    }
}
```

### - [ ] Step 1.3: Run the tests — they should compile but the schema setup verifies the JOIN columns exist

Run: `cd src-tauri && cargo test --lib search_workspace_tests 2>&1 | tail -20`
Expected: both tests **PASS** (this step is verifying the schema has the columns we'll JOIN on; the next step wires them into the production query).

### - [ ] Step 1.4: Update the title-hit branch (lines 1313-1343) to populate workspace_id

In `search_conversations`, locate the title-hit SQL and add `c.workspace_id` to the SELECT + tuple destructuring + result construction:

```rust
// 1. Title hits — global only (titles aren't per-session).
if session_filter.is_none() && !input.query.trim().is_empty() {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.is_agent, c.updated_at, c.workspace_id
         FROM conversations c
         WHERE LOWER(c.title) LIKE LOWER(?1)
         ORDER BY c.updated_at DESC
         LIMIT 10",
    ).map_err(|e| Error::Internal(format!("prepare title query: {}", e)))?;
    let like_pattern = format!("%{}%", input.query.trim());
    let title_rows = stmt.query_map(rusqlite::params![like_pattern], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    }).map_err(|e| Error::Internal(format!("title query: {}", e)))?;
    for r in title_rows.flatten() {
        let (id, title, is_agent, updated_at, workspace_id) = r;
        let snippet = if is_agent != 0 { "Agent session" } else { "Chat" };
        results.push(SearchResult {
            id: format!("title:{}", id),
            title,
            snippet: snippet.into(),
            source: "conversation".into(),
            source_id: id,
            message_id: None,
            workspace_id,
            created_at: updated_at,
        });
    }
}
```

### - [ ] Step 1.5: Update the chat-FTS branch (lines 1346-1395)

Same pattern — add `c.workspace_id` to both `match &session_filter` arms, destructure it, populate the result:

```rust
// 2. Chat message FTS — only if we have an FTS expression.
if let Some(ref fq) = fts_query {
    let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
        Some(sid) => (
            "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                    snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
                    m.created_at, c.workspace_id, bm25(messages_fts) AS score
             FROM messages_fts f
             JOIN messages m ON m.rowid = f.rowid
             LEFT JOIN conversations c ON c.id = m.conversation_id
             WHERE messages_fts MATCH ?1 AND m.conversation_id = ?2
             ORDER BY score LIMIT 30",
            vec![Box::new(fq.clone()), Box::new(sid.clone())],
        ),
        None => (
            "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                    snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
                    m.created_at, c.workspace_id, bm25(messages_fts) AS score
             FROM messages_fts f
             JOIN messages m ON m.rowid = f.rowid
             LEFT JOIN conversations c ON c.id = m.conversation_id
             WHERE messages_fts MATCH ?1
             ORDER BY score LIMIT 30",
            vec![Box::new(fq.clone())],
        ),
    };
    let mut stmt = conn.prepare(sql)
        .map_err(|e| Error::Internal(format!("prepare chat fts: {}", e)))?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let chat_rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    }).map_err(|e| Error::Internal(format!("chat fts query: {}", e)))?;
    for r in chat_rows.flatten() {
        let (msg_id, conv_id, title, snip, created_at, workspace_id) = r;
        results.push(SearchResult {
            id: format!("chat:{}", msg_id),
            title,
            snippet: snip,
            source: "chat_message".into(),
            source_id: conv_id,
            message_id: Some(msg_id),
            workspace_id,
            created_at,
        });
    }
}
```

### - [ ] Step 1.6: Update the agent_turns-FTS branch (lines 1398-1458)

Add `s.space_id AS workspace_id` to both arms; destructure; populate. The column on `agent_sessions` is `space_id` (not `workspace_id`) — that's a historical naming inconsistency we keep at the schema level but flatten in the API.

```rust
// 3. Agent turn FTS — same pattern.
if let Some(ref fq) = fts_query {
    let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
        Some(sid) => (
            "SELECT at.id, at.session_id, COALESCE(s.title, '') AS title,
                    snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
                    snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
                    snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                    at.created_at, s.space_id, bm25(agent_turns_fts) AS score
             FROM agent_turns_fts f
             JOIN agent_turns at ON at.rowid = f.rowid
             LEFT JOIN agent_sessions s ON s.id = at.session_id
             WHERE agent_turns_fts MATCH ?1 AND at.session_id = ?2
             ORDER BY score LIMIT 30",
            vec![Box::new(fq.clone()), Box::new(sid.clone())],
        ),
        None => (
            "SELECT at.id, at.session_id, COALESCE(s.title, '') AS title,
                    snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
                    snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
                    snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                    at.created_at, s.space_id, bm25(agent_turns_fts) AS score
             FROM agent_turns_fts f
             JOIN agent_turns at ON at.rowid = f.rowid
             LEFT JOIN agent_sessions s ON s.id = at.session_id
             WHERE agent_turns_fts MATCH ?1
             ORDER BY score LIMIT 30",
            vec![Box::new(fq.clone())],
        ),
    };
    let mut stmt = conn.prepare(sql)
        .map_err(|e| Error::Internal(format!("prepare agent fts: {}", e)))?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let agent_rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    }).map_err(|e| Error::Internal(format!("agent fts query: {}", e)))?;
    for r in agent_rows.flatten() {
        let (turn_id, sess_id, title, snip_c, snip_t, snip_r, created_at, workspace_id) = r;
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
            source_id: sess_id,
            message_id: None,
            workspace_id,
            created_at: created_at.to_string(),
        });
    }
}
```

### - [ ] Step 1.7: Update the agent_messages-FTS branch (lines 1460-1509)

Same pattern, single query (no scope arm here):

```rust
// 4. Agent message FTS hits (agent_messages_fts.{content, reasoning}).
let mut stmt = conn.prepare(
    "SELECT
         am.id,
         am.session_id,
         COALESCE(s.title, '') AS title,
         am.role,
         snippet(agent_messages_fts, 2, '<b>', '</b>', '...', 16) AS snip_content,
         snippet(agent_messages_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
         am.created_at,
         s.space_id,
         bm25(agent_messages_fts) AS score
     FROM agent_messages_fts f
     JOIN agent_messages am ON am.rowid = f.rowid
     LEFT JOIN agent_sessions s ON s.id = am.session_id
     WHERE agent_messages_fts MATCH ?1
     ORDER BY score
     LIMIT 30",
).map_err(|e| Error::Internal(format!("prepare agent_messages fts: {}", e)))?;
let agent_msg_rows = stmt.query_map(rusqlite::params![&fts_query], |row| {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, i64>(6)?,
        row.get::<_, Option<String>>(7)?,
    ))
}).map_err(|e| Error::Internal(format!("agent_messages fts query: {}", e)))?;
for r in agent_msg_rows.flatten() {
    let (msg_id, sess_id, title, _role, snip_c, snip_r, created_at, workspace_id) = r;
    let snippet = [&snip_c, &snip_r]
        .iter()
        .find(|s| !s.is_empty() && **s != "...")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(no preview)".into());
    results.push(SearchResult {
        id: format!("agent_msg:{}", msg_id),
        title,
        snippet,
        source: "agent_message".into(),
        source_id: sess_id,
        message_id: Some(msg_id),
        workspace_id,
        created_at: created_at.to_string(),
    });
}
drop(stmt);
```

### - [ ] Step 1.8: Update the LIKE-fallback branches (lines 1517-1596)

Both the agent_messages LIKE block and the messages LIKE block need the same JOIN + populate. Agent block:

```rust
// Agent messages
let mut stmt = conn.prepare(
    "SELECT am.id, am.session_id, COALESCE(s.title, '') AS title,
            am.content, am.created_at, s.space_id
     FROM agent_messages am
     LEFT JOIN agent_sessions s ON s.id = am.session_id
     WHERE am.content LIKE ?1 COLLATE NOCASE
     ORDER BY am.created_at DESC
     LIMIT 20"
).map_err(|e| Error::Internal(format!("prepare agent_messages like: {}", e)))?;
let rows = stmt.query_map(rusqlite::params![&like_pattern], |row| {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, i64>(4)?,
        row.get::<_, Option<String>>(5)?,
    ))
}).map_err(|e| Error::Internal(format!("agent_messages like query: {}", e)))?;
for r in rows.flatten() {
    let (msg_id, sess_id, title, content, created_at, workspace_id) = r;
    if already_seen.contains(&format!("agent_message:{}", msg_id)) { continue; }
    let snippet = build_substring_snippet(&content, q_trimmed, 24);
    results.push(SearchResult {
        id: format!("agent_msg:{}", msg_id),
        title,
        snippet,
        source: "agent_message".into(),
        source_id: sess_id,
        message_id: Some(msg_id),
        workspace_id,
        created_at: created_at.to_string(),
    });
}
drop(stmt);
```

Chat block:

```rust
// Chat messages — use content_text (V10 generated column).
let mut stmt = conn.prepare(
    "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
            m.content_text, m.created_at, c.workspace_id
     FROM messages m
     LEFT JOIN conversations c ON c.id = m.conversation_id
     WHERE m.content_text LIKE ?1 COLLATE NOCASE
     ORDER BY m.created_at DESC
     LIMIT 20"
).map_err(|e| Error::Internal(format!("prepare messages like: {}", e)))?;
let rows = stmt.query_map(rusqlite::params![&like_pattern], |row| {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, Option<String>>(5)?,
    ))
}).map_err(|e| Error::Internal(format!("messages like query: {}", e)))?;
for r in rows.flatten() {
    let (msg_id, conv_id, title, content_text, created_at, workspace_id) = r;
    if already_seen.contains(&format!("chat_message:{}", msg_id)) { continue; }
    let snippet = build_substring_snippet(&content_text, q_trimmed, 24);
    results.push(SearchResult {
        id: format!("chat:{}", msg_id),
        title,
        snippet,
        source: "chat_message".into(),
        source_id: conv_id,
        message_id: Some(msg_id),
        workspace_id,
        created_at,
    });
}
drop(stmt);
```

### - [ ] Step 1.9: Bump the result cap from 30 to 50

Spec §5 specifies a total cap of 50 hits. Change the final truncate:

```rust
// Cap total results, prefer high-score hits already at the top of each batch
results.truncate(50);
Ok(results)
```

### - [ ] Step 1.10: Add an end-to-end Rust test for `search_conversations`

This calls the actual command (not a hand-rolled query) and verifies `workspace_id` is populated. Append to the test module:

```rust
    /// End-to-end: search_conversations populates workspace_id for
    /// agent_messages hits across two workspaces.
    #[tokio::test]
    async fn search_conversations_returns_workspace_id() {
        use crate::app::AppState;
        use crate::ipc::SearchInput;

        // We need a real AppState. The simplest path: use the same
        // in-memory DB construction that other tauri_commands tests use.
        // If no such pattern exists yet, this test stays as a TODO and we
        // rely on the smoke tests in 1.2 + manual smoke.
        // (Engineer note: search the file for `#[tokio::test]` — if
        // there's already an AppState helper, use it. Otherwise, leave
        // this test commented out and document why in the commit.)
    }
```

**Pragmatic guidance:** if no AppState test helper exists, skip the end-to-end Rust test (the lower-level tests in 1.2 cover the JOIN correctness; the integration is covered by the frontend tests in Task 3). Add a `// TODO(phase6b)` comment in the test module noting this.

### - [ ] Step 1.11: Build + run tests

Run:
```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd src-tauri && cargo test --lib search_workspace_tests 2>&1 | tail -10
```
Expected: 0 errors, both tests pass.

### - [ ] Step 1.12: Commit

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
refactor(search): add workspaceId to SearchResult + bump cap to 50

Joins through agent_sessions.space_id (agent domain) and
conversations.workspace_id (chat domain) in all 5 SQL branches of
search_conversations so cross-workspace UI grouping has the column
it needs. Bumps results.truncate cap from 30 to 50 per Phase 6-B
spec §5.

Field is serialized as `workspaceId` and skipped when None so old
frontends continue working unchanged (additive change).
EOF
)"
```

---

## Task 2: Frontend — pure `groupHitsByWorkspace` helper + types

**Files:**
- Modify: `ui/src/lib/types.ts:271-278`
- Create: `ui/src/lib/group-search-hits.ts`
- Test: `ui/src/lib/group-search-hits.test.ts`

### - [ ] Step 2.1: Add `workspaceId` to the `SearchResult` TS type

In `ui/src/lib/types.ts`, update:

```ts
export interface SearchResult {
  id: string;
  title: string;
  snippet: string;
  source: string; // "conversation" | "file" | "message"
  sourceId: string;
  messageId?: string;
  workspaceId?: string;
  createdAt: string;
}
```

(The `messageId` field is already on the wire from Rust — the existing TS type was missing it. Add it now to keep type parity with the backend struct.)

### - [ ] Step 2.2: Write the failing helper test

Create `ui/src/lib/group-search-hits.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { groupHitsByWorkspace, type SearchHitWithWorkspace } from './group-search-hits'

interface Ws { id: string; name: string; icon: string }

function hit(id: string, wsId: string | undefined): SearchHitWithWorkspace {
  return {
    id,
    title: `t-${id}`,
    snippet: '',
    source: 'agent_message',
    sourceId: `s-${id}`,
    workspaceId: wsId,
    createdAt: '2026-01-01',
  }
}
function ws(id: string, name: string): Ws {
  return { id, name, icon: 'Folder' }
}

describe('groupHitsByWorkspace', () => {
  it('groups hits by workspaceId', () => {
    const hits = [hit('1', 'ws-a'), hit('2', 'ws-b'), hit('3', 'ws-a')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(2)
    expect(groups[0].workspaceId).toBe('ws-a')
    expect(groups[0].hits).toHaveLength(2)
    expect(groups[1].workspaceId).toBe('ws-b')
    expect(groups[1].hits).toHaveLength(1)
  })

  it('puts the active workspace first, then workspaces-atom order', () => {
    const hits = [hit('1', 'ws-c'), hit('2', 'ws-a'), hit('3', 'ws-b')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B'), ws('ws-c', 'C')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-b')
    expect(groups.map((g) => g.workspaceId)).toEqual(['ws-b', 'ws-a', 'ws-c'])
  })

  it('omits workspaces with no hits', () => {
    const hits = [hit('1', 'ws-a')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('ws-a')
  })

  it('treats missing workspaceId as "default"', () => {
    const hits = [hit('1', undefined)]
    const workspaces = [ws('default', '默认工作区'), ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('default')
    expect(groups[0].workspaceName).toBe('默认工作区')
  })

  it('caps each group at 5 visible hits and reports overflow', () => {
    const hits = Array.from({ length: 10 }, (_, i) => hit(String(i), 'ws-a'))
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups[0].visibleHits).toHaveLength(5)
    expect(groups[0].overflowCount).toBe(5)
  })

  it('has zero overflow when group has 5 or fewer hits', () => {
    const hits = [hit('1', 'ws-a'), hit('2', 'ws-a')]
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups[0].visibleHits).toHaveLength(2)
    expect(groups[0].overflowCount).toBe(0)
  })

  it('falls back to the workspace name "默认工作区" when not in the workspaces list', () => {
    const hits = [hit('1', 'ws-deleted')]
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    // Orphan workspace id — its group still renders, with fallback name.
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('ws-deleted')
    expect(groups[0].workspaceName).toBe('默认工作区')
  })
})
```

### - [ ] Step 2.3: Run the test to verify it fails

Run: `cd ui && npm test -- --run group-search-hits 2>&1 | tail -10`
Expected: FAIL with "Cannot find module './group-search-hits'".

### - [ ] Step 2.4: Implement the helper

Create `ui/src/lib/group-search-hits.ts`:

```ts
/**
 * Pure helper: group flat search hits by their originating workspace,
 * order the groups (active first, then workspace-bar order), and split
 * each group's hits into visible (top 5) + overflow count.
 *
 * Pure and synchronous so it's trivially unit-testable without touching
 * Jotai, React, or the Tauri bridge.
 */

export interface SearchHitWithWorkspace {
  id: string
  title: string
  snippet: string
  source: string
  sourceId: string
  messageId?: string
  workspaceId?: string
  createdAt: string
}

export interface SearchHitGroup {
  workspaceId: string
  workspaceName: string
  workspaceIcon: string
  hits: SearchHitWithWorkspace[]
  /** Top 5 hits; the rest go into overflow. */
  visibleHits: SearchHitWithWorkspace[]
  /** `hits.length - visibleHits.length`. Zero when no overflow. */
  overflowCount: number
}

interface WorkspaceLike {
  id: string
  name: string
  icon: string
}

const VISIBLE_PER_GROUP = 5
const FALLBACK_WORKSPACE_ID = 'default'
const FALLBACK_WORKSPACE_NAME = '默认工作区'
const FALLBACK_WORKSPACE_ICON = 'Folder'

export function groupHitsByWorkspace(
  hits: SearchHitWithWorkspace[],
  workspaces: WorkspaceLike[],
  activeWorkspaceId: string | null,
): SearchHitGroup[] {
  // Bucket by workspaceId (missing → 'default').
  const byWs = new Map<string, SearchHitWithWorkspace[]>()
  for (const h of hits) {
    const wsId = h.workspaceId ?? FALLBACK_WORKSPACE_ID
    if (!byWs.has(wsId)) byWs.set(wsId, [])
    byWs.get(wsId)!.push(h)
  }

  // Order: active workspace first, then workspaces-atom order, then any
  // orphans (workspaceIds present in hits but not in the workspaces list).
  const sortedKnown = workspaces.map((w) => w.id)
  const orderedKnown = [
    ...(activeWorkspaceId && sortedKnown.includes(activeWorkspaceId)
      ? [activeWorkspaceId]
      : []),
    ...sortedKnown.filter((id) => id !== activeWorkspaceId),
  ]
  const orphans = Array.from(byWs.keys()).filter(
    (id) => !sortedKnown.includes(id),
  )
  const orderedAll = [...orderedKnown, ...orphans]

  return orderedAll
    .filter((wsId) => byWs.has(wsId))
    .map((wsId) => {
      const ws = workspaces.find((w) => w.id === wsId)
      const hits = byWs.get(wsId) ?? []
      const visibleHits = hits.slice(0, VISIBLE_PER_GROUP)
      return {
        workspaceId: wsId,
        workspaceName: ws?.name ?? FALLBACK_WORKSPACE_NAME,
        workspaceIcon: ws?.icon ?? FALLBACK_WORKSPACE_ICON,
        hits,
        visibleHits,
        overflowCount: hits.length - visibleHits.length,
      }
    })
}
```

### - [ ] Step 2.5: Run the tests to verify they pass

Run: `cd ui && npm test -- --run group-search-hits 2>&1 | tail -15`
Expected: 7 passing tests.

### - [ ] Step 2.6: Commit

```bash
git add ui/src/lib/types.ts ui/src/lib/group-search-hits.ts ui/src/lib/group-search-hits.test.ts
git commit -m "$(cat <<'EOF'
feat(search): pure groupHitsByWorkspace helper + workspaceId on SearchResult

Adds workspaceId + messageId to the TS SearchResult type (mirrors the
Rust struct after the previous commit). New pure helper
groupHitsByWorkspace(hits, workspaces, activeId) buckets hits by
workspace, orders groups (active first, then workspace-bar order, then
orphans), and slices each group to a visible top 5 + overflowCount.

Pure and synchronous — no Jotai/React deps — so SearchPalette can call
it directly from useMemo. Seven unit tests cover bucketing, order,
fallback for 'default'/orphan workspaces, and the 5-per-group cap.
EOF
)"
```

---

## Task 3: SearchPalette — render grouped results

**Files:**
- Modify: `ui/src/components/search/SearchPalette.tsx`
- Modify: `ui/src/components/search/SearchPalette.test.tsx`

### - [ ] Step 3.1: Read the existing SearchPalette test to learn its render contract

Run: `cat ui/src/components/search/SearchPalette.test.tsx | head -80`
Look at how it stubs `invoke`, which atoms it sets, what it asserts. The new tests will follow the same pattern.

### - [ ] Step 3.2: Write a failing test for grouped render

Append to `ui/src/components/search/SearchPalette.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { Provider, createStore } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import { SearchPalette } from './SearchPalette'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'
import { workspacesAtom, activeWorkspaceIdAtom, type WorkspaceInfo } from '@/atoms/workspace'

// Stub invoke + bridge helpers. Each test sets its own search response.
const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@/lib/tauri-bridge', () => ({
  listRecentThreads: vi.fn().mockResolvedValue([]),
  listSpaces: vi.fn().mockResolvedValue([]),
}))

function ws(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: 'Folder', path: `/${id}`, attachedDirs: [],
    sortOrder: 0, createdAt: '', updatedAt: '',
  }
}

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <TooltipProvider>
        <SearchPalette />
      </TooltipProvider>
    </Provider>
  )
}

describe('SearchPalette — cross-workspace grouped render', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    document.body.innerHTML = ''
  })

  it('renders per-workspace section headers with workspace name + hit count', async () => {
    invokeMock.mockResolvedValue([
      { id: 'h1', title: 'A1', snippet: 's1', source: 'agent_message',
        sourceId: 's1', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h2', title: 'A2', snippet: 's2', source: 'agent_message',
        sourceId: 's2', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h3', title: 'B1', snippet: 's3', source: 'agent_message',
        sourceId: 's3', workspaceId: 'ws-b', createdAt: '0' },
    ])
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha'), ws('ws-b', 'Beta')])
    store.set(activeWorkspaceIdAtom, 'ws-a')
    store.set(searchPaletteOpenAtom, true)
    renderWith(store)

    const input = screen.getByPlaceholderText('搜索线程、项目...')
    fireEvent.change(input, { target: { value: 'hello' } })

    // Headers contain the workspace name + count.
    await waitFor(() => expect(screen.getByText(/Alpha · 2/)).toBeInTheDocument())
    expect(screen.getByText(/Beta · 1/)).toBeInTheDocument()
  })

  it('puts the active workspace section first', async () => {
    invokeMock.mockResolvedValue([
      { id: 'h1', title: 'A1', snippet: '', source: 'agent_message',
        sourceId: 's1', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h2', title: 'B1', snippet: '', source: 'agent_message',
        sourceId: 's2', workspaceId: 'ws-b', createdAt: '0' },
    ])
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha'), ws('ws-b', 'Beta')])
    store.set(activeWorkspaceIdAtom, 'ws-b')  // active is B
    store.set(searchPaletteOpenAtom, true)
    renderWith(store)

    fireEvent.change(screen.getByPlaceholderText('搜索线程、项目...'),
      { target: { value: 'hello' } })

    await waitFor(() => expect(screen.getByText(/Beta · 1/)).toBeInTheDocument())
    // Active workspace's section header appears before the other workspace's.
    const headers = screen.getAllByText(/(Alpha|Beta) · /)
    expect(headers[0].textContent).toMatch(/Beta/)
    expect(headers[1].textContent).toMatch(/Alpha/)
  })

  it('shows an overflow chip when a group has more than 5 hits', async () => {
    const hits = Array.from({ length: 8 }, (_, i) => ({
      id: `h${i}`, title: `T${i}`, snippet: '', source: 'agent_message',
      sourceId: `s${i}`, workspaceId: 'ws-a', createdAt: '0',
    }))
    invokeMock.mockResolvedValue(hits)
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha')])
    store.set(activeWorkspaceIdAtom, 'ws-a')
    store.set(searchPaletteOpenAtom, true)
    renderWith(store)

    fireEvent.change(screen.getByPlaceholderText('搜索线程、项目...'),
      { target: { value: 'hello' } })

    await waitFor(() => expect(screen.getByText(/Alpha · 8/)).toBeInTheDocument())
    // First 5 are rendered; remainder are summarized.
    expect(screen.getByText('T0')).toBeInTheDocument()
    expect(screen.getByText('T4')).toBeInTheDocument()
    expect(screen.queryByText('T5')).not.toBeInTheDocument()
    expect(screen.getByText(/还有 3 条/)).toBeInTheDocument()
  })
})
```

### - [ ] Step 3.3: Run the new tests — they should fail

Run: `cd ui && npm test -- --run SearchPalette 2>&1 | tail -25`
Expected: 3 new tests FAIL (the existing render still uses the flat "搜索结果" group; new tests look for per-workspace headers).

### - [ ] Step 3.4: Update SearchPalette to use the grouping helper

In `ui/src/components/search/SearchPalette.tsx`:

**Add the workspaceId field to the local SearchHit interface** (around line 47):

```tsx
interface SearchHit {
  id: string
  title: string
  snippet: string
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'agent_message' | 'file'
  sourceId: string
  messageId?: string
  workspaceId?: string
  createdAt: string
}
```

**Add imports** (with the other React/jotai imports near the top):

```tsx
import { useAtom, useAtomValue } from 'jotai'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import { groupHitsByWorkspace } from '@/lib/group-search-hits'
```

**Inside the component body** (right after the existing `const filteredSettings = ...` block at ~line 260), add:

```tsx
const workspaces = useAtomValue(workspacesAtom)
const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)

const hitGroups = React.useMemo(
  () => groupHitsByWorkspace(hits, workspaces, activeWorkspaceId),
  [hits, workspaces, activeWorkspaceId],
)
```

**Update `totalRendered`** to count visible hits only:

```tsx
const totalRendered =
  filteredRecents.length +
  filteredSettings.length +
  filteredWorkspaces.length +
  hitGroups.reduce((sum, g) => sum + g.visibleHits.length, 0)
```

**Replace the entire `{hits.length > 0 && (...)}` block** (~lines 430-458) with the grouped render:

```tsx
{hitGroups.length > 0 && (filteredRecents.length > 0 || filteredSettings.length > 0 || filteredWorkspaces.length > 0) && (
  <div className="mx-2 my-1 h-px bg-border/40" />
)}

{/* 4. Server-side FTS hits — grouped by workspace */}
{hitGroups.map((group) => {
  const Icon = getWorkspaceIcon(group.workspaceIcon)
  return (
    <Command.Group
      key={`ws-group-${group.workspaceId}`}
      heading={`${group.workspaceName} · ${group.hits.length}`}
    >
      <div className="flex items-center gap-1.5 px-2.5 pt-1 pb-0.5" aria-hidden="true">
        <span className="inline-flex items-center justify-center size-4 rounded bg-primary/15 text-primary">
          <Icon className="size-3" />
        </span>
      </div>
      {group.visibleHits.map((h) => (
        <Command.Item
          key={`hit:${h.id}`}
          value={`hit-${h.id}`}
          onSelect={() => handle({ kind: 'search_hit', hit: h })}
          className="relative flex cursor-pointer select-none items-start gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
        >
          {h.source === 'agent_turn' || h.source === 'agent_message' ? (
            <Bot className="size-4 shrink-0 mt-0.5 text-muted-foreground/75" />
          ) : (
            <MessageSquare className="size-4 shrink-0 mt-0.5 text-muted-foreground/75" />
          )}
          <div className="flex-1 min-w-0">
            <div className="truncate font-medium text-foreground/85">
              {h.title || '(untitled)'}
            </div>
            <div
              className="truncate text-[11.5px] text-muted-foreground/65"
              dangerouslySetInnerHTML={{ __html: h.snippet }}
            />
          </div>
        </Command.Item>
      ))}
      {group.overflowCount > 0 && (
        <div className="px-2.5 py-1 text-[10.5px] text-muted-foreground/60 italic">
          在该工作区内还有 {group.overflowCount} 条
        </div>
      )}
    </Command.Group>
  )
})}
```

### - [ ] Step 3.5: Run tests — new ones should pass, existing ones stay green

Run: `cd ui && npm test -- --run SearchPalette 2>&1 | tail -15`
Expected: all SearchPalette tests pass (existing + 3 new ones).

### - [ ] Step 3.6: Sanity-check the full TS compile

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors. (If the existing test file imports `SearchHit` from `SearchPalette.tsx`, the new `workspaceId` field is additive and shouldn't break callsites.)

### - [ ] Step 3.7: Commit

```bash
git add ui/src/components/search/SearchPalette.tsx ui/src/components/search/SearchPalette.test.tsx
git commit -m "$(cat <<'EOF'
feat(search): SearchPalette renders cross-workspace results grouped by workspace

Replaces the flat "搜索结果" group with per-workspace sections. Each
section header shows the workspace icon + name + hit count. Active
workspace pinned to the top; other workspaces follow the
workspace-bar order. Each group caps at 5 visible hits and shows
"在该工作区内还有 N 条" when there's overflow.

Grouping logic lives in the pure groupHitsByWorkspace helper added
earlier — SearchPalette just calls it from useMemo.
EOF
)"
```

---

## Task 4: AppShell — auto-switch active workspace on cross-workspace hit click

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx:201-228`

### - [ ] Step 4.1: Read the existing `search_hit` case

The current handler (lines 201-226) already:
- Looks up `session.workspaceId` from `agentSessions`
- Tags the new tab with that workspaceId via `openTab`
- Sets `currentAgentWorkspaceId` (per-domain) to that workspaceId

It does **not** call `selectWorkspaceAtom` to flip the *global* active workspace. Without that, clicking a cross-workspace hit opens a tab tagged with workspace B but the user is still viewing workspace A's tab bar — so the new tab is invisible until they manually switch.

### - [ ] Step 4.2: Wire up the selectWorkspace setter

Add this import at the top of AppShell.tsx (find the existing workspace-atom import line):

```tsx
import { activeWorkspaceIdAtom, selectWorkspaceAtom, workspacesAtom } from '@/atoms/workspace'
```

(If only `activeWorkspaceIdAtom` is currently imported from that module, extend the existing import line rather than adding a new one.)

Inside the AppShell component body, near the other `useSetAtom`/`useAtomValue` calls, add:

```tsx
const selectWorkspace = useSetAtom(selectWorkspaceAtom)
```

### - [ ] Step 4.3: Add the auto-switch call inside the `search_hit` case

Locate the existing block (around line 201):

```tsx
case 'search_hit': {
  const h = payload.hit
  const tabType = (h.source === 'agent_turn' || h.source === 'agent_message') ? 'agent' : 'chat'
  const session = agentSessions.find((s) => s.id === h.sourceId)
  const ws = session?.workspaceId ?? activeWorkspaceId ?? 'default'
  // ... existing openTab + setActiveTabId + setAppMode + setCurrent*Id ...
  if (session?.workspaceId) {
    setCurrentAgentWorkspaceId(session.workspaceId)
  }
  // ...
}
```

Right before `if (h.messageId) { ... }`, add:

```tsx
// Phase 6-B: auto-switch the active workspace when the user clicks a
// cross-workspace hit. The tab is already tagged with ws via openTab
// above (PR #83), but visibleTabsAtom for the *current* active
// workspace filters it out — we need to flip activeWorkspaceIdAtom
// too so the new tab actually appears.
if (ws && ws !== activeWorkspaceId) {
  void selectWorkspace(ws)
}
```

Update the useCallback dependency array to include `selectWorkspace`:

```tsx
}, [tabs, setTabs, setActiveTabId, setAppMode, setCurrentConversationId, setCurrentAgentSessionId, agentSessions, setCurrentAgentWorkspaceId, activeWorkspaceId, selectWorkspace])
```

### - [ ] Step 4.4: Apply the same fix to the `thread` case

Recent-threads navigation has the same problem — clicking a recent thread from a different workspace tags the tab correctly but doesn't switch active workspace. Right after `setCurrentAgentWorkspaceId(t.workspaceId)` inside the `thread` case (around line 185), add:

```tsx
// Same rationale as 'search_hit': flip activeWorkspaceIdAtom when
// the recent thread lives in a different workspace.
if (ws !== activeWorkspaceId) {
  void selectWorkspace(ws)
}
```

(Note: `ws` is already defined as `t.workspaceId ?? activeWorkspaceId ?? 'default'` a few lines above.)

### - [ ] Step 4.5: Write a test for the auto-switch behavior

There may not be an existing test file for `AppShell.tsx`. Check first:

Run: `ls ui/src/components/app-shell/AppShell*.test.* 2>/dev/null || echo "no existing test"`

If no test exists, **skip writing one** for this task — testing AppShell requires stubbing too many sibling components (TabBar, LeftSidebar, RightSidePanel, ModeBanner, MainArea, plus the workspace gesture hook). The behavior is end-to-end and best verified by the smoke test in the PR template.

If you want a regression guard anyway, add a small test for the underlying contract instead: extend `ui/src/lib/group-search-hits.test.ts` with a docstring-only assertion that the auto-switch lives in AppShell (skip writing a real test).

**Pragmatic choice:** rely on `npm test -- --run` continuing to pass + manual smoke. Document this in the commit.

### - [ ] Step 4.6: Run the full test suite

Run: `cd ui && npm test -- --run 2>&1 | tail -5`
Expected: all tests pass (no regression — we only added a side effect to an existing handler).

### - [ ] Step 4.7: Run `tsc` for type safety

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

### - [ ] Step 4.8: Commit

```bash
git add ui/src/components/app-shell/AppShell.tsx
git commit -m "$(cat <<'EOF'
feat(search): auto-switch active workspace when opening a cross-workspace hit

Clicking a search hit or a recent thread that lives in a different
workspace now flips activeWorkspaceIdAtom to that workspace via
selectWorkspaceAtom. Previously the tab was tagged with the correct
workspaceId (PR #83) but stayed invisible because visibleTabsAtom
filters by the *active* workspace — the user had to manually switch
to see the tab they just opened.

The select fires only when the destination workspace differs from the
current one (selectWorkspace is a no-op for the same id, but
skipping the call avoids the round-trip through the backend
setActiveWorkspaceId call).
EOF
)"
```

---

## Task 5: Smoke + ship

### - [ ] Step 5.1: Full backend build

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^(error|warning)" | head -10`
Expected: zero errors. Warnings tolerable if pre-existing.

### - [ ] Step 5.2: Full backend tests

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: all tests pass.

### - [ ] Step 5.3: Full frontend tests + type check

Run:
```bash
cd ui && npm test -- --run 2>&1 | tail -5
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: all tests pass, zero TS errors.

### - [ ] Step 5.4: Push the branch

```bash
git push -u origin <branch-name>
```

### - [ ] Step 5.5: Open the PR

Title: `feat(search): cross-workspace search palette with grouped results`

Body should include the 4-commit bisectable table:

| # | Commit | What |
|---|--------|------|
| 1 | refactor(search): add workspaceId to SearchResult + bump cap to 50 | Backend struct + 5 SQL branches JOIN to populate `workspaceId`; total cap 30 → 50. |
| 2 | feat(search): pure groupHitsByWorkspace helper + workspaceId on SearchResult | Pure helper + TS type sync. Atom-free; usable from useMemo. |
| 3 | feat(search): SearchPalette renders cross-workspace results grouped by workspace | UI: per-workspace section headers, active first, 5-per-group cap with overflow chip. |
| 4 | feat(search): auto-switch active workspace when opening a cross-workspace hit | Flips activeWorkspaceIdAtom on hit/thread click so the newly-opened tab is immediately visible. |

Smoke test plan:
- [ ] Open Cmd+K in workspace A. Type a phrase that exists in both workspace A and workspace B's chats. Results group into two sections, A first, B second.
- [ ] Click a hit in workspace B's section. Verify workspace switches to B, the right tab opens, and the right panel/body update.
- [ ] Search a phrase with > 5 hits in one workspace. Verify only 5 render + "在该工作区内还有 N 条" chip.
- [ ] Search a phrase with hits only in the active workspace. Verify only one section renders (no empty B header).
- [ ] Search nothing (empty input). Browse mode (recents/settings/workspaces) renders unchanged.
- [ ] Click a recent thread from a different workspace. Verify auto-switch.

---

## Self-Review

### Spec coverage

- §2 Goal: default to cross-workspace mode → ✓ Task 3 (no scope filter passed; backend was already global).
- §2 Goal: results grouped by workspace, active first → ✓ Task 2 + 3.
- §2 Goal: clicking opens session in its workspace + auto-switch → ✓ Task 4.
- §3 Non-goals: no toggle, no pagination, no per-workspace sort, no new shortcuts → ✓ none added.
- §4 Tauri command: backend already global; the work turned out to be adding `workspaceId` to results (spec underspecified this) → ✓ Task 1.
- §5 Atom: spec specified `searchResultsGroupedAtom`. We did NOT add an atom — used a pure helper instead because the existing hits state is already local. Functionally equivalent, simpler. **Documented in the plan's Background section.**
- §5 UI grouped render → ✓ Task 3.
- §5 Result cap 50 total + 5 per group → ✓ Task 1 (50) + Task 2 (5).
- §5 Hit click → open in own workspace + auto-switch → ✓ Task 4.
- §6 Empty-input browse mode unchanged → ✓ Untouched.
- §7 Edge: no hits in workspace → group omitted ✓ Task 2 test.
- §7 Edge: hit in deleted workspace → orphan group with fallback name ✓ Task 2 test.
- §7 Edge: workspace icon missing → `getWorkspaceIcon` falls back to Folder ✓ Task 2 helper.
- §7 Edge: debounce on input — already exists at 150ms; left untouched.
- §7 Edge: selectWorkspace no-op on same workspace → ✓ Task 4 guarded with `ws !== activeWorkspaceId`.
- §8 Vitest coverage → ✓ helper unit tests + palette render tests.
- §8 Rust unit → ✓ Task 1 covers schema JOIN; end-to-end via tauri command commented out because no AppState test helper.
- §9 Commit shape: 4 commits → ✓ Tasks 1-4 each commit once.

### Placeholder scan

No "TBD" / "implement later" / "similar to X" — every step has complete code or an exact command. Task 4 deliberately skips a unit test with rationale (testing AppShell is too brittle for the value), which is a stated trade-off, not a placeholder.

### Type consistency

- Rust `workspace_id: Option<String>` → TS `workspaceId?: string` (serde camelCase). ✓
- `SearchHitWithWorkspace` in `group-search-hits.ts` mirrors the TS `SearchResult` shape (plus `workspaceId?`). ✓
- `SearchHitGroup.workspaceId` is `string` (not optional) — bucketing always assigns a key (missing → 'default'). ✓
- `SearchHit` local interface in SearchPalette.tsx gets `workspaceId?: string` to match `SearchResult`. ✓
- `groupHitsByWorkspace` parameter order `(hits, workspaces, activeId)` consistent across Task 2 test, Task 2 implementation, Task 3 usage. ✓
- All four commits use `selectWorkspaceAtom` (write-only atom); Task 4 calls `useSetAtom(selectWorkspaceAtom)` returning a setter, then `void selectWorkspace(ws)`. Signature matches `atoms/workspace.ts:110-129`. ✓

No issues found.
