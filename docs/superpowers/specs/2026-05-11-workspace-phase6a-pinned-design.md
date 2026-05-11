# Phase 6-A — Pinned Sessions

> **Sub-feature of [Phase 6](./2026-05-11-workspace-phase6-design.md).**
> First to ship. Adds the ability to pin high-value sessions above the
> implicit recency ordering in WorkspaceRail.

## 1. Problem

WorkspaceRail sorts sessions by `updatedAt DESC` — the most recently
active session is always on top. For a power user with 10+ active
sessions in a workspace, this means "current ticket" / "scratchpad" /
"reference session" get pushed below whatever new session they opened
five minutes ago. No way to elevate intent over recency.

## 2. Goal

Let users pin individual sessions. Pinned sessions render in a
dedicated segment above unpinned ones in WorkspaceRail. Pin state is
persistent (lives in the DB on the session row), survives workspace
moves, and survives workspace deletion (re-homing to default doesn't
clear pin).

## 3. Non-Goals

- Cross-workspace pin limit / quota.
- Drag-to-reorder pinned sessions (use recency-of-pin instead — see §4).
- Pinning a session into a specific position (e.g., "pin to slot 1") —
  the segment is recency-sorted internally.
- Bulk pin/unpin operations.

## 4. Data Model

V18 migration adds one nullable column to `agent_sessions`:

```sql
-- src-tauri/src/db/migrations.rs
ALTER TABLE agent_sessions ADD COLUMN pinned_at INTEGER NULL;
```

Semantics:
- `NULL` = not pinned (default for existing rows + new sessions)
- `INTEGER` (ms timestamp) = pinned at this time

Sort within WorkspaceRail (Two-pass):
1. **Pinned segment** (rows with `pinned_at NOT NULL`), sorted by
   `pinned_at DESC` — most recently pinned at the top. Reasoning:
   pinning is an explicit user action; reward intent. If a user
   re-pins an older session, it surfaces.
2. **Unpinned segment** (rows with `pinned_at IS NULL`), sorted by
   `updatedAt DESC` — preserves existing behavior.

## 5. Backend (Rust)

### New Tauri command

```rust
// src-tauri/src/tauri_commands.rs

/// Toggle pin state on an agent session. Returns the new pinned_at
/// value (Some(ms) when pinned, None when unpinned). Wraps in a
/// transaction so the read-then-write race is atomic. Idempotent on
/// non-existent sessions (returns Ok(None) without erroring) so the
/// UI doesn't need to refetch before calling.
#[tauri::command]
pub async fn toggle_pin_agent_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<i64>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let tx = conn.unchecked_transaction().map_err(Error::Database)?;
    let current: Option<i64> = tx.query_row(
        "SELECT pinned_at FROM agent_sessions WHERE id = ?1",
        rusqlite::params![&id],
        |row| row.get::<_, Option<i64>>(0),
    ).optional().map_err(Error::Database)?.flatten();
    let next = if current.is_some() { None } else { Some(chrono::Utc::now().timestamp_millis()) };
    tx.execute(
        "UPDATE agent_sessions SET pinned_at = ?1 WHERE id = ?2",
        rusqlite::params![next, &id],
    ).map_err(Error::Database)?;
    tx.commit().map_err(Error::Database)?;
    Ok(next)
}
```

Register in `src-tauri/src/main.rs` invoke_handler! macro.

### `list_agent_sessions` extension

Existing command already returns `pinned: bool`. Add `pinnedAt:
Option<i64>` so the frontend can derive sort order without re-querying.

```rust
// Inside the JSON projection in list_agent_sessions:
serde_json::json!({
    "id": id,
    "workspaceId": space_id,
    // ... existing fields ...
    "pinnedAt": pinned_at, // NEW
})
```

The existing `pinned` boolean stays (used elsewhere for the pinned-chat
flow on conversations).

## 6. Frontend

### Atoms

`ui/src/atoms/agent-atoms.ts` — extend the `AgentSession` type:

```ts
export interface AgentSession {
  // ... existing fields ...
  pinnedAt: number | null
}
```

`ui/src/atoms/workspace.ts` — `syncWorkspaceSessionsAtom` already builds
the per-workspace grouped list. Extend `WorkspaceSession` with
`pinnedAt: number | null` and forward it through the projection.

### UI: WorkspaceRail two-segment render

```tsx
// ui/src/components/workspace/WorkspaceRail.tsx

const pinned = sessions
  .filter((s) => s.pinnedAt !== null)
  .sort((a, b) => (b.pinnedAt ?? 0) - (a.pinnedAt ?? 0))
const unpinned = sessions
  .filter((s) => s.pinnedAt === null)
  // existing updatedAt sort stays (whatever the source atom already
  // does — don't change it)

return (
  <>
    {pinned.length > 0 && (
      <>
        <SegmentHeader>📌 固定</SegmentHeader>
        {pinned.map((s) => <SessionItem key={s.id} ... />)}
      </>
    )}
    {pinned.length > 0 && unpinned.length > 0 && (
      <SegmentHeader>会话</SegmentHeader>
    )}
    {unpinned.map((s) => <SessionItem key={s.id} ... />)}
  </>
)
```

`SegmentHeader` is a new tiny component:

```tsx
function SegmentHeader({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-[10px] font-semibold uppercase tracking-wider
                  text-muted-foreground/70 mt-2 mb-1 px-2">
      {children}
    </p>
  )
}
```

The "会话" header only shows when pinned segment is non-empty (visual
separator). When zero pinned, the rail looks identical to today.

### UI: SessionItem 3-dot menu

Add a new menu item at the top:

```tsx
{onTogglePin && (
  <DropdownMenuItem onSelect={() => onTogglePin()}>
    {isPinned ? <PinOff /> : <Pin />}
    {isPinned ? '取消固定' : '固定'}
  </DropdownMenuItem>
)}
```

`Pin` and `PinOff` icons from lucide-react. Position above 移动到... and
删除. Existing event-bubbling fix from PR #84 (click-eater wrapper)
already covers this — no new event-handling work.

### Atom wiring: `togglePinAgentSessionAtom`

```ts
// ui/src/atoms/agent-atoms.ts
export const togglePinAgentSessionAtom = atom(
  null,
  async (get, set, sessionId: string) => {
    const newPinnedAt = await togglePinAgentSession(sessionId) // Tauri IPC
    set(agentSessionsAtom, (prev) =>
      prev.map((s) => s.id === sessionId ? { ...s, pinnedAt: newPinnedAt } : s),
    )
  },
)
```

WorkspaceRail's `onTogglePin` calls `useSetAtom(togglePinAgentSessionAtom)`.

## 7. Edge Cases

- **Existing rows after migration**: `pinned_at` defaults to NULL via
  ALTER TABLE — no backfill needed; every existing session starts
  unpinned. ✓
- **Workspace deletion**: Phase 1 re-homes orphan sessions to
  'default'. `pinned_at` is preserved (lives on the session row).
  Pinned sessions just appear under default workspace's rail. ✓
- **Move session to another workspace**: Phase 2's
  `move_agent_session_to_workspace` only touches `space_id` — pinned
  state survives the move. ✓
- **Pin then re-pin** (rare): `toggle_pin_agent_session` flips state
  unconditionally. To re-pin an already-pinned session (refreshing
  sort order), call unpin then pin. Future tweak if asked: an
  explicit `pin` command that always sets `pinned_at = now`
  regardless of current state.
- **`pinned` boolean field collision**: `agent_sessions` already has a
  `pinned INTEGER` column (used by conversations, V8). It's NOT used
  by agent UI today. Phase 6-A uses `pinned_at` as the canonical
  source; the old `pinned` column is left untouched. If anything ever
  reads it for agent sessions, it'll see 0 and that's the same as
  before. No risk of split-brain because the two columns are
  effectively independent records.

## 8. Tests

Vitest:

- `togglePinAgentSessionAtom` flips state through the IPC mock,
  updates `agentSessionsAtom` in place.
- WorkspaceRail renders pinned segment header only when ≥1 pinned
  session.
- WorkspaceRail renders pinned segment first, then unpinned segment.
- Pinned segment sort is `pinnedAt DESC` (most recent first).
- SessionItem 3-dot menu shows "固定" when `pinnedAt == null`, "取消固定"
  when `pinnedAt != null`.

Rust unit (inline `#[cfg(test)]`):

- `toggle_pin_agent_session` flips NULL → ms → NULL on consecutive
  calls.
- Idempotent for non-existent id (returns Ok(None), no error).
- Cascade: pinning a session, then deleting the workspace, leaves
  pinned_at intact on the re-homed row (covered by Phase 1 test if
  any; otherwise add a quick check).

## 9. Commit Shape (5 commits)

1. `feat(db): V18 — agent_sessions.pinned_at column`
2. `feat(agent): toggle_pin_agent_session Tauri command + list_agent_sessions returns pinnedAt`
3. `feat(atoms): AgentSession.pinnedAt + togglePinAgentSessionAtom`
4. `feat(workspace): WorkspaceRail two-segment render (pinned + unpinned)`
5. `test(pin): unit tests for the toggle command + atom + rendering`

Bisectable each step: after commit 1, DB has the column but nothing
reads it (no-op). After commit 2, backend works but UI doesn't show
pin state. After commit 3, atoms can be set but UI doesn't render
differently. After commit 4, full feature is live. Commit 5 adds
safety net.

## 10. Risks

- **`pinned` vs `pinned_at` column name confusion**: the existing
  `pinned INTEGER` was for chat conversations. Mentioned in §7. The
  new `pinned_at` lives next to it on the same table because moving
  to a separate "pin events" table would be over-engineering for a
  single nullable timestamp.
- **Sort drift if `pinnedAt` is updated mid-render**: not a real
  concern — React renders are atomic per atom snapshot.
- **Migration on a populated DB**: ALTER TABLE ADD COLUMN with NULL
  default is fast (SQLite doesn't rewrite the table). No perf concern.
