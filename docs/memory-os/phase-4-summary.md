# Memory OS Foundation — Phase 4 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 (V34 `memory_health_findings` table), Phase 2 (auto-link edges that the orphan check needs to traverse), Phase 3 (wiki tab — the new Health tab sits next to it in `MemoryModule`).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` § Tier-A A3.
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 4.

## What this PR adds

Phase 4 wakes up `memory_health_findings` (which has been sitting empty
since Phase 1's V34 migration created it). A new ProactiveService
scenario runs seven zero-LLM structural-integrity checks every
~30 minutes; the new `Health` tab in `MemoryModule` lets users see and
dismiss findings, plus force an immediate scan.

The split mirrors gbrain's `dream` cycle and llm-wiki-agent's
`health.py` vs `lint.py`: **Phase 4 is health (zero-LLM, always-on
cheap path); Phase 5 is lint (LLM, budget-guarded paid path).** Phase 4
ships *only* health.

| # | Commit | What |
|---|---|---|
| 1 | `feat(memory-os): memory_health scenario (7 zero-LLM checks)` | New `proactive/scenarios/memory_health.rs`. Seven SQL-only finders + `run_health_checks` orchestrator + `upsert_finding` dedup. 16 unit tests. |
| 2 | `feat(memory-os): wire memory_health into ProactiveService tick` | `MemoryOsConfig.memory_health_enabled` flag. ProactiveService runs the scan every 60 ticks (~30 min) on `tokio::spawn_blocking`. Offset from Phase 3's 10-tick wiki regen so the two scans rarely collide on the SQLite lock. |
| 3 | `feat(ipc): health findings list/dismiss tauri commands` | Three new IPC commands: `memory_health_list_findings` (paginated, severity-sorted), `memory_health_dismiss_finding` (idempotent), `memory_health_run_now` (gated). TS types + invoke wrappers. |
| 4 | `feat(ui): MemoryHealthPanel with dismiss + jump-to-node + Scan now` | New tab in `MemoryModule` (the live surface, not orphan `MemoryPanel`). Severity-grouped list, payload-snippet auto-extracted from `payload_json`, optimistic dismiss. |
| 5 | `docs(memory-os): Phase 4 summary` | This document. |

## The seven checks

| id | severity | what it catches |
|---|---|---|
| `orphan` | warn | non-Boot/Identity/Value/Directive node with zero in + zero out edges |
| `stub` | warn | EntityPage whose active `memory_versions.content` is shorter than 100 chars |
| `dangling_fts` | error | `memory_fts` row whose `node_id` no longer exists in `memory_nodes` |
| `index_drift` | error | `memory_routes` row pointing to a non-existent `node_id` |
| `phantom_slug` | error | `memory_edges` whose `child_node_id` is missing |
| `empty_versions` | warn | node has rows in `memory_versions` but none currently `active` |
| `missing_route` | warn | `EntityPage` node lacking any `is_primary=1` row in `memory_routes` |

All seven run sequentially under a single conn lock; total wall time
on a typical local DB (< 10k nodes) is well under 50ms. Order matters
internally because dedup is `(space_id, subject, check_kind)` —
running them sequentially keeps the dedup window narrow.

## Dedup contract

`upsert_finding` only inserts a new row when no **open** (un-dismissed)
finding exists for the same `(space_id, subject, check_kind)`.
Dismissed rows are NOT counted, so:

- Same scan running twice → no duplicate rows.
- User dismisses a finding → SCAN later still sees the underlying issue
  is present → a fresh row appears (it's "re-detected"). The original
  dismissed row stays for audit; the new row is open.
- User fixes the underlying issue → next scan inserts nothing for that
  subject; the dismissed row stays in the table but the active count
  goes down.

The 16 tests cover all three paths.

## What this PR does NOT add

- **Lint (Phase 5).** That's the LLM-driven counterpart: hub stubs,
  phantom hubs, stale summaries, contradictions. Lint will write into
  the *same* `memory_health_findings` table with `is_lint=1` — the
  Health panel already knows how to render the `lint` badge.
- **Hide-tab gating.** When `memory_health_enabled=false` the Health
  tab is still visible but its `Scan now` button surfaces the
  structured error. List + dismiss keep working so users can triage
  pre-existing findings even with the flag off.
- **Severity weighting in recall ranking.** Phase 9 (Cognitive
  provenance) will let the recall function down-weight nodes whose
  active findings include `error`-severity issues. Phase 4 only
  surfaces; Phase 9 acts.

## How to verify locally

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/p4-memory-os-health

# 1. Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head

# 2. Rust tests
cargo test --lib proactive::scenarios::memory_health 2>&1 | tail -10    # 16 cases
cargo test --lib memubot_config::tests::memory_os 2>&1 | tail            # +2 cases for Phase 4 flag

# Existing Phase 1-3 tests should still pass:
cargo test --lib memory_graph::store::tests::auto_link 2>&1 | tail
cargo test --lib memory_graph::wiki_synth 2>&1 | tail

# 3. TS check
cd ../ui && npx tsc --noEmit 2>&1 | head

# 4. IPC smoke in cargo tauri dev:
#
# // Manually fabricate an orphan node so the scan has something to find
# const a = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'orphan-test', title: 'Orphan', compiledTruth: 'x',
#            metadata: { subkind: 'entity' } }
# });
# // 'x' is < 100 chars → stub finding; no edges → orphan finding
#
# const out = await __TAURI__.core.invoke('memory_health_run_now', { input: {} });
# // -> { orphan: 1, stub: 1, ..., total_inserted: 2, active_total: 2, duration_ms: <small> }
#
# const list = await __TAURI__.core.invoke('memory_health_list_findings', {
#   input: { spaceId: 'default' }
# });
# // -> 2 HealthFindingDto rows, severity=warn, sorted error→warn→info
#
# 5. UI: open Kaleidoscope memory module, switch to the new "Health" pill.
#    Should see two warn findings with payload snippets. Click "Scan now"
#    to verify the dedup contract (count stays at 2 not 4). Click the
#    X button to dismiss; row disappears from the panel.
```

## How to disable / roll back

```jsonc
// ~/.uclaw/memubot_config.json
{
  "memory_os": {
    "memory_health_enabled": false
  }
}
```

Effects when off:
- ProactiveService stops running scans on tick.
- `memory_health_run_now` IPC returns a structured error.
- `memory_health_list_findings` + `memory_health_dismiss_finding` keep
  working so the user can still triage existing findings.
- The Health tab stays visible (the user-facing error message inside
  the Scan-now button is clearer than hiding the tab).

To physically delete all health findings from the DB (e.g. after
fixing a bunch of issues outside of the dismiss flow):

```sql
DELETE FROM memory_health_findings WHERE is_lint = 0;
-- is_lint=0 → Phase 4 health findings only; lint (Phase 5) rows preserved.
```

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/proactive/scenarios/mod.rs` — registered the new
  `memory_health` module.
- `src-tauri/src/memubot_config.rs` — `memory_health_enabled` (default
  true), forward-compat test added.
- `src-tauri/src/proactive/service.rs` — `ProactiveStateRefs` +
  `ProactiveService` grow `memory_health_enabled: bool`; constructor
  signature gains a parameter; `clone_state_refs` propagates; the new
  tick block sits BEFORE the wiki regen block so order is health →
  wiki → other checks.
- `src-tauri/src/main.rs` — `ProactiveService::new` call passes the
  flag from `memubot_config.memory_os.memory_health_enabled`; three
  new commands registered in `invoke_handler!`.
- `src-tauri/src/proactive/service.rs` test helper passes `true` to
  match `MemoryOsConfig::default()`.
- `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx` — Health
  tab added in the LIVE surface (NOT `MemoryPanel.tsx`, which is an
  orphan — Phase 3 fix-up lesson applied here too).

No new V-migration. Phase 4 reuses `memory_health_findings` from V34.

## Performance notes

- `run_health_checks` is 7 SELECT queries plus 0-N INSERTs (dedup
  short-circuits subsequent runs to mostly skip the inserts). On a
  fresh DB with 100 EntityPages the full scan takes ~5ms in tests; on
  a 10k-node DB still well under 50ms.
- The `tokio::spawn_blocking` wrapper around the tick-side call keeps
  the runtime free even if SQLite contention pushes the scan past
  what would normally block other writes.
- list IPC is severity-sorted via a SQL `CASE` expression — no
  additional column needed for ordering.
- Dismiss is a single `UPDATE` keyed by primary id; idempotent.
