# SessionStore Module Design

**Date:** 2026-06-01
**Status:** Child spec, implementation pending
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi_agent_rust/src/session_store_v2.rs`, `/Users/ryanliu/Documents/pi_agent_rust/src/session_index.rs`

## Problem

uClaw has a useful `session_tree` Module for fork and rewind lineage, but its
Interface is still shallow. Callers must know that `agent_messages` is the
source of truth, that `session_tree` is lazily materialized, and that a tree can
become stale after new messages are appended.

The current `fork_at` path documents the sharp edge directly in the
Implementation: if a source session was materialized earlier and later receives
more messages, new messages have no tree nodes. Fork still copies messages from
`agent_messages`, but the cross-session lineage edge is skipped because
`node_for_message` cannot find the target node.

That violates the intended SessionStore invariant: if a fork copies through a
message, the fork edge should point at the source node for that message.

## Goal

Deepen the session persistence seam into a `SessionStore`-shaped Module that
keeps derived lineage current before fork/rewind operations. The first slice
keeps SQLite as the storage Adapter and concentrates the invariant repair in
`session_tree`:

```text
Tauri command
  -> session_tree::fork_at(...)
       -> ensure source tree matches agent_messages
       -> copy messages
       -> materialize new session tree
       -> record cross-session fork edge
```

Callers should not need to know whether the tree was empty, fresh, or stale.

## Current uClaw Truth

- `src-tauri/src/agent/session_tree.rs` owns tree materialization, leaf
  tracking, fork, rewind, and tests.
- `materialize_session_tree` is idempotent only when no tree rows exist.
- `fork_at` calls `materialize_session_tree` before reading the fork target,
  so existing but stale tree rows are not repaired.
- `rewind_to` deletes and rebuilds tree rows after truncating messages, so it
  already refreshes the derived tree for its own write path.
- `src-tauri/src/tauri_commands.rs` delegates `fork_agent_session` and
  `rewind_session` to `session_tree`, after checking that the session is not
  currently running.

GitNexus could not resolve the `session_tree` symbols in this worktree and
reported UNKNOWN risk. Source search shows the production blast radius for this
slice is limited to the Tauri fork/rewind commands plus local tests.

## Pi Reference Truth

Pi Rust's `SessionStoreV2` is deeper than uClaw's current tree helper:

- Frames carry `entry_id`, `parent_entry_id`, payload hashes, segment sequence,
  and entry sequence.
- `read_active_path` validates duplicate IDs, cyclic parent chains, missing
  parent entries, and index/frame mismatches.
- The manifest records invariants such as `parent_links_closed`,
  `monotonic_entry_seq`, `index_within_segment_bounds`, and
  `hash_chain_valid`.
- `SessionIndex` separates derived listing data from session content and
  refreshes incrementally when file metadata changes.

The transferable design is not the file format. The useful idea is that
derived session indexes must be refreshed at the Module seam, and parent-link
integrity must be an explicit invariant.

## uClaw Adaptation

The first implementation slice adds a freshness check before fork:

- Compare `agent_messages` count for a session with its `session_tree` message
  node count.
- Treat missing leaf rows as stale when messages exist.
- Incrementally append missing message nodes when the tree trails
  `agent_messages`, preserving existing node IDs and prior fork edges.
- Rebuild tree rows from `agent_messages` only when the derived tree is ahead
  of the source or cannot identify a leaf.
- Keep `materialize_session_tree` as the public compatibility Interface, but
  make it ensure freshness rather than only fill an empty tree.
- Replace the known-gap comment with tests proving the edge is recorded after
  post-materialization appends.

This borrows Pi's `parent_links_closed` invariant and incremental-index mindset
while preserving uClaw's existing SQLite schema and command Interface.

## Interface

The first slice keeps the public API stable:

```rust
materialize_session_tree(conn, session_id) -> Result<(), Error>
fork_at(conn, source_session, up_to_message_id) -> Result<ForkResult, Error>
rewind_to(conn, session_id, target_message_id) -> Result<RewindResult, Error>
```

The deepened behaviour is behind that Interface:

- `materialize_session_tree` returns with message tree rows current for the
  session's messages.
- `fork_at` can rely on `node_for_message` for any copied source message.
- `rewind_to` continues to rebuild after truncation.

## Acceptance Evidence

- A test proves `materialize_session_tree` rebuilds when `agent_messages`
  contains more messages than `session_tree`.
- A test proves a session materialized before a later append can be forked at
  the appended message and records the cross-session edge.
- Existing materialization, path, leaf, fork, rewind, and not-found tests still
  pass.
- `cargo test --lib agent::session_tree -- --nocapture` passes.
- `git diff --check` passes.
- GitNexus `detect_changes` is recorded before commit.

## Non-Goals

- Do not introduce Pi's segment files, sidecar offset index, or manifest schema.
- Do not migrate `agent_messages` away from SQLite.
- Do not move all session-list commands out of `tauri_commands.rs` in this
  slice.
- Do not add persistent branch-head or compaction indexes.

## Rollback

Revert the SessionStore commits. The rollback restores lazy-only tree
materialization and does not change the SQLite schema or existing session data.
