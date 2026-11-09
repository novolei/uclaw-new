#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Collapse bucket_seal "seal ladders" left behind by the pre-d55f6c0b flush bug.

Before commit d55f6c0b, a forced (time-based) flush could seal an upper-level
buffer holding ONE summary into a one-child parent, whose buffer was
immediately stale again — so every flush sweep climbed each tree one level,
producing chains of summaries that are pure copies of the node beneath them.
On a real store: 98–100 % single-child summaries from L2 up, levels to 13.

A *ladder node* is a non-deleted summary at level >= 2 with exactly one child.
(Upper levels only seal on fan-out, so these come only from the forced path.
L1 single-child summaries are the first summary of a raw chunk and are kept.)

For every ladder chain this script, in ONE transaction:
  1. tombstones the ladder nodes (deleted = 1 — rows are kept, reversible);
  2. re-points whatever referenced the chain top to the chain bottom B (the
     real summary): a real parent's child_ids_json + B.parent_id, or — when
     the top was still pending in a buffer — removes it from that buffer and
     appends B to the tree's buffer at level B.level + 1;
  3. recomputes each touched tree's max_level / root_id from live summaries.

Dry-run by default. Stop the app first (single writer) and back up the DB:
    sqlite3 ~/.uclaw/bucket_seal/chunks.db ".backup 'chunks.db.bak'"
    python3 collapse_bucket_seal_ladder.py --db ~/.uclaw/bucket_seal/chunks.db           # dry run
    python3 collapse_bucket_seal_ladder.py --db ~/.uclaw/bucket_seal/chunks.db --apply   # write
Idempotent: a second run finds nothing to collapse.
"""
import argparse
import collections
import json
import os
import sqlite3
import time


def load(conn):
    rows = conn.execute(
        "SELECT id, tree_id, level, parent_id, child_ids_json, token_count, sealed_at_ms "
        "FROM mem_tree_summaries WHERE deleted = 0"
    ).fetchall()
    nodes = {}
    for sid, tree_id, level, parent_id, child_json, tokens, sealed in rows:
        nodes[sid] = {
            "id": sid, "tree": tree_id, "level": level, "parent": parent_id,
            "children": json.loads(child_json or "[]"), "tokens": tokens or 0, "sealed": sealed,
        }
    return nodes


def is_ladder(n):
    return n["level"] >= 2 and len(n["children"]) == 1


def plan(nodes):
    ladder = {sid for sid, n in nodes.items() if is_ladder(n)}

    def bottom(sid):
        cur = sid
        seen = set()
        while cur in ladder:
            if cur in seen:  # defensive: never loop on corrupt data
                raise RuntimeError(f"cycle at {cur}")
            seen.add(cur)
            cur = nodes[cur]["children"][0]
        return cur

    tops = [sid for sid in ladder if nodes[sid]["parent"] not in ladder]
    actions = []
    for top in tops:
        b = bottom(top)
        if b not in nodes:
            raise RuntimeError(f"chain bottom {b} of {top} is missing or deleted")
        actions.append((top, b, nodes[top]["parent"]))
    return ladder, actions


def apply(conn, nodes, ladder, actions):
    touched_trees = set()
    buffers_fixed = parents_fixed = 0
    for top, b, parent in actions:
        tree = nodes[top]["tree"]
        touched_trees.add(tree)
        if parent and parent in nodes:
            p = nodes[parent]
            new_children = [b if c == top else c for c in p["children"]]
            conn.execute("UPDATE mem_tree_summaries SET child_ids_json = ? WHERE id = ?",
                         (json.dumps(new_children), parent))
            p["children"] = new_children
            conn.execute("UPDATE mem_tree_summaries SET parent_id = ? WHERE id = ?", (parent, b))
            parents_fixed += 1
        else:
            # Top was pending in some buffer of this tree: move B to level B.level+1.
            for level, item_json, token_sum, oldest in conn.execute(
                "SELECT level, item_ids_json, token_sum, oldest_at_ms FROM mem_tree_buffers "
                "WHERE tree_id = ? AND item_ids_json LIKE ?", (tree, f'%"{top}"%')
            ).fetchall():
                items = [i for i in json.loads(item_json) if i != top]
                conn.execute(
                    "UPDATE mem_tree_buffers SET item_ids_json = ?, token_sum = ?, "
                    "oldest_at_ms = CASE WHEN ? = '[]' THEN NULL ELSE oldest_at_ms END, updated_at_ms = ? "
                    "WHERE tree_id = ? AND level = ?",
                    (json.dumps(items), max(0, token_sum - nodes[top]["tokens"]),
                     json.dumps(items), int(time.time() * 1000), tree, level),
                )
            target = nodes[b]["level"] + 1
            row = conn.execute(
                "SELECT item_ids_json, token_sum, oldest_at_ms FROM mem_tree_buffers "
                "WHERE tree_id = ? AND level = ?", (tree, target)).fetchone()
            items = json.loads(row[0]) if row else []
            if b not in items:
                items.append(b)
            b_start = conn.execute(
                "SELECT time_range_start_ms FROM mem_tree_summaries WHERE id = ?", (b,)).fetchone()[0]
            oldest = min(x for x in [row[2] if row else None, b_start] if x is not None)
            conn.execute(
                "INSERT INTO mem_tree_buffers (tree_id, level, item_ids_json, token_sum, oldest_at_ms, updated_at_ms) "
                "VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(tree_id, level) DO UPDATE SET "
                "item_ids_json = excluded.item_ids_json, token_sum = excluded.token_sum, "
                "oldest_at_ms = excluded.oldest_at_ms, updated_at_ms = excluded.updated_at_ms",
                (tree, target, json.dumps(items), (row[1] if row else 0) + nodes[b]["tokens"],
                 oldest, int(time.time() * 1000)),
            )
            conn.execute("UPDATE mem_tree_summaries SET parent_id = NULL WHERE id = ?", (b,))
            buffers_fixed += 1

    conn.executemany("UPDATE mem_tree_summaries SET deleted = 1 WHERE id = ?", [(s,) for s in ladder])

    for tree in touched_trees:
        top_row = conn.execute(
            "SELECT id, level FROM mem_tree_summaries WHERE tree_id = ? AND deleted = 0 "
            "ORDER BY level DESC, sealed_at_ms DESC LIMIT 1", (tree,)).fetchone()
        if top_row:
            conn.execute("UPDATE mem_tree_trees SET max_level = ?, root_id = ? WHERE id = ?",
                         (top_row[1], top_row[0], tree))
        else:
            conn.execute("UPDATE mem_tree_trees SET max_level = 0, root_id = NULL WHERE id = ?", (tree,))
    return touched_trees, buffers_fixed, parents_fixed


def verify(conn):
    """Post-conditions: no live ladder nodes; every live child/parent/root/buffer
    reference points at a live summary (or chunk, for L0 buffers / L1 children)."""
    problems = []
    live = {r[0] for r in conn.execute("SELECT id FROM mem_tree_summaries WHERE deleted = 0")}
    n_ladder = conn.execute(
        "SELECT COUNT(*) FROM mem_tree_summaries WHERE deleted = 0 AND level >= 2 "
        "AND json_array_length(child_ids_json) = 1").fetchone()[0]
    if n_ladder:
        problems.append(f"{n_ladder} live ladder nodes remain")
    for sid, level, child_json, parent in conn.execute(
            "SELECT id, level, child_ids_json, parent_id FROM mem_tree_summaries WHERE deleted = 0"):
        if level >= 2:
            dead = [c for c in json.loads(child_json) if c not in live]
            if dead:
                problems.append(f"{sid} has dead children {dead[:2]}")
        if parent and parent not in live:
            problems.append(f"{sid} has dead parent {parent}")
    for tree, root in conn.execute("SELECT id, root_id FROM mem_tree_trees WHERE root_id IS NOT NULL"):
        if root not in live:
            problems.append(f"tree {tree} root {root} is dead")
    for tree, level, item_json in conn.execute(
            "SELECT tree_id, level, item_ids_json FROM mem_tree_buffers WHERE level >= 1"):
        dead = [i for i in json.loads(item_json) if i not in live]
        if dead:
            problems.append(f"buffer {tree}@L{level} holds dead ids {dead[:2]}")
    return problems


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--db", required=True)
    ap.add_argument("--apply", action="store_true", help="write changes (default: dry run)")
    a = ap.parse_args()
    conn = sqlite3.connect(os.path.expanduser(a.db), isolation_level=None)
    conn.execute("PRAGMA busy_timeout = 5000")

    nodes = load(conn)
    ladder, actions = plan(nodes)
    by_level = collections.Counter(nodes[s]["level"] for s in ladder)
    print(f"live summaries: {len(nodes)}")
    print(f"ladder nodes to tombstone: {len(ladder)}  by level: {dict(sorted(by_level.items()))}")
    print(f"chains: {len(actions)}  (top under a real parent: "
          f"{sum(1 for _, _, p in actions if p and p in nodes)}, top pending in a buffer: "
          f"{sum(1 for _, _, p in actions if not (p and p in nodes))})")
    print(f"live summaries after: {len(nodes) - len(ladder)}")
    if not a.apply:
        print("dry run — nothing written (pass --apply)")
        return

    conn.execute("BEGIN IMMEDIATE")
    try:
        trees, bufs, parents = apply(conn, nodes, ladder, actions)
        problems = verify(conn)
        if problems:
            raise RuntimeError("verification failed: " + "; ".join(problems[:5]))
        conn.execute("COMMIT")
    except Exception:
        conn.execute("ROLLBACK")
        raise
    print(f"applied: {len(ladder)} tombstoned, {len(trees)} trees re-rooted, "
          f"{parents} parents re-pointed, {bufs} buffer entries moved; verification OK")


if __name__ == "__main__":
    main()
