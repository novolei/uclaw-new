# SessionStore Module TDD Plan

> **For agentic workers:** Execute this plan step by step. Keep the slice
> focused on the stale-tree fork invariant.

**Goal:** Deepen the existing SQLite `session_tree` Module so fork callers get
fresh parent-link lineage without knowing whether the tree was previously
materialized.

**Spec:** `docs/superpowers/specs/2026-06-01-session-store-module-design.md`

## Recon Notes

- uClaw `fork_at` copies messages from `agent_messages`, then records the fork
  edge via `node_for_message`.
- `materialize_session_tree` currently exits early when any tree rows exist.
- The code already documents that appended messages after materialization lose
  the fork edge.
- Pi `SessionStoreV2` treats closed parent links and active-path replay as
  explicit invariants.
- Pi `SessionIndex` refreshes derived data when its source changes, instead of
  forcing callers to manage freshness.
- GitNexus impact for `materialize_session_tree`, `fork_at`, and
  `node_for_message` returned UNKNOWN because the symbols were not resolved in
  the index. Source search found one production caller path through
  `tauri_commands.rs`.

## Files

- Modify: `src-tauri/src/agent/session_tree.rs`
- Update: `docs/superpowers/plans/2026-05-31-pi-modernization-six-modules.md`

## Steps

- [x] **Step 1: Add failing stale-tree tests**

Add tests in `session_tree.rs` proving:

1. `materialize_session_tree` refreshes a previously materialized tree after a
   new message is appended.
2. `fork_at` records a cross-session edge when forking at that appended
   message.

Run:

```bash
cargo test --lib agent::session_tree::tests::materialize_refreshes_stale_tree_after_append -- --nocapture
cargo test --lib agent::session_tree::tests::fork_at_records_edge_for_message_appended_after_materialize -- --nocapture
```

Observed before implementation: both failed because the stale tree was not
refreshed.

- [x] **Step 2: Implement tree freshness behind the existing Interface**

Add a private freshness helper that:

1. Counts `agent_messages` for the session.
2. Counts message nodes in `session_tree`.
3. Checks whether a leaf exists when messages exist.
4. Appends missing message nodes when the tree trails the source messages.
5. Deletes and rebuilds tree rows plus leaf rows when the tree is ahead of the
   source or leaf state is invalid.

Keep public function names unchanged.

- [x] **Step 3: Remove stale known-gap behaviour**

Update `fork_at` to rely on the fresh tree before resolving the source node.
Remove the comment that says the edge can be silently skipped for appended
messages. Keep the copy path and result shape unchanged.

- [x] **Step 4: Verify focused tests**

Run:

```bash
cargo test --lib agent::session_tree -- --nocapture
git diff --check
```

Observed: all `session_tree` tests passed and whitespace check had no output.

- [x] **Step 5: Run GitNexus detect-changes and commit**

Run:

```bash
git status --short
```

Then stage the SessionStore files and run:

```bash
gitnexus detect-changes --scope staged
git commit
```

Commit body must include the verification commands and expected passing output.

Observed before commit: GitNexus `detect_changes` on staged files reported
`risk_level: none`.

## Continuation: SessionStore Snapshot Interface

**Goal:** Complete the parent spec's replay/index/compaction acceptance items
without introducing Pi's segment files or a new schema. Borrow Pi's deeper
Interface idea: one SessionStore read seam should refresh derived state and
report invariants, rather than forcing callers to compose raw tables.

**GitNexus pre-edit:** `impact(materialize_session_tree, upstream)` returned
`UNKNOWN` in this worktree because the symbol was not found in the index. The
slice remains limited to `session_tree.rs` and this plan/spec.

- [x] **Step 6: Add failing snapshot Interface tests**

Add tests proving:

1. replay entries are ordered by `created_at ASC, rowid ASC` and expose stable
   sequence numbers plus message node IDs.
2. a stale materialized tree is refreshed before index health is reported.
3. the latest compacted message resolves to a compaction anchor and tree node.

Expected before implementation: compile failure for missing
`load_session_store_snapshot` and snapshot types.

- [x] **Step 7: Implement the snapshot Interface**

Add small serializable structs for replay entries, index health, compaction
anchor, and snapshot. Implement `load_session_store_snapshot` by refreshing
`materialize_session_tree`, reading replay rows from `agent_messages`, resolving
tree nodes, and selecting the latest `compacted != 0` message as the anchor.

- [x] **Step 8: Verify and commit**

Run:

```bash
cargo test --lib agent::session_tree -- --nocapture
git diff --check
gitnexus detect-changes --scope staged
```

Commit body must include the focused command output.

Observed before commit:

- RED: `cargo test --lib agent::session_tree::tests::snapshot_returns_replay_entries_in_message_order -- --nocapture` failed with missing `load_session_store_snapshot`.
- GREEN: `cargo test --lib agent::session_tree -- --nocapture` passed 11 tests.
