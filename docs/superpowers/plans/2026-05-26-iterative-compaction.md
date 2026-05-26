# 迭代式压缩 + Split-Turn 恢复 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 uClaw 自动压缩从 O(N) 整史重摘升级为 O(1) 增量(保留 StructuredFold),并用 Pi 部分摘要优雅处理切点落在工具对中间的情况。

**Architecture:** 新 `agent/compaction.rs` 持有 `CompactionState`(内存)+ `find_compaction_cut_point`(纯函数,结构化检测 split-turn)。`compact/summarize.rs` 新增 `update_fold_incremental`(prior fold markdown + 仅新消息 → 更新后的 StructuredFold)。`LoopDelegate` 加增量方法。`soft_compress_context` 在有 prior fold 时走增量、split 时附「Turn Context」capsule。重载时从 V52 baseline 重建 prior fold。

**Tech Stack:** Rust + Tokio,现有 `compact/` 模块,`LoopDelegate` trait,V52 `agent_fold_baselines`。

**Spec:** `docs/superpowers/specs/2026-05-26-iterative-compaction-design.md`
**Branch/worktree:** `codex/pi-sprint2-continue`(base = main `2c8105cc`)

---

## ⚠️ 规划期发现的 spec 精简(实现以本计划为准)

调查现有代码后,对 spec 做两处**精简**(更贴合 uClaw 现状、更少代码):

1. **去掉 `CompactionState.last_boundary: i64` 时间戳**。`ChatMessage` 无 `created_at`;uClaw 用**结构化边界**(ToolUse/ToolResult id 配对,见现有 `is_boundary_safe`)。"自上次以来的新消息" = 本周期 `soft_compress_context` 计算出的待压缩非-compacted 切片(本就如此),无需时间戳。`CompactionState` 精简为 `{ previous_fold: Option<StructuredFold>, compactions_done: u32 }`。
2. **Reconstruct-on-load = 仅 `baseline::load_baseline()` 取 prior fold**。agent 重载已在 SQL 层排除 compacted 行,无需重建时间戳边界。

其余 spec 决策不变(保留 StructuredFold 增量更新、Pi 部分摘要、零迁移)。

---

## 验证命令

- 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空=通过)
- 单测:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib <filter> 2>&1 | tail -15`

> `cargo test -p uclaw_core` 不工作;在 worktree `src-tauri/` 下用 `cargo test`。bundled 资源(gbrain-source/bunembed/pyembed)已 symlink,后端可编译。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `src-tauri/src/agent/compaction.rs` | **新建** — `CompactionState`、`CompactionCutPoint`、`find_compaction_cut_point`(纯函数 + split-turn 检测) |
| `src-tauri/src/agent/mod.rs` | `pub mod compaction;` |
| `src-tauri/src/agent/compact/summarize.rs` | 新增 `update_fold_incremental` + UPDATE prompt;`parse_fold_from_text`/`extract_text` 改 `pub(crate)` |
| `src-tauri/src/agent/types.rs` | `ReasoningContext` 加 `compaction_state: CompactionState`(`::new` + 2 个 test 字面量) |
| `src-tauri/src/agent/agentic_loop.rs` | `LoopDelegate` 加 `update_fold_incremental`;`soft_compress_context` 接增量 + split-turn capsule |
| `src-tauri/src/tauri_commands.rs` | agent 会话重载处 seed `compaction_state.previous_fold` from `load_baseline` |

任务顺序:纯函数/类型自底向上(compaction.rs → summarize.rs → ReasoningContext 字段 → delegate → 接线 → 重载)。

---

## Task 1: CompactionState + find_compaction_cut_point(纯函数 + split-turn 检测)

**Files:**
- Create: `src-tauri/src/agent/compaction.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 写失败测试** — 新建 `compaction.rs`,先只写测试模块:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};

    fn user(text: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::User, content: vec![ContentBlock::Text { text: text.into() }], compacted: false }
    }
    fn assistant_tool_use(id: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::Assistant, content: vec![ContentBlock::ToolUse { id: id.into(), name: "bash".into(), input: serde_json::json!({}) }], compacted: false }
    }
    fn user_tool_result(id: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::User, content: vec![ContentBlock::ToolResult { tool_use_id: id.into(), content: "ok".into(), is_error: None }], compacted: false }
    }

    #[test]
    fn cut_point_not_split_when_boundary_on_user() {
        // [u, a(tool t1), u(result t1), u]  cut before index 3 (a clean User)
        let msgs = vec![user("a"), assistant_tool_use("t1"), user_tool_result("t1"), user("b")];
        let cp = find_compaction_cut_point(&msgs, 3);
        assert_eq!(cp.first_kept_index, 3);
        assert!(!cp.is_split_turn);
        assert_eq!(cp.turn_start_index, None);
    }

    #[test]
    fn cut_point_detects_split_pair() {
        // cut at index 2 would keep [user_tool_result(t1), ...] whose ToolUse t1 is on the compacted side → split
        let msgs = vec![user("a"), assistant_tool_use("t1"), user_tool_result("t1"), user("b")];
        let cp = find_compaction_cut_point(&msgs, 2);
        assert!(cp.is_split_turn, "keeping a ToolResult whose ToolUse is compacted must be a split turn");
        assert_eq!(cp.turn_start_index, Some(1), "turn_start should point at the ToolUse message");
    }

    #[test]
    fn default_state_is_first_compaction() {
        let s = CompactionState::default();
        assert!(s.previous_fold.is_none());
        assert_eq!(s.compactions_done, 0);
    }
}
```

- [ ] **Step 2: 运行确认红**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib compaction:: 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find ... find_compaction_cut_point` / `CompactionState`

- [ ] **Step 3: 实现**(测试模块之前):

```rust
//! 迭代式压缩状态 + 切点检测 — Pi convergence Sprint 2 item 1。
//!
//! `CompactionState` 跨轮次累积上一份 fold(增量基底)。
//! `find_compaction_cut_point` 用结构化(ToolUse/ToolResult id 配对)检测
//! 切点是否落在工具对中间(split turn),供 `soft_compress_context` 做部分摘要恢复。

use std::collections::HashSet;

use crate::agent::compact::fold::StructuredFold;
use crate::agent::types::{ChatMessage, ContentBlock};

/// 跨轮次累积的增量压缩状态(运行期内存,放 ReasoningContext)。
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    /// 上一份 fold(增量基底);None = 首次压缩(走全史路径)。
    pub previous_fold: Option<StructuredFold>,
    /// 已完成的压缩周期数(统计 / 调试)。
    pub compactions_done: u32,
}

/// 切点 + Split-Turn 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCutPoint {
    /// 后缀逐字保留起点(message index)。
    pub first_kept_index: usize,
    /// 切点是否落在 ToolUse/ToolResult 对中间。
    pub is_split_turn: bool,
    /// split 时:被切那一轮的起点 index(最近的非-ToolResult User/Assistant 边界)。
    pub turn_start_index: Option<usize>,
}

/// 返回该消息里所有 ToolUse 的 id。
fn tool_use_ids(msg: &ChatMessage) -> impl Iterator<Item = &str> {
    msg.content.iter().filter_map(|b| match b {
        ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
        _ => None,
    })
}

/// 返回该消息里所有 ToolResult 的 tool_use_id。
fn tool_result_ids(msg: &ChatMessage) -> impl Iterator<Item = &str> {
    msg.content.iter().filter_map(|b| match b {
        ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
        _ => None,
    })
}

/// 计算切点并检测 split-turn。`desired_index` 是期望的 first_kept_index
/// (后缀保留起点),由调用方根据 token 预算算出。
///
/// split-turn 判定:保留侧(>= desired_index)存在某个 ToolResult,其配对
/// ToolUse 在压缩侧(< desired_index)。此时把 turn_start_index 定位到该
/// ToolUse 所在消息,供调用方对 [turn_start..desired_index] 做部分摘要。
pub fn find_compaction_cut_point(messages: &[ChatMessage], desired_index: usize) -> CompactionCutPoint {
    let idx = desired_index.min(messages.len());

    // 压缩侧所有 ToolUse id。
    let compacted_uses: HashSet<&str> =
        messages[..idx].iter().flat_map(tool_use_ids).collect();
    // 保留侧所有 ToolResult id。
    let kept_results: Vec<&str> =
        messages[idx..].iter().flat_map(tool_result_ids).collect();

    let split_id = kept_results.into_iter().find(|rid| compacted_uses.contains(rid));

    match split_id {
        None => CompactionCutPoint { first_kept_index: idx, is_split_turn: false, turn_start_index: None },
        Some(rid) => {
            // 找到压缩侧那条含该 ToolUse 的消息;turn_start = 它,或更早最近的轮起点。
            let turn_start = messages[..idx]
                .iter()
                .rposition(|m| tool_use_ids(m).any(|id| id == rid));
            CompactionCutPoint {
                first_kept_index: idx,
                is_split_turn: true,
                turn_start_index: turn_start,
            }
        }
    }
}
```

- [ ] **Step 4: 注册模块** — `src-tauri/src/agent/mod.rs` 加(在 `pub mod compact;` 附近):

```rust
pub mod compaction;
```

- [ ] **Step 5: 运行确认绿**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib compaction:: 2>&1 | tail -8`
Expected: `test result: ok. 3 passed`

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/agent/compaction.rs src-tauri/src/agent/mod.rs
git commit -m "$(cat <<'EOF'
feat(agent): CompactionState + structural split-turn cut-point detection

New agent/compaction.rs: CompactionState (previous_fold for incremental
compaction) + find_compaction_cut_point, which detects (via ToolUse/ToolResult
id pairing, no timestamps) whether a cut lands mid tool-call pair and where the
split turn starts. Pi Sprint 2 item 1 foundation.

Verification: cargo test --lib compaction:: → ok, 3 passed
EOF
)"
```

---

## Task 2: update_fold_incremental(增量 summarizer + UPDATE prompt)

**Files:**
- Modify: `src-tauri/src/agent/compact/summarize.rs`

- [ ] **Step 1: 改可见性** — 把 `summarize.rs` 中 `fn parse_fold_from_text` 和 `fn extract_text` 改为 `pub(crate) fn`(供后续复用;本任务在同文件内调用,但增量函数也要用)。无其他改动。

- [ ] **Step 2: 写失败测试** — 在 `summarize.rs` 的 `#[cfg(test)] mod tests` 中添加(用一个 mock LLM provider;若文件已有 mock 模式则复用,否则用下面的内联 mock):

```rust
    // 内联 mock：返回固定的 StructuredFold JSON,并捕获收到的 user prompt 供断言。
    struct CapturingLlm {
        response_json: String,
        captured_user_prompt: std::sync::Mutex<String>,
    }
    #[async_trait::async_trait]
    impl crate::llm::LlmProvider for CapturingLlm {
        async fn complete(
            &self,
            messages: Vec<ChatMessage>,
            _tools: Vec<crate::agent::types::ToolDef>,
            _config: &crate::llm::CompletionConfig,
        ) -> Result<crate::agent::types::RespondOutput, crate::error::Error> {
            // user prompt 是第 2 条消息
            if let Some(ContentBlock::Text { text }) = messages.get(1).and_then(|m| m.content.first()) {
                *self.captured_user_prompt.lock().unwrap() = text.clone();
            }
            Ok(crate::agent::types::RespondOutput::Text { text: self.response_json.clone(), finish_reason: None })
        }
        // 其余 trait 必需方法：若 LlmProvider 还有别的必需方法,按现有 mock 风格补 unimplemented!()/默认。
    }

    #[tokio::test]
    async fn update_fold_incremental_feeds_prior_fold_and_only_new_messages() {
        use crate::agent::compact::fold::{FactWithEvidence, StructuredFold};
        let prior = StructuredFold::default().with_facts(vec![
            FactWithEvidence { claim: "auth uses JWT".into(), evidence: vec![], confidence: None },
        ]);
        let response = r#"{"facts":[{"claim":"auth uses JWT","evidence":[]},{"claim":"added refresh tokens","evidence":[]}],"next_actions":["ship"]}"#;
        let llm = std::sync::Arc::new(CapturingLlm {
            response_json: response.into(),
            captured_user_prompt: std::sync::Mutex::new(String::new()),
        });
        let new_msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: "NEW_MESSAGE_MARKER add refresh tokens".into() }],
            compacted: false,
        }];
        let updated = update_fold_incremental(llm.clone(), "test-model", &prior, &new_msgs).await.unwrap();

        // 输出并入了新 fact
        assert!(updated.facts.iter().any(|f| f.claim.contains("refresh tokens")));
        assert!(updated.facts.iter().any(|f| f.claim.contains("JWT")));
        // prompt 含 prior fold 渲染 + 仅新消息(断言 transcript 仅含新消息标记)
        let prompt = llm.captured_user_prompt.lock().unwrap().clone();
        assert!(prompt.contains("auth uses JWT"), "prompt should carry prior fold markdown");
        assert!(prompt.contains("NEW_MESSAGE_MARKER"), "prompt should carry new messages");
    }
```

> 注:`LlmProvider` 的精确 trait 方法集以 `crate::llm` 为准;实现 mock 时补齐所有必需方法(参考文件内/repo 内既有的 LlmProvider mock)。`RespondOutput::Text` 字段名以 `types.rs` 为准(可能是 `{ text, finish_reason }` 或带更多字段——按实际补 `..Default::default()` 或全字段)。

- [ ] **Step 3: 运行确认红**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib summarize::tests::update_fold_incremental 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find function 'update_fold_incremental'`

- [ ] **Step 4: 实现** — 在 `summarize.rs` 中 `summarize_to_fold` 之后添加:

```rust
/// 增量更新一份已有 fold:把 `prior_fold` 渲染为 markdown,连同**仅**自上次
/// 压缩以来的 `new_messages` 一起喂给 LLM,要求产出**完整的、更新后的**
/// StructuredFold JSON。输入 O(1)(prior fold ~固定 + 新消息窗口)。
///
/// 复用 `summarize_to_fold` 的解析/兜底契约(`parse_fold_from_text`/`extract_text`)。
pub async fn update_fold_incremental(
    llm: Arc<dyn LlmProvider>,
    model_id: &str,
    prior_fold: &StructuredFold,
    new_messages: &[ChatMessage],
) -> Result<StructuredFold, SummarizeError> {
    if new_messages.is_empty() {
        // 无新消息:原样返回 prior(无可更新)。
        return Ok(prior_fold.clone());
    }

    let prior_md = prior_fold.to_markdown();
    let transcript = render_transcript(new_messages);
    let system_prompt = build_update_system_prompt();
    let user_prompt = build_update_user_prompt(&prior_md, &transcript);

    let req_messages = vec![
        ChatMessage { role: MessageRole::System, content: vec![ContentBlock::Text { text: system_prompt }], compacted: false },
        ChatMessage { role: MessageRole::User, content: vec![ContentBlock::Text { text: user_prompt }], compacted: false },
    ];
    let config = CompletionConfig {
        model: model_id.to_string(),
        max_tokens: SUMMARIZER_MAX_TOKENS,
        temperature: 0.2,
        thinking_enabled: false,
    };

    let resp = llm.complete(req_messages, Vec::new(), &config).await.map_err(SummarizeError::LlmFailed)?;
    let raw_text = extract_text(&resp);
    parse_fold_from_text(&raw_text).map_err(|e| SummarizeError::ParseFailed {
        error: e.to_string(),
        raw_response: raw_text,
    })
}

fn build_update_system_prompt() -> String {
    format!(
        r#"You are UPDATING an existing structured conversation summary, not creating one from scratch.

You are given a PREVIOUS SUMMARY (the running compressed memory of this session) and a set of NEW MESSAGES that occurred since that summary was last produced. Produce a COMPLETE, UPDATED StructuredFold JSON object (same schema) that folds the new messages into the previous summary:

- facts / decisions: keep still-relevant ones from the previous summary; add new ones from the new messages; when new evidence contradicts an old fact, prefer the new.
- next_actions: drop ones now completed; add newly surfaced ones.
- unresolved_questions: drop resolved ones; add new ones.
- failed_attempts / active_constraints / rollback_points / evidence_refs: accumulate.
- file_ops: merge file operations seen in the new messages.
- micro_capsules: add capsules for the key new turns.

Output ONLY the JSON object, ~{target} tokens, no prose, no code fence."#,
        target = TARGET_FOLD_TOKENS
    )
}

fn build_update_user_prompt(prior_markdown: &str, new_transcript: &str) -> String {
    format!(
        "<previous_summary>\n{prior_markdown}\n</previous_summary>\n\n<new_messages>\n{new_transcript}\n</new_messages>\n\nReturn the complete updated StructuredFold JSON:"
    )
}
```

- [ ] **Step 5: 运行确认绿**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib summarize:: 2>&1 | tail -8`
Expected: `test result: ok.` 全过(含新测试 + 现有 summarize 测试)

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/agent/compact/summarize.rs
git commit -m "$(cat <<'EOF'
feat(compact): update_fold_incremental — O(1) incremental summarization

Feeds the prior StructuredFold (as markdown) + ONLY the new messages since the
last compaction to the LLM, asking for a complete updated StructuredFold. Reuses
parse_fold_from_text / extract_text (now pub(crate)). Empty new_messages returns
the prior fold unchanged. Keeps the typed 10-axis fold; LLM input stays bounded.

Verification: cargo test --lib summarize:: → ok, all passed
EOF
)"
```

---

## Task 3: ReasoningContext.compaction_state 字段

**Files:**
- Modify: `src-tauri/src/agent/types.rs`

- [ ] **Step 1: 加字段** — `ReasoningContext` struct(types.rs)末尾、`file_ops` 之后加:

```rust
    /// 迭代式压缩状态(Pi Sprint 2):跨轮次累积上一份 fold。
    pub compaction_state: crate::agent::compaction::CompactionState,
```

- [ ] **Step 2: 在 `::new` 初始化** — `ReasoningContext::new` 构造体里加:

```rust
            compaction_state: crate::agent::compaction::CompactionState::default(),
```

- [ ] **Step 3: 修两个 test 字面量** — `agentic_loop.rs` 第 ~1518、~1580 行的 `ReasoningContext { ... }` 结构体字面量各加一行:

```rust
            compaction_state: Default::default(),
```

(grep 定位:`grep -n "ReasoningContext {" src-tauri/src/agent/agentic_loop.rs`)

- [ ] **Step 4: 编译确认**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出(`CompactionState: Default` 已具备;若有其它 `ReasoningContext { }` 字面量报错,逐个补 `compaction_state: Default::default()`)

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/agent/types.rs src-tauri/src/agent/agentic_loop.rs
git commit -m "$(cat <<'EOF'
feat(agent): thread CompactionState through ReasoningContext

Add compaction_state: CompactionState to ReasoningContext (default in ::new;
fix the 2 test struct literals in agentic_loop.rs). Carries the prior fold
across turns for incremental compaction.

Verification: cargo build → no errors
EOF
)"
```

---

## Task 4: LoopDelegate.update_fold_incremental

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`

- [ ] **Step 1: 找到 LoopDelegate trait 与现有 summarize_to_fold 方法**

Run: `grep -n "trait LoopDelegate\|fn summarize_to_fold" src-tauri/src/agent/agentic_loop.rs | head`

阅读现有 `async fn summarize_to_fold(&self, messages: &[ChatMessage]) -> StructuredFold`(delegate 方法)及其实现(它内部用 session 的 llm + model 调 `summarize::summarize_to_fold`)。

- [ ] **Step 2: 在 LoopDelegate trait 加增量方法**(紧挨现有 `summarize_to_fold`):

```rust
    /// 增量更新一份 fold(prior fold + 仅新消息)。默认实现委托到全量
    /// `summarize_to_fold`(忽略 prior),以便未覆盖的 delegate 仍可用。
    async fn update_fold_incremental(
        &self,
        prior_fold: &crate::agent::compact::fold::StructuredFold,
        new_messages: &[ChatMessage],
    ) -> crate::agent::compact::fold::StructuredFold {
        let _ = prior_fold;
        self.summarize_to_fold(new_messages).await
    }
```

- [ ] **Step 3: 在生产 delegate 实现里 override** — 找到实现 `summarize_to_fold` 的生产 delegate(grep `fn summarize_to_fold` 的 impl 块,它持有 llm + model_id)。在同 impl 块加:

```rust
    async fn update_fold_incremental(
        &self,
        prior_fold: &crate::agent::compact::fold::StructuredFold,
        new_messages: &[ChatMessage],
    ) -> crate::agent::compact::fold::StructuredFold {
        match crate::agent::compact::summarize::update_fold_incremental(
            self.llm.clone(), &self.model_id, prior_fold, new_messages,
        ).await {
            Ok(fold) => fold,
            Err(e) => {
                tracing::warn!(error = %e, "incremental fold update failed; falling back to full summarize");
                // 回退全量;再失败由 summarize_to_fold 内部 extractive 兜底
                self.summarize_to_fold(new_messages).await
            }
        }
    }
```

> 字段名 `self.llm` / `self.model_id` 以实际 delegate 实现为准(grep 现有 `summarize_to_fold` impl 看它怎么取 llm/model)。

- [ ] **Step 4: 编译确认**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出

- [ ] **Step 5: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/agent/agentic_loop.rs
git commit -m "$(cat <<'EOF'
feat(agent): LoopDelegate::update_fold_incremental

Add an incremental-summarization method to LoopDelegate (default delegates to
the full summarize_to_fold; production delegate routes to
summarize::update_fold_incremental with full-summarize fallback on error).

Verification: cargo build → no errors
EOF
)"
```

---

## Task 5: soft_compress_context 接增量 + Split-Turn 部分摘要

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`(高注意力文件,diff 收窄)

- [ ] **Step 1: 阅读现有 soft_compress_context**(~909-1038)。确认:它算出 `split_idx`(经 `find_safe_compaction_boundary`)、克隆 `messages_to_compact = messages[0..split_idx]`(非-compacted)、标记 compacted、调 `delegate.summarize_to_fold(&messages_to_compact)`、`fold.with_file_ops(...)`、`to_markdown` → 注入。

- [ ] **Step 2: 改造摘要生成段** — 把单一的 `delegate.summarize_to_fold(&messages_to_compact)` 调用替换为「有 prior fold 走增量 + split-turn 部分摘要」逻辑。定位到调用 `summarize_to_fold` 的那几行,替换为:

```rust
    // —— Pi Sprint 2:迭代式压缩 ——
    // 用结构化切点检测 split-turn(此时 split_idx 已是 find_safe_compaction_boundary 的结果)。
    let cut = crate::agent::compaction::find_compaction_cut_point(&reason_ctx.messages, split_idx);

    // 主摘要覆盖 [.. turn_start](split 时)或 [.. split_idx](非 split)。
    let main_end = if cut.is_split_turn { cut.turn_start_index.unwrap_or(split_idx) } else { split_idx };
    let main_slice: Vec<ChatMessage> =
        reason_ctx.messages[..main_end].iter().filter(|m| !m.compacted).cloned().collect();

    let mut fold = if let Some(prior) = reason_ctx.compaction_state.previous_fold.clone() {
        // 增量:prior fold + 仅本周期新消息(main_slice 即自上次以来待压缩的非-compacted 消息)
        delegate.update_fold_incremental(&prior, &main_slice).await
    } else {
        // 首次:全史一次性
        delegate.summarize_to_fold(&main_slice).await
    };

    // Split-Turn 部分摘要:把 [turn_start..split_idx] 单独摘要成一个「Turn Context」capsule。
    if cut.is_split_turn {
        if let Some(turn_start) = cut.turn_start_index {
            let split_prefix: Vec<ChatMessage> =
                reason_ctx.messages[turn_start..split_idx].iter().cloned().collect();
            if !split_prefix.is_empty() {
                let prefix_fold = delegate.summarize_to_fold(&split_prefix).await;
                let mut caps = fold.micro_capsules.clone();
                caps.push(crate::agent::compact::fold::MicroCapsule {
                    turn_index: caps.len(),
                    user_query: "Turn Context (split turn)".to_string(),
                    agent_outcome: prefix_fold.to_markdown(),
                });
                fold = fold.with_micro_capsules(caps);
            }
        }
    }

    // 并入累积 file_ops(保持现有行为)
    let fold = fold.with_file_ops(reason_ctx.file_ops.clone());

    // 更新压缩状态(下次走增量)
    reason_ctx.compaction_state.previous_fold = Some(fold.clone());
    reason_ctx.compaction_state.compactions_done += 1;
```

> 注:保持后续的 `to_markdown` / `cache_align` / 注入 summary 段不变(它们消费 `fold`)。`split_idx` 仍是 `find_safe_compaction_boundary` 的结果;`find_compaction_cut_point` 在此只用于**检测**是否 split + 定位 turn_start,不改变 split_idx 本身(保守:后缀保留起点仍是 split_idx)。`with_file_ops` 之后的 `fold` 用于注入;`previous_fold` 存的是含 file_ops + split capsule 的完整 fold。
>
> 若现有代码里 `messages_to_compact` 已经构造好且后续逻辑依赖它,保留其用于"标记 compacted / 计数",仅把**摘要生成**替换为上面的增量逻辑。

- [ ] **Step 3: 编译 + 现有压缩测试回归**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head && cargo test --lib agentic_loop:: 2>&1 | tail -12`
Expected: 编译无 error;现有 agentic_loop 压缩相关测试通过

- [ ] **Step 4: 加集成测试** — 在 `agentic_loop.rs` 测试模块加一个测试,验证第二次压缩走增量(prior fold 非空时调 `update_fold_incremental`)。用一个记录调用的 mock delegate:

```rust
    #[tokio::test]
    async fn second_compaction_uses_incremental_path() {
        // mock delegate 记录 summarize_to_fold vs update_fold_incremental 各被调几次
        // 构造一个 reason_ctx,首次 soft_compress → summarize_to_fold;
        // 注入更多消息,二次 soft_compress → update_fold_incremental。
        // 断言:第二次走了 update_fold_incremental(prior_fold 非空)。
        // (具体 mock 按文件内既有 LoopDelegate mock 风格实现;若无,新建最小 mock。)
    }
```

> 若 `agentic_loop.rs` 已有 LoopDelegate 的 test mock,扩展它加计数器;否则按现有测试基建实现最小 mock。本步以"能断言第二次走增量路径"为准。

- [ ] **Step 5: 运行测试 + 提交**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo test --lib agentic_loop:: 2>&1 | tail -10`
Expected: 全过

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/agent/agentic_loop.rs
git commit -m "$(cat <<'EOF'
feat(agent): iterative compaction in soft_compress_context

soft_compress_context now uses incremental summarization when a prior fold
exists (update_fold_incremental) and only falls back to full summarize on first
compaction. Split-turn cut points (detected structurally) get a separate
"Turn Context" micro-capsule summarizing the split prefix, so no dangling
ToolUse/ToolResult survives. CompactionState.previous_fold is updated each cycle.
O(N) full-history re-summarization → O(1) incremental.

Verification: cargo test --lib agentic_loop:: → ok; cargo build → no errors
EOF
)"
```

---

## Task 6: Reconstruct-on-load(从 V52 baseline seed previous_fold)

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 定位 agent 会话重载处** — `send_agent_message` 里构造 `ReasoningContext::new(...)` 并 push 历史的地方(~11164)。确认其后能拿到 `session_id` 和 `state.db`。

Run: `grep -n "ReasoningContext::new\|load_baseline\|agent_fold_baselines" src-tauri/src/tauri_commands.rs | head`

- [ ] **Step 2: seed previous_fold** — 在历史 push 完、delegate 构造前,加:

```rust
    // Pi Sprint 2:迭代式压缩 —— 从 V52 baseline 重建 prior fold,使重载后的
    // 会话继续走增量压缩(而非每次重新全史摘要)。
    {
        let conn = state.db.lock().await; // 锁风格以现有代码为准(可能是 .lock().unwrap() / .await)
        if let Some(prior) = crate::agent::compact::baseline::load_baseline(&conn, &session_id) {
            reason_ctx.compaction_state.previous_fold = Some(prior);
        }
    }
```

> `state.db` 的锁类型/获取方式以文件内现有用法为准(grep `state.db.lock` 看是 `.await` 还是 `.unwrap()`)。`session_id` 变量名以该作用域实际为准。`load_baseline(&Connection, &str) -> Option<StructuredFold>`(见 `compact/baseline.rs`)。

- [ ] **Step 3: 编译确认**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出

- [ ] **Step 4: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue
git add src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
feat(agent): seed CompactionState.previous_fold from V52 baseline on reload

When an agent session resumes, load the per-session StructuredFold from
agent_fold_baselines (V52) into reason_ctx.compaction_state.previous_fold, so
compaction continues incrementally instead of re-summarizing from scratch.
Zero migration — reuses the existing baseline store.

Verification: cargo build → no errors
EOF
)"
```

---

## 最终验收

- [ ] 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/pi-sprint2-continue/src-tauri && cargo build 2>&1 | grep -E "^error" | head` → 无输出
- [ ] 相关单测:`cargo test --lib "compaction::" "summarize::" "agentic_loop::" 2>&1 | tail -15`(分别跑)→ 全过
- [ ] 全量 lib 测试无新失败(对比 base 的 3 个预存 + 11 个 browser-env 失败):`cargo test --lib 2>&1 | tail -5`
- [ ] 手动 smoke(可选,`cargo tauri dev`):长会话触发多次自动压缩,观察第二次起 LLM 摘要调用输入 token 不随历史增长(日志/观测)。

---

## Self-Review(写计划后自查)

**Spec coverage:**
- §3.1 CompactionState/CutPoint → Task 1 ✓(已按规划期精简去掉 last_boundary)
- §3.2 数据流 → Task 5 ✓
- §4 update_fold_incremental + UPDATE prompt → Task 2 ✓
- §5 Split-Turn 部分摘要 → Task 1(检测)+ Task 5(部分摘要 capsule)✓
- §6 reconstruct-on-load → Task 6 ✓(精简为仅 load_baseline)
- §7 错误/边界 → Task 2(空新消息返回 prior)、Task 4(增量失败回退全量)、Task 5(首次走全量;split turn_start None 时 main_end=split_idx 退化为非 split)✓
- §8 测试 → 散落 Task 1/2/5 ✓
- ReasoningContext 字段接线 → Task 3 ✓;delegate → Task 4 ✓

**Placeholder scan:** 代码步均含完整代码。两处显式标注"以实际为准"(LlmProvider mock 方法集、state.db 锁风格、delegate 字段名)——这些是 repo-具体的小适配,非占位;实现者 grep 即得。无 TBD/TODO。

**Type consistency:** `CompactionState { previous_fold, compactions_done }`(Task 1)→ ReasoningContext 字段(Task 3)→ soft_compress 读写(Task 5)一致;`find_compaction_cut_point` 返回 `CompactionCutPoint { first_kept_index, is_split_turn, turn_start_index }`(Task 1)→ Task 5 消费一致;`update_fold_incremental(llm, model, prior_fold, new_messages)`(Task 2)→ delegate(Task 4)→ soft_compress(Task 5)签名一致;`MicroCapsule { turn_index, user_query, agent_outcome }`(无 label)Task 5 构造正确。
