# Memory Growth (T4: reflections + user model) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every N agent turns, distill the new conversation into a durable *reflection*; keep a single LLM-written *user model* synthesized from the learned facts; inject both into each turn's dynamic context; let the user see, archive and refresh them.

**Architecture:** A new `memory_graph::growth` module (store / prompts+parse / render / service) sits *above* the existing fact pipeline (openhuman-G `reflection.rs`) and the facet profile — it reads their output, it does not replace them. Background passes run through `AppState.learning_llm` under their own daily token budget, single-flight. The trigger is a DB watermark (assistant messages since the last reflection), so it survives restarts. Injection is a new per-turn dynamic block, never the cached system prompt.

**Tech Stack:** Rust (rusqlite, tokio, `MemoryOsLlm` / `MockMemoryOsLlm`), React + Vitest.

**Spec:** `docs/superpowers/plans/2026-11-09-uclaw-pi-backport.md` § T4. Port source: uclaw-pi `src-tauri/src/memory_graph/reflection_service.rs` and commits `839f7d73 67137d0f 388ca179 9c79c908 60d53a54 9d702cac 04ed4e2c 554fb8ca e6ecfac0 77705938` (read with `git -C ../uclaw-pi show main:<path>`). Daydream (T5) is out of scope.

## Decisions that differ from uclaw-pi

1. **Name:** `memory_graph::growth`, types `Growth*` — `ReflectionOrchestrator` (fact extraction) already exists here.
2. **Own budget that actually counts:** cost tags `memory_growth_reflection|promotion|consolidation`; new `cost_store::today_growth_tokens` sums `model LIKE 'memory_growth%'`; new config `growth_daily_token_budget` (default 20_000). uclaw-pi's passes never counted against any budget.
3. **Watermark trigger, not an in-memory counter:** run when `COUNT(assistant agent_messages created after the newest reflection's watermark) >= growth_every_n_turns` (default 20). Survives restart; no counter state. A V65 index on `agent_messages(created_at)` keeps it cheap.
4. **Each reflection reads only new material:** the last ≤ 50 user/assistant messages *after* the previous watermark (uclaw-pi re-read the global last 50 every time). Watermark = `created_at` of the newest message read, stored on the reflection row.
5. **Single-flight:** one `AtomicBool` guard; a trigger while a pass runs is dropped.
6. **Consolidation archives only the rows it read** (uclaw-pi archived all live rows, losing any beyond its 40-row read cap). Runs after every 5th reflection when ≥ 8 live.
7. **Promotion records history when the summary changes** (uclaw-pi only did so in consolidation) and bounds its input (≤ 60 facts, ≤ 6000 chars; uclaw-pi had no LIMIT).
8. **Promotion input:** active facets (`FacetCache::all_active`) + `memory_nodes` of kinds `user_profile|identity|value|directive` (not archived), fact text from `get_active_version`, ordered by `metadata.reinforced_count` desc then recency.
9. **Agent mode only (v1):** chat-mode `messages` hold JSON content blocks and already feed openhuman-G; the trigger and transcript use `agent_messages`. Injection goes to both composer paths.
10. **Settings + viewer ship with it:** a 记忆成长 settings section (enable, N, daily budget) and a read-mostly 成长 tab in 万花筒 → 记忆 (user model + history, reflections with archive/restore, 立即刷新).

## Global Constraints

- Migration **V65** (next free; update `CONTEXT.md` registry). Idempotent `CREATE … IF NOT EXISTS`; `archived_at` inside the CREATE (no ALTER).
- Nothing runs unless `learning_llm.is_some()` (i.e. `memory_os.learning_enabled` and its budget > 0) **and** `memory_os.growth_enabled` (default `true`) **and** `today_growth_tokens < growth_daily_token_budget`.
- max_tokens: reflection 2048, promotion 2048, consolidation 4096 (uclaw-pi `554fb8ca`).
- Prompts: port uclaw-pi's `REFLECTION_SYSTEM_PROMPT`, `PROMOTION_SYSTEM_PROMPT` (with the "CRITICAL: state ONLY…" sentence) and `CONSOLIDATION_SYSTEM_PROMPT` verbatim.
- Injection caps (characters): user model 1200, reflections 3 × 300. Blocks: `<user_model>…</user_model>` then `<recent_reflections>` with lines `- {insight}`.
- Timestamps stored as epoch ms (INTEGER), matching `agent_messages.created_at`.
- User-facing copy is Simplified Chinese. Frontend ships in the same PR.
- Pre-existing failures to ignore: Rust `shell::…test_daemon_mode_approval_unchanged`, `browser::runtime_pack_ipc::…print_real_status_diagnostics`, three `browser::provider_execution::*cli_route*`, flaky `browser::playwright_cli` worker tests; Vitest two Kaleidoscope `MemoryGraphView` tests; tsc baseline 71 lines.

## Review Focus

1. **LLM returns prose, fenced JSON, or an empty string** — prose is kept as the insight at confidence 0.5; fences are stripped; empty → no row. Test in Task 3.
2. **No `learning_llm` / growth disabled / budget spent** — no LLM call, no rows, no panic. Test in Task 4.
3. **Two triggers in quick succession** (fast turns) — only one pass runs; the second is dropped. Test in Task 4.
4. **Consolidation output larger than its input or empty** — database untouched. Test in Task 4.
5. **A user model block that would overflow** (very long summary / long insights) — truncated on a char boundary to the caps, never mid-codepoint. Test in Task 3.

---

### Task 1: V65 migration

**Files:** Modify `src-tauri/src/db/migrations.rs` (new `SQL_V65` + runner call after V64), `CONTEXT.md` (registry row).

**Interfaces — Produces (SQL):**
```sql
CREATE TABLE IF NOT EXISTS growth_reflections (
  id TEXT PRIMARY KEY, insight TEXT NOT NULL, confidence REAL NOT NULL DEFAULT 0.5,
  source_message_count INTEGER NOT NULL DEFAULT 0, watermark_ms INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, archived_at INTEGER);
CREATE INDEX IF NOT EXISTS idx_growth_reflections_created ON growth_reflections(created_at DESC);
CREATE TABLE IF NOT EXISTS growth_user_model (id TEXT PRIMARY KEY, summary TEXT NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS growth_user_model_history (id TEXT PRIMARY KEY, summary TEXT NOT NULL, replaced_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_agent_messages_created ON agent_messages(created_at);
```
(Tables prefixed `growth_` to avoid any future clash with uclaw-pi names.)

- [ ] **Step 1:** test `v65_creates_growth_tables_idempotently` (run migrations twice on in-memory DB; insert/select each table).
- [ ] **Step 2:** run `cargo test --lib v65` — FAIL.
- [ ] **Step 3:** add `SQL_V65` + runner call; registry row `| V65 | growth_reflections / growth_user_model(+history) + agent_messages(created_at) index — memory growth T4 | this PR |`.
- [ ] **Step 4:** run — PASS.
- [ ] **Step 5:** commit `feat(db): V65 growth tables (reflections, user model, history)`.

### Task 2: Growth store

**Files:** Create `src-tauri/src/memory_graph/growth/mod.rs`, `growth/store.rs`; modify `memory_graph/mod.rs`.

**Interfaces — Produces (`store.rs`, all `&rusqlite::Connection`, `anyhow::Result`):**
- `struct ReflectionRow { id: String, insight: String, confidence: f64, created_at: i64, archived_at: Option<i64> }` (Serialize, camelCase)
- `insert_reflection(conn, insight: &str, confidence: f64, source_message_count: i64, watermark_ms: i64) -> String` (uuid v4 id, `created_at = now ms`)
- `list_reflections(conn, limit: usize, include_archived: bool) -> Vec<ReflectionRow>` (newest first)
- `set_reflection_archived(conn, id: &str, archived: bool) -> bool`
- `archive_reflections(conn, ids: &[String])`
- `latest_watermark(conn) -> i64` (max `watermark_ms` over all rows incl. archived; 0 if none)
- `count_assistant_messages_since(conn, after_ms: i64) -> i64`
- `struct TranscriptMessage { role: String, content: String, created_at: i64 }`; `messages_since(conn, after_ms: i64, limit: usize) -> Vec<TranscriptMessage>` (role in user/assistant, oldest first, the *last* `limit` after `after_ms`)
- `get_user_model(conn) -> Option<UserModel>`; `struct UserModel { summary: String, updated_at: i64 }`
- `set_user_model(conn, summary: &str) -> bool` — upsert id `default`; when a different summary existed, insert it into history first; returns whether it changed
- `list_user_model_history(conn, limit) -> Vec<UserModelHistoryRow { summary, replaced_at }>`

- [ ] **Step 1:** tests (in-memory DB with V65 + a minimal `agent_messages` table): `insert_and_list_newest_first`, `archived_hidden_unless_requested`, `watermark_and_counts_track_new_assistant_messages`, `messages_since_returns_last_n_oldest_first_user_and_assistant_only`, `set_user_model_writes_history_only_on_change`.
- [ ] **Step 2:** run `cargo test --lib memory_graph::growth::store` — FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** run — PASS.
- [ ] **Step 5:** commit `feat(memory): growth store (reflections, user model, history, watermark)`.

### Task 3: Prompts, parsers, render

**Files:** Create `growth/prompts.rs`, `growth/parse.rs`, `growth/render.rs`.

**Interfaces — Produces:**
- `prompts.rs`: `REFLECTION_SYSTEM`, `PROMOTION_SYSTEM`, `CONSOLIDATION_SYSTEM` (verbatim port); `reflection_user(transcript: &str) -> String`, `promotion_user(digest: &str) -> String`, `consolidation_user(reflections: &[ReflectionRow], user_model: Option<&str>, digest: Option<&str>) -> String` (uclaw-pi formats).
- `parse.rs`: `parse_reflection(s: &str) -> Option<(String, f64)>` (JSON `{insight, confidence}` via first-`{`-to-last-`}` extraction; confidence clamp [0,1] default 0.5; non-JSON non-empty prose → `(prose, 0.5)`; empty → None); `struct Consolidation { reflections: Vec<(String, f64)>, user_model: Option<String> }`; `parse_consolidation(s) -> Option<Consolidation>`; `consolidation_applies(input_count: usize, c: &Consolidation) -> bool`.
- `render.rs`: `render_transcript(msgs: &[TranscriptMessage]) -> String` (`{role}: {content≤2000 chars}\n`); `render_growth_block(user_model: Option<&str>, reflections: &[ReflectionRow]) -> Option<String>` (caps from Global Constraints, top 3, char-boundary truncation with `…`).

- [ ] **Step 1:** tests: `parse_reflection_json_fenced_prose_and_empty`, `parse_consolidation_reads_both_and_rejects_prose_or_empty`, `consolidation_applies_guards_growth_and_empty`, `render_transcript_caps_each_message`, `render_growth_block_caps_and_orders` (user model first; 4 reflections → 3; 5000-char CJK summary → 1200 chars + `…`; none → None).
- [ ] **Step 2–4:** RED, implement, GREEN (`cargo test --lib memory_graph::growth`).
- [ ] **Step 5:** commit `feat(memory): growth prompts, parsers and prompt block`.

### Task 4: Growth service

**Files:** Create `growth/service.rs`; modify `src-tauri/src/cost_store.rs` (`today_growth_tokens`), `src-tauri/src/memubot_config.rs` (`growth_enabled: bool = true`, `growth_every_n_turns: u32 = 20`, `growth_daily_token_budget: u32 = 20_000`, serde defaults + deser test).

**Interfaces — Produces:**
- `struct GrowthDeps { db: Arc<std::sync::Mutex<Connection>>, llm: Option<Arc<dyn MemoryOsLlm>>, store: Arc<MemoryGraphStore>, facets: Arc<FacetCache>, space_id: String }` (match the real field types in `AppState`)
- `struct GrowthSettings { enabled: bool, every_n_turns: u32, daily_budget: u32 }` + `from_config(&MemoryOsConfig)`
- `struct GrowthService { running: AtomicBool }` with `pub fn new()`
- `async fn maybe_run(&self, deps: &GrowthDeps, s: &GrowthSettings) -> GrowthOutcome` — gates (llm, enabled, budget), watermark count ≥ N, single-flight, then `run_reflection` → `run_promotion` → every 5th reflection `run_consolidation`
- `async fn run_now(&self, deps, s) -> GrowthOutcome` — the 立即刷新 path: promotion + consolidation, same gates, ignores N
- `enum GrowthOutcome { Skipped(&'static str), Ran { reflection: bool, promoted: bool, consolidated: bool } }`
- `fn build_profile_digest(deps) -> Option<String>` per decision 8

- [ ] **Step 1:** tests with `MockMemoryOsLlm` + in-memory DB: `no_llm_or_disabled_or_over_budget_does_nothing`, `below_n_new_turns_skips`, `n_turns_creates_reflection_with_watermark_then_promotes`, `concurrent_trigger_is_dropped` (hold the guard, call `maybe_run` → `Skipped("busy")`), `consolidation_archives_only_rows_it_read`, `consolidation_rejected_output_leaves_db_untouched`, `growth_spend_counts_against_budget` (cost row `memory_growth_reflection:m` sums in `today_growth_tokens`).
- [ ] **Step 2–4:** RED, implement, GREEN (`cargo test --lib growth` and `cargo test --lib memubot_config`).
- [ ] **Step 5:** commit `feat(memory): growth service (reflection, promotion, consolidation; budget + single-flight)`.

### Task 5: Trigger + injection

**Files:** Modify `src-tauri/src/app.rs` (`AppState.growth: Arc<GrowthService>`), `src-tauri/src/tauri_commands.rs` (agent turn tail after the PreferenceExtractor spawn ~10557: pre-clone deps before the spawn, `tokio::spawn(growth.maybe_run(..))`; both composer paths: read the block at turn start and `set_growth_block`), `src-tauri/src/agent/dispatcher/{mod.rs,content_assembler.rs}` (`PromptBlocks.growth`, `SystemPromptContext.growth_block`, setter, render after `learned_profile`).

**Interfaces — Consumes:** `render_growth_block`, `GrowthService::maybe_run`. **Produces:** `ChatDelegate::set_growth_block(String)`; `pub fn load_growth_block(conn) -> Option<String>` in `growth/render.rs`.

- [ ] **Step 1:** tests: `growth_block_goes_to_dynamic_block_not_system` (content_assembler), `load_growth_block_reads_live_rows_only`.
- [ ] **Step 2–4:** RED, implement, GREEN; `cargo build` clean.
- [ ] **Step 5:** commit `feat(agent): trigger memory growth every N turns and inject it per turn` (body notes both composer paths + AppState as adjacent edits).

### Task 6: Tauri commands

**Files:** Create `src-tauri/src/memory_graph/growth/commands.rs`; modify `main.rs` `invoke_handler!`.

**Interfaces — Produces:** `list_growth_reflections(limit?: 30, include_archived?: false) -> Vec<ReflectionRow>`, `set_growth_reflection_archived(id, archived) -> bool`, `get_growth_user_model() -> Option<UserModel>`, `list_growth_user_model_history(limit?: 10)`, `refresh_growth_now() -> GrowthOutcome` (serializes as `{status, reason?, reflection?, promoted?, consolidated?}`), `get_growth_settings() -> GrowthSettings` (camelCase), `set_growth_settings(enabled, everyNTurns 5..=200, dailyBudget 0..=500_000)` (clamp, save config). Errors as plain Chinese strings.

- [ ] **Step 1:** tests on the core fns (`refresh_outcome_serializes`, `set_growth_settings_clamps`).
- [ ] **Step 2–4:** RED, implement, register, GREEN.
- [ ] **Step 5:** commit `feat(memory): growth Tauri commands`.

### Task 7: UI — settings section + 成长 tab

**Files:** Modify `ui/src/lib/tauri-bridge.ts` (+ types); create `ui/src/components/settings/MemoryGrowthSection.tsx` (+ test), mount in the memory settings tab (`MemoryRecallTab`); create `ui/src/views/Kaleidoscope/modules/Memory/GrowthTab.tsx` (+ test), add tab `{ value: 'growth', label: '成长' }` to `MemoryModule.tsx`.

Behavior: settings — `SettingsToggle` 启用记忆成长, number inputs 每 N 轮反思一次 / 每日 token 预算, a note that it needs 学习模型 enabled. 成长 tab — 用户模型 card (summary, updated time, 演化历史 collapsible), 反思 list with confidence, 归档/恢复, 显示已归档 toggle, header 立即刷新 button showing the outcome (`已更新` / `跳过：{reason}` mapped to Chinese).

- [ ] **Step 1:** tests: `growth_settings_loads_and_saves_clamped_values`, `growth_tab_shows_model_and_reflections`, `archive_calls_bridge_and_refetches`, `refresh_shows_skip_reason`.
- [ ] **Step 2–4:** RED, implement, GREEN; tsc ≤ 71.
- [ ] **Step 5:** commit `feat(ui): 记忆成长 settings + 成长 tab`.

---

## Out of scope / follow-ups

- Daydream (T5), chat-mode transcripts, per-space user models, model-role routing by cost tag, de-duplicating the facet block against the user model.
