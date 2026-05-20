# 子项目 E — 复活 MemoryHealthPanel（补 Drift + Importance）设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** [Agent Memory OS v2 north-star](2026-05-20-agent-memory-os-v2-second-brain-design.md) · 子项目 E（复活 L2/L3 悬空前端的剩余部分）
**Depends on:** 子项目 A（gbrain 通路）已完成（PR #288）。E 与 A 在前端不重叠 —— A 复活了 WikiView（知识层），E 复活 MemoryHealthPanel（认知层）。

---

## 1. 背景与决策

侦察发现 `MemoryHealthPanel.tsx` **并非悬空** —— 它的"结构性 findings"那一半已经在工作：`memory_health` proactive 场景填充 V35 `memory_health_findings` 表，面板通过 `memory_health_list_findings` / `memory_health_run_now` / `memory_health_dismiss_finding` 读写。这部分**不动**。

真正"剩余"的是把**本会话已建、但无 UI 的 L3 RETAINED 算法**接进 Health 面板。这些算法分两种状态：

| 算法 | 模块 | 表 | 状态 |
|---|---|---|---|
| Importance Decay | `importance_decay.rs` | V44 `memory_importance_scores` | **已被 proactive 调度**（`service.rs:1315`），表里有真数据，无读命令/无 UI |
| Drift Detection | `drift_detection.rs` | V46 `drift_events` | **已被 proactive 调度**（`service.rs:1366`），表里有真数据，无读命令/无 UI |
| Spaced Repetition | `spaced_repetition.rs` | V45 | 未调度（"待接调度+LLM"）→ 留给子项目 D |
| Triangulation | `triangulation.rs` | V47 | 未调度（需 LLM）→ 留给 D |
| Timeline | `timeline_events.rs` | V44 timeline_events | 已有独立 `timeline` tab（`memory_graph_list_timeline`）→ 不重复 |

**决策（已与用户确认）**：E 只补 **Drift Detection + Importance Decay**（两个已在跑、有真数据的），以**两个可折叠 section** 挂在现有 findings 列表下方；**Drift 可 resolve、Importance 只读**。最小代价复活，不重做 UI。

无新 migration（表已存在）、无新依赖。

---

## 2. 架构 & 模块布局

```
ui/src/components/memory/MemoryHealthPanel.tsx  (扩展：findings 列表下加 2 节)
  │  invoke('memory_drift_*' / 'memory_importance_*')
  ▼
src-tauri/src/tauri_commands.rs  (3 个 #[tauri::command]，沿用 memory_* 锁+gate 模式)
  │  调用
  ▼
src-tauri/src/memory_graph/{drift_detection.rs, importance_decay.rs}  (新增读/写 fn)
  │  rusqlite 查 drift_events / memory_importance_scores（JOIN memory_nodes 取标题）
  ▼
memory_graph_store.conn (已有 SQLite 连接)
```

新命令需在 `main.rs` 的 `invoke_handler!` 宏注册（漏注册编译过但运行时失败 —— CLAUDE.md "Adjacent edits"）。

---

## 3. 后端：新增读/写函数

**`src-tauri/src/memory_graph/drift_detection.rs`**（已有 `list_open_events_for_node` 为 per-node；新增全局列表 + resolve）：

- `list_open_drift_events(conn, space_id, limit) -> rusqlite::Result<Vec<DriftEventRow>>`
  - SQL：`SELECT d.id, d.node_id, n.title, d.score, d.computed_at FROM drift_events d JOIN memory_nodes n ON n.id = d.node_id WHERE d.status='open' AND n.space_id = ?1 ORDER BY d.score DESC LIMIT ?2`
  - `DriftEventRow { id, node_id, title, score, computed_at }`
- `resolve_drift_event(conn, id, note: Option<&str>, now_ms) -> rusqlite::Result<()>`
  - SQL：`UPDATE drift_events SET status='resolved', resolved_at=?, resolution_note=? WHERE id=?`

**`src-tauri/src/memory_graph/importance_decay.rs`**（新增衰减候选列表）：

- `list_decay_candidates(conn, space_id, limit) -> rusqlite::Result<Vec<ImportanceRow>>`
  - 候选定义：`archive_pending_since IS NOT NULL`（算法已标记、有专门索引 `idx_importance_scores_archive`）；按 `importance ASC` 排（最该归档的在前）
  - SQL：`SELECT s.node_id, n.title, s.importance, s.archive_pending_since, s.last_computed_at FROM memory_importance_scores s JOIN memory_nodes n ON n.id = s.node_id WHERE s.archive_pending_since IS NOT NULL AND n.space_id = ?1 ORDER BY s.importance ASC LIMIT ?2`
  - `ImportanceRow { node_id, title, importance, archive_pending_since, last_computed_at }`

> **plan 阶段须核对**：`memory_nodes` 的标题列名（推测 `title`）、`space` 列名（推测 `space_id`）—— 实现前 grep 确认，不对则按真实列名调整 SQL。

---

## 4. Tauri 命令面（3 个）

签名 `async fn(state: State<'_, AppState>, …) -> Result<T, String>`，沿用现有 memory 命令：锁 `state.memory_graph_store.conn`、经 `ensure_memory_health_enabled` gate（memory_os 关闭时返回既有 disabled 错误）。

| 命令 | 参数 | 返回 |
|---|---|---|
| `memory_drift_list_events` | `space_id?`, `limit?`(默认 100) | `Vec<DriftEventDto{id,nodeId,title,score,computedAt}>` |
| `memory_drift_resolve_event` | `event_id`, `note?` | `()` |
| `memory_importance_list_candidates` | `space_id?`, `limit?`(默认 100) | `Vec<ImportanceCandidateDto{nodeId,title,importance,archivePendingSince,lastComputedAt}>` |

DTO 在 tauri_commands.rs（或就近模块）定义 `#[derive(Serialize)]`；TS 镜像在 `ui/src/lib/types.ts`。

---

## 5. 前端：扩展 MemoryHealthPanel

保留现有结构（header + severity-grouped findings + dismiss），在 findings `ScrollArea` 内容**下方**追加两节，用一个新的本地 `<CollapsibleSection title count>` 组件：

- **「概念漂移」section**：`memory_drift_list_events` → 每行 `title`（点击经 `onSelectSubject(node_id)` 跳转，复用现有 prop）+ score 徽章（按阈值着色：≥0.6 `text-destructive`，否则 `text-amber-500`）+ `computedAt` 相对时间 + "标记已处理"按钮（调 `memory_drift_resolve_event`，成功后乐观移除该行）。
- **「重要度 · 衰减候选」section（只读）**：`memory_importance_list_candidates` → 每行 `title` + importance 分值（小进度条或数字）+ archive-pending 时长。无动作按钮。空时显示"无衰减候选"。

- **加载**：面板挂载时与 findings **并行**拉两节数据。每节独立 loading + 行内错误（一节失败不影响 findings 与另一节）。
- **主题合规**：全部用 token（`text-destructive`/`text-amber-500`/`text-muted-foreground`/`bg-muted/*`/`border-border/*`），不硬编码。

---

## 6. 错误处理

| 场景 | 行为 |
|---|---|
| memory_os 关闭 | 命令返回既有 disabled 字符串；两节显示"记忆健康未启用"提示，不报错崩溃 |
| drift / importance 查询失败 | 该节显示行内小错误（"加载失败"），findings 与另一节正常 |
| resolve 失败 | toast 错误 + 该行保留（不乐观移除）；不崩溃 |
| 空结果 | 正常空状态文案，不报错 |
| 表为空（算法还没产数据） | 空状态，不报错 |

---

## 7. 测试

**Rust（`drift_detection.rs` / `importance_decay.rs` 内联单测，内存 SQLite，无需 proactive）：**
- `list_open_drift_events_orders_by_score_and_filters_space` —— 种 memory_nodes（两 space）+ drift_events（open/resolved 混合），断言只返回 open、按 score 降序、space 过滤生效、join 出 title
- `resolve_drift_event_flips_status` —— resolve 后该行 `status='resolved'` 且不再出现在 list
- `list_decay_candidates_only_archive_pending_asc` —— 种 importance 行（部分 archive_pending_since 非空），断言只返回 pending、importance 升序、space 过滤、join title
- 空表/limit=0 不 panic

**TS（Vitest + RTL，mock `invoke`）：**
- 面板渲染现有 findings + 两个新 section（mock 三命令）
- 漂移行点 "标记已处理" → 调 `memory_drift_resolve_event` + 行消失
- 重要度 section 只读渲染（无 resolve 按钮）
- 某节命令 reject → 该节行内错误，findings 仍渲染
- 空数据 → 各节空状态

**手动 E2E（写进验证清单）：** 跑真 app（drift/importance proactive 已调度产数据）→ 打开 Health tab → 看两节有数据 → resolve 一条 drift → 刷新确认消失。

---

## 8. 范围边界（明确不做）

- ❌ 不碰结构性 findings 那半（已工作）
- ❌ 不做节点归档/删除（importance 只读；归档是破坏性操作，不在 E）
- ❌ 不改调度（drift/importance 已在跑）
- ❌ Spaced Repetition / Triangulation（未调度，留给 D）
- ❌ Timeline（已有独立 tab）
- ❌ 不碰 WikiView / gbrain（那是 A）
- ❌ 无新 migration、无新依赖

---

## 9. 提交形状（bisectable，预计单 PR ~5 commit）

1. `feat(memory): drift_detection list+resolve fns + importance decay-candidates fn + tests`
2. `feat(tauri): register memory_drift_* + memory_importance_* commands + invoke_handler`
3. `feat(ui): tauri-bridge wrappers + DTO types for drift/importance`
4. `feat(ui): MemoryHealthPanel — collapsible Drift (resolve) + Importance (read-only) sections`
5. `test(ui): MemoryHealthPanel vitest for new sections`

无新 migration（E 不碰 schema）。
