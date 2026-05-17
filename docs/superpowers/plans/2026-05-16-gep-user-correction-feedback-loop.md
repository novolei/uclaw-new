# GEP 自进化引擎 — 用户否定反馈捕获回路 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 修复 GEP 自进化引擎的三个关键数据回路断裂点——①Capsule→effective_streak 未回写到 GeneRetriever 导致排序陈旧、②工具失败信号未注入 gene_candidate_pool、③用户否定反馈（计划拒绝/停止/纠正）未被任何学习机制捕获——打通从用户纠正到 Gene 蒸馏的完整闭环。

**Architecture:** 核心思路是在 InfraService 消息总线新增 `UserCorrection` 事件类型，dispatcher 在检测到 exit_plan_mode 拒绝等用户反馈时发布该事件，ProactiveService 的 context_listener 消费后构建高优先级 LearningCard 注入 gene_candidate_pool，最终被 GeneEvolutionScenario 蒸馏为 AVOID cues 增补信号。

**Tech Stack:** Rust (tokio, serde_json, std::sync::Mutex, chrono), React (Jotai atoms, Tauri event listen).

**Spec:** [docs/superpowers/specs/2026-05-16-agent-self-evolution-gep-design.md](../specs/2026-05-16-agent-self-evolution-gep-design.md) §8.11

---

## 关键事实（实现者必读）

- `InfraEventType` 枚举定义在 `src-tauri/src/infra/types.rs`，所有新增事件类型需同时更新 `InfraService` 的快捷发布方法。
- `ProactiveService::start_context_listener` 在 `src-tauri/src/proactive/service.rs` 中维护一个 `match event.event_type { ... }` 的巨型 match，新事件分支需要插入到该 match 中。
- `ChatDelegate::generate_capsule_for_turn` 在每次 tool 执行完成后生成 Capsule 并持久化，是 effective_streak 回写的最佳时机。
- `ChatDelegate` 持有 `gene_retriever: Option<Arc<GeneRetriever>>` 和 `gene_repo: Option<Arc<Mutex<GeneRepository>>>`，`GeneRetriever::set_streaks` 通过 `Mutex<HashMap<String, f32>>` 内部更新无需 `&mut self`。
- `gene_candidate_pool` 是 `Arc<RwLock<VecDeque<GeneCandidate>>>`，容量上限 20。UserCorrection 使用 `push_front`（高优先级），普通 self_eval/tool_failure 使用 `push_back`。
- `exit_plan_mode` 拒绝时返回的 error 格式为 `"User rejected the plan. Feedback: {用户反馈文本}"`，前端通过 `respond_exit_plan_mode({ decision: "reject", feedback: "..." })` 触发。

---

## 数据流图

```
┌─────────────────────────────────────────────────────────────────────┐
│                     GEP 自进化引擎完整数据回路                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────────────┐ │
│  │ User Action  │     │ Agent Tool   │     │ System Signal        │ │
│  ├──────────────┤     ├──────────────┤     ├──────────────────────┤ │
│  │ 拒绝计划     │     │ exit_plan_   │     │ self_eval learnings  │ │
│  │ 停止执行     │     │ mode reject  │     │ tool_executed fail   │ │
│  │ 纠正输出     │     │ (ToolError)  │     │ session_evals        │ │
│  └──────┬───────┘     └──────┬───────┘     └──────────┬───────────┘ │
│         │                    │                        │              │
│         ▼                    ▼                        ▼              │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │              InfraService (tokio::sync::broadcast)            │   │
│  │  ┌─────────────────┐  ┌──────────────┐  ┌─────────────────┐ │   │
│  │  │ UserCorrection  │  │ SkillLearned │  │ ToolExecuted    │ │   │
│  │  │ (NEW)           │  │ (self_eval)  │  │ (success=false) │ │   │
│  │  └────────┬────────┘  └──────┬───────┘  └────────┬────────┘ │   │
│  └───────────┼──────────────────┼───────────────────┼──────────┘   │
│              │                  │                   │               │
│              ▼                  ▼                   ▼               │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │           ProactiveService context_listener                   │   │
│  │  ┌───────────────────────────────────────────────────────┐   │   │
│  │  │          gene_candidate_pool (VecDeque, max=20)        │   │   │
│  │  │  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────┐   │   │
│  │  │  │UC:P0 │ │SE:0.3│ │TF:0.3│ │SE:0.5│ │  ...      │   │   │
│  │  │  │(FRONT)│ │      │ │      │ │      │ │  (BACK)   │   │   │
│  │  │  └──────┘ └──────┘ └──────┘ └──────┘ └──────────┘   │   │
│  │  └───────────────────────────────────────────────────────┘   │   │
│  └──────────────────────────┬───────────────────────────────────┘   │
│                             │                                       │
│                             ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │              GeneEvolutionScenario                             │   │
│  │  pool.len() >= threshold → LLM distillation → Gene + Capsule  │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                             │                                       │
│                             ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Gene: AVOID cues 增补 → version bump → GeneRepository       │   │
│  │  Capsule: outcome record → effective_streak → GeneRetriever  │   │
│  │  Event: EvolutionEvent audit trail                           │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Task P0-1: Capsule→effective_streak 回写到 GeneRetriever

### Problem

`generate_capsule_for_turn` 在每次 tool 执行后计算 `effective_streak` 并持久化到 GeneRepository，但从未将更新后的 streak 推回 `GeneRetriever`。导致 GeneRetriever 的 `rank_matches` 使用 `build_gene_retriever()` 时的旧数据，直到下一次 `build_gene_retriever()` 调用（下次用户消息）才会刷新。

### Fix

在 `generate_capsule_for_turn` 末尾（Capsule 持久化完成后、`recent_tool_errors` 清除前），从 GeneRepository 重新读取所有 active genes 的最新 Capsule 历史，计算 effective_streak，调用 `GeneRetriever::set_streaks()` 回写。

### - [x] Step 1: 添加 effective_streak 批量回写逻辑

**File:** `src-tauri/src/agent/dispatcher.rs`

在 `generate_capsule_for_turn` 方法中，Capsule 持久化循环完成后，添加：

```rust
// P0-1: Push computed effective_streaks back to GeneRetriever
if let Some(ref retriever) = self.gene_retriever {
    if let Some(ref repo_arc) = self.gene_repo {
        if let Ok(repo) = repo_arc.lock() {
            let now_ts = chrono::Utc::now().timestamp_millis();
            let mut streaks = std::collections::HashMap::new();
            let mut gene_count = 0usize;
            if let Ok(active) = repo.list_active_genes() {
                gene_count = active.len();
                for gene in &active {
                    if let Ok(capsules) = repo.list_capsules(&gene.gene_id) {
                        if let Some(latest) = capsules.first() {
                            let prev: Vec<Capsule> = capsules.iter().skip(1).take(5).cloned().collect();
                            let streak = latest.compute_effective_streak(&prev, now_ts);
                            streaks.insert(gene.gene_id.clone(), streak);
                        }
                    }
                }
            }
            if !streaks.is_empty() {
                retriever.set_streaks(streaks);
            }
        }
    }
}
```

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

Expected: 编译通过，`GeneRetriever::set_streaks` 接受 `&self`（非 `&mut self`）。

---

## Task P0-2: 工具失败聚合注入到 gene_candidate_pool

### Problem

工具执行失败事件（`ToolExecuted` with `success=false`）通过 InfraService 发布后，context_listener 仅记录到 ExecutionLogCollector 和 ToolUsageMemoryManager，但**未注入 gene_candidate_pool**。这意味着工具失败模式永远不会被蒸馏为 Gene 的 AVOID cues。

### Fix

在 context_listener 的 `InfraEventType::ToolExecuted` 处理分支中，当 `success=false` 时，提取错误摘要，检查去重，作为 `GeneCandidate`（source="tool_failure", card_type=FailureLesson, score=0.3）推入 gene_candidate_pool。

### - [x] Step 1: 添加 tool failure 注入逻辑

**File:** `src-tauri/src/proactive/service.rs`

在 context_listener 的 `ToolExecuted` 分支中，`record_tool_usage` 之后添加：

```rust
// P0-2: 工具失败聚合注入到 gene_candidate_pool
if !success {
    let error_summary: String = {
        let s = tool_output_str.trim();
        if s.len() > 200 { format!("{}...", &s[..200]) } else { s.to_string() }
    };
    let session_id = last_session.read().await.clone().unwrap_or_default();

    // Simple dedup by error prefix
    let dedup_key = format!("{}:{}", tool_name, &error_summary[..error_summary.len().min(80)]);
    let mut pool = gene_pool.write().await;
    let already_exists = pool.iter().any(|c| {
        c.content.contains(&dedup_key[..dedup_key.len().min(40)])
    });

    if !already_exists {
        let candidate = GeneCandidate {
            source: "tool_failure".to_string(),
            content: format!("Tool '{}' failed: {} | Session: {}", tool_name, error_summary, session_id),
            card_type: Some(LearningCardType::FailureLesson),
            score: Some(0.3),
            session_id: Some(session_id),
            reasoning: Some(format!("Tool '{}' execution failed.", tool_name)),
            timestamp: chrono::Utc::now(),
        };
        const MAX_CANDIDATES: usize = 20;
        if pool.len() >= MAX_CANDIDATES { pool.pop_back(); }
        pool.push_back(candidate);
        new_gene_candidates_flag.store(true, Ordering::SeqCst);
        has_new.store(true, Ordering::SeqCst);
    }
}
```

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P1-3a: 新增 UserCorrection 事件类型 + publish 方法

### Problem

`InfraEventType` 枚举缺少用户纠正事件的类型定义，`InfraService` 缺少对应的快捷发布方法。UserCorrection 是整个用户否定反馈捕获回路的基础设施。

### Fix

1. 在 `InfraEventType` 枚举中添加 `UserCorrection` 变体
2. 在 `InfraService` 中添加 `publish_user_correction` 快捷发布方法

### - [x] Step 1: 添加 InfraEventType::UserCorrection

**File:** `src-tauri/src/infra/types.rs`

在 `CapsuleCreated` 之后添加：

```rust
// ─── 用户反馈相关 ───
/// 用户否定/纠正反馈（拒绝计划、停止执行、纠正输出等）
UserCorrection,
```

### - [x] Step 2: 添加 publish_user_correction 快捷方法

**File:** `src-tauri/src/infra/service.rs`

在 `publish_capsule_created` 之后添加：

```rust
/// 快捷方法：发布「用户纠正」事件
pub async fn publish_user_correction(
    &self,
    platform: &str,
    feedback: &str,
    metadata: serde_json::Value,
) {
    let event = InfraEvent {
        id: 0,
        event_type: InfraEventType::UserCorrection,
        platform: platform.to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        message: ConversationMessage {
            role: "user".to_string(),
            content: feedback.to_string(),
        },
        metadata,
        trace_id: None,
    };
    self.publish(event).await;
}
```

### - [ ] Step 3: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P1-3b: dispatcher 检测用户拒绝反馈并发布 UserCorrection 事件

### Problem

当用户通过 `exit_plan_mode` 拒绝 Agent 的计划时，ToolError 包含反馈文本（格式：`"User rejected the plan. Feedback: {text}"`），但 dispatcher 仅将其作为普通 tool error 处理，未识别为用户纠正信号并发布到消息总线。

### Fix

在 `ChatDelegate::execute_tool_calls` 的工具调用错误处理分支（`Err(e)`）中，检测 `exit_plan_mode` 的拒绝模式，提取反馈文本，调用 `infra.publish_user_correction()` 发布事件。

### - [x] Step 1: 添加检测逻辑

**File:** `src-tauri/src/agent/dispatcher.rs`

在 tool error 收集之后、error_result_str 构造之前添加：

```rust
// P1-3b: Detect user rejection feedback
if let Some(ref infra) = self.infra_service {
    let err_msg = e.to_string();
    if tc.name == "exit_plan_mode" && err_msg.starts_with("User rejected the plan.") {
        let feedback = err_msg
            .strip_prefix("User rejected the plan. Feedback: ")
            .unwrap_or(&err_msg)
            .to_string();
        infra.publish_user_correction(
            "local",
            &feedback,
            serde_json::json!({
                "session_id": self.conversation_id,
                "source": "plan_rejection",
                "feedback": feedback,
                "trigger_context": "Agent submitted a plan via exit_plan_mode; user rejected it.",
                "tool_name": tc.name,
            }),
        ).await;
    }
}
```

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P1-3c: context_listener 消费 UserCorrection 事件

### Problem

UserCorrection 事件发布后，ProactiveService 的 context_listener 没有对应的消费分支，事件被丢弃。

### Fix

在 context_listener 的事件 match 中添加 `InfraEventType::UserCorrection` 分支，从事件 metadata 中提取 source、feedback、session_id 等字段，构建 `LearningCard`（card_type=FailureLesson, score=0.1），注入 gene_candidate_pool 队列头部（`push_front`，最高优先级）。

### - [x] Step 1: 添加 UserCorrection 消费分支

**File:** `src-tauri/src/proactive/service.rs`

在 context_listener 的 `SkillLearned` 分支之后、`_ => {}` 之前添加：

```rust
// P1-3c: 用户纠正事件 → 解析为高优先级 FailureLesson 注入候选池
InfraEventType::UserCorrection => {
    let source = event.metadata.get("source").and_then(|v| v.as_str()).unwrap_or("unknown");
    let session_id = event.metadata.get("session_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let feedback = event.metadata.get("feedback").and_then(|v| v.as_str()).unwrap_or(&event.message.content);
    let trigger_context = event.metadata.get("trigger_context").and_then(|v| v.as_str()).unwrap_or("");

    let learning_card = LearningCard {
        raw: format!("[UserCorrection:{}] {} — Context: {}", source, feedback, trigger_context),
        card_type: LearningCardType::FailureLesson,
        failure_signal: Some(source.to_string()),
        tool_name: event.metadata.get("tool_name").and_then(|v| v.as_str()).map(String::from),
        strategy_hint: StrategyHint {
            condition: format!("User {} detected", source),
            action: feedback.to_string(),
            reason: trigger_context.to_string(),
        },
        files_touched: vec![],
        session_id: session_id.to_string(),
        score: 0.1, // User corrections = highest confidence failure signal
        timestamp: event.timestamp,
    };

    let candidate = GeneCandidate {
        source: format!("user_correction:{}", source),
        content: learning_card.raw.clone(),
        card_type: Some(LearningCardType::FailureLesson),
        score: Some(0.1),
        session_id: Some(session_id.to_string()),
        reasoning: Some(format!("User {} feedback: {}", source, feedback)),
        timestamp: chrono::Utc::now(),
    };

    let mut pool = gene_pool.write().await;
    const MAX_CANDIDATES: usize = 20;
    if pool.len() >= MAX_CANDIDATES { pool.pop_back(); }
    pool.push_front(candidate); // User corrections get front priority
    new_gene_candidates_flag.store(true, Ordering::SeqCst);
    has_new.store(true, Ordering::SeqCst);
}
```

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P1-1: Capsule failure→MutationCandidate 反馈回路

### Problem

当同一 Gene 产生 ≥2 个 failed/partial Capsule 时，系统应自动触发 AVOID cues 增补（Stage 1 变异），将失败模式蒸馏为新的 AVOID 警告。当前 `gep/lifecycle.rs` 中的 `check_avoid_augmentation` 已实现基础逻辑，但未与定时审计任务集成。

### Fix

确保 `GeneLifecycleManager::audit_all_active` 在 ProactiveService 的 tick 循环中被周期性调用（已有 `gene_lifecycle` 字段），触发 AVOID cues 增补检查。

### - [ ] Step 1: 验证 tick 集成

**File:** `src-tauri/src/proactive/service.rs`

检查 ProactiveService 的 tick 循环中是否已调用 `gene_lifecycle.audit_all_active()`。如果是，标记完成；如果否，添加调用。

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P1-2: self_eval 系统提示增强

### Problem

当前 system prompt 中没有明确指引 Agent 在 session 结束前调用 `self_eval`。导致 `self_eval` 完全依赖 Agent 自主判断，调用率低且不稳定。

### Fix

在 system prompt 末尾添加 `SELF_EVAL_TRIGGER` section，明确要求 Agent 在以下时机调用 `self_eval`：
1. 每次 tool 执行回合结束后（可选）
2. session 结束前（强制）
3. 用户明确要求评估时

### - [ ] Step 1: 添加 self_eval prompt section

**File:** `src-tauri/src/agent/prompts/system.md` (或等效的 system prompt 组装处)

添加以下 section：

```markdown
## Self Evaluation (GEP)

After completing a task or at the end of a session, you MUST call `self_eval` with:
- `score`: 0.0–1.0 completion quality 
- `reasoning`: why you gave this score
- `learnings`: reusable insights that could improve future performance

Failure signals (errors, user corrections, incomplete work) should be recorded as learnings — they feed into the Gene Evolution Protocol for continuous improvement.
```

### - [ ] Step 2: 编译验证

System prompt changes不需要编译，但需要验证 prompt 注入流程。

---

## Task P2-1: Gene 生命周期自动审计

### Problem

Gene 状态机（active → stale → retired）的退役条件检查（连续失败、长期未激活、环境指纹失效等）未被周期性执行，导致退役逻辑无法自动触发。

### Fix

确保 `GeneLifecycleManager::audit_all_active` 在 ProactiveService tick 中被调用，并添加 tracing 日志记录审计结果。

### - [ ] Step 1: 验证并加强 tick 集成

**File:** `src-tauri/src/proactive/service.rs`

在 tick 循环中添加（或确认已存在）：
```rust
if let Ok(mut lifecycle) = self.gene_lifecycle.lock() {
    if let Err(e) = lifecycle.audit_all_active(&config.gene_evolution) {
        tracing::warn!("Gene lifecycle audit failed: {}", e);
    }
}
```

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## Task P2-2: session 结束自动 eval 兜底

### Problem

当 session 正常结束（Agent 完成任务）或异常结束（用户停止、错误退出）时，没有机制自动触发 `self_eval`。导致许多 session 完全无评估记录。

### Fix

在 `agentic_loop.rs` 的 `run_agentic_loop` 返回后，或在 `ChatDelegate` 的 after_iteration hook 中，自动调用 `SelfEvalTool::execute` 并持久化结果。

### - [ ] Step 1: 添加 auto-eval 逻辑

**File:** `src-tauri/src/agent/agentic_loop.rs` 或 `src-tauri/src/agent/dispatcher.rs`

在 agent loop 退出后（`LoopOutcome` 返回点），如果该 session 尚未调用 `self_eval`，自动生成一个兜底评估：
- score 基于 LoopOutcome 类型：Success=0.8, MaxIterations=0.5, Failure=0.2, Stopped/Cancelled=0.3
- reasoning 自动生成（"Session ended: {outcome}"）

### - [ ] Step 2: 编译验证

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check 2>&1 | head -30
```

---

## File Structure

**Modified files (existing):**
- `src-tauri/src/infra/types.rs` — +1 `InfraEventType::UserCorrection` variant
- `src-tauri/src/infra/service.rs` — +1 `publish_user_correction` method
- `src-tauri/src/agent/dispatcher.rs` — P0-1 effective_streak 回写 + P1-3b UserCorrection 检测
- `src-tauri/src/proactive/service.rs` — P0-2 tool failure 注入 + P1-3c UserCorrection 消费
- `src-tauri/src/agent/gep/lifecycle.rs` — P1-1 AVOID cues 增补（已有基础逻辑）
- `src-tauri/src/agent/agentic_loop.rs` — P2-2 auto-eval 兜底
- `docs/superpowers/specs/2026-05-16-agent-self-evolution-gep-design.md` — §8.11 + §6 更新

**No new files created.**

---

## 验证方法

### 单元测试

| 测试场景 | 验证点 |
|---|---|
| `InfraEventType::UserCorrection` 序列化/反序列化 | 新事件类型可正确 round-trip |
| `publish_user_correction` 发布+订阅 | 订阅者能收到 UserCorrection 事件 |
| dispatcher 拒绝检测 | exit_plan_mode 拒绝错误能正确识别并发布事件 |
| tool failure 去重 | 相同错误模式不重复注入 pool |
| UserCorrection push_front 优先级 | 用户纠正信号排在其他候选前面 |
| effective_streak 回写 | Capsule 生成后 GeneRetriever streak 数据更新 |

### 端到端测试

1. **Plan 拒绝反馈回路**：
   - 进入 Plan mode → Agent 调用 `exit_plan_mode` → 用户点击"拒绝"并输入反馈 → 验证 `UserCorrection` 事件被发布 → 验证 gene_candidate_pool 中存在新候选

2. **工具失败注入**：
   - Agent 执行 `bash`（故意失败命令） → 验证 `ToolExecuted(success=false)` 被发布 → 验证 gene_candidate_pool 中存在 `source="tool_failure"` 候选

3. **effective_streak 刷新**：
   - Capsule 生成后 → 下一个 LLM 调用时 Gene 排序使用最新 streak → 验证 ranking 变化

---

## 预期效果

| 指标 | 当前状态 | 预期改善 |
|---|---|---|
| 用户否定反馈捕获率 | 0% | 100%（plan_rejection 场景） |
| self_eval 调用率 | ≤30% sessions | ≥80% sessions（P1-2 + P2-2 兜底） |
| gene_candidate_pool 来源多样性 | 1 种（self_eval） | 3 种（self_eval + tool_failure + user_correction） |
| GeneRetriever streak 新鲜度 | 用户消息间延迟（分钟级） | Capsule 生成后即时（毫秒级） |
| AVOID cues 自动增补触达 | 未触发 | ≥2 failed capsules → 自动增补 |

---

## 实施顺序与依赖

```
P0-1 (streak回写) ────┐
                      ├── 无依赖，可并行
P0-2 (tool_failure) ──┘
                      │
P1-3a (UserCorrection事件) ──┐
                              ├── P1-3a 是 P1-3b/c 的前置
P1-3b (dispatcher检测) ──────┤
                              │
P1-3c (context_listener消费) ─┘
                      │
P1-1 (AVOID增补) ─────┤── 依赖 P0-1 streak 数据新鲜度
                      │
P1-2 (self_eval提示)  │── 无依赖
                      │
P2-1 (生命周期审计) ──┤── 依赖 P1-1
                      │
P2-2 (auto-eval兜底) ─┘── 依赖 P1-2 提示增强
```

推荐执行顺序：P0-1 → P0-2 → P1-3a → P1-3b → P1-3c → P1-1 → P1-2 → P2-1 → P2-2

---

## 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 用户连续快速拒绝产生重复 UserCorrection | 低 | content 前缀去重（已实现） |
| gene_candidate_pool 被 tool_failure 淹没 | 中 | 错误去重 + 容量上限 20 |
| effective_streak 回写性能开销 | 低 | 仅 Capsule 生成后批量计算，不在检索热路径 |
| auto-eval 兜底产生低质量评估 | 低 | score 基于客观 outcome，不依赖 LLM 判断 |
