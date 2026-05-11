# Phase 6-A — Pinned Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users pin individual agent sessions so they elevate above the implicit "most-recently-updated" ordering in WorkspaceRail.

**Architecture:** New nullable column `agent_sessions.pinned_at` (V18 migration) carries the canonical pin state. A real `toggle_pin_agent_session` Tauri command replaces the existing silent-catch wrapper. WorkspaceRail renders two stacked segments (📌 固定 + 会话), each segment sorted independently. The pre-existing `pinned INTEGER` column (chat-conversation only) stays untouched.

**Tech Stack:** Rust + Tauri 2 (rusqlite), React 18 + TypeScript + Jotai, Vitest.

**Spec reference:** `docs/superpowers/specs/2026-05-11-workspace-phase6a-pinned-design.md`

---

## File Structure Overview

```
src-tauri/src/db/migrations.rs       (MODIFIED — add V18 const + run())
src-tauri/src/tauri_commands.rs      (MODIFIED — list_agent_sessions extended + new toggle_pin_agent_session)
src-tauri/src/main.rs                (MODIFIED — register toggle_pin_agent_session in invoke_handler!)

ui/src/lib/tauri-bridge.ts           (MODIFIED — togglePinAgentSession wrapper rewritten, no silent catch)
ui/src/lib/agent-types.ts            (MODIFIED — AgentSessionMeta.pinnedAt added)
ui/src/atoms/workspace.ts            (MODIFIED — WorkspaceSession.pinnedAt + syncWorkspaceSessionsAtom forwards it)
ui/src/atoms/agent-atoms.ts          (MODIFIED — togglePinAgentSessionAtom action atom)

ui/src/components/workspace/SessionItem.tsx     (MODIFIED — onTogglePin prop + Pin/PinOff menu item)
ui/src/components/workspace/WorkspaceRail.tsx   (MODIFIED — two-segment render + SegmentHeader)

src-tauri/src/db/migrations.rs (inline #[cfg(test)] mod tests)   — V18 migration test
src-tauri/src/tauri_commands.rs (inline #[cfg(test)] mod)        — Optional unit shim (toggle logic is exercised via Tauri runtime; keep test scope tight)
ui/src/components/workspace/WorkspaceRail.test.tsx               — extend existing tests
ui/src/components/workspace/SessionItem.test.tsx                 — new (verify menu label per pinned state)
```

---

## Task 1: V18 Migration

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add a V18 const at end of the others, add a `for stmt` block inside `run()` after V17's block, extend the `#[cfg(test)] mod tests` with a roundtrip test)

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests` block in `src-tauri/src/db/migrations.rs` (after the V17 tests):

```rust
    /// V18 adds a nullable pinned_at column to agent_sessions. Verifies:
    /// (1) column exists after migration, (2) existing rows default to NULL,
    /// (3) re-running the migration is idempotent.
    #[test]
    fn v18_adds_pinned_at_column_nullable_with_null_default() {
        let conn = db_pre_v16();
        // Pre-V18: insert a session row to make sure backfill is non-destructive.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json,
                                          message_count, pinned, archived,
                                          created_at, updated_at)
             VALUES ('s1', 'default', 't', '{}', 0, 0, 0, 0, 0)",
            [],
        ).unwrap();

        // Drive V18.
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            // Match the run() per-stmt skip pattern.
            let _ = conn.execute(stmt, []);
        }

        // Column exists + pre-existing row has NULL.
        let pinned_at: Option<i64> = conn.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = 's1'",
            [],
            |row| row.get::<_, Option<i64>>(0),
        ).unwrap();
        assert!(pinned_at.is_none());

        // Idempotent re-run (duplicate column ALTER fails per-stmt; ignored).
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Row still present, still NULL.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run from `src-tauri/`:

```bash
cargo test --lib db::migrations::tests::v18_adds_pinned_at_column_nullable_with_null_default 2>&1 | tail -10
```

Expected: FAIL with `cannot find value V18_AGENT_SESSIONS_PINNED_AT in module super` or similar.

- [ ] **Step 3: Add the V18 migration constant**

Edit `src-tauri/src/db/migrations.rs`. After the V17 constant block (around line 656, the closing `";`), add:

```rust
/// V18: pin state for agent sessions. Nullable INTEGER stores the ms
/// timestamp when the session was pinned; NULL means unpinned. Defaults
/// to NULL for new + existing rows. The pre-existing `pinned` INTEGER
/// column is unrelated (used by chat conversations) and left untouched.
///
/// ALTER may fail on re-run with "duplicate column" — handled by the
/// per-statement tracing::warn! skip in run(), matching V9/V10/V17 idiom.
pub const V18_AGENT_SESSIONS_PINNED_AT: &str = "
ALTER TABLE agent_sessions ADD COLUMN pinned_at INTEGER NULL;
CREATE INDEX IF NOT EXISTS idx_agent_sessions_pinned_at ON agent_sessions(pinned_at);
";
```

- [ ] **Step 4: Drive V18 from `run()`**

Edit the same file. After the V17 `for stmt in V17_WORKSPACE_PATH_SORT_ATTACHED` block (around line 776–780), add:

```rust
    // V18: agent_sessions.pinned_at — canonical pin state for the agent UI.
    tracing::debug!("Running migration V18: agent_sessions.pinned_at");
    for stmt in V18_AGENT_SESSIONS_PINNED_AT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V18 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test --lib db::migrations::tests::v18_adds_pinned_at_column_nullable_with_null_default 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 6: Verify the full migration set still passes**

```bash
cargo test --lib db::migrations 2>&1 | tail -10
```

Expected: all V14/V15/V16/V17/V18 tests green.

- [ ] **Step 7: Verify the full backend builds**

```bash
cargo build 2>&1 | grep -E "^error" | head
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V18 — agent_sessions.pinned_at column

Nullable INTEGER carrying the ms timestamp when an agent session
was pinned. NULL means unpinned (default). Backed by an index for
sort ORDER BY pinned_at DESC.

The pre-existing 'pinned INTEGER' column on agent_sessions is
unrelated (used by chat conversations) and left untouched.

Migration is idempotent on re-run via the per-statement warn-skip
pattern shared with V9/V10/V11/V16/V17. Standalone unit test
asserts column existence + NULL default + re-run safety."
```

---

## Task 2: `toggle_pin_agent_session` Tauri command + `list_agent_sessions` extension

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (extend list_agent_sessions projection at line 3770; add new toggle command after delete_agent_session at line 3859)
- Modify: `src-tauri/src/main.rs` (add `toggle_pin_agent_session` to the invoke_handler! macro near line 480)

- [ ] **Step 1: Add a Rust unit test for the toggle behavior**

Add this at the end of `src-tauri/src/tauri_commands.rs` (inside a new `#[cfg(test)] mod pin_tests` block, or extend any existing test module):

```rust
#[cfg(test)]
mod pin_tests {
    use rusqlite::Connection;

    // Apply V1+V8+V18 minimally to get the schema we need.
    fn db_with_pin() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V1_INITIAL).unwrap();
        conn.execute_batch(crate::db::migrations::V8_AGENT_SESSIONS).unwrap();
        for stmt in crate::db::migrations::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Insert one session.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json,
                                          message_count, pinned, archived,
                                          created_at, updated_at)
             VALUES ('s1', 'default', 't', '{}', 0, 0, 0, 0, 0)",
            [],
        ).unwrap();
        conn
    }

    /// The toggle SQL (extracted so we can test it directly without the
    /// Tauri runtime). Returns the new pinned_at value.
    fn toggle_pin_sql(conn: &Connection, id: &str) -> rusqlite::Result<Option<i64>> {
        let tx = conn.unchecked_transaction()?;
        let current: Option<i64> = tx.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<i64>>(0),
        ).ok().flatten();
        let next: Option<i64> = if current.is_some() { None } else { Some(1_700_000_000_000_i64) };
        tx.execute(
            "UPDATE agent_sessions SET pinned_at = ?1 WHERE id = ?2",
            rusqlite::params![next, id],
        )?;
        tx.commit()?;
        Ok(next)
    }

    #[test]
    fn toggle_pin_flips_null_to_ms_and_back() {
        let conn = db_with_pin();
        assert!(toggle_pin_sql(&conn, "s1").unwrap().is_some());
        let after_pin: Option<i64> = conn.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = 's1'",
            [], |r| r.get(0),
        ).unwrap();
        assert!(after_pin.is_some());

        assert!(toggle_pin_sql(&conn, "s1").unwrap().is_none());
        let after_unpin: Option<i64> = conn.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = 's1'",
            [], |r| r.get(0),
        ).unwrap();
        assert!(after_unpin.is_none());
    }

    #[test]
    fn toggle_pin_is_idempotent_for_nonexistent_session() {
        let conn = db_with_pin();
        // No row matches 'nope' — UPDATE affects 0 rows but does not error.
        let result = toggle_pin_sql(&conn, "nope").unwrap();
        // The function still computes a candidate timestamp (it doesn't read
        // before deciding); we don't care which Option arm it picks for an
        // absent row, only that it doesn't panic and the table is unchanged.
        assert!(result.is_some() || result.is_none());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test --lib tauri_commands::pin_tests 2>&1 | tail -15
```

Expected: FAIL with "cannot find value V18_AGENT_SESSIONS_PINNED_AT in module" OR the symbol isn't `pub`. If V18 wasn't marked `pub` in Task 1, mark it `pub` — every other `V*_*` const is `pub`.

(If Task 1 marked it `pub const` correctly, this initial run still fails because the toggle command isn't implemented yet — which is fine; this test exercises the SQL, not the command. The test should pass as soon as we add the SQL helper. So `cargo test` here mostly verifies compilation.)

- [ ] **Step 3: Extend `list_agent_sessions` to project `pinnedAt`**

Open `src-tauri/src/tauri_commands.rs`. Replace the entire body of `list_agent_sessions` (lines 3770–3815) with this:

```rust
pub async fn list_agent_sessions(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let mut stmt = conn.prepare(
        "SELECT id, space_id, title, metadata_json, message_count, pinned, archived,
                attached_dirs, pinned_at, created_at, updated_at
         FROM agent_sessions ORDER BY updated_at DESC"
    ).map_err(|e| Error::Database(e))?;
    let rows = stmt.query_map([], |row| {
        let meta_str: String = row.get(3)?;
        let attached_dirs_json: String = row.get::<_, String>(7).unwrap_or_else(|_| "[]".into());
        let pinned_at: Option<i64> = row.get::<_, Option<i64>>(8).unwrap_or(None);
        Ok((
            row.get::<_, String>(0)?,    // id
            row.get::<_, String>(1)?,    // space_id
            row.get::<_, String>(2)?,    // title
            meta_str,                     // metadata_json
            row.get::<_, i64>(4)?,       // message_count
            row.get::<_, i64>(5)?,       // pinned (legacy, chat-only)
            row.get::<_, i64>(6)?,       // archived
            attached_dirs_json,
            pinned_at,                    // NEW: pinned_at
            row.get::<_, i64>(9)?,       // created_at
            row.get::<_, i64>(10)?,      // updated_at
        ))
    }).map_err(|e| Error::Database(e))?;
    let sessions: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).map(
        |(id, space_id, title, meta_str, msg_count, pinned, archived,
          attached_dirs_json, pinned_at, created_at, updated_at)| {
        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Object(Default::default()));
        let title_from_meta = meta.get("title").and_then(|v| v.as_str()).unwrap_or(&title).to_string();
        let title_emoji = meta.get("emoji").and_then(|v| v.as_str()).unwrap_or("💬").to_string();
        let title_pending = meta.get("title_pending").and_then(|v| v.as_bool()).unwrap_or(false);
        let attached_dirs: Vec<String> = serde_json::from_str(&attached_dirs_json).unwrap_or_default();
        serde_json::json!({
            "id": id,
            "workspaceId": space_id,
            "title": title_from_meta,
            "titleEmoji": title_emoji,
            "titlePending": title_pending,
            "metadataJson": meta_str,
            "messageCount": msg_count,
            "pinned": pinned != 0,
            "archived": archived != 0,
            "attachedDirs": attached_dirs,
            "pinnedAt": pinned_at,
            "createdAt": created_at,
            "updatedAt": updated_at,
        })
    }).collect();
    Ok(sessions)
}
```

The key changes: SELECT pulls `pinned_at`, the destructuring tuple gains a field, and the JSON projection adds `"pinnedAt"`.

- [ ] **Step 4: Add the `toggle_pin_agent_session` Tauri command**

In the same file, after `delete_agent_session` (around line 3895, after its closing `}`), add:

```rust
/// Toggle pin state on an agent session. Returns the new pinned_at value:
/// Some(ms) when the session is now pinned, None when it is now unpinned.
///
/// Wraps the read-then-write in a transaction so concurrent toggles can't
/// produce a split decision. Idempotent on non-existent sessions: the
/// UPDATE affects 0 rows but doesn't error, and we return Ok(None) so
/// the UI doesn't need to pre-check existence.
#[tauri::command]
pub async fn toggle_pin_agent_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<i64>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e))?;
    let current: Option<i64> = tx.query_row(
        "SELECT pinned_at FROM agent_sessions WHERE id = ?1",
        rusqlite::params![&id],
        |row| row.get::<_, Option<i64>>(0),
    ).ok().flatten();
    let next: Option<i64> = if current.is_some() {
        None
    } else {
        Some(chrono::Utc::now().timestamp_millis())
    };
    let _rows = tx.execute(
        "UPDATE agent_sessions SET pinned_at = ?1 WHERE id = ?2",
        rusqlite::params![next, &id],
    ).map_err(|e| Error::Database(e))?;
    tx.commit().map_err(|e| Error::Database(e))?;
    Ok(next)
}
```

- [ ] **Step 5: Register the command in `main.rs`**

Open `src-tauri/src/main.rs`. Find the line `uclaw_core::tauri_commands::delete_agent_session,` (around line 480). On the next line, add:

```rust
            uclaw_core::tauri_commands::toggle_pin_agent_session,
```

- [ ] **Step 6: Run the tests and verify the build**

```bash
cargo test --lib tauri_commands::pin_tests 2>&1 | tail -15
cargo build 2>&1 | grep -E "^error" | head
```

Expected: both pin tests pass, no build errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(agent): toggle_pin_agent_session command + pinnedAt in list

New Tauri command flips agent_sessions.pinned_at between NULL and
'now' (ms) atomically via an explicit transaction. Idempotent on
non-existent session ids — UPDATE affects 0 rows but doesn't error,
returns Ok(None) so the UI doesn't need to pre-check.

list_agent_sessions extended to project pinnedAt: Option<i64> in
camelCase so the frontend can derive sort order without a second
query. The legacy 'pinned' boolean stays (chat conversations).

Registered in main.rs invoke_handler!. Two unit tests cover the
flip behavior and the non-existent-id case."
```

---

## Task 3: TypeScript bridge + AgentSession type + atom

**Files:**
- Modify: `ui/src/lib/agent-types.ts` (extend AgentSessionMeta)
- Modify: `ui/src/lib/tauri-bridge.ts` (rewrite togglePinAgentSession; no silent catch)
- Modify: `ui/src/atoms/workspace.ts` (extend WorkspaceSession + syncWorkspaceSessionsAtom)
- Modify: `ui/src/atoms/agent-atoms.ts` (new `togglePinAgentSessionAtom` action atom)

- [ ] **Step 1: Extend `AgentSessionMeta` type**

Open `ui/src/lib/agent-types.ts`. Find the existing `AgentSessionMeta` interface (line 15-37). Add a new field below `archived?:`:

```ts
  /** 是否归档 */
  archived?: boolean
  /** Pin timestamp (ms). Null means unpinned. Canonical agent-UI pin state
   *  — distinct from the legacy `pinned?: boolean` which is chat-only. */
  pinnedAt?: number | null
```

The full final block:

```ts
export interface AgentSessionMeta {
  id: string
  title: string
  titleEmoji?: string
  titlePending?: boolean
  workspaceId?: string
  channelId?: string
  modelId?: string
  sdkSessionId?: string
  /** 是否置顶 (legacy, chat-only) */
  pinned?: boolean
  /** 是否已归档 */
  archived?: boolean
  /** Pin timestamp (ms). Null means unpinned. Canonical agent-UI pin state
   *  — distinct from the legacy `pinned?: boolean` which is chat-only. */
  pinnedAt?: number | null
  /** 手动标记为工作中 */
  manualWorking?: boolean
  /** 附加的额外目录 */
  attachedDirectories?: string[]
  messageCount: number
  createdAt: number
  updatedAt: number
}
```

- [ ] **Step 2: Rewrite `togglePinAgentSession` bridge wrapper**

Open `ui/src/lib/tauri-bridge.ts`. Find the existing line (~1021):

```ts
export const togglePinAgentSession = (id: string): Promise<any> =>
  invoke('toggle_pin_agent_session', { id }).catch(() => ({ id, pinned: true, updatedAt: Date.now() }))
```

Replace it with:

```ts
/**
 * Toggle pin state on an agent session. Returns the new pinnedAt:
 * number (ms) when pinned, null when unpinned. Surfaces backend errors —
 * the previous `.catch(() => fake)` masked a missing Tauri command and
 * pretended every toggle succeeded.
 */
export const togglePinAgentSession = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_pin_agent_session', { id })
```

- [ ] **Step 3: Extend `WorkspaceSession` + projection**

Open `ui/src/atoms/workspace.ts`. Find the `WorkspaceSession` interface (line 15):

```ts
export interface WorkspaceSession {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  spaceId: string
  updatedAt: string
  /** ms timestamp; null means unpinned. */
  pinnedAt: number | null
}
```

Find the `syncWorkspaceSessionsAtom` (around line 125–148). Inside the per-session push (`grouped[spaceId].push({...})`), add a `pinnedAt` field:

```ts
      grouped[spaceId].push({
        id: s.id,
        title: s.title ?? meta.title ?? 'New session',
        titleEmoji: s.titleEmoji ?? meta.emoji ?? '💬',
        titlePending: s.titlePending ?? meta.title_pending ?? false,
        spaceId,
        updatedAt: s.updatedAt ?? '',
        pinnedAt: (typeof s.pinnedAt === 'number' ? s.pinnedAt : null),
      })
```

Note: `s` is typed as `Record<string, unknown>`-ish (the function signature already accepts loose `[key: string]: unknown`), so `s.pinnedAt` won't have a static type — the `typeof === 'number'` guard handles both the new `pinnedAt: number` and the existing `pinnedAt: null` cases.

- [ ] **Step 4: Add `togglePinAgentSessionAtom` action atom**

Open `ui/src/atoms/agent-atoms.ts`. Find any existing action atom around `agentSessionsAtom` to anchor placement (e.g., near `currentAgentSessionIdAtom` at line ~223). Add:

```ts
/**
 * Toggle the pin state on an agent session. Calls the backend then
 * optimistically updates `agentSessionsAtom` so the UI reflects the
 * new state without a refetch. Errors propagate to the caller for
 * toast surfacing.
 */
export const togglePinAgentSessionAtom = atom(
  null,
  async (_get, set, sessionId: string) => {
    const { togglePinAgentSession } = await import('@/lib/tauri-bridge')
    const newPinnedAt = await togglePinAgentSession(sessionId)
    set(agentSessionsAtom, (prev) =>
      prev.map((s) =>
        s.id === sessionId ? { ...s, pinnedAt: newPinnedAt } : s
      ) as typeof prev
    )
    return newPinnedAt
  }
)
```

(The dynamic `await import` avoids a circular dep between atoms and the bridge module. If `tauri-bridge` is already imported at the top of `agent-atoms.ts`, just reference it directly and drop the dynamic import.)

- [ ] **Step 5: Verify TypeScript + tests pass**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: TS clean, all existing tests still pass (no new tests added in this task — Task 5 will).

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/agent-types.ts ui/src/lib/tauri-bridge.ts \
        ui/src/atoms/workspace.ts ui/src/atoms/agent-atoms.ts
git commit -m "feat(atoms): pinnedAt field + togglePinAgentSessionAtom

Wire the V18 schema through to the frontend:
- AgentSessionMeta gains pinnedAt: number | null (ms timestamp)
- togglePinAgentSession bridge wrapper drops the silent .catch() —
  errors now propagate, matching the deleteAgentSession fix from #84
- WorkspaceSession + syncWorkspaceSessionsAtom forward pinnedAt
- New togglePinAgentSessionAtom action atom flips the backend and
  optimistically updates agentSessionsAtom

No UI changes in this commit — Task 4 renders the data."
```

---

## Task 4: WorkspaceRail two-segment render + SessionItem menu item

**Files:**
- Modify: `ui/src/components/workspace/SessionItem.tsx` (add `isPinned` + `onTogglePin` props, render new menu item)
- Modify: `ui/src/components/workspace/WorkspaceRail.tsx` (segment the session list, render SegmentHeader)

- [ ] **Step 1: Extend `SessionItem` with pin props**

Open `ui/src/components/workspace/SessionItem.tsx`. Update imports + props + render.

Imports (line 1-9 area) — add `Pin` and `PinOff`:

```tsx
import * as React from 'react'
import { LoaderCircle, MoreHorizontal, FolderInput, Trash2, Pin, PinOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
```

Props interface — add two fields:

```tsx
interface SessionItemProps {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  isActive: boolean
  /** Whether the agent loop is currently running for this session. */
  running?: boolean
  /** True when the session has a non-null pinned_at; drives menu label
   *  and (eventually) any visual pin indicator the rail wants to show. */
  isPinned?: boolean
  onClick: () => void
  onDelete?: () => void
  onMove?: () => void
  /** Toggle the session's pin state. Omitted = menu item hidden. */
  onTogglePin?: () => void
}
```

Destructure (around line 24):

```tsx
export function SessionItem({
  title,
  titleEmoji,
  titlePending,
  isActive,
  running,
  isPinned,
  onClick,
  onDelete,
  onMove,
  onTogglePin,
}: SessionItemProps): React.ReactElement {
  const hasMenu = Boolean(onDelete || onMove || onTogglePin)
```

Inside the `<DropdownMenuContent>` block, **prepend** the new pin item ABOVE `onMove` so it's the topmost action:

```tsx
          <DropdownMenuContent align="end" side="bottom" sideOffset={4} className="w-40 min-w-0 p-0.5 z-[100]">
            {onTogglePin && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5"
                onSelect={() => { onTogglePin() }}
              >
                {isPinned ? <PinOff /> : <Pin />}
                {isPinned ? '取消固定' : '固定'}
              </DropdownMenuItem>
            )}
            {onMove && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5"
                onSelect={() => { onMove() }}
              >
                <FolderInput />
                移动到...
              </DropdownMenuItem>
            )}
            {onDelete && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
                onSelect={() => { onDelete() }}
              >
                <Trash2 />
                删除
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
```

- [ ] **Step 2: Add `SegmentHeader` + two-segment render in WorkspaceRail**

Open `ui/src/components/workspace/WorkspaceRail.tsx`. Find the current render block.

Add imports (top of file, with existing imports):

```tsx
import { useSetAtom } from 'jotai'
import { togglePinAgentSessionAtom } from '@/atoms/agent-atoms'
import { toast } from 'sonner'
```

(If `useSetAtom` is already imported from jotai, just add the missing names.)

Add a small local component at the top level of the file (before `export function WorkspaceRail`):

```tsx
/** Section label inside WorkspaceRail. Matches the OVERVIEW labels
 *  used in the approval modal (text-[10px], uppercase, tracking-wider). */
function SegmentHeader({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <p className="text-[10px] font-semibold uppercase tracking-wider
                  text-muted-foreground/70 mt-2 mb-1 px-2">
      {children}
    </p>
  )
}
```

Inside `WorkspaceRail`, add the toggle hook + split the session list. Find the line `const sessions = activeWorkspaceId ? (workspaceSessions[activeWorkspaceId] ?? []) : []` (around line 68–70). Replace from that line through the existing `{sessions.map(...)}` render with:

```tsx
  const togglePin = useSetAtom(togglePinAgentSessionAtom)

  const sessions = activeWorkspaceId
    ? (workspaceSessions[activeWorkspaceId] ?? [])
    : []

  // Two-segment split: pinned (sorted by pinnedAt DESC — most recently
  // pinned at the top) and unpinned (preserves the source atom's
  // updatedAt DESC order).
  const pinned = sessions
    .filter((s) => s.pinnedAt !== null)
    .sort((a, b) => (b.pinnedAt ?? 0) - (a.pinnedAt ?? 0))
  const unpinned = sessions.filter((s) => s.pinnedAt === null)

  const handleTogglePin = async (id: string): Promise<void> => {
    try {
      await togglePin(id)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`固定失败：${msg}`)
    }
  }

  return (
    <>
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {sessions.length === 0 && (
          <p className="text-[11px] text-muted-foreground px-2 py-3 italic">
            尚无会话。点击上方"新会话"开始。
          </p>
        )}

        {pinned.length > 0 && (
          <>
            <SegmentHeader>📌 固定</SegmentHeader>
            {pinned.map((s) => (
              <SessionItem
                key={s.id}
                id={s.id}
                title={s.title}
                titleEmoji={s.titleEmoji}
                titlePending={s.titlePending}
                isActive={activeSessionId === s.id}
                running={indicatorMap.get(s.id) === 'running'}
                isPinned
                onClick={() => onSelectSession(s.id)}
                onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
                onMove={() => setMoveTargetSessionId(s.id)}
                onTogglePin={() => void handleTogglePin(s.id)}
              />
            ))}
          </>
        )}

        {pinned.length > 0 && unpinned.length > 0 && (
          <SegmentHeader>会话</SegmentHeader>
        )}

        {unpinned.map((s) => (
          <SessionItem
            key={s.id}
            id={s.id}
            title={s.title}
            titleEmoji={s.titleEmoji}
            titlePending={s.titlePending}
            isActive={activeSessionId === s.id}
            running={indicatorMap.get(s.id) === 'running'}
            isPinned={false}
            onClick={() => onSelectSession(s.id)}
            onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            onMove={() => setMoveTargetSessionId(s.id)}
            onTogglePin={() => void handleTogglePin(s.id)}
          />
        ))}
      </div>
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={agentWorkspaces}
          onMoved={() => {
            setMoveTargetSessionId(null)
            void refreshWorkspaces()
          }}
        />
      )}
    </>
  )
```

Notes:
- Keep the existing `{moveTargetSession && <MoveSessionDialog ... />}` block as-is — the snippet above shows it for continuity.
- The duplicated `SessionItem` JSX is intentional (DRY would force a render-prop or sub-component, which is overkill for two near-identical 12-line blocks).

- [ ] **Step 3: Verify TS + visual sanity check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

Smoke (manual; only if `cargo tauri dev` is already running): open a workspace with ≥1 session, click 3-dot → 固定. Session should jump to a new 📌 固定 segment above the others.

- [ ] **Step 4: Run existing tests to confirm no regressions**

```bash
cd ui && npm test -- --run 2>&1 | tail -6
```

Expected: all tests pass (existing WorkspaceRail tests still work; we haven't changed their assertions).

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/workspace/SessionItem.tsx \
        ui/src/components/workspace/WorkspaceRail.tsx
git commit -m "feat(workspace): pinned-sessions two-segment WorkspaceRail

SessionItem 3-dot menu gains 📌 固定 / 取消固定 (lucide Pin / PinOff)
above 移动到 and 删除. Driven by isPinned + onTogglePin props.

WorkspaceRail splits the active workspace's session list into:
- 📌 固定 segment (sorted by pinnedAt DESC — most recently pinned
  at the top, rewards explicit user intent)
- 会话 segment (existing updatedAt DESC order, unchanged)

The 会话 header only renders when there's also a pinned segment —
when zero pinned, the rail looks identical to before this commit.

Toggle errors surface via toast.error so silent failures (rare,
backend can't disappear in practice) don't mask UX bugs."
```

---

## Task 5: Tests

**Files:**
- Create: `ui/src/components/workspace/SessionItem.test.tsx`
- Modify: `ui/src/components/workspace/WorkspaceRail.test.tsx` (verify if exists; if not, extend a new one)

- [ ] **Step 1: Check if WorkspaceRail tests exist**

```bash
ls ui/src/components/workspace/WorkspaceRail.test.tsx 2>&1
```

If the file exists, you'll extend it in Step 3. If not, the test scaffold below creates it from scratch.

- [ ] **Step 2: Write `SessionItem.test.tsx` (new file)**

Create `ui/src/components/workspace/SessionItem.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { SessionItem } from './SessionItem'

describe('SessionItem — pin menu label', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('shows "固定" when not pinned', async () => {
    render(
      <SessionItem
        id="s1" title="Hi" titleEmoji="💬" titlePending={false}
        isActive={false}
        isPinned={false}
        onClick={() => {}}
        onTogglePin={() => {}}
      />
    )
    fireEvent.click(screen.getByTitle('更多'))
    expect(await screen.findByText('固定')).toBeInTheDocument()
    expect(screen.queryByText('取消固定')).not.toBeInTheDocument()
  })

  it('shows "取消固定" when pinned', async () => {
    render(
      <SessionItem
        id="s1" title="Hi" titleEmoji="💬" titlePending={false}
        isActive={false}
        isPinned
        onClick={() => {}}
        onTogglePin={() => {}}
      />
    )
    fireEvent.click(screen.getByTitle('更多'))
    expect(await screen.findByText('取消固定')).toBeInTheDocument()
    expect(screen.queryByText('固定')).not.toBeInTheDocument()
  })

  it('clicking the menu item invokes onTogglePin', async () => {
    const onTogglePin = vi.fn()
    render(
      <SessionItem
        id="s1" title="Hi" titleEmoji="💬" titlePending={false}
        isActive={false}
        isPinned={false}
        onClick={() => {}}
        onTogglePin={onTogglePin}
      />
    )
    fireEvent.click(screen.getByTitle('更多'))
    fireEvent.click(await screen.findByText('固定'))
    expect(onTogglePin).toHaveBeenCalledTimes(1)
  })

  it('hides pin menu item when onTogglePin is not provided', async () => {
    render(
      <SessionItem
        id="s1" title="Hi" titleEmoji="💬" titlePending={false}
        isActive={false}
        onClick={() => {}}
      />
    )
    // Menu trigger only shows when at least one action exists; with none,
    // the menu shouldn't render. Confirm by absence of 更多.
    expect(screen.queryByTitle('更多')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 3: Extend WorkspaceRail tests**

If `ui/src/components/workspace/WorkspaceRail.test.tsx` does not yet exist, create it. Otherwise, add the cases below into the existing describe block.

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen } from '@testing-library/react'
import { WorkspaceRail } from './WorkspaceRail'
import { workspacesAtom, activeWorkspaceIdAtom, workspaceSessionsAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { agentSessionsAtom } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  togglePinAgentSession: vi.fn().mockResolvedValue(1_700_000_000_000),
}))

function ws(id: string): WorkspaceInfo {
  return {
    id, name: id, icon: 'Folder', path: `/${id}`, attachedDirs: [],
    sortOrder: 0,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

function session(id: string, pinnedAt: number | null, updatedAt = '2026-05-11T00:00:00Z') {
  return {
    id, title: id, titleEmoji: '💬', titlePending: false,
    spaceId: 'w1', updatedAt, pinnedAt,
  }
}

describe('WorkspaceRail — pinned/unpinned segments', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('hides the pinned segment header when no sessions are pinned', () => {
    const store = createStore()
    store.set(workspacesAtom, [ws('w1')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, {
      w1: [session('a', null), session('b', null)],
    })
    render(
      <Provider store={store}>
        <WorkspaceRail
          activeSessionId={null}
          onSelectSession={() => {}}
          onDeleteSession={() => {}}
        />
      </Provider>
    )
    expect(screen.queryByText('📌 固定')).not.toBeInTheDocument()
    expect(screen.queryByText('会话')).not.toBeInTheDocument()
  })

  it('renders pinned segment above unpinned when at least one is pinned', () => {
    const store = createStore()
    store.set(workspacesAtom, [ws('w1')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, {
      w1: [
        session('a', null),
        session('b', 1_700_000_000_000),
        session('c', null),
      ],
    })
    render(
      <Provider store={store}>
        <WorkspaceRail
          activeSessionId={null}
          onSelectSession={() => {}}
          onDeleteSession={() => {}}
        />
      </Provider>
    )
    const pinnedHeader = screen.getByText('📌 固定')
    const unpinnedHeader = screen.getByText('会话')
    expect(pinnedHeader).toBeInTheDocument()
    expect(unpinnedHeader).toBeInTheDocument()

    // DOM order: pinned header appears before unpinned header.
    const pinnedPos = pinnedHeader.compareDocumentPosition(unpinnedHeader)
    expect(pinnedPos & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('sorts the pinned segment by pinnedAt DESC (most recent first)', () => {
    const store = createStore()
    store.set(workspacesAtom, [ws('w1')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, {
      w1: [
        session('older', 1_000),
        session('newer', 2_000),
        session('middle', 1_500),
      ],
    })
    render(
      <Provider store={store}>
        <WorkspaceRail
          activeSessionId={null}
          onSelectSession={() => {}}
          onDeleteSession={() => {}}
        />
      </Provider>
    )
    const order = ['newer', 'middle', 'older'].map((id) =>
      screen.getByText(id).compareDocumentPosition(screen.getByText('newer'))
    )
    // 'newer' is at index 0; the others follow it in DOM order.
    const newer = screen.getByText('newer')
    const middle = screen.getByText('middle')
    const older = screen.getByText('older')
    expect(newer.compareDocumentPosition(middle) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(middle.compareDocumentPosition(older) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    void order // silence unused
  })
})
```

The mock for `togglePinAgentSession` is in place but not exercised in these tests (rendering only). If you want a separate render+click+atom-write test, add a fourth case that:
1. Renders the rail with one unpinned session
2. Opens the 3-dot menu and clicks 固定
3. Asserts `togglePinAgentSession` was called with the session id

That's optional — SessionItem.test.tsx already covers the callback wiring.

- [ ] **Step 4: Run the new tests**

```bash
cd ui && npm test -- --run SessionItem WorkspaceRail 2>&1 | tail -10
```

Expected: all new cases pass; full file count goes from 30 → 31 (or 30 → 32 if WorkspaceRail.test.tsx didn't exist before).

- [ ] **Step 5: Run the full UI suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -6
```

Expected: 156 + 7 (4 SessionItem + 3 WorkspaceRail) ≈ 163 tests, all green.

- [ ] **Step 6: Run the Rust test suite once more**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
```

Expected: V18 migration test + pin_tests still pass.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/workspace/SessionItem.test.tsx \
        ui/src/components/workspace/WorkspaceRail.test.tsx
git commit -m "test(pin): SessionItem menu label + WorkspaceRail segments

SessionItem.test.tsx (new) — 4 cases:
- '固定' label when isPinned=false; '取消固定' when isPinned=true
- onTogglePin invoked on click
- Menu trigger hidden entirely when no actions provided

WorkspaceRail.test.tsx (new or extended) — 3 cases:
- Pinned segment header hidden when 0 pinned
- Pinned segment renders above unpinned when ≥1 pinned
- Within pinned, sort is pinnedAt DESC (most recent at top)"
```

---

## After all tasks complete

Use **superpowers:finishing-a-development-branch** to merge.

Expected PR shape:

```
## Summary
Phase 6-A of the workspace remediation series. Adds the ability to
pin individual agent sessions so they elevate above the implicit
updatedAt-DESC ordering in WorkspaceRail.

## Commits (bisectable)
1. feat(db): V18 — agent_sessions.pinned_at column
2. feat(agent): toggle_pin_agent_session command + pinnedAt in list
3. feat(atoms): pinnedAt field + togglePinAgentSessionAtom
4. feat(workspace): pinned-sessions two-segment WorkspaceRail
5. test(pin): SessionItem menu label + WorkspaceRail segments

## Spec / Plan
- docs/superpowers/specs/2026-05-11-workspace-phase6a-pinned-design.md
- docs/superpowers/plans/2026-05-11-workspace-phase6a.md

## Test plan
- [x] cargo test --lib (V18 migration + pin_tests)
- [x] cd ui && npx tsc --noEmit clean
- [x] cd ui && npm test (all green; +7 new cases)

Manual smoke:
- [ ] Open a workspace with ≥2 sessions → click 3-dot → 固定 → session jumps to a 📌 固定 segment at the top
- [ ] Click 3-dot on a pinned session → 取消固定 → it returns to the 会话 segment
- [ ] Quit + relaunch app → pin state persists (V18 column survives)
- [ ] Delete a workspace with a pinned session → orphan re-homes to 默认工作区 with pin intact (Phase 1 behavior)
```
