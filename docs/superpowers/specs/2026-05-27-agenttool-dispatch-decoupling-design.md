# AgentTool 派发缝解耦 设计 (Sprint 3 ①)

**状态:** 设计已逐节批准,待 spec 评审 → writing-plans
**分支/worktree:** `codex/sprint3-tool-dispatch`(base = main `a85063d6`)
**前置:** Pi convergence。ADR §20(`docs/adr/2026-05-20-…north-star.md:1529-1536`)、Pi 升级设计(`docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md:17,231`)。Sprint 1 计划显式把 AgentTool 提取推迟到 Sprint 3。

---

## 1. 目标与范围

**目标:** 把 `ChatDelegate` 里 ~860 行的 `execute_tool_calls` 抽离为独立、可隔离测试的 `ToolDispatcher`,把工具派发的 ~11 个纠缠关注点拆成清晰阶段,并为后续 Hooks 系统(Sprint 3 ②)提供干净的派发边界 —— 顺带把现有 HookBus 的 `PreToolUse`/`PostToolUse` 点亮为 observe-only。

**这是行为保持(behavior-preserving)的结构性重构。** 唯一的新运行时行为:两个 observe-only hook 发射(今天无订阅者)。

**范围内:**
- 抽 `ToolDispatcher`(struct)+ `ToolDispatchContext` + `ToolDispatchOutcome`,放 `agent/tool_dispatch/`。
- 统一 serial / parallel 两条路径的 per-call 例程(消除现有重复)。
- `Tool` trait 加 `concurrency()`(默认 `Sequential`),5 个只读工具覆盖为 `Parallel`,删 `PARALLEL_SAFE_TOOLS` 硬编码。
- 把 `tauri_commands.rs:1879-2067` 的 registry 装配块抽成 `build_tool_registry(...)` factory。
- `PreToolUse`/`PostToolUse` 经现有 `HookBus.emit` 点亮(observe-only,无 veto)。

**明确范围外(留给后续 sub-slice / Sprint 3 ②):**
- `Tool` → `AgentTool` 改名。
- `ToolOutput` → `AgentToolResult { terminate, file_ops }`。
- `ToolContext` 真正流入 `execute`(现 `ToolExecutionContext` stub 仍 `_ctx` 忽略)。
- `execute` 加 `tool_call_id` / `CancellationToken` 形参。
- Hook 的 veto/modify 语义 + 完整 Hook API。

---

## 2. 现状锚点(实现以此为准)

- **`Tool` trait** 在 `agent/tools/tool.rs:207-255`(post-#542 形态):必需 `name/description/parameters_schema/execute(params)`;默认 `supports_streaming()→false`、`execute_streaming(params, sink)→execute`、`estimated_cost/duration→None`、`requires_approval()→ApprovalRequirement::default()`、`path_args()→[]`、`preview_target_path()→None`。
- **`ToolOutput`**(tool.rs:10-26):`{ result: Value, cost: Option<f64>, duration_ms: u64 }`。**`ToolError`**(tool.rs:87-105):`Execution/InvalidParams/NotFound/Io/Kinded`。
- **`ToolRegistry`**(tool.rs:277-314)已存在并在用:`HashMap<String, Box<dyn Tool>>`,`register/get/list_definitions`,`Arc` 包裹后注入 `ChatDelegate`;派发走 `self.tools.get(&name)`(dispatcher.rs:2490),**无大 match**。
- **`ToolExecutionContext`**(tool.rs:158-204)stub 存在(含 session/task/message/tool_call_id/workspace/mode/safety 字段),但 `execute_tool_with_context`/`execute_streaming_with_context`(tool.rs:257-274)把 `_ctx` 忽略。**本 slice 不动它。**
- **已存在但不同轴的 `ToolExecutionMode`**(tool.rs:145-150):`AgentTurn | Direct`(调用点模式)。**不要复用它表达并行性** —— 新增 `ToolConcurrency`。
- **`execute_tool_calls`**(dispatcher.rs:2410-3270,含 JoinSet 批次):11 个关注点(见 §3)。
- **`PARALLEL_SAFE_TOOLS`**(dispatcher.rs:24-30):`read_file, search_files, search_codebase, get_file_skeleton, list_dir`;`is_parallel_safe()`(:32)。
- **registry 装配块** `tauri_commands.rs:1879-2067`(builtin + memu + browser `bt!` 宏 + MCP proxy)。
- **MCP 适配器** `McpToolProxy`(mcp.rs:1783-1910)`impl Tool`;`McpManager::create_tool_proxies`(mcp.rs:2632-2671)。
- **SafetyManager** `safety/mod.rs`:派发时读 `tool.requires_approval()` + `should_approve_with_db()`(dispatcher.rs:2490-2601),path gate `check_paths()`(:2611-2715,经 `tool.path_args()`)。
- **HookBus** `agent/hook_bus/`:13 事件类型骨架存在;`PreToolUse`/`PostToolUse` 类型有,但**在 agent 路径上从不发射**(仅 `memory_policy/executor.rs` 用 MemoryWrite)。
- **流式 coalescer** dispatcher.rs:2751-2802:bash `ToolStreamSink` drain task,50ms/8KB 合并后再 emit `chat:stream-tool-activity`。
- **trajectory + InfraService + token budget + file_ops + plan_update 反伪进展** 见 §3。

---

## 3. 架构:派发边界(已批准 Section 1)

**loop-agnostic 派发器。** `ToolDispatcher` 是 struct,持有**稳定共享依赖**,**不依赖 `ReasoningContext`**:

```rust
pub struct ToolDispatcher {
    tools: Arc<ToolRegistry>,
    safety: Arc<tokio::sync::RwLock<SafetyManager>>,
    app_handle: tauri::AppHandle,
    pending_approvals: PendingApprovals,
    trajectory: Option<Arc<TrajectoryStore>>,    // 与现有可选性一致
    infra: Option<InfraServiceHandle>,
    token_budget: ToolBudgetManager,
    hook_bus: Arc<HookBus>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,  // 审批规则查询
}
```

入口:

```rust
pub async fn dispatch(
    &self,
    calls: Vec<ToolCall>,
    ctx: &ToolDispatchContext,
) -> Vec<ToolDispatchOutcome>;
```

- **`ToolDispatchContext`**(每轮**不可变**输入,非 `ReasoningContext`):
  ```rust
  pub struct ToolDispatchContext {
      pub session_id: String,
      pub conversation_id: String,
      pub workspace_root: Option<PathBuf>,
      pub attached_dirs: Vec<PathBuf>,
      pub safety_mode: Option<SafetyMode>,
      pub iteration: usize,
  }
  ```
- **`ToolDispatchOutcome`**(每 call 结构化返回,供 loop 做 bookkeeping):
  ```rust
  pub struct ToolDispatchOutcome {
      pub tool_call_id: String,
      pub tool_name: String,
      pub result: Result<ToolOutput, ToolError>,
      pub paths_touched: Vec<PathBuf>,    // 供 reason_ctx.file_ops.track
      pub was_mutation: bool,             // 供 mutations_since_last_plan_done
      pub soft_error: Option<String>,     // detect_soft_tool_error 结果
      pub rejected: bool,                 // 审批被拒/Block
  }
  ```

**`ChatDelegate.execute_tool_calls` 变薄:**
1. `plan_update` 反伪进展 **pre-pass**(深耦合 `reason_ctx`,留在 ChatDelegate)。
2. 构造 `ToolDispatchContext`,调 `self.dispatcher.dispatch(calls, &ctx).await`。
3. 从返回的 outcomes 应用 `reason_ctx` bookkeeping:`file_ops.track`(用 `paths_touched`)、`mutations_since_last_plan_done`(用 `was_mutation`)、`recent_tool_errors`(用 `soft_error`,供 GEP)、`exit_plan_mode` 拒绝时的 UserCorrection 事件。

**为何 loop-agnostic 优于"传 `&mut ReasoningContext`":** 派发器可隔离单测(无需构造 agent loop),Hooks 缝干净(hook 观察工具派发事件,不缠 loop 状态)。代价:定义 `ToolDispatchOutcome` + 把 bookkeeping 应用上移一层(~40 行)。

---

## 4. 派发流水线 + Hook 点(已批准 Section 2)

**统一的 per-call 例程**(serial 与 parallel 两条路径共用,消除现有重复):

```
resolve tool  →  approval gate  →  path-policy gate  →  [PreToolUse hook]
  →  execute / execute_streaming  →  truncate(token budget)
  →  emit tool-result/error  →  trajectory + InfraService record  →  [PostToolUse hook]
  →  ToolDispatchOutcome
```

**serial vs parallel 编排:** `dispatch` 遍历 calls,按每个工具的 `concurrency()`(§5)分道:`Sequential` 顺序内联,`Parallel` 收集后经 `JoinSet` 并发 —— 保持现状(只读工具批处理,其余串行)。两道都走**同一** per-call 例程,故 hooks/emit/record 一致发射(修掉现有重复)。审批/path-gate 在分道前完成,与现状顺序一致。

**Hook 点(observe-only):**
- `PreToolUse`:审批 + path-gate 通过后、execute 前发射(未来 veto 的天然位置,本 slice 仅观察)。payload:工具名 + args。
- `PostToolUse`:execute 后带 result 发射。payload:工具名 + args + result。
- 经现有 `HookBus.emit`;**无 veto/modify**;今天无订阅者 → 行为不变。

**三个高风险件:**
1. **审批(oneshot await):** 派发器持 `SafetyManager` + `PendingApprovals` + `AppHandle`,整段(emit `agent:need_approval`→await `oneshot`→`always_allow` 触发 `add_auto_approved`)整体搬入。全 `Arc`,`&self`,无 `async_trait` 的 `&mut self` 摩擦。
2. **流式 coalescer:** bash `ToolStreamSink` drain task(50ms/8KB→`chat:stream-tool-activity`)成为派发器**私有 helper**;owns `app_handle` + `conversation_id`(via ctx);task 生命周期仍在该 call 的 execute 步内 join,与现状一致。
3. **`plan_update` 反伪进展:** 唯一**留在 `ChatDelegate`** 的关注点(深耦合 `reason_ctx`),作为 `dispatch` 前的 pre-pass,行为不变。

---

## 5. `concurrency()` 迁移 + registry factory + 模块布局(已批准 Section 3)

**per-tool 并行性(替代硬编码数组):** `Tool` trait 加非破坏默认:

```rust
fn concurrency(&self) -> ToolConcurrency { ToolConcurrency::Sequential }
```

新增 `ToolConcurrency { Sequential, Parallel }`(**不复用** `ToolExecutionMode`)。默认 `Sequential`(= 今天"不在 `PARALLEL_SAFE_TOOLS`")。**仅 5 个只读工具**覆盖 `Parallel`:`read_file / search_files / search_codebase / get_file_skeleton / list_dir`。派发器读 `tool.concurrency()`,删 `PARALLEL_SAFE_TOOLS` + `is_parallel_safe`。**净行为一致**(同 5 个工具批处理),改 5 个 impl,非 55 个。

**registry factory:** 把 `tauri_commands.rs:1879-2067` 装配块抽成:

```rust
pub fn build_tool_registry(
    app_handle: &tauri::AppHandle,
    state: &AppState,
    session_id: &str,
    workspace: Option<&Path>,
    mcp_manager: &SharedMcpManager,
    /* …现有所需参数,与现块一致… */
) -> Arc<ToolRegistry>;
```

builtin + memu + browser `bt!` 宏块 + MCP proxy 注册随之搬;`tauri_commands.rs` 只调它。

**模块布局(无 god file):**
- `agent/tool_dispatch/mod.rs` — `ToolDispatcher` / `ToolDispatchContext` / `ToolDispatchOutcome` / `dispatch` 编排。若增大,拆 `approval.rs`(审批)+ `stream_coalesce.rs`(coalescer)子模块。
- `agent/tools/registry_build.rs` — `build_tool_registry`。
- `agent/tools/tool.rs` — 加 `ToolConcurrency` + `concurrency()` 默认。
- `agent/dispatcher.rs` — 减 ~860 行;`execute_tool_calls` 缩为 pre-pass + `dispatch(...)` + bookkeeping。
- `agent/mod.rs` — `pub mod tool_dispatch;`。

---

## 6. 错误处理(已批准 Section 4)

- `ToolError` → emit `chat:stream-tool-activity` error + record,与现状一致;`result` 字段在 outcome 里保留 `Result`,loop 决定续行(留 ChatDelegate)。
- `detect_soft_tool_error`(dispatcher.rs:3507 附近)保留,经 `ToolDispatchOutcome.soft_error` 上报。
- 审批 `Block`/拒绝 → 同 `agent:tool-rejected` 事件;`outcome.rejected = true`。
- 现有 per-tool panic 隔离(`panic_recovery_tests` 覆盖)保留:panic 工具产出 error outcome,不让整批崩。

---

## 7. 测试(已批准 Section 4)

**行为保持是首要约束。**
- 现有 dispatcher 测试(`browser_runtime_dispatch_patch_tests`、`panic_recovery_tests` 等 ~900 行)必须全绿。
- **新隔离单测(ToolDispatcher,无需构造 agent loop):** fake registry + stub tool + fake `SafetyManager`,断言:审批 gate 放行/拦截、path gate、`Parallel` vs `Sequential` 按 `concurrency()` 分批、`PreToolUse`/`PostToolUse` 各发一次、`ToolDispatchOutcome` 携带 `paths_touched`/`was_mutation`/`soft_error`/`rejected`。
- **Hook 测试:** 订阅测试 `HookBus` listener,断言两事件带正确 payload 发射。
- **并行性 parity:** 断言 5 只读工具报 `Parallel`、其余 `Sequential`。

**验收 gate:** `cargo build` 干净;全量 `cargo test --lib` 无新增失败(已知 ~5 个预存无关:daemon_approval / truncate_for_error / browser::runtime_status×2 / gbrain_eval_harness);targeted dispatcher 测试通过;手动 smoke(跑一个工具、跑并行只读、触发一次审批)。

---

## 8. 风险与 rollout

- 最高风险:① 流式 coalescer task 生命周期;② 审批 oneshot await。两者带依赖整体搬迁,逐步抽离、每步保持测试绿。
- 无 DB 迁移、无 IPC 契约变更、无前端变更 → 外部 blast radius 低。
- commit 可二分:trait `concurrency()`(非破坏)→ registry factory → ToolDispatcher 骨架 + outcome → 搬审批/path/execute/coalescer → 统一 serial/parallel + hook 点 → 删 PARALLEL_SAFE_TOOLS + ChatDelegate 变薄 → 测试。
- 与 Sprint 3 ②(Hooks)衔接:`PreToolUse`/`PostToolUse` 已有发射点,② 只需加订阅 + veto/modify 语义。
