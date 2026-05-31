# Memory `load_context` Unification Design (Sub-project A)

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory modernization (gbrain↔openhuman), the philosophy ADR's deferred "dedicated effort". Decomposed into A (this), B (backend end-state — found mostly done, residual folded here as task 0), C (freeze consistency — deferred).
**Strategic baseline:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`; gap-audit 1.5(c) (`2026-05-27-pi-convergence-gap-audit.md`, refreshed 2026-05-31).

## Problem

The chat turn assembles its memory context from **~5 hand-wired sources** via `set_memory_context`/`append_memory_context` (tauri_commands.rs ~2280–2400):

1. `MemoryRecallEngine::format_recall_for_prompt` — the **primary** recall, still reading the (nominally frozen) `memory_graph` store.
2. gbrain adapter `recall` (tauri_commands.rs:1821).
3. proactive background recall (`prepare_background_context`).
4. session memory context.
5. a dedicated `<user_preferences>` section (+ browser-task context when in a browser task).

There is no single seam: each source has its own format/budget/dedup, none authoritative. The `MemoryAdapter` trait exposes `recall` but no `load_context`. Meanwhile the unified IPC memory family already routes through `bucket_seal` (the live default) + the router — but the chat path bypasses all of it and recalls from `memory_graph`.

## Goal

Add a router-level `load_context(query, budget) -> String` (openhuman shape: coarse-to-fine, hard char budget) that **subsumes the recall-class sources** under one budget/dedup/format, recalling via the **adapter router** (`bucket_seal` default + `gbrain`) instead of the `memory_graph` `MemoryRecallEngine`. The chat entry's 5-source assembly collapses to one `load_context` call plus the two deliberately-separate structured sections. Gated by config, default on, with the legacy `MemoryRecallEngine` path retained as a gated fallback for a validation/rollback window.

This modernizes chat recall onto the adapter stack and removes `memory_graph` from the chat recall path (assisting sub-project C's freeze story). `memory_graph`/`MemoryRecallEngine` is NOT deleted here.

## Design

### 1. `load_context` — router-level, subsumes recall sources

New `memory_adapter::router::load_context(state, query, budget_chars, extra: Vec<MemoryEntry>) -> String`:
- Recall via the router: call `recall(query, k, opts)` on the default backend (`bucket_seal`) and `gbrain` (the adapter recall sources).
- Accept `extra: Vec<MemoryEntry>` — pre-fetched recall-class candidates the **caller** supplies (proactive background, session memory). This keeps the router **decoupled** from the proactive service / session layer: the chat entry fetches those (as it does today) and hands them in as candidates, rather than the router reaching into `proactive_svc`/session.
- Merge all candidates (adapter recall + `extra`) → sort by score → dedup across sources (same fact from `bucket_seal`/`gbrain`/extra) → truncate to `budget_chars` → format coarse-to-fine (summaries first; the openhuman "summary then drill-down" shape, using what the adapters return).
- Returns the formatted budgeted string (empty string when no candidates).

Lives on the **router, not the `MemoryAdapter` trait** — it orchestrates across backends + budgets; the trait stays atomic (`recall`). The router depends only on the adapters map + the passed-in candidates, never on `proactive_svc`/session services.

**What `load_context` subsumes vs what stays separate:**

| Source | Disposition |
|---|---|
| `memory_graph` `MemoryRecallEngine` recall | **Replaced** by adapter recall inside `load_context` |
| gbrain adapter recall | Into `load_context` |
| proactive background recall | Into `load_context` |
| session memory | Into `load_context` |
| `<user_preferences>` dedicated section | **Stays separate** (deliberate structured section, not budgeted recall) |
| browser-task context | **Stays separate** (task-scoped, injected only during a browser task) |

Chat entry becomes: `delegate.set_memory_context(load_context(query, budget))` + the independent `<user_preferences>` and browser-task appends.

### 2. Recall backend switch + `memory_graph` exit + gated fallback

- `load_context` recalls via `route_recall` against the canonical default (`bucket_seal`) + `gbrain`. The `memory_graph` `MemoryRecallEngine` is removed from the chat **primary recall** path.
- **Config gate:** `memory_os.unified_load_context_enabled: bool`, **default `true`**. When `true`, the chat path uses `load_context`. When `false`, it falls back to the existing 5-source `MemoryRecallEngine` assembly (preserved verbatim, gated).
- The legacy `MemoryRecallEngine` assembly path is **retained** (gated), not deleted — a parallel validation + rollback window. Its eventual retirement is a future small PR after the new path is validated.

### 3. Sub-project B residual (folded in as task 0)

B's headline ("default flip") is already complete (`default_memory_backend = "bucket_seal"`, app.rs:1130; unified IPC routes through it). Residual cleanup, done here:
- `router::resolve_backend` poison-fallback `"legacy_kv"` → `"bucket_seal"` (fall back to the canonical default, not legacy).
- Document the **roster end-state** in `memory_adapter/mod.rs`: `bucket_seal` = canonical default; `gbrain` = retained (chat/MCP recall); `memu` = retained (item memory); `legacy_kv` + `legacy_steward` = **deprecated**, reachable only by explicit namespace, migration/removal deferred.
- Add deprecation notes (`#[deprecated]` or doc) to the `legacy_kv` / `legacy_steward` adapter modules.

## Data flow

```
chat turn (unified_load_context_enabled = true)
  → caller pre-fetches proactive background + session memory as Vec<MemoryEntry> (as today)
  → memory_adapter::router::load_context(state, query, budget, extra=[proactive,session])
        → route_recall: bucket_seal.recall + gbrain.recall
        → merge(adapter recall + extra) → sort by score → cross-source dedup → truncate to budget → coarse-to-fine format
  → delegate.set_memory_context(<that string>)
  → + delegate.append (<user_preferences>)            [separate, unchanged]
  → + delegate.append (browser-task ctx, if any)      [separate, unchanged]

unified_load_context_enabled = false  → existing MemoryRecallEngine 5-source assembly (verbatim fallback)
```

## Error handling

`load_context` is best-effort: any backend `recall` error is logged and skipped (that source contributes nothing); the call still returns whatever assembled (mirrors today's "recall failed, proceed without"). Never fails the turn.

## Testing

1. **Pure helpers** (router): merge-sort-dedup-budget — multi-source candidates → sorted, cross-source dedup, truncated to a small budget (+ truncation note); empty candidates → empty string.
2. **Subsumption (enabled):** with `unified_load_context_enabled = true`, the chat memory_context comes from `load_context` (adapter recall), contains NO `MemoryRecallEngine`/`memory_graph` output; `<user_preferences>` still appears as its own section.
3. **Fallback (disabled):** `unified_load_context_enabled = false` → the legacy 5-source `MemoryRecallEngine` path runs, behavior identical to today (regression).
4. **B task 0:** poison-fallback resolves to `bucket_seal`; legacy adapters still reachable by explicit namespace (`legacy_kv:...`) — not broken.
5. `cargo test --lib agent` / `memory_adapter` net green (only the 2 known pre-existing failures); clippy clean; `Cargo.toml` unchanged.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/router.rs` | `load_context` + pure merge/dedup/budget/format helpers + tests; poison-fallback → `bucket_seal` |
| `memory_adapter/mod.rs` + `legacy_kv.rs`/`legacy_steward.rs` | roster end-state doc + deprecation notes (B task 0) |
| `tauri_commands.rs` | chat entry: 5-source assembly → `load_context` one-liner + retained gated fallback; keep `<user_preferences>`/browser-task appends |
| `memubot_config.rs` | `unified_load_context_enabled: bool` (default true) + tests |

**Out of scope (deferred):** deleting `MemoryRecallEngine`/`memory_graph` from chat (future small PR after validation); sub-project **C** (skills+proactive `create_node` freeze consistency); fully migrating/removing `legacy_kv`/`legacy_steward` data.

## Risk

Medium. Switching the chat recall backend is a user-perceptible behavior change — mitigated by: config default-on + the legacy `MemoryRecallEngine` retained as a gated fallback (instant rollback via the flag) + a parallel validation window before retiring the old path. The dedicated sections (`<user_preferences>`, browser-task) are untouched. One branch, bisectable commits.
