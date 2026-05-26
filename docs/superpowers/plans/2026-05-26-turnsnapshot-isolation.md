# TurnSnapshot 隔离 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 引入不可变每轮快照 `Arc<TurnSnapshot>` + 轮边界机制(prepare_next_turn/NextTurnPatch),形式化 uClaw agent loop 的轮边界(为 item ③ 双队列打基础),并为未来 hot-swap 预留隔离。

**Architecture:** 新 `agent/turn.rs` 持 `TurnSnapshot`/`NextTurnPatch`/`apply_patch`。`LoopDelegate` 加 `create_turn_snapshot`(default 从 reason_ctx;ChatDelegate override 把 prompt+tools 组装从 call_llm 搬来)+ `prepare_next_turn`/`get_steering`/`get_follow_up`(default)。`call_llm` 签名加 `snapshot`,改用 snapshot.{system_prompt,model,tools}。loop 每轮建 `Arc<TurnSnapshot>`、传给 call_llm(隔离)、轮边界应用 patch。

**Tech Stack:** Rust + Tokio + async_trait,现有 agentic_loop / ChatDelegate / stream_completion。

**Spec:** `docs/superpowers/specs/2026-05-26-turnsnapshot-isolation-design.md`
**Branch/worktree:** `codex/pi-sprint2-turnsnapshot`(base = main `07647d6a`)

---

## ⚠️ 规划期发现(实现以本计划为准)

1. **`create_turn_snapshot` 给 default** —— 8 个测试 delegate 继承 default(从 `reason_ctx.system_prompt` + 空 tools),只有 `ChatDelegate` override 真实组装。减少 blast radius。
2. **`call_llm` 签名变更波及 9 个 impl**(1 ChatDelegate + 8 测试),`call_llm` 仅在 `agentic_loop.rs:130` 调用(全包含)。Task 3 为原子提交(签名 + 调用点 + 9 impl 同改)。
3. **`effective_system_prompt(&self, &SafetyMode) -> String` 是 sync 且有内部可变副作用**(`is_first_act_turn`/`last_injected_fragments`/`compose_stats_collector`,经 Atomic/Mutex)。搬到 `create_turn_snapshot(&self)` 频率不变(每轮一次)→ 行为等价。真实组装更广:`resolve_effective_mode().await` → `effective_system_prompt(&mode)` → GEP gene 追加 → plan-suggest 提示 → project rules → `cache_align::pad_to_ladder` → tools(`self.tools.list_definitions()` 归一化)。`ChatDelegate::call_llm` 经 `llm_stream::stream_completion(self.llm.as_ref(), messages, tools, &config, self, timeout)` 调 LLM(非 `complete` 直调)。

---

## 验证命令

- 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空=通过)
- 单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- bundled 资源已 symlink;`cargo test -p uclaw_core` 不工作,用 `cargo test`。已知预存失败 ~5(daemon_approval / truncate_for_error / 2× browser::runtime_status / gbrain_eval_harness)——区分新失败。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `agent/turn.rs` | **新建** — `TurnSnapshot` / `NextTurnPatch` / `apply_patch` |
| `agent/mod.rs` | `pub mod turn;` |
| `agent/types.rs` | `LoopDelegate` +4 方法(全 default);`call_llm` 签名加 `snapshot` |
| `agent/dispatcher.rs` | `ChatDelegate::create_turn_snapshot`(搬组装)+ `call_llm` 改用 snapshot(高注意力) |
| `agent/agentic_loop.rs` | 每轮建 Arc<TurnSnapshot> + 传 call_llm + 轮边界 patch(高注意力);测试 mock 适配 |
| `regular_task.rs` / `regular_task_pr4_tests.rs` / `rollout_integration.rs` | 测试 delegate 的 call_llm 签名更新(继承 create_turn_snapshot default) |

任务:turn.rs(自包含)→ trait default 方法(additive)→ ChatDelegate 搬组装 + call_llm 签名(原子,9 impl)→ loop Arc + 轮边界。

---

## Task 1: agent/turn.rs(TurnSnapshot / NextTurnPatch / apply_patch)

**Files:** Create `src-tauri/src/agent/turn.rs`;Modify `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 写失败测试** — 新建 `turn.rs`,先只写测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn snap() -> TurnSnapshot {
        TurnSnapshot {
            turn_index: 1,
            model: "m1".into(),
            system_prompt: Arc::new("sys".into()),
            tools: Arc::new(vec![]),
            force_text: false,
        }
    }

    #[test]
    fn apply_patch_overrides_model_and_bumps_turn_index() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() });
        assert_eq!(patched.model, "m2");
        assert_eq!(patched.turn_index, 2);
        assert_eq!(*patched.system_prompt, "sys"); // unchanged
    }

    #[test]
    fn apply_patch_none_fields_keep_original() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch::default());
        assert_eq!(patched.model, "m1");
        assert_eq!(patched.turn_index, 2);
    }

    #[test]
    fn arc_snapshot_isolation() {
        // 持有旧 Arc,替换 outer,旧 clone 内容不变
        let outer = Arc::new(snap());
        let held = Arc::clone(&outer);
        let _replaced = Arc::new(apply_patch((*outer).clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() }));
        assert_eq!(held.model, "m1"); // in-flight 持有者不受影响
    }
}
```

- [ ] **Step 2: 确认红**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo test --lib turn:: 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find ... TurnSnapshot` / `apply_patch`

- [ ] **Step 3: 实现**(测试之前):

```rust
//! 每轮不可变配置快照 + 轮边界补丁 — Pi convergence Sprint 2 item 2。
//!
//! `TurnSnapshot` 冻结一轮的 model + 组装好的 system_prompt + tools。以 Arc 共享:
//! in-flight 的 call_llm 持有自己的 Arc clone,轮边界替换不影响它。

use std::sync::Arc;

use crate::agent::types::{ChatMessage, ToolDefinition};

/// 一轮(一次 loop 迭代)的不可变配置快照。
#[derive(Clone, Debug)]
pub struct TurnSnapshot {
    pub turn_index: u32,
    pub model: String,
    pub system_prompt: Arc<String>,
    pub tools: Arc<Vec<ToolDefinition>>,
    pub force_text: bool,
}

/// 轮边界对下一轮的补丁(显式配置变更/注入/停止)。
/// item ② 暂无生产者(prepare_next_turn 默认 None);为 Sprint 4 hot-swap + item ③ 注入预留。
#[derive(Default, Debug)]
pub struct NextTurnPatch {
    pub model: Option<String>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub inject_message: Option<ChatMessage>,
    pub should_stop: bool,
}

/// 把补丁应用到下一轮快照(turn_index +1)。inject_message/should_stop 由 loop 处理。
pub fn apply_patch(mut snap: TurnSnapshot, patch: NextTurnPatch) -> TurnSnapshot {
    if let Some(m) = patch.model {
        snap.model = m;
    }
    if let Some(t) = patch.tools {
        snap.tools = Arc::new(t);
    }
    snap.turn_index += 1;
    snap
}
```

> 注:`ToolDefinition` 来自 `crate::agent::types`(re-export uclaw_tool_types)。`ChatMessage` 同。若 `ToolDefinition` 无 `Clone`/`Debug`,`TurnSnapshot` derive 相应裁剪(它已是 `#[derive(Clone)]` 的 plain struct,应可 Clone)。

- [ ] **Step 4: 注册** — `agent/mod.rs` 加 `pub mod turn;`(near `pub mod compaction;`)。

- [ ] **Step 5: 确认绿**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo test --lib turn:: 2>&1 | tail -6`
Expected: `test result: ok. 3 passed`

- [ ] **Step 6: 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot add src-tauri/src/agent/turn.rs src-tauri/src/agent/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot commit -m "feat(agent): TurnSnapshot + NextTurnPatch + apply_patch

New agent/turn.rs: immutable per-turn config snapshot (Arc-shared for in-flight
isolation) + NextTurnPatch + apply_patch (model/tools override, turn_index bump).
Pi Sprint 2 item 2 foundation.

Verification: cargo test --lib turn:: -> ok, 3 passed"
```

---

## Task 2: LoopDelegate +4 方法(全 default,additive)

**Files:** Modify `src-tauri/src/agent/types.rs`

- [ ] **Step 1: 加 4 个 default 方法** — 在 `LoopDelegate` trait(types.rs:348,`#[async_trait::async_trait]`)末尾(`update_fold_incremental` 之后)加:

```rust
    /// 为即将开始的一轮创建不可变快照。默认从 reason_ctx 取(测试 delegate 用);
    /// ChatDelegate override 为真实组装(model + effective_system_prompt + tools)。
    async fn create_turn_snapshot(
        &self,
        reason_ctx: &ReasoningContext,
        turn_index: u32,
    ) -> crate::agent::turn::TurnSnapshot {
        crate::agent::turn::TurnSnapshot {
            turn_index,
            model: String::new(),
            system_prompt: std::sync::Arc::new(reason_ctx.system_prompt.clone()),
            tools: std::sync::Arc::new(Vec::new()),
            force_text: reason_ctx.force_text,
        }
    }

    /// 轮边界钩子:返回对下一轮的补丁。默认 None(item ② 无生产者)。
    async fn prepare_next_turn(
        &self,
        _reason_ctx: &ReasoningContext,
        _turn_index: u32,
    ) -> Option<crate::agent::turn::NextTurnPatch> {
        None
    }

    /// 自然停止点钩子,item ③(双队列)填充。默认空。
    async fn get_steering_messages(&self) -> Vec<ChatMessage> {
        Vec::new()
    }
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
        Vec::new()
    }
```

> 这一步是纯 additive(全 default body)——不改任何 impl,直接编译通过。

- [ ] **Step 2: 编译确认**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出(additive default,无 impl 改动)

- [ ] **Step 3: 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot add src-tauri/src/agent/types.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot commit -m "feat(agent): LoopDelegate turn-boundary methods (defaults)

Add create_turn_snapshot (default from reason_ctx), prepare_next_turn (default
None), get_steering_messages / get_follow_up_messages (default empty, item 3 fills)
to LoopDelegate. Purely additive — all defaults, no impl changes.

Verification: cargo build -> no errors"
```

---

## Task 3: ChatDelegate 搬组装 + call_llm 消费 snapshot(原子,9 impl)

**Files:** Modify `dispatcher.rs`, `agentic_loop.rs`(call 点 + 测试 mock), `regular_task.rs`, `regular_task_pr4_tests.rs`, `rollout_integration.rs`

这是最大的机械改动:`call_llm` 签名加 `snapshot: &TurnSnapshot`,9 个 impl 同步;ChatDelegate override `create_turn_snapshot` 把组装搬来、`call_llm` 改用 snapshot。**必须原子提交**(签名变更不可拆)。

- [ ] **Step 1: 改 trait `call_llm` 签名** — types.rs LoopDelegate:

```rust
    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        snapshot: &crate::agent::turn::TurnSnapshot,
        iteration: usize,
    ) -> Result<RespondOutput, Error>;
```

- [ ] **Step 2: ChatDelegate override create_turn_snapshot** — dispatcher.rs。把现有 `call_llm` 中的 prompt+tools 组装(resolve_effective_mode → effective_system_prompt → GEP 追加 → plan-suggest → rules → pad_to_ladder → tools)抽到这里。**先 READ ChatDelegate::call_llm 当前 prompt 组装段(约 dispatcher.rs:1803-2021)**,把"系统 prompt 组装"与"tools 组装"两段原样搬入:

```rust
    async fn create_turn_snapshot(&self, reason_ctx: &ReasoningContext, turn_index: u32)
        -> crate::agent::turn::TurnSnapshot
    {
        // —— 系统 prompt 组装(从 call_llm 原样搬来)——
        let effective_mode = self.resolve_effective_mode().await;
        let effective_prompt = self.effective_system_prompt(&effective_mode);
        let mut full_system_prompt = effective_prompt;
        // GEP gene 追加(若 gene_retriever 有)…… [原 call_llm 对应段]
        // plan-suggest 提示追加(若 db 有 + rejection rate < 20%)…… [原段]
        // project rules 追加(RuleContextBuilder)…… [原段]
        let full_system_prompt = crate::agent::compact::cache_align::pad_to_ladder(full_system_prompt);
        // —— tools 组装(从 call_llm 原样搬来)——
        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            // self.tools.list_definitions() + L2 归一化 [原段]
            self.tools.list_definitions() /* + normalize */
        };
        crate::agent::turn::TurnSnapshot {
            turn_index,
            model: self.model.clone(),
            system_prompt: std::sync::Arc::new(full_system_prompt),
            tools: std::sync::Arc::new(tools),
            force_text: reason_ctx.force_text,
        }
    }
```

> **核对点**:这些组装段当前在 call_llm 里;`effective_system_prompt` 的副作用(is_first_act_turn / last_injected_fragments / compose_stats_collector,内部可变)随之移到 create_turn_snapshot,**频率不变(每轮一次)**,行为等价。`create_turn_snapshot` 取 `&self`(副作用走 Atomic/Mutex,OK)+ `&reason_ctx`(只读)。

- [ ] **Step 3: ChatDelegate call_llm 改用 snapshot** — 删掉搬走的组装段,改用 `snapshot.system_prompt` / `snapshot.model` / `snapshot.tools`:

```rust
    async fn call_llm(&self, reason_ctx: &mut ReasoningContext, snapshot: &crate::agent::turn::TurnSnapshot, iteration: usize)
        -> Result<RespondOutput, Error>
    {
        self.chunk_seq.store(0, Ordering::Relaxed); self.thinking_seq.store(0, Ordering::Relaxed);
        self.beat(/* LLM_CALL */);
        // system msg 用冻结的 snapshot.system_prompt
        let mut messages: Vec<ChatMessage> = vec![ChatMessage { role: MessageRole::System,
            content: vec![ContentBlock::Text { text: (*snapshot.system_prompt).clone() }], compacted: false }];
        // 追加 non-compacted reason_ctx.messages —— [原 call_llm 的 messages 组装段:audit_chat_history / image strip / build_dynamic_context prepend 保留]
        // …… [原 step 9-12 保留]
        let tools = (*snapshot.tools).clone();
        let config = crate::llm::CompletionConfig {
            model: snapshot.model.clone(),
            max_tokens: /* 原值 */,
            temperature: 0.7,
            thinking_enabled: self.thinking_enabled,
        };
        let stream_idle_timeout = /* 原解析 */;
        crate::agent::llm_stream::stream_completion(self.llm.as_ref(), messages, tools, &config, self, stream_idle_timeout).await
    }
```

> 保留 call_llm 里**消息组装**(steps 9-12:audit/image-strip/dynamic-context)——它们依赖 live `reason_ctx.messages`,不进 snapshot。只把 **system-prompt 组装 + tools 组装 + model** 换成 snapshot。

- [ ] **Step 4: loop 调用点改造** — agentic_loop.rs:130。在 call_llm 之前建 snapshot:

```rust
        // 轮起点建快照(Task 4 会改成 Arc + 轮边界;本任务先建 local 并传入)
        let snapshot = delegate.create_turn_snapshot(reason_ctx, iteration as u32).await;
        let output = match delegate.call_llm(reason_ctx, &snapshot, iteration).await {
            Ok(output) => output,
            Err(e) => { /* 原 ThreadState::Failed + return Failure 不变 */ }
        };
```

- [ ] **Step 5: 更新 8 个测试 delegate 的 call_llm 签名** — 逐个加 `_snapshot: &crate::agent::turn::TurnSnapshot` 参数(它们忽略 snapshot,继承 create_turn_snapshot default)。文件 + 类型(grep 定位):

```bash
grep -rn "async fn call_llm" /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri/src/agent/agentic_loop.rs /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri/src/agent/regular_task.rs /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri/src/agent/regular_task_pr4_tests.rs /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri/src/agent/rollout_integration.rs
```
对每个(`CountingDelegate`、`ImmediateTextDelegate`、`ResponseWithUsageDelegate`、`FailingDelegate`、`ReasoningResponseDelegate`、`CancelFromInsideCallLlmDelegate`、`NeedApprovalDelegate`、`OneShotResponseDelegate`):把签名改为
```rust
    async fn call_llm(&self, reason_ctx: &mut ReasoningContext, _snapshot: &crate::agent::turn::TurnSnapshot, iteration: usize) -> Result<RespondOutput, Error> {
```
方法体不变。

- [ ] **Step 6: 编译 + 回归**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(无输出)
Run: `cargo test --lib agentic_loop:: regular_task:: 2>&1 | tail -12`(全过——行为等价:snapshot 每轮建一次 = 原组装频率)
Expected: 编译无 error;现有 loop/task 测试通过

- [ ] **Step 7: 加 create_turn_snapshot 单测**(ChatDelegate 太重则测 default 行为 / 或断言 loop 集成里 call_llm 收到的 snapshot 与 create 产出一致——放 Task 4)。本步至少加一个 turn-snapshot default 行为测试(在 agentic_loop 测试模块,用现有 CountingDelegate 调 create_turn_snapshot,断言 system_prompt == reason_ctx.system_prompt)。

- [ ] **Step 8: 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot commit -m "feat(agent): call_llm consumes TurnSnapshot; move prompt+tools assembly to create_turn_snapshot

ChatDelegate::create_turn_snapshot now does the system-prompt assembly (mode +
effective_system_prompt + GEP + plan-suggest + rules + pad) and tool assembly
that call_llm used to do per-call; call_llm consumes snapshot.{system_prompt,
model,tools}. call_llm signature gains &TurnSnapshot (9 impls updated; only
called from the loop). Loop builds the snapshot at iteration start and passes it.
Behavior-equivalent: snapshot built once per iteration = old assembly frequency.

Verification: cargo build -> no errors; cargo test --lib agentic_loop:: regular_task:: -> ok"
```

---

## Task 4: Arc 隔离 + 轮边界 patch(prepare_next_turn / apply_patch / should_stop)

**Files:** Modify `agentic_loop.rs`(高注意力)

- [ ] **Step 1: 改 loop 用 Arc + pending_patch + 轮边界** — agentic_loop.rs。把 Task 3 的 `let snapshot = ...; call_llm(&snapshot)` 升级:

在 `for iteration` 之前:
```rust
    let mut pending_patch: Option<crate::agent::turn::NextTurnPatch> = None;
```
轮起点(替换 Task 3 的 snapshot 建立):
```rust
        let mut snap = delegate.create_turn_snapshot(reason_ctx, iteration as u32).await;
        if let Some(p) = pending_patch.take() { snap = crate::agent::turn::apply_patch(snap, p); }
        let current_snapshot = std::sync::Arc::new(snap);
        // call_llm 传 &*current_snapshot(in-flight 持 Arc clone 的引用,隔离)
        let output = match delegate.call_llm(reason_ctx, &current_snapshot, iteration).await { ... };
```
轮尾(在该 iteration 所有 `continue`/正常推进之前的统一收尾点——即 `after_iteration` 之后、下一轮之前)加轮边界:
```rust
        // 轮边界:取下一轮补丁
        match delegate.prepare_next_turn(reason_ctx, iteration as u32).await {
            Some(p) if p.should_stop => { /* 设 ThreadState::Completed */ break; }
            Some(p) => {
                if let Some(m) = &p.inject_message { reason_ctx.messages.push(m.clone()); }
                pending_patch = Some(p);
            }
            None => {}
        }
```

> **借用检查器**:`call_llm` 取 `&current_snapshot`(`&Arc` deref 成 `&TurnSnapshot`)+ `&mut reason_ctx` —— 两者不冲突(snapshot 与 reason_ctx 是不同变量)。prepare_next_turn 取 `&reason_ctx`(只读),在 call_llm 之后,无冲突。**若**出现跨 `continue` 的借用问题,把 LLM-调用+RespondOutput-处理提取为 `run_inner_turn(delegate, reason_ctx, &current_snapshot, iteration) -> InnerOutcome`(返回枚举,外层据此 return/continue/break),如 spec §3.3。否则内联即可(uClaw 是单层 for-loop,通常无需提取)。
>
> **轮边界放置**:现有每个 `continue` 分支前都 `after_iteration`。轮边界 `prepare_next_turn` 应在**每轮推进到下一迭代前**统一执行一次。最稳妥:把各 `continue` 改为先跑轮边界再 `continue`,或用一个 `'turn:` loop label + 在循环体末尾统一收尾。实现期按现有 continue 点的数量选最小侵入方式(若 continue 点多,提取 run_inner_turn 返回 InnerOutcome 让收尾集中——推荐)。

- [ ] **Step 2: 加 PatchingDelegate 集成测试** — agentic_loop.rs 测试模块。基于现有 CountingDelegate 扩展(或新 mock):
  - `create_turn_snapshot` 记录调用次数 + 每次的 turn_index;断言每轮调一次、turn_index 递增。
  - call_llm 记录收到的 `snapshot.model`;mock 的 prepare_next_turn 在 iteration 1 返回 `NextTurnPatch { model: Some("swapped".into()), ..default }`;断言 **iteration 2** 的 call_llm 收到 `snapshot.model == "swapped"`,iteration 1 不受影响(hot-swap 机制验证)。
  - mock 的 prepare_next_turn 返回 `should_stop: true` → 断言 loop 在该轮后终止。

```rust
    #[tokio::test]
    async fn patch_applies_at_next_turn_boundary() {
        // PatchingDelegate:iteration 1 的 prepare_next_turn 返回 model=Some("swapped");
        // call_llm 记录每轮 snapshot.model 到 Vec;
        // 跑 run_agentic_loop(限 2-3 轮,call_llm 返回 ToolCalls→continue 推进);
        // 断言 models == ["", "swapped", ...](第2轮起 swapped)。
        // (mock 细节按现有 agentic_loop 测试基建实现。)
    }

    #[tokio::test]
    async fn should_stop_terminates_loop() {
        // prepare_next_turn 返回 should_stop=true → loop break;断言只跑了 1 轮。
    }
```

- [ ] **Step 2 验证(确认红→绿):** 先确认新测试在未接线时失败,接线后通过。

- [ ] **Step 3: 编译 + 全 loop 测试**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(无输出)
Run: `cargo test --lib agentic_loop:: 2>&1 | tail -15`(含新 2 测试 + 现有全过)

- [ ] **Step 4: 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-turnsnapshot commit -m "feat(agent): Arc<TurnSnapshot> isolation + turn-boundary patch

run_agentic_loop wraps each turn's snapshot in Arc (in-flight call_llm holds an
Arc-ref, unaffected by boundary swaps) and applies NextTurnPatch from
prepare_next_turn at the turn boundary (model/tools hot-swap staged for next turn;
should_stop breaks; inject_message pushed). prepare_next_turn defaults None ->
behavior-equivalent. PatchingDelegate test verifies patch applies at NEXT turn.

Verification: cargo test --lib agentic_loop:: -> ok; cargo build -> no errors"
```

---

## 最终验收

- [ ] 编译:`cargo build 2>&1 | grep -E "^error" | head` → 无输出
- [ ] 相关单测:`cargo test --lib "turn::" "agentic_loop::" "regular_task::" 2>&1 | tail -15` → 全过
- [ ] 全 lib 测试无新失败:`cargo test --lib 2>&1 | tail -5`(对比 ~5 预存)
- [ ] 行为等价确认:现有 agentic_loop/regular_task 测试全绿(单/多轮、tool calls、cancellation、approval 路径未变)。

---

## Self-Review

**Spec coverage:** §3.1 turn.rs → Task 1 ✓;§3.2 LoopDelegate 方法 → Task 2(defaults)+ Task 3(call_llm sig)✓;§4 create_turn_snapshot 搬组装 → Task 3 ✓;§5 call_llm 消费 snapshot → Task 3 ✓;§3.3 Arc + 双层/run_inner_turn + 轮边界 → Task 4 ✓;§6 patch/should_stop/③ stub → Task 2(stub)+ Task 4(patch)✓;§7 错误边界 → Task 3(call_llm Err 不变)/Task 4(should_stop/None)✓;§8 测试 → Task 1/3/4 ✓。

**Placeholder scan:** Task 3 Step 2/3 用 `[原段]` 标注"从现有 call_llm 原样搬来"——非占位,是"先 READ 现有代码再搬"的明确指令(组装段太长不内联全文,实现者按 dispatcher.rs:1803-2021 搬)。无 TBD/TODO。Task 4 Step 1 的轮边界放置给了两条具体路径(改 continue 点 / 提取 run_inner_turn),非模糊。

**Type consistency:** `TurnSnapshot { turn_index:u32, model:String, system_prompt:Arc<String>, tools:Arc<Vec<ToolDefinition>>, force_text:bool }`(Task 1)→ LoopDelegate default(Task 2)→ ChatDelegate override(Task 3)→ loop Arc(Task 4)一致;`call_llm(&self, &mut reason_ctx, &TurnSnapshot, usize)`(Task 3)9 impl 一致;`apply_patch(TurnSnapshot, NextTurnPatch) -> TurnSnapshot`(Task 1)→ Task 4 一致;`NextTurnPatch { model, tools, inject_message, should_stop }` 一致。
