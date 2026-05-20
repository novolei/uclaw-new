---
name: uclaw-migrations
description: Use whenever you need to add, modify, or coordinate a SQLite migration in src-tauri/src/db/migrations.rs. Trigger words include "migration", "schema change", "ALTER TABLE", "CREATE TABLE", "new V-number", "FTS index", "V40 / V41 / V42", "agent_messages columns", or any work that adds/removes columns/indexes/tables. Loads the V-number registry, idempotency rules, FTS backfill checklist, and the cross-PR coordination ritual that prevents the #1 merge accident in this repo (two PRs reusing the same V-number).
---

# uClaw — Database Migrations

`src-tauri/src/db/migrations.rs` holds every schema migration as embedded
`const`-string SQL blocks. Migrations run on **every startup** against the
opened connection, in V-number order. Each one must be **idempotent** so a
mid-update interrupt doesn't break the next launch.

## Hard rules — break these and the merge fails

1. **Pick the next free V-number across BOTH merged AND open PRs.** Two PRs
   claiming the same V is this repo's #1 merge accident. See `CONTEXT.md`
   *Active migration registry* (or `CLAUDE.md` Part 2 if CONTEXT.md not yet
   split) for the current table, including in-progress PRs.
2. **Every migration is idempotent.** Use `CREATE TABLE IF NOT EXISTS`,
   `ALTER TABLE … ADD COLUMN` wrapped in a try-once-catch-error loop, or
   `INSERT … WHERE NOT EXISTS`. NEVER write a migration that fails on
   second run.
3. **Update the V-table in `CLAUDE.md` (or `CONTEXT.md` post-refactor) in
   the SAME PR** as the migration. Don't promise a "follow-up doc update".
4. **No DROP/REMOVE columns or tables** without the DRI's explicit OK —
   downgrade-safety matters on a desktop app where users hold old data.
   Mark columns deprecated; sweep later in a coordinated PR.

## Procedure — adding a migration

1. **Check the registry** (CLAUDE.md/CONTEXT.md table). Note the highest V
   number across both merged and `gh pr list --state open` open PRs.
   ```bash
   gh pr list --state open --search "migration" --json number,title,headRefName
   ```
2. **Pick the next free V**. If you see two open PRs that both claim V42,
   raise the conflict in chat before continuing — don't quietly bump.
3. **Add the SQL block** in `src-tauri/src/db/migrations.rs`. Match the
   existing format (each migration is a `pub(crate) const VNN: &str = …`
   string, registered in the array at the bottom of the file).
4. **Make it idempotent**. Test by running `cargo tauri dev` twice — second
   run must produce zero error logs.
5. **If you added an FTS-backed table**: backfill in the same migration:
   ```sql
   INSERT INTO mytable_fts(rowid, col1, col2)
   SELECT rowid, col1, col2 FROM mytable
   WHERE rowid NOT IN (SELECT rowid FROM mytable_fts);
   ```
   Without the backfill, FTS misses every row that pre-dates the migration.
6. **Update the V-table** in CLAUDE.md (or CONTEXT.md) — same PR.
7. **Run the build verification**:
   ```bash
   cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
   cd src-tauri && cargo test --lib 2>&1 | tail -20
   ```

## The two-table-per-domain trap

Chat lives in `messages` (small / often empty on a dev DB). Agent lives in
`agent_messages` (visible conversation) AND `agent_turns` (per-tool-call
breakdown). When adding index, FTS, or migration work for "conversations":

- Most dev DBs have ≫ rows in `agent_messages` / `agent_turns`, often 0 in
  `messages`.
- If your change is supposed to cover "all conversations", you almost
  always need to touch BOTH the chat and agent tables.
- Search functions (e.g. `search_conversations`) use a UNION-of-branches
  pattern (flat enumeration over chat + agent tables). Add a new branch
  in the same file rather than introducing an abstraction layer.

## Examples

### Good — V45 adding a column with idempotent ADD + index

```rust
pub(crate) const V45: &str = r#"
-- V45: add spaced_repetition_state for L3 §4.12.3
CREATE TABLE IF NOT EXISTS spaced_repetition_state (
    entity_page_id TEXT PRIMARY KEY,
    interval_days INTEGER NOT NULL DEFAULT 1,
    ease_factor REAL NOT NULL DEFAULT 2.5,
    next_review_at TEXT NOT NULL,
    last_reviewed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_srs_next_review
    ON spaced_repetition_state(next_review_at);
"#;
```

### Bad — non-idempotent rename

```rust
// 💥 Re-running fails because column already renamed.
const V_BAD: &str = "ALTER TABLE foo RENAME COLUMN bar TO baz;";
```

Use this idempotent pattern instead:

```rust
const V_GOOD: &str = r#"
-- SQLite has no IF NOT EXISTS for ALTER. Try once; ignore "duplicate column" errors.
ALTER TABLE foo ADD COLUMN baz TEXT;
UPDATE foo SET baz = bar WHERE baz IS NULL;
"#;
```

(The migration loop in `migrations.rs` has an explicit catch for
"duplicate column" SQLite errors that lets idempotent ADDs pass.)

## See also

- `CLAUDE.md` Part 2 — *Active migration registry* (current V-number table)
- `CONTEXT.md` *Active migration registry* (if the split has happened)
- `src-tauri/src/db/migrations.rs` — every existing migration as reference
- BEHAVIOR.md §"uClaw-specific rules" — DMZ list (migrations.rs is in DMZ)
