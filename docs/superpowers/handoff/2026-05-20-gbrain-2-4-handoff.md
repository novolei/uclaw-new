# gbrain Sprint 2.4 + 2.2.5c + Migration Registry — Hand-off

**Date:** 2026-05-20
**Branch:** `claude/sprint-gbrain-2-4`
**Base:** `main` @ `56a6365` (post-PR-#253 bridge stability fix)
**Companion plan:** `docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md`

This PR closes out the gbrain agent loop plan started in PR #223 by
landing the two remaining commits (Sprint 2.2.5c + Sprint 2.4) plus
a doc backfill that was already overdue.

## What's in this PR

| Commit | Sprint / topic | Status |
|---|---|---|
| `8765da9` | **Sprint 2.2.5c** — embedding endpoint URL health check + `test_embedding_endpoint` IPC + UI "测试连接" button | ✅ Code + 3 unit tests in place |
| `e25a4a2` | **Docs** — CLAUDE.md migration registry backfill (V39 → merged, V40/V41/V42 added) | ✅ doc-only |
| `f0ddec1`* | **Sprint 2.4a** — `crate::gbrain::chat_extractor` new module + `GbrainPageProposal` + Haiku-cheap extractor + parser | ✅ Code + 7 unit tests |
| `dad6842` | **test fixup** — align `gbrain_prompt` assertion with PR #253 vocabulary (recall → query/search) | ✅ 1-line test fix |
| `eac1bf2`* | **Sprint 2.4b** — `ChatDelegate` extractor pipeline + `cost_store::today_gbrain_extract_tokens` + `memubot_config` 2 fields (default ON, 30k budget) + 3 callsite wiring | ✅ Code + 2 unit tests |
| (this commit) | **Docs** — this hand-off + plan checkboxes | — |

\* SHAs accurate at branch tip; will update if rebase happens before merge.

## Why a test fixup commit

PR #253 (memu/gbrain bridge stability, commit `2dc86f3`) renamed
`mcp__gbrain__recall` → `mcp__gbrain__query` / `mcp__gbrain__search`
in `GBRAIN_INSTRUCTIONS` but didn't update the matching unit test.
The assertion `out.contains("recall")` failed against the actual
prompt content. Caught when running `cargo test --lib agent::gbrain_prompt`
during Sprint 2.4b verification. Tiny standalone fix kept off
Sprint 2.4b's diff for bisectability.

## Defaults shipped

- `memory_os.gbrain_extractor_enabled` = **true** (Sprint 2.4b)
- `memory_os.gbrain_extractor_daily_token_budget` = **30,000** (≈$0.05/day with Haiku)

Rationale: Sprint 2.3 (PR #223) post-merge QA validated agent does
fire put_page when explicitly prompted. The auto-extractor is the
safety net for entities the agent missed in its own reasoning,
gated by daily token budget so worst-case spend is bounded.

## Post-merge QA (Mac side)

### Sprint 2.2.5c — embedding URL health check

```
1. Settings → 系统 → Embedding 端点配置
2. Change base_url to http://invalid.local:9999/v1
3. Click 测试连接 → expect red error "cannot connect to … verify host/port"
4. Click 保存 → expect SAME error (probe runs implicitly before persist)
5. Restore base_url to http://localhost:7337/v1 → 测试连接 → green toast
6. Click 保存 → succeeds
```

### Sprint 2.4 — chat extractor

```
1. Confirm Settings shows gbrain init green (PR #223 diagnostics).
2. Start NEW chat (clear context). Say:
     "我们昨天讨论了一个新项目叫 ProjectFalcon — 主要做 AI driven UX 实验。
      负责人是 Alice，目标是 Q3 2026 发布 MVP。"
3. Watch agent's tool calls. EXPECTED behavior (default ON):
   - Agent itself may call put_page (Sprint 2.3 path).
   - Extractor (this PR) also fires asynchronously in the background.
     Look for `[ChatDelegate] gbrain extractor — firing put_page calls`
     in logs.
4. After 1–2 turns, run via gbrain CLI:
     ~/.uclaw/gbrain/run.sh query "project falcon"
   → expect 1+ pages with slug like "project-falcon" or "person-alice"
5. Check cost_records:
     sqlite3 ~/.uclaw/uclaw.db \
       "SELECT model, SUM(input_tokens+output_tokens) FROM cost_records
        WHERE model LIKE 'gbrain_extract%'
          AND created_at >= strftime('%s','now','start of day')*1000
        GROUP BY model"
   → expect 1+ rows with model prefix `gbrain_extract:` and tokens > 0
6. Hit the budget ceiling:
     - Set `memory_os.gbrain_extractor_daily_token_budget = 100` in
       ~/.uclaw/memubot_config.json
     - Restart uClaw
     - Have a long chat → after token count crosses 100, extractor
       should stop firing for the day (look for "budget exhausted"
       debug log)
7. Flip OFF:
     - Set `memory_os.gbrain_extractor_enabled = false`
     - Restart → confirm no `gbrain_extract%` rows appear in cost_records
       on subsequent chat turns
```

### Doc backfill (Sprint registry)

```
1. Open CLAUDE.md, find "Active migration registry"
2. Confirm V39 marked merged (was "in progress")
3. Confirm V40 (mcp_audit) + V41 (browser_task_runs etc) + V42
   (browser_task_checkpoints) present
4. Cross-reference against migrations.rs — next free V-number is V43
```

## What this PR does NOT do

- **No prompt re-tuning.** The instruction block in
  `agent/gbrain_prompt.rs::GBRAIN_INSTRUCTIONS` stays exactly as
  PR #253 left it. If post-merge QA shows the agent misuses
  `query` vs `search` tools, that's a separate follow-up commit.
- **No post-response extraction hook.** The extractor sees only the
  user message at iteration=0 (matching learning_extractor shape).
  Capturing assistant-surfaced knowledge is a future Sprint —
  needs a new `after_iteration` hook with access to
  `ReasoningContext`.
- **No UI toggle for `gbrain_extractor_enabled`.** Today the flag
  lives in `~/.uclaw/memubot_config.json` — users who want to
  disable it edit the file + restart. A Settings UI toggle can
  ship with Sprint 2.5+ alongside any other memory_os tuning UI.

## Estimated PR review surface

- 5 production commits + 1 doc commit
- ~440 lines of Rust + ~50 lines of TS + ~5 lines of CLAUDE.md
- 13 new unit tests, all passing
- No new dependencies (reqwest already in workspace)
- No new migrations
- Backwards-compatible: existing AppState shape unchanged, new
  fields all behind defaulted memubot_config flags
