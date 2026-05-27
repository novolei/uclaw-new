# Hooks 系统(决策门控基底)设计 (Sprint 3 ②)

**状态:** 设计已逐节批准,待 spec 评审 → writing-plans
**分支/worktree:** `codex/sprint3-hooks`(base = main `2fb57c3d`)
**前置:** Pi convergence。ADR §20(`docs/adr/2026-05-20-…north-star.md:1535`)、Pi 升级设计(`…2026-05-26-agent-framework-pi-upgrade-design.md:23,1400-1594`)。Sprint 3 ①(#557)已留 observe-only `PreToolUse`/`PostToolUse` 发射点 + `AppState` 共享 `Arc<HookBus>`。

---

## 1. 目标与范围

**目标:** 把 Hook 系统从"骨架 + 死信"做成可用的**双向决策门控基底**:让共享 bus 真正有订阅者,`PreToolUse` 接 `dispatch_with_decision`(veto/AskUser),`MemoryWrite` 经共享 bus 门控,点亮高价值死事件供观测。用现有 `HookDecision`(Allow/Deny/AskUser),**不做 modify**。

**安全默认:** 启动 `PolicySpec` 默认 Allow-all(或极小规则集)→ 行为零变化,直到规则真正 deny/ask。

**范围内:**
- **register-before-share 修复**:`AppState::new` 在 `Arc::new` 前 `register(...)` 固定订阅者集;字段保持 `Arc<HookBus>`(派发热路径无锁)。
- **`PolicySpecSubscriber`**:`HookSubscriber`,`interest_in` = 5 个 decision-capable 事件,`on_event` 把事件→`ActionRequest`→`evaluate(&spec, &req)`→`HookDecision`。
- **`PreToolUse` 决策门控**:`ToolDispatcher::run_one` 把 `PreToolUse` 从 `dispatch_observe` 换 `dispatch_with_decision`,作为审批/path 之后、`emit_tool_start`/execute 之前的第三道门;`Deny`→复用 `ApprovalGate::Rejected` 拒绝路径,`AskUser`→复用 `pending_approvals` oneshot + `agent:need_approval`。
- **`MemoryWrite` 共享门控**:`MemoryPolicyExecutor` 改持 `Arc<HookBus>`(共享),其 `dispatch_with_decision(MemoryWrite)` 咨询共享订阅者集。
- **事件覆盖(observe-only 点亮)**:`TaskStart`/`TaskEnd`、`PreLlmCall`/`PostLlmCall`、`PrePermission`/`PostPermission`。

**明确范围外(留给后续):**
- **modify 语义**(Pi `on(AfterToolCall)`→替换 result、`BeforeProviderRequest`→patch 参数;`HookDecision::Modify` / typed `AgentHookResult`)。
- `PreLlmCall`/`PrePermission`/`PreContextInject` 的**决策 honor**(本 slice 仅 observe-fire,不接 abort 语义)。
- `PreContextInject`/`PostContextInject`、`MemoryRecall`、`Checkpoint` 的发射。
- **用户可配置 hook**(shell hook / settings / DB 规则加载)。
- bus 订阅者并发化(现 series 派发)+ 订阅者 panic 隔离。

---

## 2. 现状锚点(实现以此为准)

- `HookBus`(`agent/hook_bus/bus.rs:37-40`):`#[derive(Clone, Default)] struct { subscribers: Vec<Arc<dyn HookSubscriber>> }`,**无 RwLock/Mutex**。`new()`;`register(&mut self, Arc<dyn HookSubscriber>) -> Result<(), BusError>`(去重 `SubscriberId`);`unregister(&mut self, &SubscriberId)->bool`;`subscriber_count(&self)->usize`;`async dispatch_observe(&self, &HookEvent)`(丢决策);`async dispatch_with_decision(&self, &HookEvent) -> HookDecision`(首 Deny 胜,否则首 AskUser,否则 Allow;observe-only 事件强制 Allow)。Arc 包裹后冻结(无内部可变)。
- `HookEvent`(`event.rs:11-84`,13 变体,均带 `task_id`):`PreToolUse{task_id,tool_name,args_json}`、`PostToolUse{task_id,tool_name,success,result_preview}`、`PreLlmCall{task_id,provider,model,prompt_tokens_estimate}`、`PostLlmCall{task_id,provider,model,input_tokens,output_tokens}`、`PrePermission{task_id,action,target}`、`PostPermission{task_id,action,granted}`、`PreContextInject{...}`、`PostContextInject{...}`、`TaskStart{task_id,intent_id}`、`TaskEnd{task_id,outcome}`、`MemoryWrite{task_id,topic,size_bytes}`、`MemoryRecall{...}`、`Checkpoint{...}`。`is_decision_capable()`(event.rs:129-138)= `PreToolUse|PreLlmCall|PrePermission|PreContextInject|MemoryWrite`(5 个)。`kind()`、`task_id()` 访问器全覆盖。`HookEventKind::ALL`(13)。
- `HookDecision`(`crates/uclaw-runtime-contracts/src/lib.rs:573-607`):`Allow | Deny{reason:String} | AskUser{prompt:String, risk_class:Option<RiskClass>}`;`is_allow/is_deny/requires_user`。
- `HookSubscriber`(`agent/hook_bus/subscriber.rs:42-53`,`#[async_trait]`):`fn id()->SubscriberId`、`fn interest_in()->&'static [HookEventKind]`、`async fn on_event(&self, &HookEvent)->Option<HookDecision>`(`None`=无意见)。`SubscriberId(pub String)`(subscriber.rs:14-23)。
- `PolicySpec`(`policy_eval/spec.rs:120-152`):`struct { rules: Vec<PolicyRule> }`;`PolicyRule{id, pattern: MatchPattern, outcome: HookDecision}`;`pub fn evaluate<'a>(spec:&'a PolicySpec, req:&ActionRequest)->(HookDecision, Option<&'a str>)`(首匹配胜,否则 Allow)。`ActionRequest::new(class, target, risk: RiskClass)`。`MatchPattern::{AnyTarget{action_class}, ExactTarget{action_class, target}, ...}`(读 spec.rs 取全变体)。`pub use spec::{evaluate, ActionRequest, MatchPattern, PolicyRule, PolicySpec}`(`policy_eval/mod.rs:27`)。
- `AppState.hook_bus`(`app.rs:958`):`std::sync::Arc::new(HookBus::new())`,**零订阅者**;production 无任何 `register`。`AppState::new` 是唯一构造点(单 `Ok(Self{..})`)。
- `ToolDispatcher::run_one`(`agent/tool_dispatch/mod.rs:157-365`):序列 `resolve(161) → approve(174-190, ApprovalGate::{Allow, Rejected{reason,message}}) → gate_paths(193-209, PathGate::{Allow{paths}, Rejected{reason,message}}) → emit_tool_start(216) → PreToolUse dispatch_observe(219-223) → run_tool → PostToolUse dispatch_observe(233-238)`。`Rejected` → `ToolDispatchOutcome{rejected:true, result:Err(ToolError::Execution(reason)), message_content, is_error:true}` + emit `agent:tool-rejected`。approve() 的 `RequireApproval`/`AskUser` 路径用 `pending_approvals` register oneshot + emit `agent:need_approval` + await(读 approve() 镜像)。
- `MemoryPolicyExecutor`(`memory_policy/executor.rs:23`):持**本地** `hook_bus: HookBus`(值,非 Arc),构造时传入;`dispatch_with_decision(MemoryWrite)`(:138)驱动 Rejected/Deferred/执行。production 该本地 bus 也无真实订阅者(仅测试 `DenyMemoryWrites`)。
- 死事件:13 中仅 `PreToolUse`/`PostToolUse` 在共享 bus 发射;`MemoryWrite` 在本地 bus;其余 10 个仅测试 helper 构造。`call_llm` 在 `dispatcher.rs`;loop start/end 在 `agentic_loop.rs`/`tauri_commands.rs`。

---

## 3. 架构:bus 拥有 + 首订阅者(已批准 Section 1)

**pre-register before Arc。** `AppState::new` 内:`let mut bus = HookBus::new(); bus.register(Arc::new(PolicySpecSubscriber::new(default_policy())))?; let hook_bus = Arc::new(bus);`。本 slice **唯一**注册的订阅者就是 `PolicySpecSubscriber`(MemoryWrite 门控也经它走共享 bus)。字段保持 `Arc<HookBus>` → 派发 `&self` 无锁;`ToolDispatcher.hook_bus: Arc<HookBus>` 不变。代价:无运行时增删(用户配置已范围外)。

> 备选 `Arc<RwLock<HookBus>>`(运行时注册)被否:每次 per-tool-call 派发需读锁 + 改派发器字段类型;固定启动集不需要。

**`PolicySpecSubscriber`**(新建 `policy_eval/subscriber.rs` —— wire-up 属 policy 逻辑,与 `PolicySpec` 同模块;import `HookSubscriber` from `agent/hook_bus`):

```rust
pub struct PolicySpecSubscriber { spec: PolicySpec }
impl PolicySpecSubscriber { pub fn new(spec: PolicySpec) -> Self { Self { spec } } }

#[async_trait]
impl HookSubscriber for PolicySpecSubscriber {
    fn id(&self) -> SubscriberId { SubscriberId::new("policy-spec") }
    fn interest_in(&self) -> &'static [HookEventKind] {
        &[HookEventKind::PreToolUse, HookEventKind::PreLlmCall,
          HookEventKind::PrePermission, HookEventKind::PreContextInject,
          HookEventKind::MemoryWrite]
    }
    async fn on_event(&self, event: &HookEvent) -> Option<HookDecision> {
        let req = action_request_from_event(event)?;   // 见下
        let (decision, _rule_id) = crate::policy_eval::evaluate(&self.spec, &req);
        Some(decision)
    }
}
```

`action_request_from_event(&HookEvent) -> Option<ActionRequest>`:
- `PreToolUse{tool_name,..}` → `ActionRequest::new("tool_use", tool_name, RiskClass::<默认/按工具>)`
- `MemoryWrite{topic,..}` → `ActionRequest::new("memory_write", topic, ..)`
- `PrePermission{action,target,..}` → `ActionRequest::new(action, target, ..)`
- `PreLlmCall{model,..}` → `ActionRequest::new("llm_call", model, ..)`
- `PreContextInject{..}` → `ActionRequest::new("context_inject", "", ..)`
- 其它 → `None`

> RiskClass 来源:本 slice 用一个保守默认(如 `RiskClass::Low`)或按 action_class 粗分;细化(按工具风险表)留后续。`default_policy()` 本 slice 返回 Allow-all(空 rules)或含 1-2 条高风险 AskUser 规则的示例集;从 settings/DB 加载留后续(用户配置范围外)。

---

## 4. `PreToolUse` 决策门控(已批准 Section 2)

`run_one` 重排 + 换 API。新序列:

```
resolve → approve → gate_paths → PreToolUse(dispatch_with_decision)
  → [Allow] emit_tool_start → run_tool → PostToolUse(observe) → outcome
```

把现 line 216 的 `emit_tool_start` 移到 PreToolUse 决策**之后**(Allow 分支内),使被 deny 的工具不发 `tool_start`(与 approval/path 拒绝一致)。PreToolUse 处理:

```rust
match self.hook_bus.dispatch_with_decision(&HookEvent::PreToolUse {
    task_id: ctx.session_id.clone(), tool_name: tc.name.clone(), args_json: tc.arguments.to_string(),
}).await {
    HookDecision::Allow => { /* emit_tool_start; run_tool; ... */ }
    HookDecision::Deny { reason } => {
        let _ = self.app_handle.emit("agent:tool-rejected", /* 同 approval reject 的 payload */);
        return ToolDispatchOutcome {
            tool_call_id: tc.id.clone(), tool_name: tc.name.clone(), arguments: tc.arguments.clone(),
            result: Err(ToolError::Execution(reason.clone())),
            message_content: format!("Error: Hook denied tool — {reason}"),
            is_error: true, rejected: true, paths_touched: vec![], was_mutation: false, soft_error: None,
        };
    }
    HookDecision::AskUser { prompt, risk_class } => {
        // 复用 approve() 的 RequireApproval 机制:pending_approvals.register oneshot
        // + emit "agent:need_approval"{kind:"hook", prompt, risk_class} + await;
        // 拒 → 同上 rejected outcome;准 → 继续 Allow 分支。
    }
}
```

`PostToolUse` 保持 `dispatch_observe`(非 decision-capable)。bus 聚合多订阅者(首 Deny 胜)→ 单/多订阅者皆可。Allow-all 默认 → 行为不变。

---

## 5. 事件覆盖 + memory_policy 共享 bus(已批准 Section 3)

**决策 honor(2):**
- `PreToolUse` → §4。
- `MemoryWrite` → `MemoryPolicyExecutor` 构造签名 `hook_bus: HookBus` 改 `hook_bus: Arc<HookBus>`(共享);调用点传 `state.hook_bus.clone()`(读 executor 现构造点 + 其在 AppState 的装配处)。其 `dispatch_with_decision(MemoryWrite)` 现咨询共享订阅者(PolicySpec)。

**observe-only 点亮(6):** 经 `dispatch_observe` 发射(决策不 honor):

| 事件 | 发射点 |
|---|---|
| `TaskStart` / `TaskEnd` | agentic loop 起点/终点(`agentic_loop.rs` run_agentic_loop 进入处 / 返回前;`intent_id`/`outcome` 取现有可达值,无则填 `session_id`/`""`) |
| `PreLlmCall` / `PostLlmCall` | `ChatDelegate::call_llm`(LLM 请求前/后;provider/model/token 取现有估算) |
| `PrePermission` / `PostPermission` | dispatcher 审批门(`approve()` 内:决策前 PrePermission`{action:"tool_use", target:tool_name}`,决策后 PostPermission`{granted}`) |

> `PreLlmCall`/`PrePermission` 是 decision-capable,但本 slice 仅 observe-fire(决策不 honor,避免半成品 abort 语义)—— spec 显式声明。

**延后(不发射):** `PreContextInject`/`PostContextInject`、`MemoryRecall`、`Checkpoint`。

---

## 6. 错误处理与行为保持(已批准 Section 4)

- **安全默认:** `default_policy()` Allow-all → `PreToolUse`/`MemoryWrite` 恒 Allow,observe 事件无决策效应 → agent 路径不变,现有 dispatcher 测试全绿。
- **订阅者纪律:** `on_event` async 且 series 派发阻塞 loop → 订阅者须快、非阻塞。`PolicySpecSubscriber` 是纯内存规则匹配(可忽略)。**不加**订阅者 panic 隔离(YAGNI;唯一订阅者纯函数)—— 文档化 subscriber.rs 既有"须快/可靠"约束。
- **AskUser timeout/断连:** 复用 approve() 的 `pending_approvals` 语义(dropped oneshot → 视作未批准),无新行为。
- **series 派发阻塞:** 已知约束,文档化;PolicySpec 无碍,未来重订阅者需注意 —— 本 slice 不处理。

---

## 7. 测试(已批准 Section 4)

- `PolicySpecSubscriber` 单测:`interest_in()` = 5 decision kinds;`on_event(PreToolUse{tool})` → ActionRequest → 规则 Allow/Deny/AskUser;`MemoryWrite` 同理;非映射事件 → `None`/Allow。
- 派发器 veto 集成(扩 `tool_dispatch` 测试):bus 携带 deny `"echo"` 的 PolicySpecSubscriber → dispatch → 断言 `rejected` 且**未执行**;allow 工具 → 执行;AskUser 规则 → 路由 `pending_approvals`(镜像现有 approval AskUser 测试)。
- `memory_policy` 共享 bus 测试:`MemoryWrite` 经共享订阅者得 Deny → Rejected。
- 注册测试:`AppState::new` 后 `state.hook_bus.subscriber_count() > 0`。
- observe-fire:单测可达处用 capturing subscriber 断言 `TaskStart/End`、`PreLlmCall/Post`、`Pre/PostPermission` 发射;loop 级别不易单测的以手动 smoke 兜底。
- 验收 gate:`cargo build` 干净;全量 `cargo test --lib` 无新失败(默认 Allow-all);手动 smoke(配一条 deny 规则,观察工具被 Hook 拒)。

---

## 8. 文件结构 + commit 序列(可二分)

| 文件 | 责任 |
|---|---|
| `policy_eval/subscriber.rs` | **新建** `PolicySpecSubscriber` + `action_request_from_event`;`policy_eval/mod.rs` 加 `pub mod subscriber;` |
| `app.rs` | `AppState::new` pre-register PolicySpecSubscriber(+ `default_policy()`) |
| `agent/tool_dispatch/mod.rs` | `PreToolUse` → dispatch_with_decision + Deny/AskUser 处理 + emit_tool_start 重排 |
| `memory_policy/executor.rs`(+ 其 AppState 装配点) | `HookBus` → `Arc<HookBus>` 共享 |
| `agent/dispatcher.rs` | `PreLlmCall`/`PostLlmCall` observe 发射;approve() 内 `Pre/PostPermission` 发射 |
| `agent/agentic_loop.rs` | `TaskStart`/`TaskEnd` observe 发射 |

commit 序列:`PolicySpecSubscriber + action_request_from_event(单测)` → `AppState pre-register` → `PreToolUse 决策门控(派发器 veto/AskUser + 重排)` → `memory_policy 共享 bus` → `observe 事件点亮(loop/LLM/permission)` → `集成测试收口`。
