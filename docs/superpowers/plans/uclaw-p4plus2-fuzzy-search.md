# P4++ — Fuzzy + Scoped Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade `search_conversations` from "phrase prefix on `unicode61`" to "trigram-substring + multi-token + scoped" so the ⌘K palette can find:
- Substring matches inside long words (`gomo` → `gomoku`)
- CJK content (`五子棋` matches text containing those characters anywhere)
- Multi-word queries with the words in any order (`rules gomoku` matches `gomoku rules`)
- Hits scoped to the current session, when the user presses `Tab` to flip scope

Plus a small UX win: when the user picks a search hit, the destination view briefly flashes the matched row so it's findable on scroll-end.

**Architecture:**

- **Backend (Rust):** Migrate the two existing FTS5 tables to `tokenize='trigram'` (SQLite 3.34+ built-in). With trigram tokenization, FTS5 `MATCH "<phrase>"` reduces to "substring" — all overlapping 3-char shingles must appear in order — which gives us CJK + partial-word naturally. Multi-token queries use FTS5's implicit AND between space-separated terms. Trigram is heavier on disk (≈3× index size for English) but acceptable for desktop SQLite.
- **Backend (query builder):** A pure helper `build_fts_query(input: &str) -> Option<String>` that:
  - Splits input on Unicode whitespace
  - For each token: escapes `"` and wraps in `"…"`
  - Joins tokens with `" "` (FTS5 reads this as implicit AND)
  - Returns `None` for empty / whitespace-only input (caller treats as "no FTS branch, only title LIKE")
- **Backend (scope):** `SearchInput.scope` already exists as `Option<String>` per `ipc.rs`. Honor it: when `scope = Some("session:<id>")`, add `WHERE m.conversation_id = ?` to the chat branch and `WHERE at.session_id = ?` to the agent branch, and skip the title-LIKE branch (titles aren't per-session).
- **Frontend:** New atom `searchPaletteScopeAtom` holds `'all' | { kind: 'session', id: string, label: string }`. `Tab` key toggles when there's an "active session" context (taken from `currentConversationId` / `currentAgentSessionId` atoms — whichever is non-null in the active app mode). A chip in the input row shows the current scope; pressing `Esc` while the chip is non-default first clears the scope, then on a second `Esc` closes the palette (cmdk pattern).
- **Frontend (highlight):** PR #29 already dispatches `window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', { detail }))` on FTS-hit click. Nothing listens for it. Add a small listener in chat + agent message lists that:
  1. Scrolls the matching DOM node into view
  2. Toggles a `.flash-hit` class for ~1.4s (CSS keyframe in `globals.css`)

**Tech Stack:** Same as P4 — rusqlite, FTS5, cmdk, Jotai. Adds one CSS keyframe. No new npm/cargo deps.

**Reference:** Roadmap entry `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md` §P4 (this plan extends the same row).

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p4plus2-fuzzy-search
```

- [ ] **Step 0.2: Confirm SQLite supports trigram tokenizer**

```bash
cd src-tauri && cargo run --quiet --bin uclaw -- --version 2>/dev/null || true
# Quick sanity: the sqlite shipped with rusqlite must be >= 3.34
sqlite3 ~/.uclaw/uclaw.db "SELECT sqlite_version();"
```
Expected: `>= 3.34.0`. The macOS-bundled `sqlite3` may differ from rusqlite's bundled copy — the bundled one is what matters at runtime; this is just an indicator.

- [ ] **Step 0.3: Baseline pipeline**

```bash
cd /Users/ryanliu/Documents/uclaw
(cd src-tauri && cargo build 2>&1 | tail -3)
(cd ui && npx tsc --noEmit 2>&1 | head -3)
(cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean build, clean tsc, **31/31 tests** passing (P3's 20 + P4+'s 11).

---

## Task 1: Migration V11 — switch FTS tokenizer to trigram

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `V11_FTS_TRIGRAM` constant + apply in `run`)

`messages_fts` and `agent_turns_fts` were created in V10/V8 with `tokenize='unicode61'`. We can't `ALTER` an FTS5 table's tokenizer — only `DROP` + recreate. The external-content tables (`content='messages'`, `content='agent_turns'`) hold zero data themselves, so dropping is cheap; we then re-`INSERT … SELECT` to rebuild.

- [ ] **Step 1.1: Add the migration constant**

Edit `src-tauri/src/db/migrations.rs`. Add **after** `V10_MESSAGES_FTS`:

```rust
/// V11: switch FTS tokenizer from `unicode61` to `trigram`.
///
/// Why trigram:
///   * Substring match within tokens — `gomo` finds `gomoku` without `*`.
///   * CJK works naturally — `unicode61` treats Chinese runs as one token,
///     trigram splits into 3-char shingles so `五子棋` matches anywhere.
///   * Multi-word queries get FTS5's implicit AND between whitespace-
///     separated terms, so `rules gomoku` matches both orderings.
///
/// Cost: ~3× index size. Acceptable for a desktop SQLite. Both tables are
/// external-content (`content='messages'` / `content='agent_turns'`), so
/// the FTS shadow has zero text bytes of its own — only postings.
///
/// We must DROP + recreate (FTS5 can't ALTER a tokenizer).
pub const V11_FTS_TRIGRAM: &str = "
-- Drop old triggers + tables. The data lives in messages / agent_turns and
-- is preserved.
DROP TRIGGER IF EXISTS messages_fts_insert;
DROP TRIGGER IF EXISTS messages_fts_update;
DROP TRIGGER IF EXISTS messages_fts_delete;
DROP TABLE IF EXISTS messages_fts;

DROP TRIGGER IF EXISTS agent_turns_fts_insert;
DROP TRIGGER IF EXISTS agent_turns_fts_update;
DROP TRIGGER IF EXISTS agent_turns_fts_delete;
DROP TABLE IF EXISTS agent_turns_fts;

-- Recreate messages_fts with trigram tokenizer.
CREATE VIRTUAL TABLE messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER messages_fts_insert
AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_update
AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;

-- Recreate agent_turns_fts with trigram tokenizer.
CREATE VIRTUAL TABLE agent_turns_fts USING fts5(
    session_id UNINDEXED,
    content,
    tool_result,
    reasoning,
    content='agent_turns',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER agent_turns_fts_insert
AFTER INSERT ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_update
AFTER UPDATE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_delete
AFTER DELETE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
END;
";

/// Backfill query for the recreated FTS tables. Run after V11 DDL.
pub const V11_BACKFILL_MESSAGES: &str = "
INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
SELECT rowid, conversation_id, role, content_text, reasoning FROM messages
";

pub const V11_BACKFILL_AGENT_TURNS: &str = "
INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
SELECT rowid, session_id, content, tool_result, reasoning FROM agent_turns
";
```

- [ ] **Step 1.2: Apply in `run`**

Edit `run` in the same file. Add **after** the V10 backfill block (around line 499):

```rust
    // V11: re-tokenize FTS5 with trigram for CJK + substring + typo-resilience.
    // Idempotent because the migration drops + recreates; backfill uses
    // INSERT … SELECT into the freshly-empty external-content shadow.
    tracing::debug!("Running migration V11: FTS trigram tokenizer");
    for stmt in V11_FTS_TRIGRAM.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V11 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(V11_BACKFILL_MESSAGES, []) {
        tracing::warn!("V11 messages backfill failed: {}", e);
    }
    if let Err(e) = conn.execute(V11_BACKFILL_AGENT_TURNS, []) {
        tracing::warn!("V11 agent_turns backfill failed: {}", e);
    }
```

- [ ] **Step 1.3: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors.

- [ ] **Step 1.4: Smoke test against the dev DB**

Run the binary briefly to apply V11. Then verify the schema:
```bash
sqlite3 ~/.uclaw/uclaw.db ".schema messages_fts" | head -3
sqlite3 ~/.uclaw/uclaw.db ".schema agent_turns_fts" | head -3
```
Expected: each `CREATE VIRTUAL TABLE` line includes `tokenize='trigram'`.

If the user has a long conversation history, the backfill could take a few seconds. That's acceptable for a one-time migration on app start.

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "$(cat <<'EOF'
feat(search): V11 migration — switch FTS to trigram tokenizer

unicode61 treats CJK runs as one token (so 五子棋 was effectively
unsearchable) and required prefix `*` for substring match. Trigram
tokenizer:
  - Splits CJK character-by-character via 3-char shingles
  - Substring-matches inside long words (gomo → gomoku) without `*`
  - Lets FTS5 implicit-AND multi-word queries

Drop + recreate is required (FTS5 has no ALTER for tokenizer). The
backing tables (messages / agent_turns) hold the data, so the
external-content shadow can be repopulated with INSERT … SELECT.

Cost: ~3× FTS index size. Acceptable for a desktop SQLite.
EOF
)"
```

---

## Task 2: Backend — multi-token query builder + scope filter

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (replace the inline `fts_query` formatter with a tested helper, wire `scope`)

- [ ] **Step 2.1: Add `build_fts_query` helper**

In `src-tauri/src/tauri_commands.rs`, **above** `pub async fn search_conversations`, add:

```rust
/// Build an FTS5 MATCH expression from raw user input.
///
/// Splits on Unicode whitespace, escapes any double-quotes inside each
/// token, wraps each token as a phrase (`"…"`), and space-joins them so
/// FTS5 reads the result as implicit AND of substring matches (under the
/// trigram tokenizer added in V11).
///
/// Returns `None` for empty / whitespace-only input — the caller should
/// then skip the FTS branches and only do title LIKE.
fn build_fts_query(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<String> = trimmed
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

/// Parse the optional `scope` field on `SearchInput` into a typed value.
/// Format: `"session:<id>"` for a session-scoped search, anything else
/// (or `None`) → unscoped global search.
fn parse_scope(scope: Option<&str>) -> Option<String> {
    let raw = scope?;
    raw.strip_prefix("session:").map(|id| id.to_string())
}
```

- [ ] **Step 2.2: Rewrite `search_conversations` to use the helper + scope**

Replace the body of `pub async fn search_conversations`. New flow:

1. Try `build_fts_query` → `fts_query: Option<String>`.
2. Parse scope → `session_filter: Option<String>`.
3. **Title-LIKE** branch only runs when `session_filter` is `None` (titles aren't per-session).
4. **Chat FTS** branch runs when `fts_query.is_some()`. Uses param binding for both the FTS query and (optionally) the session id.
5. **Agent FTS** branch — same pattern.

Concrete replacement:

```rust
#[tauri::command]
pub async fn search_conversations(state: State<'_, AppState>, input: SearchInput) -> Result<Vec<SearchResult>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let fts_query = build_fts_query(&input.query);
    let session_filter = parse_scope(input.scope.as_deref());

    let mut results: Vec<SearchResult> = Vec::new();

    // 1. Title hits — global only (titles aren't per-session).
    if session_filter.is_none() && !input.query.trim().is_empty() {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.title, c.is_agent, c.updated_at
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
    }

    // 2. Chat message FTS — only if we have an FTS expression.
    if let Some(ref fq) = fts_query {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
            Some(sid) => (
                "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                        snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
                        m.created_at, bm25(messages_fts) AS score
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
                        m.created_at, bm25(messages_fts) AS score
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
    }

    // 3. Agent turn FTS — same pattern.
    if let Some(ref fq) = fts_query {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
            Some(sid) => (
                "SELECT at.id, at.session_id, COALESCE(s.title, '') AS title,
                        snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
                        snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
                        snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                        at.created_at, bm25(agent_turns_fts) AS score
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
                        at.created_at, bm25(agent_turns_fts) AS score
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
            ))
        }).map_err(|e| Error::Internal(format!("agent fts query: {}", e)))?;
        for r in agent_rows.flatten() {
            let (turn_id, sess_id, title, snip_c, snip_t, snip_r, created_at) = r;
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
                created_at: created_at.to_string(),
            });
        }
    }

    results.truncate(30);
    Ok(results)
}
```

- [ ] **Step 2.3: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors.

- [ ] **Step 2.4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
feat(search): multi-token query builder + session scope

Replace the inline `format!("\"{}\"*", q)` with a tested helper that
splits on whitespace and joins each escaped token as `"…"` — FTS5
under the trigram tokenizer reads this as implicit-AND of substrings,
which lets multi-word queries match in any order.

Also wires SearchInput.scope (already in the IPC type but unused):
"session:<id>" restricts both FTS branches to that conversation /
agent session and skips the title-LIKE branch (titles aren't
per-session).
EOF
)"
```

---

## Task 3: Backend — unit tests for query builder

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add `#[cfg(test)] mod tests`)

The query builder is the riskiest unit (FTS5 will throw a syntax error if it gets a malformed expression). Cover the core cases.

- [ ] **Step 3.1: Add tests**

Append to the bottom of `src-tauri/src/tauri_commands.rs`:

```rust
#[cfg(test)]
mod fts_query_tests {
    use super::{build_fts_query, parse_scope};

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(build_fts_query(""), None);
        assert_eq!(build_fts_query("   "), None);
        assert_eq!(build_fts_query("\t\n"), None);
    }

    #[test]
    fn single_word() {
        assert_eq!(build_fts_query("gomoku").unwrap(), "\"gomoku\"");
    }

    #[test]
    fn multi_word_implicit_and() {
        assert_eq!(
            build_fts_query("gomoku rules").unwrap(),
            "\"gomoku\" \"rules\""
        );
    }

    #[test]
    fn cjk_token_preserved_as_phrase() {
        // Trigram tokenizer will further split this server-side;
        // build_fts_query just wraps the user's runs as phrases.
        assert_eq!(build_fts_query("五子棋").unwrap(), "\"五子棋\"");
    }

    #[test]
    fn mixed_cjk_and_ascii() {
        assert_eq!(
            build_fts_query("五子棋 rules").unwrap(),
            "\"五子棋\" \"rules\""
        );
    }

    #[test]
    fn embedded_double_quotes_are_doubled() {
        // FTS5 phrase escape: `"` → `""` inside a quoted phrase.
        assert_eq!(
            build_fts_query("a\"b c").unwrap(),
            "\"a\"\"b\" \"c\""
        );
    }

    #[test]
    fn whitespace_collapsed() {
        assert_eq!(
            build_fts_query("  foo   bar  ").unwrap(),
            "\"foo\" \"bar\""
        );
    }

    #[test]
    fn scope_session_parses() {
        assert_eq!(
            parse_scope(Some("session:abc-123")),
            Some("abc-123".to_string())
        );
    }

    #[test]
    fn scope_unknown_returns_none() {
        assert_eq!(parse_scope(Some("workspace:foo")), None);
        assert_eq!(parse_scope(Some("")), None);
        assert_eq!(parse_scope(None), None);
    }
}
```

- [ ] **Step 3.2: Run + commit**

```bash
cd src-tauri && cargo test --lib fts_query_tests 2>&1 | tail -10
```
Expected: 9 passed.

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "test(search): cover build_fts_query + parse_scope"
```

---

## Task 4: Frontend — scope toggle + chip

**Files:**
- Create: `ui/src/atoms/search-atoms.ts` (already exists — check; if so, modify)
- Modify: `ui/src/components/search/SearchPalette.tsx`
- Modify: `ui/src/lib/tauri-bridge.ts` (the bridge wrapper for `searchConversations` if it exists; otherwise just call `invoke` from the palette as it already does)

- [ ] **Step 4.1: Add `searchPaletteScopeAtom`**

Edit `ui/src/atoms/search-atoms.ts`. Add (or co-locate with `searchPaletteOpenAtom`):

```ts
import { atom } from 'jotai'

export const searchPaletteOpenAtom = atom(false)

/**
 * Search-palette scope. `'all'` (default) does a global FTS; an object
 * limits the search to one conversation / agent session.
 *
 * The palette renders a chip when scope is non-default and supports
 * `Tab` (set to current session) / `Esc` (clear scope, then close).
 */
export type SearchScope =
  | 'all'
  | { kind: 'session'; id: string; label: string }

export const searchPaletteScopeAtom = atom<SearchScope>('all')
```

- [ ] **Step 4.2: Read scope + currentSession in the palette**

Edit `ui/src/components/search/SearchPalette.tsx`. Imports — add the new atom plus the existing `currentConversationId` / `currentAgentSessionId` / `appMode` atoms (find them with `grep -rn "currentConversationIdAtom\|currentAgentSessionIdAtom\|appModeAtom" ui/src/atoms`). At the top of the component:

```tsx
const [scope, setScope] = useAtom(searchPaletteScopeAtom)
const appMode = useAtomValue(appModeAtom)
const currentConversationId = useAtomValue(currentConversationIdAtom)
const currentAgentSessionId = useAtomValue(currentAgentSessionIdAtom)

const activeSessionTarget = React.useMemo<{
  id: string
  label: string
} | null>(() => {
  if (appMode === 'agent' && currentAgentSessionId) {
    return { id: currentAgentSessionId, label: '当前 Agent 会话' }
  }
  if (appMode === 'chat' && currentConversationId) {
    return { id: currentConversationId, label: '当前聊天' }
  }
  return null
}, [appMode, currentConversationId, currentAgentSessionId])
```

- [ ] **Step 4.3: Tab toggles scope; Esc clears then closes**

Update the global keyboard handler. Replace its body:

```tsx
React.useEffect(() => {
  const handler = (e: KeyboardEvent) => {
    // Outside the palette: ⌘K / Ctrl-K opens.
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault()
      setOpen((v) => !v)
      return
    }
    if (!open) return
    if (e.key === 'Escape') {
      e.preventDefault()
      // Two-step Esc: first clear scope, second close.
      if (scope !== 'all') {
        setScope('all')
      } else {
        setOpen(false)
      }
      return
    }
    if (e.key === 'Tab' && activeSessionTarget) {
      e.preventDefault()
      setScope((s) =>
        s === 'all'
          ? { kind: 'session', id: activeSessionTarget.id, label: activeSessionTarget.label }
          : 'all',
      )
    }
  }
  document.addEventListener('keydown', handler)
  return () => document.removeEventListener('keydown', handler)
}, [open, setOpen, scope, setScope, activeSessionTarget])
```

- [ ] **Step 4.4: Pass scope to the FTS invoke**

In the debounced search effect, change the `invoke` call:

```tsx
const scopeArg =
  scope === 'all' ? null : `session:${scope.id}`
const result = await invoke<SearchHit[]>('search_conversations', {
  input: { query: query.trim(), scope: scopeArg },
})
```

- [ ] **Step 4.5: Render the scope chip in the input row**

Replace the input-row JSX. The chip sits between the search icon and the input when scope is non-default:

```tsx
<div className="flex items-center gap-3 border-b border-border/50 px-4 py-3.5">
  <Search className="size-4 shrink-0 text-muted-foreground/60" />
  {scope !== 'all' && (
    <button
      type="button"
      onClick={() => setScope('all')}
      className="flex shrink-0 items-center gap-1.5 rounded-md bg-accent/60 px-2 py-1 text-[11.5px] text-accent-foreground/90 hover:bg-accent transition-colors"
      title="按 Esc 清除范围"
    >
      <Hash className="size-3" />
      {scope.label}
      <span className="text-accent-foreground/50">×</span>
    </button>
  )}
  <Command.Input
    autoFocus
    value={query}
    onValueChange={setQuery}
    placeholder={scope === 'all' ? '搜索线程、项目...' : '在当前会话内搜索...'}
    className="flex-1 bg-transparent outline-none text-[13.5px] text-foreground placeholder:text-muted-foreground/40"
  />
  {searching && (
    <span className="text-[10.5px] text-muted-foreground/40 tabular-nums">…</span>
  )}
</div>
```

- [ ] **Step 4.6: Update footer kbd hints with the new Tab affordance**

In the footer JSX, **prepend** the Tab hint so it only appears when `activeSessionTarget` is non-null:

```tsx
{activeSessionTarget && (
  <span className="flex items-center gap-1">
    <kbd className="rounded bg-muted px-1 py-0.5 font-mono text-[10px] text-muted-foreground border border-border/40">Tab</kbd>
    {scope === 'all' ? '限定当前会话' : '取消限定'}
  </span>
)}
```

- [ ] **Step 4.7: Reset scope on close**

In the existing "reset query when palette closes" effect, also reset scope:

```tsx
React.useEffect(() => {
  if (!open) {
    setQuery('')
    setScope('all')
  }
}, [open, setScope])
```

- [ ] **Step 4.8: TS check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/atoms/search-atoms.ts ui/src/components/search/SearchPalette.tsx
git commit -m "$(cat <<'EOF'
feat(search): scope toggle — Tab limits search to current session

When the active app pane has a session in focus (chat or agent), Tab
toggles a scope chip in the palette input. While the chip is on, the
FTS search sends scope="session:<id>" so the backend filters both
messages_fts and agent_turns_fts to that one thread.

Esc becomes two-step: first press clears the chip, second closes the
palette (cmdk convention). Footer gets a Tab kbd hint that's only
visible when scope toggling is meaningful.
EOF
)"
```

---

## Task 5: Frontend — flash-on-scroll-to highlight

**Files:**
- Modify: `ui/src/styles/globals.css` (add `.flash-hit` keyframe)
- Modify: chat message list + agent turn list components — find them with `grep -rn "uclaw:scroll-to-message" ui/src` (PR #29 dispatches this; nothing listens). The likely targets are the components that render `messages[].id` / `agent_turns[].id` rows. Search:
  ```bash
  grep -rn "MessageList\|TurnList\|message-bubble" ui/src/components | head
  ```

- [ ] **Step 5.1: CSS keyframe**

Append to `ui/src/styles/globals.css` (anywhere in the global section):

```css
/* Search-hit flash — applied briefly to a scrolled-to message row. */
@keyframes flash-hit-pulse {
  0% { background-color: hsl(var(--accent) / 0.0); }
  20% { background-color: hsl(var(--accent) / 0.55); }
  100% { background-color: hsl(var(--accent) / 0.0); }
}
.flash-hit {
  animation: flash-hit-pulse 1400ms ease-out;
  border-radius: 0.5rem;
}
```

- [ ] **Step 5.2: Listener helper**

Create `ui/src/lib/scroll-to-message.ts`:

```ts
/**
 * Listens for `uclaw:scroll-to-message` events dispatched by the
 * SearchPalette and scrolls + flashes the matching DOM element.
 *
 * Element discovery: rows must have `data-message-id={id}` on a
 * stable wrapper. Returns an unsubscribe.
 */
export function installScrollToMessage(): () => void {
  const handler = (e: Event) => {
    const detail = (e as CustomEvent).detail as
      | { sessionId: string; messageId: string }
      | undefined
    if (!detail?.messageId) return
    // Defer one frame so any tab-switch or list-mount has time to settle.
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>(
        `[data-message-id="${CSS.escape(detail.messageId)}"]`,
      )
      if (!el) return
      el.scrollIntoView({ behavior: 'smooth', block: 'center' })
      el.classList.remove('flash-hit') // restart animation
      void el.offsetWidth // force reflow
      el.classList.add('flash-hit')
      window.setTimeout(() => el.classList.remove('flash-hit'), 1500)
    })
  }
  window.addEventListener('uclaw:scroll-to-message', handler)
  return () => window.removeEventListener('uclaw:scroll-to-message', handler)
}
```

- [ ] **Step 5.3: Wire in AppShell**

Edit `ui/src/components/app-shell/AppShell.tsx`. Add inside the existing component, near other `useEffect` calls:

```tsx
React.useEffect(() => {
  const dispose = installScrollToMessage()
  return dispose
}, [])
```

(Add `import { installScrollToMessage } from '@/lib/scroll-to-message'` at the top.)

- [ ] **Step 5.4: Ensure `data-message-id` is on the rows**

Find chat-message rendering (`grep -n "message-bubble\|MessageBubble\|MessageRow" ui/src/components/chat -r | head`).
Find agent-turn rendering (`grep -n "agent.*turn\|TurnRow\|AgentMessage" ui/src/components/agent -r | head`).

For whichever component renders one row at a time, add `data-message-id={msg.id}` (chat) or `data-message-id={turn.id}` (agent) on the outermost `<div>`.

If the search hit's `messageId` is an agent-turn id, scroll-to needs to find the corresponding row. Since PR #29 sets `messageId` only on chat hits (the agent-turn branch leaves it `None`), focus on the chat path; agent-turn highlighting would need a separate `data-turn-id` attr — punt to a follow-up if that's non-trivial.

- [ ] **Step 5.5: Manual smoke**

`cargo tauri dev` → ⌘K → search a known message → click → confirm scroll + flash.

- [ ] **Step 5.6: Commit**

```bash
git add ui/src/styles/globals.css ui/src/lib/scroll-to-message.ts ui/src/components/app-shell/AppShell.tsx ui/src/components/chat
git commit -m "$(cat <<'EOF'
feat(search): flash-and-scroll to picked search hit

PR #29 already dispatches `uclaw:scroll-to-message` on FTS-hit click
but nothing listened. Add a small AppShell-level listener that finds
the row by `data-message-id`, scrolls it into view, and pulses an
accent-colored flash for 1.4s.

Chat message rows now carry `data-message-id`. Agent-turn highlight
is a follow-up (the FTS-hit payload doesn't currently include the
turn id on the agent path).
EOF
)"
```

---

## Task 6: Tests — frontend palette suite update

**Files:**
- Modify: `ui/src/components/search/SearchPalette.test.tsx`

Existing 11 tests should still pass — none assert on the FTS query string. But add 2 new tests for the scope behavior.

- [ ] **Step 6.1: Add scope tests**

Append to the `describe('SearchPalette')` block:

```tsx
import { searchPaletteScopeAtom } from '@/atoms/search-atoms'

it('Tab toggles scope chip when an active session exists', async () => {
  const { store, user } = renderWithProviders(<SearchPalette />, {
    initialAtoms: [
      [appModeAtom, 'chat'],
      [currentConversationIdAtom, 'conv-1'],
    ],
  })
  store.set(searchPaletteOpenAtom, true)
  await screen.findByPlaceholderText('搜索线程、项目...')
  expect(store.get(searchPaletteScopeAtom)).toBe('all')
  await user.keyboard('{Tab}')
  await waitFor(() => {
    const s = store.get(searchPaletteScopeAtom)
    expect(s).not.toBe('all')
    if (s !== 'all') expect(s.id).toBe('conv-1')
  })
})

it('first Esc clears scope, second Esc closes the palette', async () => {
  const { store, user } = renderWithProviders(<SearchPalette />, {
    initialAtoms: [
      [appModeAtom, 'chat'],
      [currentConversationIdAtom, 'conv-1'],
    ],
  })
  store.set(searchPaletteOpenAtom, true)
  await screen.findByPlaceholderText('搜索线程、项目...')
  store.set(searchPaletteScopeAtom, { kind: 'session', id: 'conv-1', label: '当前聊天' })
  await user.keyboard('{Escape}')
  await waitFor(() => expect(store.get(searchPaletteScopeAtom)).toBe('all'))
  expect(store.get(searchPaletteOpenAtom)).toBe(true)
  await user.keyboard('{Escape}')
  await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(false))
})
```

If `renderWithProviders` doesn't accept `initialAtoms`, adjust to use `store.set(...)` after render. Check the helper at `ui/src/test-utils/render.tsx` first.

- [ ] **Step 6.2: Run + commit**

```bash
cd ui && npx vitest run SearchPalette 2>&1 | tail -10
```
Expected: 13/13 passing.

```bash
git add ui/src/components/search/SearchPalette.test.tsx
git commit -m "test(search): cover Tab-scope + two-step Esc"
```

---

## Task 7: Final verification + push + PR

- [ ] **Step 7.1: Full pipeline**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib fts_query_tests 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, 9 rust tests pass, 0 TS errors, 33/33 frontend tests pass (P3 20 + SearchPalette 13).

- [ ] **Step 7.2: Push + PR**

```bash
git push -u origin claude/p4plus2-fuzzy-search
gh pr create --title "P4++: fuzzy + scoped search (trigram FTS)" --body "$(cat <<'EOF'
## Summary

Lifts the ⌘K palette from "phrase prefix on unicode61" to "trigram-substring + multi-token + scoped":

| Capability | Before | After |
|---|---|---|
| Substring match | needed `*` suffix | naturally finds `gomo` → `gomoku` |
| CJK | `unicode61` treated 五子棋 as one token (only exact match worked) | trigram splits into 3-char shingles, finds anywhere |
| Multi-word | treated whole input as a phrase | implicit AND across tokens, any order |
| Scope | global only | `Tab` limits to current session; chip in input |
| Hit highlight | scroll dispatched, no listener | scrolls + pulses flash for 1.4s |

## What changed

| Layer | Change |
|---|---|
| DB | V11 migration: drop+recreate `messages_fts` / `agent_turns_fts` with `tokenize='trigram'`, backfill from source tables |
| Backend | New `build_fts_query` helper — escapes + phrases + space-joins. `parse_scope` reads `SearchInput.scope` ("session:<id>"). Both FTS branches respect scope; title-LIKE skipped under scope. |
| Backend tests | 9 unit tests on `build_fts_query` + `parse_scope` |
| Frontend | `searchPaletteScopeAtom` + `Tab` toggle + chip + two-step Esc + footer hint |
| Frontend | Flash-on-scroll-to listener + `data-message-id` on chat rows |
| Frontend tests | 2 new scope tests (13 total) |

## Verification

- ✅ `cargo build` clean
- ✅ 9 backend FTS query tests passing
- ✅ `tsc --noEmit` clean
- ✅ 33 frontend tests passing
- ✅ Manual: ⌘K → 中文 query → finds session content; Tab toggles scope; Esc twice closes

## Out of scope (follow-ups)

- Agent-turn highlighting (search-hit payload doesn't include turn id on agent path — needs IPC change)
- True edit-distance / Levenshtein typo tolerance — would need a custom tokenizer or post-filter
- Search history / saved queries

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria

- ✅ V11 migration runs clean on a fresh DB and on an existing dev DB
- ✅ `messages_fts` and `agent_turns_fts` use `tokenize='trigram'`
- ✅ `gomo` finds messages containing `gomoku` without a `*` suffix
- ✅ `五子棋` finds messages containing those characters anywhere
- ✅ `rules gomoku` returns the same rows as `gomoku rules`
- ✅ `Tab` adds a scope chip when an active session is in focus; FTS narrowed
- ✅ Two-step `Esc` (clear scope, then close)
- ✅ Picking a hit scrolls + flashes the row
- ✅ 33 frontend tests + 9 backend FTS tests passing
- ✅ Each task is its own commit (bisectable)

## Out of scope (deferred)

- Edit-distance typo tolerance
- Agent-turn id in FTS-hit payload (needed for agent-side flash)
- Search history
- Saved searches / pinned queries
- Workspace-scoped search ("search in this workspace's threads only")
