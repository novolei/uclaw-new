# Memory Recall Eval + Segmented BM25 (T7) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure how often the memory a question needs actually reaches the prompt, then add a jieba-segmented BM25 index (no models) that makes Chinese recall work, and prove the gain on the same measurement.

**Architecture:** A recall-eval library (`src/eval/recall/`: golden-set parsing, hit matching, metrics) plus a thin `examples/memory_recall_eval` CLI that runs every backend against *snapshots* of the live DBs. A new `memory_core` module holds jieba segmentation, a derived FTS5 index over graph facts + bucket_seal chunks (hash-diff sync every 60 s), BM25 search and a `<relevant_memories>` block injected first into `memory_context` on both composer paths under the existing 400 ms deadline.

**Tech Stack:** Rust, rusqlite FTS5 (`unicode61` over pre-segmented text), `jieba-rs 0.11`, tokio.

**Spec:** `docs/superpowers/plans/2026-11-09-uclaw-pi-backport.md` § T7, and uclaw-pi's measurement `git -C ../uclaw-pi show main:docs/eval/2026-11-09-memory-recall-baseline.md` (jieba BM25 alone: hit@5 67.6% / zh 74.1% at < 1 ms; bge-m3 + reranker add ~3 pts at ~700 ms). Port source: uclaw-pi `src-tauri/examples/memory_recall_eval/main.rs`, `src-tauri/src/memory_core/{segment,index,search,mod}.rs`.

## Why this repo needs it (verified on the dev DB)

- `memory_fts` is **trigram** (≥ 3-char substrings) and `mem_tree_chunks_fts` is **`porter unicode61`** (a CJK run is one token): `MATCH` on a two-character Chinese place name returns 0 on both while `LIKE` finds it in 10 chunks; a natural question returns 0 everywhere.
- Both recall FTS calls pass the raw question to `MATCH` unescaped — any `?` is an FTS5 syntax error → empty.
- The only vectors are bge-small-en-v1.5 (English) over bucket_seal *summaries*; chunks and graph facts have none.

## Decisions

1. **Eval first, with a gate.** Port the harness, run the baseline on `~/.uclaw`, record it. Continue to the index only if the baseline confirms the gap (best backend hit@5 < 50% on the golden set). Otherwise stop and report.
2. **Golden set stays private.** Default path `~/.uclaw/eval/memory_recall_golden.jsonl` (seeded once by copying uclaw-pi's 37-question set — same user); only `golden.sample.jsonl` is committed.
3. **BM25 only in this PR.** No fastembed / ort bump / model downloads. Vectors + reranker are a follow-up decided by the post-index eval (the cross-lingual gap — English facts, Chinese questions — is what they'd buy).
4. **Index content:** graph nodes (`memory_nodes` + latest active `memory_versions`, all spaces, not archived) and bucket_seal chunks — chunk text read from the full-body file (`full_content`, derived path), not the 500-char SQL preview. Docs capped at 2000 chars.
5. **Injection:** `<relevant_memories>` (≤ 6 hits, 300 chars each) goes first in the memory context on both paths; computed concurrently with existing recall, bounded by the same 400 ms deadline; empty on miss.
6. **Fix the raw-MATCH bug on the way:** a shared `fts_quote_query` used by `MemoryGraphStore::fts_search` and `BucketSealAdapter::recall` (whitespace tokens, each `"…"`-quoted, `"` doubled) — no more syntax errors.
7. **Out of scope (separate PRs):** doubled `<memory_context>` tag; proactive `prepare_background_context` running an unbounded recall whose result is dropped; agent path hard-coding space `default`; vectors/reranker.

## Global Constraints

- Migration **V66** (next free; registry row in `CONTEXT.md`).
- New dependency: `jieba-rs = "0.11"` only.
- Eval never writes live data: snapshots via `VACUUM INTO` into a temp dir; migrations run on the snapshot only.
- Golden schema (verbatim from uclaw-pi): `{"id","lang","category"?,"query","expect_any"?:[…],"expect_word"?:[…]}`; blank and `//` lines skipped. Hit = any `expect_any` as lowercase substring, or any `expect_word` as `(?-u:\b)WORD(?-u:\b)`.
- Metrics: hit@1/3/5/10, MRR@10, p50 latency, per `all` / `lang=` / `cat=`; prompt table: stored anywhere / in-prompt hit / hit÷stored.
- Segmentation (port of uclaw-pi `segment.rs`): ASCII identifier runs kept whole (lowercased, trailing `.-_` trimmed), everything else through `jieba.cut_for_search(…, true)`; tokens without an alphanumeric char dropped; query side removes the 41 zh + 24 en stopwords and dedups; FTS query = first 32 distinct tokens, each quoted, `OR`-joined; `None` when nothing survives.
- Pre-existing failures to ignore: Rust `shell::…approval_unchanged`, `browser::runtime_pack_ipc::…diagnostics`, three `browser::provider_execution::*cli_route*`, flaky `browser::playwright_cli`; Vitest two Kaleidoscope `MemoryGraphView`; tsc baseline 71.

## Review Focus

1. **Queries that are only punctuation / emoji / stopwords** (`？？`, `我的`) — no FTS error, empty result, other recall unaffected. Test in Task 3.
2. **chunks.db unreadable or missing during a sync** — the cycle is skipped; existing chunk docs are not deleted (sync treats its input as the full corpus). Test in Task 5.
3. **A doc changes or disappears** — its FTS row is replaced or removed, never duplicated. Test in Task 4.
4. **The index is empty or still building at first boot** — injection yields no block, no delay beyond the deadline. Test in Task 6.
5. **Golden file missing or malformed line** — the harness says which file/line and exits non-zero. Test in Task 1.

---

### Task 1: Recall-eval library + harness

**Files:** Create `src-tauri/src/eval/recall/{mod.rs,golden.rs,matcher.rs,metrics.rs}`, `src-tauri/examples/memory_recall_eval/{main.rs,golden.sample.jsonl}`; modify `src-tauri/src/eval/mod.rs`, `src-tauri/Cargo.toml` (`[[example]] memory_recall_eval`).

**Interfaces — Produces:**
- `golden.rs`: `struct GoldenItem { id, lang, category: String, query, expect_any: Vec<String>, expect_word: Vec<String> }`; `fn parse_golden(text: &str) -> Result<Vec<GoldenItem>, String>` (error names the 1-based line).
- `matcher.rs`: `struct Matcher` with `fn new(item: &GoldenItem) -> Self` and `fn hits(&self, text: &str) -> bool`.
- `metrics.rs`: `struct RankedResult { first_hit_rank: Option<usize>, latency_ms: f64 }`; `struct GroupStats { n, hit_at: [f64; 4] /*1,3,5,10*/, mrr: f64, p50_ms: f64 }`; `fn group_stats(results: &[RankedResult]) -> GroupStats`; `fn render_markdown(...)`.
- CLI flags: `--data-dir` (default `~/.uclaw`), `--golden`, `--out`, `--verbose`, `--dump-context`. Backends: `graph_fts` (`MemoryGraphStore::fts_search`), `graph_plan` (`MemoryRecallEngine::build_recall_plan`, triggered+relevant+expanded), `bucket_seal_fts` (`BucketSealAdapter::recall`), `bucket_seal_hybrid` (`recall_hybrid` with the configured embedder when its model dir exists, else inert), `prompt_agent` (graph ctx + bucket_seal block as `send_agent_message` composes them), `prompt_chat` (`load_context_with_skills` as `send_message` does).

- [ ] **Step 1:** unit tests: `parse_golden_reads_fields_and_skips_comments`, `parse_golden_reports_bad_line_number`, `matcher_substring_is_case_insensitive_and_word_is_bounded`, `group_stats_hit_at_k_mrr_and_p50`.
- [ ] **Step 2–4:** RED, implement, GREEN (`cargo test --lib eval::recall`); `cargo build --example memory_recall_eval`.
- [ ] **Step 5:** commit `feat(eval): memory recall eval library + harness`.

### Task 2: Baseline run (gate)

- [ ] **Step 1:** seed `~/.uclaw/eval/memory_recall_golden.jsonl` from `~/.uclaw-pi/eval/` (local only).
- [ ] **Step 2:** run `cargo run --example memory_recall_eval -- --verbose --out /tmp/recall-baseline.json`; expected: tables print, live DB mtimes unchanged.
- [ ] **Step 3:** write `docs/eval/2026-11-09-memory-recall-baseline.md` (setup, data sizes, ranked + prompt tables, root causes). No personal facts in the doc — ids and aggregates only.
- [ ] **Step 4:** **Gate:** best ranked hit@5 < 50% → continue; else stop and report.
- [ ] **Step 5:** commit `docs(eval): uclaw-new memory recall baseline`.

### Task 3: Segmentation

**Files:** Create `src-tauri/src/memory_core/{mod.rs,segment.rs}`; modify `lib.rs`, `Cargo.toml` (`jieba-rs = "0.11"`).

**Produces:** `segment(text) -> Vec<String>`, `segmented_text(text) -> String`, `segment_query(text) -> Vec<String>`, `MAX_QUERY_TOKENS = 32`, `fts_match_query(text) -> Option<String>`.

- [ ] **Step 1:** port uclaw-pi tests: `query_drops_function_words` (宠物, 名字 kept; 我, 什么 dropped), `english_lowercased_and_identifiers_kept`, `fts_query_shapes` (punctuation/emoji/stopword-only → None; quotes balanced), `fts_query_caps_distinct_tokens`, `match_query_works_in_sqlite_fts5`.
- [ ] **Step 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** commit `feat(memory_core): jieba segmentation + safe FTS query builder`.

### Task 4: V66 index + sync

**Files:** Modify `db/migrations.rs` (V66), `CONTEXT.md`; create `memory_core/index.rs`.

**Produces (SQL):** `memory_search_docs(doc_id PK, source, source_id, title, content, content_hash, indexed_at INTEGER)`, `memory_search_fts USING fts5(doc_id UNINDEXED, segmented, tokenize='unicode61 remove_diacritics 2')`.
**Produces (Rust):** `struct SourceDoc { doc_id, source, source_id, title, content }`; `graph_docs(conn) -> Vec<SourceDoc>` (decision 4); `chunk_docs(chunks: &Connection, read_full: impl Fn(&str, &str) -> Option<String>) -> Vec<SourceDoc>`; `struct SyncStats { upserted, deleted, unchanged }`; `sync(conn, docs: &[SourceDoc]) -> SyncStats` (complete-corpus hash diff, one transaction; FTS row deleted only for pre-existing docs).

- [ ] **Step 1:** tests: `v66_tables_exist_and_match_segmented_text`, `incremental_sync_upserts_and_removes` (unchanged → 0 upserts; changed doc replaced once in FTS; vanished doc deleted), `graph_docs_picks_latest_active_version_and_skips_archived`, `chunk_docs_prefers_full_body_and_truncates`.
- [ ] **Step 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** commit `feat(memory_core): V66 search index + hash-diff sync`.

### Task 5: Search + background service

**Files:** Modify `memory_core/mod.rs`; create `memory_core/service.rs`; modify `app.rs` (`AppState.memory_core: Arc<MemoryCore>`, boot spawn in the Stage 3 block).

**Produces:** `struct MemoryHit { doc_id, source, title, content, score: f64 }`; `MemoryCore::new(db)`, `search(&self, query, limit) -> anyhow::Result<Vec<MemoryHit>>` (BM25 `ORDER BY bm25(...) LIMIT limit`); `render_block(hits) -> Option<String>` (`<relevant_memories>`, `- [title] content≤300`, chunk hits without title, `<`/`>` neutralized); `service::run_sync_cycle(core, chunks_path, content_root) -> anyhow::Result<Option<SyncStats>>` (None when chunks.db unreadable); `service::spawn(core, …)` (warm jieba, cycle every 60 s).

- [ ] **Step 1:** tests: `bm25_search_ranks_segmented_chinese_match_first`, `unsearchable_query_is_empty_ok`, `render_block_formats_truncates_and_escapes`, `cycle_skips_when_chunks_unreadable_and_keeps_docs`.
- [ ] **Step 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** commit `feat(memory_core): BM25 search + background index sync`.

### Task 6: Injection + FTS escaping fix

**Files:** Modify `tauri_commands.rs` (both paths: spawn `core.search` on `spawn_blocking`, join under the existing 400 ms deadline, put the block first via a new `ChatDelegate::prepend_memory_context` or by composing before `set_memory_context`); `memory_graph/search.rs` + `memory_bucket_seal/adapter.rs` (use `fts_quote_query`); new `memory_core::segment::fts_quote_query` or a shared helper.

- [ ] **Step 1:** tests: `core_block_first_in_memory_context`, `core_search_timeout_yields_no_block`, `graph_fts_search_survives_question_mark`, `bucket_seal_recall_survives_question_mark`.
- [ ] **Step 2–4:** RED, implement, GREEN; full `cargo test --lib`.
- [ ] **Step 5:** commits `fix(memory): quote raw FTS MATCH input in graph + bucket_seal recall` and `feat(agent): inject segmented BM25 memories first on both composer paths`.

### Task 7: Status in the UI

**Files:** Tauri command `get_memory_index_status -> { docs, lastSyncAt, lastSyncStats }` (in `memory_core/commands.rs`, registered in `main.rs`); a status line in 设置 → 记忆 (`MemoryRecallTab`): "关键词索引：N 条 · 上次同步 …".

- [ ] **Step 1:** tests: Rust `status_reports_doc_count`; Vitest `memory_index_status_line_renders`.
- [ ] **Step 2–5:** RED, implement, GREEN, commit `feat(ui): memory keyword index status`.

### Task 8: Re-run eval (decision)

- [ ] **Step 1:** add backend `memory_core` (index snapshot, `search`) and pass the core block into both prompt backends.
- [ ] **Step 2:** run; append an "After segmented BM25" section to the baseline doc (same tables, plus what still misses and whether vectors look worth it).
- [ ] **Step 3:** commit `docs(eval): recall after segmented BM25`.

---

## Out of scope / follow-ups

Vectors + reranker (bge-m3 / bge-reranker-v2-m3, ort bump), doubled `<memory_context>` tag, proactive wasted recall, agent path's hard-coded `default` space.
