# TurnSnapshot 隔离 — 设计

> Pi 框架融合 Sprint 2(item 2/3)。把 Pi spec §1/§9 的不可变每轮快照(`Arc<TurnSnapshot>`)+ 轮边界钩子 + `NextTurnPatch`(配置在轮边界安全应用)适配进 uClaw 的 `agentic_loop.rs`,形式化「一轮 = 一次 loop 迭代」的边界。
>
> **状态**:已通过 brainstorming 评审,待转 writing-plans。
> **日期**:2026-05-26
> **分支/worktree**:`codex/pi-sprint2-turnsnapshot`(base = main `07647d6a`,含已合并的迭代式压缩 #548)
> **Pi 参考**:`agent-framework-pi-upgrade-design.md` §1/§9;ADR §20 TurnSnapshot Isolation。

---

## 1. 背景与现状

uClaw 的 `run_agentic_loop`(`agentic_loop.rs:56`)是单层 `for iteration in 1..=max_iterations` loop;**一轮 = 一次迭代**(一次 LLM 调用 + 工具分派)。当前:

- `AgenticLoopConfig`(`types.rs:383`)只含循环参数(max_iterations、token budget、压缩阈值等),**无 model / system_prompt / tools**,以 `&` 不可变借入,整个 run 固定。
- `model` + `system_prompt` 住在 `ChatDelegate`(`dispatcher.rs:46-48`):`self.model: String`、`self.system_prompt: String`。每次 `call_llm` 内部调 `effective_system_prompt()` **实时重新组装**(base + mode overrides + GEP gene 注入 + plan-suggest 信号 + project rules + cache padding)。`reason_ctx.system_prompt` 字段存在但被 `self.system_prompt` 取代。
- **无 mid-session 配置变更**:无 model/prompt setter;改配置需重建 ChatDelegate(= 新 user message)。无「pending config / apply at next turn」概念。
- `run_agentic_loop` 跑**一条 user message 触发的整个多轮 agent 任务**(最多 max_iterations 轮);一个 session = 多次 run,共享 `ReasoningContext`。

**目标**:引入不可变每轮快照 + 轮边界机制,形式化轮边界(为 item ③ 双队列注入打基础),并为未来多模型 hot-swap(Sprint 4)预留隔离。

### 价值定位(诚实)

- **(c) 主要**:形式化轮边界 —— item ③(SteeringQueue/FollowUpQueue)需要明确的轮边界安全注入消息。
- **隔离**:in-flight `call_llm` 持 `Arc<TurnSnapshot>` clone;轮边界替换 current_snapshot 不影响它(为 Sprint 4 hot-swap 预留)。
- **(b) 次要/边际**:B2 缓存。`effective_system_prompt()` 本就每轮变(GEP/plan-suggest),稳定前缀已由 `cache_align` 缓存;TurnSnapshot 不新增缓存收益,只是冻结 in-flight 调用看到的 prompt。**不把 B2 当主要卖点**。

---

## 2. 关键决策(brainstorming 已定)

| 决策 | 选择 |
|---|---|
| 范围 | **完整 Pi**:Arc<TurnSnapshot> + 轮边界钩子 + NextTurnPatch + 双层 loop 重构 + GEP/plan-suggest 组装搬进 snapshot 创建 |
| 轮边界 trait | **扩展现有 LoopDelegate**(不新建 TurnBoundaryDelegate;ChatDelegate 已实现 LoopDelegate;run_agentic_loop 不需第二个 trait 参数) |
| 重新快照 vs 冻结整 run | **每轮重建 snapshot**(动态 prompt 每轮重评估,保持 GEP/plan-suggest 响应性);in-flight 调用期间冻结(隔离) |

---

## 3. 架构

### 3.1 新模块 `agent/turn.rs`

```rust
use std::sync::Arc;

/// 一轮(一次 loop 迭代)的不可变配置快照。以 Arc 共享:in-flight 的
/// call_llm 持有自己的 Arc clone,轮边界替换 current_snapshot 不影响它。
#[derive(Clone)]
pub struct TurnSnapshot {
    pub turn_index: u32,
    pub model: String,
    pub system_prompt: Arc<String>,         // 本轮冻结的 effective_system_prompt() 输出
    pub tools: Arc<Vec<crate::agent::types::ToolDef>>,  // 本轮冻结的工具集
    pub force_text: bool,
}

/// 轮边界对下一轮的补丁(显式配置变更/注入/停止)。
/// item ② 暂无生产者(prepare_next_turn 默认 None);为 Sprint 4 多模型
/// hot-swap + item ③ 注入预留。
#[derive(Default)]
pub struct NextTurnPatch {
    pub model: Option<String>,
    pub tools: Option<Vec<crate::agent::types::ToolDef>>,
    pub inject_message: Option<crate::agent::types::ChatMessage>,
    pub should_stop: bool,
}

/// 把补丁应用到下一轮快照(turn_index +1)。inject_message/should_stop 由 loop 处理。
pub fn apply_patch(mut snap: TurnSnapshot, patch: NextTurnPatch) -> TurnSnapshot {
    if let Some(m) = patch.model { snap.model = m; }
    if let Some(t) = patch.tools { snap.tools = Arc::new(t); }
    snap.turn_index += 1;
    snap
}
```

### 3.2 LoopDelegate 扩展(`types.rs`)

```rust
// 为即将开始的一轮创建不可变快照:冻结 model + 组装好的 system_prompt + tools。
// 必需方法(无 default)—— 所有 impl 实现。
async fn create_turn_snapshot(&self, reason_ctx: &ReasoningContext, turn_index: u32)
    -> crate::agent::turn::TurnSnapshot;

// 轮边界钩子:返回对下一轮的补丁。默认 None(item ② 无生产者)。
async fn prepare_next_turn(&self, _reason_ctx: &ReasoningContext, _turn_index: u32)
    -> Option<crate::agent::turn::NextTurnPatch> { None }

// 自然停止点钩子,item ③ 填充。默认空。
async fn get_steering_messages(&self) -> Vec<ChatMessage> { Vec::new() }
async fn get_follow_up_messages(&self) -> Vec<ChatMessage> { Vec::new() }
```

`call_llm` 签名改为消费快照:`call_llm(&self, reason_ctx: &mut ReasoningContext, snapshot: &TurnSnapshot, iteration: usize) -> Result<RespondOutput, Error>`。

### 3.3 run_agentic_loop 双层 loop 重构(数据流)

```rust
let mut pending_patch: Option<NextTurnPatch> = None;
for iteration in 1..=config.max_iterations {
    // 轮起点:重建快照(动态 prompt 每轮重评估)
    let mut snap = delegate.create_turn_snapshot(reason_ctx, iteration as u32).await;
    if let Some(p) = pending_patch.take() { snap = apply_patch(snap, p); }  // 应用上轮补丁(hot-swap)
    let current_snapshot = Arc::new(snap);

    check_signals(...); compress_context_if_needed(...).await; // before_llm hook
    // 内层 turn(提取成 fn 满足借用检查器):call_llm(持 Arc clone)+ 处理 RespondOutput
    let outcome = run_inner_turn(delegate, reason_ctx, &current_snapshot, iteration).await;
    // after_iteration hook
    match outcome { InnerOutcome::Return(o) => return o, InnerOutcome::Continue => {} }

    // 轮边界
    match delegate.prepare_next_turn(reason_ctx, iteration as u32).await {
        Some(p) if p.should_stop => break,
        Some(p) => {
            if let Some(m) = &p.inject_message { reason_ctx.messages.push(m.clone()); }
            pending_patch = Some(p);
        }
        None => {}
    }
}
```

- **隔离**:in-flight `call_llm` 持 `Arc<TurnSnapshot>` clone(经 `&*current_snapshot` 传入,调用栈期间有效);外层替换 current_snapshot 不影响它。
- **借用检查器**:内层 turn 提取成独立 fn `run_inner_turn(delegate, &mut reason_ctx, &TurnSnapshot, iteration) -> InnerOutcome`(spec §9.2「挑战3」推荐),避免 `&mut reason_ctx` 跨嵌套 loop 冲突。`InnerOutcome` 枚举映射现有 in-loop 控制流(Return / Continue / Nudge / Rescue 等)。

### 3.4 触及文件

| 文件 | 改动 |
|---|---|
| `agent/turn.rs` | **新建** — `TurnSnapshot` / `NextTurnPatch` / `apply_patch` |
| `agent/mod.rs` | `pub mod turn;` |
| `agent/types.rs` | `LoopDelegate` 加 4 方法(create_turn_snapshot 必需 + 3 default);`call_llm` 签名加 `snapshot` |
| `agent/agentic_loop.rs` | 双层 loop 重构;`run_inner_turn` 提取;snapshot 驱动 call_llm(高注意力) |
| `agent/dispatcher.rs` | `ChatDelegate::create_turn_snapshot`(搬 effective_system_prompt 组装)+ call_llm 改用 snapshot(高注意力) |
| 其它 LoopDelegate impl(regular_task / rollout_integration / 测试 mock) | 继承 3 个 default 钩子;实现 create_turn_snapshot;call_llm 签名更新 |

---

## 4. create_turn_snapshot 机制(搬组装)

现状:`call_llm` 每迭代内部调 `effective_system_prompt()`(base + GEP gene + plan-suggest + rules + padding)。改造:搬到 `create_turn_snapshot`,轮起点跑一次、冻结进 snapshot。

```rust
// dispatcher.rs ChatDelegate
async fn create_turn_snapshot(&self, reason_ctx: &ReasoningContext, turn_index: u32) -> TurnSnapshot {
    let system_prompt = self.effective_system_prompt(reason_ctx).await;  // 现有逻辑原样搬来
    TurnSnapshot {
        turn_index,
        model: self.model.clone(),
        system_prompt: Arc::new(system_prompt),
        tools: Arc::new(self.assemble_tools(reason_ctx)),  // 若 call_llm 里有工具组装,一并搬来
        force_text: reason_ctx.force_text,
    }
}
```

- `effective_system_prompt()` 调用**频率不变**(今天每迭代一次 = 现在每轮一次)→ GEP/plan-suggest 响应性不退化;只移动调用点。
- `reason_ctx` 以 `&`(只读)传入。**实现期核对点**:现有组装是否纯只读?若有副作用(写 plan-suggest 状态等),甄别——只读用于组装,副作用保持幂等或留原处。

## 5. call_llm 改造(消费 snapshot)

```rust
async fn call_llm(&self, reason_ctx, snapshot: &TurnSnapshot, iteration) -> Result<RespondOutput, Error> {
    let config = CompletionConfig { model: snapshot.model.clone(), /* max_tokens 等不变 */ ..};
    let messages = build_request_messages(&snapshot.system_prompt, &reason_ctx.messages);  // 用冻结 prompt
    let tools = (*snapshot.tools).clone();
    self.llm.complete(messages, tools, &config).await   // 流式/cost capture/重试逻辑不变
}
```

- prompt/model/tools 来自 snapshot,不来自 self/reason_ctx。`reason_ctx` 仍 `&mut`(更新 token 计数等)。
- `reason_ctx.system_prompt` 字段保持现状(今天就被取代);snapshot.system_prompt 是本轮权威。**不重构** reason_ctx.system_prompt(避免扩散)。
- **其它 LoopDelegate impl 适配面**:凡实现 call_llm 的(regular_task/rollout/test mock)签名都加 `snapshot`;create_turn_snapshot 必需方法所有 impl 实现(非生产 mock 给最简实现:model + reason_ctx.system_prompt + 空 tools)。

## 6. 轮边界 + ③ 钩子 stub

- `prepare_next_turn` 默认 `None`(item ② 无生产者)。机制由**返回 patch 的 mock** 验证 hot-swap(下一轮 snapshot 换 model)。
- `should_stop` → loop break(汇合现有 Return 路径);`inject_message` → push reason_ctx.messages(为 ③ 注入预留)。
- `get_steering_messages`/`get_follow_up_messages`:LoopDelegate default 返回空。item ② **不调用**(纯定义 trait 表面);item ③ 在 loop 接入 + ChatDelegate override。

---

## 7. 错误处理 / 边界

| 情况 | 处理 |
|---|---|
| create_turn_snapshot 组装失败 | 复用现有 effective_system_prompt 容错;失败回退现有降级 |
| prepare_next_turn None | 正常;下一轮 create_turn_snapshot 重建(动态重评估) |
| should_stop | loop break,汇合现有 LoopOutcome::Return |
| 内层 turn 借用冲突 | run_inner_turn 取 `&mut reason_ctx` + `&TurnSnapshot`,返回 InnerOutcome enum,外层分支 |
| 单轮/首轮 | turn_index=1 正常;无 patch;行为等价今天 |
| 流式 call_llm 跨 await | in-flight 持 Arc clone;隔离成立 |

---

## 8. 测试

**Rust 单元**(`turn.rs` + `agentic_loop.rs`):
- `apply_patch`:model/tools 覆盖正确;turn_index +1;None 字段不变。
- Arc 隔离语义:clone 一个 Arc,替换 outer,旧 clone 内容不变。
- run_agentic_loop 集成(Counting/PatchingDelegate mock):
  - 每轮 create_turn_snapshot 调一次(turn_index 递增)。
  - call_llm 收到的 snapshot.system_prompt/model == create_turn_snapshot 产出(冻结一致)。
  - prepare_next_turn 返回 `model: Some("X")` 的 patch → **下一轮** snapshot.model == "X",当前轮不受影响(hot-swap 机制验证)。
  - `should_stop=true` → loop 终止。
  - default 钩子返回空、item ② 不调用(回归保护)。
- 现有 agentic_loop 测试全绿(行为等价回归)。

---

## 9. ADR §18 子集

- **Intent**:形式化轮边界(为 ③ 打基础)+ 不可变每轮快照隔离(为 Sprint 4 hot-swap 预留);不改 agent 决策与单轮行为。
- **Truth source**:snapshot 是本轮配置权威;reason_ctx 仍是累积状态权威。
- **Capability**:无新 IPC、无新表、无迁移。
- **Harness/测试**:见 §8。
- **Rollback**:`git revert`;无 schema 变更;prepare_next_turn 默认 None → 行为对单/多轮等价。
- **不拥有**:steering/follow_up 队列实现(③);hot-swap 生产者/多模型(Sprint 4);reason_ctx.system_prompt 重构;压缩(已 ①)。

---

## 10. 范围边界(YAGNI)

✅ 做:`TurnSnapshot`(Arc)、create_turn_snapshot(冻结组装)、call_llm 消费 snapshot、双层 loop 重构 + run_inner_turn 提取、prepare_next_turn/NextTurnPatch/apply_patch、should_stop、get_steering/get_follow_up 的 **default-empty stub**。
❌ 不做:steering/follow_up 队列实现(③)、hot-swap 生产者 / 多模型(Sprint 4)、reason_ctx.system_prompt 字段重构、压缩改动。
