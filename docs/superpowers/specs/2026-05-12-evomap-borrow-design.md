# Skill Recall v2 — EvoMap-borrowed Enhancements

**Date:** 2026-05-12
**Status:** Draft (pending writing-plans)
**Builds on:** PR #103 (skill-recall closed loop), PR #104 (badge Skills row), PR #106 (usage-field spread fix)
**Inspired by:** EvoMap/evolver architecture study (see chat thread; arXiv:2604.15097)

## Background

PR #103 closed the basic recall loop (extract → store → manifest → tools → cite → re-rank). After studying [EvoMap/evolver](https://github.com/EvoMap/evolver) we identified **seven concrete enhancements** that fit uClaw's probe-first philosophy without adopting EvoMap's heavier Gene-schema architecture, plus a small fix to the permanent-cited_count problem.

These changes share two properties:

1. **No new SQL schema.** Everything is additive metadata on `memory_nodes.metadata_json`, or pure read-side logic.
2. **No philosophy violation.** Manifest stays compact, recall stays agent-driven, validation stays advisory.

## Goals

- Make skill recall **more accurate** (signals, semantic search) and **less stale** (decay).
- Surface **richer evidence** in the UI (timeline of how skills evolved).
- Let the user **tilt recall** toward the kind of work at hand (repair / optimize / innovate) — Evolver's strategy preset idea.

## Non-goals

- Adopting EvoMap's full Gene JSON schema (philosophy conflict — see EvoMap study §4 "不应该做").
- Sharing/Hub layer (we stay local).
- Auto-running validation commands (validation stays advisory, agent decides).
- Replacing the existing keyword search; semantic search **augments**, doesn't replace.

---

## The 7+1 items

### Item 1: `signals[]` on learned-skill metadata

**Problem:** Today `skill_search` ranks by `keyword_hits + cited×0.5 + usage×0.2`. Keywords come from `memory_keywords` table, populated from the skill body. A skill about "API key rotation" with body keywords `api`, `key`, `rotation` won't fire on a query like "我的 401 错误处理" even though it's the right skill.

**Fix:** Have the extraction prompt output 3-5 **trigger signals** — short phrases describing *when* the skill applies. Store in `metadata.signals: string[]`. `skill_search` scoring formula gains a third term:

```
score = keyword_hits × 1.0
      + signal_match_count × 1.5    -- NEW; matches user query tokens against metadata.signals
      + cited × 0.5
      + usage × 0.2
```

`signals` are also surfaced in the `skill_search` tool result (one line in the response) so the LLM can decide whether to `load_skill` based on the trigger description, not just the title.

**Manifest impact:** signals are NOT added to the manifest line (keeps it short). Only summary + cite count stay on the manifest. signals are first revealed when the agent calls `skill_search`.

### Item 2: `signals_seen[]` from agent_turns failures

**Problem:** Today skills are extracted from `ExecutionLog` (success/failure tuples) without classifying the failure type. A skill about "fallback when Yahoo Finance returns 403" doesn't know it was born from a 403 — that knowledge is lost.

**Fix:** During `skill_extraction` scenario, while reading `ExecutionLog`, run a lightweight regex/classifier to extract failure signals:
- HTTP 4xx → `signal: "http_4xx"`
- HTTP 5xx → `signal: "http_5xx"`
- timeout → `signal: "timeout"`
- "Permission denied" → `signal: "permission_denied"`
- JSON parse error → `signal: "parse_error"`
- (5-8 categories total; small fixed taxonomy in Rust)

Persisted in `metadata.signals_seen: string[]` (different from Item 1's `signals`; this is empirical, that's prescriptive).

`skill_search` scoring uses `signals_seen ∩ current_session_failures` as another match dimension. Session failures come from the existing agent_turns FTS — no new schema.

### Item 3: `validation_hint` on `load_skill` return

**Problem:** When agent applies a skill, there's no signal of "how do I know it worked?". Evolver's Gene has structured `validation[]` commands; we don't want auto-execution but we DO want the hint.

**Fix:** Extraction prompt asks LLM to optionally fill one sentence describing post-application verification. Stored in `metadata.validation_hint: string?`. `load_skill` returns it; agent sees the hint and decides whether to actually verify. Probe-first preserved.

**Example:** Skill body says "split request into per-quarter calls when daily granularity returns 429"; validation_hint says "Re-run with smaller date range and confirm 200 response".

### Item 4: Semantic search augmentation

**Problem:** Keyword + LIKE search misses paraphrases. "Apple 财报" doesn't match a skill titled `stock-research-multi-source` even though semantically perfect.

**Fix:** Add semantic channel using `fastembed` (already bundled in `pyembed`). On skill creation/update, generate a 384-dim embedding of the body + signals, store in `memory_versions.embedding_json` (existing column, currently null). On `skill_search`, compute query embedding, cosine-sim against all active versions in the space, take top-N by cosine, merge with keyword hits.

Scoring after merge:
```
score = (keyword_hits × 1.0 + signal_match × 1.5 + cited × 0.5 + usage × 0.2)
       + cosine_sim × 2.0   -- NEW; scaled so a perfect match dominates a single keyword hit
```

`memory_versions.embedding_json` is already declared but unused — no schema change. For backward compat: if a version has no embedding, treat as cosine=0 (keyword channel still works alone).

**Backfill:** On boot, scan active versions with `embedding_json IS NULL` and enqueue background embedding generation. Idle-priority, doesn't block other proactive work.

### Item 5: Skill evolution timeline panel (Settings UI)

**Problem:** `memory_versions` already records active/superseded versions but no UI exposes the history. Users can't see "this skill was updated last Tuesday because the previous version had X issue".

**Fix:** Settings → 已学技能 → click a skill → new "Evolution" tab showing:
- All versions (active + superseded) with timestamps
- Diff between consecutive versions (use `react-diff-view` or just side-by-side text)
- Citation/usage events per version (from agent_turns where signals_seen exists)
- Why the version was superseded (capture this in `memory_versions.metadata_json` during update; if missing for legacy versions, show "(no reason recorded)")

Pure frontend + one new IPC `get_skill_versions(node_id) → Vec<MemoryVersionDetail>`. Backend method already exists (`get_versions`); wrap it.

### Item 6: Strategy preset (manifest re-ranking)

**Problem:** Evolver's `EVOLVE_STRATEGY=harden` lets the user tilt selection toward repair skills. uClaw treats all skills equally regardless of session intent.

**Fix:** Two-part:
- **Extraction prompt** asks LLM to tag the skill `category: "repair" | "optimize" | "innovate"`. Stored in `metadata.category`.
- **Manifest builder** accepts an optional `bias?: 'repair' | 'optimize' | 'innovate'`. When set, scores get an extra `category_match × 3.0` term, pushing matching-category skills up.

**UI:** A small dropdown in the agent input toolbar (next to ContextUsageBadge): `🎯 当前模式: 平衡 / 修 bug / 优化 / 探索`. State is per-session in `agentSessionStrategyMapAtom`. Default `balanced` = no bias.

**Trade-off:** Adds 1 line to the system prompt manifest header ("Current mode: <category>") so the LLM knows which category is being prioritized.

### Item 7: `cited_count` decay (cron + ranking formula)

**Problem:** `cited_count` is monotonically increasing forever. A skill cited 10 times three months ago beats a fresh skill cited 3 times last week.

**Fix:** Two layers:
- **Cron-driven raw decay:** Background task (in `ProactiveService`) runs weekly: `metadata.cited_count = floor(metadata.cited_count * 0.95)` for all learned-skill nodes. Trivial.
- **Recency-aware ranking:** `list_top_learned_skills` (the E3 query) adds a `last_cited_at`-derived bonus. See Item 8.

Decay is **not** retroactive; existing counters just start declining from current value. No data loss.

### Item 8 (=§5): `last_cited_at` field

**Problem:** Even with weekly decay, ranking still treats "cited 5 times last month" same as "cited 5 times today" until the next decay tick.

**Fix:** Add `metadata.last_cited_at: ISO-8601 string?`. Updated on every `record_skill_cited` call. The E3 ranking query incorporates a recency multiplier:

```
recency_factor = max(0.5, 1.0 - days_since_last_cited / 30.0)
final_score = (cited_count × recency_factor) × 10 + usage_count × 3 + recency_bonus
```

Where:
- `days_since_last_cited` = (now - last_cited_at) in days, or `Infinity` if never cited
- `recency_factor` clamps to 0.5 floor — old skills don't disappear, just deprioritize
- `recency_bonus` already exists from PR #103 (updated_at within 7/30 days)

For learned skills never cited: `recency_factor = 0.5` (since `last_cited_at` is null and days_since = infinity → clamp). They lose half their score, which is consistent with "not yet validated".

---

## Architecture summary

| Item | Where | Touches |
|---|---|---|
| 1. signals[] | extraction prompt + `skills_manifest.rs` + `skill_search.rs` | metadata + scoring |
| 2. signals_seen[] | `skill_extraction.rs` + small classifier helper | metadata + analytics |
| 3. validation_hint | extraction prompt + `load_skill.rs` | metadata + return shape |
| 4. semantic search | new embedding service + `skill_search.rs` + boot backfill | uses existing embedding_json column |
| 5. timeline UI | new Settings tab + 1 IPC wrapping existing `get_versions` | frontend-only on backend side |
| 6. strategy preset | extraction prompt + `skills_manifest.rs` + new atom + toolbar dropdown | metadata + scoring + UI |
| 7. cited_count decay | ProactiveService cron | metadata mutation |
| 8. last_cited_at | `record_skill_cited` + `list_top_learned_skills` query | metadata + ranking |

**No SQL migrations.** All metadata JSON additions.

## Failure modes

| Scenario | Behavior |
|---|---|
| Extraction LLM doesn't output signals/category/validation_hint | All fields optional; absence → field stays unset; no error |
| Backfill embedding fails for one node | Skip + log; that node stays cosine=0; doesn't block other backfills |
| Embedding model unavailable (pyembed missing) | Semantic channel degrades silently to keyword-only; tracing::warn on first detection per session |
| `last_cited_at` parse error (malformed legacy data) | Treat as `null` → recency_factor = 0.5; skill keeps participating |
| Strategy preset changes mid-session | Manifest rebuilds on next LLM call (cache invalidated); takes effect immediately |
| Old skill with `cited_count=100, last_cited_at=null` | Half score (0.5 factor); demoted but visible. After first re-cite, normalizes. |

## Philosophy check

- Manifest size: items add at most 1 line per skill (signals are NOT in manifest); cap at 30/1500 tokens unchanged.
- Agent autonomy: nothing forces `skill_search` / `load_skill` calls; nothing auto-runs validation_hint.
- Sharing: zero — all changes are local.
- Schema: zero — only metadata JSON additions.

## Test coverage

Each item ships unit tests verifying:
- Item 1: signals match contributes to scoring; query without signal match still works
- Item 2: signals_seen taxonomy classifier round-trip; metadata write
- Item 3: validation_hint round-trip in load_skill return
- Item 4: cosine search merges with keyword; null embedding → cosine=0 graceful
- Item 5: IPC returns versions; frontend renders diff (vitest)
- Item 6: strategy preset bias affects manifest ordering; default `balanced` matches old behavior
- Item 7: decay tick updates cited_count correctly; cron registration smoke test
- Item 8: `last_cited_at` write on record_skill_cited; recency_factor clamp at 0.5

## Estimated LOC

| Item | Est. LOC |
|---|---|
| 1. signals[] | ~150 (extraction + scoring + tests) |
| 2. signals_seen[] | ~180 (classifier + storage + tests) |
| 3. validation_hint | ~80 (extraction + return field + tests) |
| 4. semantic search | ~450 (fastembed wrapper + backfill + cosine + tests) |
| 5. timeline UI | ~350 (Settings tab + diff component + IPC + tests) |
| 6. strategy preset | ~250 (atom + dropdown + manifest bias + tests) |
| 7. decay cron | ~80 (ProactiveService task + tests) |
| 8. last_cited_at | ~50 (write + ranking + tests) |

**Total:** ~1,590 LOC across ~14 files, one PR with 8 bisectable commits.

## Out-of-scope, noted for later

- **Multi-version embeddings.** Each skill version gets one embedding from active version's body. If a skill evolves significantly, the embedding only reflects the latest. Acceptable v1.
- **Strategy preset auto-detection.** UI is manual; could later auto-infer from session activity ("you've been editing code → repair mode"). Out of scope.
- **Skill conflict / supersede UX.** Timeline shows what changed but doesn't help resolve "this old version was good for X, why was it replaced?". UX work, defer.
- **Hot decay tuning.** The 0.95 weekly + 0.5 floor + 30-day half-life are heuristics. After dogfood, may want to make them configurable.

## Migration impact

- No SQL changes
- Existing skills lack new fields (signals, validation_hint, category, last_cited_at) — all optional, treated as absent
- Embeddings backfill happens lazily in idle proactive cycles; no startup delay
- Existing `recordSkillCited` IPC continues to work; gains the `last_cited_at` write as a side effect

## Done definition

This spec is "done" when:
1. All 8 items have passing tests
2. `cargo test --lib` and `npm test -- --run` both clean
3. `tsc --noEmit` clean
4. PR opened with one squash-merge commit message per item (8 commits in branch, 1 commit on main)
5. Manual smoke: extract a new skill on a real session → verify signals/category fields populate → invoke `skill_search` with a paraphrased query (semantic) → confirm cosine path fires → strategy preset dropdown reorders manifest visibly → timeline tab shows ≥1 version
