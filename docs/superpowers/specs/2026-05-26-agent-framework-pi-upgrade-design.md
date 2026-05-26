# UClaw Agent Framework — Pi 对标升级设计报告

**日期**: 2026-05-26（追加更新 2026-05-26）  
**范围**: Agent Loop · Tool注册 · Prompt构建 · 历史压缩 · Hooks · 会话持久化 · 多模型 · Coding Agent · **双交互队列 · 迭代式压缩 · FileOps持久记忆 · Bash输出保护**  
**目标用户**: 日常办公用户 + Coding 开发者  
**架构前提**: 保持 Rust + Tauri，移植 Pi 的设计模式（不做语言迁移）  
**战略方向**: UClaw Agent 核心向 Pi 框架对齐融合，以 Rust idiom 实现 Pi 全部关键设计

---

## 总体对比快照

| 维度 | UClaw 现状 | Pi 设计 | 差距等级 |
|------|-----------|---------|---------|
| Agent Loop 扩展点 | 6阶段固定循环 + LoopDelegate trait | 双层循环 + prepareNextTurn + shouldStopAfterTurn | 🔴 HIGH |
| **双交互队列** | SoftInterruptQueue 单队列轮询 | steer()实时插入 + followUp()串行后续，语义分离 | 🔴 HIGH |
| Tool 注册 | dispatcher.rs 单体 4026 行 | AgentTool trait + 并行执行 + onUpdate流式 | 🔴 HIGH |
| Prompt 构建 | 静态10块baseline + InjectionPolicy | 动态Provider fn + Skills XML注入 | 🟡 MEDIUM |
| 历史压缩 | 逻辑标记(compacted:bool) + 硬截断 | **迭代式LLM摘要** (UPDATE_SUMMARIZATION_PROMPT) | 🔴 HIGH |
| **Split-Turn恢复** | 无，压缩碰到ToolCall中断即报错 | isSplitTurn + turnPrefix + Active Suffix 无缝衔接 | 🔴 HIGH |
| **FileOps持久记忆** | 无 | compaction携带readFiles/modifiedFiles永久记忆 | 🔴 HIGH |
| **Bash输出保护** | 无上限，大输出可能阻塞IPC | Rolling Tail Buffer + 溢出落盘temp文件 | 🔴 HIGH |
| Hooks系统 | 无，依赖Tauri单向emit | 双向Hook(observe/on) + Phantom Type安全 | 🔴 HIGH |
| 会话持久化 | 3表SQLite线性历史 | JSONL追加写 + 树形多分支导航 | 🟡 MEDIUM |
| 多模型支持 | 仅Anthropic Claude | 10+ 提供商统一抽象 | 🔴 HIGH |
| Coding Agent | code_rescue workaround + 工具三处注册 | 规范工具接口 + 流式bash输出 | 🟡 MEDIUM |

---

## Section 1 — Agent Loop 设计

### 1.1 现状深度分析

UClaw 的 `agentic_loop.rs`（1811行）实现了经典的 React（Reason-Act-Observe）循环。结构清晰但扩展点硬编码：

```rust
// 固定六阶段，无法在turn边界插入动态逻辑
for iteration in 1..=config.max_iterations {
    // 1. check_signals()
    // 2. compress_context_if_needed()
    // 3. delegate.before_llm_call()   ← 唯一扩展点，返回 Option<LoopOutcome>
    // 4. delegate.call_llm()
    // 5. handle response (Text → TextAction | ToolCalls → execute)
    // 6. delegate.after_iteration()
}
```

**核心痛点**：
- `ReasoningContext` 是可变共享状态，在 `&mut` 引用下单向流动——无法在 turn 边界做快照
- 模型/工具集/系统prompt的变更会即时生效，无隔离边界，可能影响进行中的 LLM 调用
- `TextAction` 枚举（Return/Continue/ContinueWithNudge/RescueWithToolCalls）把loop逻辑散落在 dispatcher.rs 中
- 没有 `shouldStopAfterTurn`——应用层无法细粒度控制停止时机（如"用户点了暂停"）
- 没有 `prepareNextTurn`——无法实现"规划阶段用Sonnet，执行阶段用Haiku"的模型动态切换

### 1.2 Pi 的事件驱动 + 快照隔离模型

Pi 的 `agent-loop.ts`（742行）采用双层循环 + 快照隔离：

```typescript
// 外层：处理 follow-up 消息队列
while (true) {
  // 内层：处理工具调用 + steering 消息
  while (hasMoreToolCalls || pendingMessages.length > 0) {
    const message = await streamAssistantResponse(currentContext, config, signal, emit);
    const { messages: toolResults, terminate } = await executeToolCalls(...);
    
    // ← 关键：turn结束后的快照创建点
    const nextTurnSnapshot = await config.prepareNextTurn?.(nextTurnContext);
    if (nextTurnSnapshot) {
      currentContext = nextTurnSnapshot.context ?? currentContext;
      config = { ...config, model: nextTurnSnapshot.model ?? config.model };
    }
    
    // ← 应用层停止控制
    if (await config.shouldStopAfterTurn?.({ message, toolResults, context, newMessages })) {
      return;
    }
    
    pendingMessages = await config.getSteeringMessages?.() || [];
  }
  
  // 外层：follow-up消息检查（agent本要停止，但有新消息进来）
  const followUpMessages = await config.getFollowUpMessages?.() || [];
  if (followUpMessages.length === 0) break;
  pendingMessages = followUpMessages;
}
```

**快照隔离的本质**：`config` 对象在每个 turn 边界（`prepareNextTurn`）被原子替换，进行中的 LLM 调用持有旧快照引用，不受新配置影响。

### 1.3 Brainstorming：Rust 中的快照隔离实现

#### 方案 A：TurnSnapshot + Arc 替换（推荐）

```rust
/// 每个 turn 开始时创建的不可变快照
#[derive(Clone)]
pub struct TurnSnapshot {
    pub model: String,
    pub system_prompt: String,    // 当前 turn 的系统 prompt（已渲染）
    pub tools: Vec<ToolDefinition>,
    pub stream_options: StreamOptions,
    pub turn_index: u32,
}

/// 应用层可在 turn 边界注入的变更
pub struct NextTurnPatch {
    pub model: Option<String>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub inject_message: Option<String>,  // 转向消息
    pub should_stop: bool,
}

/// turn 边界回调 trait
#[async_trait]
pub trait TurnBoundaryDelegate: Send + Sync {
    /// 在 turn 结束后调用，返回下一个 turn 的变更
    async fn prepare_next_turn(&self, ctx: &TurnContext) -> Option<NextTurnPatch>;
    /// steer 消息队列
    async fn get_steering_messages(&self) -> Vec<ChatMessage>;
    /// follow-up 消息队列（agent本要停止时检查）
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage>;
}
```

**状态分离**：
- `TurnSnapshot`：不可变，每 turn 克隆，LLM 调用只持有 `Arc<TurnSnapshot>`
- `ReasoningContext`：可变累加器，只存 token 计数、迭代计数、压缩状态、消息列表

**双层循环结构**（Rust 版）：
```rust
pub async fn run_agentic_loop_v2(
    delegate: &dyn LoopDelegate,
    boundary: &dyn TurnBoundaryDelegate,
    reason_ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
) -> LoopOutcome {
    let mut current_snapshot = Arc::new(create_initial_snapshot(delegate, reason_ctx).await);
    
    'outer: loop {
        let mut has_more_tools = true;
        let mut pending = boundary.get_steering_messages().await;
        
        'inner: while has_more_tools || !pending.is_empty() {
            // 注入转向消息
            for msg in pending.drain(..) {
                reason_ctx.messages.push(msg);
            }
            
            // LLM 调用持有快照的 Arc 引用（不受外部变更影响）
            let snapshot = Arc::clone(&current_snapshot);
            let output = delegate.call_llm_with_snapshot(&snapshot, reason_ctx).await?;
            
            // 执行工具调用
            let (tool_results, terminate) = delegate.execute_tool_calls(&output, reason_ctx).await?;
            has_more_tools = !terminate;
            
            // ← turn 边界：创建下一快照
            let turn_ctx = TurnContext { output: &output, tool_results: &tool_results };
            if let Some(patch) = boundary.prepare_next_turn(&turn_ctx).await {
                if patch.should_stop { break 'outer; }
                current_snapshot = Arc::new(apply_patch(&current_snapshot, patch));
            }
            
            pending = boundary.get_steering_messages().await;
        }
        
        // 外层：follow-up 检查
        let follow_ups = boundary.get_follow_up_messages().await;
        if follow_ups.is_empty() { break; }
        pending = follow_ups;
    }
    
    LoopOutcome::Completed
}
```

#### 方案 B：渐进式改造（保留现有结构，只加回调）

在现有六阶段循环的 `after_iteration` 钩子里插入：
- `delegate.prepare_next_turn()` 返回可选的模型变更
- `delegate.should_stop_after_turn()` 返回是否终止

**优点**：改动最小，风险最低  
**缺点**：没有真正的快照隔离，配置变更仍可能影响进行中调用

#### 方案 C：完全事件流化

将 loop 改为事件生产者（`tokio::sync::broadcast`），每个阶段发射事件，外部消费者可注入。架构最灵活但复杂度最高，短期内过度设计。

### 1.4 快照隔离 ROI 评估

| 收益项 | 影响 | 实现难度 |
|--------|------|---------|
| 消除配置变更竞态 | 高：多模型切换不再有中途配置污染 | 中 |
| per-turn 模型动态切换 | 极高：Sonnet规划→Haiku执行成本优化 | 低（有了snapshot后）|
| 细粒度停止控制 | 高：企业场景每步审批、用户暂停 | 低 |
| 转向消息队列（steer） | 高：用户打字时的实时注入 | 低 |
| follow-up 消息（外层） | 中：agent停止后继续处理新消息 | 低 |
| Harness 测试确定性 | 高：快照可回放，测试稳定性↑ | 中 |

**建议**：采用方案 A（完整快照隔离），与方案 B 相比，额外成本约 2 天，但解锁多模型和 harness 测试两个大型后续功能。

**实施估计**：方案 A 约 4-5 天，方案 B 约 2 天。

---

## Section 2 — Tool 注册与分发

### 2.1 现状深度分析

`dispatcher.rs`（4026行）是 UClaw 最大的单体文件，承担过多职责：

- **工具执行编排**（`execute_tool_calls`）
- **文本响应处理**（`handle_text_response` → `TextAction`）
- **Safety/批准流**
- **代码救援**（`code_rescue`）
- **工具预览**、**工具活动事件**、**工具预算管理**
- **上下文压缩**辅助

新增一个工具需要修改三处：
1. `dispatcher.rs` — 执行逻辑
2. `tauri_commands.rs` — Tauri 命令注册
3. `main.rs` 的 `invoke_handler!` 宏 — 运行时分发

没有工具级并行执行声明，没有工具内流式更新。

### 2.2 Pi 的工具接口设计

```typescript
export interface AgentTool<TParameters extends TSchema = TSchema, TDetails = any> {
  name: string;
  label: string;
  description: string;
  parameters: TParameters;              // TypeBox Schema，自动验证
  executionMode?: ToolExecutionMode;   // "sequential" | "parallel"（默认 parallel）
  prepareArguments?: (args: unknown) => Static<TParameters>;  // 兼容垫片
  execute: (
    toolCallId: string,
    params: Static<TParameters>,
    signal?: AbortSignal,
    onUpdate?: AgentToolUpdateCallback<TDetails>,  // ← 流式更新回调
  ) => Promise<AgentToolResult<TDetails>>;
}

export interface AgentToolResult<T> {
  content: (TextContent | ImageContent)[];  // 返回给模型的内容
  details: T;                                // 结构化详情供UI显示
  terminate?: boolean;                       // 信号提前终止loop
}
```

**执行策略**（Pi的实现）：
```typescript
// 并行批：所有 parallel 工具并发执行，按完成顺序发出事件，按源顺序聚合结果
// 顺序批：sequential 工具逐个执行
const groups = groupByExecutionMode(toolCalls);
for (const group of groups) {
  if (group.mode === "parallel") {
    results = await Promise.all(group.calls.map(exec));
  } else {
    for (const call of group.calls) {
      results.push(await exec(call));
    }
  }
}
```

### 2.3 Brainstorming：Rust 工具插件化架构

#### 核心接口设计

```rust
/// 工具执行模式
#[derive(Default, Clone)]
pub enum ToolExecutionMode {
    #[default]
    Parallel,
    Sequential,
}

/// 工具更新回调（流式进度报告）
pub type ToolUpdateSender = tokio::sync::mpsc::UnboundedSender<ToolUpdate>;

pub struct ToolUpdate {
    pub tool_call_id: String,
    pub content: String,      // 中间状态描述
    pub progress: Option<f32>, // 0.0-1.0
}

/// 工具结果
pub struct AgentToolResult {
    pub content: Vec<ContentBlock>,    // 返回给LLM的内容
    pub details: serde_json::Value,    // UI显示的结构化数据
    pub terminate: bool,               // 是否提前终止loop
    pub file_ops: FileOps,             // 追踪的文件操作（压缩用）
}

/// 核心工具 trait
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn label(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;  // JSON Schema
    
    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel  // 默认并行
    }
    
    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        signal: CancellationToken,
        on_update: Option<ToolUpdateSender>,
    ) -> Result<AgentToolResult, ToolError>;
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: impl AgentTool + 'static) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }
    
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| ToolDefinition {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.schema(),
        }).collect()
    }
}
```

#### 并行执行引擎

```rust
pub async fn execute_tool_calls_parallel(
    registry: &ToolRegistry,
    tool_calls: Vec<ToolCall>,
    signal: CancellationToken,
    update_tx: ToolUpdateSender,
) -> Vec<(ToolCall, AgentToolResult)> {
    // 按执行模式分组
    let groups = group_by_execution_mode(registry, &tool_calls);
    let mut results = Vec::new();
    
    for group in groups {
        match group.mode {
            ToolExecutionMode::Parallel => {
                // tokio::task::JoinSet 并行执行
                let mut join_set = JoinSet::new();
                for call in group.calls {
                    let tool = registry.get(&call.name).clone();
                    let tx = update_tx.clone();
                    let sig = signal.clone();
                    join_set.spawn(async move {
                        let result = tool.execute(&call.id, call.arguments, sig, Some(tx)).await;
                        (call, result)
                    });
                }
                while let Some(r) = join_set.join_next().await {
                    results.push(r.unwrap());
                }
            }
            ToolExecutionMode::Sequential => {
                for call in group.calls {
                    let tool = registry.get(&call.name);
                    let result = tool.execute(&call.id, call.arguments.clone(), signal.clone(), Some(update_tx.clone())).await;
                    results.push((call, result.unwrap_or_else(|e| AgentToolResult::error(e))));
                }
            }
        }
    }
    results
}
```

#### Dispatcher 重构路径

**Phase 1（3天）**：抽取 `AgentTool` trait，将现有工具逐一迁移至 `tools/` 模块目录，保留 dispatcher.rs 作为调用层。

**Phase 2（2天）**：实现 `ToolRegistry`，替换 dispatcher 内的 match 分发为 registry 查找。

**Phase 3（2天）**：引入并行执行引擎，为 bash/read_file 等工具添加 `onUpdate` 流式回调。

### 2.4 ROI 评估

| 改进项 | 用户价值 | 开发成本 |
|--------|---------|---------|
| AgentTool trait 规范化 | 新增工具从3处改→1处，开发速度5x | 3天 |
| 并行工具执行 | coding 用户：同时读多文件速度2x+ | 2天 |
| onUpdate 流式回调 | bash长命令实时输出，用户体验↑↑ | 2天 |
| ToolRegistry 自注册 | 未来支持用户自定义工具 | 已含在上述3天 |
| terminate 信号 | 工具可主动触发loop结束 | <1天 |

**总 ROI**：7天工作量，dispatcher.rs从4026行缩减到~1000行，并解锁并行执行和流式输出。

---

## Section 3 — Prompt 构建系统

### 3.1 现状深度分析

UClaw 的 prompt 构建是**静态分层组合**：

```rust
// compose_system_prompt_with_injection 生成的固定结构
[user_global_base]            // 用户全局配置
[workspace uclaw.md]          // 项目上下文
[WORKSPACE: /path/to/cwd]    // 工作区路径块
[Karpathy Baseline]           // 行为守则
[Mode-specific additions]     // 按 SafetyMode 追加

// baseline_blocks.rs 的 10 块，按 InjectionPolicy 条件注入
pub enum InjectionPolicy {
    Always,
    FirstActTurnOnly,
    OnErrorRecovery,     // 上一轮有工具错误时
    OnContextPressure,   // context > 75% 时
}
```

**B2缓存策略**（重要）：系统 prompt 字节稳定（静态），时间/内存/fragments 放 per-turn 动态块，最大化 Anthropic 缓存命中。这是 UClaw 的成本优化核心——不能轻易破坏。

**缺口**：
1. 无 Skills 标准化注入（当前 gstack skills 在 UI 层处理，不在 agent 系统 prompt 中）
2. 无动态 provider fn（无法根据当前模型/工具集/会话状态调整 prompt）
3. 无 PromptTemplate 系统（用户无法定义自己的 slash 命令模板）

### 3.2 Pi 精确设计分析

#### 3.2.1 动态 SystemPrompt Provider fn

Pi 的 `agent-harness.ts` 的 `createTurnState()`（第 313 行）在每个 turn 开始时调用：

```typescript
// agent-harness.ts:313 — createTurnState() 在每个 turn 边界执行一次
private async createTurnState(): Promise<AgentHarnessTurnState<...>> {
  const activeTools = this.activeToolNames.map(n => this.tools.get(n))...;
  
  let systemPrompt = "You are a helpful assistant.";
  if (typeof this.systemPrompt === "string") {
    systemPrompt = this.systemPrompt;                           // 静态字符串，直接用
  } else if (this.systemPrompt) {
    systemPrompt = await this.systemPrompt({                   // 动态函数，每 turn 求值
      env: this.env,
      session: this.session,
      model: this.model,             // ← 知道当前用哪个模型
      thinkingLevel: this.thinkingLevel,
      activeTools,                   // ← 知道本 turn 激活了哪些工具
      resources: this.getResources(),
    });
  }
  return { systemPrompt, model: this.model, tools, activeTools, ... };
}
// 关键：返回值是快照，整个 turn 期间的所有 LLM 调用共享同一份 systemPrompt。
// 本 turn 运行期间对 this.systemPrompt 的修改不影响当前 turn。
```

**设计要点**：
1. `systemPrompt` 可以是 `string` 或 `async (ctx) => Promise<string>` — 同一接口兼容静态和动态
2. 动态函数的求值发生在 **turn 开始前**，结果被快照进 `TurnState`（与 Section 1 快照隔离对齐）
3. `activeTools` 参数让 prompt 可以根据"当前有没有 bash 工具"动态调整安全警示

#### 3.2.2 formatSkillsForSystemPrompt — 精确输出格式

Pi `system-prompt.ts` 的完整实现（**注意：3 条固定指令头行是 Pi 规范的一部分**）：

```typescript
// harness/system-prompt.ts — 完整实现
export function formatSkillsForSystemPrompt(skills: Skill[]): string {
  const visibleSkills = skills.filter(s => !s.disableModelInvocation);
  if (visibleSkills.length === 0) return "";

  const lines = [
    // ↓ 3 条固定头行，告知模型"如何使用 skills"
    "The following skills provide specialized instructions for specific tasks.",
    "Read the full skill file when the task matches its description.",
    "When a skill file references a relative path, resolve it against the skill directory " +
      "(parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.",
    "",
    "<available_skills>",
  ];
  for (const skill of visibleSkills) {
    lines.push("  <skill>");
    lines.push(`    <name>${escapeXml(skill.name)}</name>`);
    lines.push(`    <description>${escapeXml(skill.description)}</description>`);
    lines.push(`    <location>${escapeXml(skill.filePath)}</location>`);
    lines.push("  </skill>");
  }
  lines.push("</available_skills>");
  return lines.join("\n");
}
```

**精确输出示例**：

```
The following skills provide specialized instructions for specific tasks.
Read the full skill file when the task matches its description.
When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.

<available_skills>
  <skill>
    <name>investigate</name>
    <description>Debug bugs by following a systematic investigation process</description>
    <location>/Users/ryanliu/.claude/skills/investigate/SKILL.md</location>
  </skill>
  <skill>
    <name>brainstorming</name>
    <description>Turn ideas into fully formed designs through collaborative dialogue</description>
    <location>/Users/ryanliu/.claude/plugins/cache/.../skills/brainstorming/SKILL.md</location>
  </skill>
</available_skills>
```

#### 3.2.3 formatSkillInvocation — 运行时 Skill 内容注入

当 Agent 决定调用某个 skill（读取完整内容）时，Pi 用：

```typescript
// harness/skills.ts:38
export function formatSkillInvocation(skill: Skill, additionalInstructions?: string): string {
  const skillBlock = [
    `<skill name="${skill.name}" location="${skill.filePath}">`,
    `References are relative to ${dirnameEnvPath(skill.filePath)}.`,
    ``,
    skill.content,
    `</skill>`,
  ].join("\n");
  return additionalInstructions ? `${skillBlock}\n\n${additionalInstructions}` : skillBlock;
}
// 产出示例：
// <skill name="investigate" location="/path/to/SKILL.md">
// References are relative to /path/to/.
//
// [skill 的完整 markdown 内容]
// </skill>
```

**用途**：Agent 在会话中 Read 了 SKILL.md 后，将内容以这种 XML 包装注入到用户消息或 tool result 中，让模型在当前 turn 拥有完整 skill 上下文。

#### 3.2.4 loadSkills — 目录遍历 + ignore 文件支持

```typescript
// harness/skills.ts:49
export async function loadSkills(
  env: ExecutionEnv,
  dirs: string | string[],
): Promise<{ skills: Skill[]; diagnostics: SkillDiagnostic[] }>

// 核心规则：
// 1. 遍历每个目录（递归），跳过不存在的目录（静默）
// 2. 在每级目录读取 .gitignore / .ignore / .fdignore，构建 IgnoreMatcher
// 3. 目录根部的 .md 文件（非 SKILL.md）也作为 skill 加载（"root flat files"）
// 4. 子目录中必须有 SKILL.md 才算 skill（非 SKILL.md 的 .md 文件忽略）
// 5. SKILL.md 优先级高于根部同名 .md 文件

// SkillDiagnostic 诊断系统
type SkillDiagnosticCode =
  | "file_info_failed"   // env.fileInfo 调用失败
  | "list_failed"        // env.listDir 调用失败
  | "read_failed"        // 读取文件内容失败
  | "parse_failed"       // YAML frontmatter 解析失败
  | "invalid_metadata";  // name/description 验证失败（但 skill 仍可能加载）

interface SkillDiagnostic { type: "warning"; code: SkillDiagnosticCode; message: string; path: string; }
```

**validateName 规则**（`harness/skills.ts:281`）：
- `name` 必须等于父目录名（`parentDirName`）
- 只允许 `^[a-z0-9-]+$`（小写字母、数字、连字符）
- 不能以 `-` 开头或结尾
- 不能包含 `--`（连续连字符）
- 最大 64 字符

**validateDescription 规则**（`harness/skills.ts:293`）：
- `description` 字段必填且非空
- 最大 1024 字符
- 缺少 description → skill **不加载**（`null` 返回）

#### 3.2.5 loadSourcedSkills — 多来源溯源加载

```typescript
// harness/skills.ts:83
export async function loadSourcedSkills<TSource, TSkill extends Skill = Skill>(
  env: ExecutionEnv,
  inputs: Array<{ path: string; source: TSource }>,    // source 是应用定义的溯源标签
  mapSkill?: (skill: Skill, source: TSource) => TSkill,
): Promise<{
  skills: Array<{ skill: TSkill; source: TSource }>;
  diagnostics: Array<SkillDiagnostic & { source: TSource }>;
}>
// 使用场景：
//   loadSourcedSkills(env, [
//     { path: "~/.claude/skills/", source: "user-global" },
//     { path: ".claude/skills/",   source: "project-local" },
//     { path: "~/.claude/plugins/cache/.../skills/", source: "plugin:superpowers:5.1.0" },
//   ])
// → 每个 skill 携带来源标签，UClaw UI 可以显示 "来自插件 superpowers"
```

#### 3.2.6 parseFrontmatter — YAML 解析

```typescript
// harness/skills.ts:303
function parseFrontmatter<T>(content: string): Result<{ frontmatter: T; body: string }, Error> {
  const normalized = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  if (!normalized.startsWith("---")) {
    return { ok: true, value: { frontmatter: {} as T, body: normalized } }; // 无 frontmatter，graceful
  }
  const endIndex = normalized.indexOf("\n---", 3);
  // ... 提取 YAML 块，用 yaml.parse() 解析
}
// 支持 frontmatter 字段：
//   name: my-skill           （可选，默认用父目录名）
//   description: "..."       （必填，否则 skill 不加载）
//   disable-model-invocation: true  （true 时不出现在 available_skills XML 中）
```

#### 3.2.7 parseCommandArgs + substituteArgs — PromptTemplate 参数系统

```typescript
// harness/prompt-templates.ts:223
export function parseCommandArgs(argsString: string): string[] {
  // Shell 风格 tokenizer：支持单引号/双引号包裹，引号内保留空白
  // 例：parseCommandArgs('foo "bar baz" \'qux quux\'') → ["foo", "bar baz", "qux quux"]
}

// harness/prompt-templates.ts:249
export function substituteArgs(content: string, args: string[]): string {
  // 5 种占位符（按优先级顺序处理）：
  // 1. $N         → args[N-1]                 ($1, $2, ...)
  // 2. ${@:N:L}  → args.slice(N-1, N-1+L).join(" ")  (从第N个取L个)
  // 3. ${@:N}    → args.slice(N-1).join(" ")  (从第N个到末尾)
  // 4. $ARGUMENTS → args.join(" ")             (全部参数)
  // 5. $@         → args.join(" ")             (全部参数，同 $ARGUMENTS)
}
```

---

### 3.3 Rust 完整实现设计

#### 3.3.1 数据类型

```rust
// agent/skills.rs

/// 诊断码，与 Pi 完全对齐
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillDiagnosticCode {
    FileInfoFailed,    // stat 调用失败
    ListFailed,        // readdir 失败
    ReadFailed,        // 读取文件内容失败
    ParseFailed,       // YAML frontmatter 解析失败
    InvalidMetadata,   // name/description 验证失败（不阻止加载，仅发出 warning）
}

/// 加载 skill 时产生的诊断信息
#[derive(Debug, Clone)]
pub struct SkillDiagnostic {
    pub severity: DiagnosticSeverity,  // 目前只有 Warning
    pub code: SkillDiagnosticCode,
    pub message: String,
    pub path: PathBuf,
}

/// 已加载的 Skill
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: PathBuf,
    pub disable_model_invocation: bool,
}

/// YAML frontmatter 结构（serde_yaml 反序列化）
#[derive(Debug, Default, Deserialize)]
pub struct SkillFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "disable-model-invocation", default)]
    pub disable_model_invocation: bool,
}

/// 带溯源标签的 Skill（用于多来源加载）
#[derive(Debug, Clone)]
pub struct ProvenancedSkill<S> {
    pub skill: Skill,
    pub source: S,
}
```

#### 3.3.2 validateName + validateDescription

```rust
// 与 Pi 完全对应的验证规则

fn validate_name(name: &str, parent_dir_name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(format!(r#"name "{name}" does not match parent directory "{parent_dir_name}""#));
    }
    if name.len() > 64 {
        errors.push(format!("name exceeds 64 characters ({})", name.len()));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)".into());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".into());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".into());
    }
    errors
}

fn validate_description(description: Option<&str>) -> Vec<String> {
    match description {
        None | Some("") => vec!["description is required".into()],
        Some(d) if d.trim().is_empty() => vec!["description is required".into()],
        Some(d) if d.len() > 1024 => vec![format!("description exceeds 1024 characters ({})", d.len())],
        _ => vec![],
    }
}
```

#### 3.3.3 parseFrontmatter

```rust
use serde_yaml;

struct ParsedFrontmatter<T> {
    frontmatter: T,
    body: String,
}

fn parse_frontmatter<T: for<'de> Deserialize<'de> + Default>(
    content: &str,
) -> Result<ParsedFrontmatter<T>, String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return Ok(ParsedFrontmatter { frontmatter: T::default(), body: normalized });
    }
    let end = normalized[3..].find("\n---")
        .ok_or_else(|| "unclosed frontmatter block".to_string())?;
    let yaml_block = &normalized[3..end + 3];  // 3 = "---".len()
    let body_start = end + 3 + 4;              // 4 = "\n---".len()
    let body = normalized[body_start..].trim_start_matches('\n').to_string();
    let frontmatter: T = serde_yaml::from_str(yaml_block)
        .map_err(|e| e.to_string())?;
    Ok(ParsedFrontmatter { frontmatter, body })
}
```

#### 3.3.4 SkillLoader — 递归遍历 + ignore 文件支持

```rust
use ignore::gitignore::{Gitignore, GitignoreBuilder};

const IGNORE_FILE_NAMES: &[&str] = &[".gitignore", ".ignore", ".fdignore"];

pub struct SkillLoader;

impl SkillLoader {
    pub async fn load_skills(dirs: &[PathBuf]) -> (Vec<Skill>, Vec<SkillDiagnostic>) {
        let mut skills = Vec::new();
        let mut diagnostics = Vec::new();
        for dir in dirs {
            if !dir.is_dir() { continue; }  // 不存在的目录静默跳过
            let mut builder = GitignoreBuilder::new(dir);
            let result = Self::load_from_dir(dir, true, &mut builder, dir).await;
            skills.extend(result.0);
            diagnostics.extend(result.1);
        }
        (skills, diagnostics)
    }

    async fn load_from_dir(
        dir: &Path,
        include_root_files: bool,  // 只有顶层目录才加载根部 .md 文件
        builder: &mut GitignoreBuilder,
        root: &Path,
    ) -> (Vec<Skill>, Vec<SkillDiagnostic>) {
        let mut skills = Vec::new();
        let mut diagnostics = Vec::new();

        // 先收集 ignore 规则
        for ignore_name in IGNORE_FILE_NAMES {
            let ignore_path = dir.join(ignore_name);
            if ignore_path.is_file() {
                let _ = builder.add(&ignore_path); // ignore crate 处理格式兼容
            }
        }
        let gitignore = builder.build().unwrap_or_default();

        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                diagnostics.push(SkillDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: SkillDiagnosticCode::ListFailed,
                    message: e.to_string(),
                    path: dir.to_path_buf(),
                });
                return (skills, diagnostics);
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            if gitignore.matched(rel, path.is_dir()).is_ignore() { continue; }

            if path.is_dir() {
                // 子目录：只处理含 SKILL.md 的目录
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    let (s, d) = Self::load_skill_file(&skill_md).await;
                    skills.extend(s);
                    diagnostics.extend(d);
                }
                // 递归子目录（但 include_root_files=false）
                let mut sub_builder = GitignoreBuilder::new(&path);
                let (sub_s, sub_d) = Box::pin(Self::load_from_dir(
                    &path, false, &mut sub_builder, root
                )).await;
                skills.extend(sub_s);
                diagnostics.extend(sub_d);
            } else if include_root_files {
                // 根部 .md 文件（非 SKILL.md）也作为 flat skill 加载
                if let Some(ext) = path.extension() {
                    if ext == "md" && path.file_name() != Some("SKILL.md".as_ref()) {
                        let (s, d) = Self::load_skill_file(&path).await;
                        skills.extend(s);
                        diagnostics.extend(d);
                    }
                }
            }
        }
        (skills, diagnostics)
    }

    async fn load_skill_file(path: &Path) -> (Vec<Skill>, Vec<SkillDiagnostic>) {
        let mut diagnostics = Vec::new();
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                diagnostics.push(SkillDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: SkillDiagnosticCode::ReadFailed,
                    message: e.to_string(),
                    path: path.to_path_buf(),
                });
                return (vec![], diagnostics);
            }
        };

        let parsed = match parse_frontmatter::<SkillFrontmatter>(&content) {
            Ok(p) => p,
            Err(e) => {
                diagnostics.push(SkillDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: SkillDiagnosticCode::ParseFailed,
                    message: e,
                    path: path.to_path_buf(),
                });
                return (vec![], diagnostics);
            }
        };

        let parent_dir = path.parent().unwrap_or(path);
        let parent_name = parent_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // description 必须存在，否则不加载
        let description = match parsed.frontmatter.description {
            Some(d) if !d.trim().is_empty() => d,
            _ => {
                for e in validate_description(None) {
                    diagnostics.push(SkillDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        code: SkillDiagnosticCode::InvalidMetadata,
                        message: e, path: path.to_path_buf(),
                    });
                }
                return (vec![], diagnostics);
            }
        };
        for e in validate_description(Some(&description)) {
            diagnostics.push(SkillDiagnostic {
                severity: DiagnosticSeverity::Warning,
                code: SkillDiagnosticCode::InvalidMetadata,
                message: e, path: path.to_path_buf(),
            });
        }

        let name = parsed.frontmatter.name.unwrap_or_else(|| parent_name.to_string());
        for e in validate_name(&name, parent_name) {
            diagnostics.push(SkillDiagnostic {
                severity: DiagnosticSeverity::Warning,
                code: SkillDiagnosticCode::InvalidMetadata,
                message: e, path: path.to_path_buf(),
            });
        }

        let skill = Skill {
            name,
            description,
            content: parsed.body,
            file_path: path.to_path_buf(),
            disable_model_invocation: parsed.frontmatter.disable_model_invocation,
        };
        (vec![skill], diagnostics)
    }
}
```

#### 3.3.5 loadSourcedSkills

```rust
pub async fn load_sourced_skills<S: Clone>(
    dirs: &[(&PathBuf, S)],  // (path, source_tag) 对
) -> (Vec<ProvenancedSkill<S>>, Vec<(SkillDiagnostic, S)>) {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    for (dir, source) in dirs {
        let (s, d) = SkillLoader::load_skills(&[(*dir).clone()]).await;
        for skill in s {
            skills.push(ProvenancedSkill { skill, source: source.clone() });
        }
        for diag in d {
            diagnostics.push((diag, source.clone()));
        }
    }
    (skills, diagnostics)
}

// UClaw 使用示例：
//   let sources = vec![
//     (&user_skills_dir,    SkillSource::UserGlobal),
//     (&project_skills_dir, SkillSource::ProjectLocal),
//     (&plugin_skills_dir,  SkillSource::Plugin { name: "superpowers".into(), version: "5.1.0".into() }),
//   ];
//   let (skills, _diags) = load_sourced_skills(&sources).await;
```

#### 3.3.6 format_skills_for_system_prompt — 精确还原 Pi 3 头行格式

```rust
// 3 条头行是 Pi 规范的一部分，模型需要它们来理解"如何使用 skills"
pub fn format_skills_for_system_prompt(skills: &[Skill]) -> String {
    let visible: Vec<&Skill> = skills.iter()
        .filter(|s| !s.disable_model_invocation)
        .collect();
    if visible.is_empty() { return String::new(); }

    let mut lines = vec![
        "The following skills provide specialized instructions for specific tasks.",
        "Read the full skill file when the task matches its description.",
        "When a skill file references a relative path, resolve it against the skill directory \
         (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.",
        "",
        "<available_skills>",
    ];

    // 每个 skill 四行缩进 XML
    let skill_lines: Vec<String> = visible.iter().flat_map(|s| {
        vec![
            "  <skill>".to_string(),
            format!("    <name>{}</name>", escape_xml(&s.name)),
            format!("    <description>{}</description>", escape_xml(&s.description)),
            format!("    <location>{}</location>", escape_xml(s.file_path.to_str().unwrap_or(""))),
            "  </skill>".to_string(),
        ]
    }).collect();
    let mut result = lines.join("\n");
    if !skill_lines.is_empty() {
        result.push('\n');
        result.push_str(&skill_lines.join("\n"));
    }
    result.push_str("\n</available_skills>");
    result
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&apos;")
}
```

#### 3.3.7 format_skill_invocation — 运行时内容注入

```rust
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String {
    let dir = skill.file_path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let skill_block = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        escape_xml(&skill.name),
        escape_xml(skill.file_path.to_str().unwrap_or("")),
        dir,
        skill.content,
    );
    match additional_instructions {
        Some(instr) => format!("{skill_block}\n\n{instr}"),
        None => skill_block,
    }
}
```

#### 3.3.8 parseCommandArgs — Shell 风格 Tokenizer

```rust
/// 精确移植 Pi 的 shell 风格参数解析：支持单引号/双引号
pub fn parse_command_args(args_string: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in args_string.chars() {
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch == ' ' || ch == '\t' {
            if !current.is_empty() {
                args.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() { args.push(current); }
    args
}
// parse_command_args(r#"foo "bar baz" 'qux quux'"#) → ["foo", "bar baz", "qux quux"]
```

#### 3.3.9 substituteArgs — 5 种占位符（精确还原 Pi 顺序）

```rust
use regex::Regex;
use std::sync::OnceLock;

/// 精确还原 Pi 的 5 种占位符（处理顺序：$N → ${@:N:L} → ${@:N} → $ARGUMENTS → $@）
pub fn substitute_args(content: &str, args: &[&str]) -> String {
    static RE_POSITIONAL: OnceLock<Regex> = OnceLock::new();
    static RE_SLICE_LEN:  OnceLock<Regex> = OnceLock::new();
    static RE_SLICE:      OnceLock<Regex> = OnceLock::new();

    let re_pos = RE_POSITIONAL.get_or_init(|| Regex::new(r"\$(\d+)").unwrap());
    let re_sl  = RE_SLICE_LEN.get_or_init(|| Regex::new(r"\$\{@:(\d+):(\d+)\}").unwrap());
    let re_s   = RE_SLICE.get_or_init(|| Regex::new(r"\$\{@:(\d+)\}").unwrap());

    // 1. $N → args[N-1]（空字符串 if 越界）
    let mut result = re_pos.replace_all(content, |caps: &regex::Captures| {
        let n: usize = caps[1].parse().unwrap_or(0);
        args.get(n.saturating_sub(1)).copied().unwrap_or("").to_string()
    }).to_string();

    // 2. ${@:N:L} → args.slice(N-1, N-1+L).join(" ")
    result = re_sl.replace_all(&result, |caps: &regex::Captures| {
        let start: usize = caps[1].parse::<usize>().unwrap_or(1).saturating_sub(1);
        let len:   usize = caps[2].parse().unwrap_or(0);
        args[start..args.len().min(start + len)].join(" ")
    }).to_string();

    // 3. ${@:N} → args.slice(N-1).join(" ")
    result = re_s.replace_all(&result, |caps: &regex::Captures| {
        let start: usize = caps[1].parse::<usize>().unwrap_or(1).saturating_sub(1);
        args[start..].join(" ")
    }).to_string();

    // 4. $ARGUMENTS → all args joined
    let all_args = args.join(" ");
    result = result.replace("$ARGUMENTS", &all_args);

    // 5. $@ → all args joined
    result.replace("$@", &all_args)
}
```

#### 3.3.10 DynamicSystemPromptFn + TurnStateBuilder（Pi createTurnState 对应）

```rust
// Pi 的 systemPrompt: string | async fn 模式在 Rust 中用枚举表达

pub struct TurnStatePromptContext<'a> {
    pub model: &'a str,
    pub thinking_level: ThinkingLevel,
    pub active_tools: &'a [String],
    pub session_turn: u32,
    pub session_id: &'a str,
}

pub enum SystemPromptSource {
    /// 静态字符串（等效于 Pi 的 string 分支）
    Static(String),
    /// 动态函数（等效于 Pi 的 async fn 分支），每 turn 开始时求值一次
    Dynamic(Arc<dyn Fn(&TurnStatePromptContext<'_>) -> BoxFuture<'_, String> + Send + Sync>),
}

impl SystemPromptSource {
    /// 对应 Pi createTurnState() 中的 systemPrompt 求值逻辑
    pub async fn resolve(&self, ctx: &TurnStatePromptContext<'_>) -> String {
        match self {
            Self::Static(s) => s.clone(),
            Self::Dynamic(f) => f(ctx).await,
        }
    }
}

/// TurnState 快照（调用 resolve() 后的结果冻结在此）
pub struct TurnState {
    pub system_prompt: String,    // 已渲染完毕，本 turn 期间不变
    pub model: String,
    pub active_tools: Vec<String>,
    pub session_id: String,
    // ... 其他字段
}

impl TurnState {
    pub async fn build(source: &SystemPromptSource, ctx: &TurnStatePromptContext<'_>) -> Self {
        TurnState {
            system_prompt: source.resolve(ctx).await,
            model: ctx.model.to_string(),
            active_tools: ctx.active_tools.to_vec(),
            session_id: ctx.session_id.to_string(),
        }
    }
}
```

#### 3.3.11 UClaw 集成：SkillsRegistry 桥接 + B2 缓存冻结

```rust
// agent/prompt_builder.rs

/// 会话开始时创建的 Skills 快照（整个会话不变 → B2 缓存稳定）
pub struct SessionSkillsSnapshot {
    pub skills: Arc<Vec<Skill>>,
    pub formatted_block: String,  // format_skills_for_system_prompt 的结果
}

impl SessionSkillsSnapshot {
    /// 在会话 Session::new() 时调用一次
    pub async fn from_registry(registry: &SkillsRegistry) -> Self {
        let dirs = registry.all_skill_dirs().await;
        let (skills, diagnostics) = SkillLoader::load_skills(&dirs).await;
        if !diagnostics.is_empty() {
            tracing::warn!("skill load warnings: {:?}", diagnostics);
        }
        let formatted_block = format_skills_for_system_prompt(&skills);
        Self { skills: Arc::new(skills), formatted_block }
    }
}

/// 将 Skills 快照接入现有 SystemPromptProvider trait
pub struct SkillsInjector {
    snapshot: Arc<SessionSkillsSnapshot>,
}

#[async_trait]
impl SystemPromptProvider for SkillsInjector {
    async fn stable_content(&self, _ctx: &PromptContext) -> String {
        self.snapshot.formatted_block.clone()  // 完全稳定，缓存友好
    }
}

// compose_system_prompt 中的集成：
// let skills_block = skills_injector.stable_content(&ctx).await;
// let stable = format!("{base_content}\n\n{skills_block}");
// let dynamic = format!("{time_block}\n{memory_block}\n{fragments_block}");
// full_prompt = format!("{stable}\n\n{dynamic}");
```

**B2 缓存保证**：
- `SessionSkillsSnapshot` 在 `Session::new()` 时创建一次，整个会话期间 `Arc` 共享
- `SkillsInjector::stable_content()` 永远返回同一个 `String`（无 dynamic 求值）
- 时间戳 / fragments / memory 只进入 `dynamic_content()`，不影响缓存层

### 3.4 ROI 评估

| 改进项 | 优先级 | 开发成本 | 用户价值 |
|--------|--------|---------|---------|
| Skills XML 注入到系统prompt（含 3 头行）| 🔴 最高 | 1.5天 | Agent 主动选择正确 skill；office 用户核心场景 |
| SkillLoader + ignore 文件支持 | 🔴 最高 | 1天 | 精准加载用户/项目/插件 skills，不污染系统 |
| loadSourcedSkills 溯源标签 | 🟡 中 | 0.5天 | UI 显示"来自插件 superpowers"溯源信息 |
| DynamicSystemPromptFn（fn 分支）| 🟡 中 | 1天 | 为 Claude Haiku 定制更简洁 prompt |
| PromptTemplate + substituteArgs | 🟡 中 | 2天 | 用户自定义工作流 slash 命令 |
| parseCommandArgs shell tokenizer | 🟢 低 | 0.5天 | 与 substitute_args 配套，支持带引号参数 |

**缓存注意**：Skills 注入必须确保内容稳定（session 开始时冻结），否则破坏 B2 缓存，月成本可能增加 30%+。

---

## Section 4 — 历史管理与上下文压缩

### 4.1 现状深度分析

UClaw 的压缩是**逻辑标记**策略：

```rust
// Soft compress：标记 compacted=true，消息仍在内存
// 好处：可replay；坏处：无语义摘要，LLM依然"看不见"被压缩内容
pub fn soft_compress_context(messages: &mut Vec<ChatMessage>, keep_turns: usize)

// Hard truncate：逐个标记最老消息直到低于75%预算
pub fn hard_truncate_context(messages: &mut Vec<ChatMessage>, budget: usize)
// 然后调用 purge_orphaned_tool_results 修复孤立ToolResult
```

**核心问题**：
- 压缩后早期上下文完全丢失（无语义摘要）
- 长coding session（数小时）中，早期的"用户需求""项目背景"会被截断，Agent开始"失忆"
- 无分支历史：不支持"回到某个决策点重新探索"

### 4.2 Pi 的 LLM 摘要 + 分支摘要

```typescript
// LLM 生成语义摘要
export async function compact(
  entries: SessionTreeEntry[],
  session: Session,
  model: Model<any>,
  settings: CompactionSettings,
  options: CompactOptions,
): Promise<CompactionResult>

// 流程：
// 1. prepareCompaction：找切点（turn边界），分离"压缩"和"保留"消息
// 2. extractFileOperations：提取被压缩消息中的文件读写操作
// 3. generateSummary：调用LLM生成语义摘要（serializeConversation → 纯文本 → LLM）
// 4. 返回 CompactionResult { summary, firstKeptEntryId, tokensBefore, details }

// 分支摘要（切换分支时）
export async function collectEntriesForBranchSummary(
  session, oldLeafId, targetId
): Promise<CollectEntriesResult>
// 提取被放弃的分支中修改了哪些文件，生成摘要供context重建
```

**FileOps 提取**（关键设计）：
```typescript
// 从被压缩消息中提取文件操作足迹
function extractFileOpsFromMessage(message: AgentMessage, fileOps: FileOperations) {
  // 遍历 toolCall 内容块
  // - "read" → fileOps.read.add(path)
  // - "write" → fileOps.written.add(path)
  // - "edit" → fileOps.edited.add(path)
}
// 压缩摘要中包含"这段历史读/写了这些文件"，帮助Agent重建上下文
```

### 4.3 Brainstorming：Rust 中的 LLM 语义摘要压缩

#### 方案 A：LLM 摘要替换（推荐）

```rust
pub struct CompactionResult {
    pub summary: String,             // LLM生成的语义摘要
    pub first_kept_entry_id: u64,    // 从这条消息开始保留
    pub tokens_before: u32,          // 压缩前token数
    pub file_ops: CompactionFileOps, // 被压缩消息中的文件操作
}

pub struct CompactionFileOps {
    pub read_files: Vec<String>,
    pub modified_files: Vec<String>,
}

pub async fn generate_compaction_summary(
    messages_to_compact: &[ChatMessage],
    llm_client: &dyn LlmClient,
    model: &str,
) -> Result<String, CompactionError> {
    // 将消息序列化为纯文本对话格式
    let conversation = serialize_conversation(messages_to_compact);
    
    // 调用 LLM 生成摘要（用较便宜的模型，如 Haiku）
    let prompt = format!(
        "请总结以下对话历史的关键信息。重点保留：用户的核心需求、重要决策、关键代码变更、已完成和未完成的工作项。\n\n{conversation}"
    );
    
    llm_client.complete(model, &prompt, CompletionOptions {
        max_tokens: 2000,
        ..Default::default()
    }).await
}

pub async fn compress_context_with_llm(
    reason_ctx: &mut ReasoningContext,
    config: &CompactionConfig,
    llm_client: &dyn LlmClient,
) -> Option<CompactionResult> {
    if !should_compact(&reason_ctx.messages, config) {
        return None;
    }
    
    let cut_point = find_compaction_cut_point(&reason_ctx.messages, config.keep_recent_turns);
    let (to_compact, to_keep) = reason_ctx.messages.split_at(cut_point);
    
    // 提取文件操作
    let file_ops = extract_file_ops(to_compact);
    
    // 生成摘要
    let summary = generate_compaction_summary(to_compact, llm_client, &config.summary_model).await?;
    
    // 替换被压缩消息为摘要消息
    let mut new_messages = vec![ChatMessage::system_summary(&summary, &file_ops)];
    new_messages.extend_from_slice(to_keep);
    reason_ctx.messages = new_messages;
    
    Some(CompactionResult { summary, file_ops, tokens_before: reason_ctx.total_input_tokens })
}
```

#### 方案 B：渐进改进（更安全）

1. 先保留现有逻辑标记策略
2. 压缩时额外插入一条 `SystemSummaryMessage`（用预置模板而非LLM生成，成本为零）
3. 下一步再升级到 LLM 生成摘要

```rust
// Phase 1：模板摘要（免费，立即改善）
fn generate_template_summary(messages: &[ChatMessage]) -> String {
    let tool_calls: Vec<_> = extract_tool_calls(messages);
    let file_writes: Vec<_> = extract_file_writes(messages);
    format!(
        "[Context compacted - {} turns, {} tool calls, files modified: {}]",
        count_turns(messages),
        tool_calls.len(),
        file_writes.join(", ")
    )
}
```

#### FileOps 追踪（独立价值）

```rust
/// 每个会话维护的文件操作记录
pub struct SessionFileOps {
    pub read: HashSet<PathBuf>,
    pub written: HashSet<PathBuf>,
    pub edited: HashSet<PathBuf>,
}

impl SessionFileOps {
    pub fn track_tool_result(&mut self, tool_name: &str, args: &Value, result: &AgentToolResult) {
        match tool_name {
            "read_file" => {
                if let Some(path) = args.get("path").and_then(|p| p.as_str()) {
                    self.read.insert(PathBuf::from(path));
                }
            }
            "write_file" => { /* ... */ }
            "edit" | "apply_patch" => { /* ... */ }
            _ => {}
        }
        // 同步更新 result.file_ops
    }
}
```

### 4.4 ROI 评估

| 改进项 | 用户场景 | 开发成本 | 价值 |
|--------|---------|---------|------|
| FileOps 追踪 | "Agent刚才改了哪些文件" | 1天 | 中高 |
| 模板摘要（Phase 1）| 压缩后不再完全失忆 | 1天 | 高（零成本）|
| LLM 语义摘要 | 数小时coding session不失忆 | 2天 | 极高 |
| 分支历史 | 探索性编程"回到决策点" | 5天+ | 极高（需会话持久化支持）|

**推荐路径**：先做 FileOps 追踪 + 模板摘要（2天），下一个 sprint 做 LLM 摘要（2天），分支历史与会话持久化改造捆绑做。

---

## Section 5 — Hooks 系统与可观察性

### 5.1 现状深度分析

UClaw 的可观察性依赖 Tauri 事件系统（**单向**）：

```rust
// 只能向UI推送，应用层代码无法"钩入"并修改行为
emit("agent:heartbeat", { conversationId, iteration, stage, lastActivityMsAgo })
emit("agent:stalled", { stage, stalledForMs })
emit("agent:tool-activity", { toolName, toolCallId, status, preview })
```

**LoopDelegate trait** 提供了有限的扩展点：
- `before_llm_call`：可返回 `Option<LoopOutcome>` 提前终止
- `handle_text_response`：返回 `TextAction` 控制循环
- `on_usage`：仅观察，无副作用

**缺口**：
- 无 `before_provider_request`：无法在发送给 Claude API 之前修改 payload（如添加 metadata、调整参数）
- 无 `after_tool_call`：无法后处理工具结果（如过滤敏感信息、添加日志）
- 无 `observe()` vs `on()`：所有扩展点都是参与型（有副作用），无法做纯观察

### 5.2 Pi 的 Phantom Type 安全 Hook 系统

```typescript
// Phantom Type：在类型级别绑定事件类型与其结果类型
declare const HookResult: unique symbol;
interface HookEvent<TType extends string, TResult = void> {
  type: TType;
  readonly [HookResult]?: TResult;
}

// before_provider_request 事件：可返回 StreamOptionsPatch（修改请求参数）
interface BeforeProviderRequestEvent extends HookEvent<"before_provider_request", {
  streamOptions?: AgentHarnessStreamOptionsPatch;
}> {
  model: Model<any>;
  streamOptions: AgentHarnessStreamOptions;
}

// Hook 注册
interface AgentHarnessHooks {
  // 纯观察（无副作用，不影响执行）
  observe(handler: (event: E, signal: AbortSignal) => void): () => void;
  
  // 参与执行（可返回结果修改行为）
  on<TType>(type: TType, handler: (event) => ResultOf<E> | void): () => void;
  
  // 发射事件
  emit<TEvent>(event: TEvent, signal?: AbortSignal): Promise<ResultOf<TEvent>>;
}
```

### 5.3 Brainstorming：Rust 中的双向 Hook 系统

#### 类型安全 Hook 设计

Rust 的 enum + associated type 天然适合实现 Phantom Type 等价物：

```rust
/// Hook 事件枚举（每个 variant 携带数据 + 定义结果类型）
pub enum AgentHookEvent {
    BeforeProviderRequest(BeforeProviderRequestData),
    AfterToolCall(AfterToolCallData),
    TurnEnd(TurnEndData),
    ContextPrepared(ContextPreparedData),
}

/// 对应每个事件的结果类型
pub enum AgentHookResult {
    BeforeProviderRequest(Option<StreamOptionsPatch>),
    AfterToolCall(Option<Vec<ContentBlock>>),  // 可修改工具结果
    TurnEnd(()),
    ContextPrepared(()),
}

/// Stream options 补丁（可修改的字段）
pub struct StreamOptionsPatch {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub additional_headers: Option<HashMap<String, String>>,
    pub metadata: Option<serde_json::Value>,
}

/// Hook handler：纯观察
pub type ObserveHandler = Box<dyn Fn(&AgentHookEvent) + Send + Sync>;

/// Hook handler：参与型，可修改行为
pub type OnHandler = Box<dyn Fn(&AgentHookEvent) -> Option<AgentHookResult> + Send + Sync>;
/// 异步版本
pub type AsyncOnHandler = Box<dyn for<'a> Fn(&'a AgentHookEvent, CancellationToken) 
    -> BoxFuture<'a, Option<AgentHookResult>> + Send + Sync>;

/// Hook 注册表
pub struct HookRegistry {
    observers: Vec<ObserveHandler>,
    handlers: HashMap<AgentHookEventType, Vec<AsyncOnHandler>>,
}

impl HookRegistry {
    /// 纯观察（日志、指标、UI事件）
    pub fn observe(&mut self, handler: impl Fn(&AgentHookEvent) + Send + Sync + 'static) -> HookHandle {
        let id = HookHandle::new();
        self.observers.push((id, Box::new(handler)));
        id
    }
    
    /// 参与型（可修改请求/结果）
    pub fn on<F, Fut>(&mut self, event_type: AgentHookEventType, handler: F) -> HookHandle
    where
        F: Fn(&AgentHookEvent, CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<AgentHookResult>> + Send + 'static,
    {
        // ...
    }
    
    /// 发射事件，收集所有 handler 的结果并合并
    pub async fn emit(&self, event: &AgentHookEvent, signal: CancellationToken) -> Option<AgentHookResult> {
        // 1. 通知所有 observers
        for obs in &self.observers {
            obs(event);
        }
        // 2. 调用 handlers，取第一个非 None 结果（或合并所有patch）
        if let Some(handlers) = self.handlers.get(&event.event_type()) {
            for handler in handlers {
                if let Some(result) = handler(event, signal.clone()).await {
                    return Some(result);
                }
            }
        }
        None
    }
    
    /// 返回 HookHandle 可用于取消注册
    pub fn unregister(&mut self, handle: HookHandle) { /* ... */ }
}
```

#### 核心 Hook 事件定义

```rust
// 关键hook点（按loop阶段顺序）

// 1. LLM 调用前（可修改请求参数）
BeforeProviderRequest {
    model: String,
    messages: Vec<ChatMessage>,  // 只读
    stream_options: StreamOptions,
}
// 返回：Option<StreamOptionsPatch>

// 2. 工具调用结束后（可修改结果）
AfterToolCall {
    tool_name: String,
    tool_call_id: String,
    args: serde_json::Value,
    result: Vec<ContentBlock>,
}
// 返回：Option<Vec<ContentBlock>>（替换原始结果）

// 3. Turn 结束（纯观察，用于日志/成本追踪）
TurnEnd {
    iteration: u32,
    input_tokens: u32,
    output_tokens: u32,
    tool_calls: Vec<String>,
}
// 返回：void

// 4. Context 准备完毕（可注入额外系统提示）
ContextPrepared {
    system_prompt: String,
    message_count: usize,
    estimated_tokens: u32,
}
// 返回：Option<String>（追加到系统prompt）
```

#### 应用场景（face to 目标用户）

| 场景 | Hook | Office用户 | Coding用户 |
|------|------|-----------|-----------|
| 企业合规审计 | observe(AfterToolCall) 记录所有bash命令 | ✅ | ✅ |
| 成本限制 | observe(TurnEnd) 累计成本，达阈值停止 | ✅ | ✅ |
| 动态模型降级 | on(BeforeProviderRequest) 低优先级任务降到Haiku | | ✅ |
| 工具结果过滤 | on(AfterToolCall) 过滤输出中的敏感路径 | ✅ | |
| 自定义系统提示 | on(ContextPrepared) 注入团队规范 | ✅ | ✅ |

### 5.4 ROI 评估

**开发成本**：Hook trait 定义 + HookRegistry 约 3 天；集成到 agentic_loop 约 1 天。

**价值**：解锁整个扩展生态。现有的 heartbeat、stall 检测可以迁移到 observe() hook，Tauri UI 事件降格为 observer，核心逻辑更纯粹。

---

## Section 6 — 会话持久化

### 6.1 现状深度分析

UClaw 使用 SQLite 三表结构：

```sql
-- messages：Chat模式消息
messages (id, session_id, role, content, created_at, ...)

-- agent_messages：Agent模式可见对话（实际有行）
agent_messages (id, conversation_id, role, content, reasoning, created_at, ...)

-- agent_turns：per-tool-call细粒度分解（实际有行）
agent_turns (id, conversation_id, turn_type, tool_name, tool_input, tool_output, created_at, ...)
```

**问题**：
- 线性历史，无分支支持
- agent_turns 细粒度但从历史重建会话需要复杂的 SQL JOIN
- 无 `compaction` 记录类型（压缩事件无处持久化）
- 无 `label`/`checkpoint` 概念

### 6.2 Pi 的 JSONL 树形持久化

```
版本3格式（逐行JSON，追加写）：

行1（头）: {"type":"session","version":3,"id":"uuid","cwd":"/path","timestamp":"..."}

后续行（各类条目）:
{"type":"message","id":"m1","parentId":null,"message":{...},"timestamp":"..."}
{"type":"message","id":"m2","parentId":"m1","message":{...}}
{"type":"leaf","id":"l1","targetId":"m5"}          ← 当前分支指针
{"type":"compaction","id":"c1","parentId":"m2","summary":"...","firstKeptEntryId":"m3","tokensBefore":50000}
{"type":"branch_summary","id":"b1","fromId":"m8","summary":"..."}  ← 从分支m8离开时生成
{"type":"label","id":"lb1","targetId":"m5","label":"Checkpoint 1"} ← 用户标记的检查点
{"type":"model_change","id":"mc1","thinkingLevel":"high"}
```

**树形导航**：
- `getPathToRoot(leafId)` → 从叶子回溯到根，构建当前分支消息列表
- `navigateTree(targetId)` → 切换分支（先生成 branch_summary，再更新 leaf 指针）
- 分支切换自动记录"这个分支改了哪些文件"

### 6.3 Brainstorming：UClaw 的持久化升级路径

#### 方案 A：SQLite 扩展（推荐，渐进）

在现有 SQLite 基础上增加树形支持，保持向后兼容：

```sql
-- 新增：会话树表（替代 agent_messages 的线性追加模式）
CREATE TABLE session_tree (
    id TEXT PRIMARY KEY,              -- UUID，每条条目唯一
    session_id TEXT NOT NULL,
    parent_id TEXT,                   -- NULL 表示根
    entry_type TEXT NOT NULL,         -- 'message' | 'leaf' | 'compaction' | 'branch_summary' | 'label'
    data JSONB NOT NULL,              -- 条目内容
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES conversations(id)
);

CREATE INDEX idx_session_tree_session ON session_tree(session_id);
CREATE INDEX idx_session_tree_parent ON session_tree(parent_id);

-- 新增：当前叶子指针（每个会话一条）
CREATE TABLE session_leaves (
    session_id TEXT PRIMARY KEY,
    leaf_id TEXT,  -- 指向 session_tree 中的 'leaf' 条目
    FOREIGN KEY (session_id) REFERENCES conversations(id)
);
```

**Rust 实现**：

```rust
/// 会话树条目
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionTreeEntry {
    Message {
        id: String,
        parent_id: Option<String>,
        message: AgentMessage,
        timestamp: i64,
    },
    Leaf {
        id: String,
        target_id: String,  // 指向某条 Message 的 id
        timestamp: i64,
    },
    Compaction {
        id: String,
        parent_id: Option<String>,
        summary: String,
        first_kept_entry_id: String,
        tokens_before: u32,
        details: serde_json::Value,  // FileOps等
        timestamp: i64,
    },
    BranchSummary {
        id: String,
        from_id: String,  // 从哪个分支离开
        summary: String,
        timestamp: i64,
    },
    Label {
        id: String,
        target_id: String,
        label: String,
        timestamp: i64,
    },
}

/// 会话存储接口（抽象，可换SQLite或JSONL实现）
#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn append_entry(&self, entry: SessionTreeEntry) -> Result<()>;
    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>>;
    async fn get_leaf_id(&self, session_id: &str) -> Result<Option<String>>;
    async fn set_leaf_id(&self, session_id: &str, leaf_id: &str) -> Result<()>;
}
```

**迁移策略**：
1. 新对话写入 `session_tree` 表
2. 旧对话通过 `agent_messages` 读取（只读兼容）
3. V25 migration 添加新表，无破坏性变更

#### 方案 B：JSONL 并行导出

保留 SQLite 作为查询层，同时写 JSONL 文件到 `~/.uclaw/sessions/<id>.jsonl`。JSONL 文件用于跨机器同步、调试、replay。SQLite 用于搜索和关联查询。

**优点**：不改动 DB schema，JSONL 渐进引入  
**缺点**：双写增加复杂度，两种格式可能不同步

### 6.4 ROI 评估

| 改进项 | 开发成本 | 用户价值 |
|--------|---------|---------|
| session_tree 表 + SQLite迁移 | 3天 | 为分支导航奠基 |
| getPathToRoot / setLeafId | 1天 | 树形历史重建 |
| CompactionEntry 持久化 | 1天 | 压缩历史可见、可搜索 |
| Label/Checkpoint API | 1天 | 用户标记重要时间点 |
| UI：分支切换（navigateTree） | 5天 | 探索性编程差异化功能 |

**建议先做DB层（5天），UI层下一个milestone再做。**

---

## Section 7 — 多模型支持

### 7.1 现状深度分析

UClaw 完全依赖 Anthropic Claude：
- `llm_stream.rs` 直接调用 Anthropic API（SSE流）
- 无 provider 抽象层
- 模型参数硬编码（claude-sonnet-4-6、claude-opus-4-7等）

**市场影响**：
- 目标用户中有大量已有 OpenAI API key 的开发者
- 部分企业用户在 Azure OpenAI 上
- Google Gemini 2.5 Pro 在代码理解上有竞争力
- 多模型对比是 power user 的核心需求

### 7.2 Pi 的多提供商统一抽象

```typescript
interface StreamProvider {
  stream(model, context, options): AssistantMessageEventStream;
}

// 已支持的提供商（packages/ai/）：
// openai-responses, openai-completions, openai-codex-responses
// anthropic-messages
// google-generative-ai, google-vertex
// azure-openai-responses
// mistral-conversations
// bedrock-converse-stream
// 等10+种

// 模型元数据
interface Model<TApi> {
  id: string;
  name: string;
  api: TApi;
  contextWindow: number;
  maxTokens: number;
  cost: { input: number; output: number; cacheRead: number; cacheWrite: number; };
}
```

### 7.3 Brainstorming：Rust 中的多模型架构

#### LlmProvider Trait

```rust
/// 统一的 LLM 提供商 trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_id(&self) -> &str;   // "anthropic" | "openai" | "google"
    fn supported_models(&self) -> Vec<ModelSpec>;
    
    async fn stream(
        &self,
        model_id: &str,
        context: &LlmContext,
        options: &StreamOptions,
        signal: CancellationToken,
    ) -> Result<Box<dyn LlmStream>, LlmError>;
}

/// 模型规格
pub struct ModelSpec {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub supports_thinking: bool,
    pub cost: ModelCost,
}

pub struct ModelCost {
    pub input_per_1m: f64,    // USD per 1M tokens
    pub output_per_1m: f64,
    pub cache_read_per_1m: f64,
    pub cache_write_per_1m: f64,
}

/// 提供商注册表
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    model_to_provider: HashMap<String, String>,
}

impl ProviderRegistry {
    pub fn register(&mut self, provider: impl LlmProvider + 'static) {
        let id = provider.provider_id().to_string();
        for model in provider.supported_models() {
            self.model_to_provider.insert(model.id.clone(), id.clone());
        }
        self.providers.insert(id, Box::new(provider));
    }
    
    pub fn get_provider_for_model(&self, model_id: &str) -> Option<&dyn LlmProvider> {
        self.model_to_provider.get(model_id)
            .and_then(|p| self.providers.get(p))
            .map(|p| p.as_ref())
    }
}
```

#### BYOK（Bring Your Own Key）配置

```rust
/// 用户级提供商配置（持久化到设置）
pub struct ProviderConfig {
    pub provider: String,          // "openai" | "google" | "anthropic"
    pub api_key: String,           // 用户的 API key
    pub base_url: Option<String>,  // Azure/代理场景
    pub enabled: bool,
}

/// 设置序列化（存入 uclaw settings）
#[derive(Serialize, Deserialize)]
pub struct ModelSettings {
    pub active_model: String,         // 当前选择的模型
    pub providers: Vec<ProviderConfig>, // 用户配置的提供商
}
```

#### 实施优先级

**Phase 1（1周）**：
- `LlmProvider` trait 定义
- 将现有 `llm_stream.rs` 重构为 `AnthropicProvider: LlmProvider`
- `ProviderRegistry` 基础实现

**Phase 2（1周）**：
- `OpenAiProvider`（兼容 OpenAI Responses API）
- UI：模型选择器（按提供商分组展示可用模型）
- BYOK：设置页面输入 API key

**Phase 3（1周）**：
- `GoogleProvider`（Gemini 2.5 Pro）
- Azure OpenAI 支持（企业场景）
- 成本估算显示（每次对话的预估成本）

#### 工具调用兼容性注意事项

不同提供商的工具调用格式不同：
- Anthropic：`tool_use` / `tool_result` content blocks
- OpenAI：`tool_calls` / `tool_choice` 字段
- Google：`functionCall` / `functionResponse`

**解决方案**：在 `LlmProvider` 层做 protocol translation，内部统一使用 UClaw 的 `ToolCall`/`ToolResult` 类型。

### 7.4 ROI 评估

这是**最高用户获取 ROI 的单项改进**：

| 场景 | 影响 |
|------|------|
| 用户已有 OpenAI Plus，带自己的 key | 直接拉新 |
| 开发者对比 Claude vs GPT-4o 代码质量 | 用户粘性↑↑ |
| 企业用户在 Azure OpenAI 上 | 打开企业市场 |
| Gemini 2.5 Pro 的长上下文能力 | 差异化场景 |

**开发成本**：3周。**用户价值**：极高（直接影响注册转化率）。

---

## Section 8 — Coding Agent 专项

### 8.1 现状深度分析

UClaw 的 coding 能力依赖几个机制：

**code_rescue（主要 workaround）**：
```rust
// 当 LLM 输出 markdown 代码块而非工具调用时，合成 write_file 调用
pub fn extract_write_file_calls(text: &str, workspace_root) -> Vec<ToolCall>
// 仅在完整块 >= 10行时触发
// 解析文件名优先级：前文提及 → plan步骤 → 语言→默认文件名映射
```

**workspace rule_context_builder**：扫描工作区的规则文件并注入 prompt。

**proactive/tool_memory.rs**：LLM 驱动的学习管道，从聊天轮提取知识写入 GBrain。

**主要问题**：
- code_rescue 是架构债：工具调用越规范，越不需要这个 workaround
- bash 工具无流式输出——长时间命令（`cargo build`、`npm test`）用户只能等结果
- 工具注册三处改动，新增 coding 工具（如 `ripgrep`、`fd`）成本高
- 无文件操作追踪（见 Section 4）

### 8.2 Pi 的 Coding Agent 工具栈

```typescript
// 工具三层设计
// Layer 1: ToolDefinition（用于LLM schema注册）
// Layer 2: AgentTool（完整执行，含onUpdate）
// Layer 3: UI包装（渲染工具结果的React组件）

// Bash 工具 - 流式输出设计
interface BashOperations {
  exec: (command, cwd, { onData: (data: Buffer) => void, signal?, timeout?, env? }) 
    => Promise<{ exitCode: number | null }>;
}

// 创建本地bash执行器
createLocalBashOperations({ shellPath?: string }): BashOperations
// - 使用 spawn()，跟踪分离进程
// - stdout/stderr 实时通过 onData 回调
// - 支持 AbortSignal（发 SIGTERM）

// 工具管理器：按需下载二进制
ToolBinaryManager {
  ensureFd(): Promise<string>;   // fast file finder
  ensureRg(): Promise<string>;   // ripgrep
  // 优先使用 PATH，不存在则下载到 ~/.pi/bin/
}
```

### 8.3 Brainstorming：Coding Agent 专项改进

#### 改进 1：Bash 工具流式输出

```rust
// 当前：bash执行完成后一次性返回输出
// 改进：通过 ToolUpdateSender 实时流式输出

impl AgentTool for BashTool {
    async fn execute(
        &self,
        tool_call_id: &str,
        args: Value,
        signal: CancellationToken,
        on_update: Option<ToolUpdateSender>,
    ) -> Result<AgentToolResult, ToolError> {
        let command = args["command"].as_str().unwrap();
        let mut child = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let mut stdout_reader = BufReader::new(child.stdout.take().unwrap());
        let mut output_buffer = String::new();
        let mut line = String::new();
        
        // 实时流式读取
        while stdout_reader.read_line(&mut line).await? > 0 {
            if let Some(ref tx) = on_update {
                let _ = tx.send(ToolUpdate {
                    tool_call_id: tool_call_id.to_string(),
                    content: line.clone(),
                    progress: None,
                });
            }
            output_buffer.push_str(&line);
            line.clear();
            
            if signal.is_cancelled() {
                child.kill().await?;
                break;
            }
        }
        
        let exit_status = child.wait().await?;
        Ok(AgentToolResult {
            content: vec![ContentBlock::Text { text: output_buffer }],
            details: json!({ "exitCode": exit_status.code() }),
            terminate: false,
            file_ops: FileOps::default(),
        })
    }
}
```

**UI效果**：cargo build 实时显示编译进度，npm test 实时显示测试结果，用户不再盯着"执行中..."等待。

#### 改进 2：减少对 code_rescue 的依赖

code_rescue 存在是因为 LLM 有时输出 markdown 代码块而非工具调用。根本解法：

1. **更强的 system prompt 约束**：明确要求"写文件必须调用 write_file 工具，不允许输出代码块"
2. **工具调用 schema 更清晰**：write_file 工具描述中添加 `"MUST use this tool to write files, never output code blocks"`
3. **保留 code_rescue 作为 last resort**，但减少其触发频率

#### 改进 3：Workspace 感知的工具集

```rust
pub struct WorkspaceToolKit {
    registry: ToolRegistry,
}

impl WorkspaceToolKit {
    pub async fn for_workspace(workspace: &Path) -> Self {
        let mut kit = Self::default_tools();
        
        // 检测项目类型，加载对应工具集
        if workspace.join("Cargo.toml").exists() {
            kit.add_cargo_tools();  // cargo build/test/check
        }
        if workspace.join("package.json").exists() {
            kit.add_npm_tools();    // npm install/test/build
        }
        if workspace.join(".git").exists() {
            kit.add_git_tools();    // git status/diff/log/commit
        }
        
        kit
    }
}
```

#### 改进 4：File Operations Tracker（配合 Section 4）

```rust
/// 每个 AgentTool 执行后自动记录文件操作
impl ToolRegistry {
    pub async fn execute_with_tracking(
        &self,
        call: &ToolCall,
        file_ops: &mut SessionFileOps,
        signal: CancellationToken,
        on_update: Option<ToolUpdateSender>,
    ) -> AgentToolResult {
        let result = self.execute(call, signal, on_update).await;
        // 根据工具名和参数自动追踪文件操作
        file_ops.track(&call.name, &call.arguments, &result.file_ops);
        result
    }
}
```

### 8.4 ROI 评估

| 改进项 | 对 Coding 用户的价值 | 成本 |
|--------|---------------------|------|
| Bash 流式输出 | 极高（长命令体验质变）| 2天 |
| 减少 code_rescue 依赖 | 中（减少奇怪行为）| 1天 |
| Workspace 感知工具集 | 高（自动加载正确工具）| 2天 |
| 文件操作追踪 | 高（配合压缩/分支功能）| 1天（依赖Section4基础）|

---

## Section 9 — Pi 事件驱动 + 快照隔离综合评估

### 9.1 核心设计差异总结

| 维度 | UClaw（当前） | Pi（目标方向）|
|------|-------------|-------------|
| **状态隔离** | `ReasoningContext` 单一可变引用，turn间共享 | `TurnSnapshot` 每turn克隆，LLM调用持有快照Arc |
| **配置变更时机** | 即时生效（可能污染进行中调用） | turn边界（prepareNextTurn）原子替换 |
| **应用层控制** | 有限（before_llm_call可提前退出）| 充分（shouldStopAfterTurn + prepareNextTurn）|
| **消息注入** | SoftInterruptQueue（poll式）| steer/followUp 队列（事件式）|
| **事件流** | Tauri emit（单向，UI消费）| EventStream（双向，可组合）|
| **Loop 终止** | 内部状态机（ThreadState）| 应用层回调（shouldStopAfterTurn）|

### 9.2 在 Rust 中实现快照隔离的关键挑战

**挑战 1：所有权与借用**

Pi 的 TypeScript 用对象展开（`{...config}`）轻松创建快照。Rust 需要 `Clone` + `Arc` 的配合：

```rust
// TurnSnapshot 必须 Clone（turn边界创建新快照）
// 同时 LLM 调用持有 Arc<TurnSnapshot>，可以在不同 task 中共享
#[derive(Clone)]
pub struct TurnSnapshot {
    pub model: String,
    pub tools: Arc<Vec<ToolDefinition>>,  // Arc避免深拷贝
    pub system_prompt: Arc<String>,       // Arc避免深拷贝
    pub stream_options: StreamOptions,
}
```

**挑战 2：异步取消与快照生命周期**

```rust
// LLM 调用持有 Arc<TurnSnapshot>，即使外部更新了配置，进行中的调用不受影响
let snapshot = Arc::clone(&self.current_snapshot);  // 增加引用计数
tokio::spawn(async move {
    // snapshot 在这里是独立的引用，外部 current_snapshot 更新不影响此处
    call_llm_with_snapshot(&snapshot, ...).await
});
```

**挑战 3：双层循环与 Rust 的生命周期**

Rust 的 borrow checker 对双层循环（outer: loop + inner: while）比较严格，需要仔细管理 `&mut ReasoningContext` 的借用。解决方案是将内层循环提取为独立函数。

### 9.3 快照隔离的直接收益清单

1. **多模型动态切换**（最高价值）：规划用 Sonnet、执行用 Haiku，turn边界切换，无竞态
2. **deterministic Harness 测试**：给定相同 TurnSnapshot + 相同 messages，LLM 调用完全复现
3. **配置变更安全性**：用户在设置页面改模型，下一个 turn 才生效，不打断当前执行
4. **shouldStopAfterTurn**：细粒度暂停控制（"执行完这一步再停"）
5. **steer 队列**：用户打字时的消息会在当前 turn 结束后才注入，避免破坏 ToolUse/ToolResult 配对
6. **follow-up 消息**：agent 本要停止，但发现有新消息（自动化触发），无缝继续

### 9.4 可行性评分

| 维度 | 评分 | 说明 |
|------|------|------|
| 技术可行性 | 9/10 | Rust 天然支持所有需要的模式 |
| 架构兼容性 | 7/10 | 需要改造 agentic_loop.rs 和 dispatcher.rs，有一定风险 |
| 迁移成本 | 7/10 | 约5-7天完整迁移，有充分测试覆盖后风险可控 |
| 长期维护性 | 9/10 | 快照隔离减少竞态 bug，长期更好维护 |
| 功能解锁 | 10/10 | 解锁多模型、harness测试、探索性编程三个大功能 |

**综合结论**：快照隔离在 Rust 中完全可行，推荐作为 Agent 架构升级的核心主线。它是一个基础改进，其他7个 section 的改进都会从中受益。

---

## 最终改进路线图

### ROI 矩阵

| 改进项 | 用户价值 | 开发成本 | ROI | 推荐顺序 |
|--------|---------|---------|-----|---------|
| Skills XML 注入 | ⭐⭐⭐⭐⭐ | 1.5天 | 极高 | Sprint 1 |
| Bash 流式输出 | ⭐⭐⭐⭐⭐ | 2天 | 极高 | Sprint 1 |
| 工具并行执行 | ⭐⭐⭐⭐ | 2天 | 高 | Sprint 1 |
| LLM 语义摘要压缩 | ⭐⭐⭐⭐⭐ | 2天 | 极高 | Sprint 2 |
| FileOps 追踪 | ⭐⭐⭐⭐ | 1天 | 高 | Sprint 2 |
| TurnSnapshot 快照隔离 | ⭐⭐⭐⭐⭐ | 5天 | 极高（基础） | Sprint 2 |
| Hooks 系统 | ⭐⭐⭐⭐ | 4天 | 高 | Sprint 3 |
| AgentTool Trait 规范化 | ⭐⭐⭐⭐ | 3天 | 高 | Sprint 3 |
| session_tree DB 迁移 | ⭐⭐⭐⭐ | 3天 | 高 | Sprint 3 |
| 多模型支持（OpenAI） | ⭐⭐⭐⭐⭐ | 1周 | 最高（用户获取）| Sprint 4 |
| 多模型支持（Google）| ⭐⭐⭐⭐ | 1周 | 高 | Sprint 5 |
| 分支历史 UI | ⭐⭐⭐⭐ | 5天 | 高 | Sprint 5 |

### 推荐执行序列

```
Sprint 1（2周）— 立即可感知的体验改善
  ✦ Skills XML 注入到系统 prompt（Office 用户核心场景）
  ✦ Bash 工具流式输出（Coding 用户体验质变）
  ✦ 工具并行执行（多文件读取 2x 加速）

Sprint 2（2周）— 长会话可靠性
  ✦ LLM 语义摘要压缩（数小时 session 不失忆）
  ✦ FileOps 追踪（为后续功能奠基）
  ✦ TurnSnapshot 快照隔离（架构基础，解锁后续）

Sprint 3（3周）— 扩展生态基础
  ✦ Hooks 系统（before_provider_request / after_tool_call）
  ✦ AgentTool Trait 规范化（dispatcher.rs 解耦）
  ✦ session_tree DB 迁移（分支功能数据层）

Sprint 4（3周）— 用户增长驱动
  ✦ 多模型支持：LlmProvider trait + AnthropicProvider 重构
  ✦ OpenAI Provider 实现
  ✦ BYOK UI（设置页 API key 管理）

Sprint 5（3周）— 差异化竞争优势
  ✦ Google Gemini Provider
  ✦ 分支历史 UI（navigateTree + 可视化）
  ✦ Workspace 感知工具集（Rust/Node/Python 项目自动识别）
```

### 保留的 UClaw 独特优势（不需要向 Pi 学习）

| 功能 | 保留理由 |
|------|---------|
| Heartbeat + FlightRecorder | 桌面应用崩溃恢复，Pi（无头服务）不需要 |
| 不洁关闭恢复 | 桌面平台独特需求 |
| 防假进展守卫 | 提升 coding 任务可靠性，Pi没有对应设计 |
| B2 缓存优化 | 降低API成本，需要在动态prompt时特别保护 |
| SafetyMode/批准流 | 企业/家长控制场景 |
| Tauri 原生能力 | 文件系统、系统托盘、原生通知 |

---

## Section 10 — 双交互队列 (Interactive Dual-Queue Steering)

### 10.1 问题溯源

UClaw 现有的 `SoftInterruptQueue` 是一个**单队列轮询**设计：

```rust
pub struct SoftInterruptQueue {
    messages: Arc<VecDeque<SoftInterruptMessage>>,
}
pub struct SoftInterruptMessage {
    source: SoftInterruptSource,  // User / System / Automation
    content: String,
    urgent: bool,
}
```

所有中断消息走同一队列，没有语义区分：
- 用户在 Agent 运行时发来的**实时调整**（"先不要写文件，先看看测试"）
- 用户在 Agent 停止后发来的**后续任务**（"刚才的功能写完了，现在帮我写测试"）

两者混在一起，导致：
1. 运行中注入消息可能打乱正在进行的 ToolUse/ToolResult 配对
2. 没有明确的"等 Agent 停止后再处理"语义
3. LLM 历史不断增长，无法区分"steering 上下文"与"主任务历史"

### 10.2 Pi 的双队列语义设计

Pi 在 `AgentLoopConfig` 中定义了两个语义完全分离的队列接口：

```typescript
// 队列1：Steering（实时插入）
// - 在当前turn的工具调用全部完成后，下一个LLM调用前注入
// - 不中断当前ToolUse/ToolResult配对
// - 适用：用户打字、自动化系统的实时调整指令
getSteeringMessages?: () => Promise<AgentMessage[]>;

// 队列2：Follow-up（串行后续）
// - 仅在 Agent 本要停止时（无更多工具调用、无steering消息）才检查
// - 注入后 Agent 继续运行新的Turn，历史连续
// - 适用：子任务链、自动化触发的后续工作
getFollowUpMessages?: () => Promise<AgentMessage[]>;
```

**双层循环的关键设计**（来自 Pi agent-loop.ts 第170-268行）：
```typescript
// 外层：处理 follow-up（串行后续任务）
while (true) {
  let pendingMessages = await config.getSteeringMessages?.() || [];
  
  // 内层：处理工具调用 + steering
  while (hasMoreToolCalls || pendingMessages.length > 0) {
    // 注入steering消息（在turn边界，不破坏ToolUse配对）
    for (const msg of pendingMessages) {
      currentContext.messages.push(msg);
    }
    // ... LLM调用、工具执行 ...
    pendingMessages = await config.getSteeringMessages?.() || [];
  }
  
  // Agent到达自然终止点：检查follow-up
  const followUpMessages = await config.getFollowUpMessages?.() || [];
  if (followUpMessages.length === 0) break;  // 真正结束
  pendingMessages = followUpMessages;         // 继续新Turn
}
```

**两个队列的本质区别**：

| 维度 | Steering 队列 | Follow-up 队列 |
|------|--------------|---------------|
| 注入时机 | 每个Turn结束后立即检查 | 仅在Agent自然停止时检查 |
| 对LLM历史的影响 | 插入现有历史中继续 | 插入现有历史中继续（无缝） |
| 适用场景 | 用户实时调整、用户打字中途插入 | 子任务链、自动化后续步骤 |
| 并发安全性 | 可在工具执行期间写入 | 在自然停止点消费 |

### 10.3 Rust 实现方案

#### 双队列数据结构

```rust
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// Steering 队列：实时插入，turn边界消费
pub struct SteeringQueue {
    messages: Arc<Mutex<VecDeque<ChatMessage>>>,
}

impl SteeringQueue {
    /// 外部调用（Tauri command / automation）：随时插入
    pub async fn push(&self, msg: ChatMessage) {
        self.messages.lock().await.push_back(msg);
    }

    /// Loop内消费：取出所有pending消息
    pub async fn drain(&self) -> Vec<ChatMessage> {
        let mut q = self.messages.lock().await;
        q.drain(..).collect()
    }

    pub async fn is_empty(&self) -> bool {
        self.messages.lock().await.is_empty()
    }
}

/// Follow-up 队列：串行后续任务，Agent停止时消费
pub struct FollowUpQueue {
    tasks: Arc<Mutex<VecDeque<Vec<ChatMessage>>>>,
    mode: QueueMode,
}

/// 队列消费模式
pub enum QueueMode {
    /// 一次取所有任务合并注入
    All,
    /// 一次取一个任务（严格串行）
    OneAtATime,
}

impl FollowUpQueue {
    /// 追加一组消息作为一个后续任务
    pub async fn push_task(&self, messages: Vec<ChatMessage>) {
        self.tasks.lock().await.push_back(messages);
    }

    /// Agent停止时消费：按mode返回消息
    pub async fn next(&self) -> Vec<ChatMessage> {
        let mut q = self.tasks.lock().await;
        match self.mode {
            QueueMode::All => q.drain(..).flatten().collect(),
            QueueMode::OneAtATime => q.pop_front().unwrap_or_default(),
        }
    }

    pub async fn is_empty(&self) -> bool {
        self.tasks.lock().await.is_empty()
    }
}
```

#### 集成到 ReasoningContext

```rust
pub struct ReasoningContext {
    pub messages: Vec<ChatMessage>,
    // ... 现有字段 ...

    /// Steering 队列（新增）：实时插入，turn边界消费
    pub steering_queue: Arc<SteeringQueue>,

    /// Follow-up 队列（新增）：串行后续任务，自然停止时消费
    pub follow_up_queue: Arc<FollowUpQueue>,
}
```

#### 双层循环改造（agentic_loop.rs）

```rust
pub async fn run_agentic_loop_v2(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
) -> LoopOutcome {
    reason_ctx.thread_state = ThreadState::Processing;

    // ── 外层：follow-up 串行后续 ──────────────────────────────
    'outer: loop {
        // ── 内层：工具调用 + steering 实时插入 ───────────────
        let mut has_more_tools = true;
        let mut pending_steering = reason_ctx.steering_queue.drain().await;

        'inner: while has_more_tools || !pending_steering.is_empty() {
            // 1. 注入steering消息（turn边界，安全点）
            for msg in pending_steering.drain(..) {
                reason_ctx.messages.push(msg);
            }

            // 2. 信号检查
            match delegate.check_signals().await {
                LoopSignal::Stop => return LoopOutcome::Stopped,
                LoopSignal::Cancel => return LoopOutcome::Cancelled { partial_code: None },
                _ => {}
            }

            // 3. 上下文压缩（含FileOps提取、迭代摘要）
            compress_context_if_needed(reason_ctx, config, delegate).await;

            // 4. Pre-LLM hook
            if let Some(outcome) = delegate.before_llm_call(reason_ctx, iteration).await {
                return outcome;
            }

            // 5. LLM调用 + TurnSnapshot（快照隔离）
            let output = delegate.call_llm(reason_ctx, iteration).await?;

            // 6. 处理响应
            match output {
                RespondOutput::ToolCalls { tool_calls, .. } => {
                    let (results, terminate) = delegate
                        .execute_tool_calls(&tool_calls, reason_ctx).await?;
                    has_more_tools = !terminate;
                }
                RespondOutput::Text { .. } => {
                    has_more_tools = false;
                }
            }

            // 7. Turn边界：prepareNextTurn + shouldStopAfterTurn
            if let Some(patch) = delegate.prepare_next_turn(reason_ctx).await {
                apply_turn_patch(reason_ctx, patch);
            }
            if delegate.should_stop_after_turn(reason_ctx).await {
                return LoopOutcome::Completed;
            }

            // 8. 消费下一批steering消息
            pending_steering = reason_ctx.steering_queue.drain().await;
        }

        // ── 外层：检查follow-up ────────────────────────────────
        let follow_ups = reason_ctx.follow_up_queue.next().await;
        if follow_ups.is_empty() {
            break 'outer;  // 真正结束
        }
        // 注入follow-up消息，继续新Turn（历史连续，无需reset）
        for msg in follow_ups {
            reason_ctx.messages.push(msg);
        }
    }

    reason_ctx.thread_state = ThreadState::Completed;
    LoopOutcome::Completed
}
```

#### Tauri IPC 接口

```rust
// 新增 Tauri commands
#[tauri::command]
pub async fn agent_steer(
    conversation_id: String,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.get_reasoning_context(&conversation_id)?;
    ctx.steering_queue.push(ChatMessage::user(&content)).await;
    Ok(())
}

#[tauri::command]
pub async fn agent_follow_up(
    conversation_id: String,
    messages: Vec<ChatMessageDto>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.get_reasoning_context(&conversation_id)?;
    let msgs = messages.into_iter().map(ChatMessage::from).collect();
    ctx.follow_up_queue.push_task(msgs).await;
    Ok(())
}
```

### 10.4 用户场景

| 场景 | 队列 | 效果 |
|------|------|------|
| 用户打字"先别写文件，检查一下测试" | Steering | 当前工具执行完成后，下一轮LLM立即看到指令 |
| 自动化触发"帮我把刚才的功能加到README" | Follow-up | Agent完成主任务后无缝继续子任务，历史连续 |
| 用户点击"取消"按钮 | 现有 LoopSignal::Cancel | 立即中断（保留） |
| 子任务链（Task A → Task B → Task C）| Follow-up（OneAtATime）| 串行执行，每个任务独立可追踪 |

### 10.5 ROI 评估

**开发成本**：双队列结构 1天 + 双层循环改造 2天 + Tauri IPC 0.5天 = **约3.5天**

**用户价值**：
- Office用户：多步骤工作流无缝串联（"写邮件 → 发送 → 更新CRM"）
- Coding用户：不打断Agent工作流的实时纠偏（当前最痛的交互场景）
- 自动化层：可靠的任务队列机制，不再需要重置整个会话

---

## Section 11 — 迭代式更新压缩 (Iterative Compaction)

### 11.1 问题溯源

UClaw 当前的压缩策略在触发时**从头生成摘要**：
- 每次压缩都把所有需要折叠的消息全量发给LLM生成新摘要
- 随着会话变长，被压缩的消息越来越多，摘要生成的输入token数线性增长
- 如果历史中有10轮对话已被折叠过，下次压缩时这10轮还要再次参与计算

**成本分析**：假设每次压缩的消息池为 N tokens，每折叠一次，下次折叠的输入池增加约 0.3N（摘要文本）。在长会话中，这会导致摘要调用的成本指数级增长。

此外，UClaw 当前在压缩时如果碰到**工具调用执行中**（ToolUse 已发出但 ToolResult 还在队列中），只能等待或报错，没有优雅的分割策略。

### 11.2 Pi 的增量摘要设计

Pi 在 `compaction.ts` 中实现了两个关键机制：

#### 机制1：UPDATE_SUMMARIZATION_PROMPT（增量更新）

```typescript
// 当存在上一次摘要时，不从头生成，而是增量更新
const UPDATE_SUMMARIZATION_PROMPT = `The messages above are NEW conversation messages 
to incorporate into the existing summary provided in <previous-summary> tags.

Update the existing structured summary with new information. RULES:
- PRESERVE all existing information from the previous summary
- ADD new progress, decisions, and context from the new messages
- UPDATE the Progress section: move items from "In Progress" to "Done" when completed
- PRESERVE exact file paths, function names, and error messages
- If something is no longer relevant, you may remove it`;

// 调用时的 prompt 构造
let promptText = `<conversation>\n${conversationText}\n</conversation>\n\n`;
if (previousSummary) {
  // 只发送【新增消息】+ 【上次摘要】，不发完整历史
  promptText += `<previous-summary>\n${previousSummary}\n</previous-summary>\n\n`;
}
promptText += previousSummary ? UPDATE_SUMMARIZATION_PROMPT : SUMMARIZATION_PROMPT;
```

**token 节省**：只发送新消息（自上次压缩以来的增量）+ 上次摘要（通常 < 2000 tokens）。不管历史有多长，每次摘要调用的输入 token 数都是**恒定有界的**。

#### 机制2：isSplitTurn + turnPrefix Active Suffix

```typescript
// prepareCompaction 的关键逻辑
const cutPoint = findCutPoint(pathEntries, boundaryStart, boundaryEnd, settings.keepRecentTokens);

// isSplitTurn = true 意味着压缩切点碰到了一个正在执行的Turn
if (cutPoint.isSplitTurn) {
  // 将Turn的前缀（已完成的工具调用）单独摘要
  for (let i = cutPoint.turnStartIndex; i < cutPoint.firstKeptEntryIndex; i++) {
    turnPrefixMessages.push(getMessageFromEntry(pathEntries[i]));
  }
}

// compact() 调用时的双重摘要
if (preparation.isSplitTurn && preparation.turnPrefixMessages.length > 0) {
  // 1. 摘要历史部分（不含当前Turn）
  const historyResult = await generateSummary(messagesToSummarize, ...);
  // 2. 摘要当前Turn前缀（已完成的工具调用）
  const turnPrefixResult = await generateSummary(turnPrefixMessages, ...);
  // 3. 拼接：history_summary + "---" + "Turn Context (split turn):" + turn_prefix_summary
  summary = `${historyResult.value}\n\n---\n\n**Turn Context (split turn):**\n\n${turnPrefixResult.value}`;
}
```

**活动后缀（Active Suffix）**：`firstKeptEntryId` 之后的消息原样保留（即当前正在执行的 ToolUse/ToolResult 后缀），前面全部替换为摘要，实现无缝衔接。

### 11.3 Rust 实现方案

#### 增量压缩状态

```rust
/// 每个会话维护的压缩状态
pub struct CompactionState {
    /// 上一次压缩生成的摘要（用于增量更新）
    pub previous_summary: Option<String>,
    /// 上一次压缩后保留消息的起始ID
    pub last_compaction_boundary: Option<u64>,
    /// 本次压缩前的token数（用于统计）
    pub tokens_before_last_compaction: u32,
}
```

#### 迭代摘要生成

```rust
/// 生成压缩摘要（支持增量更新）
pub async fn generate_compaction_summary(
    /// 仅传入自上次压缩以来的【新增消息】，不传全量历史
    new_messages: &[ChatMessage],
    previous_summary: Option<&str>,
    llm_client: &dyn LlmClient,
    model: &str,   // 推荐用便宜的模型（Haiku）
    reserve_tokens: u32,
) -> Result<String, CompactionError> {
    // 序列化仅新增消息（token数有界）
    let conversation_text = serialize_conversation(new_messages);

    let prompt = if let Some(prev) = previous_summary {
        // 增量模式：只发新消息 + 旧摘要
        format!(
            "<conversation>\n{}\n</conversation>\n\n<previous-summary>\n{}\n</previous-summary>\n\n{}",
            conversation_text,
            prev,
            UPDATE_SUMMARIZATION_PROMPT
        )
    } else {
        // 首次压缩：完整摘要
        format!("<conversation>\n{}\n</conversation>\n\n{}", conversation_text, SUMMARIZATION_PROMPT)
    };

    let max_tokens = (reserve_tokens as f32 * 0.8) as u32;
    llm_client.complete(model, &prompt, CompletionOptions { max_tokens, ..Default::default() }).await
}

const UPDATE_SUMMARIZATION_PROMPT: &str = r#"以上是需要并入到 <previous-summary> 中的新对话内容。

请增量更新现有摘要。规则：
- 保留已有摘要中的所有信息
- 将新对话中的进展、决策和上下文添加进来
- 更新"进行中"项目：已完成的移至"已完成"
- 保留精确的文件路径、函数名、错误信息
- 不再相关的内容可以删除

保持原有格式结构（## 目标 / ## 进展 / ## 关键决策 / ## 下一步）。"#;
```

#### Split-Turn 检测与恢复

```rust
/// 压缩切点结构（检测是否切中了正在执行的Turn）
pub struct CompactionCutPoint {
    /// 从此索引开始的消息原样保留（Active Suffix）
    pub first_kept_index: usize,
    /// 是否切到了一个正在执行的Turn中间
    pub is_split_turn: bool,
    /// 如果 is_split_turn，当前Turn的起始索引
    pub turn_start_index: Option<usize>,
}

pub fn find_compaction_cut_point(
    messages: &[ChatMessage],
    keep_recent_tokens: u32,
) -> CompactionCutPoint {
    // 从后往前计算token，找到keep_recent_tokens的边界
    let mut token_count = 0u32;
    let mut cut_index = messages.len();

    for (i, msg) in messages.iter().enumerate().rev() {
        token_count += estimate_tokens(msg);
        if token_count >= keep_recent_tokens {
            cut_index = i;
            break;
        }
    }

    // 检查cut_index是否落在一个Turn的中间
    // Turn边界：ToolUse 之后必须有对应的 ToolResult
    let is_split_turn = is_inside_tool_turn(messages, cut_index);

    if is_split_turn {
        // 找到当前Turn的起始位置（上一个user消息之后）
        let turn_start = find_turn_start(messages, cut_index);
        CompactionCutPoint {
            first_kept_index: cut_index,
            is_split_turn: true,
            turn_start_index: Some(turn_start),
        }
    } else {
        CompactionCutPoint {
            first_kept_index: cut_index,
            is_split_turn: false,
            turn_start_index: None,
        }
    }
}

/// 执行带Split-Turn恢复的压缩
pub async fn compress_with_iterative_summary(
    reason_ctx: &mut ReasoningContext,
    compaction_state: &mut CompactionState,
    llm_client: &dyn LlmClient,
    config: &CompactionConfig,
) -> Option<String> {
    let cut = find_compaction_cut_point(&reason_ctx.messages, config.keep_recent_tokens);

    // 新增消息：自上次压缩边界到cut_index（或turn_start）
    let boundary_start = compaction_state.last_compaction_boundary.unwrap_or(0) as usize;
    let history_end = cut.turn_start_index.unwrap_or(cut.first_kept_index);
    let new_messages = &reason_ctx.messages[boundary_start..history_end];

    // 生成增量摘要
    let summary = generate_compaction_summary(
        new_messages,
        compaction_state.previous_summary.as_deref(),
        llm_client,
        &config.summary_model,
        config.reserve_tokens,
    ).await.ok()?;

    // Split-Turn处理：为Turn前缀（已完成的工具调用）额外生成摘要
    let final_summary = if cut.is_split_turn {
        let turn_start = cut.turn_start_index.unwrap();
        let turn_prefix = &reason_ctx.messages[turn_start..cut.first_kept_index];
        let turn_prefix_summary = generate_compaction_summary(
            turn_prefix, None, llm_client, &config.summary_model, config.reserve_tokens / 2,
        ).await.ok()?;
        format!("{}\n\n---\n\n**当前Turn进度（分割摘要）：**\n\n{}", summary, turn_prefix_summary)
    } else {
        summary
    };

    // 重建消息列表：[摘要消息] + [Active Suffix（从first_kept_index开始）]
    let active_suffix = reason_ctx.messages[cut.first_kept_index..].to_vec();
    reason_ctx.messages = vec![ChatMessage::compaction_summary(&final_summary)];
    reason_ctx.messages.extend(active_suffix);

    // 更新压缩状态（下次压缩时只发新增消息）
    compaction_state.previous_summary = Some(final_summary.clone());
    compaction_state.last_compaction_boundary = Some(cut.first_kept_index as u64);

    Some(final_summary)
}
```

### 11.4 Token 优化对比

| 压缩方式 | 第1次压缩输入 | 第2次压缩输入 | 第N次压缩输入 |
|---------|------------|------------|------------|
| UClaw 当前（全量）| 8000 tokens | 12000 tokens | N×4000 tokens（线性增长）|
| Pi 迭代式（增量）| 8000 tokens | **4000 tokens新增 + 2000 summary** | **4000 tokens新增 + 2000 summary**（恒定）|

**结论**：迭代式压缩将第N次摘要调用的输入从 O(N) 降至 O(1)。对于4小时以上的长 coding session，token 节省超过 70%。

### 11.5 ROI 评估

**开发成本**：CompactionState 结构 0.5天 + 增量摘要逻辑 1.5天 + Split-Turn 检测 1天 = **约3天**

**用户价值**：
- 长会话成本大幅降低（token 节省 40-70%）
- 压缩不再丢失语义（增量更新保留所有上下文）
- 压缩碰到 ToolCall 不再报错（Split-Turn 优雅处理）

---

## Section 12 — 文件操作持久记忆 (File Operations Memory)

### 12.1 问题溯源

**最常见的 Agent 失忆场景**：

用户开启4小时 coding session，Agent 前期读取了 `src/auth.rs`、修改了 `src/db.rs`，然后在 Session 中段触发了压缩。压缩后，Agent 被问到"之前改过哪些文件"时，完全不知道——因为压缩摘要里没有文件操作记录，LLM 对自己做过的事情彻底失忆。

UClaw 目前的 `StructuredFold`（`agent/skeleton.rs` 的8轴摘要）也没有明确的文件读写列表提取机制。

### 12.2 Pi 的 FileOps 提取设计

Pi 在 `compaction/utils.ts` 中实现了从消息中自动提取文件操作的逻辑：

```typescript
export interface FileOperations {
  read: Set<string>;    // 已读文件路径
  written: Set<string>; // 已写文件路径
  edited: Set<string>;  // 已编辑文件路径（edit/apply_patch）
}

export function extractFileOpsFromMessage(message: AgentMessage, fileOps: FileOperations) {
  for (const content of message.content) {
    if (content.type !== "toolCall") continue;
    const { name, input } = content;
    switch (name) {
      case "read":
        if (input.path) fileOps.read.add(String(input.path));
        break;
      case "write":
        if (input.path) fileOps.written.add(String(input.path));
        break;
      case "edit":
        if (input.path) fileOps.edited.add(String(input.path));
        break;
    }
  }
}

// 在每次压缩时提取并附加到 CompactionDetails
export interface CompactionDetails {
  readFiles: string[];
  modifiedFiles: string[];  // written + edited 合并
}
```

**关键**：FileOps 在 compaction entry 中**累积**，而不是丢失：
```typescript
// 如果存在上一次压缩，先加载上次的文件操作
if (prevCompactionIndex >= 0) {
  const prevDetails = prevCompaction.details as CompactionDetails;
  for (const f of prevDetails.readFiles) fileOps.read.add(f);
  for (const f of prevDetails.modifiedFiles) fileOps.edited.add(f);
}
// 再追加新消息的文件操作
for (const msg of messagesToSummarize) {
  extractFileOpsFromMessage(msg, fileOps);
}
```

这意味着文件操作列表在多次压缩后**永不丢失**，形成了完整的会话文件操作历史。

### 12.3 Rust 实现方案

#### FileOps 追踪结构

```rust
/// 会话文件操作记录（跨压缩周期累积）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFileOps {
    pub read: HashSet<PathBuf>,
    pub written: HashSet<PathBuf>,
    pub edited: HashSet<PathBuf>,
}

impl SessionFileOps {
    /// 从工具调用中提取文件操作
    pub fn track_tool_call(&mut self, tool_name: &str, args: &serde_json::Value) {
        let path = match args.get("path").or(args.get("file_path"))
            .and_then(|p| p.as_str())
        {
            Some(p) => PathBuf::from(p),
            None => return,
        };

        match tool_name {
            "read_file" | "read" => { self.read.insert(path); }
            "write_file" | "write" => { self.written.insert(path); }
            "edit" | "apply_patch" | "str_replace" => { self.edited.insert(path); }
            "bash" => {
                // 尝试从bash命令中提取文件操作
                if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                    self.extract_from_bash(cmd);
                }
            }
            _ => {}
        }
    }

    /// 从bash命令中启发式提取文件操作（简单模式匹配）
    fn extract_from_bash(&mut self, cmd: &str) {
        // 检测: cat/less/head/tail <path> → read
        // 检测: > <path> / tee <path> / cp ... <path> → written
        // 粗粒度，主要依赖工具调用追踪
    }

    /// 合并另一个 FileOps（用于跨压缩周期累积）
    pub fn merge(&mut self, other: &SessionFileOps) {
        self.read.extend(other.read.iter().cloned());
        self.written.extend(other.written.iter().cloned());
        self.edited.extend(other.edited.iter().cloned());
    }

    /// 生成摘要附加到压缩文本尾部
    pub fn format_for_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.read.is_empty() {
            let files: Vec<_> = self.read.iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            parts.push(format!("**已读取文件**: {}", files.join(", ")));
        }
        let modified: HashSet<_> = self.written.union(&self.edited).collect();
        if !modified.is_empty() {
            let files: Vec<_> = modified.iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            parts.push(format!("**已修改文件**: {}", files.join(", ")));
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("\n\n---\n\n## 文件操作记录（跨压缩持久）\n\n{}", parts.join("\n"))
        }
    }
}
```

#### 与 StructuredFold 集成（8轴扩展）

UClaw 的 `agent/skeleton.rs` 实现了8轴 `StructuredFold`。在现有8轴基础上追加文件操作轴：

```rust
pub struct StructuredFold {
    // 现有8轴（省略）...

    /// 轴9：文件操作持久记忆（新增）
    pub file_ops: SessionFileOps,
}

impl StructuredFold {
    pub fn render_with_file_ops(&self) -> String {
        let base = self.render();
        let file_ops_section = self.file_ops.format_for_summary();
        format!("{}{}", base, file_ops_section)
    }

    /// 在每次工具调用后更新文件操作记录
    pub fn update_from_tool_call(&mut self, tool_name: &str, args: &serde_json::Value) {
        self.file_ops.track_tool_call(tool_name, args);
    }
}
```

#### 压缩摘要中的文件操作尾注

每次压缩后，摘要文本末尾追加文件操作列表：

```
## 进展
### 已完成
- [x] 实现了 AuthService 的 JWT 验证逻辑
- [x] 修复了数据库连接池泄漏

### 进行中
- [ ] 编写集成测试

---

## 文件操作记录（跨压缩持久）

**已读取文件**: src/auth.rs, src/db.rs, src/config.rs, Cargo.toml
**已修改文件**: src/auth.rs, src/db/pool.rs, tests/integration_test.rs
```

当下次压缩时，这段记录会被保留进新摘要（通过 UPDATE_SUMMARIZATION_PROMPT 的 PRESERVE 规则），形成永久记忆。

### 12.4 ROI 评估

**开发成本**：SessionFileOps 结构 0.5天 + 工具调用 hook 集成 0.5天 + StructuredFold 扩展 0.5天 + 摘要尾注格式化 0.5天 = **约2天**

**用户价值**：
- 完全消除"Agent 忘记自己改过哪些文件"的失忆场景
- 为用户提供"本次 session 的文件操作全览"视图（可在 UI 展示）
- 配合 Section 11 迭代摘要，实现真正的跨压缩永久上下文记忆

---

## Section 13 — 大体量终端输出保护 (Bash Temp Logging)

### 13.1 问题溯源

UClaw 的 bash 工具执行后将输出通过 Tauri IPC 传输到前端。当命令输出超大时（如 `cargo build` 的完整编译日志、`npm test --verbose` 的完整测试输出、日志文件 `cat`），存在两个严重问题：

1. **Tauri IPC 阻塞**：大 payload 通过 IPC 序列化时，JS 主线程被阻塞，整个 UI 卡死
2. **LLM Context 爆炸**：将100KB的 bash 输出直接放入 context，消耗大量 token，可能触发 hard_truncate

UClaw 目前没有对 bash 输出做任何大小限制，依赖 `hard_truncate_context` 被动处理，但这是在 context 已经爆炸之后。

### 13.2 Pi 的 Shell 输出管理

Pi 的 bash 工具设计了**流式输出 + 截断保护**机制：

```typescript
// BashTool 的执行设计
interface BashOperations {
  exec: (
    command: string,
    cwd: string,
    options: {
      onData: (data: Buffer) => void;  // 实时流式回调
      signal?: AbortSignal;
      timeout?: number;
    }
  ) => Promise<{ exitCode: number | null }>;
}

// Shell 输出截断（packages/agent/src/harness/utils/shell-output.ts）
export function formatShellOutput(
  stdout: string,
  stderr: string,
  exitCode: number | null,
  options: { maxLength?: number; tailLines?: number }
): string {
  // 超过 maxLength 时只取 tail（最后 N 行）
  // 被截断时在开头标注："[Output truncated - showing last N lines]"
}
```

UClaw 需要的保护更全面：不仅截断给 context，还需要将溢出内容落盘到 temp 文件，让用户和 Agent 都能访问完整输出。

### 13.3 Rust 实现方案

#### Rolling Tail Buffer

```rust
/// 滚动尾部缓冲区：保留最近 N 字节，超出时丢弃头部
pub struct RollingTailBuffer {
    /// 最大缓冲容量（字节）
    capacity: usize,
    /// 实际内容（ringbuffer 语义）
    buf: VecDeque<u8>,
    /// 总写入字节数（含已丢弃部分）
    total_written: usize,
    /// 已丢弃的字节数
    dropped: usize,
}

impl RollingTailBuffer {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, buf: VecDeque::with_capacity(capacity), total_written: 0, dropped: 0 }
    }

    pub fn push_bytes(&mut self, data: &[u8]) {
        self.total_written += data.len();
        for &byte in data {
            if self.buf.len() >= self.capacity {
                self.buf.pop_front();
                self.dropped += 1;
            }
            self.buf.push_back(byte);
        }
    }

    /// 生成返回给 LLM 的截断文本
    pub fn to_context_string(&self) -> String {
        let content = String::from_utf8_lossy(self.buf.as_slices().0).to_string()
            + &String::from_utf8_lossy(self.buf.as_slices().1);

        if self.dropped > 0 {
            format!(
                "[输出已截断：共 {} 字节，显示最后 {} 字节。完整输出已保存至 {}]\n\n{}",
                self.total_written,
                self.buf.len(),
                self.temp_path.display(),
                content
            )
        } else {
            content
        }
    }
}
```

#### 落盘 Temp 文件

```rust
/// Bash 工具执行器（带 Rolling Tail + Temp 落盘）
pub struct BashExecutor {
    /// 保留给 LLM context 的最大字节（默认 32KB）
    pub context_limit: usize,
    /// Temp 文件目录（~/.uclaw/temp/）
    pub temp_dir: PathBuf,
}

impl BashExecutor {
    pub async fn execute(
        &self,
        command: &str,
        cwd: &Path,
        signal: CancellationToken,
        on_update: Option<ToolUpdateSender>,
    ) -> Result<BashResult, ToolError> {
        // 创建 temp 文件（命令完成前就创建，Agent可以在工具执行中访问）
        let temp_path = self.temp_dir.join(format!("bash-{}.log", uuid::Uuid::new_v4()));
        let mut temp_file = tokio::fs::File::create(&temp_path).await?;

        let mut tail_buffer = RollingTailBuffer::new(self.context_limit);

        let mut child = tokio::process::Command::new("bash")
            .arg("-c").arg(command)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // 合并 stdout + stderr，同时写入 tail_buffer 和 temp_file
        let mut merged = tokio::io::join(stdout, stderr);
        let mut read_buf = vec![0u8; 4096];

        loop {
            tokio::select! {
                // 读取输出
                n = merged.read(&mut read_buf) => {
                    let n = n?;
                    if n == 0 { break; }
                    let chunk = &read_buf[..n];

                    // 1. 落盘 temp 文件（完整记录）
                    temp_file.write_all(chunk).await?;

                    // 2. Rolling tail buffer（只保留最近 context_limit 字节）
                    tail_buffer.push_bytes(chunk);

                    // 3. 流式更新 UI（如果有 on_update）
                    if let Some(ref tx) = on_update {
                        let text = String::from_utf8_lossy(chunk).to_string();
                        let _ = tx.send(ToolUpdate {
                            content: text,
                            progress: None,
                        });
                    }
                }
                // 取消信号
                _ = signal.cancelled() => {
                    child.kill().await?;
                    break;
                }
            }
        }

        let exit_status = child.wait().await?;

        // 生成返回给 LLM 的截断文本
        let context_output = tail_buffer.to_context_string_with_path(&temp_path);

        Ok(BashResult {
            context_output,       // 给 LLM 看的（有界）
            temp_path,            // Agent 可以 read_file 获取完整输出
            total_bytes: tail_buffer.total_written,
            was_truncated: tail_buffer.dropped > 0,
            exit_code: exit_status.code(),
        })
    }
}
```

#### 返回给 LLM 的输出格式

```
[输出已截断：共 487,392 字节，显示最后 32,768 字节。
完整输出已保存至 /Users/ryanliu/.uclaw/temp/bash-a3f9b2c1.log
如需查看完整内容，请使用 read_file 工具读取该文件。]

   Compiling uclaw_core v0.1.0
   Compiling agent v0.1.0
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 47.3s
```

**设计要点**：Agent 知道完整输出在哪，可以主动调用 `read_file` 工具读取，实现"按需访问完整日志"而非"全量塞入 context"。

#### 配置参数

```rust
pub struct BashConfig {
    /// 返回给 LLM 的最大字节数（默认 32KB，约 8K tokens）
    pub context_limit: usize,

    /// Temp 文件保留时间（默认 24 小时，之后自动清理）
    pub temp_retention: Duration,

    /// 大输出阈值：超过此值时提示 Agent 使用 read_file
    /// （默认 8KB，即使不截断也建议分段读取）
    pub large_output_threshold: usize,

    /// Temp 文件目录
    pub temp_dir: PathBuf,  // 默认: ~/.uclaw/temp/
}

impl Default for BashConfig {
    fn default() -> Self {
        Self {
            context_limit: 32 * 1024,           // 32KB
            temp_retention: Duration::hours(24),
            large_output_threshold: 8 * 1024,   // 8KB
            temp_dir: uclaw_home().join("temp"),
        }
    }
}
```

#### Temp 文件清理

```rust
/// 后台定时清理过期 temp 文件（在 ServiceManager [Stage 3] 注册）
pub struct TempCleanupService {
    config: BashConfig,
}

impl TempCleanupService {
    pub async fn cleanup_expired(&self) -> Result<usize, IoError> {
        let cutoff = SystemTime::now() - self.config.temp_retention;
        let mut removed = 0;
        let mut entries = tokio::fs::read_dir(&self.config.temp_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if let Ok(metadata) = entry.metadata().await {
                if metadata.modified().ok().map_or(false, |t| t < cutoff) {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }
}
```

### 13.4 ROI 评估

**开发成本**：RollingTailBuffer 1天 + BashExecutor 改造 1天 + TempCleanupService 0.5天 = **约2.5天**

**用户价值**：
- 彻底消除大输出导致的 UI 卡死（Tauri IPC 阻塞）
- LLM context 不再因大输出而爆炸（主动保护）
- Agent 可通过 read_file 按需访问完整日志（能力不丢失）
- 对 Coding 用户：`cargo build`、`npm test`、`docker logs` 等大输出命令变得可靠

---

## 更新后的完整改进路线图

### 综合 ROI 矩阵（含 Section 10-13）

| 改进项 | 用户价值 | 开发成本 | ROI | 推荐顺序 |
|--------|---------|---------|-----|---------|
| **Bash Temp Logging** | ⭐⭐⭐⭐⭐ | 2.5天 | 极高（防崩溃）| Sprint 1 |
| **Bash 流式输出** | ⭐⭐⭐⭐⭐ | 2天 | 极高 | Sprint 1 |
| **FileOps 持久记忆** | ⭐⭐⭐⭐⭐ | 2天 | 极高 | Sprint 1 |
| Skills XML 注入 | ⭐⭐⭐⭐⭐ | 1.5天 | 极高 | Sprint 1 |
| 工具并行执行 | ⭐⭐⭐⭐ | 2天 | 高 | Sprint 1 |
| **双交互队列** | ⭐⭐⭐⭐⭐ | 3.5天 | 极高 | Sprint 2 |
| **迭代式摘要压缩** | ⭐⭐⭐⭐⭐ | 3天 | 极高（省Token）| Sprint 2 |
| TurnSnapshot 快照隔离 | ⭐⭐⭐⭐⭐ | 5天 | 极高（架构基础）| Sprint 2 |
| Hooks 系统 | ⭐⭐⭐⭐ | 4天 | 高 | Sprint 3 |
| AgentTool Trait 规范化 | ⭐⭐⭐⭐ | 3天 | 高 | Sprint 3 |
| session_tree DB 迁移 | ⭐⭐⭐⭐ | 3天 | 高 | Sprint 3 |
| 多模型支持（OpenAI）| ⭐⭐⭐⭐⭐ | 1周 | 最高（用户获取）| Sprint 4 |
| 多模型支持（Google）| ⭐⭐⭐⭐ | 1周 | 高 | Sprint 5 |
| 分支历史 UI | ⭐⭐⭐⭐ | 5天 | 高 | Sprint 5 |

### 最终执行序列（更新版）

```
Sprint 1（2.5周）— 稳定性 + 即时可感知体验
  ✦ Bash Temp Logging（RollingTailBuffer + temp落盘，防IPC崩溃）
  ✦ Bash 流式输出（onUpdate回调，实时显示）
  ✦ FileOps 持久记忆（StructuredFold第9轴 + compaction尾注）
  ✦ Skills XML 注入到系统 prompt
  ✦ 工具并行执行

Sprint 2（3周）— 长会话可靠性 + 架构基础
  ✦ 双交互队列（SteeringQueue + FollowUpQueue + Tauri IPC）
  ✦ 迭代式压缩（UPDATE_SUMMARIZATION_PROMPT + Split-Turn恢复）
  ✦ TurnSnapshot 快照隔离（为多模型切换奠基）

Sprint 3（3周）— 扩展生态基础
  ✦ Hooks 系统
  ✦ AgentTool Trait 规范化（dispatcher.rs解耦）
  ✦ session_tree DB 迁移

Sprint 4（3周）— 用户增长驱动
  ✦ LlmProvider trait + OpenAI Provider
  ✦ BYOK UI

Sprint 5（3周）— 差异化竞争优势
  ✦ Google Gemini Provider
  ✦ 分支历史 UI
  ✦ Workspace 感知工具集
```

### 保留的 UClaw 独特优势（不需要向 Pi 学习）

| 功能 | 保留理由 |
|------|---------|
| Heartbeat + FlightRecorder | 桌面应用崩溃恢复，Pi（无头服务）不需要 |
| 不洁关闭恢复 | 桌面平台独特需求 |
| 防假进展守卫 | 提升 coding 任务可靠性，Pi没有对应设计 |
| B2 缓存优化 | 降低API成本，在动态prompt时需特别保护 |
| SafetyMode/批准流 | 企业/家长控制场景 |
| Tauri 原生能力 | 文件系统、系统托盘、原生通知 |

---

*报告生成: 2026-05-26 | 覆盖代码: uclaw(38998 symbols) + pi(packages/agent, packages/ai) | 作者: Claude Sonnet 4.6*

*报告生成: 2026-05-26 | 覆盖代码: uclaw(38998 symbols) + pi(packages/agent, packages/ai) | 作者: Claude Sonnet 4.6*
