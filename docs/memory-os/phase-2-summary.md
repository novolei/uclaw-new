# Memory OS Foundation — Phase 2 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 — EntityPage CRUD + V34 migration.
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 2.

## What this PR adds

Phase 2 turns the manual `memory_graph_create_edge` IPC call into a
**zero-LLM write-time side-effect** of `create_version` /
`create_entity_page`. When an Agent or user writes
`[[entity:zhang-san]]` (or `[[node:uuid]]`, or `[Text](entity/slug)`)
inside a markdown body, the system:

1. Extracts the reference via regex (skipping fenced code blocks).
2. Resolves the slug or UUID to a `memory_nodes.id`.
3. Infers a typed edge (`works_at` / `founded` / `invested_in` /
   `advises` / `attended` / `source` / `mentions`) from the source
   kind, destination kind, and surrounding cue words.
4. Inserts both `memory_edges` and `memory_edge_audit` rows
   (`source=auto_link`, `confidence=0.6`, `inferred_by=heuristic`).
5. Reconciles stale links — if a new version drops a previously-present
   reference, the corresponding auto_link edge is deleted, **but
   explicit edges (Task 2.4) are preserved**.

| # | Commit | What |
|---|---|---|
| 1 | `feat(memory): add 7 typed MemoryRelationKind variants` | Extends MemoryRelationKind from 4 to 11. No exhaustive matches exist in the codebase so no adjacent edits needed (verified via grep). |
| 2 | `feat(memory): auto_link reference extractor + heuristic typer (zero-LLM)` | New module `memory_graph/auto_link.rs`. `extract_refs(markdown)` handles 3 ref shapes, strips code fences. `infer_link_type(src_kind, dst_kind, context)` uses gbrain's substring-cue heuristic with English + Chinese variants. |
| 3 | `feat(memory): wire auto_link post-hook into create_version + create_entity_page` | The hook itself, plus `MemoryGraphStore::auto_link_enabled` flag (AtomicBool, defaults true). Both call sites swallow failures with `tracing::warn` so a buggy hook never breaks the write path. |
| 4 | `feat(memory): explicit create_edge writes memory_edge_audit (source=explicit)` | Every explicit edge now has an audit row (confidence=1.0). This is what makes stale reconciliation safe — only `audit.source='auto_link'` edges are eligible for deletion. |
| 5 | `feat(memory-os): Phase 2 feature flag + frontend typed-edge constants + summary` | `memubot_config.memory_os.auto_link_enabled` plumbed through `AppState::new` to the store's flag. TS-side `MEMORY_RELATION_KINDS` constant + `PHASE_2_TYPED_RELATION_KINDS` subset for future UI use. This document. |

## What this PR does NOT add

- A new migration. Phase 2 reuses V34 tables created in Phase 1 — the
  `memory_edge_audit` table grows; no schema change required.
- MemoryGraphView typed-edge stroke patterns. The TS constant
  `MEMORY_RELATION_KINDS` is in place so callers can match against the
  canonical names, but the visual mapping (colours, dash patterns,
  thickness) lands in Phase 3 when WikiView + MemoryGraphView get
  rebuilt together. Phase 2 leaves the rendering layer alone to keep
  this PR Rust-side and bisectable.
- Cognitive-Layer confidence weighting (Phase 9).
- NER / alias resolver for free-text mentions without explicit
  `[[entity:...]]` syntax — that's Engines-Layer Phase 15.

## How to verify locally

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib memory_graph::models::tests 2>&1 | tail
cargo test --lib memory_graph::auto_link 2>&1 | tail
cargo test --lib memory_graph::store::tests::auto_link 2>&1 | tail
cargo test --lib memory_graph::store::tests::create_edge_writes_explicit_audit 2>&1 | tail
cargo test --lib memory_graph::store::tests::explicit_edge_via_create_edge 2>&1 | tail
cargo test --lib memubot_config::tests::memory_os 2>&1 | tail

cd ../ui && npx tsc --noEmit 2>&1 | head

# IPC smoke after cargo tauri dev — create two EntityPages, the second
# referencing the first, and verify the edge appears:
#
#   const a = await __TAURI__.core.invoke('memory_entity_page_create', {
#     input: { spaceId: 'default', slug: 'acme', title: 'Acme', compiledTruth: '', metadata: {} }
#   });
#   const b = await __TAURI__.core.invoke('memory_entity_page_create', {
#     input: { spaceId: 'default', slug: 'alice', title: 'Alice',
#              compiledTruth: 'Works at [[entity:acme]] on search.', metadata: {} }
#   });
#   // Then SELECT * FROM memory_edges WHERE parent_node_id = b.node.id
#   //   should show one works_at edge to acme.
```

## How to disable / roll back

```jsonc
// ~/.uclaw/memubot_config.json
{
  "memory_os": {
    "auto_link_enabled": false
  }
}
```

Then restart. `create_version` / `create_entity_page` will continue to
write nodes + versions identically; only the auto-link side-effect
disappears. Existing auto_link edges on disk are untouched (they'll be
reactivated for stale reconciliation if you flip the flag back on).

To physically clean out all auto-link edges produced so far:

```sql
DELETE FROM memory_edges
WHERE id IN (
  SELECT edge_id FROM memory_edge_audit WHERE source = 'auto_link'
);
-- memory_edge_audit cascades via FK.
```

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/app.rs` (Stage 1 `MemubotConfig` load) calls
  `memory_graph_store.set_auto_link_enabled(...)` after both have been
  constructed.
- `src-tauri/src/memory_graph/store.rs::fresh_test_store()` now applies
  V34 migration alongside V4 so the audit table is in scope for tests.
- `ui/src/lib/types.ts` — additive TS-side typed-edge constants. No
  existing types touched.
- No frontend composer changes; no `invoke_handler!` changes (Phase 2
  introduces no new IPC commands — the flag works at the store layer
  beneath all existing commands).

## Performance notes

- `extract_refs` is O(n) over content with three regex passes after
  one fence-strip pass; regexes are `Lazy<Regex>` so compilation is
  one-time.
- The hook runs while holding the conn lock acquired by `create_version`
  — 1 SELECT (space+kind) + 0-N inserts + 0-N deletes, dominated by
  the number of refs in content (typically 0-5).
- `auto_link_preserves_explicit_edges_during_reconciliation` /
  `auto_link_is_idempotent_for_same_ref_across_versions` confirm no
  pathological insert loops on common write patterns.
