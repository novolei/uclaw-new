# Memory OS Sprint 1 Summary — openhuman warm-start

**Status:** in progress (this PR + the two predecessor PRs that landed Sprint 1.1-1.4).
**Builds on:** Phase 1-7 Foundation (merged), strategy doc PR #195.
**Strategy ref:** `docs/memory-os/strategy-2026-05-18-research-synthesis.md` Part 1.4 + Appendix C.

## What this Sprint adds

Sprint 1 ports openhuman's "20-30 min smart" mechanism — the C+D loops
of their three-loop architecture (stability detector + PROFILE.md
injection). We don't have the OAuth bulk-backfill loop A (no Composio
integrations yet); ProactiveService is our loop B equivalent.

**End-to-end flow once Sprint 1 lands on main:**

1. Agent reply finishes → chat-turn extractor scans the user message
   (regex + optional LLM)
2. Producer pushes `LearningCandidate` rows into the shared `Buffer`
3. Every 30 min, `ProactiveService` tick → `LearningScheduler::rebuild_now`
   → drain buffer, score via `stability_detector`, write
   `user_profile_facets`, refresh `FacetCache`
4. Next agent turn: `effective_system_prompt` appends a `## User
   Profile (Learned)` block rendered from active facets in `FacetCache`
5. User feels: "agent learned my preferences without me explicitly
   telling it"

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1.1 | `feat(db): V39 — user_profile_facets` | New schema for stability-graded facet store. 4 unit tests + CLAUDE.md V39 row |
| 1.2 | `feat(learning): candidate buffer + taxonomy types` | `FacetClass` × `CueFamily` × `EvidenceRef` + bounded `Buffer`. 13 unit tests |
| 1.3 | `feat(learning): stability detector + class budget enforcement` | Pure-function scorer + per-class top-K trim. 15 unit tests |
| 1.4 | `feat(learning): FacetStore + LearningScheduler` | DB I/O + `rebuild_now` orchestrator. 11 unit tests |
| 1.4b | `fix(learning): EXPLICIT_BOOST 2.0→3.0` | Math fix: single explicit cue at fresh evidence_count=1 now actually crosses TAU_PROMOTE=1.5 |
| 1.5 | `feat(learning): FacetCache — shared read-side mirror` | `Arc<RwLock<HashMap>>` keyed by class, sorted by stability DESC. 7 unit tests |
| 1.6 | `feat(learning): UserProfileSection prompt renderer` | Stateless renderer with class priority + char cap. 7 unit tests |
| 1.7 | `feat(memory-os): PROFILE.md managed-block parser + renderer` | Markdown round-trip with user-prose preservation. 13 unit tests |
| 1.8 | `feat(agent): inject learned UserProfileSection into system prompt` | ChatDelegate.learned_profile_block + setter + append after skills manifest |
| 1.9 | `feat(learning): chat-turn candidate producer` (hidden big rock) | Regex + LLM extractor over user turns. 19 unit tests |
| 1.10 | `feat(learning): wire scheduler tick + AppState + IPC` | The integration commit; tick block + 3 Tauri commands |
| 1.11 | `docs(memory-os): Sprint 1 summary` | This document |

**Test totals across Sprint 1:** ~89 new unit tests, all in-memory SQLite where DB needed.

## Architecture diagram

```
┌────────────────────────────────────────────────────────────────┐
│                Producer side (chat-turn extractor)              │
│                                                                 │
│  user turn text  ──► extractor::extract_from_chat_turn          │
│                          │                                      │
│                          ├── regex layer (always, zero cost)    │
│                          │     "I prefer X" → Tooling           │
│                          │     "my name is X" → Identity        │
│                          │     bilingual EN+CN patterns         │
│                          │                                      │
│                          └── LLM layer (opt-in, daily cap)      │
│                                via memory_os_llm                │
│                                                                 │
│            pushes LearningCandidate into ▼                      │
└──────────────────────────────────────┬─────────────────────────┘
                                       │
                          Arc<learning::candidate::Buffer>
                                       │
┌──────────────────────────────────────▼─────────────────────────┐
│         Consumer side (ProactiveService scheduler tick)          │
│                                                                 │
│  every 60 ticks (~30 min):                                      │
│    LearningScheduler::rebuild_now(now_ms)                       │
│      1. buffer.drain()                                          │
│      2. FacetStore::load_all() → Vec<FacetSnapshot>             │
│      3. StabilityDetector::rebuild(snaps, cands, now_ms)        │
│           ↳ score = base × cue_mult                             │
│           ↳ pre-budget state via TAU thresholds                 │
│           ↳ per-class top-K trim (Active overflow → Provisional)│
│      4. FacetStore::write_transitions() — INSERT OR REPLACE      │
│      5. FacetCache::refresh_from(store) — hot read snapshot     │
└──────────────────────────────────────┬─────────────────────────┘
                                       │
                          Arc<learning::cache::FacetCache>
                                       │
┌──────────────────────────────────────▼─────────────────────────┐
│                  Read side (system prompt builder)               │
│                                                                 │
│  ChatDelegate::effective_system_prompt(...) →                   │
│    1. compose base prompt + memory_context + mode prompt        │
│    2. append skills_manifest_block (existing behaviour)         │
│    3. append learned_profile_block ◄── new (Sprint 1.8)         │
│         (built once per agent loop by Sprint 1.10 from           │
│          UserProfileSection::render(&facet_cache))              │
│                                                                 │
│  Output: '## User Profile (Learned)' block in every LLM call    │
└────────────────────────────────────────────────────────────────┘
```

## Six-class taxonomy (cheat sheet)

| Class | Half-life | Budget | Example |
|---|---|---|---|
| Identity | 90d | 4 | name, timezone, role |
| Goal | 60d | 3 | ship phase 8 |
| Tooling | 30d | 5 | editor=helix, package_manager=pnpm |
| Veto | 30d | 3 | "never use vim" |
| Style | 14d | 4 | verbosity=terse |
| Channel | 7d | 1 | primary=wechat |

## Cue family weights (used in score formula)

| Cue | Weight | Example |
|---|---|---|
| Explicit | 1.0 | "I prefer pnpm" |
| Structural | 0.9 | `package.json#packageManager` |
| Behavioral | 0.7 | observed-from-context |
| Recurrence | 0.6 | tier_escalator backlinks |

## State transitions

```
new candidate              base × cue_mult
  ───►                     ───►              ── score ──►
                                                  │
                                                  ├── ≥ 1.5  → Active        (prompt-visible)
                                                  ├── ≥ 0.7  → Provisional   (DB-only)
                                                  ├── ≥ 0.4  → Candidate     (waiting for evidence)
                                                  └── < 0.4  → Forgotten     (kept for audit, not deleted)

After scoring: per-class top-K Active trim;
budget overflow → Provisional. Class budgets:
  Tooling=5, Style=4, Identity=4, Veto=3, Goal=3, Channel=1
```

## Three new IPC commands

| Command | Input | Output |
|---|---|---|
| `memory_learning_rebuild_now` | `{ space_id?: string }` | `RebuildOutcome` JSON (promoted_to_active / promoted_to_provisional / demoted_for_budget / forgotten / unchanged / total) |
| `memory_learning_list_facets` | `{ class?: string, state?: string }` | `Vec<FacetDto>` (camelCase wire) |
| `memory_learning_dismiss_facet` | `{ facet_id: string }` | `{ facet_id, rows_updated, new_state: 'forgotten' }` |

## Config flag

```jsonc
{
  "memory_os": {
    "learning_enabled": true   // Sprint 1 default ON
  }
}
```

Effects when off:
- ProactiveService scheduler tick block skips `rebuild_now`
- `memory_learning_rebuild_now` IPC returns a structured error
- `list` + `dismiss` IPCs keep working (so user can triage pre-existing facets)
- `learned_profile_block` stays empty → `effective_system_prompt` skips
  the append → no `## User Profile (Learned)` in the prompt

## Local verify recipe

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/sprint-1-prompt-section-and-extractor
git pull --ff-only

cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Per-module test commands (~89 tests total):
cargo test --lib learning::candidate 2>&1 | tail               # 13
cargo test --lib learning::stability_detector 2>&1 | tail      # 15
cargo test --lib learning::scheduler 2>&1 | tail               # 11
cargo test --lib learning::cache 2>&1 | tail                   # 7
cargo test --lib learning::prompt_section 2>&1 | tail          # 7
cargo test --lib learning::extractor 2>&1 | tail               # 19
cargo test --lib memory_graph::profile_md 2>&1 | tail          # 13
cargo test --lib db::migrations::tests::v39 2>&1 | tail        # 4

cd ../ui && npx tsc --noEmit 2>&1 | head
```

**End-to-end smoke** (after `cargo tauri dev`):

1. Open a new chat session.
2. Send: "Hi, my name is Alice. I work in PST and I prefer pnpm over npm."
3. Wait 30+ min OR call `__TAURI__.core.invoke('memory_learning_rebuild_now', { input: {} })` from the dev console.
4. Open Settings → call `memory_learning_list_facets` from dev console — see 3 facets in Active state: identity:name=Alice, identity:location=PST, tooling:primary=pnpm.
5. Send another message. The system prompt now contains:
   ```
   ## User Profile (Learned)

   **Identity**
   - name: Alice
   - location: PST

   **Tooling**
   - primary: pnpm
   ```

## What this PR does NOT add

- **PROFILE.md on disk** — Sprint 1.7 ships the parser/renderer + I/O
  helpers, but Sprint 1.10 didn't yet wire the periodic write to
  `~/<workspace>/PROFILE.md`. The prompt injection works purely from
  `FacetCache`; PROFILE.md disk-write is a Sprint 2-era item.
- **Frontend UI for facets** — Settings tab to view/dismiss facets,
  inline "agent learned about you" indicator. IPC is ready; the React
  component is future work.
- **`extractor::extract_from_chat_turn` callsite in the agent loop** —
  Sprint 1.9 ships the function but Sprint 1.10 didn't wire it into
  `dispatcher::run_loop`'s post-turn hook yet. **Until that lands the
  pipeline is dormant.** This is the one Sprint 1.5-1.11 follow-up
  that needs to happen before the system actually does anything.
- **Composio integration / OAuth bulk backfill** — openhuman's piece A
  loop. Out of scope (we don't have Composio).
- **`UserState::Pinned` / human-in-the-loop override** — Phase 13
  review-queue work.
- **`EvidenceRef::Provider` variant** — added only when integrations
  land.

## Adjacent edits per CLAUDE.md

- `src-tauri/src/db/migrations.rs` — V39 const + run-block + 4 unit tests
- `CLAUDE.md` — V39 row in migration registry
- `src-tauri/src/learning/` — new top-level module (6 files: mod, candidate, stability_detector, scheduler, cache, prompt_section, extractor)
- `src-tauri/src/memory_graph/profile_md.rs` — new file (13 unit tests)
- `src-tauri/src/memory_graph/mod.rs` — `pub mod profile_md`
- `src-tauri/src/lib.rs` — `pub mod learning`
- `src-tauri/src/memubot_config.rs` — `learning_enabled` flag + default
- `src-tauri/src/agent/dispatcher.rs` — `learned_profile_block` field + setter + append in `effective_system_prompt`
- `src-tauri/src/proactive/service.rs` — `MemoryOsRuntimeConfig` grows 3 fields + tick block
- `src-tauri/src/app.rs` — 3 new AppState fields + bootstrap
- `src-tauri/src/main.rs` — wires `learning_scheduler` + `facet_cache` into `MemoryOsRuntimeConfig`; invoke_handler! registers 3 new commands
- `src-tauri/src/ipc.rs` — 4 new DTOs
- `src-tauri/src/tauri_commands.rs` — 3 new commands

No new V-migration besides V39. No new external dependencies.

## Honest open items (track these for Sprint 2 entry)

1. **The producer is dormant** — `extract_from_chat_turn` exists but
   isn't called from the agent path. Sprint 2 first commit will be a
   tiny ~5-line hook in `dispatcher::run_loop` (or post-turn callback)
   that calls it on each completed user turn. Until that lands, the
   FacetCache will never gain facets and the system prompt stays
   unchanged.
2. **PROFILE.md disk-write loop** — `profile_md::render` is called by
   nobody yet. The Sprint 2-era hook is one line in the same tick
   block as the FacetCache refresh.
3. **Producer→LLM cost cap** — `extract_via_llm` writes to
   `cost_records.model = 'memory_learning:<actual>'` but no daily-cap
   summation is wired yet. Phase 5's `memory_lint%` cost guard
   pattern is the template; should be ~2 commits in Sprint 2.

## What Sprint 2 should do (preview)

1. **Wire producer into agent path** (the dormant-pipeline fix above)
2. **PROFILE.md disk write on every rebuild**
3. **Producer daily token cap**
4. **Frontend Profile UI** (Settings tab listing active/provisional facets + dismiss button)
5. **The MCP completeness audit** queued as Task #77 — should land before Sprint 2 finishes since gbrain Sprint depends on stable MCP server abstractions.
