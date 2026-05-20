---
name: uclaw-memory-graph-freeze
description: Use whenever you're tempted to write to memory_graph or are working on memory persistence, knowledge graph, entity storage, or "save this for later" features. Trigger phrases include "memory_graph", "knowledge graph", "Steward", "entity graph", "memory write", "memory persist", "store fact", "remember user", "long-term memory", "gbrain", "Dream Cycle", "EntityPage". Loads the freeze rationale (ADR 2026-05-20-gbrain-primary-freeze), the gbrain redirect, and the exempt-path list.
---

# uClaw — Memory Graph is FROZEN

**Per ADR `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md`**:
`memory_graph::write|insert|update|delete*` calls are frozen.
**gbrain is the primary long-term knowledge layer.** Path C-2 (Bun + gbrain
as MCP subprocess) is the runtime; Memory OS Cognitive Layer Phase 8.1
(V43 migration) shipped empty + paused.

## Why frozen

Two overlapping systems were going to fragment the truth: `memory_graph`
(local SQLite Steward-style) and `gbrain` (entity graph + dream cycle).
Two systems = two sources of truth = drift. ADR §11.2 picked gbrain
because: better entity resolution, has the Entity Graph + Dream Cycle
pipeline, designed for triangulation + drift detection (L3 §4.12).
`memory_graph` retains its rows for backward compat (reads still work),
but **writes are off**.

## What's blocked

`.claude/hooks/check-memory-graph.sh` (in-session) +
`scripts/git-hooks/checks/check-memory-graph-freeze.sh` (commit-time) both
block any new code adding:

```
memory_graph::write*
memory_graph::insert*
memory_graph::update*
memory_graph::delete*
```

inside `src-tauri/src/*.rs` — **except** files under
`src-tauri/src/memory_graph/` itself (the implementation has to write to
its own internal tables for backward-compat reads to function).

## What's allowed

- `memory_graph::read*`, `memory_graph::query*`, `memory_graph::get*` — all reads
- Migrations that touch the `memory_*` tables (you'll be coordinating with
  the DRI anyway because it's a schema change)
- ADR / docs / comments that reference `memory_graph::write*` as a string
  literal (the hook is restricted to `.rs` files, so doc files are fine)

## What to do instead

For new "store this knowledge" code paths:

1. **gbrain MCP** (Sprint 2.1 will spawn `bun gbrain --stdio` as a
   default MCP server). Until then, gbrain writes go through the MCP tool
   surface that's being scaffolded. Check `src-tauri/src/mcp.rs` and the
   `gbrain-source/` Tauri resource setup.
2. **For per-turn cost / metrics that aren't "knowledge"** — those go to
   `cost_records` (V13) or `agent_messages` metrics columns (V15), NOT
   `memory_graph`. The hook only blocks `memory_graph::write/insert/etc`,
   not normal table writes.
3. **For user-stated facts ("user lives in Tokyo")** — gbrain Entity Graph
   is the destination. Path: `gbrain → openhuman facets → user_profile_facets`
   (V39, stability-graded).
4. **For ephemeral "remember for this session"** — agent_messages or the
   in-memory session state. Not persistence.

## If you genuinely need a write

The hook exempts `src-tauri/src/memory_graph/*`. If you're modifying the
internal implementation for a real reason (data healing migration, repair
job, deprecation sweep), you're fine inside that path.

**Outside that path, get DRI approval first.** Don't `--no-verify` your
way past the git hook. The freeze is a strategic call; the hooks just
enforce it.

## See also

- `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` — the freeze ADR
- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` §11.2 — the broader memory architecture
- `scripts/setup-bun-runtime.sh` + `scripts/setup-gbrain-source.sh` — gbrain bootstrap
- CLAUDE.md (or CONTEXT.md) *Active migration registry* — V43 (paused) and V44+ (RETAINED, in progress)
