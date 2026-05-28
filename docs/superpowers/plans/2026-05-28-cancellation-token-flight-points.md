# Cancellation at Flight Points (Slice 1a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an in-flight LLM stream and an in-flight tool execution observe the agent's `CancellationToken` so a cancel signal aborts them within ~100ms instead of only being noticed after they complete.

**Architecture:** The token already lives in `ReasoningContext.cancellation_token`, and `run_turn_body` already converts `is_cancelled()` into `LoopOutcome::Cancelled` at two checkpoints (`agentic_loop.rs:89` after the LLM call, `:502` after tool execution). The only gap is that the two awaits *block* until completion. We thread the token into the two free/struct boundaries (`stream_completion` and `ToolDispatchContext`) and wrap each await in a `tokio::select!` so a fired token returns early — the existing checkpoints then produce `Cancelled`. **No `LoopDelegate` trait signature change is needed** (the deferred-to-M1-T2e comment at `types.rs:88-94` is now obsolete because the token rides in `reason_ctx`).

**Tech Stack:** Rust, `tokio`, `tokio_util::sync::CancellationToken` (already a dependency, used in `regular_task.rs`/`runtime/task.rs`), `tokio::select!`, `futures::stream`.

**Background facts verified against current code (2026-05-28):**
- `ReasoningContext.cancellation_token: Option<CancellationToken>` — `types.rs:100`; `is_cancelled()` — `types.rs:141`.
- `stream_completion(...)` — `llm_stream.rs:66`; chunk-wait — `llm_stream.rs:97-101`; end-of-stream finalize that builds the partial `RespondOutput` — `llm_stream.rs:297-319`.
- `ChatDelegate::call_llm` calls `stream_completion` — `dispatcher.rs:2214`; `HeadlessDelegate` also calls it — `headless.rs:199`.
- `ToolDispatchContext` struct — `tool_dispatch/mod.rs:36`; `ToolDispatcher::dispatch` — `:105`; prod ctx literal — `dispatcher.rs:2524`; test ctx literal `ctx()` — `tool_dispatch/mod.rs:855`.
- `ToolError::kinded(ToolErrorKind, msg)` — `tools/tool.rs:108`; `ToolErrorKind::Other` — `tools/tool.rs` enum; `ToolDispatchOutcome` fields — `tool_dispatch/mod.rs:46-61`.

**Pre-flight (before Task 1):** The line numbers above are a 2026-05-28 snapshot. Re-verify each with `grep`/Read before editing; if the GitNexus index is fresh, run `gitnexus_impact({target: "stream_completion", direction: "upstream"})` and `gitnexus_impact({target: "dispatch", direction: "upstream"})` and report blast radius. Expected callers: `stream_completion` → 2 prod + 3 test; `ToolDispatchContext` literal → 1 prod + 1 test helper. The compiler enforces completeness because we add non-`Default` struct fields.

---

## File Structure

- `src-tauri/src/agent/llm_stream.rs` — `stream_completion` gains a `cancel: Option<&CancellationToken>` param and a select! around the chunk wait. Owns LLM-stream cancellation. (Task 1)
- `src-tauri/src/agent/dispatcher.rs` — `ChatDelegate::call_llm` passes the token into `stream_completion`; `execute_tool_calls` sets `ToolDispatchContext.cancel`. Owns prod wiring. (Tasks 2, 3)
- `src-tauri/src/agent/headless.rs` — `HeadlessDelegate::call_llm` passes `None` (automation keeps its DB-poll cancellation; out of scope here). (Task 2)
- `src-tauri/src/agent/tool_dispatch/mod.rs` — `ToolDispatchContext` gains `cancel`; `dispatch` short-circuits / aborts on cancel via select!; new `cancelled_outcome` helper. Owns tool-exec cancellation. (Task 3)
- `src-tauri/src/agent/agentic_loop.rs` — new loop-contract test only (no production change). (Task 4)

---

## Task 1: `stream_completion` aborts the in-flight chunk wait on cancel

**Files:**
- Modify: `src-tauri/src/agent/llm_stream.rs:66` (signature), `:97-101` (chunk wait), `:447,:466,:483` (3 test callers)
- Test: `src-tauri/src/agent/llm_stream.rs` (tests module, after `text_response_assembles_full_text_and_emits_deltas`)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `llm_stream.rs`:

```rust
#[tokio::test]
async fn cancellation_token_aborts_in_flight_stream() {
    // A provider whose stream never yields — without cancellation,
    // stream_completion would block on the idle timeout (90s).
    struct PendingProvider;
    #[async_trait]
    impl LlmProvider for PendingProvider {
        async fn complete(
            &self,
            _: Vec<ChatMessage>,
            _: Vec<ToolDefinition>,
            _: &CompletionConfig,
        ) -> Result<RespondOutput, Error> {
            unimplemented!()
        }
        async fn stream(
            &self,
            _: Vec<ChatMessage>,
            _: Vec<ToolDefinition>,
            _: &CompletionConfig,
        ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error> {
            Ok(Box::new(stream::pending()))
        }
    }

    let token = tokio_util::sync::CancellationToken::new();
    token.cancel(); // pre-fired: the cancel arm wins immediately under `biased`
    let sink = RecordingSink::default();

    let out = tokio::time::timeout(
        Duration::from_millis(500),
        stream_completion(
            &PendingProvider,
            vec![],
            vec![],
            &cfg(),
            &sink,
            Duration::from_secs(90),
            Some(&token),
        ),
    )
    .await
    .expect("stream_completion did not return within 500ms after cancel");

    // On cancel we `break` to the finalize path → Ok partial (empty) text.
    assert!(matches!(out, Ok(RespondOutput::Text { .. })));
}
```

- [ ] **Step 2: Run test to verify it fails (compile error)**

Run: `cd src-tauri && cargo test --lib agent::llm_stream::tests::cancellation_token_aborts_in_flight_stream 2>&1 | tail -20`
Expected: FAIL — compile error, `stream_completion` takes 6 args not 7 (`this function takes 6 arguments but 7 arguments were supplied`).

- [ ] **Step 3: Add the import and change the signature**

At the top of `llm_stream.rs`, add to the imports (near the other `use` lines):

```rust
use tokio_util::sync::CancellationToken;
```

Change the signature at `llm_stream.rs:66` from:

```rust
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    config: &CompletionConfig,
    sink: &dyn StreamSink,
    stream_idle_timeout: Duration,
) -> Result<RespondOutput, Error> {
```

to:

```rust
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    config: &CompletionConfig,
    sink: &dyn StreamSink,
    stream_idle_timeout: Duration,
    cancel: Option<&CancellationToken>,
) -> Result<RespondOutput, Error> {
```

- [ ] **Step 4: Wrap the chunk wait in a cancellation-aware select**

Replace the chunk wait at `llm_stream.rs:97-101`:

```rust
                    let next_result = tokio::time::timeout(
                        stream_idle_timeout,
                        stream.next(),
                    )
                    .await;
```

with:

```rust
                    // M1-T2e — observe the cancellation token mid-stream. A
                    // fired token breaks the chunk loop, falling through to the
                    // finalize path below (returns the partial RespondOutput).
                    // run_turn_body's post-call is_cancelled() check
                    // (agentic_loop.rs:89) then converts this to
                    // LoopOutcome::Cancelled. `biased` checks cancel first so a
                    // pre-fired token wins deterministically.
                    let next_result = match cancel {
                        Some(tok) => tokio::select! {
                            biased;
                            _ = tok.cancelled() => {
                                tracing::info!("[M1-T2e] LLM stream cancelled mid-flight");
                                break;
                            }
                            r = tokio::time::timeout(stream_idle_timeout, stream.next()) => r,
                        },
                        None => {
                            tokio::time::timeout(stream_idle_timeout, stream.next()).await
                        }
                    };
```

- [ ] **Step 5: Update the 3 existing test callers**

At `llm_stream.rs:447`, `:466`, `:483`, each call ends with `Duration::from_secs(90)).await`. Add `, None` before the closing paren of `stream_completion(...)`. Each becomes:

```rust
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink, Duration::from_secs(90), None).await.unwrap();
```

- [ ] **Step 6: Run the new test and the existing stream tests**

Run: `cd src-tauri && cargo test --lib agent::llm_stream::tests 2>&1 | tail -20`
Expected: PASS — `cancellation_token_aborts_in_flight_stream` plus the pre-existing tests all green.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/agent/llm_stream.rs
git commit -m "feat(agent): stream_completion observes CancellationToken mid-flight

Thread an optional cancel token into stream_completion and wrap the chunk
wait in a biased select!. A fired token breaks to the finalize path,
returning the partial RespondOutput; run_turn_body's existing post-call
is_cancelled() check converts it to LoopOutcome::Cancelled. Closes the
LLM-stream half of R-6 (deferred at types.rs M1-T2e)."
```

---

## Task 2: `ChatDelegate`/`HeadlessDelegate` pass the token into `stream_completion`

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs:2214` (ChatDelegate call site), `src-tauri/src/agent/headless.rs:199` (HeadlessDelegate call site)

No new test — this is wiring; correctness is proven by Task 1's unit test (the flight point) and Task 4's loop-contract test. It must compile and keep existing tests green.

- [ ] **Step 1: Pass the token from `ChatDelegate::call_llm`**

In `dispatcher.rs`, immediately before the `stream_completion(` call at `:2214`, add a clone of the token (clone avoids holding a `&reason_ctx` borrow across the await; `CancellationToken` clone is cheap — it shares an `Arc`):

```rust
        // M1-T2e — clone the per-run cancellation token (if installed) so the
        // stream can abort mid-flight. Clone (not borrow) to avoid pinning a
        // &reason_ctx borrow across the streaming await.
        let cancel = reason_ctx.cancellation_token.clone();

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            self,
            stream_idle_timeout,
            cancel.as_ref(),
        )
        .await
```

(The only change to the existing `stream_completion(...)` call is the new final argument `cancel.as_ref()` plus the `let cancel = ...` line above it.)

- [ ] **Step 2: Pass `None` from `HeadlessDelegate::call_llm`**

In `headless.rs`, the `stream_completion(` call at `:199` gets `None` as its new final argument (automation runs cancel via `activity_is_cancelled` DB polling — `headless.rs:47`; wiring that token is a separate slice). Add `None,` as the last argument before `.await`:

```rust
        crate::agent::llm_stream::stream_completion(
            // ...existing args unchanged...
            None,
        )
        .await
```

(Match the existing argument list exactly; only append `None,` as the final arg.)

- [ ] **Step 3: Verify backend compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (no `error` lines).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent/dispatcher.rs src-tauri/src/agent/headless.rs
git commit -m "feat(agent): thread cancel token from call_llm into stream_completion

ChatDelegate passes reason_ctx.cancellation_token; HeadlessDelegate passes
None (automation keeps its DB-poll cancellation). Wires the Task 1 flight
point into the production loop."
```

---

## Task 3: `ToolDispatcher::dispatch` aborts in-flight tools on cancel

**Files:**
- Modify: `src-tauri/src/agent/tool_dispatch/mod.rs:36` (add field), `:105-153` (wrap dispatch), add `cancelled_outcome` helper, `:855` (test `ctx()` helper)
- Modify: `src-tauri/src/agent/dispatcher.rs:2524` (prod ctx literal)
- Test: `src-tauri/src/agent/tool_dispatch/mod.rs` (tests module)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `tool_dispatch/mod.rs` (after `dispatch_executes_and_returns_outcome`):

```rust
#[tokio::test]
async fn dispatch_short_circuits_when_cancelled() {
    // A registry with one real tool; a pre-fired token must short-circuit
    // BEFORE the tool runs, yielding one cancelled outcome per call.
    let executed = Arc::new(AtomicBool::new(false));
    let mut reg = ToolRegistry::new();
    reg.register(EchoTool::new(executed.clone()));
    let d = make_dispatcher(Arc::new(reg));

    let token = tokio_util::sync::CancellationToken::new();
    token.cancel(); // pre-fired

    let mut c = ctx();
    c.cancel = Some(token);

    let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];
    let outs = d.dispatch(calls, &c).await;

    assert_eq!(outs.len(), 1, "one outcome per call (no orphaned tool_use)");
    assert_eq!(outs[0].tool_call_id, "c1");
    assert!(outs[0].result.is_err());
    assert!(outs[0].is_error);
    assert_eq!(outs[0].message_content, "Error: tool execution cancelled");
    assert!(!executed.load(Ordering::SeqCst), "tool must not run when pre-cancelled");
}
```

- [ ] **Step 2: Run test to verify it fails (compile error)**

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests::dispatch_short_circuits_when_cancelled 2>&1 | tail -20`
Expected: FAIL — compile error, `ToolDispatchContext` has no field `cancel` (`no field 'cancel' on type 'ToolDispatchContext'`).

- [ ] **Step 3: Add the `cancel` field to `ToolDispatchContext`**

In `tool_dispatch/mod.rs`, change the struct at `:36`:

```rust
#[derive(Clone)]
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
}
```

to add the field (and the import at the top of the file: `use tokio_util::sync::CancellationToken;`):

```rust
#[derive(Clone)]
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
    /// M1-T2e — per-run cancellation token. When fired, `dispatch` aborts
    /// in-flight tools and returns one cancelled outcome per call. None for
    /// contexts without cancellation (tests, headless).
    pub cancel: Option<CancellationToken>,
}
```

- [ ] **Step 4: Rename the dispatch body to `dispatch_inner` and add the cancellation wrapper + helper**

In `tool_dispatch/mod.rs`, rename the existing public method body. Change the signature at `:105` from `pub async fn dispatch(...)` to a private `async fn dispatch_inner(...)` (body unchanged), then add the new public `dispatch` wrapper and the `cancelled_outcome` helper just above it:

```rust
    /// Dispatch a batch of tool calls, observing `ctx.cancel`. A fired token
    /// short-circuits (no tools run) or aborts in-flight tools (the dropped
    /// `dispatch_inner` future drops its JoinSet, which aborts spawned tasks),
    /// returning exactly one cancelled outcome per call so every tool_use gets
    /// a matching tool_result (no orphaned pairs in the message history).
    pub async fn dispatch(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
    where
        R: 'static,
    {
        let idents: Vec<(String, String, serde_json::Value)> = calls
            .iter()
            .map(|c| (c.id.clone(), c.name.clone(), c.arguments.clone()))
            .collect();
        let make_cancelled = || {
            idents
                .iter()
                .map(|(id, name, args)| Self::cancelled_outcome(id, name, args))
                .collect::<Vec<_>>()
        };

        match &ctx.cancel {
            Some(tok) if tok.is_cancelled() => make_cancelled(),
            Some(tok) => {
                let tok = tok.clone();
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => {
                        tracing::info!("[M1-T2e] tool dispatch cancelled mid-flight");
                        make_cancelled()
                    }
                    out = self.dispatch_inner(calls, ctx) => out,
                }
            }
            None => self.dispatch_inner(calls, ctx).await,
        }
    }

    /// Build a cancelled outcome for one call. Mirrors the shape of a hard-error
    /// outcome so execute_tool_calls bookkeeping pushes a matching tool_result.
    fn cancelled_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
        ToolDispatchOutcome {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            arguments: args.clone(),
            result: Err(crate::agent::tools::tool::ToolError::kinded(
                crate::agent::tools::tool::ToolErrorKind::Other,
                "tool execution cancelled",
            )),
            paths_touched: vec![],
            was_mutation: false,
            soft_error: None,
            rejected: false,
            message_content: "Error: tool execution cancelled".to_string(),
            is_error: true,
        }
    }
```

And change the existing method header at `:105`:

```rust
    pub async fn dispatch(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
```

to:

```rust
    async fn dispatch_inner(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
```

(Leave the entire body of `dispatch_inner` — the JoinSet/parallel/sequential logic — exactly as it is today.)

- [ ] **Step 5: Set `cancel` at the prod ctx literal and the test `ctx()` helper**

In `dispatcher.rs:2524`, add the field to the `ToolDispatchContext { ... }` literal:

```rust
        let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
            session_id: self.conversation_id.clone(),
            conversation_id: self.conversation_id.clone(),
            workspace_root: self.workspace_root.clone(),
            attached_dirs: vec![],
            safety_mode: self.safety_mode.clone(),
            iteration: self.turn_index.fetch_add(1, Ordering::Relaxed) as usize,
            cancel: reason_ctx.cancellation_token.clone(),
        };
```

In `tool_dispatch/mod.rs:855` (the test `ctx()` helper), add `cancel: None`:

```rust
        ToolDispatchContext { session_id: "s".into(), conversation_id: "s".into(),
            workspace_root: None, attached_dirs: vec![], safety_mode: None, iteration: 1,
            cancel: None }
```

If `cargo build` reports any other `ToolDispatchContext { ... }` literal missing `cancel`, add `cancel: None` there (test contexts) or the appropriate token (prod contexts).

- [ ] **Step 6: Run the new test + the full tool_dispatch suite**

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests 2>&1 | tail -20`
Expected: PASS — `dispatch_short_circuits_when_cancelled` plus all pre-existing dispatch tests green.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/agent/tool_dispatch/mod.rs src-tauri/src/agent/dispatcher.rs
git commit -m "feat(agent): ToolDispatcher aborts in-flight tools on cancel

Add ToolDispatchContext.cancel; dispatch wraps the batch in a biased select!
so a fired token short-circuits (or aborts in-flight tools via dropped
JoinSet) and returns one cancelled outcome per call, preserving tool_use ↔
tool_result pairing. run_turn_body's post-tool is_cancelled() check then
ends the turn as Cancelled. Closes the tool-exec half of R-6."
```

---

## Task 4: Loop-contract test — a cancel-aware delegate yields `LoopOutcome::Cancelled`

**Files:**
- Test: `src-tauri/src/agent/agentic_loop.rs` (tests module, alongside the existing mock-delegate integration tests at `:2406+`)

This locks the loop↔delegate contract: when `call_llm` returns promptly after the token fires (which Tasks 1-2 now guarantee for the real delegate), the loop returns `Cancelled`. It uses a mock delegate so it needs no Tauri runtime.

- [ ] **Step 1: Inspect the existing mock-delegate test pattern**

Run: `cd src-tauri && sed -n '2050,2120p' src/agent/agentic_loop.rs`
Expected: see the existing `LoopDelegate` mock impl (the `call_llm` at `:2058`) and how `run_agentic_loop` is invoked in a test, so the new test reuses the same harness/types (`ReasoningContext::new(...).with_cancellation(token)`, `AgenticLoopConfig`, the mock struct).

- [ ] **Step 2: Write the test**

Add a test that builds a mock delegate whose `call_llm` waits on the token (simulating the now-cancellable stream) and returns a partial `RespondOutput::Text`, installs a pre-fired token via `ReasoningContext::with_cancellation`, runs the loop, and asserts `LoopOutcome::Cancelled`. Model the mock on the existing one found in Step 1; the cancel-aware `call_llm` body is:

```rust
        async fn call_llm(
            &self,
            reason_ctx: &mut ReasoningContext,
            _snapshot: &crate::agent::turn::TurnSnapshot,
            _iteration: usize,
        ) -> Result<RespondOutput, Error> {
            // Mirror the real delegate post-Task-1: return promptly when the
            // installed token is fired, instead of blocking on a stream.
            if let Some(tok) = reason_ctx.cancellation_token.clone() {
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                }
            }
            Ok(RespondOutput::Text {
                text: String::new(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata {
                    model: "test".into(),
                    finish_reason: Some("stream_ended".into()),
                    usage: None,
                },
            })
        }
```

And the assertion body:

```rust
#[tokio::test]
async fn fired_token_yields_cancelled_outcome() {
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel(); // pre-fired
    let mut reason_ctx = ReasoningContext::new("sys".into()).with_cancellation(token);
    // ... build the cancel-aware mock delegate + AgenticLoopConfig as in Step 1 ...
    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        run_agentic_loop(&delegate, &mut reason_ctx, &config),
    )
    .await
    .expect("loop did not return within 500ms after cancel");
    assert!(matches!(outcome, LoopOutcome::Cancelled { .. }));
}
```

(Fill the `// ...` using the exact mock-delegate constructor and `run_agentic_loop` signature observed in Step 1 — do not invent field names; copy them from the existing test.)

- [ ] **Step 3: Run the test**

Run: `cd src-tauri && cargo test --lib agent::agentic_loop::tests::fired_token_yields_cancelled_outcome 2>&1 | tail -20`
Expected: PASS within well under 500ms (proves no 30s block).

- [ ] **Step 4: Run the whole agent test suite to check for regressions**

Run: `cd src-tauri && cargo test --lib agent:: 2>&1 | tail -15`
Expected: all agent tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/agentic_loop.rs
git commit -m "test(agent): loop returns Cancelled when token fires during call_llm

Locks the loop↔delegate cancellation contract that Tasks 1-3 rely on: a
prompt-returning cancel-aware call_llm + fired token yields
LoopOutcome::Cancelled within 500ms (no 30s block)."
```

---

## Self-Review

**1. Spec coverage** (spec = audit §1.1 CRITICAL "thread CancellationToken into stream_completion and ToolDispatcher::dispatch"):
- LLM flight point (`stream_completion`) → Task 1 (impl + unit test) + Task 2 (prod wiring). ✓
- Tool flight point (`ToolDispatcher::dispatch`) → Task 3 (impl + unit test + wiring). ✓
- Loop-level "returns within 100ms / Cancelled" guarantee → Task 1 test (500ms guard on the LLM await), Task 3 test (short-circuit), Task 4 test (end-to-end loop contract). ✓
- No `LoopDelegate` trait change needed (token rides in `reason_ctx`) — verified at `types.rs:352` (call_llm) and `:359` (execute_tool_calls). ✓

**2. Placeholder scan:** No "TBD"/"add error handling"/"similar to Task N". The two `// ...` markers in Task 4 are explicit instructions to copy the *existing* mock from Step 1's `sed` output (the only codebase-specific harness this plan can't safely inline without first reading it), not hidden work. All edited code is shown in full.

**3. Type consistency:** `cancel: Option<&CancellationToken>` (free fn param, Task 1) vs `cancel: Option<CancellationToken>` (owned struct field, Task 3) — intentional: the fn borrows for one await; the struct owns across `dispatch_inner`. Bridged by `.clone()` (dispatcher.rs) and `.as_ref()` (call_llm). `cancelled_outcome` returns every `ToolDispatchOutcome` field defined at `mod.rs:46-61`. `ToolError::kinded` / `ToolErrorKind::Other` match `tools/tool.rs:108`/enum. `with_cancellation` matches `types.rs:134`.

**Risk note:** Adding non-`Default` fields to `ToolDispatchContext` and a param to `stream_completion` makes the compiler enumerate every call/literal site — there is no silent-miss path. Blast radius is compile-bounded. Behavior for the `None`/no-token path is byte-identical to today (the `None` match arms are the original code).
