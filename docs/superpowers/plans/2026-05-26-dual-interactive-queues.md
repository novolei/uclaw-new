# 双交互队列 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 真 mid-run steering(SteeringQueue,运行 loop 轮起点 drain,无 orphan)+ 后端串行 follow-up(FollowUpQueue,自然停止点外层 loop 逐一 drain),取代现有 "引导" spawn-new-loop hack,复用 QueuedMessagesBanner UI。

**Architecture:** `agent/queues.rs` 两队列(Arc<Mutex>);AppState 按 session_id 持有 + 经 `agent_queues_for` get-or-create 共享;ChatDelegate drain 钩子 + persist;`run_agentic_loop` 外层 `'followup` loop + steering 轮起点 drain;Tauri `agent_steer`/`agent_follow_up`;前端 引导按钮→agent_steer、排队→agent_follow_up。

**Tech Stack:** Rust + Tokio,async_trait,Tauri,React/Jotai。

**Spec:** `docs/superpowers/specs/2026-05-26-dual-interactive-queues-design.md`
**Branch/worktree:** `codex/pi-sprint2-dualqueue`(base = main `eed905f1`)

---

## ⚠️ 规划期事实(实现以此为准)

- `conversation_id == session_id`(send_agent_message 把 input.session_id 当 conversation_id 传给 ChatDelegate)。队列 key = session_id。
- `running_sessions: Arc<tokio::sync::Mutex<HashMap<String, CancellationToken>>>`(app.rs:261)。活跃判定:`.lock().await.contains_key(&session_id)`。
- ChatDelegate 经 `ChatDelegate::new(...)`(dispatcher.rs:227,10 参数,多调用点含 chat-mode send_message)。**不改 ::new 签名**:加 default-空字段 + `with_agent_queues`/`with_db` setter,仅 agent 路径调。
- SoftInterruptQueue 在 `agent/interrupts.rs:52` + `interrupts_tests.rs`(`#[path]` at :106)。**harness/campaign.rs 无引用**。`LoopSignal::InjectMessage` arm 在 agentic_loop.rs:478(本计划保留 LoopSignal,仅删 SoftInterruptQueue struct)。
- 用户消息持久化 INSERT 范式(tauri_commands.rs:10586-10605):`INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?,?, 'user', ?, ?)` + `UPDATE agent_sessions SET message_count = message_count + 1, updated_at WHERE id`。
- 前端重接线点:`handleSteerQueued`(AgentView.tsx:1199,`queueAgentMessage interrupt:true`)、completion-flush effect(:1267,`interrupt:false`)、`tauri-bridge.ts:1645`、`agent-queue-messages.ts`(`removeQueuedMessage(prev, sessionId, id)`)。

---

## 验证命令

- 后端编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
- 后端单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- 前端:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue/ui && npm test -- --run <file> 2>&1 | tail -12` + `npx tsc --noEmit 2>&1 | head`
- 已知后端预存失败 ~5(daemon_approval / truncate_for_error / 2× browser::runtime_status / gbrain_eval_harness);若 stale tauri 构建产物报 `pi-sprint2-turnsnapshot` 路径,清 `target/debug/build/tauri-*` + `uclaw-*`。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `agent/queues.rs` | **新建** SteeringQueue/FollowUpQueue |
| `agent/mod.rs` | `pub mod queues;`;(Task 6)删 `pub mod interrupts;` |
| `app.rs` | AppState `agent_queues` + `AgentQueues` + `agent_queues_for` + `is_session_running` |
| `agent/types.rs` | LoopDelegate `persist_user_message`(default 空)|
| `agent/dispatcher.rs` | ChatDelegate +queues/db 字段 + setter + get_steering/get_follow_up/persist_user_message |
| `agent/agentic_loop.rs` | 外层 follow-up loop + steering 轮起点 drain + persist 注入 |
| `tauri_commands.rs` + `main.rs` | agent_steer/agent_follow_up + 注册 + send_agent_message 注入 queues+db |
| `agent/interrupts.rs` + `interrupts_tests.rs` | **删除**(Task 6)|
| `ui/.../tauri-bridge.ts`, `AgentView.tsx` | agentSteer/agentFollowUp + 重接线 引导/排队/dequeue |

任务顺序自底向上:queues → AppState → delegate 钩子 → loop → 命令+接线 → 删 SoftInterruptQueue → 前端。

---

## Task 1: agent/queues.rs

**Files:** Create `src-tauri/src/agent/queues.rs`; Modify `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 失败测试** — 新建 `queues.rs`,先只写测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::ChatMessage;

    #[test]
    fn steering_push_drain() {
        let q = SteeringQueue::default();
        assert!(q.is_empty());
        q.push(ChatMessage::user("a"));
        q.push(ChatMessage::user("b"));
        let drained = q.drain();
        assert_eq!(drained.len(), 2);
        assert!(q.is_empty());           // drain 取走全部
        assert!(q.drain().is_empty());   // 再 drain 为空
    }

    #[test]
    fn followup_one_at_a_time() {
        let q = FollowUpQueue::default();
        assert!(q.is_empty());
        q.push_task(vec![ChatMessage::user("t1")]);
        q.push_task(vec![ChatMessage::user("t2")]);
        let first = q.next().unwrap();    // 取一个任务
        assert_eq!(first.len(), 1);
        assert!(!q.is_empty());           // 还剩一个
        let second = q.next().unwrap();
        assert!(q.is_empty());
        assert!(q.next().is_none());      // 空 → None
        let _ = second;
    }

    #[test]
    fn clone_shares_inner() {
        let q = SteeringQueue::default();
        let q2 = q.clone();
        q.push(ChatMessage::user("x"));
        assert!(!q2.is_empty());          // clone 共享内部 Arc(生产者/消费者共享)
    }
}
```

- [ ] **Step 2: 确认红**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue/src-tauri && cargo test --lib queues:: 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find ... SteeringQueue`

- [ ] **Step 3: 实现**(测试之前):

```rust
//! 双交互队列 — Pi convergence Sprint 2 item 3。
//!
//! `SteeringQueue`:agent 工作期间用户注入,运行 loop 在轮起点排空(mid-run)。
//! `FollowUpQueue`:仅 agent 自然停止(Response)时 OneAtATime 取一个任务注入并重入。
//! 两者 Arc<Mutex> + Clone(克隆共享内部),生产者(Tauri 命令)/消费者(loop)共享。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::agent::types::ChatMessage;

#[derive(Clone, Default)]
pub struct SteeringQueue {
    inner: Arc<Mutex<VecDeque<ChatMessage>>>,
}
impl SteeringQueue {
    pub fn push(&self, msg: ChatMessage) {
        self.inner.lock().unwrap().push_back(msg);
    }
    pub fn drain(&self) -> Vec<ChatMessage> {
        self.inner.lock().unwrap().drain(..).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

#[derive(Clone, Default)]
pub struct FollowUpQueue {
    inner: Arc<Mutex<VecDeque<Vec<ChatMessage>>>>,
}
impl FollowUpQueue {
    pub fn push_task(&self, messages: Vec<ChatMessage>) {
        self.inner.lock().unwrap().push_back(messages);
    }
    pub fn next(&self) -> Option<Vec<ChatMessage>> {
        self.inner.lock().unwrap().pop_front()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}
```

> 用 `std::sync::Mutex`(短临界区,无 await 跨锁)。`lock().unwrap()` 在毒化时 panic — 与 codebase 其它 std Mutex 用法一致(队列无敏感不变量,毒化即 bug)。

- [ ] **Step 4: 注册** — `agent/mod.rs` 加 `pub mod queues;`(near `pub mod turn;`)。

- [ ] **Step 5: 绿 + 提交**

Run: `cd .../src-tauri && cargo test --lib queues:: 2>&1 | tail -6`(`ok. 3 passed`)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add src-tauri/src/agent/queues.rs src-tauri/src/agent/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(agent): SteeringQueue + FollowUpQueue

New agent/queues.rs: SteeringQueue (drain-all, mid-run) + FollowUpQueue
(OneAtATime push_task/next). Arc<Mutex>+Clone for producer/consumer sharing.

Verification: cargo test --lib queues:: -> ok, 3 passed"
```

---

## Task 2: AppState.agent_queues + helpers

**Files:** Modify `src-tauri/src/app.rs`

- [ ] **Step 1: 加类型 + 字段** — 在 app.rs 合适处定义 `AgentQueues`,并给 `AppState` 加字段(near `running_sessions`,~line 261):

```rust
#[derive(Clone, Default)]
pub struct AgentQueues {
    pub steering: crate::agent::queues::SteeringQueue,
    pub follow_up: crate::agent::queues::FollowUpQueue,
}

// AppState struct 内,running_sessions 附近:
pub agent_queues: Arc<std::sync::Mutex<std::collections::HashMap<String, AgentQueues>>>,
```

- [ ] **Step 2: 构造初始化** — 在 `AppState { ... }` 字面量(~line 867,running_sessions 初始化 ~908 附近)加:

```rust
            agent_queues: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
```

- [ ] **Step 3: 加 helper** — 在 `impl AppState`:

```rust
    /// 取(或创建)某 session 的队列对。生产者(Tauri 命令)与消费者(ChatDelegate)经此共享同一对。
    pub fn agent_queues_for(&self, session_id: &str) -> AgentQueues {
        self.agent_queues
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    /// 该 session 是否有活跃 agent run。
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        self.running_sessions.lock().await.contains_key(session_id)
    }
```

- [ ] **Step 4: 编译 + 提交**

Run: `cd .../src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add src-tauri/src/app.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(app): AppState.agent_queues + agent_queues_for + is_session_running

Per-session (steering, follow_up) queues shared between producers (Tauri cmds)
and the agent loop's ChatDelegate via get-or-create. is_session_running checks
running_sessions membership.

Verification: cargo build -> no errors"
```

---

## Task 3: LoopDelegate.persist_user_message + ChatDelegate 钩子

**Files:** Modify `src-tauri/src/agent/types.rs`, `src-tauri/src/agent/dispatcher.rs`

- [ ] **Step 1: trait 加 persist_user_message(default 空)** — types.rs LoopDelegate,在 get_follow_up_messages 后:

```rust
    /// 把注入的 steering/follow-up 用户消息持久化到 agent_messages(重载连续)。默认空(测试 mock)。
    async fn persist_user_message(&self, _msg: &ChatMessage) {}
```

- [ ] **Step 2: ChatDelegate 加字段 + setter** — dispatcher.rs。给 `ChatDelegate` struct 加(default-空,不改 ::new 签名):

```rust
    steering_queue: crate::agent::queues::SteeringQueue,
    follow_up_queue: crate::agent::queues::FollowUpQueue,
    persist_db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
```

在 `ChatDelegate::new` 的返回字面量里给这三个字段默认值:`steering_queue: Default::default(), follow_up_queue: Default::default(), persist_db: None,`。加 setter(链式,与现有 GEP setter 风格一致):

```rust
    pub fn with_agent_queues(mut self, queues: crate::app::AgentQueues, db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        self.steering_queue = queues.steering;
        self.follow_up_queue = queues.follow_up;
        self.persist_db = Some(db);
        self
    }
```

- [ ] **Step 3: 实现钩子** — dispatcher.rs `impl LoopDelegate for ChatDelegate`:

```rust
    async fn get_steering_messages(&self) -> Vec<ChatMessage> {
        self.steering_queue.drain()
    }
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
        self.follow_up_queue.next().unwrap_or_default()
    }
    async fn persist_user_message(&self, msg: &ChatMessage) {
        let Some(db) = &self.persist_db else { return };
        // 取 msg 文本(ChatMessage::user → 单 Text block)
        let text: String = msg.content.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        if let Ok(conn) = db.lock() {
            let _ = conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,'user',?3,?4)",
                rusqlite::params![id, self.conversation_id, text, now],
            );
            let _ = conn.execute(
                "UPDATE agent_sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, self.conversation_id],
            );
        }
    }
```

> `conversation_id == session_id`。`ContentBlock`/`rusqlite`/`uuid`/`chrono` 已在 dispatcher.rs 作用域(用于其它持久化)。

- [ ] **Step 4: 编译 + 提交**

Run: `cd .../src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib dispatcher 2>&1 | tail -6`(现有过)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add src-tauri/src/agent/types.rs src-tauri/src/agent/dispatcher.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(agent): ChatDelegate steering/follow-up drain + persist_user_message

LoopDelegate gains persist_user_message (default no-op). ChatDelegate gets
steering_queue/follow_up_queue/persist_db fields (default empty/None) + a
with_agent_queues setter (agent path only; ::new signature unchanged so chat
mode is unaffected). get_steering_messages drains SteeringQueue;
get_follow_up_messages pops one FollowUpQueue task; persist_user_message mirrors
the agent_messages INSERT.

Verification: cargo build -> no errors; cargo test --lib dispatcher -> ok"
```

---

## Task 4: run_agentic_loop 外层 follow-up loop + steering drain

**Files:** Modify `src-tauri/src/agent/agentic_loop.rs`(高注意力)

- [ ] **Step 1: 读现状** — `run_agentic_loop`(line 443):per-run locals `truncation_count`(448)/`consecutive_tool_intent_nudges`(449)/`pending_patch`(453);`for iteration`(459);steering 注入点 = for 体顶部(snapshot 创建前);run_turn_body match(535-548);prepare_next_turn 边界(556-568);for 结束(569);MaxIterations(576)。`thread_state = Processing`(456)。

- [ ] **Step 2: 包外层 loop + steering drain + follow-up drain** — 把 line 456..576 区间重构为:

```rust
    'followup: loop {
        reason_ctx.thread_state = ThreadState::Processing;        // 原 456
        let mut truncation_count = 0usize;                        // 重置(每 follow-up 新预算)
        let mut consecutive_tool_intent_nudges = 0usize;
        let mut pending_patch: Option<crate::agent::turn::NextTurnPatch> = None;
        let mut completed: Option<LoopOutcome> = None;

        for iteration in 1..=config.max_iterations {
            // 现有 pre-LLM:check_signals / cancel / compress / before_llm(可早 return)—— 保持
            ...

            // —— NEW:steering 轮起点排空 + 持久化 ——
            for m in delegate.get_steering_messages().await {
                reason_ctx.messages.push(m.clone());
                delegate.persist_user_message(&m).await;
            }

            let mut snap = delegate.create_turn_snapshot(reason_ctx, iteration as u32).await;   // ②
            if let Some(p) = pending_patch.take() { snap = crate::agent::turn::apply_patch(snap, p); }
            let current_snapshot = std::sync::Arc::new(snap);

            match run_turn_body(delegate, reason_ctx, &current_snapshot, iteration, config,
                                &mut truncation_count, &mut consecutive_tool_intent_nudges).await {
                TurnFlow::Return(LoopOutcome::Response { text, usage, truncated, model }) => {
                    completed = Some(LoopOutcome::Response { text, usage, truncated, model });
                    break;   // 自然停止 → follow-up 检查
                }
                TurnFlow::Return(other) => return other,           // 终态:跳过 follow-up
                TurnFlow::Continue => {
                    // ② 轮边界 prepare_next_turn(556-568)保持
                    match delegate.prepare_next_turn(reason_ctx, iteration as u32).await { ... pending_patch = ... }
                }
            }
        }

        // 内层 for 结束:Response break(completed=Some)或 max 耗尽(None)
        let outcome = match completed { Some(o) => o, None => return LoopOutcome::MaxIterations };  // 仅 Response 触发 follow-up

        let follow_up = delegate.get_follow_up_messages().await;   // OneAtATime
        if follow_up.is_empty() { return outcome; }
        for m in follow_up {
            reason_ctx.messages.push(m.clone());
            delegate.persist_user_message(&m).await;
        }
        continue 'followup;
    }
```

> 关键:per-run locals 移进外层 loop 体(每 follow-up 重置);`TurnFlow::Return(LoopOutcome::Response{..})` 解构后重建(因 break 需带出 outcome — 用 completed 暂存)。其余 TurnFlow::Return 直接 return(终态不 follow-up)。`run_turn_body`/TurnFlow/prepare_next_turn 逻辑不变(只是被外层 loop 包住 + 改 Response 分支为 break)。

- [ ] **Step 3: 集成测试**(agentic_loop 测试模块,扩展现有 mock delegate 加 SteeringQueue/FollowUpQueue 注入):

```rust
    #[tokio::test]
    async fn steering_injected_at_turn_start() {
        // delegate.get_steering_messages 第1轮返回 [user("STEER")] 然后空;
        // 断言 reason_ctx.messages 在该轮后含 "STEER" + persist_user_message 被调。
    }
    #[tokio::test]
    async fn follow_up_reenters_loop_serially() {
        // delegate.get_follow_up_messages 第一次 natural-stop 返回 [user("F1")],第二次返回 [];
        // call_llm 第一轮返回 Response → drain F1 → 重入 → 第二轮 Response → 队空 → 返回。
        // 断言:loop 处理了 F1(messages 含 F1)、最终返回 Response、follow-up 取了两次(F1 + 空)。
    }
    #[tokio::test]
    async fn terminal_outcome_skips_follow_up() {
        // call_llm 返回触发 Failure/Stopped 的路径 → 断言 get_follow_up_messages 未被调、直接返回终态。
    }
```

(mock 按现有 agentic_loop 测试基建实现 steering/follow-up 计数 + 注入。)

- [ ] **Step 4: 编译 + 测试 + 提交**

Run: `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib agentic_loop:: 2>&1 | tail -12`(全过)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add src-tauri/src/agent/agentic_loop.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(agent): steering drain + outer follow-up loop in run_agentic_loop

Wrap the turn loop in 'followup: loop. Drain SteeringQueue at each turn start
(inject + persist) for true mid-run steering. On natural stop (Response only),
drain FollowUpQueue OneAtATime, append + persist + re-enter (continuous history);
terminal outcomes skip follow-up. Per-run counters reset each follow-up.

Verification: cargo test --lib agentic_loop:: -> ok; cargo build -> no errors"
```

---

## Task 5: agent_steer / agent_follow_up commands + wire queues into ChatDelegate

**Files:** Modify `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: 命令** — tauri_commands.rs(near queue_agent_message):

```rust
#[derive(serde::Deserialize)]
pub struct AgentSteerInput { pub session_id: String, pub user_message: String, #[serde(default)] pub uuid: Option<String> }

#[tauri::command]
pub async fn agent_steer(state: State<'_, AppState>, input: AgentSteerInput) -> Result<(), Error> {
    state.agent_queues_for(&input.session_id).steering.push(crate::agent::types::ChatMessage::user(&input.user_message));
    Ok(())
}

#[tauri::command]
pub async fn agent_follow_up(state: State<'_, AppState>, app_handle: tauri::AppHandle, input: AgentSteerInput) -> Result<(), Error> {
    if state.is_session_running(&input.session_id).await {
        state.agent_queues_for(&input.session_id).follow_up.push_task(vec![crate::agent::types::ChatMessage::user(&input.user_message)]);
        Ok(())
    } else {
        // 空闲 → 普通新 run
        let send_input = SendAgentMessageInput {
            session_id: input.session_id, user_message: input.user_message,
            channel_id: None, model_id: None, workspace_id: None, strategy: None, prompt_id: None,
        };
        send_agent_message(state, app_handle, send_input).await
    }
}
```

> 复用 `AgentSteerInput` 给两命令(同形)。`uuid` 透传给前端 dequeue 同步(命令侧暂不用,保留字段)。

- [ ] **Step 2: send_agent_message 注入 queues+db 到 ChatDelegate** — 在 ChatDelegate::new 之后(~line 11275)链 setter:

```rust
    let agent_queues = state.agent_queues_for(&session_id);   // 注意:state 须在此可用;若已 move 进 spawn,在 spawn 前 clone agent_queues + db
    let db_for_persist = Arc::clone(&state.db);
    // ... 在 spawn 前 clone agent_queues / db_for_persist,move 进闭包 ...
    let mut delegate = ChatDelegate::new(...).with_agent_queues(agent_queues, db_for_persist);
```

> `state.agent_queues_for` 须在 spawn 前调(state 不 move 进闭包);把 `agent_queues` + `db_for_persist` 与现有 pre-spawn clone(llm/tools/...)一起 clone,move 进 spawn。

- [ ] **Step 3: 注册** — main.rs invoke_handler(near queue_agent_message):
```rust
    uclaw_core::tauri_commands::agent_steer,
    uclaw_core::tauri_commands::agent_follow_up,
```

- [ ] **Step 4: 编译 + 提交**

Run: `cargo build 2>&1 | grep -E "^error" | head`(空)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(api): agent_steer / agent_follow_up commands; wire queues into ChatDelegate

agent_steer pushes to the session's SteeringQueue. agent_follow_up pushes to
FollowUpQueue when a run is active, else starts a normal run (send_agent_message).
send_agent_message wires the session's queues + db into ChatDelegate via
with_agent_queues. Registered in main.rs.

Verification: cargo build -> no errors"
```

---

## Task 6: 删除 SoftInterruptQueue(死码)

**Files:** Delete `src-tauri/src/agent/interrupts.rs` + `interrupts_tests.rs`; Modify `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 确认无生产引用** — `grep -rn "SoftInterruptQueue\|interrupts::" src-tauri/src | grep -v "interrupts.rs\|interrupts_tests.rs"`。预期:无(死码)。若有,停下报告(本步以"无外部引用"为前提)。

- [ ] **Step 2: 删除** — `git rm src-tauri/src/agent/interrupts.rs src-tauri/src/agent/interrupts_tests.rs`;`agent/mod.rs` 删 `pub mod interrupts;`。

> 保留 `LoopSignal::InjectMessage`(harmless;steering 现走 get_steering_messages)——不在本任务动 LoopSignal/agentic_loop(已 Task 4 改完,避免再触)。

- [ ] **Step 3: 编译 + 提交**

Run: `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib 2>&1 | tail -5`(无新失败)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "chore(agent): delete dead SoftInterruptQueue

SoftInterruptQueue (interrupts.rs + tests) was never wired into any production
path; superseded by SteeringQueue. Removed. LoopSignal::InjectMessage left as a
harmless unused variant.

Verification: cargo build -> no errors; cargo test --lib -> no new failures"
```

---

## Task 7: 前端 引导 UI 重接线

**Files:** Modify `ui/src/lib/tauri-bridge.ts`, `ui/src/components/agent/AgentView.tsx`

- [ ] **Step 1: bridge fns** — tauri-bridge.ts(near queueAgentMessage:1645):

```typescript
export const agentSteer = (input: { sessionId: string; userMessage: string; uuid?: string }): Promise<void> =>
  invoke<void>('agent_steer', { input: { session_id: input.sessionId, user_message: input.userMessage, uuid: input.uuid } })

export const agentFollowUp = (input: { sessionId: string; userMessage: string; uuid?: string }): Promise<void> =>
  invoke<void>('agent_follow_up', { input: { session_id: input.sessionId, user_message: input.userMessage, uuid: input.uuid } })
```

- [ ] **Step 2: 引导按钮 → agent_steer** — AgentView.tsx `handleSteerQueued`(~1199):把
```typescript
queueAgentMessage({ sessionId, userMessage: msg.text, uuid: localUuid, interrupt: true })
```
换成
```typescript
agentSteer({ sessionId, userMessage: msg.text, uuid: localUuid })
```
(其余:从 atom 移除该条、注入 synthetic 消息、设 running —— 保留不变。)

- [ ] **Step 3: 排队时推 follow-up + 移除 completion-flush** —
  a. enqueue 路径(~898,composer submit while streaming):在 `enqueueAgentMessage(...)` 之后,加 `agentFollowUp({ sessionId, userMessage: effectiveText, uuid: <该条 id> })`(后端 FollowUpQueue;运行 loop 外层 drain)。
  b. 删除 completion-flush effect(~1235-1276)整段(后端外层 loop 接管串行化;不再前端逐条新起 run)。

- [ ] **Step 4: banner dequeue 同步** — follow-up 被后端处理后会作为 user turn 出现在 live 流。在现有处理 incoming agent 消息的地方(live messages 更新),当收到含某 queued 项 uuid 的 user 消息时,`removeQueuedMessage(prev, sessionId, id)` 从 banner atom 移除。
  > 若 uuid 回传链路不易接(后端 persist 用新 uuid),退化方案:enqueue 时 `agentFollowUp` 后即乐观从 banner 移除并加一个"已提交,待处理"轻提示。实现者按 live 流是否带 uuid 选其一,并在 PR 说明取舍。

- [ ] **Step 5: 类型检查 + 测试 + 提交**

Run: `cd .../ui && npx tsc --noEmit 2>&1 | head` (无新错误);`npm test -- --run AgentView QueuedMessagesBanner 2>&1 | tail -10`(若有相关测试;否则跳过,以 tsc 为准)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue add ui/src/lib/tauri-bridge.ts ui/src/components/agent/AgentView.tsx
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-dualqueue commit -m "feat(ui): rewire 引导 UI to agent_steer / agent_follow_up

引导 button -> agentSteer (true mid-run inject, no loop orphan). Queued messages
-> agentFollowUp on enqueue (backend FollowUpQueue owns serialization). Remove
the per-completion auto-flush effect. Banner dequeues by uuid when the follow-up
lands as a user turn. Reuses QueuedMessagesBanner UI unchanged.

Verification: npx tsc --noEmit -> no new errors"
```

---

## 最终验收

- [ ] 后端编译:`cargo build 2>&1 | grep -E "^error" | head` → 空
- [ ] 后端单测:`cargo test --lib "queues::" "agentic_loop::" "dispatcher" 2>&1 | tail -15` → 全过;全量无新失败
- [ ] 前端:`cd ui && npx tsc --noEmit 2>&1 | head` → 无新错误
- [ ] 手动 smoke(`cargo tauri dev`):agent 跑长任务时,(a) composer 输入 → banner;点 引导 → 运行中的 agent 在下一轮看到该消息(无 orphan 新 loop);(b) 排队一条不点引导 → agent 自然完成后串行处理它(同一会话连续历史)。

---

## Self-Review

**Spec coverage:** §3.1 queues→Task1;§3.2 AppState→Task2;§3.3 ChatDelegate 钩子+persist→Task3;§3.4 loop→Task4;§4 命令→Task5;§5 前端→Task7;§6 删 SoftInterruptQueue→Task6;§7 错误边界→Task4(终态跳过)/Task5(空闲起 run)/Task1(Mutex);§8 测试→Task1/4/7。

**Placeholder scan:** Task4 Step2 用 `...` 标"现有 pre-LLM 保持"/"prepare_next_turn 保持"——是"保留现有代码"指令(实现者 READ line 456-576 后包外层),非占位。Task7 Step4 给了 dequeue 两条具体路径(uuid 匹配 / 乐观移除+提示),非模糊。无 TBD/TODO。

**Type consistency:** `SteeringQueue{push/drain/is_empty}`/`FollowUpQueue{push_task/next/is_empty}`(Task1)→ AgentQueues(Task2)→ ChatDelegate 字段+钩子(Task3)→ loop drain(Task4)→ 命令 push(Task5)一致;`agent_queues_for(session_id)->AgentQueues` / `is_session_running` 一致;`persist_user_message(&ChatMessage)`(Task3 trait + impl)→ Task4 调用一致;`agent_steer`/`agent_follow_up`(Task5)↔ `agentSteer`/`agentFollowUp`(Task7)命令名+参数(session_id/user_message/uuid)一致。
