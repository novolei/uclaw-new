# 阶段 3 P3-5b3 — `agentic_loop` Assistant-Message Helper Consolidation · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate 6 nearly-identical assistant-message assembly sites in `src-tauri/src/agent/agentic_loop.rs` (lines ~153, ~203, ~221, ~243, ~276, ~388) into ONE call: `ChatMessage::assistant_from_response(thinking, thinking_signature, text, tool_uses)`. This addresses the Pi-convergence gap audit's §1.1 MAJOR finding ("`run_turn_body` 有 5 处近乎复制的 `ContentBlock` 拼装, 顺序不变量只靠注释维护") — by encoding the canonical block order **Thinking → Text → ToolUse** in the helper, the invariant becomes type-enforced instead of comment-maintained.

**Architecture:** Add ONE new constructor to `ChatMessage` in the `uclaw_message_types` crate. Each of the 6 callsites in `agentic_loop.rs::run_loop` collapses from ~8 lines (build `Vec<ContentBlock>` + conditional thinking push + text push + optional tool_use loop + `ChatMessage { ... }` literal + push) to ONE line: `reason_ctx.messages.push(ChatMessage::assistant_from_response(...))`. Net ~30 lines saved + 6 hand-maintained order invariants → 1 type-enforced order.

**Tech Stack:** Rust 2021, no new crates.

**Related design:**
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.1 "5 处近乎复制的 ContentBlock 拼装".
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §6.1 (5b3 was originally framed as "5 ContentBlock dup sites → 1 helper in `content_assembler.rs`" — recon corrects: the sites are in `agentic_loop.rs::run_loop`, not in the new `dispatcher/content_assembler.rs` module).
- Prior 阶段 3 PRs: #570 (P3-1), #571 (P3-2), #572 (P3-3), #573 (P3-4), #574 (P3-5a), #575 (P3-5b1), #576 (P3-5b2). Merged to main at `9ce2f95b`.

---

## Recon-discovered facts (verified against `9ce2f95b` main, 2026-05-29)

**Spec interpretation correction:** v1 of the AgentApi handle design said "5 ContentBlock assembly sites need consolidation in `content_assembler.rs`". Recon shows:

1. `agent/dispatcher/content_assembler.rs` does NOT have 5 ContentBlock dup sites — it has `effective_system_prompt` + `build_dynamic_context` which assemble TEXT prompts, not `Vec<ContentBlock>`.
2. The 5 (actually 6) sites are in `agent/agentic_loop.rs::run_loop` (the function the audit called "run_turn_body"), around lines 153, 203, 221, 243, 276, 388.
3. Each site builds a `Vec<ContentBlock>` from `thinking + text + tool_uses` and constructs `ChatMessage { role: Assistant, content: blocks, compacted: false }` inline.
4. The `uclaw_message_types::ChatMessage` impl already has single-block constructors (`system`, `user`, `assistant`, `assistant_with_tool_use`, `user_tool_result`) but NO multi-block constructor for the (thinking, text, N×tool_use) assistant-response shape.

So P3-5b3 targets `agentic_loop.rs` + `uclaw_message_types`, NOT `dispatcher/content_assembler.rs`.

### The 6 dup sites (all in `agent/agentic_loop.rs::run_loop`)

| Approx line | Path / trigger | tool_uses pushed? | Followed by |
|---:|---|---|---|
| 153 | `llm_signals_tool_intent` true → `on_tool_intent_nudge` | No (text only) | `push(ChatMessage::user(TOOL_INTENT_NUDGE))` |
| 203 | `TextAction::Return(...)` | No (text only) | `thread_state = Completed; return Return(outcome)` |
| 221 | `TextAction::Continue` | No (text only) | `return Continue` |
| 243 | `TextAction::ContinueWithNudge(nudge)` | No (text only) | `push(ChatMessage::user(&nudge))` |
| 276 | `TextAction::RescueWithToolCalls(synthetic_calls)` | Yes (from `synthetic_calls`) | `delegate.execute_tool_calls(...)` |
| 388 | Main tool-call dispatch path (`tool_calls` non-empty) | Yes (from `tool_calls`) | `delegate.execute_tool_calls(...)` |

Each site's block-building logic is **identical** (verified by reading lines around each):
```rust
let mut blocks = Vec::new();
if let Some(ref t) = thinking {
    if !t.is_empty() {
        blocks.push(ContentBlock::Thinking { thinking: t.clone(), signature: thinking_signature.clone() });
    }
}
blocks.push(ContentBlock::Text { text: text.clone() });
// (sites 276 + 388 only:)
for tc in &<tool_calls_source> {
    blocks.push(ContentBlock::ToolUse { id: tc.id.clone(), name: tc.name.clone(), input: tc.arguments.clone() });
}
reason_ctx.messages.push(ChatMessage {
    role: MessageRole::Assistant,
    content: blocks,
    compacted: false,
});
```

### Baselines to hold

- `cargo build`: 0 errors, **≤49 warnings** (post-5b2 baseline).
- `cargo test --lib agent::dispatcher`: **50 passed / 0 failed**.
- `cargo test --lib agent::`: **796 passed / 2 pre-existing failed**.
- `cargo test --lib agent::agentic_loop`: capture during Pre-flight (the file has its own test module).
- `cargo test --lib` total: **3,050 passed / 7 pre-existing failed**.
- `cargo test -p uclaw_message_types`: capture during Pre-flight.

### External callers / scope

This refactor touches:
- `crates/uclaw-message-types/src/lib.rs` — add ONE new constructor.
- `src-tauri/src/agent/agentic_loop.rs` — 6 callsite collapses.

NO changes to: `dispatcher/`, `app.rs`, `tauri_commands.rs`, the LLM providers, or any other consumer of `ChatMessage`. The new constructor is additive — existing constructors stay.

---

## Target shape

### New constructor on `ChatMessage`

```rust
impl ChatMessage {
    /// Build an Assistant message from a streaming LLM response's components.
    ///
    /// Encodes the canonical block order **Thinking → Text → ToolUse**.
    /// Replaces 6 nearly-identical inline assemblies in
    /// `agent::agentic_loop::run_loop` (P3-5b3 of the agent framework
    /// Pi-convergence remediation).
    ///
    /// - `thinking`: optional extended-thinking text from the model. Empty
    ///   strings are treated as absent (no Thinking block is emitted).
    /// - `thinking_signature`: optional signature accompanying the Thinking
    ///   block (currently only Anthropic supplies one).
    /// - `text`: the assistant's visible text reply. Always included as a
    ///   single Text block, even if empty.
    /// - `tool_uses`: zero or more `(id, name, input)` tuples for tool calls
    ///   the model emitted. Iterated in the provided order.
    pub fn assistant_from_response(
        thinking: Option<&str>,
        thinking_signature: Option<String>,
        text: &str,
        tool_uses: impl IntoIterator<Item = (String, String, serde_json::Value)>,
    ) -> Self {
        let mut blocks = Vec::new();
        if let Some(t) = thinking {
            if !t.is_empty() {
                blocks.push(ContentBlock::Thinking {
                    thinking: t.to_string(),
                    signature: thinking_signature,
                });
            }
        }
        blocks.push(ContentBlock::Text { text: text.to_string() });
        for (id, name, input) in tool_uses {
            blocks.push(ContentBlock::ToolUse { id, name, input });
        }
        Self {
            role: MessageRole::Assistant,
            content: blocks,
            compacted: false,
        }
    }
}
```

### Callsite shape AFTER

```rust
// Sites WITHOUT tool_uses (4 sites: 153, 203, 221, 243):
reason_ctx.messages.push(ChatMessage::assistant_from_response(
    thinking.as_deref(),
    thinking_signature.clone(),
    &text,
    std::iter::empty(),
));

// Sites WITH tool_uses (2 sites: 276, 388):
reason_ctx.messages.push(ChatMessage::assistant_from_response(
    thinking.as_deref(),
    thinking_signature.clone(),
    &text,
    <tool_calls_source>.iter().map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone())),
));
```

Net delta: ~8 lines per site → ~5 lines per site (or ~1 line if formatted on one line). For 6 sites: roughly **20-30 net LoC reduction** + the order-invariant comment maintenance disappears.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```
   Expected: `## main...origin/main` at `9ce2f95b`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage3-p5b3-content-block-helper \
       /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/bunembed
   ```

3. **Capture baselines:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent::agentic_loop 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper && cargo test -p uclaw-message-types 2>&1 | tail -3
   ```

---

## Task 1: Add `assistant_from_response` constructor to `ChatMessage`

**Files:**
- Modify: `crates/uclaw-message-types/src/lib.rs` (add ONE new method to `impl ChatMessage`)
- Modify: `crates/uclaw-message-types/src/lib.rs` (add a unit test verifying block order: thinking present, thinking empty/None, with N tool_uses)

### Steps

- [ ] **Step 1.1: Locate the `impl ChatMessage` block**

  ```bash
  grep -n "impl ChatMessage" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/crates/uclaw-message-types/src/lib.rs
  ```
  Expected: line ~43.

- [ ] **Step 1.2: Add the new constructor**

  Insert AT THE END of the `impl ChatMessage { ... }` block (after `user_tool_result`):

  ```rust
  /// Build an Assistant message from a streaming LLM response's components.
  ///
  /// Encodes the canonical block order **Thinking → Text → ToolUse**.
  /// Replaces 6 nearly-identical inline assemblies in
  /// `agent::agentic_loop::run_loop` (P3-5b3 of the agent framework
  /// Pi-convergence remediation).
  ///
  /// - `thinking`: optional extended-thinking text. Empty strings are
  ///   treated as absent (no Thinking block is emitted).
  /// - `thinking_signature`: optional signature accompanying Thinking
  ///   (currently only Anthropic supplies one).
  /// - `text`: the assistant's visible text reply. Always included as a
  ///   single Text block, even if empty.
  /// - `tool_uses`: zero or more `(id, name, input)` tuples for tool
  ///   calls the model emitted. Iterated in the provided order.
  pub fn assistant_from_response(
      thinking: Option<&str>,
      thinking_signature: Option<String>,
      text: &str,
      tool_uses: impl IntoIterator<Item = (String, String, serde_json::Value)>,
  ) -> Self {
      let mut blocks = Vec::new();
      if let Some(t) = thinking {
          if !t.is_empty() {
              blocks.push(ContentBlock::Thinking {
                  thinking: t.to_string(),
                  signature: thinking_signature,
              });
          }
      }
      blocks.push(ContentBlock::Text {
          text: text.to_string(),
      });
      for (id, name, input) in tool_uses {
          blocks.push(ContentBlock::ToolUse { id, name, input });
      }
      Self {
          role: MessageRole::Assistant,
          content: blocks,
          compacted: false,
      }
  }
  ```

- [ ] **Step 1.3: Add a unit test verifying block ordering**

  Find the `#[cfg(test)] mod tests { ... }` block at the bottom of `lib.rs` (or create one if it doesn't exist). Add:

  ```rust
  #[test]
  fn assistant_from_response_with_thinking_text_and_tool_uses() {
      let msg = ChatMessage::assistant_from_response(
          Some("reasoning..."),
          Some("sig-abc".to_string()),
          "I'll call two tools.",
          vec![
              ("call_1".to_string(), "bash".to_string(), serde_json::json!({"cmd": "ls"})),
              ("call_2".to_string(), "read_file".to_string(), serde_json::json!({"path": "/tmp/x"})),
          ],
      );

      assert_eq!(msg.role, MessageRole::Assistant);
      assert!(!msg.compacted);
      assert_eq!(msg.content.len(), 4);  // 1 Thinking + 1 Text + 2 ToolUse

      match &msg.content[0] {
          ContentBlock::Thinking { thinking, signature } => {
              assert_eq!(thinking, "reasoning...");
              assert_eq!(signature.as_deref(), Some("sig-abc"));
          }
          other => panic!("expected Thinking, got {:?}", other),
      }
      assert!(matches!(&msg.content[1], ContentBlock::Text { text } if text == "I'll call two tools."));
      assert!(matches!(&msg.content[2], ContentBlock::ToolUse { id, .. } if id == "call_1"));
      assert!(matches!(&msg.content[3], ContentBlock::ToolUse { id, .. } if id == "call_2"));
  }

  #[test]
  fn assistant_from_response_empty_thinking_is_omitted() {
      let msg = ChatMessage::assistant_from_response(
          Some(""),  // empty thinking → no Thinking block
          None,
          "just text",
          std::iter::empty(),
      );
      assert_eq!(msg.content.len(), 1);  // only Text
      assert!(matches!(&msg.content[0], ContentBlock::Text { .. }));
  }

  #[test]
  fn assistant_from_response_no_thinking_no_tools() {
      let msg = ChatMessage::assistant_from_response(None, None, "hi", std::iter::empty());
      assert_eq!(msg.content.len(), 1);
      assert!(matches!(&msg.content[0], ContentBlock::Text { .. }));
  }
  ```

  If the existing test module's `use` block doesn't include `MessageRole`, `ContentBlock`, `ChatMessage`, add them (or use `super::*`).

- [ ] **Step 1.4: Build + test (helper-only)**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper && cargo test -p uclaw-message-types 2>&1 | tail -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: +3 tests in `uclaw-message-types`, all pass. Cargo build clean. agent:: still 796/2.

  No callsites consume the new helper yet — they'll come in Tasks 2-5. A `dead_code` warning on `assistant_from_response` is acceptable at this stage but should disappear after Task 2.

- [ ] **Step 1.5: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A crates/uclaw-message-types/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "feat(message-types): add ChatMessage::assistant_from_response constructor (P3-5b3.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Replace 2 sites in `TextAction::ToolIntentNudge` + `TextAction::Return` paths

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs` (sites at lines ~153 and ~203)

### Steps

- [ ] **Step 2.1: Locate site 1 (around line 153)**

  ```bash
  grep -n "Tool intent nudge\|on_tool_intent_nudge\|TOOL_INTENT_NUDGE" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -3
  ```
  Read the surrounding 30 lines to confirm the assembly pattern.

- [ ] **Step 2.2: Replace site 1**

  Replace the block:
  ```rust
  let mut blocks = Vec::new();
  if let Some(ref t) = thinking {
      if !t.is_empty() {
          blocks.push(ContentBlock::Thinking { thinking: t.clone(), signature: thinking_signature.clone() });
      }
  }
  blocks.push(ContentBlock::Text { text: text.clone() });
  reason_ctx.messages.push(ChatMessage {
      role: MessageRole::Assistant,
      content: blocks,
      compacted: false,
  });
  ```
  with:
  ```rust
  reason_ctx.messages.push(ChatMessage::assistant_from_response(
      thinking.as_deref(),
      thinking_signature.clone(),
      &text,
      std::iter::empty(),
  ));
  ```

  Watch the variable types: `thinking` may be `Option<String>` or `Option<&String>`; `.as_deref()` converts to `Option<&str>`. `thinking_signature` may be `Option<String>` — `.clone()` is fine. `text` is `String` (passed as `&text` → `&str`).

- [ ] **Step 2.3: Locate site 2 (around line 203 — `TextAction::Return`)**

  ```bash
  grep -n "TextAction::Return\|ThreadState::Completed" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -3
  ```
  Same assembly pattern; same replacement.

- [ ] **Step 2.4: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent::agentic_loop 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: 0 errors, ≤49 warnings, agent::agentic_loop pass count unchanged, agent:: 796/2.

- [ ] **Step 2.5: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A src-tauri/src/agent/agentic_loop.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "refactor(agent): consolidate 2 assistant-message sites (intent-nudge + return) → assistant_from_response (P3-5b3.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: Replace 2 sites in `TextAction::Continue` + `TextAction::ContinueWithNudge` paths

Same pattern as Task 2, for sites at ~lines 221 and ~243.

### Steps

- [ ] **Step 3.1: Locate sites**

  ```bash
  grep -n "TextAction::Continue\b\|TextAction::ContinueWithNudge" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -3
  ```

- [ ] **Step 3.2: Replace both sites**

  Same replacement shape as Task 2 (text-only, no tool_uses). The `ContinueWithNudge` arm has a follow-up `reason_ctx.messages.push(ChatMessage::user(&nudge));` that STAYS — only the assistant-message assembly collapses.

- [ ] **Step 3.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

- [ ] **Step 3.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A src-tauri/src/agent/agentic_loop.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "refactor(agent): consolidate 2 assistant-message sites (continue + nudge) → assistant_from_response (P3-5b3.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Replace `TextAction::RescueWithToolCalls` site (with synthetic tool_uses)

Site at ~line 276. This site DOES push tool_uses (from `synthetic_calls`).

### Steps

- [ ] **Step 4.1: Locate site**

  ```bash
  grep -n "TextAction::RescueWithToolCalls\|synthetic_calls" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -5
  ```

- [ ] **Step 4.2: Replace site**

  Replace the inline assembly with:
  ```rust
  reason_ctx.messages.push(ChatMessage::assistant_from_response(
      thinking.as_deref(),
      thinking_signature.clone(),
      &text,
      synthetic_calls.iter().map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone())),
  ));
  ```

  Note the `tool_uses` argument is an iterator over `(String, String, serde_json::Value)`.

- [ ] **Step 4.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

- [ ] **Step 4.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A src-tauri/src/agent/agentic_loop.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "refactor(agent): consolidate rescue-tool-calls assistant-message site → assistant_from_response (P3-5b3.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Replace main tool-call dispatch site (final dup site)

Site at ~line 388 — the main loop body's tool-call dispatch path. Builds the assistant message from text + the LLM's actual `tool_calls` list.

### Steps

- [ ] **Step 5.1: Locate site**

  ```bash
  grep -n "let assistant_msg = ChatMessage\|let mut blocks = " /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -10
  ```

  Find the LAST remaining dup site — the one in the main tool-dispatch path that does NOT route through a `TextAction::*` arm.

- [ ] **Step 5.2: Replace site**

  Same replacement shape as Task 4, but the tool_uses source is `tool_calls` (the actual `Vec<ToolCall>` parameter passed to the function).

  ```rust
  reason_ctx.messages.push(ChatMessage::assistant_from_response(
      thinking.as_deref(),
      thinking_signature.clone(),
      &text,
      tool_calls.iter().map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone())),
  ));
  ```

  If the site has a `let assistant_msg = ...` binding before the push, the assignment can stay or be inlined — the implementer's choice. Inlining is preferred to match the other 5 sites.

- [ ] **Step 5.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

- [ ] **Step 5.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A src-tauri/src/agent/agentic_loop.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "refactor(agent): consolidate main tool-dispatch assistant-message site → assistant_from_response (P3-5b3.5 of 阶段 3)"
  ```

Continue to Task 6.

---

## Task 6: Final audit + cleanup

### Steps

- [ ] **Step 6.1: Confirm no remaining inline assistant-message assembly**

  ```bash
  grep -nE "role:\s*MessageRole::Assistant," /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -10
  ```

  Expected: only test-code matches (inside `#[cfg(test)]` modules at the bottom of the file). No production inline `ChatMessage { role: Assistant, ... }` literals.

  If any production site remains, it's a missed dup — flag it.

- [ ] **Step 6.2: Confirm helper is referenced N times**

  ```bash
  grep -nc "ChatMessage::assistant_from_response" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs
  ```
  Expected: 6 (all production callsites).

- [ ] **Step 6.3: Full test battery**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent::agentic_loop 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri && cargo test --lib 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper && cargo test -p uclaw-message-types 2>&1 | tail -5
  ```

  Required:
  - 0 errors.
  - Warnings ≤49 (same as post-5b2 baseline).
  - agent::agentic_loop tests pass at the same count as Pre-flight.
  - agent:: 796/2.
  - cargo test --lib total: 3050+ / 7 pre-existing failed (3 new tests from Task 1 may bring total to 3053).
  - uclaw-message-types: 3 new tests pass.

- [ ] **Step 6.4: Clean unused imports**

  After collapsing the 6 sites, `agentic_loop.rs` may no longer need direct imports of `MessageRole` (if it was used only by the inline `MessageRole::Assistant`). Check:

  ```bash
  grep -nE "^use .*MessageRole|^use .*ContentBlock" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper/src-tauri/src/agent/agentic_loop.rs | head -5
  ```

  If `MessageRole` or `ContentBlock` are no longer referenced in production code (only in tests), remove from the production `use` block. Re-add inside the `#[cfg(test)] mod tests { use super::*; ... }` if tests need them.

  If `cargo build` doesn't warn, the imports are still used elsewhere — skip.

- [ ] **Step 6.5: Commit cleanup (if any)**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper add -A src-tauri/src/agent/agentic_loop.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper commit -m "refactor(agent): clean unused imports after P3-5b3 (P3-5b3.6)"
  ```

- [ ] **Step 6.6: Verify final chain**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper log --oneline main..HEAD
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b3-content-block-helper status -sb
  ```

  Expected: 5-6 commits ahead of main (Tasks 1-5 mandatory + optional Task 6 cleanup). Working tree clean.

---

## Self-Review

**1. Spec coverage:**
- ✅ Audit §1.1 MAJOR "5 处近乎复制的 ContentBlock 拼装" — addressed via `assistant_from_response` constructor.
- ✅ "顺序不变量只靠注释维护" — now type-enforced (the constructor's body encodes the order; no comments needed).
- 🟡 The audit cited "5 sites" but recon found 6. Plan addresses all 6.

**2. Placeholder scan:**
- All code blocks concrete. No TODOs / TBDs / `unimplemented!()` in shipped code.

**3. Type consistency:**
- `assistant_from_response(thinking: Option<&str>, thinking_signature: Option<String>, text: &str, tool_uses: impl IntoIterator<...>)` — naming + types consistent across plan + helper definition + all 6 callsite templates.
- Iterator item type: `(String, String, serde_json::Value)` — matches `ContentBlock::ToolUse { id, name, input }` field shape exactly.

**4. Bisectability:**
- One logical change per commit:
  - Task 1: add helper + tests (no callsite changes — helper alone passes its own tests)
  - Task 2: 2 simplest sites (text-only, no nudge)
  - Task 3: 2 nudge-adjacent sites
  - Task 4: 1 synthetic-tool-calls site
  - Task 5: 1 main tool-dispatch site
  - Task 6: optional cleanup
- Each commit's `cargo test --lib agent::` must pass.

---

## Cumulative summary

- **Tasks:** 6 (5 mandatory + 1 optional cleanup).
- **Estimated time:** 0.5-1 person-day. Pure mechanical replacement after the helper lands.
- **Risk:** Very low. Pure API consolidation; helper encodes the order invariant.
- **Total commits:** 5-6.

After 5b3 ships: the gap-audit's §1.1 MAJOR "5 ContentBlock dup sites" is fully addressed. With 5a/5b1/5b2/5b3 done, the only remaining 阶段-3 work is **P3-6** (`effective_system_prompt → assemble_system_prompt(SystemPromptContext)` single seam + golden snapshots) which closes the stage.
