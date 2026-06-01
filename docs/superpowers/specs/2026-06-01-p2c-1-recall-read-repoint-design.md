# P2c-1 — Passive-Recall Read Repoint (retire redundant gbrain leg) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P2**, sub-slice **P2c** (read repoint), first slice **P2c-1** (passive-recall read). Follows **P2a-1 + P2a-2** (write dual-write, done) and **P2b** (history migrated). The remaining P2c slices — **P2c-2** (LLM read tools `mcp__gbrain__{query,search,list_pages,get_page}` + `gbrain_prompt`) and **P2c-3** (UI/IPC read commands `gbrain_search`/`gbrain_get_page`) — are separate later slices.

## Problem

The convergence ADR retires gbrain in favour of the adapter (bucket_seal). Reads must move off gbrain. The **passive recall** surface — the knowledge block injected into the system prompt on each chat/agent turn — currently runs **two legs** at each of its two sites (`tauri_commands.rs:1835` and `:11152`):

1. **bucket_seal leg (primary):** `state.bucket_seal_adapter.recall_hybrid(query, None, 6)` — semantic + FTS5 hybrid; `namespace = None` scans **all** namespaces, including `"pages"`.
2. **gbrain leg (secondary, best-effort):** `state.memory_adapters.get("gbrain").recall(query, 6, …)`, where `GbrainAdapter::recall` is just `browse::search` (plain gbrain page FTS).

Because P2b migrated gbrain's history into the adapter `"pages"` namespace and P2a-1/P2a-2 dual-write keeps it current, the bucket_seal leg already surfaces those pages — at equal-or-better quality (semantic+FTS hybrid vs gbrain's plain FTS). **The gbrain leg is redundant.**

## Decision (P2c-1 scope)

Retire the gbrain passive-recall leg behind a new read flag, and — before relying on the bucket_seal leg alone — run a one-time **re-sync** to guarantee bucket_seal completeness.

1. **Gate** `MemoryOsConfig.gbrain_read_repoint_enabled: bool` (default `true`). When `true` (the new default), the gbrain leg is skipped at both recall sites; the bucket_seal primary leg runs unconditionally (unchanged). When `false`, legacy behaviour (both legs) — the rollback path. Read cutover is independent of the write flag `gbrain_dual_write_pages_enabled`; this same flag will later gate P2c-2.
2. **Re-sync** via a marker bump: `MIGRATION_MARKER_SLUG` `v1 → v2` in `gbrain_page_migration.rs`, so the existing boot-spawned `migrate_gbrain_pages` runs exactly one more full idempotent pass — backfilling any page written to gbrain after P2b's marker was set but before dual-write was live (or missed by a best-effort dual-write failure) — then sets v2 and skips thereafter.

Out of scope: P2c-2 (LLM read tools + prompt); P2c-3 (UI/IPC read commands); P2d (retiring the gbrain MCP server / Bun / PGLite / source). The gbrain leg **code** is retained (gated off), not deleted — deletion happens in P2d.

## Design

### §1 Gate + the two recall sites

In `MemoryOsConfig` (`src-tauri/src/memubot_config.rs`):

```rust
    /// P2c-1 — when on, gbrain knowledge READS are served from the adapter
    /// (bucket_seal), not gbrain. Currently gates the redundant passive-recall
    /// gbrain leg (the bucket_seal hybrid leg already surfaces the migrated +
    /// dual-written pages). Default ON = repointed; rollback = false restores the
    /// gbrain leg. Independent of `gbrain_dual_write_pages_enabled` (write side).
    #[serde(default = "default_gbrain_read_repoint_enabled")]
    pub gbrain_read_repoint_enabled: bool,
```
with `fn default_gbrain_read_repoint_enabled() -> bool { true }` (near the other `default_*` fns) and `gbrain_read_repoint_enabled: true` added to the manual `impl Default for MemoryOsConfig`.

At **both** recall sites — `tauri_commands.rs:~1835` and `~11152` — the gbrain leg is the block:

```rust
    if let Some(adapter) = state.memory_adapters.get("gbrain") {
        let opts = crate::memory_adapter::RecallOpts { namespace: None, category: None, session_id: None, min_score: None };
        match adapter.recall(query, 6, opts).await { /* … render GBRAIN_RECALL_MARKER block … */ }
    }
```

Wrap it so it runs only when the flag is off:

```rust
    let gbrain_read_repoint = /* read once, no guard held across await */
        state.memubot_config.read().await.memory_os.gbrain_read_repoint_enabled;
    if !gbrain_read_repoint {
        if let Some(adapter) = state.memory_adapters.get("gbrain") {
            // …unchanged gbrain leg…
        }
    }
```

The bucket_seal primary leg (`recall_hybrid(query, None, 6)` + `render_recall_block(BUCKET_SEAL_RECALL_MARKER, …)`) is **untouched** and always runs. The exact config-read expression mirrors the existing `unified_load_context_enabled` read in `tauri_commands.rs`; read into a local `bool` before any `.await` so no `RwLockReadGuard` is held across the recall awaits. (Site `:11152` is the agent-spawn path building `gbrain_recall_block_for_spawn`; site `:1835` is the agent-teams delegate path — apply the same gate to each, matching its local structure.)

### §2 Re-sync (marker bump)

In `src-tauri/src/memory_adapter/gbrain_page_migration.rs`:

```rust
// P2c-1 re-sync: bumped v1 → v2 so the boot migration runs one more full
// idempotent pass, backfilling any gbrain page not yet in bucket_seal before the
// passive-recall gbrain leg is retired. Sets v2 on success → skips thereafter.
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v2__";
```

No other change to the module — `already_migrated`, `migrate_gbrain_pages`, the boot spawn in `app.rs`, and the tests all reference the `const` (not the literal), so they are unaffected. On the next launch: `already_migrated` sees no v2 marker → `migrate_gbrain_pages` re-reads all gbrain pages and `put_page`s them (idempotent by slug) → writes the v2 marker → subsequent boots skip. The stale v1 marker page remains in bucket_seal, harmless (`page_type == "_migration_marker"`, filtered by page consumers).

### Data flow

```
boot: gbrain_page_migration (v2 marker absent) → one full idempotent re-copy → set v2   [bucket_seal complete]
chat/agent turn recall:
  bucket_seal_adapter.recall_hybrid(query, None, 6) → BUCKET_SEAL block injected   [primary, unchanged]
  if !gbrain_read_repoint_enabled:  gbrain leg → GBRAIN block                       [default: SKIPPED]
  ⇒ default: knowledge injected from bucket_seal only (covers migrated + dual-written pages)
```

## Error handling

Unchanged from today's best-effort recall: the bucket_seal leg already logs+continues on error; the (now-gated) gbrain leg's error handling is untouched. The re-sync inherits P2b's infallible, boot-safe, fire-and-forget posture (gbrain absent / per-page error → logged + skipped; never blocks boot; the v2 marker is set only on a fully-successful pass, so a partial re-sync retries next boot).

## Testing

1. **Config default** — `default_gbrain_read_repoint_enabled()` is `true`; `MemoryOsConfig::default().gbrain_read_repoint_enabled` is `true`; deserialize-without-field yields `true` (add to the existing config test module alongside the `unified_load_context_enabled` / `gbrain_dual_write_pages_enabled` default tests).
2. **Re-sync transparency** — the existing `gbrain_page_migration` tests (idempotency, marker present/absent) still pass with the bumped const (they reference the const). No new test needed for the bump itself.
3. The two recall-site gates are in `tauri_commands.rs` (IPC handlers, not unit-tested); verified by `cargo build` clean + the gate's plain structure (`if !flag { …legacy leg… }`).
4. `cargo build` + `cargo test --lib memory_adapter` + clippy clean.

## Scope / files

| File | Change |
|---|---|
| `src-tauri/src/memubot_config.rs` | `gbrain_read_repoint_enabled` field + `default_*` fn + manual `impl Default` entry + default tests |
| `src-tauri/src/tauri_commands.rs` | gate the gbrain recall leg at `:~1835` and `:~11152` behind `if !gbrain_read_repoint_enabled` |
| `src-tauri/src/memory_adapter/gbrain_page_migration.rs` | `MIGRATION_MARKER_SLUG` v1 → v2 (one-line re-sync) |

**Out of scope (later):** **P2c-2** LLM read tools (`mcp__gbrain__{query,search,list_pages,get_page}`) repoint + `gbrain_prompt` block; **P2c-3** UI/IPC read commands (`gbrain_search`/`gbrain_get_page`); **P2d** retire gbrain MCP + Bun/PGLite + source + gbrain_prompt + delete the gated gbrain-leg code.

## Risk

Low. A gated removal of a **redundant** recall leg (the bucket_seal hybrid leg already covers the same pages, at equal-or-better quality) plus an **idempotent, infallible re-sync** (P2b's proven path) that makes bucket_seal provably complete before the leg retires — so the retirement is evidence-backed, not a leap of faith. Default on = retired; rollback is flipping `gbrain_read_repoint_enabled` to `false` (the gbrain-leg code is retained, gated). The re-sync's one extra boot pass is a one-time cost. No migration, no schema change. One branch, bisectable (config flag → re-sync bump → site gates, or grouped).
