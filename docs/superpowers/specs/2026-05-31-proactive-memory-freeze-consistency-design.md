# Proactive Memory Freeze-Consistency Design (Sub-project C)

**Date:** 2026-05-31 · **Revised:** 2026-05-31 (scope corrected after recon — see below)
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory modernization (gbrain↔openhuman) — the final sub-project (A merged as #617; B folded into A). Closes gap-audit 1.5 STILL-OPEN.
**Strategic baseline:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`; `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` (the freeze ADR).

## Problem

`memory_graph` is ADR-frozen (gbrain primary, memory_graph read-only legacy). The pre-commit hook (`check-memory-graph-freeze.sh`) blocks `memory_graph::write|insert|update|delete` **path-calls**, but **method calls** `store.create_node(...)`/`create_edge`/`create_version`/`create_keyword` bypass it.

**Recon (corrected scope):** the `~86`/`create_node` count was mostly `#[cfg(test)]` setup (`skill_search.rs` 6, `skills_manifest.rs` 2, `service.rs:3277 insert_learned_skill` — all after `#[cfg(test)]`). The **genuine production writers** to the frozen store are three:

1. **task_memory.rs:174** — `Episode` nodes (+ keyword rows); read via `list_nodes_by_kind(space, Episode)`. **Node-only → clean adapter fit.**
2. **tool_memory.rs:213/384** — `Procedure` nodes **+ `create_edge(RelatesTo)`** (co-used-tools **graph**); read via `get_co_used_tools` (edge traversal). **Graph-shaped — no adapter equivalent.**
3. **skill_parser.rs:428/444/467** — learned-skill `Procedure` nodes **+ `create_version` + `create_keyword`**; read via `list_top_learned_skills` (cited_count top-N) + `find_learned_skill_by_normalized_title` (dedup) + decay. **Versioned + keyword-indexed + ranked — richer than `store`/`recall`.**

## Decision (scope)

Migrate ONLY the **lossless** writer (task_memory Episode → adapter). The two **rich** writers (tool_memory's edge graph, skill_parser's versioned/keyworded/ranked store) are **explicitly exempted + documented** — flattening them into `MemoryEntry.content` would lose real functionality (co-used-tools queries, skill versioning, keyword search, exact cited-count top-N), which the philosophy ADR (line 120) explicitly **defers** to a dedicated gbrain↔openhuman effort. Tighten the freeze hook so NEW production bypasses are blocked. This makes the freeze **honest** (no undocumented bypass) without a lossy migration.

## Design

### 1. Migrate task_memory (Episode) → `MemoryAdapter` (`bucket_seal`) — lossless

- Namespace `proactive:episode:{space_id}`; each Episode → a `MemoryEntry` with the node payload (title + metadata JSON + extracted keywords) serialized into `MemoryEntry.content` (so `adapter.recall` content-search matches keywords).
- **Write** (`task_memory.rs` `record_task`): replace `store.create_node(Episode)` + keyword rows with `adapter.store` into the namespace.
- **Read** (`list_recent_tasks`): replace `store.list_nodes_by_kind(space, Episode, limit)` with `adapter.recall`/list over the namespace, reconstructing `SimilarTask` from the deserialized content.
- Backend fixed to `bucket_seal` (canonical default) via the namespace-prefix routing / explicit backend.

### 2. One-time data migration of existing Episode nodes (idempotent, startup, non-blocking)

New `proactive/memory_migration.rs`:
- Startup fire-and-forget (`tauri::async_runtime::spawn`, the boot idiom): read existing `Episode` nodes from `memory_graph` → serialize → `adapter.store` into `proactive:episode:{space}`.
- **Idempotency:** config flag `memory_os.proactive_episode_migrated_v1: bool` (default false). Skip if true; set true after a successful pass.
- **Old nodes retained** (memory_graph stays frozen read-only legacy); post-migration the adapter is the source of truth for Episodes. Physical deletion deferred.
- **Infallible:** malformed node → `warn` + skip; never panics or blocks boot.

### 3. tool_memory + skill_parser — explicit exemption + documentation

Both stay on `memory_graph` (rich semantics with no clean adapter mapping). Add:
- `tool_memory.rs` module-doc: `// EXEMPT from memory_graph freeze: co-used-tools graph (edges) has no MemoryAdapter equivalent; migration deferred to the gbrain↔openhuman effort.`
- `skill_parser.rs` module-doc: `// EXEMPT from memory_graph freeze: versioned/keyword-indexed/ranked learned-skill store has no MemoryAdapter equivalent; migration deferred to the gbrain↔openhuman effort.`
- A note in the freeze ADR (`2026-05-20-gbrain-primary-freeze-l2-cognitive.md`) + `memory_adapter/mod.rs` roster: these two are the documented freeze exemptions, scoped to the deferred effort.

### 4. Tighten the freeze hook

Extend `scripts/git-hooks/checks/check-memory-graph-freeze.sh`: in addition to `memory_graph::write|insert|update|delete` path-calls, flag added lines matching `\.(create_node|create_entity_page|create_edge|create_version|create_keyword)\s*\(` (method-call bypasses).
- **File-level allowlist** (legitimate/exempt/test-heavy writers, skipped): `tool_memory.rs`, `skill_parser.rs` (the two exempt rich writers), `proactive/memory_migration.rs` (the migration), `memory_graph/legacy_migration/`, `memory_graph/mod.rs` (own impls), and the test-only writers `agent/tools/builtin/skill_search.rs` + `skills_manifest.rs`.
- Effect: a NEW production `create_node`/`create_edge`/etc. in a non-allowlisted file is blocked → author must route through the adapter OR consciously edit the allowlist (a reviewed decision). Closes the bypass class.

## Data flow (post-migration)

```
startup → (if !proactive_episode_migrated_v1) spawn: memory_graph Episode nodes
          → serialize → adapter.store(proactive:episode:{space}) → set flag true
task_memory record_task → adapter.store(proactive:episode)   [memory_graph NOT touched]
task_memory list_recent  → adapter.recall/list                [not list_nodes_by_kind]
tool_memory (co-used graph)   → memory_graph create_node/create_edge   [EXEMPT, unchanged]
skill_parser (learned skills) → memory_graph create_node/version/keyword [EXEMPT, unchanged]
```

## Error handling

Migration + adapter calls best-effort: failed recall/store logged + skipped (mirrors proactive tolerance), never fails a turn or blocks boot. A get-miss is treated as empty.

## Testing

1. task_memory: `record_task`→`list_recent_tasks` round-trip via adapter (Episode JSON ser/de + keyword folded into content + `SimilarTask` reconstruction); read no longer calls `list_nodes_by_kind`.
2. migration: idempotent (run twice → migrates once via the flag); malformed node skipped without panic; post-migration adapter holds the old Episodes.
3. tool_memory + skill_parser: unchanged — still write/read memory_graph (co-used graph + versioned skills work).
4. hook: a new production `.create_node(` in a non-allowlisted file → blocked; allowlisted files (tool_memory, skill_parser, migration) pass (shell test or manual verification).
5. `cargo test --lib agent` / `proactive` / `memubot_config` net green (only the 2 known pre-existing failures); clippy clean.

## Scope / files

| File | Change |
|---|---|
| `proactive/task_memory.rs` | Episode write+read → adapter (namespace `proactive:episode`) |
| `proactive/memory_migration.rs` | **new** — one-time idempotent Episode migration |
| `app.rs` | startup fire-and-forget migration call |
| `memubot_config.rs` | `proactive_episode_migrated_v1` flag (default false) |
| `proactive/tool_memory.rs`, `proactive/skill_parser.rs` | exemption doc notes only (stay on memory_graph) |
| `scripts/git-hooks/checks/check-memory-graph-freeze.sh` | flag `create_node`/`create_entity_page`/`create_edge`/`create_version`/`create_keyword` + file allowlist |
| `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md`, `memory_adapter/mod.rs` | tool_memory + skill_parser exemption notes |

**Out of scope (deferred to the gbrain↔openhuman effort):** migrating tool_memory's edge graph + skill_parser's versioned/keyworded/ranked store; physically deleting old `memory_graph` nodes.

## Risk

Medium — one data migration (Episode) + changing task_memory's read path. Mitigations: migration idempotent + infallible + old nodes retained (rollback by reverting the read path); task/proactive memory is a background enhancement, not the core chat path; the two rich writers are untouched (no functionality loss); the hook tightening prevents regressions. One branch, bisectable commits (config → migration module → task_memory → exemption docs + hook → verify).
