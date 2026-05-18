# Memory OS 战略研究综合 — openhuman + gbrain 评估

**日期:** 2026-05-18
**状态:** 研究结论,等用户决策
**输入:** 两个并行 agent 的深度分析(openhuman 代码库 deep dive + gbrain 集成可行性)
**关键问题:** 后续 Phase 8-21 还要不要做?如果要,做什么?如果不要,替代是什么?

---

## TL;DR(一段话)

**决策锁定(2026-05-18):走 Path C — Bundle gbrain as stdio MCP subprocess。**Path A++ Native cherry-pick 作为命名 fallback,只在 Sprint 0 验证发现 hard blocker(Bun 跨平台 compile 完全不工作 / license 不兼容 redistribute / PGLite WASM 在 Tauri context 加载不了)时启动。

**约束:用户零安装** —— App ship 给最终用户时不需要他们装 Bun / gbrain / 任何外部依赖。这把决策空间收紧到 "Bundle" 或 "Native 全自己写"两个选项;"用户自装 gbrain"路线出局。

**Path C 代价已接受:** Installer +~65MB(Bun runtime + gbrain 源 + PGLite WASM);Sprint 0 必须真正跑通 1 周的跨平台打包验证;持续维护负担 ×2(memU Python + gbrain Bun);debug 链路 3 层(Rust + Python + JS)。

**回报:** 3-4 周拿到 gbrain 替代规划中 Phase 8-21 的 ~70% 工作 + retrieval 质量起点高(gbrain battle-tested)+ 集成后 atomic UI 触发 "Dream Cycle" 等操作。

**Phase 1-7 不浪费**(desktop UX + SQLite + memory_health/lint 是 gbrain 没有的,我们独有)。Sprint 1 (openhuman stability_detector + PROFILE.md) 跟 Path C/A 决策正交,独立高 ROI。

---

## Part 1 — openhuman 分析综述(基于真实 deep-dive)

openhuman 在 `/Users/ryanliu/Documents/openhuman/` —— 是 tinyhumans.ai 的 Tauri v2 桌面 AI coworker,Rust+TypeScript monorepo。`openhuman_core` v0.53.49,带两个独立 backfill 二进制(`slack-backfill`、`gmail-backfill-3d`)。**memory 域**层次比我们丰富:`store/`(legacy unified 后端)+ `tree/`(新 bucket-seal LLD 架构)同时存活,迁移中。

### 1.1 那个"20-30 分钟变聪明"的真实机制 —— 是**三个循环堆叠**,不是单一魔法

**关键发现:** 不是 one trick,是三层缠绕的循环:

**Piece A —— OAuth 一次性 bulk backfill(`composio/providers/traits.rs:71-142`)**

用户 OAuth 任何一个连接后,`ComposioProvider::on_connection_created` 默认实现按顺序触发:
1. `fetch_user_profile(ctx)` → identity facets 写到 `user_profile`
2. `identity_set(&profile)` → 多 toolkit 身份对齐(`composio/providers/profile.rs:1-79`)
3. `merge_provider_into_profile_md(...)` → 立即合并进 `PROFILE.md`,managed block 渲染(`composio/providers/profile_md.rs:1-100`)
4. `self.sync(ctx, SyncReason::ConnectionCreated)` → 首次 sync

Gmail 首次 sync 在 `composio/providers/gmail/provider.rs:53-78` 配置激进:`INITIAL_PAGE_SIZE = 50`、`MAX_PAGES_PER_SYNC = 20`、`DEFAULT_DAILY_REQUEST_LIMIT = 500` —— 第一次秒级倾泻最多 500 封邮件进 memory tree。

**Piece B —— 20 分钟周期 tick(`composio/periodic.rs:65`)**

`const TICK_SECONDS: u64 = 1200;` —— 字面意思的 20 分钟。`run_one_tick`(`periodic.rs:150-264`)遍历每个 active connection,基于 `LAST_SYNC_AT` map 避重,过期则触发 `provider.sync(SyncReason::Periodic)`。README `:64` 原话:"every twenty minutes the core walks each active connection."

**Piece C —— 30 分钟 stability rebuild(`learning/scheduler.rs:28`)**

最 sophisticated 的一块。`learning/stability_detector.rs:1-94` 实现公式:
```
stability = base × cue_mult × user_state_mult
base = Σ(cue_family.weight × exp(-Δt / half_life(class)) × ln(1 + evidence_count))
```
三个阈值:`TAU_PROMOTE = 1.5`、`TAU_PROVISIONAL = 0.7`、`TAU_EVICT = 0.4`(`stability_detector.rs:46-50`)。

六类 FacetClass + class budgets:`BUDGET_STYLE=4`、`BUDGET_IDENTITY=4`、`BUDGET_TOOLING=5`、`BUDGET_VETO=3`、`BUDGET_GOAL=3`、`BUDGET_CHANNEL=1`(`stability_detector.rs:75-82`)。

半衰期:Channel=7d,Style=14d,Tooling=30d,Veto=30d,Goal=60d,Identity=90d(`stability_detector.rs:54-59`)。

Candidate 侧(`learning/candidate.rs:24-83`)定义 cue family 权重:Explicit=1.0、Structural=0.9、Behavioral=0.7、Recurrence=0.6。Evidence 带 typed pointer 回 source(`candidate.rs:92-120`:Episodic / SourceSummary / TreeTopic / DocumentChunk / EmailMessage / Provider 等)—— 每个 promoted fact 都可追溯。

Scheduling:每 30 分钟跑一次,加上 `DocumentCanonicalized` / `TreeSummarizerPropagated` event 触发的 60 秒 debounced rebuild(`scheduler.rs:32-130`)。

**Piece D —— 注入入口**

Active facets 通过两条路径喂进 LLM:
1. `learning/prompt_sections.rs:71-98` 的 `UserProfileSection` 渲染成 `## User Profile (Learned)` 块
2. `agent/prompts/mod.rs:466-511` 的 `UserFilesSection` 把 `PROFILE.md` 整体注入,带 `USER_FILE_MAX_CHARS` cap(~1000 tokens)

`PROFILE.md` 在 workspace 里既被 OAuth handoff(`profile_md.rs`)写,又被 `learning/profile_md_renderer.rs` 从 facet cache 重新渲染 managed block。

**verdict:** "20-30 min smart" 是真的,但**不是单一开关**:
- A 在秒级把 100-500 个 chunk 倾入 tree
- B 在 20 分钟时第一次定期 sync 跑完
- C 在 30 分钟时第一次 stability rebuild 把累积证据 promote 成 active facets
- D 让 active facets 立即进入下一轮 prompt

**这意味着对我们而言:Piece A 依赖 Composio 基础设施(我们没有),Piece C+D 是 portable 核心。**

### 1.2 我们没有 / openhuman 有的其它重要模式

完整列表(从 agent 报告):

1. **Composio periodic + initial backfill** —— `composio/periodic.rs`(1200s tick)+ `composio/providers/{gmail,slack,notion,github}/`(per-provider sync)。**我们没有任何 OAuth 集成。**
2. **Stability detector + class budgets + half-life decay** —— 上面 Piece C 详述。
3. **PROFILE.md / MEMORY.md prompt injection** —— `agent/prompts/mod.rs:466-511`,managed-block 协议保留用户手编辑内容。
4. **Per-source bucket-seal summary trees** —— `memory/tree/tree_source/`,`INPUT_TOKEN_BUDGET=50000`、`SUMMARY_FANOUT=10`(`tree_source/types.rs:188-205`)。给每个 channel/inbox 做滚动摘要 —— **我们完全没有这层**。
5. **Per-topic trees materialized by hotness** —— `memory/tree/tree_topic/`,惰性 spawn 在 `TOPIC_CREATION_THRESHOLD` 跨越时。
6. **Daily global digest tree** —— `memory/tree/tree_global/` + `DigestDaily` job。
7. **Write-time score signals + cheap-only short-circuit** —— `memory/tree/score/signals/`:`token_count`、`unique_words`、`metadata_weight`、`source_weight`、`interaction`(权重 3.0,最强)、`entity_density`、`llm_importance`。`signals/ops.rs:76-93` 在 cheap signals 已经 firmly inside/outside 入场带时**跳过 LLM importance 调用** —— 直接照搬就是成本优化。
8. **TokenJuice 压缩层** —— `src/openhuman/tokenjuice/`,HTML→MD/URL 缩短/ASCII 标准化,README 称 up to 80% token reduction。
9. **Job-queue worker pool with `JobOutcome::Defer`** —— `memory/tree/jobs/worker.rs`,5 个 job kind(ExtractChunk / Seal / TopicRoute / DigestDaily / FlushStale)。**我们的 ProactiveService scenarios 是 in-memory 调度,重启就丢失;这是持久化 SQLite-backed job queue**。
10. **Obsidian vault bootstrap** —— `.obsidian/graph.json` 自动写入(`content_store/obsidian.rs:1-60`),用户的 vault 在 graph view 里能看到 level-coloured 视觉化。
11. **Subconscious + heartbeat engines** —— `subconscious/engine.rs`、`heartbeat/engine.rs` 背景 reflection 循环。
12. **Screen intelligence** —— `screen_intelligence/` capture worker + vision。
13. **Standalone backfill binaries** —— `slack-backfill`、`gmail-backfill-3d` CLI 工具(`Cargo.toml:13-18`)。
14. **Identity matching across toolkits** —— `composio/providers/profile.rs`,IdentityKind + confidence threshold 决定 `is_self` 自动晋升。

### 1.3 我们有 / openhuman 没有的(诚实清单)

1. **EntityPage 作为一等公民 versioned record** —— openhuman 实体是 `mem_tree_entity_index` 倒排索引行,不是带 `compiled_truth`/`timeline`/`contradictions`/`enrichment_tier` 的 page
2. **Typed-edge graph with MemoryRelationKind enum** —— openhuman 在 tree 路径无真正 typed edge graph(legacy `store/unified/graph.rs` 只有松散 triples)
3. **`[[entity:slug]]` auto-link 抽取器** —— openhuman 无等价物;它们靠 extract-time NER 绑定实体,不是 wiki-link syntax
4. **`MemoryVersion` versioned content** —— openhuman 摘要 post-seal 不可变
5. **RRF-based hybrid recall with explicit boost factors** —— openhuman 用固定权重 `GRAPH_WEIGHT=0.55, VECTOR_WEIGHT=0.30, KEYWORD_WEIGHT=0.15, EPISODIC_WEIGHT=0.20` 加权混合,不是 RRF
6. **`memory_health`(零 LLM SQL 检查)+ `memory_lint`(成本守卫 LLM 检查)** —— **openhuman 完全没有等价物**。我们这块比他们成熟
7. **Tier escalator(backlink → tier 1/2/3 + 日上限)** —— openhuman 有 hotness 触发 topic spawn,但没有绑定 enrichment 深度的 tier 阶梯
8. **`MemoryGraphStore` with `Arc<Mutex<Connection>>` and create_version auto-link 钩子** —— openhuman 通过 job handler 写,没有单一 store API

### 1.4 真正的 Top 5 移植目标(按 ROI 排序)

| # | 模式 | 工作量 | 风险 |
|---|---|---|---|
| **1** | **Stability detector + candidate buffer**(`learning/{stability_detector,candidate,cache,scheduler}.rs` + `user_profile_facets` 表 schema)—— 把我们 `compiled_truth` 从 "synthesizer 最后写的" 升级成 "stable-over-time、evidence-weighted、budgeted"。落在新 `src-tauri/src/learning/` 模块 + V38 migration | **~6 commit** | **中** —— 他们的 FacetClass taxonomy 假设了 producer set(`extract/heuristics.rs`、`extract/signature.rs`)就位,只移植 detector 不写 producer 是 dead code。`episodic_log` 表是 `EvidenceRef::Episodic` 的硬依赖,需要映射到我们 `agent_messages` + `agent_turns` |
| **2** | **Per-source bucket-seal summary trees**(`memory/tree/tree_source/`)—— 给每个 channel/inbox/document 一棵滚动摘要树。新 `subkind="source_summary"` 节点。落在新 `src-tauri/src/memory_graph/tree_source/` + V38/V39 migration | **~10 commit** | **中-高** —— 与我们 EntityPage 模型并存(不冲突,但开发者必须内化"两种内容模型"的概念分工) |
| **3** | **Write-time multi-signal score + cheap-only short-circuit**(`memory/tree/score/signals/`)—— 在 `MemoryGraphStore::create_version` 后挂钩,给每个 version 打 token_count / unique_words / metadata_weight / source_weight / interaction / entity_density / llm_importance 7 个分,cheap 信号已能定夺时跳过 LLM 调用 | **~4 commit** | **低** —— 纯加法,无语义冲突。`interaction` 信号需要 uclaw-specific source |
| **4** | **PROFILE.md managed-block synthesizer**(`composio/providers/profile_md.rs` + `learning/profile_md_renderer.rs` + `agent/prompts/mod.rs:466-511`)—— managed-block 解析器 + 从 EntityPage(subkind='user')渲染 + system prompt 注入,带 `USER_FILE_MAX_CHARS` cap | **~3 commit** | **低-中** —— "user" EntityPage 约定要严格;KV-cache-stable contract("frozen for the remainder of that session")必须遵守 |
| **5** | **`JobOutcome::Defer` 队列 + 5-kind worker pool**(`memory/tree/jobs/{worker,scheduler,store,types,handlers/mod}.rs`)—— 真正 persistent SQLite-backed job queue,survives 重启。可以替代或增强我们 4 个 in-memory proactive scenarios | **~5 commit** | **中** —— 跟我们 `ServiceManager` + proactive scenarios 框架架构上有重叠。半截留两个并行 job 系统更糟,只在打算做正经 refactor 时启动 |

**Composio 集成不在 Top 5:** OAuth handoff bulk backfill(Piece A)是最 user-visible 的特性,但 agent 不推荐作为 porting target —— 它绑定 Composio tenant + tinyhumans.ai 后端基础设施,我们 roadmap 已有 `mcp.rs` 集成路径。可移植的抽象是 `ComposioProvider` trait 的 `on_connection_created + sync` 生命周期 —— 但底层 tenant infra 不是 Rust-code lift。

### 1.5 移植的非平凡性诚实声明

**最难的 porting blocker(agent 原话):** openhuman 在 tree 路径把实体当**倒排索引行**(mention_count、hotness、无一等 page),我们把实体当**一等 versioned page**(typed edges + `[[entity:slug]]` auto-link)。移植 #2 bucket-seal tree 意味着承诺第二种平行内容模型(`subkind='source_summary'` 节点 ≠ 实体)—— 两种模型都没有完全胜过对方;对一种模型工作得好的 agent 可能误引另一种。**Mitigation:** 把我们 EntityPage 当 canonical 写入目标,把 openhuman 的 tree_source 只当 derived rollup,绝不当 primary store。

---

## Part 2 — gbrain 集成可行性综述

### 2.1 gbrain 是什么(实证)

**真实可运行**,不是参考设计。它有两种形态:

- `gbrain serve` —— 本地 **stdio MCP server**(canonical 模式,跟 Claude Desktop / Cursor 等用)
- `gbrain serve --http` —— HTTP MCP server,带 OAuth 2.1 + admin dashboard(v0.26.0+)

**Runtime:** Bun + TypeScript(硬依赖 Bun,Node 不行)
**Storage:** `PGLite`(WASM Postgres)默认 / `Postgres`(Supabase 测试过)可选
**Source of truth:** 用户的 markdown brain repo(git tracked),PGLite 只是索引
**Inference:** "Thin harness, fat skills" —— 34 个 skill(markdown + 嵌入式 TS helper),LLM 在 host agent(Claude Code/Cursor)里跑,gbrain 自己做 deterministic 工作 + 必要时升级到 LLM 合成
**API:** ~30 个 MCP tool(get_page / put_page / search / query / add_link / traverse_graph / sync_brain / file_upload / 等),全部 stdio JSON-RPC 2.0
**License:** **MIT(很可能,未直接验证)** —— gbrain-evals 子仓库确认 MIT;主仓库 LICENSE 文件 fetch 失败

### 2.2 跟我们的对照(inverted twins)

|  | uClaw memory_graph | gbrain |
|---|---|---|
| Canonical store | SQLite | Markdown files in git repo |
| Mirror | `~/Documents/workground/brain/<subkind>/<slug>.md`(Phase 7,opt-in) | PGLite/Postgres(always) |
| Direction | DB → MD(opt-in FS watcher) | MD → DB → MD round-trip |
| Edge model | typed edges in `memory_edges` | typed predicates(works_at, founded, attended 等)by regex |
| Search | FTS5 trigram + memory_fts + RRF(Phase 5) | tsvector + pgvector + RRF + backlink boost |
| Process | in-process Rust + Tauri | external Bun stdio daemon |

**我们和 gbrain 是镜像孪生:同样的组件,反向的 source-of-truth。**

### 2.3 gbrain 已经实现 / 我们 Phase 8-21 规划的

| 我们 Phase 计划做的 | gbrain 状态 |
|---|---|
| Phase 8 知识对象 taxonomy(entity/concept/comparison/...) | ✅ 已有(`people/companies/concepts/meetings/voice-notes` MECE 目录 + RESOLVER.md) |
| Phase 9 页级 provenance | 部分(citations in compiled-truth,but not segment-level inferredParagraphs) |
| Phase 10 two-stage compile pipeline(extract → compile) | ✅ 已有(整个 dream cycle 跑这个) |
| Phase 11 SHA-256 incremental compile | ✅ 已有(`compiled-truth` doc 明确这个 pattern) |
| Phase 12 control plane files(hot/purpose/log.md) | 部分(`RESOLVER.md` + `inbox/`) |
| Phase 13 review queue(brake) | ⚠️ 不确定(未在 docs 中明确确认) |
| Phase 14 adaptive RAG classifier | ✅ 已有(multi-query expansion + hybrid search) |
| Phase 15 entity graph engine | ✅ 已有(typed predicate 边 + traverse_graph MCP tool) |
| Phase 16 timeline engine | ⚠️ 部分(timeline pattern 在 compiled-truth 下方,but no temporal query DSL) |
| Phase 17 dream cycle | ✅ 已有(整个 nightly consolidation pass) |
| Phase 18-21 misc engines | ⚠️ 部分 |

**估算:** gbrain 涵盖 Phase 8-21 计划工作的 **~70%**。

### 2.4 集成路径估算(memU bridge 模式克隆)

| 步骤 | 估算 |
|---|---|
| 打包 Bun 二进制 + gbrain 源码进 `src-tauri/gbrain_bundle/`(类比 `pyembed/`),Stage 3 service 注册 | 2 commit |
| Rust 端 gbrain client(参考 `memu/client.rs` + `memu/bridge.rs`),stdio JSON-RPC,30 个 tool stub + 错误处理 + 测试 | 4-6 commit |
| **数据二元性(最难)**:gbrain 当主 / memory_graph 当 UI 缓存,或 memory_graph 当主 / gbrain 当合成 worker | 10-15 commit + 一个新失败模式(两侧 drift) |
| 现有数据 migration:`memory_nodes` → markdown → `gbrain import`,需要 LLM 辅助补全 compiled-truth/timeline 结构 | 3-5 commit + 一次 LLM pass |

**总估算:** 19-28 commit + 持续维护一个 JS runtime 在 Rust/Python 桌面 app 里 + 安装包 +50MB

### 2.5 三条路径

- **Path A — 按计划继续 Phase 8-21**(全自己写)
  - 优点:无外部依赖,Rust-native,体积最小
  - 缺点:3-4 个月工程量,retrieval 质量在 catch-up 上跑得比 gbrain 慢
- **Path B — 全 gbrain 替换**(扔掉 memory_graph,gbrain 当唯一后端)
  - 优点:架构纯粹
  - 缺点:扔掉我们的 Tauri IPC、approval flow、SQLite UI —— 太激进
- **Path C — 混合**(Phase 1-7 当本地优先基础,gbrain 当 compile/dream worker)
  - 优点:Phase 1-7 不浪费,~20 commit 拿到 Phase 8-21 的 ~70% 功能
  - 缺点:数据二元性增加 ~1 个失败模式,需要确认 Bun 跨平台打包能成

**Agent 推荐 Path C。**

---

## Part 3 — 综合战略建议

### 3.1 兼容性分析

| 维度 | 兼容性 | 说明 |
|---|---|---|
| 数据格式 | 🟢 高 | Phase 7 我们已经输出 `<subkind>/<slug>.md` + YAML frontmatter,gbrain 输入格式几乎是同一个。`compiled-truth ---  timeline` 分隔约定可以加进 Phase 7 frontmatter spec(目前我们的 body 没分上下) |
| API 协议 | 🟢 高 | stdio JSON-RPC 2.0,跟我们已有的 memU bridge 同款 |
| 进程模型 | 🟡 中 | Bun 子进程跟 Python 子进程类似,但需要验证 Tauri 打包 |
| Schema | 🟡 中 | gbrain PGLite + 我们 SQLite 并存,双写或单写都需要清晰边界 |
| License | 🔴 待验证 | MIT 高概率但未直接确认 |
| 平台支持 | 🟡 中 | macOS arm64 + Linux 已知工作,Windows + PGLite WASM 在 Tauri 下未测 |

### 3.2 工作量对比

| 方案 | Commits | 工程时间 | 维护成本 |
|---|---|---|---|
| **Path A(自己写 Phase 8-21)** | ~80-100 commits | 12-16 周 | 低(纯 Rust,我们的栈) |
| **Path B(全 gbrain 替换)** | ~40-50 commits | 6-8 周 | 中(放弃我们的 IPC 等,需要 wrapper 层) |
| **Path C(混合,推荐)** | ~25-30 commits | 4-6 周 | 中(Bun 子进程 + 数据同步边界) |
| **Path C + openhuman warm-start** | ~30-38 commits | 5-7 周 | 中 |

### 3.3 ROI 矩阵

| 选项 | 实施成本 | 用户感知价值 | 风险 |
|---|---|---|---|
| 继续 Phase 8-14 | 高(12 周) | 中(我们写的最终质量难超 gbrain) | 高(catch-up race) |
| 集成 gbrain | 中(5 周) | 高(立刻拿到 dream cycle + compile + skill resolver) | 中(Bun bundling + 数据二元性) |
| 移植 openhuman warm-start | **低(1 周,5-8 commit)** | **很高**(这是"agent 突然变聪明"的核心机制) | 低(独立于其他决策) |
| 移植 openhuman event_log | 低(3 commit + migration) | 中-高(decision/commitment 查询能力质变) | 低 |
| 移植 openhuman half-life decay | 极低(2 commit) | 中(让旧 compiled_truth 自然褪色) | 极低 |
| 全 gbrain 替换 | 高(6-8 周 + 扔掉已有功能) | 高 | 高 |

### 3.4 集成路径建议

**推荐分阶段执行,每个 Sprint 都有独立交付价值,失败一个 Sprint 不阻塞后面:**

#### Sprint 0 — 兼容性验证(1 周,无 commit 但有决策点)

**目标:** 在做任何 gbrain 集成承诺前,把硬约束验证清楚。

- [ ] 跑 `gbrain init` + `gbrain serve` 本地,验证它真的能跑
- [ ] 检查 gbrain 仓库 LICENSE 文件,确认 MIT
- [ ] 试在 macOS 上把 Bun + gbrain 打包成单一可执行(类比我们 `scripts/setup-python-env.sh`)。验证 PGLite WASM 在 Tauri context 下能加载
- [ ] 把当前 memory_graph 里几个 EntityPage 导出到 markdown → `gbrain import` 看能否往返
- [ ] **决策点:** Sprint 0 全绿才进 Sprint 2;有红点则降级到 Sprint 1 only

#### Sprint 1 — openhuman stability detector + PROFILE.md 移植(2 周,9-11 commit)

**目标:** 独立于 gbrain 决策的最高 ROI 单项。Pieces C+D —— 真正可移植的部分(Piece A OAuth backfill 依赖 Composio 我们没有;Piece B 周期 tick 我们已有 ProactiveService scenarios 等价物)。

新增模块(对应真实 openhuman 文件):

1. **`src-tauri/src/learning/candidate.rs`**(对照 openhuman `learning/candidate.rs:24-120`)—— 6 类 FacetClass(Style/Identity/Tooling/Veto/Goal/Channel)+ 4 类 cue family(Explicit/Structural/Behavioral/Recurrence)+ EvidenceRef typed pointer。
2. **`src-tauri/src/learning/stability_detector.rs`**(对照 `learning/stability_detector.rs:1-94`)—— stability 公式 + 三阈值(promote/provisional/evict)+ 6 类 budget + per-class half-life。
3. **`src-tauri/src/learning/scheduler.rs`**(对照 `learning/scheduler.rs:28`)—— 每 30 分钟 rebuild,加 event-driven 60s debounce。
4. **V38 migration** —— `user_profile_facets` 表(对照 `memory/store/unified/profile.rs:25-47`):`(facet_id, class, name, value, state, stability, cue_families_json, evidence_count, last_seen_at, ...)`。
5. **`src-tauri/src/learning/cache.rs`**(对照 `learning/cache.rs:14-22`)—— FacetCache typed handle。
6. **`src-tauri/src/learning/prompt_section.rs`**(对照 `learning/prompt_sections.rs:71-98`)—— `UserProfileSection` 渲染 active facets。
7. **`src-tauri/src/memory_graph/profile_md.rs`**(对照 openhuman `composio/providers/profile_md.rs` + `learning/profile_md_renderer.rs`)—— managed-block 解析 + 渲染。
8. **System prompt 集成** —— 在 agent prompt builder 里挂 `UserProfileSection` + 读 `PROFILE.md`(带 `USER_FILE_MAX_CHARS` cap)。
9. **Candidate producer #1** —— 从 chat turn 用 regex/简单 LLM 提取 candidates(对照 openhuman 的 `extract/heuristics.rs`,我们没有这个所以要新写)。这是 **最大的未知工作量** —— 没有这个就只是 dead code。
10. **Sprint 1 summary doc**。

期望体感:**用户跟 agent 聊几次后,system prompt 里自动出现"User Profile (Learned)" 块,列出最近 evidence-weighted 最稳定的 4 个 identity facet + 5 个 tooling facet + 4 个 style facet。每 30 分钟更新。配合 PROFILE.md 持久化在 workspace。**

**坦诚的工作量上调:** 之前我估 5-8 commit,但 agent 报告强调 producer 是隐性大块 —— 没有 producer 整个 stability detector 是空架子。修正估算 **9-11 commit + 1 migration**。

**坦诚的局限:** 没有 Composio 也就没有 OAuth 一次性 100-500 chunk 倾入。我们的"20-30 min smart"体感会弱于 openhuman —— 因为只有 C+D,没有 A 的初始倾泻。Producer 只能从用户跟 agent 的真实对话累积,需要时间。这是 honest expectations 设置。

#### Sprint 2 — gbrain 集成(如果 Sprint 0 全绿,3-4 周,~20 commit)

**目标:** 用 gbrain 替代规划中的 Phase 8-21 ~70% 工作。

7.0a — Bun + gbrain bundle script(类比 `setup-python-env.sh`)
7.0b — `gbrain/bridge.rs`(类比 `memu/bridge.rs`)
7.1 — `gbrain/client.rs` + 30 个 MCP tool stub(分 3-4 commit 写)
7.2 — `MemoryOsRuntimeConfig.gbrain_enabled` flag,AppState.gbrain_client
7.3 — 数据二元性策略实现:**memory_graph 当主,gbrain 当 compile/dream worker**
   - `put_page` 调用:`memory_graph → markdown export → gbrain import`(我们 Phase 7 已经能做前半段)
   - `compile_now` 调用:在 gbrain 里跑 dream cycle 子集 → 输出 rewritten markdown
   - `sync_back`:`gbrain markdown → memory_wiki_sync_from_disk`(我们 Phase 7.2 已有)
7.4 — UI 加 "Dream Cycle" 按钮(类比 Phase 6.3 的 Synthesize),触发 gbrain 跑后台 consolidation
7.5 — 评估:用 `gbrain eval --qrels` 跑 retrieval benchmark,跟我们 Phase 5 的 RRF 对比

**保留我们的:** Phase 1-7 全部、`memory_health` SQL checks、`memory_lint` LLM checks(它们是我们独有优势)、cost_records 守卫。

#### Sprint 3 — openhuman 剩余 Top 5 移植(3-5 周,~22 commit,可拆 sub-sprint)

按真实 Top 5 排序,可独立交付:

- **#2 Per-source bucket-seal summary trees**(~10 commit)—— 给每个 channel/inbox/document 滚动摘要。新 subkind="source_summary" 节点。最大块,但开启给我们 channel-level memory(我们目前只有 entity-level)
- **#3 Write-time multi-signal score with cheap-only short-circuit**(~4 commit)—— 在 create_version 后挂分数,cheap signals 已 firm 时跳 LLM 调用,**显著降低 Phase 5/6 LLM 成本**
- **#4 PROFILE.md managed-block synthesizer**(~3 commit)—— **如果 Sprint 1 已经做了,这部分包含在内**;否则单独做
- **#5 JobOutcome::Defer queue**(~5 commit)—— 真正持久化 job queue,替代或增强我们 in-memory ProactiveService scenarios。**仅在打算 refactor proactive 框架时启动**,否则会留下双 job 系统

**注意:** Sprint 3 不是 must-do,可以做完 Sprint 1 + 2 后基于实际体验决定哪几个继续做。

### 3.5 对现有架构的影响评估

**保留(no impact):**
- Phase 1-7 全部代码、IPC、数据 schema
- Tauri 集成、approval flow、cost_records、safety_manager
- memory_health / memory_lint scenarios

**新增(additive):**
- `src-tauri/src/gbrain/` 模块(bridge + client + types)
- `src-tauri/gbrain_bundle/`(Bun + gbrain 源码,类比 pyembed)
- `src-tauri/src/profile_md.rs`(openhuman warm-start 移植)
- AppState 新字段:`gbrain_client: Option<GbrainClient>`、`profile_md: ProfileMd`
- 安装包体积:+50MB(Bun)

**修改(touch but compatible):**
- `RealEntitySynthesizer` 扩展输出 facets(向后兼容,旧调用者忽略 facets 字段)
- `recall.rs` 加 adaptive routing(opt-in,默认行为不变)
- WikiView 加 "Dream Cycle" 按钮(addition)

**风险点:**
- 双 store(SQLite + PGLite)drift:通过 "memory_graph 主、gbrain 写 worker" 的方向性约束降低
- Bun 子进程崩溃恢复:复用 memU bridge 的 supervisor 模式
- 用户已有数据 migration 一次性 LLM pass 成本

### 3.6 最优实施策略建议

**最优路径(按优先级):**

1. **立刻做 Sprint 1**(openhuman warm-start)—— 独立、低风险、高 ROI、不依赖任何外部决策
2. **并行做 Sprint 0**(gbrain 兼容性验证)—— 1 周内决策点清晰
3. **基于 Sprint 0 结果决定 Sprint 2:**
   - Sprint 0 全绿 → Sprint 2(gbrain 集成),Phase 8-21 大部分作废
   - Sprint 0 有红 → 保留 Phase 8-21 计划但用 openhuman/gbrain 的设计作 reference,自己写实现
4. **Sprint 3**(openhuman 剩余模式)—— 不论 Sprint 2 走哪条路都要做

**最大的不要做:**
- 不要在没验证 Bun 打包前就承诺 gbrain 集成
- 不要并行实现 Phase 8-14 + gbrain 集成,二选一
- 不要扔掉 Phase 1-7 走 Path B,沉没成本以外这些代码本身有独立价值

### 3.7 决策需要回答的硬问题(给 Ryan)

1. **OK 接受安装包 +50MB Bun 吗?** uClaw 当前安装包约多大?这个增量在用户接受度上是 deal-breaker 吗?
2. **OK 接受 markdown 作为 canonical source-of-truth 吗?** Phase 7 我们走的是 SQLite 主、markdown 副。gbrain 是反过来。这个反转需要明确表态
3. **能否接受 ~5-7 周延迟换 ~10-12 周的工作量节省?** 这是 ROI 的核心交换
4. **是否需要 Windows 支持?** gbrain + Bun 在 Tauri Windows 下的稳定性未测试 —— 如果 Windows 是 must-have,Path A 风险较小

### 3.8 我对 Phase 8-21 是否"无效劳动"的诚实判断

**部分是,部分不是:**

| Phase | 是否被 gbrain 替代 | 备注 |
|---|---|---|
| Phase 8 知识对象 taxonomy | ✅ 几乎全部 | gbrain MECE 目录 + RESOLVER.md 等价 |
| Phase 9 页级 provenance | 🟡 部分 | gbrain 有 citations,但 inferredParagraphs 粒度更细 |
| Phase 10 two-stage compile | ✅ 全部 | gbrain 整个 dream cycle |
| Phase 11 SHA-256 incremental | ✅ 全部 | gbrain compiled-truth doc 明说这个 pattern |
| Phase 12 control plane files | 🟡 部分 | gbrain 有 inbox + RESOLVER,我们的 hot/purpose/log 概念稍不同 |
| Phase 13 review queue | ❌ 没替代 | 这是我们独特设计,gbrain 没有 |
| Phase 14 adaptive RAG | 🟡 部分 | gbrain 有 multi-query expansion,我们的 classifier 更细 |
| Phase 15-21 Engines 层 | 🟡 大部分 | dream cycle / entity graph / timeline 多数被替代,部分细节(如温度系数控制)我们更细 |

**最坦诚的结论:** 如果 Path C 走通,我们值得自己 dedicate 工程时间继续做的,只剩 **Phase 13 review queue**(独特安全网)+ **Phase 9 inferredParagraphs**(细粒度 provenance)+ **openhuman 风格的 personal facets + PROFILE.md**(Sprint 1)。

其它 ~80% 的 Phase 8-21 计划应当被 gbrain 集成替代。

---

## 附录 A — Agent 报告原文存档

完整两份 agent 报告核心结论已综合到上面 Part 1 / Part 2。Sandbox 中可继续追问:
- openhuman agent ID: `a3a0310e72a0ff1f4`
- gbrain agent ID: `ab29b3a0a1dd5705d`

## 附录 B — 诚实声明 / 文档版本说明

本文档第一版的 openhuman 部分曾包含未经过实际 deep-dive 的推断内容(具体 Top 5 排序和 Sprint 1 sub-tasks)。修订后基于 `/Users/ryanliu/Documents/openhuman/` 的真实 grep + Read 输出重写。主要修正:

- 真实"20-30 min smart"机制是 **三循环堆叠**(OAuth backfill + 20 min sync tick + 30 min stability rebuild + PROFILE.md 注入),不是单一 `on_connection_created` warm-start hook
- 真实 Top 5 ROI 排序:**stability_detector + candidate buffer** > per-source bucket-seal trees > write-time score signals > PROFILE.md > job queue。之前未提及的 per-source tree(`tree_source/`)是 openhuman 重要的内容模型组成,我们完全没有等价物
- Sprint 1 真实工作量 **9-11 commit + 1 migration**(原估 5-8 commit 漏算了 candidate producer)
- 我们没有 Composio = 不能复制 OAuth 一次性 bulk backfill 那部分体感;只能复制后两个循环

## 附录 C — Path A++ Native fallback 规格(只在 Path C Sprint 0 失败时启动)

Sprint 0 三个 hard blocker(任一触发则 fallback):
1. `bun build --compile` 在 macOS arm64 或 x86_64 或 Linux x86_64 任一平台无法产出可运行的 standalone 二进制
2. gbrain MIT license transitive 依赖发现 GPL/AGPL,无法 redistribute
3. PGLite WASM 在 spawned-from-Tauri subprocess 的 Bun runtime 下无法加载(基本意味着集成方案彻底破产)

Fallback Path A++ 规格(~49 commit,6-8 周):

| Phase | 灵感来源 | 实施 | 估算 |
|---|---|---|---|
| Phase 8 Knowledge Object Taxonomy | gbrain MECE 目录 + RESOLVER.md | EntityPage.subkind 升级为 typed enum + resolver,SQL gate 强类型边 | 6 commit |
| Phase 10 Two-Stage LLM Compile | gbrain compile-truth 模式 | extract → compile 两步,SHA 增量(Phase 7 已有 SHA 短路);新 `compile_now` IPC | 10 commit |
| Phase 17 Dream Cycle scenario | gbrain dream cycle + openhuman tree_source | 类比 memory_health/memory_lint 的 ProactiveService scenario,每晚跑;merge timeline、refresh stale、broken-link repair | 8 commit |
| Sprint 1: openhuman stability_detector + PROFILE.md | openhuman learning/ | 跟主路径同 11 commit |
| openhuman score signals + cheap-only short-circuit | openhuman memory/tree/score/ | create_version 后挂分,cheap 信号 firm 时跳 LLM | 4 commit |
| openhuman per-source bucket-seal trees(可选) | openhuman memory/tree/tree_source/ | 新 subkind='source_summary' 节点;给 channel-level memory | 10 commit |

**Skip 的:** Phase 9 inferredParagraphs、Phase 11 SHA-256 incremental(Phase 7 已有)、Phase 12 hot/purpose/log、Phase 13 review queue、Phase 14 adaptive RAG(Phase 5 RRF 够用)、Phase 15-21 Engines 层(被 Phase 10/17 + openhuman 覆盖)

**回报对比:**
- Path C(Bundle):20 commit / 3-4 周 / +65MB / 30-40% Sprint 0 失败率
- Path A++(Native):49 commit / 6-8 周 / 0MB / 零外部依赖

Path A++ 是 "Sprint 0 出红灯,但我们仍然要拿 Cognitive 层" 的最优兜底,**不是 Path C 的劣化版**。Native 路径长期维护更轻,代价是上手多 3-4 周。

## 附录 D — Sprint 0 真实结果 + Embedding 复用调查(2026-05-18)

### D.1 Sprint 0 实证(Mac 端跑过)

| 项 | 结果 | 备注 |
|---|---|---|
| 0.1 License | ✅ MIT clean | 无 transitive GPL/AGPL |
| 0.2 本地跑通 | ✅ `bun src/cli.ts` 工作,63 migrations 过,MCP stdio server tools/list 返回完整工具集 | PGLite 初始化正常 |
| 0.3 Bun `--compile` 单 binary | ⚠️ 106MB binary 产出,但运行时 PGLite `ENOENT: open '/$bunfs/root/pglite.data'` — WASM data 文件没被嵌入 bunfs | **Path C-1(单 binary sidecar)死亡** |
| 0.4 Tauri subprocess | ❌ 因 0.3 失败,单 binary 路径不可行 | 走 Path C-2 替代(见下) |
| 0.5 Round-trip | ✅ compiled_truth + aliases 自动识别;slug/subkind/timeline 有 schema 差异(详见 Sprint 2.7 frontmatter v2 计划) | 4 个差异点全部有处置方案 |

**结论:Path C 仍然活,但分支路径从 C-1 转 C-2。**

| 名字 | 含义 | 状态 |
|---|---|---|
| **Path C-1** | `bun build --compile` 单 binary,Tauri sidecar 分发 | ❌ Dead(PGLite ENOENT) |
| **Path C-2** | Ship Bun runtime + gbrain source tree,Tauri spawn `bun src/cli.ts` | ✅ **采用** |
| **Path A++** | 不集成 gbrain,Rust 重写 | 命名 fallback |

### D.2 Path C-2 bundle 规格

```
src-tauri/resources/
├── bun-darwin-arm64      ~55MB
├── bun-darwin-x64        ~55MB
├── bun-linux-x64         ~55MB
└── gbrain/               ~60MB
    ├── src/cli.ts
    ├── package.json
    ├── node_modules/  (production deps only,含 PGLite WASM)
    └── ...
```

Per-platform installer 增量约 **115-130MB**(比原 C-1 预估 65MB 多一倍但接受)。

### D.3 Schema 差异 4 项处置(Sprint 2.7 Frontmatter v2)

| 差异 | uClaw 现状 | gbrain 期望 | 处置 |
|---|---|---|---|
| slug | `alice` | `person/alice`(path-qualified) | export 时写 path-qualified slug |
| subkind/type | `subkind: person` | `type: person` | 双写两个字段,兼容双方 |
| timeline | YAML array | body 内 `---` 分隔块 | 双格式:YAML 主用 + body 块 generated from YAML |
| embedding | 无 | OpenAI / Ollama / `--no-embed` | gbrain `--no-embed`(见 D.4) |

### D.4 Embedding 复用结论 —— 复用现有 memU FastEmbed,gbrain 跑 `--no-embed`

**现有栈摸底(2026-05-18 grep + Read 实证):**
- **Backend:** `BAAI/bge-small-en-v1.5` via FastEmbed Python,**384 维向量**,本地推理零 LLM cost
- **入口:** `MemUClient::embed_text(&[&str])`(`src-tauri/src/memu/client.rs:253`)
- **Helpers:** `src-tauri/src/memu/embedding.rs`(`embed_skill_body` / `serialize_embedding` / `parse_embedding` / `cosine_sim`)
- **Storage:** `memory_versions.embedding_json` TEXT 列(V1 schema,Phase 1+ 已有)
- **Production:** 在 installer 里(memU + fastembed 通过 `scripts/setup-python-env.sh` ship)

**架构:**

```
┌──────────────────────────────────────────────────────────┐
│                     uClaw (我们)                         │
│  memory_versions.embedding_json ← memU FastEmbed (384)   │
│  Phase 5 recall: FTS5 + cosine + backlink + RRF          │
└────────────────────┬─────────────────────────────────────┘
                     │  markdown sync (Phase 7)
                     ▼
┌──────────────────────────────────────────────────────────┐
│                    gbrain (Path C-2)                     │
│  PGLite + FTS-only (--no-embed)                          │
│  跑 compile / dream / skill resolver                     │
│  写回 markdown → sync 进 memory_nodes                    │
└──────────────────────────────────────────────────────────┘
```

gbrain 跑 `--no-embed`,**它的 PGLite 完全无 vector 索引**。语义检索 100% 在我们这边走 FastEmbed + Phase 5 RRF。这避免:
- gbrain 配置 OpenAI/Ollama 的麻烦
- 维度不匹配(我们 384 vs OpenAI text-embedding-3-small 1536)
- 重复 embed 同一份内容两遍

### D.5 发现的 pre-Sprint-2 bug 🐛

**EntityPage 版本目前漏挂 embedding 钩子。** `embed_skill_body` 只在 learned-skill / gene retrieval / skill_search 路径调用,Phase 6.2 `EntitySynthesizer::persist_synthesis` 漏接。

后果:Phase 5 RRF 的 cosine 通道对 EntityPage 检索现在是空跑。

**Pre-Sprint-2 独立 PR(~2 commit):**
1. `entity_synthesizer.rs::persist_synthesis` 末尾加 embedding hook(类比 `proactive/service.rs:1725`)
2. 一次性 backfill 任务: scan `memory_versions WHERE node_id IN (entity_page nodes) AND embedding_json IS NULL`,批量调 `MemUClient::embed_text` 填上

跟 gbrain 集成解耦,任何时候都能做。建议:Sprint 1 完成后 / Sprint 2 启动前插入。

### D.6 不需要 Ollama / OpenAI embedding 的最终理由

- memU + fastembed 已在 installer 里,产品成本沉没
- bge-small 384 维对我们规模够用(不是 millions of docs)
- Ollama 默认 model ~150MB,违反"用户零安装"前提
- gbrain `--no-embed` 模式 = 我们不靠 gbrain 做检索 = 不需要给它配 embedding
- **唯一引入的合理场景:** 后期决定让 gbrain 也参与检索 + 跟 Phase 5 做 RRF 二次融合。Sprint 2 不做,future work

Path A++ 是 "Sprint 0 出红灯,但我们仍然要拿 Cognitive 层" 的最优兜底,**不是 Path C 的劣化版**。Native 路径长期维护更轻,代价是上手多 3-4 周。
