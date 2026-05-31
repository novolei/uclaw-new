# Item 1.A — Thread CancellationToken to Flight Points Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax. **This touches the live agent loop + tool execution — high blast radius. Recon thoroughly; keep changes additive to the await points.**

**Goal:** Make in-flight LLM streams and long-running bash commands actually interruptible. Today `CancellationToken` is observed only at loop checkpoints (`agentic_loop.rs:89/441`) + a dispatch-level `ToolDispatchContext.cancel`, but the two real blocking await points — bash's `child.wait().await` (shell.rs:641) and the production LLM-stream `.next().await` — don't race the token, so a 30s stream / a runaway bash can't be aborted mid-execution (gap-audit §1.1 CRITICAL, loop-audit R-6).

**Architecture:** Thread the existing `CancellationToken` (from `ToolDispatchContext.cancel` / `ReasoningContext.cancellation_token`) into the two flight points: (1) bash — wrap `child.wait().await` in `tokio::select!` against `token.cancelled()`, killing the child + returning a "cancelled" `ToolOutput` on fire; (2) LLM stream — wrap the production stream consume's `.next().await` in `select!` against the token, ending the stream early. No new cancellation infra (the token + `ToolDispatchContext.cancel` + the `execute_*_with_context` mechanism already exist) — this completes the plumbing to the await points.

**Tech Stack:** Rust, `tokio::select!`, `tokio_util::sync::CancellationToken` (present), `tokio::process::Child::kill`. No new deps.

---

## Source-of-truth references (verified)

- `agent/tool_dispatch/mod.rs` — `ToolDispatchContext.cancel: Option<CancellationToken>` (line 77, "M1-T2e — when fired, `dispatch` aborts"); `dispatch` (178) / `dispatch_inner` (278) / `run_one` (330) all take `ctx: &ToolDispatchContext`. The token reaches `run_one`; **it does not currently reach inside the tool's execution** (the await point).
- `agent/tools/tool.rs` — `trait Tool` (219): `async fn execute(&self, params) -> Result<ToolOutput, ToolError>` (224) + `async fn execute_streaming(&self, params, sink)` (232). `execute_tool_with_context` (272) + `execute_streaming_with_context` (282) — **context-carrying entry points** (recon: do they already pass a token, or is there a `ToolContext`/`ToolExecCtx` to extend with the token? This is the threading seam — prefer extending it over changing every `Tool::execute` signature).
- `agent/tools/builtin/shell.rs` — `run(&self, params, sink)` (506); `child.wait().await` (641) — the bash flight point. `child` is a `tokio::process::Child` (has `.kill().await` / `.start_kill()`). Daemon mode (spawn_daemon 440) returns immediately — NOT a flight point (leave it).
- `agent/agentic_loop.rs` — `ReasoningContext.cancellation_token: Option<CancellationToken>` (the source); checkpoints at 89/441; the mid-flight contract TEST `fired_token_yields_cancelled_outcome` (~2876) with a mock select! at 2831. **The production `call_llm` + its stream consume is the LLM flight point — recon: find the real ChatDelegate `call_llm` / streaming consume (`grep -rn "stream(" + "while let Some" + StreamDelta` across `agent/dispatcher/` + `llm/`).** The gap-audit cited `llm_stream.rs` / `dispatcher.rs:2214 stream_completion`.

---

## CRITICAL facts

1. **The infra exists; only the await-point wiring is missing.** `ToolDispatchContext.cancel` + `ReasoningContext.cancellation_token` + `execute_*_with_context` are already there. Do NOT build new cancellation machinery — thread the existing token to `child.wait()` and the stream `.next()`.
2. **Bash: kill the child on cancel.** `select! { biased; _ = token.cancelled() => { child.start_kill().ok(); return cancelled ToolOutput }, status = child.wait() => { ... existing path } }`. Use `biased` so cancel wins. After `start_kill`, optionally `child.wait().await` to reap. Return a clear `ToolOutput`/`ToolError` ("command cancelled") — NOT a hang.
3. **LLM stream: end early on cancel.** Wrap the `while let Some(delta) = stream.next().await` consume: `select! { biased; _ = token.cancelled() => break, maybe = stream.next() => { ... } }`. On cancel, break the consume loop + return what's accumulated (or a cancelled marker) so the loop sees `Cancelled` at the next checkpoint.
4. **No-token path unchanged.** When the token is `None` (tests, headless), behave exactly as today (no select!, direct await). Gate the select! on `Some(token)`.
5. **Live-loop safety.** The dispatcher is a god object with ordering invariants — the change is confined to the two await points (wrap-in-select!), not the surrounding bookkeeping. Existing tests (incl. `fired_token_yields_cancelled_outcome`) must stay green; ADD tests for the two new flight points.

---

## File Structure

| File | Mod | Change | LoC |
|---|---|---|---|
| `agent/tools/builtin/shell.rs` | mod | thread token into `run`; wrap `child.wait().await` in select!-against-cancel + kill; "cancelled" output + test | ~+60 |
| `agent/tools/tool.rs` / the context type | mod | extend the tool-exec context to carry the token (if not already) so `run`/`execute` can reach it without a trait-wide signature break | ~+20 |
| `agent/tool_dispatch/mod.rs` | mod | pass `ctx.cancel` into the tool-exec context at `run_one` | ~+10 |
| the LLM stream-consume site (recon — `dispatcher/` or a streaming module) | mod | wrap `.next().await` in select!-against-cancel + early-break + test | ~+40 |

Est. ~130 source + ~120 tests.

---

## Adaptation responsibilities

1. **RECON FIRST — map the two flight points + the token path before any edit:**
   - Bash: `shell.rs:641 child.wait().await` (confirmed). How does `run` get the token? Trace `ToolDispatchContext.cancel` → `run_one` → the tool-exec call. Find the `execute_*_with_context` context type; if it carries a token already, use it; else extend it with `cancel: Option<CancellationToken>` and populate from `ctx.cancel`.
   - LLM stream: `grep -rn "stream(" "while let Some" "StreamDelta" "\.next()\.await"` across `agent/dispatcher/` + `llm/`. Find the PRODUCTION consume (not the mock at agentic_loop.rs:2831). Confirm the token is reachable there (ReasoningContext.cancellation_token is in scope in the real `call_llm`).
2. **Bash select! + kill** — `tokio::process::Child::start_kill()` (non-blocking) then return; or `.kill().await` (kills + reaps). Use `biased` + cancel-arm-first. Map to a `ToolError::kinded(Cancelled, ...)` or a `ToolOutput` flagged cancelled — match how the loop expects a cancelled tool result (check `ToolDispatchOutcome` for a cancelled variant; if none, return an error the loop maps to Cancelled).
3. **LLM stream select!** — wrap the consume; on `token.cancelled()` break + return the partial/empty result so the loop's post-call checkpoint (`agentic_loop.rs:89`) sees cancellation + returns `LoopOutcome::Cancelled`. Don't leave a dangling stream (drop it on break).
4. **No-token path** — `match token { Some(t) => select!{...}, None => <direct await> }`. Tests without a token unaffected.
5. **Tests:**
   - Bash: spawn `sleep 30` (or a long loop) with a token fired ~50ms in → `run` returns within ~500ms with a cancelled result + the child is killed (assert via timing + that the process is gone). Mirror the existing shell test style.
   - LLM stream: a fake stream that yields slowly / never-ends + a token fired mid-consume → the consume returns within ~500ms. (May reuse the agentic_loop mock pattern, but assert the PRODUCTION consume path, not the mock delegate.)
   - No-token: bash + stream complete normally when token is None (no regression).
6. **Keep `fired_token_yields_cancelled_outcome` + the loop checkpoints green** — this SP makes the mock contract real at the production await points; the loop-level test should still pass.
7. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: bash mid-execution cancel

- [ ] **Step 1: RECON** the token path from `ToolDispatchContext.cancel` (tool_dispatch/mod.rs:77) → `run_one` (330) → the tool-exec call → shell `run`. Identify the context type to carry the token (extend `execute_*_with_context`'s ctx if needed). Read `ToolDispatchOutcome` for a cancelled representation.

- [ ] **Step 2: Write the failing test** (shell.rs tests): spawn a long command (`sleep 30` on unix), fire a token ~50ms in, assert `run` returns < 500ms with a cancelled result + (best-effort) the process is killed. Gate on unix (`#[cfg(unix)]`) if needed.

- [ ] **Step 3: Thread the token + wrap `child.wait()`** in `select!`-against-cancel with `biased` + `start_kill`/`kill`. No-token path = direct `child.wait().await` (unchanged).

- [ ] **Step 4: Run + commit.**
```bash
cd src-tauri && cargo test --lib agent::tools::builtin::shell 2>&1 | tail
git add -A
git commit -m "feat(agent): cancel long bash mid-execution (kill child on token) (1.A.1)"
```

### Task 2: LLM stream mid-flight cancel

- [ ] **Step 1: RECON** the production stream-consume site (`agent/dispatcher/` / `llm/`) + confirm the token is in scope there.

- [ ] **Step 2: Write the failing test** — a fake/slow stream + a fired token → the consume returns < 500ms (production path).

- [ ] **Step 3: Wrap the `.next().await` consume** in `select!`-against-cancel + early-break + drop the stream. No-token path unchanged.

- [ ] **Step 4: Run + commit.**
```bash
cd src-tauri && cargo test --lib agent 2>&1 | tail -5
git add -A
git commit -m "feat(agent): abort LLM stream mid-flight on cancel (1.A.2)"
```

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::shell 2>&1 | tail` (bash cancel + no-regression pass).
- [ ] `cd src-tauri && cargo test --lib agent 2>&1 | tail -5` (incl. `fired_token_yields_cancelled_outcome` still green; new stream test passes; 2 pre-existing failures unchanged).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "shell\.rs|tool_dispatch|tool\.rs|dispatcher" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **The R-6 contract**: a fired token returns the bash tool + the LLM stream within ~500ms (loop-audit R-6 "#2: 100ms" — aim < 500ms to be safe given test scheduling).
- [ ] **No-token no-regression**: bash + stream complete normally when the token is absent.

---

## Self-Review

- ✅ Spec coverage: bash flight point (Task 1) + LLM stream flight point (Task 2) wired to the existing token. The loop checkpoints + ToolDispatchContext.cancel pre-exist (untouched).
- ✅ No placeholders — the await points are named (shell.rs:641; the stream consume is a recon-and-wrap with the exact select! pattern). The token-threading seam (execute_*_with_context ctx) is a recon-and-extend with a clear fallback.
- ✅ Type consistency: `select! { biased; _ = token.cancelled() => …, x = <await> => … }` at both points; no-token → direct await; token sourced from `ToolDispatchContext.cancel` / `ReasoningContext.cancellation_token`.
- ✅ Risk-scaled: high blast radius (live loop) → changes confined to the two await points (wrap-in-select!), additive, no-token path preserved, existing cancellation tests kept green + new flight-point tests added.
- Decisions: thread the EXISTING token (no new infra); kill the bash child on cancel; early-break + drop the stream; gate select! on Some(token) so headless/test paths are unchanged.
