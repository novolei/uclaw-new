# Plan — Backport selected uclaw-pi features into uclaw-new

**Date:** 2026-11-09
**Status:** accepted — mainline confirmed by the user 2026-11-09
**Source repo:** `novolei/uclaw-pi` (local `~/Documents/uclaw-pi`, `main` @ `bd980989`)
**Target repo:** `novolei/uclaw-new` (this repo)

## Update — benchmark results (2026-11-09, later the same day)

Measured with `scripts/bench/` (recording proxy, deepseek-v4-flash, same 7
prompts in both apps). The perceived "uclaw-pi is much faster" gap was **not**
prompt caching or the pi engine: uclaw-new waited a median 77 s before every
LLM request on an unbounded bucket_seal recall starved by background seal
jobs (e2e TTFT 78.7 s vs pi 2.1 s; uclaw-new's provider TTFT and cache hit
were already fine). Fixed on `fix/pi-backport-bugs` (e76b6648, d55f6c0b):
e2e TTFT median now 1.18 s — faster than uclaw-pi. Consequences for this plan:

- T1/T2 (prompt alignment / delta annotation) stay worthwhile for token cost,
  but are **not** latency fixes — the data shows little TTFT impact.
- The pi engine's speed advantage is gone; this strengthens "uclaw-new as
  mainline, borrow patterns, don't vendor the engine".
- Remaining latency work is tracked as P1 there (graph recall ~0.7 s exceeds
  its 400 ms deadline every turn; reuse the HTTP client).

## Background

uclaw-pi is a fork of uclaw-new taken at `a84b51ed` (2026-05-30). After the fork it
vendored `pi_agent_rust` (`crates/pi`, ~385K LOC) as an **opt-in second agent engine**
(`pi_engine_enabled` / `UCLAW_PI_ENGINE`) running alongside the legacy `agentic_loop`.
Both repos stopped at 2026-06-05 (~420 / ~400 commits since the fork).

Decision (2026-11-09): **uclaw-new is the mainline.** It matches the Pi-lightweight ADR
(borrow Pi *patterns* natively, one kernel), has zero external runtimes (memU + gbrain
retired), and owns the complete plugin system. uclaw-pi features are ported **selectively**
as independent slices; the pi engine itself is not ported.

To reference uclaw-pi code from this repo:

```bash
git fetch /Users/ryanliu/Documents/uclaw-pi main:refs/remotes/pi/main
git show pi/main:<path>
```

## Triage of uclaw-pi-only work

| uclaw-pi work | Verdict | Reason |
|---|---|---|
| `memory_versions.updated_at` fix (`2425ad00`) | ✅ **Done** — branch `fix/pi-backport-bugs` (V62) | Same latent bug existed here |
| DeepSeek V4 = 1M ctx (`bd980989`) | ✅ **Done** — branch `fix/pi-backport-bugs` | Here it also drove compaction budgets, not just the meter |
| System-prompt 1024 cache alignment (`2893d256`) | ✅ **Already here** — `95eeae59` (2026-06-01) | Mis-triaged as pi-only; verified 2026-11-09 (`content_assembler` 23/23, `cache_align` 3/3) |
| Drop M2-D memory delta annotation (`08f0480e`) | ✅ **Already here** — `64af65d9` (2026-06-01) | Mis-triaged as pi-only; snapshot test asserts the annotation is absent |
| Skills-manifest relevance ranking (#106) + semantic boost (#111) | ✅ **Done (re-designed)** — PRs #690 + #691 | Here learned skills never reached the prompt; now the turn's recall splits them into a per-turn `<relevant_learned_skills>` block (not the system prompt). #690 fixed skill reads past bucket_seal's 500-char preview |
| ReflectionService + `user_model` (Phase 3 growth) | **Port — T4** (re-home trigger + injection) | Complements openhuman-G; trigger/injection were pi-path specific |
| Daydream | **Optional — T5** | Depends on T4; low user-facing value; decide after T4 soak |
| Skills marketplace module + UI (multi-provider, workspace install, uninstall) | **Port — T6** | Only the agent-tool skills.sh search/install exists here |
| Live tool output, ask_user, auto-retry notice, partial reply on abort, compaction fold card | ❌ **Skip** | pi-path *parity* work (see uclaw-pi `docs/audits/2026-06-04-legacy-pi-event-parity-matrix.md`); the legacy loop here already has all of them (`AgentStatusBar`, `agent/recovery.rs`, `AgentMessages` fold card) |
| Local MiniCPM + desk pet + personas | ❌ **Skip** | Built independently in both repos; this repo's version is the keeper |
| `crates/pi` engine, `engine_sink`, R1–R5 bridge | ❌ **Skip** | Two-engine maintenance cost; contradicts Pi-lightweight ADR |
| memU / gbrain adapters, gbrain dep hints | ❌ **Skip** | Both runtimes retired here |
| Session titles quality-first routing (#98) | ❓ **Needs decision** | Conflicts with this repo's utility-role title model (cheap model by design) |
| Frontend `features/settings` + `features/agent` restructure | ⏸ **Defer** | Large mechanical churn; revisit as its own refactor if desired |
| `memory_core` hybrid retrieval (jieba FTS + bge-m3 vector + RRF + bge-reranker, V61 index tables, M0/P1 eval harness; `fb456d07`..`cef409b5`, landed after this plan's snapshot) | **Evaluate — T7** | Overlaps this repo's unified two-layer recall (openhuman-A) and in-process fastembed embedder; port only what the eval shows beats current recall |

## Global rules for every task

- One branch + one PR per task; bisectable commits; `## Commits (bisectable)` table in PR body.
- New migrations take the **next free V-number at implementation time** (currently V63+ once
  V62 merges) and update the `CONTEXT.md` registry. uclaw-pi's V-numbers do **not** carry over.
- Run `gitnexus_impact` on every edited symbol; note that GitNexus under-counts callers of
  free functions — confirm with `grep`.
- Port behavior and tests, not uclaw-pi file layout. Adapt to this repo's module shape.
- Known pre-existing failures on `main` (not regressions): `shell::tests::test_daemon_mode_approval_unchanged`,
  `browser::runtime_pack_ipc::tests::print_real_status_diagnostics`, and three
  `browser::provider_execution::tests::*cli_route*` tests.

---

## T1 — Cache-align the system prompt to 1024 tokens (perf) — ✅ ALREADY ON MAIN (`95eeae59`)

**Source:** `2893d256` · **Size:** S · **Risk:** low

**Problem:** `assemble_system_prompt` calls `cache_align::pad_to_ladder`
(`agent/dispatcher/content_assembler.rs:100`), rounding the prompt up to the next
2048/4096/8192/16384/32768 tier. uclaw-pi logs showed thousands of filler tokens per turn.
Anthropic caches at 1024-token granularity; DeepSeek prefix-caches automatically.

**Steps**
1. Swap `pad_to_ladder` → `align_to_1024_tokens` in `assemble_system_prompt`; update the
   canonical-order comments (`:89`, `:1689`).
2. Update the snapshot test that asserts the `<!-- pad: -->` marker.
3. Leave `pad_to_ladder` in place (still tested); retire in a follow-up.

**Done when:** `cargo test --lib content_assembler` and `cache_align` pass; a manual agent
turn shows `system_prompt_tokens` no longer jumping to a ladder tier.

## T2 — Stop injecting the M2-D memory delta annotation (perf) — ✅ ALREADY ON MAIN (`64af65d9`)

**Source:** `08f0480e` · **Size:** S · **Risk:** low

**Problem:** `build_dynamic_block` appends a `<memory_context_changes>` annotation
(`content_assembler.rs:~154/213`) on top of the full memory block, which is already
re-injected every turn (~500–700 redundant tokens/turn).

**Steps**
1. Make the drift match logging-only (`[M2-D]` log kept); always emit the full block;
   remove the annotation append.
2. Keep `line_diff` / `render_delta_annotation` in `context_diff` (still unit-tested).
3. Update the small-drift snapshot test.

**Done when:** content_assembler tests pass; prompt no longer contains `<memory_context_changes>`.

## T3 — Rank the skills manifest by task relevance + semantic boost — ✅ DONE, re-designed (#690, #691)

**Source:** #106 (`e4bc305c`), #111 (`d5cbfe23`) · **Size:** M · **Risk:** medium (affects every prompt)

**Problem:** `build_skills_manifest` (`skills_manifest.rs:43`) orders by recency/usage only,
so relevant skills fall off the capped manifest.

**Steps**
1. Port #106's lexical relevance scoring into `skills_manifest.rs`; thread the current
   user message from the call site in `content_assembler.rs`.
2. Port #111's cosine boost **but replace the memU embedder** with this repo's in-process
   path. Prefer reusing the existing `recall_hybrid` skill ranking that already backs
   `skill_search` (memory Step 1b) over a new embedding query. Missing embeddings ⇒ lexical only.
3. Respect a deadline so manifest build never blocks the turn on a cold embedder.
4. Port the uclaw-pi tests; add one for "no embedder ⇒ lexical ordering unchanged".

**Done when:** a query naming a rarely-used skill's topic surfaces that skill in the manifest;
manifest build p95 stays within the existing budget.

## T4 — ReflectionService + user model (memory growth)

**Source:** `839f7d73`, `67137d0f`, `388ca179`, `9c79c908`, `60d53a54`, `9d702cac`
(uclaw-pi `memory_graph/reflection_service.rs`, ~1150 lines incl. daydream) · **Size:** L · **Risk:** medium

**What it does:** every N (=20) agent turns, an LLM pass distills recent turns into
`reflections`; a promotion pass distills facts + profile nodes into a single `user_model`
summary; recent reflections + the user model are injected into the prompt.

**Relationship to existing work:** this repo's `memory_graph/reflection.rs` (openhuman-G) is
fact extraction/dedup/linking. T4 adds a *narrative* layer above it — complementary, not a
replacement. `user_model` promotion should read openhuman-G's deduped facts.

**Steps**
1. **Migration** (next free V): `reflections`, `user_model`, `user_model_history` tables.
2. Port the pure parts first (parse/should_run/schema/store helpers) with tests.
3. Port `run_once` + `run_promotion`; LLM via `AppState.learning_llm` (exists here,
   `app.rs:278`) and its daily token budget. No LLM configured ⇒ no-op.
4. **Re-home the trigger:** uclaw-pi counted turns in `engine_sink::persist_assistant`
   (pi-only). Here, hook the legacy turn tail next to `infra::service::publish_incoming`,
   with an `AtomicU64` counter.
5. **Re-home injection:** uclaw-pi used `PiPromptContext.{reflections,user_model}`. Here,
   inject via the `load_context` / dynamic-block path in `content_assembler`, under the
   existing memory budget, behind a config flag (default on after soak).
6. Settings: expose the flag + N in the existing memory settings.

**Done when:** after 20 turns a reflection row appears; promotion writes `user_model`; the
next prompt contains both blocks within budget; with `learning_llm` unset nothing runs.
Manual soak per the openhuman slices.

## T5 — Daydream (optional)

**Source:** `b93dce09`, `9384d4fd`, `2f28eecc` · **Size:** M · **Depends on:** T4

Free association over random grounded memory seeds → `daydreams` table → reflows into
reflections; emits `agent:daydream`. Decide after T4 soak whether it earns its token cost.

## T6 — Skills marketplace (multi-provider, workspace install, installed list)

**Source:** uclaw-pi `src-tauri/src/skills_marketplace/` (~43 KB) +
`commands/skills_marketplace.rs` + UI (`views/Kaleidoscope/modules/Skills/SkillMarketplaceDetailDrawer.tsx`,
`shared/tool-rendering/.../skill-marketplace-search-result.tsx`) · **Size:** L · **Risk:** medium

**Today here:** `skills.rs` + `agent/tools/builtin/skill_marketplace.rs` /
`skill_search.rs` — skills.sh search + global install via the agent tool; no UI tab.

**Steps (split into 3 PRs)**
1. **Backend:** port `skills_marketplace/` (`MarketplaceProvider` enum: skills.sh + skillsmp,
   client, github fetch, install service with workspace symlink + activation tag), Tauri
   commands (search/list/detail/audit/install/uninstall/check-update) — register in both
   `tauri_commands.rs` and the `invoke_handler!` in `main.rs`. Reuse the existing V25
   standalone-installs table (pre-fork, present in both). Point the legacy agent tool at the
   new module so there is one install path.
2. **Settings / API keys:** skills.sh + skillsmp key get-status/set (status only, never echo keys).
3. **UI:** market tab (browse/search/install/detail drawer, 已安装 list + uninstall) and the
   in-chat install card ([全局]/[本工作区]). Place it next to the existing 插件市场 for a
   consistent marketplace experience; reuse PluginCard/drawer styling.

**Done when:** search → install (global and workspace) → skill appears in the manifest →
uninstall removes files + tracking row; in-chat card works.

---

## T7 — memory_core hybrid retrieval (evaluate, then port selectively)

**Source:** uclaw-pi `fb456d07`..`cef409b5` (M0 recall harness, P0 prompt-budget fix,
P1 `memory_core`: jieba segmentation + FTS query builder, bge-m3 int8 embedder, bge-reranker,
BM25 + vector + RRF + rerank, incremental index sync from graph + bucket_seal, 1–1.3 s
search budget, CPU-only ORT) · **Size:** L · **Risk:** medium (new model downloads, new tables)

**Today here:** openhuman-A unified two-layer recall + recency/reinforcement ranking
(openhuman-B) + in-process fastembed embedder; no reranker, no Chinese segmentation in FTS.

**Steps**
1. Port the M0 recall eval harness first (`fb456d07`, `21697742`) and run it against this
   repo's current recall to get a baseline on the same golden set.
2. Port the pieces that win on that baseline, likely in this order: jieba FTS query builder
   (cheap, no model), reranker with the busy-skip + bounded budget, then bge-m3 only if it
   beats the current embedder. Land each as its own PR; new tables take the next free V-number.
3. Keep the hard-won fixes: CPU-only ORT (CoreML ballooned to 40 GB+), bounded query size,
   never queue query embedding behind backfill, degrade to BM25 on model errors.

**Done when:** the eval shows equal-or-better recall than the baseline at ≤ the current
latency budget, and recall still degrades gracefully with models missing.

---

## Suggested order

| Order | Task | Why |
|---|---|---|
| ~~1~~ | ~~T1 + T2~~ | Already on main since 2026-06-01 — nothing to port |
| ~~2~~ | ~~T3~~ | Done 2026-11-09 (#690, #691) |
| 3 | T6 | Most visible user-facing gap; independent of memory work |
| 4 | T4 | Largest; needs a soak |
| 5 | T5 | Only if T4 proves valuable |
| 2b | T7 | Eval harness first; can run beside T3 since both touch retrieval |

## Open questions

1. **#98 session titles:** keep the cheap utility-role model (current) or adopt
   quality-first routing from uclaw-pi?
2. ~~**uclaw-pi's future**~~ — **Decided 2026-11-09:** uclaw-new is the mainline. uclaw-pi takes
   no new feature work; it stays runnable as the daily driver until uclaw-new replaces it,
   then gets archived. Anything still landing there gets triaged into this table.
3. **Frontend restructure:** is the `features/` layout worth adopting later as its own refactor?
