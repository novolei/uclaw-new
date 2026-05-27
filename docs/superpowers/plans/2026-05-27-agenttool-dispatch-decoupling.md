# AgentTool 派发缝解耦 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `ChatDelegate::execute_tool_calls`(~860 行)抽离为独立、可隔离测试的 `ToolDispatcher`,统一 serial/parallel 路径,observe-only 点亮 `PreToolUse`/`PostToolUse`,为 Sprint 3 ②(Hooks)提供干净派发缝。行为保持。

**Architecture:** 先做非破坏 prep(`concurrency()` trait 默认、共享 `Arc<HookBus>`、`build_tool_registry` factory);再在 `agent/tool_dispatch/` 里**隔离构建**完整的 `ToolDispatcher`(逐 concern 加 + 单测),最后一次性 cutover 把 `ChatDelegate.execute_tool_calls` 换成薄包装并删旧码。每个 commit 可编译可测。

**Tech Stack:** Rust + Tokio,`async_trait`,Tauri,rusqlite。

**Spec:** `docs/superpowers/specs/2026-05-27-agenttool-dispatch-decoupling-design.md`
**Branch/worktree:** `codex/sprint3-tool-dispatch`(base = main `a85063d6`)

---

## ⚠️ 规划期事实(实现以此为准)

- `ToolCall` 在 **crate** `crates/uclaw-tool-types/src/lib.rs:3-8`:`{ id: String, name: String, arguments: serde_json::Value }`,经 `agent/types.rs:6` re-export。`execute_tool_calls(…, tool_calls: Vec<ToolCall>, …)` 在 dispatcher.rs:2412。tool_call_id = `tc.id`。
- `Tool` trait 在 `agent/tools/tool.rs:206-255`。现有 `ToolExecutionMode { AgentTurn, Direct }`(tool.rs:144-150,`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`)是**别的轴**,勿复用。
- **5 个 `PARALLEL_SAFE_TOOLS`(dispatcher.rs:24-30)里只有 2 个对应真实 struct**:`ReadFileTool`(`agent/tools/builtin/file.rs:29`,`name()=="read_file"`)、`GetFileSkeletonTool`(`agent/tools/builtin/get_file_skeleton.rs:10`,`name()=="get_file_skeleton"`)。`search_files`/`search_codebase`/`list_dir` **无任何 `impl Tool`**(实际搜索工具是 `GrepTool`name=`grep` / `GlobTool`name=`glob`,不在数组里)。⇒ 这 3 个名字是**死项**,删掉它们不改变行为;`concurrency()→Parallel` 只加到上述 2 个 struct。
- `ChatDelegate` **无 `HookBus` 字段**;`memory_policy/executor.rs:66,76` 各自 `HookBus::new()`。**无共享 bus** → 本计划在 `AppState` 加 `Arc<HookBus>`。
- HookBus API(`agent/hook_bus/bus.rs`):`async fn dispatch_observe(&self, event: &HookEvent)`(fire-and-forget,返回 `()`)。事件(`agent/hook_bus/event.rs:13-24`):`HookEvent::PreToolUse { task_id, tool_name, args_json }`、`PostToolUse { task_id, tool_name, success, result_preview }`。
- ChatDelegate 派发相关字段(dispatcher.rs:38-233,**新 ToolDispatcher 用同样类型**):`tools: Arc<ToolRegistry>`、`app_handle: tauri::AppHandle`、`safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>`、`safety_mode: Option<SafetyMode>`、`pending_approvals: Arc<PendingApprovals>`、`conversation_id: String`、`infra_service: Option<Arc<InfraService>>`、`trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>`、`tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>`、`workspace_root: Option<PathBuf>`、`turn_index: Arc<AtomicU32>`。
- registry 装配块:`tauri_commands.rs:1878-2067`(`ToolRegistry::new()` … `let tools = Arc::new(tools);`),**async**(read-locks `state.mcp_manager`)。消费 `app_handle`、`state`(诸多字段)、`input.conversation_id`、`workspace = active_workspace_root(&state).unwrap_or_else(|| state.workspace_root.clone())`。
- 流式 coalescer:dispatcher.rs:2751-2834。`ToolStreamSink::channel(256)→(sink, rx)`;tool 结束 `drop(sink); let _ = handle.await;`。
- `detect_soft_tool_error(result: &serde_json::Value) -> bool`(dispatcher.rs:3513)。path gate:dispatcher.rs:2615-2647(`tool.path_args` → `mgr.check_paths`)。`reason_ctx.file_ops.track_tool_call(&tc.name, &tc.arguments)`(serial:2930 仅 `!soft_error`;parallel:3129 无条件)。`mutations_since_last_plan_done`(2939-2944,`is_mutating_tool`)+ plan_update done 重置(2955-2959)。

---

## 验证命令

- 后端编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
- 单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- 已知预存失败 ~5(daemon_approval / truncate_for_error / browser::runtime_status×2 / gbrain_eval_harness);若 stale tauri 产物报旧 worktree 路径,清 `target/debug/build/tauri-*` + `uclaw-*` 重建。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `agent/tools/tool.rs` | 加 `ToolConcurrency` + `Tool::concurrency()` 默认 |
| `agent/tools/builtin/file.rs`, `…/get_file_skeleton.rs` | 2 个工具 `concurrency()→Parallel` 覆盖 |
| `app.rs` | AppState 加 `hook_bus: Arc<HookBus>` |
| `main.rs` | 构造 `Arc<HookBus>` 注入 AppState |
| `agent/tools/registry_build.rs` | **新建** `build_tool_registry` factory |
| `agent/tool_dispatch/mod.rs` | **新建** `ToolDispatcher` + `ToolDispatchContext` + `ToolDispatchOutcome` + `dispatch` |
| `agent/tool_dispatch/approval.rs`, `stream_coalesce.rs` | (按需)审批 / coalescer 子模块 |
| `agent/mod.rs` | `pub mod tool_dispatch;` |
| `agent/dispatcher.rs` | `execute_tool_calls` 变薄 + 删 PARALLEL_SAFE_TOOLS;ChatDelegate 加 `dispatcher: ToolDispatcher` 字段 |
| `tauri_commands.rs` | 调 `build_tool_registry`;构造 ToolDispatcher |

任务顺序(spec §8):prep(T1-T3)→ 隔离构建 dispatcher(T4-T8)→ cutover(T9)→ 隔离单测收口(T10)。

---

## Task 1: `ToolConcurrency` + `Tool::concurrency()` 默认(非破坏)

**Files:** Modify `src-tauri/src/agent/tools/tool.rs`, `src-tauri/src/agent/tools/builtin/file.rs`, `src-tauri/src/agent/tools/builtin/get_file_skeleton.rs`

- [ ] **Step 1: 失败测试** — 在 `tool.rs` 底部 `#[cfg(test)] mod` 加(若无 test mod 则新建):

```rust
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use crate::agent::tools::builtin::file::ReadFileTool;
    use crate::agent::tools::builtin::get_file_skeleton::GetFileSkeletonTool;
    use crate::agent::tools::builtin::shell::BashTool;
    use std::path::PathBuf;

    #[test]
    fn read_only_tools_are_parallel() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(ReadFileTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
        assert_eq!(GetFileSkeletonTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
    }

    #[test]
    fn other_tools_default_sequential() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(BashTool::new(ws).concurrency(), ToolConcurrency::Sequential);
    }
}
```

- [ ] **Step 2: 确认红** — `cargo test --lib concurrency_tests 2>&1 | grep -E "cannot find|no method"`,预期 `ToolConcurrency` / `concurrency` 未定义。

- [ ] **Step 3: 实现** — `tool.rs`:在 `ToolExecutionMode` 枚举(~line 150)后加:

```rust
/// 工具并发性声明(Pi `executionMode` 子集)。ToolDispatcher 据此把工具分到
/// 串行内联 / 并行 JoinSet 两道。与上面的 `ToolExecutionMode`(调用点模式)是
/// 不同的轴,故另立枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolConcurrency {
    /// 串行(默认):有副作用 / 需审批 / 网络 IO 的工具。
    Sequential,
    /// 并行安全:纯只读工具,可进 JoinSet 批次。
    Parallel,
}
```

在 `Tool` trait 内(`preview_target_path` 默认方法后,tool.rs:~253)加:

```rust
    /// 该工具是否并行安全。默认 `Sequential`(= 旧 PARALLEL_SAFE_TOOLS 白名单之外)。
    /// 只读工具 override 为 `Parallel`。
    fn concurrency(&self) -> ToolConcurrency { ToolConcurrency::Sequential }
```

`builtin/file.rs`:在 `impl Tool for ReadFileTool` 块内加:

```rust
    fn concurrency(&self) -> crate::agent::tools::tool::ToolConcurrency {
        crate::agent::tools::tool::ToolConcurrency::Parallel
    }
```

`builtin/get_file_skeleton.rs`:在 `impl Tool for GetFileSkeletonTool` 块内加同样的 `concurrency()→Parallel`。

> 仅这 2 个工具真实命中旧并行路径(`search_files`/`search_codebase`/`list_dir` 无对应 struct,是死项)。

- [ ] **Step 4: 绿** — `cargo test --lib concurrency_tests 2>&1 | tail -6`(`ok. 2 passed`);`cargo build 2>&1 | grep -E "^error" | head`(空)。

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/agent/tools/tool.rs src-tauri/src/agent/tools/builtin/file.rs src-tauri/src/agent/tools/builtin/get_file_skeleton.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(tools): ToolConcurrency + Tool::concurrency() default (non-breaking)

New ToolConcurrency { Sequential, Parallel } + Tool::concurrency() defaulting to
Sequential. ReadFileTool + GetFileSkeletonTool override to Parallel (the only 2
PARALLEL_SAFE_TOOLS names with real impls). Dispatcher still uses the const array
for now — behavior unchanged.

Verification: cargo test --lib concurrency_tests -> 2 passed; build clean"
```

---

## Task 2: 共享 `Arc<HookBus>` 注入 AppState(非破坏)

**Files:** Modify `src-tauri/src/app.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: AppState 加字段** — `app.rs` `AppState` struct(near 其它 `Arc<...>` 字段)加:

```rust
    /// 共享 Hook 总线 — ToolDispatcher 经此 observe-only 发射 PreToolUse/PostToolUse;
    /// Sprint 3 ② 在同一实例上注册订阅者。
    pub hook_bus: std::sync::Arc<crate::agent::hook_bus::HookBus>,
```

- [ ] **Step 2: 构造初始化** — 在 `AppState { ... }` 字面量(app.rs 构造处)加:

```rust
            hook_bus: std::sync::Arc::new(crate::agent::hook_bus::HookBus::new()),
```

> 若 `HookBus::new()` 不是 `pub` 或需参数,读 `agent/hook_bus/bus.rs` 用其真实构造器;空 bus(无订阅者)即可,本 slice 不加订阅。

- [ ] **Step 3: 编译 + 冒烟测试** — 新增最小测试(`app.rs` 的 test mod 或 hook_bus tests)确认共享 bus 可 `dispatch_observe` 无 panic:

```rust
#[tokio::test]
async fn shared_hook_bus_dispatch_observe_is_noop_without_subscribers() {
    use crate::agent::hook_bus::{HookBus, HookEvent};
    let bus = std::sync::Arc::new(HookBus::new());
    bus.dispatch_observe(&HookEvent::PostToolUse {
        task_id: "t".into(), tool_name: "read_file".into(),
        success: true, result_preview: "ok".into(),
    }).await;
    // 无订阅者 → 不 panic、无副作用
}
```

- [ ] **Step 4: 绿 + 提交** — `cargo build 2>&1 | grep -E "^error"`(空);`cargo test --lib shared_hook_bus 2>&1 | tail -5`。

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/app.rs src-tauri/src/main.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(app): shared Arc<HookBus> in AppState

Adds a process-shared HookBus so the ToolDispatcher (next tasks) can fire
observe-only PreToolUse/PostToolUse, and Sprint 3 ② can subscribe to the same
instance. Empty bus (no subscribers) — behavior unchanged.

Verification: cargo build clean; dispatch_observe no-op test passes"
```

> 注:若 main.rs 不直接构造 AppState(由 app.rs 完成),Step 2 已覆盖,main.rs 可能无需改 —— 以实际构造点为准。

---

## Task 3: `build_tool_registry` factory(从 tauri_commands 抽离;纯移动)

**Files:** Create `src-tauri/src/agent/tools/registry_build.rs`; Modify `src-tauri/src/agent/tools/mod.rs`, `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 读现状** — 通读 `tauri_commands.rs:1878-2067` 整块(`let mut tools = ToolRegistry::new();` … `let tools = Arc::new(tools);`),记下它引用的所有局部:`app_handle`、`state`(`pending_ask_users`/`pending_exit_plans`/`db`/`infra_service`/`memu_client`/`memory_graph_store`/`browser_context_manager`/`mcp_manager`/`settings`/`browser_runtime_status_service`/`browser_identity_task_registry`/`memory_store` 等)、`input.conversation_id`、`workspace`。

- [ ] **Step 2: 新建 factory** — `agent/tools/registry_build.rs`:

```rust
//! 工具注册表装配 —— 从 send_agent_message 处理器抽出,集中一处构建。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::ToolRegistry;
use crate::app::AppState;

/// 构建某会话的工具注册表(builtin + memu + browser + MCP proxy)。
/// async:需 read-lock state.mcp_manager 生成 MCP proxy。
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
) -> Arc<ToolRegistry> {
    // ↓↓↓ 把 tauri_commands.rs:1879-2066 的整块原样移到这里,
    //     把原 `input.conversation_id` 改为 `session_id`,`active_workspace_root(...)`
    //     的结果改为入参 `workspace`(调用方算好传入)。其余逐行不变。
    let mut tools = ToolRegistry::new();
    // … (moved verbatim) …
    Arc::new(tools)
}
```

`agent/tools/mod.rs` 加 `pub mod registry_build;`。

- [ ] **Step 3: 调用点替换** — `tauri_commands.rs` 把 1878-2067 整块替换为:

```rust
    let workspace = active_workspace_root(&state).unwrap_or_else(|| state.workspace_root.clone());
    let tools = crate::agent::tools::registry_build::build_tool_registry(
        app_handle.clone(),
        &state,
        input.conversation_id.clone(),
        workspace,
    ).await;
```

> 若移动后有"借用 state 已被 move"类编译错,按编译器提示把 `&state` 借用顺序或 clone 调整;factory 取 `&AppState`,调用前 state 不可被 move。

- [ ] **Step 4: 编译 + 现有测试** — `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib 2>&1 | tail -5`(无新失败)。纯移动,无新行为。

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/agent/tools/registry_build.rs src-tauri/src/agent/tools/mod.rs src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "refactor(tools): extract build_tool_registry factory from tauri command

Moves the 180-line ToolRegistry assembly out of the send_agent_message handler
into agent/tools/registry_build.rs. Pure move — same tools registered.

Verification: cargo build clean; cargo test --lib -> no new failures"
```

---

## Task 4: `tool_dispatch` 模块骨架 — 类型 + 简单派发(隔离,未接线)

**Files:** Create `src-tauri/src/agent/tool_dispatch/mod.rs`; Modify `src-tauri/src/agent/mod.rs`

> 策略:在 ChatDelegate **之外**隔离构建 ToolDispatcher,逐 concern 加 + 单测;旧 `execute_tool_calls` 暂不动(T9 才 cutover)。本任务先做最简派发(resolve→execute→emit→outcome,无审批/path/stream/hook)。

- [ ] **Step 1: 类型 + 失败测试** — `agent/tool_dispatch/mod.rs`:

```rust
//! ToolDispatcher —— 从 ChatDelegate 抽离的工具派发缝(Sprint 3 ①)。
//! loop-agnostic:不依赖 ReasoningContext;reason_ctx bookkeeping 经 outcome 上报。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::{Tool, ToolRegistry, ToolOutput, ToolError, ToolConcurrency};
use uclaw_tool_types::ToolCall;

/// 每轮不可变派发输入(非 ReasoningContext)。
#[derive(Clone)]
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
}

/// 每个 tool call 的结构化结果,供 loop 做 reason_ctx bookkeeping。
pub struct ToolDispatchOutcome {
    pub tool_call_id: String,
    pub tool_name: String,
    /// 原始 call 参数 —— ChatDelegate cutover(T9)用它做 file_ops.track / is_mutating 等 bookkeeping。
    pub arguments: serde_json::Value,
    pub result: Result<ToolOutput, ToolError>,
    pub paths_touched: Vec<PathBuf>,
    pub was_mutation: bool,
    pub soft_error: Option<String>,
    pub rejected: bool,
}

pub struct ToolDispatcher {
    pub(crate) tools: Arc<ToolRegistry>,
    pub(crate) app_handle: tauri::AppHandle,
    pub(crate) safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    pub(crate) pending_approvals: Arc<crate::agent::PendingApprovals>,
    pub(crate) infra_service: Option<Arc<crate::infra::InfraService>>,
    pub(crate) trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    pub(crate) tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    pub(crate) hook_bus: Arc<crate::agent::hook_bus::HookBus>,
}

impl ToolDispatcher {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle,
        safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
        pending_approvals: Arc<crate::agent::PendingApprovals>,
        infra_service: Option<Arc<crate::infra::InfraService>>,
        trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
        tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
        hook_bus: Arc<crate::agent::hook_bus::HookBus>,
    ) -> Self {
        Self { tools, app_handle, safety_manager, pending_approvals, infra_service, trajectory_store, tool_budget, hook_bus }
    }

    /// 派发一组 tool calls,返回每个的结构化 outcome。
    pub async fn dispatch(&self, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome> {
        let mut out = Vec::with_capacity(calls.len());
        for tc in calls {
            out.push(self.run_one(&tc, ctx).await);
        }
        out
    }

    /// 单个 call 的 per-call 例程(本任务:最简 resolve→execute→outcome;
    /// 后续任务在此插入 approval/path/stream/record/hook)。
    async fn run_one(&self, tc: &ToolCall, _ctx: &ToolDispatchContext) -> ToolDispatchOutcome {
        let Some(tool) = self.tools.get(&tc.name) else {
            return ToolDispatchOutcome {
                tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                result: Err(ToolError::NotFound(tc.name.clone())),
                paths_touched: vec![], was_mutation: false, soft_error: None, rejected: false,
            };
        };
        let result = tool.execute(tc.arguments.clone()).await;
        let soft_error = result.as_ref().ok()
            .and_then(|o| if crate::agent::dispatcher::detect_soft_tool_error(&o.result) {
                Some("soft_error".to_string())
            } else { None });
        ToolDispatchOutcome {
            tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            result,
            paths_touched: vec![],
            was_mutation: crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments),
            soft_error, rejected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "echo" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput { result: json!({ "echoed": params }), cost: None, duration_ms: 0 })
        }
    }

    fn ctx() -> ToolDispatchContext {
        ToolDispatchContext { session_id: "s".into(), conversation_id: "s".into(),
            workspace_root: None, attached_dirs: vec![], safety_mode: None, iteration: 1 }
    }

    #[tokio::test]
    async fn dispatch_executes_and_returns_outcome() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool);
        let d = test_dispatcher(Arc::new(reg));
        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];
        let outs = d.dispatch(calls, &ctx()).await;
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].tool_call_id, "c1");
        assert!(outs[0].result.is_ok());
        assert!(!outs[0].rejected);
    }

    #[tokio::test]
    async fn unknown_tool_yields_not_found_outcome() {
        let d = test_dispatcher(Arc::new(ToolRegistry::new()));
        let calls = vec![ToolCall { id: "c1".into(), name: "nope".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;
        assert!(matches!(outs[0].result, Err(ToolError::NotFound(_))));
    }

    // 测试用 dispatcher 构造:见下方 helper(用真实 AppHandle 较重,
    // 故测试只构造 dispatch 不需要的依赖为 mock/None —— 见 Step 3 备注)。
    fn test_dispatcher(tools: Arc<ToolRegistry>) -> ToolDispatcher { test_only::make(tools) }
}
```

> **AppHandle 在单测中的处理(关键):** `ToolDispatcher` 持有 `tauri::AppHandle`,单测难造。在本任务实现时,把 dispatcher 对 `app_handle` 的使用收敛到 emit helper 后,提供一个 `#[cfg(test)] mod test_only` 构造器 —— 它用 `tauri::test::mock_builder()`/`mock_app()` 造一个测试 AppHandle(uclaw 既有测试若有此模式则复用;否则把 emit 抽象成一个 `EventSink` trait,dispatcher 持 `Arc<dyn EventSink>`,测试传 no-op sink)。**实现者按 codebase 既有测试是否已有 mock AppHandle 二选一,并在 PR 说明。** 若选 EventSink 抽象,则 §Task 8 的 emit 走该 trait。

- [ ] **Step 2: 注册模块** — `agent/mod.rs` 加 `pub mod tool_dispatch;`。确保 `detect_soft_tool_error` 对 `tool_dispatch` 可见(现为 `pub(crate)`,够用)。

- [ ] **Step 3: 确认红 → 实现 → 绿** — `cargo test --lib tool_dispatch:: 2>&1 | tail -8`(2 passed)。

- [ ] **Step 4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/agent/tool_dispatch/mod.rs src-tauri/src/agent/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(agent): ToolDispatcher skeleton + Context/Outcome types (isolated)

New agent/tool_dispatch: ToolDispatchContext, ToolDispatchOutcome, ToolDispatcher
with a minimal resolve->execute->outcome dispatch + isolation unit tests. Not yet
wired into ChatDelegate.

Verification: cargo test --lib tool_dispatch:: -> 2 passed"
```

---

## Task 5: 审批门移入 dispatch + 测试

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`(或新增 `tool_dispatch/approval.rs`)

- [ ] **Step 1: 移植审批逻辑** — 把 dispatcher.rs:2490-2601 的审批流(`tool.requires_approval` → Yolo 短路 → `mgr.should_approve_with_db(...)` → `RequireApproval` 时 emit `agent:need_approval` + await `pending_approvals` oneshot → `always_allow` 触发 `add_auto_approved`)移入 `run_one` 的 execute **之前**,作为私有 `async fn approve(&self, tc, ctx) -> ApprovalGate`(返回 Allow / Rejected{reason})。`Block`/拒绝 ⇒ `run_one` 直接返回 `outcome.rejected = true`、`result = Err(ToolError::Execution(reason))`,并 emit 现有 `agent:tool-rejected`(逐行照搬该 emit)。

> 变量替换:`self.conversation_id` → `ctx.conversation_id`;`self.safety_mode` → `ctx.safety_mode`;其余 `self.safety_manager` / `self.pending_approvals` / `self.app_handle` 同名直用。

- [ ] **Step 2: 测试** — 加单测:fake `SafetyManager`(或经 SafetyPolicy 配 `blocked_tools` / `auto_approved_tools`)断言:被 block 的工具 → `rejected==true` 且不执行;auto-approve 工具 → 正常执行。(若 SafetyManager 难 mock,用其真实构造 + `SafetyPolicy { blocked_tools: {"echo"}, .. }`。)

```rust
    #[tokio::test]
    async fn blocked_tool_is_rejected_not_executed() {
        // 配 SafetyManager 把 "echo" 列入 blocked,dispatch 后断言 outs[0].rejected == true
        // 且 result 为 Err,且工具未真正执行(EchoTool 可加 AtomicBool 记录是否被调用)。
    }
```

- [ ] **Step 3: 绿 + 提交** — `cargo test --lib tool_dispatch:: 2>&1 | tail -8`。

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(agent): move approval gating into ToolDispatcher

Approval flow (requires_approval -> Yolo short-circuit -> should_approve_with_db
-> need_approval emit + oneshot await -> always_allow whitelist) moved into the
dispatcher's per-call routine. Blocked/rejected calls return rejected outcomes.

Verification: cargo test --lib tool_dispatch:: -> passed"
```

---

## Task 6: path-policy 门移入 dispatch + 测试

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`

- [ ] **Step 1: 移植 path gate** — 把 dispatcher.rs:2615-2647(`tool.path_args(&tc.arguments)` → 相对路径用 `ctx.workspace_root` 拼绝对 → `load_attached_dirs_for_session(&self.app_handle, &ctx.conversation_id)` → `mgr.check_paths(...)` → `PathDecision`)移入 `run_one`,在审批门**之后**、execute **之前**。`check_paths` 返回 `Block`/`Prompt` 的处理逐行照搬(prompt 走与审批相同的 oneshot/emit `kind:"path"`)。把命中的 candidate paths 填入 `outcome.paths_touched`。

> `self.workspace_root` → `ctx.workspace_root`;`self.conversation_id` → `ctx.conversation_id`。

- [ ] **Step 2: 测试** — fake 一个 `path_args` 返回工作区外路径的工具,配 PathPolicy 拦截,断言 `rejected`/`paths_touched` 行为。(若 prompt 路径难测,至少覆盖 Allow + Block 两分支。)

- [ ] **Step 3: 绿 + 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(agent): move path-policy gating into ToolDispatcher

path_args -> check_paths -> PathDecision moved into the per-call routine after
approval, before execute. Candidate paths recorded in outcome.paths_touched.

Verification: cargo test --lib tool_dispatch:: -> passed"
```

---

## Task 7: execute + 流式 coalescer + serial/parallel(concurrency)+ 测试

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`(coalescer 可拆 `tool_dispatch/stream_coalesce.rs`)

- [ ] **Step 1: 移植执行 + coalescer** — 把 dispatcher.rs:2751-2834 的 coalescer 块抽成私有 `async fn run_tool(&self, tool: &dyn Tool, tc, ctx) -> Result<ToolOutput, ToolError>`:`tool.supports_streaming()` 为真时建 `ToolStreamSink::channel(256)`、spawn drain task(逐行照搬 emit `chat:stream-tool-activity tool_output_chunk`,`self.conversation_id`→`ctx.conversation_id`)、调 `tool.execute_streaming(...)`、结束 `drop(sink); handle.await`;否则 `tool.execute(params)`。替换 `run_one` 里的直接 `tool.execute`。

- [ ] **Step 2: serial/parallel 分道** — 改 `dispatch`:遍历 calls,`tool.concurrency()==Parallel` 的收进 batch,经 `tokio::task::JoinSet` 并发跑 `run_one`;`Sequential` 顺序内联。两道都走 `run_one`。保持"approval/path 在执行前、按出现顺序"的语义(并行 batch 内的 call 互不依赖 —— 与现状一致,只读工具)。

> ⚠️ `run_one` 需 `&self` 跨 JoinSet:把 `self` 包成 `Arc<ToolDispatcher>` 或让 batch 任务 clone 所需 `Arc` 字段。实现者按编译器借用要求选最小改动(推荐 `dispatch(self: &Arc<Self>, ...)` 或内部 `Arc::clone`)。

- [ ] **Step 3: 测试** — (a) 一个 `concurrency()==Parallel` 的 stub 工具 + 一个 `Sequential` 的,断言两者都被执行、outcome 齐全;(b) streaming stub 工具(`supports_streaming()==true`,经 sink 发 2 个 chunk)断言 execute_streaming 被走、最终 outcome ok。(emit 经测试 EventSink/mock AppHandle,见 Task 4 备注。)

- [ ] **Step 4: 绿 + 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(agent): move execute + streaming coalescer + serial/parallel into ToolDispatcher

run_tool owns the bash streaming coalescer (ToolStreamSink + drain task). dispatch
splits calls by tool.concurrency(): Parallel -> JoinSet batch, Sequential -> inline.

Verification: cargo test --lib tool_dispatch:: -> passed"
```

---

## Task 8: 记录(trajectory/infra/token-budget)+ observe-only hook 点 + 测试

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`

- [ ] **Step 1: 移植记录 + emit** — 在 `run_one` 的 execute **之后**移入:tool-budget 截断(dispatcher.rs:2850-2855)、emit `chat:stream-tool-activity` result/error(照搬)、trajectory 写入(2860-2880)、InfraService `publish_tool_executed`(2884-2898)。`self.turn_index`/`self.trajectory_store`/`self.infra_service`/`self.tool_budget` 同名直用(turn_index 改为从 `ctx.iteration` 或 dispatcher 持有的计数 —— 以现用法为准,实现者对齐)。

- [ ] **Step 2: observe-only hook** — 在审批+path 通过后、execute 前:

```rust
self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PreToolUse {
    task_id: ctx.session_id.clone(),
    tool_name: tc.name.clone(),
    args_json: tc.arguments.to_string(),
}).await;
```

execute 后:

```rust
let success = matches!(&result, Ok(o) if !crate::agent::dispatcher::detect_soft_tool_error(&o.result));
let result_preview = match &result {
    Ok(o) => crate::agent::dispatcher::truncate_utf8(&o.result.to_string(), 256),
    Err(e) => format!("{e}"),
};
self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostToolUse {
    task_id: ctx.session_id.clone(),
    tool_name: tc.name.clone(),
    success, result_preview,
}).await;
```

> `truncate_utf8` 在 dispatcher.rs ~3507 区(`pub(crate)`)。若不可见则内联简单截断。

- [ ] **Step 3: hook 测试** — 实现一个测试 `HookSubscriber`(`interest_in` 含 `PreToolUse`+`PostToolUse`,`on_event` 把事件 push 进 `Arc<Mutex<Vec<HookEvent>>>`),用 `HookBus::builder`/`register`(按 bus.rs 真实 API)注册后构造 dispatcher,dispatch 一个工具,断言收到恰好一个 PreToolUse + 一个 PostToolUse,payload 的 `tool_name` 正确。

- [ ] **Step 4: 绿 + 提交**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "feat(agent): tool recording + observe-only PreToolUse/PostToolUse in ToolDispatcher

Moves token-budget truncation, result/error emit, trajectory + InfraService
recording into the per-call routine; fires observe-only PreToolUse (before execute)
and PostToolUse (after) through the shared HookBus. No subscribers in this slice.

Verification: cargo test --lib tool_dispatch:: -> passed (incl. hook fire test)"
```

---

## Task 9: cutover —— ChatDelegate 用 ToolDispatcher,删旧码

**Files:** Modify `src-tauri/src/agent/dispatcher.rs`, `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: ChatDelegate 持有 dispatcher** — 给 `ChatDelegate` 加字段 `dispatcher: crate::agent::tool_dispatch::ToolDispatcher`,在其构造处(`ChatDelegate::new` / setter)用现有 deps + `state.hook_bus` 构造(经 `ToolDispatcher::new(...)`)。`tauri_commands.rs` 构造 ChatDelegate 处把 `hook_bus` 传入。

- [ ] **Step 2: execute_tool_calls 变薄** — 把 `execute_tool_calls`(2410-3270)整体替换为:
  1. `plan_update` 反伪进展 pre-pass(照搬 2440-2488 那段,**留在此处**)。
  2. 构造 `ToolDispatchContext`(从 `reason_ctx` / self 取 session_id/conversation_id/workspace_root/attached_dirs/safety_mode/iteration)。
  3. `let outcomes = self.dispatcher.dispatch(tool_calls, &ctx).await;`
  4. 遍历 outcomes 应用 reason_ctx bookkeeping:
     - `if outcome.soft_error.is_none() { reason_ctx.file_ops.track_tool_call(&outcome.tool_name, &outcome.arguments); }` — 照搬 2930 的语义(`outcome.arguments` 已在 Task 4 结构里)。
     - `if outcome.was_mutation && outcome.soft_error.is_none() { reason_ctx.mutations_since_last_plan_done += 1 }`;plan_update done 重置(照搬 2955-2959)。
     - `recent_tool_errors`(GEP)从 `outcome.soft_error` 收集(照搬 2905-2918)。
     - `exit_plan_mode` 拒绝 UserCorrection(照搬 2980-3005)。
  5. 把 outcomes 转回 `execute_tool_calls` 原返回类型(原是 `Vec<(ToolCall, Result<...>)>` 或类似 —— 以现签名为准,用 `outcome.result` 重建)。
- [ ] **Step 3: 删死码** — 删 `PARALLEL_SAFE_TOOLS`(24-30)+ `is_parallel_safe`(32);删原 serial 路径 + JoinSet 批次(已移入 dispatcher)。

> ⚠️ 本任务是高风险 cutover。`outcome.arguments`(Task 4 已定义)是 bookkeeping 的数据来源;cutover 后 reason_ctx 的更新全部从 outcomes 读,不再内联。

- [ ] **Step 4: 行为保持验证** — `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib dispatcher 2>&1 | tail -15`(现有 ~71 + browser_runtime_dispatch_patch_tests + panic_recovery_tests **全过**);`cargo test --lib 2>&1 | tail -6`(仅 ~5 已知预存失败,无新失败)。

- [ ] **Step 5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/agent/dispatcher.rs src-tauri/src/agent/tool_dispatch/mod.rs src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "refactor(agent): cut ChatDelegate.execute_tool_calls over to ToolDispatcher

execute_tool_calls is now: plan_update pre-pass + dispatcher.dispatch + reason_ctx
bookkeeping from outcomes. ~860 inline lines + PARALLEL_SAFE_TOOLS const removed.
Behavior-preserving (existing dispatcher tests green).

Verification: cargo test --lib dispatcher -> all pass; full suite -> no new failures"
```

---

## Task 10: 隔离单测收口 + 并发 parity

**Files:** Modify `src-tauri/src/agent/tool_dispatch/mod.rs`

- [ ] **Step 1: 补全 spec §7 断言** — 在 tool_dispatch tests 补:`ToolDispatchOutcome` 携带 `paths_touched`/`was_mutation`/`soft_error`/`rejected`/`arguments` 的端到端断言;Parallel vs Sequential 分批 parity(已在 Task7,确认覆盖)。

- [ ] **Step 2: 全量验证 gate** — `cargo test --lib 2>&1 | tail -6`,确认仅 ~5 已知预存失败。手动 smoke 提示写入 PR(`cargo tauri dev`:跑一个工具、并行读 3 文件、触发一次审批)。

- [ ] **Step 3: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch add src-tauri/src/agent/tool_dispatch/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-tool-dispatch commit -m "test(agent): ToolDispatcher isolation coverage + concurrency parity

Verification: cargo test --lib -> no new failures (5 known pre-existing)"
```

---

## 最终验收

- [ ] `cargo build` → 空 error
- [ ] `cargo test --lib dispatcher` + `tool_dispatch::` → 全过
- [ ] 全量 `cargo test --lib` → 仅 ~5 已知预存失败
- [ ] `PARALLEL_SAFE_TOOLS` / `is_parallel_safe` 已删;`execute_tool_calls` 已变薄
- [ ] `PreToolUse`/`PostToolUse` 经共享 `Arc<HookBus>` observe-only 发射(无订阅者 → 行为不变)
- [ ] 手动 smoke:工具执行 / 并行只读 / 审批 三条路径正常

---

## Self-Review

**Spec coverage:** §1 范围→T1-T10;§3 loop-agnostic dispatcher + Context/Outcome→T4;§4 per-call 流水线 + 审批/path/coalescer/hook→T5/T6/T7/T8;§5 concurrency()→T1、registry factory→T3、模块布局→T4;§6 错误处理(soft_error/rejected/panic)→T4/T5/T9;§7 测试→T4-T8/T10;§8 commit 序列→T1-T10 顺序一致。共享 HookBus(spec 未显式,现状无共享 bus)→T2(已在 handoff 标注为 spec 细化)。

**Placeholder scan:** Task4/Task9 的"AppHandle 单测 mock 二选一(mock_app vs EventSink)"与"outcome.arguments 回填"是给实现者的**明确决策点 + 具体两选项 + 回填指令**,非 TBD。bulk 移植用精确源行号 + 变量替换规则(`self.conversation_id`→`ctx.conversation_id` 等),非"类似 Task N"。无 TODO/TBD。

**Type consistency:** `ToolConcurrency{Sequential,Parallel}`(T1)→ `tool.concurrency()` 分道(T7)一致;`ToolDispatchContext{session_id,conversation_id,workspace_root,attached_dirs,safety_mode,iteration}`(T4)→ T5/T6/T8 取用一致;`ToolDispatchOutcome{tool_call_id,tool_name,arguments,result,paths_touched,was_mutation,soft_error,rejected}`(T4 完整定义,T9 读取);`dispatch(calls, &ctx)->Vec<ToolDispatchOutcome>`(T4)→ T9 调用一致;`HookEvent::PreToolUse{task_id,tool_name,args_json}` / `PostToolUse{task_id,tool_name,success,result_preview}`(T8)与 event.rs 实际字段一致;`Arc<HookBus>`(T2 AppState)→ T4 字段 → T8 `dispatch_observe` 一致。
