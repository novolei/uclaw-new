# PR Integration Strategy v2.4 — 2026-05-22

> **重建说明**: 这份文档替换 2026-05-21 失踪的同名 strategy doc(原稿在 Cowork
> session 结束时被 scratchpad 清掉了 — git history / 所有分支 / 所有 stash 都搜
> 不到)。本文基于 PR #289-#396 的真实历史 + task #36 ~ #174 + 主线两份文档重新合
> 成。
>
> **作用域**: Phase 0.5 启动 (2026-05-20 PR #289) → Bundle 26-B/26-D/27-B 合并
> (2026-05-22 PR #396)。共 108 个合并 PR。
>
> **目的**: (1) 把 108 个 PR 映射回 ADR M0-M9 里程碑;(2) 量化主线-vs-战术比例;
> (3) 建立 closed-loop 进度追踪流程,防止 Bundle 18-27 那种"全跑战术不推主线"的漂
> 移再次发生。

---

## §1. 背景:为什么需要这份 doc 重建

2026-05-21 那天 18 个 PR 一次性合掉(#377-#395),其中 Bundle 18-27 完全是战术债
(bug fix + UX polish),**零米程碑推进**。M1 之后到 5-22 这两天,M2/M3/M4 都只有
pilots 落地 + 少量 slice wire-up,**没有任何一个主线里程碑被正式 close**。

这种漂移的根因有两个:

1. **没有 single source of truth 跟踪 milestone 进度** — 主线文档
   `uclaw-upgrade-implementation-plan.md` 是 May 20 写完的静态设计文档,不会随
   PR 合并自动更新。task list 175 条任务里,M-标签的命名跟 plan 文档的 M-T-编
   号不一致 (task list 把 M3 拆成 T1-T9 pilot,plan 文档 M3 是 T1-T6) — 没办法
   一眼对账。
2. **没有 drift 检测** — Bundle 18-27 这种"修 bug → 又修 bug → 又修 bug"的链反
   应没有任何 forcing function 喊停。一周下来 30 个 PR,milestone 0 进展。

本次重建提供 3 件工具来闭环:

- 这份 strategy doc(本文)— 给 PR 分类的方法论 + 当前状态
- `docs/superpowers/MILESTONE_STATUS.md` — 单一真理表,每个 PR 落地后**人工**改
  一行
- `scripts/milestone-drift-check.sh` — 自动跑 drift 检测,生成"上周 N 个 PR 里
  战术占比 X%"的报告

---

## §2. PR 分类方法论

每个合并 PR 落入下列 4 类之一:

| 类别 | 定义 | 计入里程碑 % | 例子 |
|---|---|---|---|
| **M-Foundation** | 直接对应 ADR §16 中 M0-M9 的任务清单(M*-T*) | ✓ | #304 M1-T1 runtime contracts |
| **M-Pilot** | 某 M-T 任务的 "pilot" — 类型骨架/skeleton,无生产路径 | 部分 (0.3×) | #338 M1-T1 contract patch — HookDecision |
| **M-Wireup (Slice)** | pilot 接入真实代码路径,真重构 | ✓ (1.0×) | #365 Slice 2 — wire M2-H L2/L5/L6 |
| **Tactical** | 战术 bug fix / UX polish / dogfood fix,不推动任何 M 进展 | ✗ | #383 Bundle 15 find_bun_path |
| **Phase 0.5** | M0 之前的 infrastructure(LICENSE / hooks / skills / crate 复制) | M0 内 | #289 LICENSE + NOTICE |
| **Backlog** | 既不属于 M-* 也不属于 Tactical 的散件 | ✗ | #322 backlog real provider/model |

**关键决策**: M-Pilot 计 0.3× — pilot 类型存在但生产路径不走它,意味着"任务卡完
成 30%,剩 70% 是 wire-up"。Wire-up 计 1.0×。Tactical/Backlog 不计入 milestone 进度。

---

## §3. 全 108 PR 分类时间轴

按合并日期排序。**完整表见 [§3-detail](#3-detail-pr-逐条分类)**;这里展示按里
程碑聚合的进度:

### §3.1 进度矩阵 (2026-05-22 截止)

| Milestone | ADR §16 任务数 | 完成的 PR(分类) | 估算 % | DoD 还差什么 |
|---|---|---|---|---|
| **Phase 0.5** | T1-T10(infra) | #289-#302 + #291 + #292 + #294 + ... = 14 PR | **100%** | 完工 |
| **M0** | ADR Lock + License | 已在 Phase 0.5 内 | **100%** | 完工 |
| **M1: Runtime Contracts** | T1-T7 | #304-#320(17 PR)+ retrospective #321 | **100%** | task #57 标 "closes M1" |
| **M2: Context Fabric** | A-J 10 子任务 | A pilot+wire(#326-328);B pilot(#339);C pilot(#329);D pilot(#340)+ Bundle 16+17(#384-385);E template(#324-325);F pilot(#330);G pilot(#331)+ Slice 3-A(#367);H L1-L7 pilots(#332-337)+ Slice 2(#365-366)+ Slice 3-C(#379);I pilot(#341);J pilot(#342) | **~55%** | 大量 wire-up 没落,bench 没做 |
| **M3: Capability Mesh** | T1-T6+(plan) / T1-T9(task) | T1-T9 pilots(#343, #350-358);M3-T1 wire-up slice 1+2(#390-391) | **~22%** | 7 个 T 的 wire-up 全部缺 |
| **M4: World Projection** | T1-T4 | T1-T8 pilots(#346 + 其他在 #343-358 区间)| **~24%** | 0% wire-up |
| **M5: Policy Hooks + Isolation** | T1-T4 | M1-T1 contract patch (HookDecision) #338 | **~10%** | 整个 milestone 基本没动 |
| **M6: Browser Provider** | T1-T4 | 无 | **0%** | 整个 milestone 没动 |
| **M7: Evolution Factory** | T1-T6 | 无(task 的 M7-T1 = plugin manifest,跟 plan 的 M7 = evolution 是**不同概念**)| **0%** | 整个 milestone 没动 |
| **M8: Teams v1** | T1-T6 | 无 | **0%** | 整个 milestone 没动 |
| **M9: Cluster v1** | T1-T5 | 无 | **0%** | 整个 milestone 没动(也是远期) |

### §3.2 战术 vs 主线比例

| 时期 | 合并 PR 数 | 主线 PR(M-* + Phase 0.5)| 战术 PR(Bundle / Backlog)| 战术 % |
|---|---|---|---|---|
| **2026-05-19 周**(#264-#288) | 25 | 23(Memory OS Phase 8 + Harness)| 2(dock UI 修)| 8% |
| **2026-05-20 周**(#289-#329) | 41 | 38(Phase 0.5 + M1 + M2 pilots)| 3(diagnostics 修)| 7% |
| **2026-05-21 单日**(#330-#395) | 66 | 36(M2/M3/M4 pilots + 4 Slices)| 30(**Bundle 1-27 大爆炸**)| **45%** |
| **2026-05-22**(#396)| 1 | 1(Bundle 26-B/26-D/27-B + settings)| 0 | 0% |
| **整窗** | 133 | 98 | 35 | **26%** |

**结论**: 5-21 单日战术 45% 是真实警报 — 那天 18 个 Bundle PR 全是 dogfood
回归债。但平均 26% 在"可接受"范围。**红线建议**: 任何 7-day window 战术 % > 40
就触发 Drift Alarm。

### §3-detail PR 逐条分类

> 完整表保留在 [docs/superpowers/MILESTONE_STATUS.md](../MILESTONE_STATUS.md)
> 的附录里。本文档只列分类规则;实际 PR 状态是 MILESTONE_STATUS 的 live data。

---

## §4. M2/M3/M4 完成定义(DoD)交叉核对

按 `uclaw-upgrade-implementation-plan.md` 各 milestone §X.3 的 DoD 复制:

### §4.1 M2 完成定义(plan §4.3,line 770-)

- [ ] 全部 M2-A ~ M2-J 合并
- [ ] **ADR M2 Exit criteria**: agent 可按需检索 code/memory/browser/prior
  trace 上下文,不预加载全部
- [ ] **Benchmark**: 50-turn 会话 token 节约 60-75%
- [ ] **Cached token 命中率 ≥ 50%**
- [ ] 月度成本下降 ≥ 60%
- [ ] 输出格式一致性主观评分 +1.5/5 以上

**当前**: 4/6 条没动(标 ❌ 的)— bench、cache hit、cost down、format consistency
都没有量化数据。**M2 实际进度 < 60%**。

### §4.2 M3 完成定义(plan §5.x 暗示)

- [ ] M3-T1 ~ M3-T6 全部 wire-up(只 T1 完成 slice 1+2)
- [ ] **Exit criteria**: 本地 browser + gbrain 注册为 provider;至少 1 个 bundled
  plugin 可发现但 disabled;1 个 task 以受限 capability profile 运行

**当前**: T1 wire-up 完了一半 slice。T2 (现有 tools 注册到 ToolRegistry) 没动。
T3 (gbrain/memU 注册到 ProviderRegistry) 没动。**M3 < 25%**。

### §4.3 M4 完成定义(plan §6.x)

- [ ] M4-T1 ~ M4-T4 全部完成
- [ ] **Exit criteria**: UI 能回答 agent 在做什么 / 等什么 / 用什么 / 能否 resume

**当前**: M4-T1 ~ M4-T8 的 pilot 都在(注意 plan 文档里 M4 只 T1-T4,task list
拆得更细到 T8 — 这两个编号体系需要在 MILESTONE_STATUS 里 reconcile)。0% wire-up。
**M4 < 25%**。

---

## §5. Closed-loop 进度追踪流程

下面这 6 步把 "PR merge → milestone status 反映" 形成闭环。**每个 PR 合并后**
都跑一次前 3 步;**每周一**跑后 3 步。

### §5.1 Per-PR 流程(每个 PR 合并后立即,< 2 分钟)

1. **打 milestone 标签**: PR title 或 commit msg 含明示标签,e.g.
   `[M3-T2 wire-up]` / `[Bundle 27-D]` / `[Phase 0.5-T11]` / `[Backlog]`
2. **更新 MILESTONE_STATUS.md**: 1 行 patch,移 PR 编号到 "Done" 列,如有需要
   更新 % 估算
3. **如果是 wire-up 关键 slice**: 触发 verify 脚本 — 见
   [`scripts/verify/<name>.sh`](../../scripts/verify/) + 技能
   `uclaw-tick-feature-verify`

### §5.2 Per-Week 流程(每周一上午,< 30 分钟)

4. **跑 drift check**:
   ```bash
   ./scripts/milestone-drift-check.sh --since "1 week ago"
   ```
   输出格式:
   ```
   ==================== Drift Check 2026-05-22 ====================
   Window:      2026-05-15 → 2026-05-22 (7 days)
   PRs merged:  18
   M-Foundation:  3 (#322, #390, #396)
   M-Wireup:      1 (#391)
   M-Pilot:       0
   Tactical:     12 (#383-394 Bundle 13-25)
   Phase 0.5:     0
   Backlog:       2 (#385, #386)

   Tactical ratio: 12/18 = 67%
   ⚠ ALARM: tactical ratio > 40% threshold
   Recommendation: hold tactical PRs, finish M2-J + Bundle 17-B/C wire-up
   before opening next bundle.
   ```
5. **审核 drift alarm**: 如果触发,在 MILESTONE_STATUS 顶部留一条 NOTE,在
   下次 planning 会议(或 self-review)讨论该不该开始 milestone 收尾
6. **月底审计**: 每月 1 号回写 `uclaw-upgrade-implementation-plan.md` §34
   "v2.4 进度快照",冻结当月 milestone 状态。配合主线文档版本号 v2.5 / v2.6 ...

### §5.3 当 Bundle 类工作不可避免时(real-world 折衷)

dogfood 会持续产生 Bundle 类 PR — 不是 bug,是健康反馈。规则:

- **Bundle 单 PR 允许**: 任意一周里 5 个以内的 Bundle 不算"战术爆炸"
- **Bundle 连续 ≥ 7 PR**: 强制开 "Bundle window" tag,在 MILESTONE_STATUS 里
  注明本 Bundle window 阻塞了哪个 milestone,什么时候关 window
- **关 window 标准**: 至少 1 个 milestone 切片(Slice / wire-up)在本 window
  内合并,证明主线没全停

### §5.4 Drift alarm 阈值表

| 指标 | 绿 | 黄 | 红 |
|---|---|---|---|
| 7-day 战术 % | < 30% | 30-40% | > 40% |
| 连续 Bundle PR | < 5 | 5-7 | > 7 |
| Pilot 滞留 (从 merge 到 wire-up 的天数) | < 14 天 | 14-30 天 | > 30 天 |
| Milestone idle (无新 PR 推进) | < 14 天 | 14-30 天 | > 30 天 |

红色:**强制**在下个 PR 开之前修;黄色:在下个 review 提及;绿色:常态。

---

## §6. Cutoff 标准:何时关一个 Milestone

每个 milestone 关闭需满足:

1. ADR §16 该 milestone 的 Exit Criteria 全部 ✓
2. plan 文档 §X.3 (`### X.3 M? 完成定义`) 全部 ✓
3. Benchmark 类指标有数据支撑(不能只是 "应该达到")
4. 在 PR description 中明示 "Closes M?" + 引用上面三项的证据
5. 写一份 `docs/superpowers/reports/2026-XX-XX-M?-closeout.md` 复盘,含
   - 实际 vs 预估工作量(plan 文档预估 e.g. M2 = 5-7 周)
   - 哪些 sub-task 漂移成了 Bundle,根因
   - 给下个 milestone 的建议

参考已有的 [M1 retrospective doc (#321)](../reports/) 的格式。

---

## §7. C1 → C2 → C3 执行序列详细分解

按你的指令,接下来按这个顺序推进。每个 C 块 (M2 收尾 / M3 / M4) 都对应一个或多
个 PR,严格执行 §5.1 流程。

### C1: 收尾 M2(预估 1.5 天)

| 步 | 任务 | PR 标签 | 验证 |
|---|---|---|---|
| C1.1 | Bundle 17-B/C dispatcher fold delta wire-up(task #146 解锁) | `[M2-D Phase 2 wire-up]` | telemetry on next 100-turn session |
| C1.2 | M2-J TokenBudgetSnapshot UI 接入 Settings → Token Usage 页 | `[M2-J wire-up]` | UI 展示 progress bar + top-10 tools |
| C1.3 | M2-H L3 skills top-K wire-up(pilot #336 + budget 1500)| `[M2-H L3 wire-up]` | per-turn skill manifest token 测量 |
| C1.4 | M2-B ContextManager skeleton wire-up + M2-F context tools wire-up | `[M2-B+F wire-up]` | context.search / context.read 真用起来 |
| C1.5 | 50-turn benchmark + cache hit measurement(M2 DoD 量化数据)| `[M2 benchmark]` | `docs/superpowers/reports/2026-XX-M2-benchmark.md` |
| C1.6 | M2 closeout report | `[M2 closeout]` | 上面那份 report 链回 §6 标准 |

### C2: 推进 M3(预估 6-8 周,按 plan 文档)

按 plan 文档 §5.2 顺序:

| 步 | 任务 | 工时 | 验证 |
|---|---|---|---|
| C2.1 | M3-T2 现有 tools(builtin/MCP/memU/skill-as-tool)注册到 ToolRegistry | 1w | tool list IPC 走 registry 而不是 hardcoded |
| C2.2 | M3-T3 MCP/providers/gbrain/memU 注册到 ProviderRegistry + health TTL | 1w | `provider_status` IPC reads from registry |
| C2.3 | M3-T4 新增缺失工具(mcp_resource / request_permissions / view_image / tool_search / unified_exec)+ V47 migration | 0.5w | 5 个新 builtin tool 可调用 |
| C2.4 | M3-T5 Skill scope ENUM + per-turn 注入(配合 V43 已有 migration)| 0.5w | UI 显示 skill 来源 (User/Repo/Workspace/System) |
| C2.5 | M3-T6 PluginRegistry + Plugin manifest(4 source + 5 kind + install/update/uninstall/list)| 2w | bundled plugin 可发现 |
| C2.6 | M3 closeout report | — | 1 个 task 以受限 capability profile 跑通 |

### C3: 推进 M4(预估 3-4 周)

| 步 | 任务 | 工时 | 验证 |
|---|---|---|---|
| C3.1 | M4-T1 WorldProjection 类型 + apply_event(pilot 已有,补 wire-up)| 1w | TaskEvent → projection state update |
| C3.2 | M4-T2 projection subscriber + diff_since(version) + V53 migration | 1w | 增量更新 + resume snapshot |
| C3.3 | M4-T3 前端 `useWorldProjection(taskId)` hook + 20+ store 迁移 | 1w | chat panel 用 projection 直读 |
| C3.4 | M4-T4 各 panel 消费方迁移(chat/browser/automation/timeline)| 1w | 不再走 hybrid store mix |
| C3.5 | M4 closeout report | — | UI 能回答"agent 在做什么/等什么/用什么/能否 resume" |

### Beyond C3: M5 → M9

按 plan 文档:

| Milestone | 周数 | 关键依赖 | 备注 |
|---|---|---|---|
| **M5** Policy Hooks + Isolation | 4-5w | M3 (CapabilityProfileRegistry) | hookbus + 13 events |
| **M6** Browser Provider 抽象 | 3-4w | M3 (ProviderRegistry) | local + remote |
| **M7** Evolution Factory | 6-8w | M2 + M3 | learning loop 升级 |
| **M8** Teams v1 | 5-7w | M5 + M7 | subagent topology |
| **M9** Cluster v1 | 12-16w | M8 | 远期,可推后 |

总剩余: ~30-40 周(按 plan 估算)。**建议节奏**: 每 milestone 之间留 1 周
buffer 处理 dogfood 反馈(就是 Bundle 类工作),不要连续无 buffer 切下一个。

---

## §8. 跟 codex-comparison-and-design.md 的对照

`uclaw-codex-comparison-and-design.md` 是设计层文档,本 strategy 是执行层。
关键引用映射:

| Milestone | 设计层节 | 主要约束 |
|---|---|---|
| M1 | §6 Runtime Kernel | SessionTask trait + RegularTask + TaskEvent stream |
| M2 | §7 Context Fabric | 11 层运行时模型 + 30+ fragment + 7 context tools + 8 字段 fold |
| M3 | §8 Capability Mesh | 5 registry + CapabilityCard + Hermes-style 4-source plugin |
| M4 | §9 World Projection | UI 真理来源 + diff_since + resume |
| M5 | §10 Safety & Policy | 13 hook events + isolation profiles |
| M6 | §14 Browser Provider | local + remote 抽象 |
| M7 | §11 Evolution Layer | Learning / Harness / Proactive pipeline |
| M8 | §12 Workers | Subagent + Teams |
| M9 | §12 Workers (Cluster 部分)| 远期 |

**操作纪律**: 写每个 milestone 的 spec / plan / commit msg 时,**必须**回链到
对应设计层节,让两份文档保持双向 traceability。MILESTONE_STATUS 表会有 "Design ref"
列做这件事。

---

## §9. 立即可执行的下一步

按 C1 → C2 → C3,**今天/明天先做 C1.1 + C1.2**:

```bash
# Step 1: 读 Bundle 17 是怎么把 fold delta 接到 dispatcher 的(task #145)
# Step 2: 找 task #146 阻塞的 telemetry — 是不是真阻塞还是可以推迟
# Step 3: 开 prep/m2-bundle-17bc-wireup 分支,1-2 commit,PR
# Step 4: 合并,跑 drift check,更新 MILESTONE_STATUS

# 并行 C1.2:
# Step 1: 看 M2-J pilot #342 的 TokenBudgetSnapshot 类型
# Step 2: 在 Settings 加 Token Usage 页(类似 StreamSkillThresholdsSection)
# Step 3: 一个 PR
```

完事后 C1.3-C1.6,再开 C2.

---

## §10. 这份 strategy 的版本约定

| 版本 | 日期 | 变更 |
|---|---|---|
| v2.4 | 2026-05-22 | 重建初版 (PR #396 后) |

每完成一个 milestone 升一个版本号 (v2.5 = M2 closeout 后,v2.6 = M3 closeout 后,...)。
strategy 文档自身也跟 plan 文档保持版本同步。
