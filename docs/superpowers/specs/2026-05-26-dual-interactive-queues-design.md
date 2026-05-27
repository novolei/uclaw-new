# 双交互队列(SteeringQueue + FollowUpQueue)— 设计

> Pi 框架融合 Sprint 2(item 3/3,最后一项)。把 Pi spec §10 的双队列(steering = mid-run 轮边界注入;follow-up = 自然停止点串行注入)适配进 uClaw,**取代现有 "引导" 的 spawn-new-loop hack 并复用其 UI**,接入 item ② 已就位的 `get_steering_messages`/`get_follow_up_messages` 钩子。
>
> **状态**:已通过 brainstorming 评审,待转 writing-plans。
> **日期**:2026-05-26
> **分支/worktree**:`codex/pi-sprint2-dualqueue`(base = main `eed905f1`,含 ①迭代压缩 #548 + ②TurnSnapshot #550)
> **Pi 参考**:`agent-framework-pi-upgrade-design.md` §10;ADR §20。

---

## 1. 背景与现状

### 现有 "引导" 功能(我们要替换其后端、复用其 UI)

uClaw 已有 mid-run 消息注入功能,UI = `ui/src/components/agent/QueuedMessagesBanner.tsx`:

- **入队**:agent streaming 时,composer 提交的消息进入 `agentQueuedMessagesMapAtom`(按 sessionId,localStorage 持久),banner 显示排队卡片。
- **引导按钮**(`interrupt:true`):卡片上 "引导" 按钮,tooltip "立刻把这条消息引导给运行中的 Agent(会打断当前 turn)"。`handleSteerQueued` → `queueAgentMessage({interrupt:true})` → `invoke('queue_agent_message')`。
- **完成后自动发送**(`interrupt:false`):`useEffect` 监 `streaming` true→false 边沿,逐一(FIFO,一次一个)pop 并 `queueAgentMessage({interrupt:false})`。
- **后端**:`queue_agent_message`(tauri_commands.rs)是 `send_agent_message` 的薄别名;`interrupt` 标志被**静默丢弃**。实际机制:把消息持久化到 `agent_messages` + spawn 第二个 agent loop —— **旧 loop 被 orphan(token 仅被 HashMap 静默替换,未 cancel),两 loop 可能短暂并存**;新 loop 从 DB 历史(含 steer 消息在尾)重建。

**问题**:引导 = 起新 loop + orphan 旧 loop(非真 mid-run 注入);`interrupt` 是前端幻觉。

### item ② 留下的钩子(集成点)

`LoopDelegate`(types.rs)已有 `prepare_next_turn`(轮边界)、`get_steering_messages`/`get_follow_up_messages`(default 空,本项填)。`run_agentic_loop` 已用 `run_turn_body -> TurnFlow` 结构 + 轮边界。

### SoftInterruptQueue

死码(types.rs,有 push/drain API 但未接入 AppState/ChatDelegate/任何生产者)。本项删除。

**目标**:真正的 mid-run steering(运行 loop 在轮边界 drain,无 orphan)+ 后端串行 follow-up(自然停止点逐一处理,连续历史),复用现有 QueuedMessagesBanner UI。

---

## 2. 关键决策(brainstorming 已定)

| 决策 | 选择 |
|---|---|
| 范围 | 机制 + 后端生产者 + **复用现有 引导 UI**(重接线,非新建前端)|
| follow-up 串行所有权 | **后端拥有(完整 Pi 双队列)**:前端推到 FollowUpQueue,loop 外层串行化;移除前端 per-completion flush |
| follow-up 位置 | **run_agentic_loop 内外层 loop**(连续历史,Pi 语义)|
| QueueMode | follow-up OneAtATime(一次一个任务)|
| 触发 follow-up 的终态 | **仅 LoopOutcome::Response**(终态 Stopped/Cancelled/Failure/MaxIterations 不触发)|
| 队列 key | **session_id**(同现有 引导 + send_agent_message)|

---

## 3. 架构

### 3.1 新模块 `agent/queues.rs`

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use crate::agent::types::ChatMessage;

/// mid-run steering:agent 工作期间注入,轮起点排空。
#[derive(Clone, Default)]
pub struct SteeringQueue { inner: Arc<Mutex<VecDeque<ChatMessage>>> }
impl SteeringQueue {
    pub fn push(&self, msg: ChatMessage);
    pub fn drain(&self) -> Vec<ChatMessage>;  // 原子取走全部
    pub fn is_empty(&self) -> bool;
}

/// follow-up:仅 agent 自然停止(Response)时,OneAtATime 取一个任务注入并重入。
#[derive(Clone, Default)]
pub struct FollowUpQueue { inner: Arc<Mutex<VecDeque<Vec<ChatMessage>>>> }
impl FollowUpQueue {
    pub fn push_task(&self, messages: Vec<ChatMessage>);
    pub fn next(&self) -> Option<Vec<ChatMessage>>;  // 取一个任务
    pub fn is_empty(&self) -> bool;
}
```

### 3.2 AppState 按 session 拥有(生产者/消费者共享)

```rust
// app.rs
pub agent_queues: Arc<Mutex<HashMap<String, AgentQueues>>>,   // key = session_id

#[derive(Clone, Default)]
pub struct AgentQueues { pub steering: SteeringQueue, pub follow_up: FollowUpQueue }

impl AppState {
    pub fn agent_queues_for(&self, session_id: &str) -> AgentQueues { /* lock + entry().or_default().clone() */ }
    // 已有 running_sessions HashMap;agent_follow_up 用它判活跃
    pub fn is_session_running(&self, session_id: &str) -> bool { /* running_sessions.contains_key */ }
}
```

### 3.3 ChatDelegate 实现钩子 + persist

```rust
// dispatcher.rs ChatDelegate 新增字段(构造时 = state.agent_queues_for(&input.session_id))
steering_queue: SteeringQueue,
follow_up_queue: FollowUpQueue,

async fn get_steering_messages(&self) -> Vec<ChatMessage> { self.steering_queue.drain() }
async fn get_follow_up_messages(&self) -> Vec<ChatMessage> { self.follow_up_queue.next().unwrap_or_default() }
// 注入的 steering/follow-up 写 agent_messages(重载连续);默认空(测试 mock)。
async fn persist_user_message(&self, _m: &ChatMessage) { /* ChatDelegate: INSERT agent_messages */ }
```

新增 `persist_user_message` 到 LoopDelegate(default 空)。

### 3.4 run_agentic_loop 重构(外层 follow-up loop + steering 轮起点 drain)

```rust
pub async fn run_agentic_loop(delegate, reason_ctx, config) -> LoopOutcome {
  'followup: loop {
    let mut pending_patch = None;
    let mut completed: Option<LoopOutcome> = None;
    for iteration in 1..=config.max_iterations {
        check_signals; cancel; compress; before_llm;          // 现有 pre-LLM(可早返回)
        // steering 轮起点排空
        for m in delegate.get_steering_messages().await {
            reason_ctx.messages.push(m.clone());
            delegate.persist_user_message(&m).await;
        }
        let mut snap = delegate.create_turn_snapshot(reason_ctx, iteration as u32).await;
        if let Some(p) = pending_patch.take() { snap = apply_patch(snap, p); }
        let current_snapshot = Arc::new(snap);
        match run_turn_body(delegate, reason_ctx, &current_snapshot, iteration, config, ...).await {
            TurnFlow::Return(LoopOutcome::Response(r)) => { completed = Some(LoopOutcome::Response(r)); break; }
            TurnFlow::Return(other) => return other,           // 终态:跳过 follow-up
            TurnFlow::Continue => {
                match delegate.prepare_next_turn(reason_ctx, iteration as u32).await { /* ② 轮边界 */ }
            }
        }
    }
    let outcome = match completed { Some(o) => o, None => return LoopOutcome::MaxIterations };  // 仅 Response 触发
    let follow_up = delegate.get_follow_up_messages().await;   // OneAtATime
    if follow_up.is_empty() { return outcome; }
    for m in follow_up {
        reason_ctx.messages.push(m.clone());
        delegate.persist_user_message(&m).await;
    }
    // 每-follow-up 重置 per-run 计数(truncation/nudge);compaction_state/file_ops 沿用 in-memory;continue
    continue 'followup;
  }
}
```

- **steering**:运行 loop 在轮起点 drain,实时注入,**无 orphan 旧 loop**(取代现有 hack)。
- **follow-up**:仅 Response 触发;OneAtATime 取一任务,append + 重入外层(同一 run,连续历史)。
- **持久化**:注入消息写 agent_messages(重载连续)。
- 借用/结构沿用 ② 的 run_turn_body/TurnFlow;外层 `'followup: loop` 包内层 for。

### 3.5 触及文件

| 文件 | 改动 |
|---|---|
| `agent/queues.rs` | **新建** SteeringQueue/FollowUpQueue |
| `agent/types.rs` | LoopDelegate 加 `persist_user_message`(default 空);移除 SoftInterruptQueue(+ 可选 LoopSignal::InjectMessage)|
| `agent/mod.rs` | `pub mod queues;` 替 `pub mod interrupts;`(若 interrupts 是独立文件)/ 删 SoftInterruptQueue |
| `app.rs` | AppState `agent_queues` + `AgentQueues` + `agent_queues_for` + `is_session_running` |
| `agent/dispatcher.rs` | ChatDelegate +2 字段 + get_steering/get_follow_up/persist_user_message(高注意力)|
| `agent/agentic_loop.rs` | 外层 follow-up loop + steering 轮起点 drain(高注意力);移除 InjectMessage arm(可选)|
| `tauri_commands.rs` + `main.rs` | `agent_steer` / `agent_follow_up` 命令 + 注册;ChatDelegate 构造注入 queues |
| `harness/campaign.rs`(+ tests) | 清理 SoftInterruptQueue 引用 |
| `ui/.../QueuedMessagesBanner.tsx` + `AgentView.tsx` + `tauri-bridge.ts` | 引导按钮→agent_steer;排队→agent_follow_up;移除 per-completion flush;banner 按 uuid dequeue;新增 agentSteer/agentFollowUp bridge |

---

## 4. Tauri 命令

```rust
/// 引导:mid-run 注入。push 到 session 的 SteeringQueue;运行 loop 轮起点 drain。
#[tauri::command]
async fn agent_steer(state, session_id: String, content: String, uuid: Option<String>) -> Result<(), String> {
    state.agent_queues_for(&session_id).steering.push(ChatMessage::user(&content));
    Ok(())
}

/// follow-up:活跃 run → push FollowUpQueue(外层 loop drain);空闲 → 普通新 run。
#[tauri::command]
async fn agent_follow_up(state, app_handle, session_id: String, content: String, uuid: Option<String>) -> Result<(), String> {
    if state.is_session_running(&session_id) {
        state.agent_queues_for(&session_id).follow_up.push_task(vec![ChatMessage::user(&content)]);
        Ok(())
    } else {
        send_agent_message(state, app_handle, SendAgentMessageInput { session_id, user_message: content, ..Default::default() })
            .await.map_err(|e| e.to_string())
    }
}
```

在 `main.rs` invoke_handler 注册。`uuid` 透传供前端 banner dequeue 同步。`queue_agent_message` 保留(不再被 引导/排队 调用),后续可删。

## 5. 前端 引导 UI 重接线(复用 QueuedMessagesBanner)

| 现有 | 改为 |
|---|---|
| 引导按钮 `handleSteerQueued` → `queueAgentMessage({interrupt:true})` | → `agentSteer({sessionId, content, uuid})`(新 bridge → `agent_steer`);从 banner atom 移除该条 |
| 排队(composer submit while streaming)| 入 atom **并** `agentFollowUp({sessionId, content, uuid})`(新 bridge → `agent_follow_up`)|
| per-completion auto-flush effect(streaming true→false,逐一新起 run)| **移除**(后端外层 loop 接管串行化)|
| banner dequeue | follow-up 成为 turn 时(stream 出现该 uuid 的 user 消息)→ 前端按 uuid 从 atom 移除 |

新增 `agentSteer`/`agentFollowUp` 于 tauri-bridge.ts。banner 组件、编辑/删除按钮、排队 UX 不变(纯复用)。

## 6. 删除 SoftInterruptQueue(死码)

删 `SoftInterruptQueue` + 其测试 + `harness/campaign.rs` 引用 + mod/types 条目。`LoopSignal::InjectMessage` 死分支(steering 现走 get_steering_messages)→ 可选移除 variant + agentic_loop arm。

---

## 7. 错误处理 / 边界

| 情况 | 处理 |
|---|---|
| agent_steer/follow_up 未知 session | agent_queues_for get-or-create(无害;下个 run 用该 session 时 drain)|
| follow-up 在 run 已返回后到 | agent_follow_up 见空闲 → 起普通 run |
| run 终态(Stopped/Cancelled/Failure/NeedApproval/MaxIterations)| 直接返回,不 drain follow-up(留队列待下次)|
| 并发 push vs drain | Mutex-safe |
| steering 在 run 已停后到(UI 仅 streaming 时显示引导)| 留队列,下个 run 轮起点 drain |
| run 崩溃但 FollowUpQueue 非空 | 队列留存;下次 agent_follow_up 空闲 → 起 run 处理 |

---

## 8. 测试

- **队列单元**:SteeringQueue push/drain;FollowUpQueue push_task/next(OneAtATime)。
- **loop 集成**(mock delegate):steering 轮起点注入 → 下一轮 messages 含它(+ persist 调用);follow-up 在 Response drain → 外层重入处理 → 队空返回;仅 Response 触发(终态不 drain);多 follow-up 串行(逐一)。
- **命令**:agent_steer push 到该 session SteeringQueue;agent_follow_up 活跃→push 队列 / 空闲→起 run(mock running_sessions)。
- **前端 vitest**:引导按钮调 agent_steer + 移除 banner 项;排队调 agent_follow_up;移除 completion-flush;banner 按 uuid dequeue。
- 现有 agentic_loop/regular_task 全绿(get_steering/follow_up/persist 默认空 → 行为等价)。

## 9. ADR §18 子集

- **Intent**:真 mid-run steering(无 orphan loop)+ 后端串行 follow-up(连续历史);取代 引导 spawn-new-loop hack + 前端 completion-flush。不改 agent 决策。
- **Capability**:新增 `agent_steer`/`agent_follow_up`;AppState 按 session 持有队列。无新表/迁移。
- **Truth source**:队列 = 注入意图;注入后 persist 到 agent_messages(权威历史)。
- **Harness/测试**:见 §8。
- **Rollback**:`git revert`;无 schema 变更;get_steering/follow_up 默认空 → loop 行为等价(其它 delegate 不受影响)。
- **不拥有**:多模型(Sprint 4);queue_agent_message 删除(后续);banner 高级编排。

## 10. 范围边界(YAGNI)

✅ 做:SteeringQueue/FollowUpQueue + AppState 持有 + agent_steer/agent_follow_up(含空闲起 run)+ loop steering drain + 外层 follow-up loop + persist 注入 + 前端 引导 UI 重接线 + 删 SoftInterruptQueue。
❌ 不做:新迁移、多模型、queue_agent_message 删除、banner 高级编排、QueueMode 可配置(固定 OneAtATime)。
