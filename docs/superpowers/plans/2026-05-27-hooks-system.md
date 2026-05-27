# Hooks 系统(决策门控基底)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Hook 系统做成可用的双向决策门控基底:共享 bus 注册真实订阅者(`PolicySpecSubscriber`),`PreToolUse` 接 `dispatch_with_decision`(veto/AskUser),点亮高价值死事件(LLM / permission / task)供观测。用现有 `HookDecision`,不做 modify。

**Architecture:** `PolicySpecSubscriber`(policy_eval)把 5 个 decision 事件→`ActionRequest`→`evaluate`→`HookDecision`;`AppState::new` pre-register 到 `Arc<HookBus>`(冻结、无锁);`ToolDispatcher::run_one` 用 `dispatch_with_decision(PreToolUse)` 复用既有 Rejected/pending_approvals 路径;LLM/permission/task 事件 observe-only 点亮。

**Tech Stack:** Rust + Tokio,`async_trait`,Tauri。

**Spec:** `docs/superpowers/specs/2026-05-27-hooks-system-design.md`
**Branch/worktree:** `codex/sprint3-hooks`(base = main `2fb57c3d`)

---

## ⚠️ 规划期事实(实现以此为准)

- `HookBus`(`agent/hook_bus/bus.rs`):`Vec<Arc<dyn HookSubscriber>>` 无内部可变;`new()`、`register(&mut self, Arc<dyn HookSubscriber>)->Result<(),BusError>`、`subscriber_count(&self)->usize`、`async dispatch_observe(&self,&HookEvent)`、`async dispatch_with_decision(&self,&HookEvent)->HookDecision`(首 Deny 胜→首 AskUser→Allow)。`mod.rs` re-export `HookBus / HookSubscriber / SubscriberId / HookEvent / HookEventKind`。
- `HookDecision`(`crates/uclaw-runtime-contracts/src/lib.rs:573`):`Allow | Deny{reason:String} | AskUser{prompt:String, risk_class:Option<RiskClass>}`。
- `RiskClass`(同 crate :85):`Low|Medium|High|Restricted`(`Copy`)。
- `HookSubscriber`(`subscriber.rs:42`):`fn id()->SubscriberId`、`fn interest_in()->&'static [HookEventKind]`、`async fn on_event(&self,&HookEvent)->Option<HookDecision>`。`SubscriberId::new(impl Into<String>)`。
- `HookEvent`(`event.rs`):`PreToolUse{task_id,tool_name,args_json}`、`PreLlmCall{task_id,provider,model,prompt_tokens_estimate:usize}`、`PostLlmCall{task_id,provider,model,input_tokens:u64,output_tokens:u64}`、`PrePermission{task_id,action,target}`、`PostPermission{task_id,action,granted:bool}`、`TaskStart{task_id,intent_id}`、`TaskEnd{task_id,outcome}`、`MemoryWrite{task_id,topic,size_bytes}`、`PreContextInject{..}`、`MemoryRecall{..}`、`Checkpoint{..}`、`PostToolUse{..}`、`PostContextInject{..}`。`HookEventKind::{PreToolUse,PreLlmCall,PrePermission,PreContextInject,MemoryWrite,...}`。
- `PolicySpec`(`policy_eval/spec.rs`):`struct{rules:Vec<PolicyRule>}`,`new()`,`with_rule(PolicyRule)->Self`。`PolicyRule::new(id, MatchPattern, outcome:HookDecision)`。`evaluate<'a>(&'a PolicySpec,&ActionRequest)->(HookDecision,Option<&'a str>)`(首匹配→Allow)。`ActionRequest::new(action_class:impl Into<String>, target:impl Into<String>, risk:RiskClass)`。`MatchPattern::{AnyTarget{action_class}, ExactTarget{action_class,target}, TargetPrefix{action_class,target_prefix}, AtLeastRisk{risk}}`。`policy_eval/mod.rs:27` re-export `evaluate/ActionRequest/MatchPattern/PolicyRule/PolicySpec`。
- `AppState`(`app.rs`):field `pub hook_bus: std::sync::Arc<crate::agent::hook_bus::HookBus>`(:372);构造 `hook_bus: std::sync::Arc::new(crate::agent::hook_bus::HookBus::new())`(:958);`pub fn new(app_handle:&tauri::AppHandle)->Result<Self, crate::error::Error>`(:378,**fallible**,可用 `?`)。
- `ToolDispatcher<R>`(`tool_dispatch/mod.rs`):`pub(crate) hook_bus: Arc<HookBus>`(:74)。`run_one`(157-239):`resolve→approve(174)→gate_paths(193)→emit_tool_start(216)→PreToolUse dispatch_observe(219-223)→run_tool→PostToolUse dispatch_observe(233-238)`。`emit_tool_start`(375-405)。`approve()`(663-775):`ApprovalDecision::{AutoApprove, RequireApproval{reason}, Block{reason}}`;`RequireApproval` → `let rx=self.pending_approvals.register(tc.id.clone()); emit "agent:need_approval"{toolName,toolId,arguments,reason,sessionId,riskLevel:"medium",timestamp}; rx.await → ApprovalResult{approved,always_allow,..}; !approved→emit "agent:tool-rejected"{toolName,toolCallId,timestamp}+Rejected; always_allow→mgr.add_auto_approved`。`ApprovalGate::{Allow, Rejected{reason:String,message:String}}`(11-20)。`PendingApprovals::register(String)->oneshot::Receiver<ApprovalResult>`(app.rs:46-72);`ApprovalResult{approved:bool,always_allow:bool,tool_name:Option<String>,path_scope:Option<String>,paths:Option<Vec<String>>}`。
- `ChatDelegate`(`dispatcher.rs`):**有** `hook_bus: Arc<HookBus>`(:219)、`provider:String`(:284)、`model:String`(:30)、`conversation_id:String`(:42)。`call_llm`(2030-2215):请求前算 `sys_tok/tool_tok/msg_tok`(u32,~2169-2193),`stream_completion`(~2206);`on_usage`(回调,~2641)收 `TokenUsage{input_tokens:u32,output_tokens:u32,...}`。
- `MemoryPolicyExecutor`(`memory_policy/executor.rs:22-86`):field `hook_bus: HookBus`(:23,**裸值**);`new(hook_bus:HookBus, gbrain, memu, browser_artifact)`、`with_real_gbrain_and_artifacts(hook_bus:HookBus, ...)`、`for_tests_allow_all()`(`HookBus::new()`)、`for_tests_deny_all()`(`let mut bus=HookBus::new(); bus.register(Arc::new(DenyMemoryWrites))`)。`gate_write` 用 `self.hook_bus.dispatch_with_decision`(:138)。**production 无 `new`/`with_real_*` 调用点**(仅 `tests.rs` 用 `for_tests_*`)。
- `run_agentic_loop(delegate:&dyn LoopDelegate, reason_ctx:&mut ReasoningContext, config:&AgenticLoopConfig)->LoopOutcome`(`agentic_loop.rs:443`)。**无 hook_bus / task_id 访问路径**(LoopDelegate/ReasoningContext 都不暴露)。`LoopOutcome::{Response, Stopped, Cancelled, MaxIterations, Failure}`。⇒ TaskStart/TaskEnd 在**调用点**(`tauri_commands.rs` send_agent_message 内 spawn 包住 `run_agentic_loop`)发射,那里有 `state.hook_bus`+session_id+返回的 LoopOutcome。

---

## 验证命令
- 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
- 单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- 已知预存失败 ~6(daemon_approval / truncate_for_error / browser provider_execution×3 / gbrain_eval_harness);stale tauri 产物报旧路径则清 `target/debug/build/tauri-*`+`uclaw-*`。

---

## File Structure
| 文件 | 责任 |
|---|---|
| `policy_eval/subscriber.rs` | **新建** `PolicySpecSubscriber` + `action_request_from_event`;`policy_eval/mod.rs` 加 `pub mod subscriber;` + re-export |
| `app.rs` | `AppState::new` pre-register PolicySpecSubscriber + `default_policy()` |
| `agent/tool_dispatch/mod.rs` | `PreToolUse`→`dispatch_with_decision` + Deny/AskUser + emit_tool_start 重排;approve() 内 Pre/PostPermission observe |
| `agent/dispatcher.rs` | `call_llm` PreLlmCall + `on_usage` PostLlmCall(observe)|
| `tauri_commands.rs` | send_agent_message 调用点 TaskStart/TaskEnd(observe)|
| `memory_policy/executor.rs` | field `HookBus`→`Arc<HookBus>`(type-readiness,见 Task 7 备注)|

---

## Task 1: PolicySpecSubscriber + action_request_from_event

**Files:** Create `src-tauri/src/policy_eval/subscriber.rs`; Modify `src-tauri/src/policy_eval/mod.rs`

- [ ] **Step 1: 失败测试 + 实现** — 新建 `policy_eval/subscriber.rs`:

```rust
//! PolicySpecSubscriber —— 把 PolicySpec 接入共享 HookBus(Sprint 3 ②)。
//! 5 个 decision-capable 事件 → ActionRequest → evaluate → HookDecision。
use crate::agent::hook_bus::{HookEvent, HookEventKind, HookSubscriber, SubscriberId};
use crate::policy_eval::{evaluate, ActionRequest, PolicySpec};
use crate::runtime::contracts::{HookDecision, RiskClass};
use async_trait::async_trait;

/// 把一个 decision-capable HookEvent 映射成 PolicySpec 的 ActionRequest。
/// 非 decision 事件 / 不关心的事件返回 None(订阅者据此放行)。
pub(crate) fn action_request_from_event(event: &HookEvent) -> Option<ActionRequest> {
    match event {
        HookEvent::PreToolUse { tool_name, .. } =>
            Some(ActionRequest::new("tool_use", tool_name.clone(), RiskClass::Low)),
        HookEvent::MemoryWrite { topic, .. } =>
            Some(ActionRequest::new("memory_write", topic.clone(), RiskClass::Low)),
        HookEvent::PrePermission { action, target, .. } =>
            Some(ActionRequest::new(action.clone(), target.clone(), RiskClass::Low)),
        HookEvent::PreLlmCall { model, .. } =>
            Some(ActionRequest::new("llm_call", model.clone(), RiskClass::Low)),
        HookEvent::PreContextInject { .. } =>
            Some(ActionRequest::new("context_inject", "", RiskClass::Low)),
        _ => None,
    }
}

pub struct PolicySpecSubscriber {
    spec: PolicySpec,
}

impl PolicySpecSubscriber {
    pub fn new(spec: PolicySpec) -> Self { Self { spec } }
}

#[async_trait]
impl HookSubscriber for PolicySpecSubscriber {
    fn id(&self) -> SubscriberId { SubscriberId::new("policy-spec") }
    fn interest_in(&self) -> &'static [HookEventKind] {
        &[
            HookEventKind::PreToolUse,
            HookEventKind::PreLlmCall,
            HookEventKind::PrePermission,
            HookEventKind::PreContextInject,
            HookEventKind::MemoryWrite,
        ]
    }
    async fn on_event(&self, event: &HookEvent) -> Option<HookDecision> {
        let req = action_request_from_event(event)?;
        let (decision, _rule_id) = evaluate(&self.spec, &req);
        Some(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_eval::{MatchPattern, PolicyRule};

    fn deny_tool(name: &str) -> PolicySpec {
        PolicySpec::new().with_rule(PolicyRule::new(
            "deny-one-tool",
            MatchPattern::ExactTarget { action_class: "tool_use".into(), target: name.into() },
            HookDecision::Deny { reason: format!("policy denies {name}") },
        ))
    }

    #[tokio::test]
    async fn denies_matching_tool() {
        let sub = PolicySpecSubscriber::new(deny_tool("bash"));
        let d = sub.on_event(&HookEvent::PreToolUse {
            task_id: "t".into(), tool_name: "bash".into(), args_json: "{}".into(),
        }).await;
        assert!(matches!(d, Some(HookDecision::Deny { .. })));
    }

    #[tokio::test]
    async fn allows_non_matching_tool() {
        let sub = PolicySpecSubscriber::new(deny_tool("bash"));
        let d = sub.on_event(&HookEvent::PreToolUse {
            task_id: "t".into(), tool_name: "read_file".into(), args_json: "{}".into(),
        }).await;
        assert!(matches!(d, Some(HookDecision::Allow)));
    }

    #[tokio::test]
    async fn empty_policy_allows() {
        let sub = PolicySpecSubscriber::new(PolicySpec::new());
        let d = sub.on_event(&HookEvent::MemoryWrite {
            task_id: "t".into(), topic: "x".into(), size_bytes: 1,
        }).await;
        assert!(matches!(d, Some(HookDecision::Allow)));
    }

    #[test]
    fn interest_covers_five_decision_kinds() {
        let sub = PolicySpecSubscriber::new(PolicySpec::new());
        assert_eq!(sub.interest_in().len(), 5);
    }
}
```

> `HookDecision`/`RiskClass` 的真实路径:确认是 `crate::runtime::contracts::{...}`(spec.rs:6 用的就是它);若 re-export 路径不同,用 spec.rs 同款 import。

- [ ] **Step 2: 注册模块** — `policy_eval/mod.rs` 加 `pub mod subscriber;` 和 `pub use subscriber::PolicySpecSubscriber;`。

- [ ] **Step 3: 确认红→绿** — `cargo test --lib policy_eval::subscriber 2>&1 | tail -8`(4 passed);`cargo build 2>&1 | grep -E "^error" | head`(空)。

- [ ] **Step 4: Commit**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/policy_eval/subscriber.rs src-tauri/src/policy_eval/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(policy): PolicySpecSubscriber bridging PolicySpec to HookBus

New policy_eval/subscriber.rs: HookSubscriber over the 5 decision-capable events,
mapping each to an ActionRequest and returning PolicySpec's verdict. + unit tests.

Verification: cargo test --lib policy_eval::subscriber -> 4 passed"
```

---

## Task 2: AppState pre-register PolicySpecSubscriber

**Files:** Modify `src-tauri/src/app.rs`

- [ ] **Step 1: default_policy + pre-register** — 在 `app.rs` 加一个模块级 fn(near AppState::new):

```rust
/// 启动默认 Hook 策略。本 slice 为 Allow-all(空 rules)—— 行为零变化。
/// 从 settings/DB 加载规则留后续(用户配置范围外)。
fn default_hook_policy() -> crate::policy_eval::PolicySpec {
    crate::policy_eval::PolicySpec::new()
}
```

把构造点(app.rs:958)`hook_bus: std::sync::Arc::new(crate::agent::hook_bus::HookBus::new()),` 改为**先在 `Ok(Self{..})` 之前**构造并注册,再在字面量里用变量。即在 `AppState::new` body 里(`Ok(Self {` 之前)加:

```rust
        let hook_bus = {
            let mut bus = crate::agent::hook_bus::HookBus::new();
            bus.register(std::sync::Arc::new(
                crate::policy_eval::PolicySpecSubscriber::new(default_hook_policy()),
            ))
            .map_err(|e| crate::error::Error::Internal(format!("hook subscriber register: {e:?}")))?;
            std::sync::Arc::new(bus)
        };
```

字面量里把 `hook_bus: std::sync::Arc::new(...)` 改为 `hook_bus,`(用上面的变量)。

> `crate::error::Error::Internal` 若不存在,用现有的合适变体(读 `error.rs` 取一个 `String`-承载变体);`BusError` 无 `Display` 则用 `{e:?}`(已 derive Debug)。

- [ ] **Step 2: 注册测试** — 在 app.rs test mod(或新建)加:

```rust
#[test]
fn appstate_hook_bus_has_policy_subscriber() {
    // 经 AppState::new 构造一个 state(用既有测试构造 helper / mock app_handle),
    // 断言 state.hook_bus.subscriber_count() >= 1。
    // 若 AppState::new 在单测里难构造(需真实 AppHandle),退化为直接测 default_hook_policy()
    // + 手动 register 后 subscriber_count()==1(覆盖注册逻辑)。
}
```
实现者按 AppState 在测试中是否可构造二选一(若有 mock-app 模式则走 full;否则测 register 逻辑)。

- [ ] **Step 3: 编译 + 测试 + 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/app.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(app): pre-register PolicySpecSubscriber on shared HookBus

AppState::new builds the HookBus, registers PolicySpecSubscriber (default Allow-all
policy) before Arc-wrapping, so the shared bus has a real subscriber. Field stays
Arc<HookBus> (lock-free dispatch). Behavior unchanged (Allow-all).

Verification: cargo build clean; subscriber_count test passes"
```

---

## Task 3: PreToolUse 决策门控(派发器 veto/AskUser + emit_tool_start 重排)

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`

- [ ] **Step 1: 重排 + 换 decision** — 在 `run_one`(157-239)里:把 `emit_tool_start`(216)从 PreToolUse 之前移到 PreToolUse 的 **Allow 分支内**;把 PreToolUse 的 `dispatch_observe`(219-223)换成 `dispatch_with_decision` + 分支处理:

```rust
        // ── PreToolUse 决策门(审批/path 之后,execute 之前的第三道门)──
        match self.hook_bus.dispatch_with_decision(&crate::agent::hook_bus::HookEvent::PreToolUse {
            task_id: ctx.session_id.clone(),
            tool_name: tc.name.clone(),
            args_json: tc.arguments.to_string(),
        }).await {
            crate::runtime::contracts::HookDecision::Allow => {}
            crate::runtime::contracts::HookDecision::Deny { reason } => {
                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                    "toolName": tc.name, "toolCallId": tc.id, "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(), arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason.clone())),
                    message_content: format!("Error: Hook denied tool — {reason}"),
                    is_error: true, rejected: true, paths_touched: vec![], was_mutation: false, soft_error: None,
                };
            }
            crate::runtime::contracts::HookDecision::AskUser { prompt, risk_class } => {
                // 复用 approve() 的 RequireApproval 机制(镜像)。
                let rx = self.pending_approvals.register(tc.id.clone());
                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                    "toolName": tc.name, "toolId": tc.id, "arguments": tc.arguments,
                    "reason": prompt, "sessionId": ctx.conversation_id,
                    "riskLevel": match risk_class { Some(r) => format!("{r:?}").to_lowercase(), None => "medium".into() },
                    "kind": "hook", "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                let approval = rx.await.unwrap_or(crate::app::ApprovalResult {
                    approved: false, always_allow: false, tool_name: None, path_scope: None, paths: None,
                });
                if !approval.approved {
                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                        "toolName": tc.name, "toolCallId": tc.id, "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    return ToolDispatchOutcome {
                        tool_call_id: tc.id.clone(), tool_name: tc.name.clone(), arguments: tc.arguments.clone(),
                        result: Err(ToolError::Execution("Hook gate: rejected by user.".into())),
                        message_content: "Error: Hook gate rejected by user.".into(),
                        is_error: true, rejected: true, paths_touched: vec![], was_mutation: false, soft_error: None,
                    };
                }
            }
        }

        // Allow(或 AskUser 通过)→ 现在才发 tool_start,再 execute。
        self.emit_tool_start(tool, tc, ctx);
        let result = self.run_tool(tool, tc, ctx).await;
```

> `HookDecision` 路径用 `crate::runtime::contracts::HookDecision`(与 spec.rs/contracts 一致;若 hook_bus re-export 了别名则用之)。`message_content`/`is_error`/`rejected` 与既有 Rejected outcome 形状一致(参 approve 拒绝分支的 outcome 构造)。

- [ ] **Step 2: 测试** — 扩 `tool_dispatch` 测试:构造一个 bus 携带 deny `"echo"` 的 `PolicySpecSubscriber`(`PolicySpec` 含一条 ExactTarget tool_use/echo → Deny),注入 dispatcher;dispatch `echo` → 断言 `rejected==true` 且**未执行**(EchoTool 带 AtomicBool);dispatch 一个非 deny 工具 → 执行。(AskUser 路径:可加一条 AskUser 规则 + 预先 `pending_approvals.resolve(tc.id, approved:false)` 模拟拒绝;若 resolve 时序难控,至少覆盖 Deny + Allow 两分支,AskUser 留集成/手动。)

```rust
    #[tokio::test]
    async fn pretooluse_deny_blocks_and_skips_execution() {
        // bus = HookBus with PolicySpecSubscriber denying "echo"
        // d = make_dispatcher_with_bus(reg_with_echo, bus)  // 见下注
        // outs = d.dispatch(vec![echo_call], &ctx).await
        // assert outs[0].rejected && !executed
    }
```
> 需要一个能注入自定义 bus 的测试 dispatcher 构造器。若现有 `make_dispatcher` 用空 bus,新增 `make_dispatcher_with_bus(reg, bus)` 变体(或给现有的加 bus 参数),让本测试传入带订阅者的 bus。复用 Sprint 3 ① 的 mock-app 模式。

- [ ] **Step 3: 编译 + 测试 + 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/agent/tool_dispatch/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(agent): PreToolUse decision gating in ToolDispatcher (veto/AskUser)

PreToolUse now uses dispatch_with_decision: Deny -> rejected outcome (reusing the
Rejected shape + agent:tool-rejected emit); AskUser -> pending_approvals oneshot
(mirroring approve()). emit_tool_start moved into the Allow branch so a denied tool
emits no tool_start. Allow-all default -> behavior unchanged.

Verification: cargo test --lib tool_dispatch:: -> pass (incl. deny test); build clean"
```

---

## Task 4: Pre/PostPermission observe 点亮(approve 内)

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`

- [ ] **Step 1: 发射** — 在 `approve()`(663-775)里:`tool_approval` 取得后、decision 计算前,加 `PrePermission`;decision 计算后、`match decision` 前,加 `PostPermission`:

```rust
        // (在 let tool_approval = ...; 之后)
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PrePermission {
            task_id: ctx.session_id.clone(), action: "tool_use".into(), target: tc.name.clone(),
        }).await;

        let decision = { /* 现有 ... */ };

        let granted = !matches!(decision, ApprovalDecision::Block { .. });
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostPermission {
            task_id: ctx.session_id.clone(), action: "tool_use".into(), granted,
        }).await;
```

> observe-only(`dispatch_observe`)—— 本 slice 不 honor PrePermission 决策(范围外)。

- [ ] **Step 2: 编译 + 提交**(observe-fire 无独立断言;靠编译 + 既有 approve 测试不回归)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/agent/tool_dispatch/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(agent): fire Pre/PostPermission (observe-only) in approval gate

Verification: cargo build clean; cargo test --lib tool_dispatch:: -> no regressions"
```

---

## Task 5: PreLlmCall/PostLlmCall observe 点亮

**Files:** Modify `src-tauri/src/agent/dispatcher.rs`

- [ ] **Step 1: PreLlmCall(call_llm 请求前)** — 在 `call_llm`(2030-2215)算完 `sys_tok/tool_tok/msg_tok`(~2193)、`stream_completion`(~2206)之前:

```rust
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PreLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            prompt_tokens_estimate: (sys_tok + tool_tok + msg_tok) as usize,
        }).await;
```
> 用真实变量名(读 2169-2193 确认 `sys_tok/tool_tok/msg_tok` 的实际名;若名不同则对齐)。

- [ ] **Step 2: PostLlmCall(on_usage 回调内)** — 在 `on_usage`(~2641)收到 `TokenUsage` 处:

```rust
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            input_tokens: usage.input_tokens as u64,
            output_tokens: usage.output_tokens as u64,
        }).await;
```
> `on_usage` 若非 async 或在非 async 闭包里,`dispatch_observe` 是 async —— 读 on_usage 签名:若同步上下文无法 await,改用 `tokio::spawn`(clone hook_bus + 值)发射,或把 PostLlmCall 移到 call_llm 拿到 usage 的 async 返回处。实现者按 on_usage 真实形态选 await / spawn,并在 PR 说明。

- [ ] **Step 3: 编译 + 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/agent/dispatcher.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(agent): fire PreLlmCall/PostLlmCall (observe-only) in call_llm/on_usage

Verification: cargo build clean; cargo test --lib dispatcher -> no regressions"
```

---

## Task 6: TaskStart/TaskEnd observe 点亮(send_agent_message 调用点)

**Files:** Modify `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 找调用点** — 在 `send_agent_message` 里找 `run_agentic_loop(...).await` 的调用(在 spawn 内)。确认 `session_id` + 一个可达的 hook_bus(spawn 前 `let hook_bus = state.hook_bus.clone();` move 进闭包,与 Sprint 3 ① 的 deps clone 同款)。

- [ ] **Step 2: 包 TaskStart/TaskEnd** — 围 `run_agentic_loop` 调用:

```rust
        hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::TaskStart {
            task_id: session_id.clone(), intent_id: String::new(),
        }).await;

        let outcome = run_agentic_loop(&delegate, &mut reason_ctx, &config).await;

        let outcome_str = match &outcome {
            crate::agent::types::LoopOutcome::Response { .. } => "completed",
            crate::agent::types::LoopOutcome::Stopped => "cancelled",
            crate::agent::types::LoopOutcome::Cancelled { .. } => "cancelled",
            crate::agent::types::LoopOutcome::MaxIterations => "failed",
            crate::agent::types::LoopOutcome::Failure { .. } => "failed",
        };
        hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::TaskEnd {
            task_id: session_id.clone(), outcome: outcome_str.into(),
        }).await;
```
> 用真实的 outcome 绑定名 + LoopOutcome 真实变体(读 types.rs;若有额外变体补全 match)。`intent_id` 无意图系统 → 空串。若 run_agentic_loop 的返回直接被 return/用掉,先 `let outcome = ...;` 绑定再发 TaskEnd 再用。

- [ ] **Step 3: 编译 + 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "feat(api): fire TaskStart/TaskEnd (observe-only) around run_agentic_loop

Verification: cargo build clean"
```

---

## Task 7: memory_policy 共享 bus(type-readiness)

> ⚠️ **备注:** `MemoryPolicyExecutor` 当前**无 production 构造点**(仅 `for_tests_*`)。本任务把 field 改 `Arc<HookBus>` 是**类型就绪**(让它日后能接共享 bus),本 slice 无 live MemoryWrite 门控。**若你想跳过这一任务,只做 Task 1-6 + 8 即可**(spec §5 的 MemoryWrite 这条届时降为"延后")。

**Files:** Modify `src-tauri/src/memory_policy/executor.rs`

- [ ] **Step 1: field + 构造签名** — `hook_bus: HookBus` → `hook_bus: std::sync::Arc<crate::agent::hook_bus::HookBus>`;`new(hook_bus: Arc<HookBus>, ...)` / `with_real_gbrain_and_artifacts(hook_bus: Arc<HookBus>, ...)` 同改;`for_tests_allow_all()` 用 `Arc::new(HookBus::new())`;`for_tests_deny_all()` 注册后 `Arc::new(bus)`。`gate_write` 的 `self.hook_bus.dispatch_with_decision(...)` 经 Arc deref 不变。

- [ ] **Step 2: 共享门控测试** — 加测试:用一个携带 deny-MemoryWrite 的 `PolicySpecSubscriber` 的 `Arc<HookBus>` 构造 executor,断言 `gate_write` 得 Deny → Rejected/Deferred(按 gate_write 真实返回)。

- [ ] **Step 3: 编译 + 测试 + 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add src-tauri/src/memory_policy/executor.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "refactor(memory): MemoryPolicyExecutor takes Arc<HookBus> (shared-bus ready)

Type-readiness so MemoryWrite gating can consult the shared subscriber set once
the executor is production-wired. No production construction site yet.

Verification: cargo test --lib memory_policy -> pass; build clean"
```

---

## Task 8: 集成测试收口 + 全量验收

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`(或测试模块)

- [ ] **Step 1: 端到端 veto 集成** — 确认 Task 3 的 deny 测试覆盖:`PolicySpecSubscriber` 在共享 bus 上 deny 一个工具 → 派发器 reject + 未执行。补一个 Allow-all bus → 工具正常执行(证明默认零变化)。
- [ ] **Step 2: 全量验收** — `cargo build`(空 error);`cargo test --lib "policy_eval::" "tool_dispatch::" "memory_policy" dispatcher 2>&1 | tail -15`(全过);全量 `cargo test --lib 2>&1 | tail -6`(仅 ~6 已知预存失败,零新增)。手动 smoke 提示写入 PR:`default_hook_policy()` 临时改为含一条 deny 规则 → `cargo tauri dev` → 观察该工具被 Hook 拒(`agent:tool-rejected`)。
- [ ] **Step 3: Commit**(若有新增测试)
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-hooks commit -m "test(hooks): end-to-end PreToolUse veto coverage + verification

Verification: cargo test --lib -> no new failures (6 known pre-existing)"
```

---

## 最终验收
- [ ] `cargo build` → 空 error
- [ ] `policy_eval::subscriber` / `tool_dispatch::`(含 deny)/ `memory_policy` 测试全过
- [ ] 全量 `cargo test --lib` → 仅 ~6 已知预存失败
- [ ] 共享 bus 有 PolicySpecSubscriber(`subscriber_count() >= 1`)
- [ ] 默认 Allow-all → 行为零变化(既有 dispatcher 测试不回归)
- [ ] 手动 smoke:临时 deny 规则 → 工具被 Hook 拒

---

## Self-Review
**Spec coverage:** §3 pre-register + PolicySpecSubscriber → Task1/Task2;§4 PreToolUse veto/AskUser + emit_tool_start 重排 → Task3;§5 MemoryWrite 共享 → Task7(type-readiness,标注)、observe 6 事件 → Task4(permission)+Task5(LLM)+Task6(task);§6 Allow-all 安全默认 → Task2/Task8;§7 测试 → Task1/3/7/8;§8 commit 序列 → Task1-8 顺序一致。

**Placeholder scan:** Task5 Step2 的 "on_usage await/spawn 二选一"、Task2 Step2 的 "AppState 可构造性二选一"、Task6 的 "LoopOutcome 变体补全" 是给实现者的**明确决策点 + 具体两选项 + 读取指令**,非 TBD。Task7 顶部备注是**发现-降级说明**(executor 无 production 构造点),非占位。其余代码块完整。无 TODO/TBD。

**Type consistency:** `PolicySpecSubscriber::new(PolicySpec)`(Task1)→ Task2/Task3/Task7 构造一致;`action_request_from_event`→`ActionRequest::new(class,target,RiskClass)`(Task1)与 spec.rs 签名一致;`HookEvent::PreToolUse{task_id,tool_name,args_json}` / `PrePermission{task_id,action,target}` / `PostPermission{task_id,action,granted}` / `PreLlmCall{...prompt_tokens_estimate:usize}` / `PostLlmCall{...input_tokens:u64,output_tokens:u64}` / `TaskStart{task_id,intent_id}` / `TaskEnd{task_id,outcome}`(Task3-6)与 event.rs 字段一致;`HookDecision::{Allow,Deny{reason},AskUser{prompt,risk_class}}`(Task3)与 contracts 一致;`ApprovalResult{approved,always_allow,tool_name,path_scope,paths}` + `pending_approvals.register`(Task3)与 approve() 一致;`ToolDispatchOutcome` 字段集(Task3)与 Sprint 3 ① 定义一致。
