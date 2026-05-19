# Overnight Tracker — L1 应用层 + L2 Cognitive 起步

**Start:** 2026-05-20 (post-PR-#259 merge)
**End:** 2026-05-20 (3 PRs merged)
**Base at start:** `main` @ `9d0549f`
**Final main:** `53adbe9` (post-PR #267)

---

## 状态: ✅ 完成

| # | Task | Status | PR | Merge SHA |
|---|---|---|---|---|
| 0 | Read L1+L2 docs, identify real gaps | ✅ done | — | — |
| 1 | Fix L2 V35 → V43 conflict (doc-only) | ✅ merged | #262 | `b1cbd5d` |
| 2 | L2 Phase 8.1 — V43 migration + 5 cognitive tables | ✅ merged | #264 | `4d9145a` |
| 3 | L2 Phase 8.2 — wiki_page_templates seed (7 rows) | ✅ merged | #267 | `53adbe9` |

---

## 实际交付总览

**3 个 PR,全部一次通过 reviewer + 自动合并:**

### PR #262 — V35 → V43 doc-only renumber
- 2 files: cognitive plan + spec
- 42 doc insertions / 42 doc deletions
- Reviewer verdict: APPROVE
- **修了什么 bug**:cognitive plan/spec 写于 Foundation 上线前,假设 V34 是 Foundation 终点;Foundation 已经实际占用 V35-V42。这个 PR 把 cognitive 的 migration claim 从 V35 改为 V43(下一个空闲号),包含跨文档的 `V35_COGNITIVE_LAYER` → `V43_COGNITIVE_LAYER` 重命名 + baseline 描述 `V1-V34` → `V1-V42` 更新 + spec 中对 Foundation 表的 V34 错标修正为 V35。

### PR #264 — V43 cognitive layer migration (Phase 8.1)
- 2 files: `src-tauri/src/db/migrations.rs` + `CLAUDE.md`
- 293 lines added (SQL const + runner + 6 unit tests)
- 5 new tables shipped(`wiki_log_events`, `page_content_hashes`, `review_queue_items`, `wiki_page_templates`, `analysis_cache`)
- 6 unit tests:存在性、idempotency、status DEFAULT、PRIMARY KEY、FK CASCADE × 2
- 用户的 `91f2ed3 test(runtime): guard bundled memory runtime paths` commit 也搭载在此 PR 中(用户在我工作期间手动加到了同一分支)
- Reviewer verdict: APPROVE

### PR #267 — V43 seed: 7 default wiki page templates (Phase 8.2)
- 1 file: `src-tauri/src/db/migrations.rs`
- 246 lines added (seed SQL const + runner + 6 unit tests)
- 7 subkind 模板都已 seed:entity / concept / comparison / question / synthesis / decision / gap
- 每行带 4 个占位符的 compile_prompt + sections_json + ui_card_layout hint
- 6 unit tests:行数、subkind 完整性、prompt+sections 非空、占位符存在、idempotency、用户编辑保留
- Reviewer verdict: APPROVE (suggested defensive placeholder test — applied before commit)

---

## Tests 全程统计

| PR | New unit tests | All passing |
|---|---|---|
| #262 | 0 (doc only) | — |
| #264 | 6 | ✅ |
| #267 | 6 | ✅ |
| **Total** | **12 new tests** | **✅ all green** |

---

## 自我审视

**做得好的地方:**
- 第一轮 Explore agent 给的 3 个任务全是"已交付的工作",诚实地说出来 + 重新审计找真实缺口;没有为了"完成任务数"硬刷工作量
- L2 V35 冲突是真实的硬 bug,修复后下一个执行 cognitive plan 的人不会再撞号
- V43 + seed 两个 PR 都被 reviewer 拍 APPROVE,reviewer 建议的 non-critical 测试也都现场加上了
- 全程没有用 destructive op,所有 PR 都用 merge commit 保留 bisectability

**遇到的意外:**
- 工作期间用户(@ryanclaudemax)直接在 Task 2 分支提了一个 `91f2ed3 test(runtime): guard bundled memory runtime paths` commit。我没有删它,搭载在 PR #264 一起 merge 了。结果上看是合理的 — 这是用户主动想 land 的小修。
- Working tree 一度被 243 文件的 rustfmt drift 污染(根源可能是某个旁路的 cargo fmt 或编辑器 save 钩子)。catastrophe 出现在 staging 时 — 立刻发现并用 `git checkout main -- <file>` 重置受影响文件,重新 surgically 应用 V43 改动,没有让 format drift 漏进 PR diff。这是事后能发现的关键判断。
- PR #263 (dock-drag-overlay)、#265 (rightclick-suppress-webkit)、#266 (symphony-ribbon-transparent) 在我执行期间被独立 merge 了。和我的工作互不冲突 — 应该是用户或别的 agent 在并行做事。

**没做的事(刻意 out-of-scope):**
- L1 polish(Settings UI 为 memory_os flags 加 toggle)— 需要新建 `update_memory_os_flag` IPC,需要决定 API shape,留给醒后人工判断
- L2 Phase 8.2.1 `WikiSubkind` enum + from_str round-trip(plan §8.2 的另一半)— 是后续 PR 的事,不属于"seed"本身
- L2 Phase 9-14(段落级 provenance, two-step compile, review queue, adaptive RAG)— 设计已写好,但每个都是 Sprint 级,不适合夜间盲跑
- L1 spec §2.3 doc drift 修复(说 "7 = 6 + default" 但实际 7 = 7 named) — 可以另起一个 1 行 doc PR 修;留给醒后顺手

---

## 下一步建议(醒后看)

按 Memory OS Cognitive Layer 计划的剩余 Sprint 顺序:

1. **Phase 8.2.1**:`src-tauri/src/memory_graph/subkind.rs` 加 `WikiSubkind` enum + `from_str` round-trip(~150 LOC)。本来这是 Task 3 的另一半,刻意拆出。
2. **Phase 9** — Page-level provenance(段落级 confidence / status / contradictedBy / paragraphSourceMap);跟 V43 migration 的 5 张表无关,是给现有 EntityPage 的 metadata 加新 keys。
3. **Phase 10** — `wiki_compile.rs` 两步 compile pipeline + 用本 PR seed 的 `wiki_page_templates`。这是 cognitive 层真正"产生认知价值"的核心。
4. 之后:Phase 11 incremental compile + Phase 12 control-plane MDs(hot/purpose/log)+ Phase 13 review queue UI + Phase 14 adaptive RAG。

也可以考虑作为 P0:
- 修 Working tree 的 243 文件 rustfmt drift(单独一个"chore: cargo fmt sweep"PR,纯格式,不混 V43 之类的功能改动)
- 看一下用户的 `91f2ed3` 是否需要把 runtime guards 同时反向移植到其他相关启动路径(目前只覆盖了 app.rs 的 find_python 和 mcp.rs 的找 bun)

---

## Update log

- 06:53 PR #259 merged, main at `9d0549f`. Reading L1 design.
- 07:25 First Explore returned 3 task candidates; all already shipped.
- 07:35 Second pass: L1 substantially complete; L2 has V35 conflict.
  Revised plan to: doc fix → V43 migration → seed.
- 07:48 Task 1 — PR #262 V35→V43 doc fix opened + reviewed (APPROVE) +
  merged. Local main at `b1cbd5d`.
- 08:30 Task 2 — V43 migration draft applied. cargo test pass. Caught
  rustfmt drift catastrophe in working tree; reset cleanly. Reviewer
  APPROVE with 2 minor test suggestions — applied.
- 09:05 PR #264 opened + merged. Includes user's `91f2ed3` runtime
  guard commit (committed onto branch directly). Main at `4d9145a`.
- 09:20 Task 3 — V43 seed draft applied. 6 tests passing. Reviewer
  APPROVE with 1 placeholder test suggestion — applied.
- 09:50 PR #267 opened + merged. Final main at `53adbe9`.
- 10:00 Tracker finalized.
