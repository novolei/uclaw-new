# 迭代式压缩 + Split-Turn 恢复 — 设计

> Pi 框架融合 Sprint 2(item 1/3)。把 Pi spec §11 的 `CompactionState` / `UPDATE_SUMMARIZATION_PROMPT` / Split-Turn 设计适配进 uClaw 现有 `compact/` 模块,把自动压缩从 O(N) 整史重摘升级为 O(1) 增量,并优雅处理切点落在工具对中间的情况。
>
> **状态**:已通过 brainstorming 评审,待转 writing-plans。
> **日期**:2026-05-26
> **分支/worktree**:`codex/pi-sprint2-continue`(base = main `2c8105cc`)
> **Pi 参考**:`docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md` §11;ADR §20 P2。

---

## 1. 背景与现状

uClaw 已有一套压缩系统,但 LLM 层是**全史一次性**:

- `compact/summarize.rs::summarize_to_fold(llm, model_id, messages)` 接收**整个**待压缩切片,渲成 transcript,一次 LLM 调用产出 `StructuredFold` JSON。无「上一份摘要」输入——每次从零重摘。`extractive_fallback_fold` 为无 LLM 降级路径。
- `compact/fold.rs::StructuredFold` 是规范摘要表示,10 轴:`facts / decisions / unresolved_questions / evidence_refs / failed_attempts / active_constraints / next_actions / rollback_points / micro_capsules / file_ops`(file_ops 为 Sprint 1 第 10 轴)。
- `compact/fold_diff.rs`(`StructuredFold::diff` / `apply_delta`)+ `render.rs`(`to_markdown` / `render_fold_delta_block`)+ `cache_align.rs` + `baseline.rs`(读写 V52 `agent_fold_baselines` 每会话一行 fold)。这是**渲染层 delta**(旧 fold + 注解变化),不是**生成层 delta**。
- 触发:`agentic_loop.rs::compress_context_if_needed`(soft 75% → `soft_compress_context`;hard → `hard_truncate_context`)。用户 `/compact`:`force_compact_sync`(extractive,无 LLM)+ `tauri_commands.rs` 里的 LLM `summarize_to_fold` 调用。
- 持久化:`agent_messages.compacted=1`(切点为时间戳)+ `compaction_markers`(V29)+ `agent_fold_baselines`(V52,每会话 fold)。
- **现状无 LLM 层增量**:每次压缩把全切片喂 LLM,token 随会话长度 O(N) 增长。

**目标**:自动压缩改为增量(O(1) 每周期),保留 uClaw 全部 StructuredFold 基础设施,Split-Turn 用 Pi 的部分摘要恢复,零 schema 迁移。

---

## 2. 关键决策(brainstorming 已定)

| 决策 | 选择 | 理由 |
|---|---|---|
| 增量摘要表示 | **保留 StructuredFold,增量更新** | 复用 10 轴 + fold_diff/render/baseline;LLM"重述完整结构"比"产 JSON diff"可靠 |
| 切点/状态持久化 | **零迁移:内存态 + 重载重建** | 复用 V52 baseline + 现有 compacted 标记;无新表/列 |
| Split-Turn 处理 | **Pi 部分摘要** | 切点落工具对中间时,split 前缀单独摘要;压缩更彻底,安全边界远也能压 |

---

## 3. 架构

### 3.1 新模块 `agent/compaction.rs`

```rust
/// 跨轮次累积的增量压缩状态(运行期内存,放 ReasoningContext)。
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    /// 上一份 fold(增量基底);None = 首次压缩。
    pub previous_fold: Option<StructuredFold>,
    /// 上次切点(created_at 时间戳,与现有 compacted 标记同源);None = 尚未压缩。
    pub last_boundary: Option<i64>,
    /// 统计:上次压缩前的 token 估算(重载后不要求精确)。
    pub tokens_before_last: u32,
}

/// 切点 + Split-Turn 信息。
#[derive(Debug, Clone)]
pub struct CompactionCutPoint {
    /// 后缀逐字保留起点(message index)。
    pub first_kept_index: usize,
    /// 切点是否落在 ToolUse/ToolResult 对中间。
    pub is_split_turn: bool,
    /// split 时:被切那一轮的起点 index。
    pub turn_start_index: Option<usize>,
}

/// 计算切点 + 检测 split-turn。从 token 预算边界向后扫。
pub fn find_compaction_cut_point(messages: &[ChatMessage], target_tokens: usize) -> CompactionCutPoint;

/// 增量压缩编排器:首次走全史,后续走增量,处理 split-turn,更新 state。
pub async fn compress_with_iterative_summary(
    llm: &dyn LlmProvider,
    model_id: &str,
    messages: &[ChatMessage],
    state: &mut CompactionState,
    target_tokens: usize,
) -> Result<CompactionOutcome, CompactionError>;

pub struct CompactionOutcome {
    pub fold: StructuredFold,        // 更新后的 fold(已含 split-turn 段)
    pub first_kept_index: usize,     // 后缀逐字保留起点
    pub new_boundary: i64,           // 新切点时间戳
}
```

### 3.2 数据流(auto soft-compress,75% 预算触发)

```
soft_compress_context 触发
  → find_compaction_cut_point(messages, target) → CutPoint
  → state.previous_fold 为 None(首次)?
       是 → summarize_to_fold(待压缩全切片)              [复用现有全史路径]
       否 → update_fold_incremental(prior_fold.to_markdown() + [last_boundary..cut] 新消息)
            → 更新后的 StructuredFold                     [新增,O(1)]
  → CutPoint.is_split_turn?
       是 → summarize 前缀 [turn_start..first_kept] → 作为「Turn Context (split turn)」MicroCapsule 附到 fold
  → 后缀 [first_kept..] 逐字保留
  → state ← { previous_fold=新fold, last_boundary=new_boundary, tokens_before_last=… }
  → 持久化:写 V52 baseline(现有)+ 标记 compacted(现有)+ 写 compaction_marker(现有)
  → 渲染:fold.to_markdown()(+ 现有 fold_diff delta-render)注入上下文
```

LLM 输入恒定(旧 fold ~固定 + 新消息窗口)→ 压缩 O(N) → O(1)。

### 3.3 触及文件

| 文件 | 改动 |
|---|---|
| `agent/compaction.rs` | **新建** — `CompactionState` / `CompactionCutPoint` / `CompactionOutcome` / `find_compaction_cut_point` / `compress_with_iterative_summary` |
| `agent/compact/summarize.rs` | 新增 `update_fold_incremental(...)` + UPDATE prompt;抽出 JSON 解析/校验/extractive 兜底供复用;现有 `summarize_to_fold` / `extractive_fallback_fold` 保留 |
| `agent/agentic_loop.rs` | `soft_compress_context` 改调 `compaction.rs`,内联逻辑下沉(高注意力文件,diff 收窄) |
| `agent/types.rs` | `ReasoningContext` 加 `compaction_state: CompactionState` |
| `agent/mod.rs` | `pub mod compaction;` |
| `agent/session.rs`(或会话重载路径) | reconstruct-on-load:seed `CompactionState` |
| `tauri_commands.rs` | LLM `/compact` 路径在有 prior fold 时走增量(复用 `compress_with_iterative_summary`) |

---

## 4. 增量 UPDATE summarizer

### 4.1 `update_fold_incremental`(新增于 `summarize.rs`)

```rust
pub async fn update_fold_incremental(
    llm: &dyn LlmProvider, model_id: &str,
    prior_fold: &StructuredFold,   // 上一份 fold
    new_messages: &[ChatMessage],  // 仅 [last_boundary..cut] 的新消息
) -> Result<StructuredFold, SummarizeError>;
```

### 4.2 Prompt 结构(复用现有 structured-JSON 输出契约,新增 UPDATE 语义)

```
<previous_summary>
{prior_fold.to_markdown()}          ← 已有 to_markdown(),把 10 轴渲成 markdown
</previous_summary>

<new_messages>
{render_transcript(new_messages)}   ← 复用现有 render_transcript
</new_messages>

指令:你在「更新」一份已有的结构化摘要,而非从零生成。基于 previous_summary,
吸收 new_messages 带来的变化,输出**完整的、更新后的** StructuredFold JSON(同一 schema):
- facts/decisions:保留仍相关的;新增 new_messages 引入的;矛盾时以新证据为准
- next_actions:已完成的移除,新出现的加入
- unresolved_questions:已解决的移除
- failed_attempts / active_constraints / rollback_points:累积
- file_ops:并入 new_messages 中的文件操作(与 SessionFileOps 一致)
- micro_capsules:为 new_messages 里的关键轮次补 capsule
```

→ 输出仍是**完整** StructuredFold(非 JSON diff):模型擅长"重述完整结构";渲染/baseline/fold_diff 不变;输入 O(1)。`summarize_to_fold` 的 JSON 解析 + 校验 + extractive 兜底抽出复用。

---

## 5. Split-Turn 部分摘要

**检测**(`find_compaction_cut_point`):从 token 预算边界向后扫定 `first_kept_index`。若 `messages[first_kept_index]` 是 `ToolResult` 而其配对 `ToolUse` 在切点之前(工具对被切开)→ `is_split_turn = true`,`turn_start_index` = 该轮起点(最近的 User/assistant 轮边界)。

**恢复**(`compress_with_iterative_summary`):
1. 主摘要覆盖 `[.. turn_start_index]`(增量或首次路径)。
2. split 前缀 `[turn_start_index .. first_kept_index]` **单独**摘要(无 prior,一次性小摘要)→ 文本。
3. 拼接进 fold:作为带标记的 `MicroCapsule`(`label: "Turn Context (split turn)"`),复用 micro_capsules 轴,`to_markdown()` 天然带出。
4. 活跃后缀 `[first_kept_index ..]` 逐字保留(含完整 ToolResult + 后续)。

→ 保证:LLM 历史里**无**悬空 ToolUse(缺配对 ToolResult)或悬空 ToolResult。被切轮的语义以「Turn Context」短摘要保住。

---

## 6. Reconstruct-on-load(零迁移)

会话从 DB 重载、重建 `ReasoningContext` 时 seed `CompactionState`:

```
若存在 compacted 消息(SELECT 1 FROM agent_messages WHERE conversation_id=? AND compacted=1 LIMIT 1):
  previous_fold      ← V52 agent_fold_baselines 该 session 的 fold(已有 baseline.rs 读取)
  last_boundary      ← MAX(created_at) WHERE compacted=1
                       (回退:最后一个 compaction_marker 的边界时间戳)
  tokens_before_last ← 0
否则:
  CompactionState::default()(previous_fold=None → 下次走首次全史路径)
```

不新增任何表/列。baseline 读不到(老会话)→ 当作 None,下次首次压缩重建。

---

## 7. 错误处理 / 边界

| 情况 | 处理 |
|---|---|
| 首次压缩(previous_fold=None) | 走现有 `summarize_to_fold`(全史一次性),结果作为新 baseline |
| 增量 LLM 调用失败 | 回退 `summarize_to_fold`(全史);再失败 → `extractive_fallback_fold`(无 LLM) |
| 自上次切点无新消息 | 跳过(无可压缩) |
| Split-Turn 但 turn_start 找不到 | 回退现有 `find_safe_compaction_boundary`(refuse-and-move) |
| reconstruct 时 baseline 缺失 | 当作 None,下次首次压缩 |
| hard_truncate 路径 | **不改**(仍走现有硬截断;增量只接 soft-compress) |

---

## 8. 测试

**Rust 单元**(`compaction.rs` + `summarize.rs`):
- `find_compaction_cut_point`:(a) 普通切点 `is_split_turn=false`;(b) 切点落 ToolUse/ToolResult 对中间 → `is_split_turn=true` 且 `turn_start_index` 正确;(c) 后缀无悬空工具对。
- `update_fold_incremental`(mock LLM):prior fold + 新消息 → 更新后 fold 保留 prior facts + 并入新项;断言喂入 transcript **仅含新消息**(不含历史)。
- `compress_with_iterative_summary`:首次走全史;第二次走增量(断言喂入 prior fold + 仅新消息);split-turn 产出「Turn Context」capsule;后缀逐字保留。
- token 有界性:连续多轮增量,断言每次 LLM 输入 token 不随历史增长(O(1))。
- reconstruct-on-load:给定 compacted 消息 + baseline,seed 出正确 `previous_fold` + `last_boundary`。

---

## 9. ADR §18 子集

- **Intent**:压缩 O(N) 整史重摘 → O(1) 增量;长会话 token/延迟/成本骤降;不改 agent 决策。
- **Truth source**:活跃后缀逐字保留为权威;fold 是有损摘要;增量以新证据覆盖旧。
- **Capability**:无新 IPC、无新表。
- **Harness/测试**:见 §8。
- **Rollback**:`git revert`;无 schema 变更;失败自动回退全史路径。
- **不拥有**:不改 hard_truncate、不改 fold_diff 渲染层、不碰 TurnSnapshot/双队列(后续 Sprint 项)。

---

## 10. 范围边界(YAGNI)

✅ 做:auto soft-compress 增量化、Split-Turn 部分摘要、StructuredFold 增量更新、reconstruct-on-load、失败回退、LLM `/compact` 路径增量化。
❌ 不做:新迁移、hard_truncate 改造、fold_diff 渲染层重写、跨 session 共享压缩、TurnSnapshot/双队列(Sprint 2 item 2/3)。
