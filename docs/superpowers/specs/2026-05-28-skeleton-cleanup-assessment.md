# 阶段 2 清骨架 · 评估、策略与执行方案

**Date:** 2026-05-28
**Status:** Assessment (代码级评估;待 user review 后转 writing-plans)
**Audit anchor:** `docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md` §4 阶段 2
**Strategic ADR:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`
**Method:** 4 个并行 subagent 对 main HEAD `d323a4d6`(Slice 1a + 1b + follow-up 全部已合并)做代码级评估,GitNexus index 已刷新
**Author:** claude-opus-4-7 (synthesizer) + 4 triage subagents

---

## 第 0 部分:为什么要清骨架(rationale)

uClaw 当前有 ~4,800 行 dead/skeleton 代码,集中在 6 个模块。这不是中性的"看不见就不管"——它在持续产生负价值:

### 1. 制造"已完成"的错觉
`memory_contract`、`memory_policy`、`plugin_manifest`、`registries` 等模块都有完整的类型定义、文档注释、合理命名,**看起来架构很完整**。新人或未来的我看到 `RegistryHub` 字段在 `AppState` 里、看到 `MemoryPolicyExecutor` 在 `memory_policy::` 下,会自然假设"这套架构已经工作了"。然后:
- 添加新功能时往这些骨架里加代码而不接线
- 文档/ADR 引用这些类型作为既成事实
- milestone 计算包含这些模块作为"已交付"

这是 audit §17 ADR 自列的两条风险("Capability Mesh becomes plugin chaos / skeleton"、"memory splits again")**实际发生**的机制。

### 2. 与新方向直接冲突
ADR `2026-05-28-uclaw-pi-lightweight-product-philosophy.md` 决定收敛到 **一个 `AgentApi` 句柄**(Pi `ExtensionAPI` 模式)。`registries::RegistryHub`(1,777 LoC)正是被这一决策**显式取代**的"4-Registry 分治"路线。每多保留一天,就多一份"哪个是真的"的认知负担:新人会困惑该用 `registry_hub` 还是阶段 3 的 AgentApi。

### 3. 阻塞名字
- `harness/` 当下其实是离线 eval 跑机,**名字本身**阻挡了 ADR §10 "autonomy harness"(长任务监督器)拿到一个清晰的语义槽。
- `memory_policy::MemoryPolicyExecutor` 占用"policy"语义,记忆专项重建时要先想"和这个旧的怎么共存"。

### 4. 维护成本(隐性)
- 编译时间(虽小但累积)
- 重构波及(grep 命中,但无关)
- 工具链噪声(`cargo doc` 生成大量未使用 API 文档)
- 测试运行时间(`memory_policy/tests.rs` 170 LoC 测试一个生产无人调的执行器)

### 5. 不清骨架的代价 ≠ 0
留着这些 4,800 LoC,**未来必然要再次评估**——而每次评估都是相同结论(还是没接线、还是死代码)+ 一次"上次为什么没删"的检索成本。今天清掉是一次性成本,留着是周期性成本。

**反对意见的回应**(用户在审阅时可能想到):
- *"留着作设计探索参考"* → git 历史已永久保留;5 个 commit 的 `memory_policy` 史完整。
- *"以后可能要重新接线"* → ADR 明确说方向变了(AgentApi 单句柄取代 4-Registry;openhuman bucket-seal 取代 memory_contract);旧骨架接线收益为负。
- *"删了显得激进"* → audit 已说"二选一,不留半成品"——这是审计已建议的动作,不是激进发明。

---

## 第 1 部分:清单 · 分组逐项评估

> 命名约定 · KILL = 删除 · WIRE = 接线变活 · SPLIT = 拆分/重命名 · REPURPOSE = 删一部分留另一部分 · DEFER = 暂不动并记录原因

### 1.A 骨架组 · skill 子系统(零风险,425 LoC)

| 项 | 状态 | LoC | verdict |
|---|---|---|---|
| `agent/skill_selection/`(全模块) | 1 个 commit 龄(2026-05-26),0 生产调用方 | 393 | **KILL** |
| `skills.rs::build_skill_prompt` + `combined_system_prompt` | 死 markdown 渲染器,被 `format_for_system_prompt_xml` 取代 | 32 | **KILL** |
| `agent/token_budget/snapshot.rs::l3_skills_selected` / `l3_skills_dropped_for_budget` | stranded 字段,无任何赋值点 | ~5 | **KILL**(顺带) |

**总计 425 LoC,零风险**。Live path 是 `skills_manifest::build_skills_manifest` → `format_for_system_prompt_xml`(`tauri_commands.rs:2060/11257` 调用),完全替代。

### 1.B 骨架组 · 记忆抽象(大头,~1,778 LoC + 适配文件)

> ⚠️ **Correction (2026-05-28, post-P4)**: 本节"KILL memory_policy + memory_contract"的判断**事后证明是错的**。审计时 `5df3ade1` HookBus refactor("shared-bus ready")才 3 天龄,加上 Agent Memory OS v2 程序 A+C+E 已通过 PR #288 (2026-05-20) merge — 这两个模块是**未落地的 B+D 阶段的载重基础**,不是死骨架。P5 因此被取消。详见 [`2026-05-28-skeleton-cleanup-stage2-closeout.md`](2026-05-28-skeleton-cleanup-stage2-closeout.md) §4 的完整冲突分析 + §5 deferred-to-post-B+D 跟踪项。

| 项 | 状态 | LoC | verdict |
|---|---|---|---|
| `memory_contract/`(全 crate) | 1 commit 龄,0 trait impl 非测试 | 660 | **KILL** |
| `memory_policy/`(全 crate,含 `executor.rs`/`classifier.rs`/`receipts.rs`/`targets/`) | `MemoryPolicyExecutor` 0 生产调用方;receipt 类型被 3 个适配文件 import 但那些适配文件本身也是死的 | 1,118 | **KILL** |
| 上层适配文件(`runtime/context_memory_policy.rs`、`harness/adapters/memory_policy.rs`、`browser/runtime_memory_policy.rs`+ 各自测试) | 只服务 `memory_policy` 的死外延 | ~100 | **KILL**(连带) |
| `scripts/git-hooks/checks/check-memory-graph-freeze.sh` 冻结 hook | regex 拦的字面 token 是不存在的调用风格;真正写 API 是 `.create_node()` 等方法,有 34 处 live 写 | hook | **DEFER**(打补丁注释,留给记忆专项) |

**子结论:** ~1,778 LoC 死代码 + ~100 LoC 适配文件可清。`memory_graph` 冻结 hook **不能现在硬化**——34 处 live 写会立即破坏构建。

⚠️ **`memory_policy` recency 风险**:最近 commit `5df3ade1`(3 天前)是 HookBus 重构、message 写"shared-bus ready"——这是给"未来接线"打地基。**需要确认无 WIP 分支在路上**(见 §3 Open Decisions)。

### 1.C 骨架组 · 运行时/自主性(**关键修正:`harness/` 不是全死**)

| 项 | 状态 | LoC | verdict |
|---|---|---|---|
| `harness/` 全模块 | **混合**——`TrajectoryStore` + `ToolBudgetManager` 是 **load-bearing** 生产类型(每个 agent turn 写,在 AppState 里,7 个 Tauri 评估命令公开);其余是真离线 eval 机器(`HarnessRuntime`/`campaign`/`case`/`graders`/`adapters`/...) | ~7,200 | **SPLIT + RENAME**(非 KILL) |
| `runtime/task.rs::TaskScheduler` struct | struct 本身死;**但同文件**的 `SessionTask` trait + `TaskKind` enum 被 `agent/regular_task.rs` + `agent/rollout_integration.rs` 实在用 | ~60(只切 struct) | **KILL struct,留 trait/enum** |
| `workers/` 全模块 | 1 commit 后再无 follow-up,0 生产调用方 | 401 | **KILL** |
| `task_scheduler/` 全模块 | 自我标注 "actual runner lives in M3-T4 commit 2" — 那个 commit 不存在 | 391 | **KILL** |

**子结论:**
- 立即可清:`workers/` + `task_scheduler/` + `TaskScheduler` struct = **~852 LoC**(高把握)。
- `harness/` 不是 kill,是 split-rename(~1 人天纯机械重构):
  - 抽 `trajectory.rs` → `agent/trajectory.rs`(或 `db/`)
  - 抽 `budget.rs` → `agent/tool_budget.rs`
  - 余下 eval 机器 `harness/` → `eval/`
  - 完成后 `harness/` 这个名字空出,留给将来的 autonomy 监督器(ADR §10 PolicyHook 矩阵的载体)

### 1.D 骨架组 · 插件/Registry(~1,731 LoC,部分重用)

| 项 | 状态 | LoC | verdict |
|---|---|---|---|
| `plugin_manifest/schema.rs` | 5 个 manifest 类型,与 ADR §6.5 子进程/RPC 插件协议规划契合 | 279 | **KEEP**(原样保留作 future schema) |
| `plugin_manifest/load.rs` | 死 TOML loader,模块头自己说 installer 在 "M7-T1 commit 2"(不存在) | 233 | **KILL** |
| `registries/` 全模块 | 1,777 LoC,**有 boot 接线但下游零读取**——`AppState.registry_hub`、`main.rs` 启动同步、`proactive/service.rs` 携带——全是 fill-but-not-read | 1,777 | **KILL**(提取 `tool_families.rs` 174 LoC 后) |
| `registries/tool_families.rs::ToolFamilyCard` + `jcode_inspired_tool_family_cards` | 抓取了真实领域知识(工具分组),AgentApi 句柄需要 | 174 | **EXTRACT** 到 `agent/tool_families.rs`,再删 `registries/` |

**子结论:** 净清理 ~**1,731 LoC**(`load.rs` + `registries/` - `tool_families.rs`),保留 ~**453 LoC** 给阶段 3 用(`plugin_manifest/schema.rs` + `tool_families.rs` 提取)。

⚠️ **registries recency 风险**:最近 commits 跨 2026-05-21 → 2026-05-26 增量构建。**`proactive/service.rs` 7 处 registry_hub 字段** 是删除的最危险面——是 ProactiveStateRefs 的字段透传,但需要逐处确认确无下游读取。

---

## 第 2 部分:战略与优先级

### 2.1 总账

| 组 | 立即可清 LoC | Refactor LoC | 风险 |
|---|---:|---:|---|
| A skill | 425 | 0 | 极低 |
| B memory | 1,878 | 0 | 中(memory_policy recency) |
| C runtime | 852 | ~7,200 重排(eval rename) | 低(只清 dead);中(rename 波及多文件) |
| D plugin/registry | 1,964 | 174 提取 | 中(registries 在 boot 接线) |
| **合计** | **~5,119 LoC kill** | **~7,374 LoC 重排** | 多组混合 |

### 2.2 排序原则

按 **风险 × 阻塞性** 排,从最干净起步:

1. **零风险打头阵**(组 A) — 425 LoC,无下游影响,建立"清骨架不破事"的信号。
2. **中风险但战略关键**(组 D 的 plugin_manifest 部分) — 233 LoC `load.rs` 清掉、`schema.rs` 留下;这是阶段 3 单句柄插件的前置工作。
3. **运行时清干净**(组 C 的可清部分:`workers/` + `task_scheduler/` + `TaskScheduler` struct) — 852 LoC,与 Slice 1a/1b 已落地的 heartbeat / NeedApproval 路径无冲突。
4. **harness/ split-rename**(组 C 的剩余部分) — 0 LoC 净 kill,但是大规模 rename;独立 PR 因为是纯机械重构,review 容易。
5. **registries 清理**(组 D 大头) — 1,777 LoC + extract;**先 extract `tool_families.rs` 到 agent/**(为阶段 3 留接口),再删 `registries/` + boot 接线;PR 风险中等。
6. **memory 清理**(组 B) — 1,778 LoC + 100 LoC;**最后** 做因为 recency 最高、和记忆专项面对面。需要先和用户确认无 WIP。

### 2.3 PR 切片建议(5 个 PR)

| # | 主题 | 内容 | 估 LoC | 估 PR 大小 |
|---|---|---|---:|---|
| **P1** | skill skeleton kill | `skill_selection/` + 死 skill 渲染器 + stranded snapshot 字段 | -425 | 小 |
| **P2** | plugin_manifest 减重 + workers/task_scheduler kill | `load.rs` 删 + `workers/` 全删 + `task_scheduler/` 全删 + `TaskScheduler` struct 删 | -1,317 | 中 |
| **P3** | harness split + rename | `trajectory` / `tool_budget` 抽出;`harness/` → `eval/`;7 个 Tauri 评估命令名 + import 全更新 | 0 净 / ~7,374 重排 | 中-大(纯机械) |
| **P4** | registries kill + extract | extract `tool_families.rs` → `agent/tool_families.rs`;删 `registries/`;清 `AppState`、`main.rs`、`proactive/service.rs` 7 个站点 | -1,731 + 174 留 | 中 |
| **P5** | memory skeleton kill | `memory_contract/` + `memory_policy/` + 3 适配文件 + freeze hook 注释补丁 | -1,878 | 中(需要先确认 recency) |

每个 PR 独立可 revert、独立 bisect。整体顺序 P1 → P2 → P3 → P4 → P5。
**预期总耗时:** 3-5 个工作日(纯机械重构,无新逻辑)。

### 2.4 每个 PR 的 TDD 节奏(与 Slice 1a/1b 一致)

- 每个 PR 一个 worktree + branch。
- 每删一个模块,先 `grep -rn "<symbol>" src/` 确认零非测试调用方 — 这是 red test 的替代(因为没有"先红再绿"可言,但"删后 `cargo build` 干净 + `cargo test agent::` 通过"是绿)。
- 每个 PR 至少一个验证步:`cargo build` empty errors + `cargo test --lib agent::` 与 baseline 一致(773+2 pre-existing)。
- Subagent-driven-development(同 Slice 1a/1b 流程):一个 implementer + spec 审 + 代码质量审 per PR。

### 2.5 风险矩阵 · 单独说

| 风险 | 影响 | 缓解 |
|---|---|---|
| `memory_policy` 最近 HookBus refactor 暗示 WIP 接线 | 删错正在做的人的活 | **P5 之前显式问用户**;若有 WIP,等其落地再清 |
| `registries` 在 `proactive/service.rs` 有 7 处字段透传 | 删除时漏改导致编译失败 | `cargo build` 守门 + 每删一处 grep `registry_hub` 确认零残留 |
| `harness/` rename 涉及 7 个 Tauri 命令名 | 前端调用方失效 | 同步检查 `ui/` 下对 `run_*_harness` 命令的引用,确认是否需要保留旧名做 alias |
| 大批量删除让 `cargo doc` 生成的引用断链 | 文档化影响 | rustdoc 警告非阻塞;一次性清理即可 |
| `automation_executor_kind` 等 pre-existing 死代码 dead-code 警告 | 不增不减 | 不在本评估范围 |

---

## 第 3 部分:Open Decisions(需用户回答才能开 P5)

> ⚠️ **Status (2026-05-28, post-P4)**: P5 已取消。Decision #1 答案是"有 WIP"(Memory OS v2 B+D 等待 A+C+E 之上接线)。Decisions #2-#4 都已在 P3/P4 实施中落实。完整 closeout 见 [`2026-05-28-skeleton-cleanup-stage2-closeout.md`](2026-05-28-skeleton-cleanup-stage2-closeout.md)。

1. **`memory_policy` 最近 HookBus refactor(2026-05-25 `5df3ade1`,3 天前)是否有 WIP 接线正在路上?**
   - 若 **有 WIP**:P5 推迟;等 WIP 落地或撤掉后再清。
   - 若 **无 WIP**(refactor 是孤立准备):按计划 KILL,git 历史保留设计。
   - **答案 (2026-05-28):** 有 WIP — Memory OS v2 程序 A+C+E 已 merge,B+D 在路上,memory_policy/memory_contract 是它们的载重基础。**P5 取消**,memory cleanup 推到 B+D 之后做。

2. **`harness/` rename 是否纳入本期清理?**
   - 选项 A:**单独 P3** 做 split + rename(推荐,腾出 `harness/` 名字给未来 autonomy 监督器)。
   - 选项 B:**推迟**到阶段 3 一起做,避免连续 PR 风暴。
   - 选项 C:**只 split 不 rename**(`harness/` 留作 eval 别名)——不推荐,延续语义混淆。

3. **`registries::tool_families.rs` 提取后落点?**
   - `agent/tool_families.rs`(推荐——属于 agent 内核知识)
   - `agent/tools/tool_families.rs`(更深嵌套)
   - 留在 `registries::` 等阶段 3 再处理(推后但 registries 还得分两次删)

4. **是否在本期 PR 系列结束时一并写一份"阶段 2 清骨架完成报告"?**(类似 Slice 1a/1b 的审计 closeout,记录最终 LoC 减少 + 哪些 commit 落地哪些 PR)

---

## 第 4 部分:非目标(Non-Goals)

- **不做新功能**——每个 PR 都是"删除或纯机械重构"。无新抽象,无新 API。
- **不重建 memory architecture**——`memory_contract` 和 `memory_policy` 删完后留**空缺**;openhuman bucket-seal 是记忆专项(阶段 4)的事。
- **不重建 AgentApi handle**——`plugin_manifest/schema.rs` + `tool_families.rs` 提取后**保留作 future schema**,但 AgentApi 的实际构造是阶段 3 的工作。
- **不动 memory_graph live store**——只给冻结 hook 打文档补丁。memory_graph 该不该退役交给记忆专项。
- **不动 ChatDelegate dispatcher.rs 上帝对象**——审计也提到 3853 行 `ChatDelegate` 71 字段问题,但那是阶段 3(Pi 单句柄)的拆解工作,不是阶段 2 清骨架。
- **不动 8 个记忆存储的合并**——同上,记忆专项工作。
- **不写新测试**(除非删除引发既有测试不可编译,需要适配)。

---

## 第 5 部分:推荐次序与时间线

```
今天 → P1 skill skeleton kill(0.5 天)
     → P2 plugin_manifest 减重 + workers/task_scheduler kill(0.5 天)
     → P3 harness split + rename(1 天 — 最大单 PR)
     → P4 registries kill + tool_families extract(0.5 天)
     → 拍板 memory_policy recency 后:
     → P5 memory skeleton kill(0.5 天)
     → 收官 commit:文档化净减 ~5,000 LoC,影子架构清零
```

**累计预算:** 3 个工作日 + 1 天容错。预期净减 ~5,119 LoC + ~7,374 LoC 重排 = 影子架构在 main 上消失。

---

## 第 6 部分:与未来阶段的衔接

| 阶段 | 受益于本期 |
|---|---|
| **阶段 3** Pi 一个 `AgentApi` 句柄 | 没有 `registries/` 老路线争夺语义;`plugin_manifest/schema.rs` + `agent/tool_families.rs` 可直接作为 manifest schema |
| **阶段 4** 记忆专项 | 没有 `memory_contract` / `memory_policy` 老路线争夺语义;一张白纸接 openhuman bucket-seal |
| **阶段 5** hermes coding | 不直接相关,但减少的 ChatDelegate 注意力负担帮助拆 dispatcher.rs |
| **未来 autonomy supervisor**(ADR §10) | `harness/` 名字空出,可直接命名为 `harness/` |

---

## 推荐动作

**请你回答 §3 的 4 个 Open Decisions**(尤其 #1 关于 `memory_policy` recency),我据此用 `superpowers:writing-plans` 生成 P1 的 TDD bisectable 计划开始执行。其余 PR 沿同一节奏分别落地。

如果你想先看 P1 单独跑一遍(纯零风险 425 LoC),也可以直接说 "P1 先开始",我把 §3 的问题留到 P5 之前再问。
