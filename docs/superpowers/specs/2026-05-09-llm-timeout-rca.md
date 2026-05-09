# LLM Timeout / Stream-Error Root Cause Analysis & Fix

**Date:** 2026-05-09
**Reporter:** User (during Gomoku game upgrade session)
**Status:** Spec — fix not yet implemented.

## Symptom

Two errors observed in the same agent session, in this order:

```
2026-05-09T05:32:20.347780Z  WARN uclaw_core::agent::dispatcher:
  Stream error, falling back to complete: Internal error:
  OpenAI stream read error: error decoding response body
```

…then later, after some retries:

```
Agent 出错了
Request timed out — the model took too long to respond.
```

The session was a multi-iteration agent task (incrementally upgrading a Gomoku game) — multiple file reads, code generation, several thousand output tokens per LLM round.

This is not a one-off network blip. The same shape will recur for any sufficiently complex agent task on OpenAI **or Anthropic** providers (both share the bug — see §1.4). Smaller test tasks succeeded only because they completed inside the budget by accident.

## TL;DR

The reported error is one symptom of three stacked misconfigurations that together make the LLM call layer unreliable for any task that takes longer than ~60 seconds end-to-end:

| | What's wrong | File |
|---|---|---|
| **Bug A** | `client.post(...).timeout(60s)` is applied to **streaming** requests. reqwest's `.timeout()` is a TOTAL request budget — it kills the open connection at 60s mid-stream regardless of whether the model is still emitting tokens. | `src-tauri/src/llm/providers/openai.rs:228`, `anthropic.rs:228` |
| **Bug B** | When the stream errors out, dispatcher falls back to non-streaming `complete()` which has the same 60s cap × 3 retries. This **doubles** the wall time on a slow request and **triggers on benign causes** (network blips). | `src-tauri/src/agent/dispatcher.rs:454-469` |
| **Bug C** | The outer agentic-loop timeout is **180s**, less than the worst-case 60s × (1 stream + 3 fallback retries) = 240s — so the user always sees the outer timeout, never the underlying reason. | `src-tauri/src/tauri_commands.rs:2745-2763` |

The fix is **not** "raise the timeout to 600s." That's the band-aid the user explicitly rejected. The fix is to give each layer the right kind of timeout for its purpose:

- HTTP CONNECT + headers: short (10–15s) — fail fast on bad endpoints.
- Streaming response: NO total cap; **per-chunk** stall timeout (45s of silence = stall) — accommodates long generations, still detects dead connections.
- Non-streaming response: total cap (120s) — bounded by request shape.
- Agent loop wall-clock: configurable (default 600s = 10 min) — bounded by user attention, not network mechanics.

Each of these is the **structurally correct** value for what it's bounding, not a guess. Details below.

## 1. Root cause analysis

### 1.1 What the reported error actually means

`error decoding response body` is the hyper layer's error message when a chunked-transfer body terminates abnormally. There are exactly three causes in practice:

1. The TCP connection was closed by the server before the stream ended (e.g. provider-side timeout, OOM, scaling event).
2. The TCP connection was closed by **us** (the client) before the stream ended.
3. The chunked encoding itself was malformed (a bad proxy mangled it).

Cause #1 is rare for OpenAI/Anthropic on healthy endpoints. Cause #3 is rare period. **Cause #2 is what hit you** — and it's caused by the `.timeout(60s)` we set ourselves.

### 1.2 reqwest `.timeout()` semantics on streaming

```rust
// src-tauri/src/llm/providers/openai.rs:223-231
let result = self.client
    .post(format!("{}/v1/chat/completions", self.base_url))
    .header("Authorization", format!("Bearer {}", self.api_key))
    .header("content-type", "application/json")
    .timeout(self.timeout)        // ← 60 seconds, applied to BOTH stream and complete
    .json(body)
    .send()
    .await;
```

In reqwest 0.12, `RequestBuilder::timeout(Duration)` sets a **per-request total timeout**. It bounds:
- DNS resolution
- TCP connect
- TLS handshake
- Sending the request body
- Receiving the entire response body

For a non-streaming request, this is fine — the body arrives in one shot, usually well under 60s.

For a **streaming** request, reqwest still enforces the timeout against the entire request lifecycle, including the time spent reading the streaming body. When the timer fires:

1. Hyper closes the underlying TCP connection.
2. The next `bytes_stream().poll_next()` returns `Poll::Ready(Some(Err(reqwest::Error)))`.
3. The `Display` impl on that error reads `error decoding response body` because the chunked stream ended without a terminating `0\r\n\r\n`.
4. Our wrapper at `openai.rs:504-509` repackages it as `Error::Internal("OpenAI stream read error: …")`.

This means we have a **hard 60s ceiling** on any streaming generation, regardless of whether the model is healthily producing tokens. Long-running agent tasks (Gomoku upgrade, multi-file refactors, deep reasoning passes) WILL hit this ceiling.

### 1.3 The fallback is not a recovery — it's an accelerant

`dispatcher.rs:454-469`:

```rust
Err(e) => {
    tracing::warn!("Stream error, falling back to complete: {}", e);
    self.emit_stream_reset();
    break;
}
// ...
// Fallback to non-streaming complete
self.llm.complete(messages, tools, &config).await
```

Three problems with this fallback:

**1. Same underlying timeout.**
`complete()` calls `send_with_retry()` which uses the same `.timeout(60s)`. If the request would have streamed 90s of tokens, it'll also need ~90s as one big response — and time out again.

**2. Retries multiply the wall time.**
`send_with_retry()` retries up to 3 times with exponential backoff (500ms → 1s → 2s). Each retry hits the same 60s timeout. Worst-case wall time for the fallback alone: 60 × 3 + 0.5 + 1 + 2 ≈ 183s.

**3. Wrong response to the wrong cause.**
A genuine network blip mid-stream could be cured by a fresh stream attempt. Falling back to non-streaming for a blip is a category mismatch — and erases all the user's streaming UX (live thinking + tool blocks).

**4. The frontend gets confused state.**
The fallback emits `emit_stream_reset()` but the frontend has already displayed live thinking/tool deltas. Now those vanish, then the non-streaming response replaces them. The user sees "process flashed and disappeared" which is exactly what the user complained about earlier in this session (PR #9 fixed *one* manifestation; this is another).

### 1.4 The bug is in BOTH providers

Identical structure in `anthropic.rs`:
- Line 12: `const DEFAULT_TIMEOUT_SECS: u64 = 60;`
- Line 31: `timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS)`
- Line 228 (in `send_with_retry`): `.timeout(self.timeout)`

So switching providers won't help. Both have the same 60s cap on streaming.

### 1.5 The outer 180s cap is incoherent with the inner retry math

`tauri_commands.rs:2745-2763`:

```rust
let outcome = tokio::select! {
    result = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut ctx, &config)
    ) => match result {
        Ok(o) => o,
        Err(_) => {
            tracing::error!(session_id = %session_id, "Agentic loop timed out after 180s");
            // ... emit "Request timed out"
        }
    },
    // ...
};
```

180s for the entire agent loop — including all LLM rounds, all tool executions, all retries. But:
- One stream attempt: up to 60s
- One fallback complete: up to 60s × 3 retries ≈ 183s

A SINGLE LLM round can already exceed 180s in worst-case retry math. Once tool calls + multiple rounds enter the picture, the 180s ceiling is permanently hostile to any non-trivial task.

This is what the user actually saw: "Request timed out — the model took too long to respond."

### 1.6 Why the user's task hit it

Gomoku upgrade session:
- Several rounds of "read file → think → propose change → write file"
- Each round: ~3-8k output tokens (file diffs are verbose)
- At a streaming rate of ~30-60 tokens/sec, each round takes 60-180s of generation
- Plus thinking content, plus tool call execution
- → most rounds blow through the 60s HTTP cap → fall back to non-streaming → that hits 60s × retry → outer 180s fires → user sees the message

This is **structurally inevitable** for the workload, given the current configuration. A different task that happened to fit in 60s/round doesn't reveal the bug, but the bug is still latent.

## 2. Why "raise the timeouts" is not the right fix

The user explicitly asked for the *right* fix, not a patch. Here's why "just bump everything to 10 minutes" fails:

| Naive fix | Why it's wrong |
|---|---|
| `DEFAULT_TIMEOUT_SECS = 600` | Now a hung connect (DNS dead, port firewalled) takes 10 minutes to fail. UX regression: user sees nothing for 10 minutes when something simple is wrong. Fast-fail signals are lost. |
| Outer agent loop = 600s | Doesn't fix the inner stream timeout. Stream still dies at 60s; fallback still races. Just buries the problem deeper. |
| Disable retries | Loses the genuine value of retries for transient errors (rate-limit, 5xx). |
| Disable streaming, only use complete | Loses live thinking/tool UX. Still needs a long timeout. |

The right fix recognizes that **different parts of the request lifecycle have different right answers** for what to bound:

| Phase | Right answer | Reason |
|---|---|---|
| DNS + TCP connect + TLS handshake | 10–15s hard cap | Healthy endpoints respond in <1s; slow hosts indicate config error or network failure — fail fast. |
| Headers received | 30s | Server is alive but is it our endpoint? |
| **Streaming body** | **No total cap; 45s per-chunk stall timeout** | Healthy streams emit at least one byte every few seconds; 45s of silence means the connection is dead even if reqwest hasn't noticed. A genuine slow generation doesn't stall — it emits steady deltas. |
| Non-streaming body | 120s total | One round-trip; no progress signal; bounded by what the model could realistically generate in one shot. |
| Agent loop | Configurable, default 600s | This is "user attention" budget, not a network primitive. Different users have different patience. Default high, expose in settings. |

## 3. The fix

Three coordinated changes. None standalone fixes the problem; together they remove the entire failure class.

### 3.1 Fix the HTTP layer (Bug A)

**Files:** `src-tauri/src/llm/providers/openai.rs`, `src-tauri/src/llm/providers/anthropic.rs`.

#### 3.1.1 Constructor: configure the client once, not per-request

Replace:
```rust
client: reqwest::Client::new(),
timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
```

with:
```rust
client: reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(15))    // TCP+TLS budget only
    .pool_idle_timeout(Duration::from_secs(90))   // keep-alive sanity
    .timeout(Duration::from_secs(120))            // for non-streaming complete()
    .build()
    .expect("reqwest client should build"),
stream_stall_timeout: Duration::from_secs(45),    // per-chunk stall for streams
complete_timeout: Duration::from_secs(120),
```

Drop the bare `timeout` field — replace with two named durations.

#### 3.1.2 `send_with_retry` for non-streaming: keep the total timeout

```rust
async fn send_with_retry(
    &self,
    body: &serde_json::Value,
    is_stream: bool,
) -> Result<reqwest::Response, Error> {
    // ...
    let mut req = self.client
        .post(format!("{}/v1/chat/completions", self.base_url))
        .header("Authorization", format!("Bearer {}", self.api_key))
        .header("content-type", "application/json")
        .json(body);

    // For non-streaming requests, use the per-request total timeout.
    // For streaming, only the connect_timeout from the client config applies;
    // the response body has no total cap (we use per-chunk stall timeout in
    // the SSE state machine instead).
    if !is_stream {
        req = req.timeout(self.complete_timeout);
    }

    let result = req.send().await;
    // ... rest unchanged
}
```

Plumb `is_stream` to both call sites: `complete()` passes `false`, `stream()` passes `true`.

#### 3.1.3 Stream state machine: per-chunk stall timeout

In `OpenAiSseState::poll_next` / `OpenAISseStream` (and equivalent in `anthropic.rs`), wrap the `byte_stream.next().await` in `tokio::time::timeout`:

```rust
match tokio::time::timeout(self.stall_timeout, self.byte_stream.next()).await {
    Ok(Some(Ok(bytes))) => {
        let text = String::from_utf8_lossy(&bytes);
        self.buffer.push_str(&text);
    }
    Ok(Some(Err(e))) => {
        self.done = true;
        return Some(Err(Error::Internal(format!(
            "OpenAI stream read error: {}",
            e
        ))));
    }
    Ok(None) => {
        // existing graceful end logic
    }
    Err(_) => {
        // Stall: no chunk in `stall_timeout`. Treat as a recoverable error
        // (caller decides whether to retry the stream).
        self.done = true;
        return Some(Err(Error::StreamStalled {
            duration: self.stall_timeout,
        }));
    }
}
```

This requires:
- New `Error::StreamStalled { duration: Duration }` variant in `error.rs`. Distinct from `Error::Internal` so the dispatcher can react differently.
- Stream state structs gain `stall_timeout: Duration` field, plumbed from `OpenAIProvider::stream()`.

#### 3.1.4 Why `connect_timeout(15s)` is enough for connect

OpenAI's `api.openai.com` and Anthropic's `api.anthropic.com` answer DNS in <100ms and TLS in <500ms on a healthy network. 15s gives us 30× headroom; anything longer indicates a real problem (firewall, DNS misconfiguration, bad proxy) where failing fast is the right answer.

### 3.2 Replace the bad fallback (Bug B)

**File:** `src-tauri/src/agent/dispatcher.rs:454-469`.

Current:
```rust
Err(e) => {
    tracing::warn!("Stream error, falling back to complete: {}", e);
    self.emit_stream_reset();
    break;
}
// ...
// Fallback to non-streaming complete
self.llm.complete(messages, tools, &config).await
```

New behavior:

```rust
Err(e) => {
    // Classify the stream error and decide between three responses:
    //   (1) StreamStalled: connection healthy but no bytes for N seconds → retry the stream
    //   (2) Recoverable network error mid-stream → retry the stream once
    //   (3) Unrecoverable (4xx, fatal): bubble up
    match classify_stream_error(&e) {
        StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork
            if stream_retry_count < MAX_STREAM_RETRIES =>
        {
            tracing::warn!(error = %e, attempt = stream_retry_count + 1,
                "Stream interrupted, retrying with a fresh stream");
            self.emit_stream_reset();
            stream_retry_count += 1;
            continue 'stream_attempt;
        }
        StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
            // Out of retries — surface as a stream error to the UI; do NOT
            // fall back to non-streaming because that would erase all the
            // streamed thinking/tools the user has already seen.
            tracing::error!(error = %e, "Stream failed after all retries");
            return Err(e);
        }
        StreamErrorKind::Fatal => {
            // 4xx (auth, model-not-found, etc.) — fail loudly, retries won't help.
            tracing::error!(error = %e, "Stream failed with fatal error");
            return Err(e);
        }
    }
}
```

The `classify_stream_error` helper inspects the error variant:
- `Error::StreamStalled { .. }` → `Stalled`
- `Error::Internal(msg)` where `msg.contains("error decoding response body")` or `"connection reset"` or `"broken pipe"` → `TransientNetwork`
- `Error::Internal(msg)` from a 4xx → `Fatal`

`MAX_STREAM_RETRIES = 2` (3 total attempts: original + 2 retries). With the per-chunk stall behavior, each retry only fires after a real stall, so worst-case wall time is bounded by `(stall_timeout + retry_overhead) × MAX_STREAM_RETRIES`.

**The non-streaming `complete()` fallback is removed entirely from the streaming path.** It still exists as a separate code path for callers that explicitly choose non-streaming (or for providers that don't support streaming) — but a stream failure no longer auto-degrades. This:
- Eliminates the wall-time doubling (Bug B's main harm)
- Preserves the streaming UX (live thinking/tools never disappear)
- Surfaces real failures to the user (so they know what's wrong, not get a fallback they didn't ask for)

### 3.3 Right-size the agent loop timeout (Bug C)

**File:** `src-tauri/src/tauri_commands.rs:2745-2763`.

Current: hard-coded 180s.

New:
- Default 600s (10 min) — fits the slowest realistic agent task before requiring user intervention.
- Read from `MemubotConfig.agent_loop_timeout_secs` (new field, default 600).
- Expose in the Tauri settings UI under "Advanced".
- On timeout: emit a structured error that includes which step the loop was on (read from `reason_ctx.thread_state`) so the user knows whether the model was thinking, calling tools, or waiting on tool results.

The 600s default is chosen as: ~5 LLM rounds × ~90s/round + tool execution overhead. Anything longer is genuinely runaway and should require explicit user opt-in (the configurability handles that).

### 3.4 Frontend: surface the new error states gracefully

**Files:** `ui/src/hooks/useGlobalAgentListeners.ts`, `ui/src/atoms/agent-atoms.ts`.

Three new error categories to surface (currently they all collapse to "stream error" generic toast):

| Backend signal | UX |
|---|---|
| `StreamStalled` (auto-retrying) | Inline "重新连接中…" badge on the streaming bubble. No toast. |
| `StreamStalled` (out of retries) | Inline error message on the bubble; "重试" button that re-runs the same agent step. |
| `Fatal` (auth / 4xx) | Full toast. Linkable to settings. No retry button (model rejection is not a retry-friendly state). |
| Outer 600s timeout | Inline timeout explanation + "继续运行" button (extend by 5 minutes), since user attention is the real bound. |

This needs minor schema additions to the `chat:stream-error` event payload (`kind` field) and corresponding render branches, but it's mechanical — the bulk of the fix is in the Rust layer.

## 4. What we are explicitly NOT doing and why

| Tempting move | Why we don't do it |
|---|---|
| Bump `DEFAULT_TIMEOUT_SECS` to 600 | Treats a 60-second connect identically to a 60-second body read. Loses fast-fail on broken endpoints. |
| Disable streaming entirely | Loses live UX. Doesn't actually reduce wall time. |
| Set `connect_timeout` only and remove all body timeouts | A genuinely hung body (TCP zombie connection that never times out kernel-side) would block forever. Per-chunk stall is the answer. |
| Use `tokio::time::timeout` around the entire stream consumer | Reintroduces Bug A but at a different layer. We want STALL detection, not TOTAL bound. |
| Drop the agent-loop outer timeout | User attention is real. A genuinely runaway loop (model in a tool-call death spiral) needs a ceiling. We're just choosing a sane one. |
| Add more retries everywhere | Retries on 4xx are wasteful; retries on 5xx are good but already exist. We tune retries per kind, not blanket. |

## 5. Acceptance criteria for the fix

- [ ] A 90-second-streaming generation completes successfully end-to-end. (Currently fails at 60s.)
- [ ] A 5-minute-streaming generation also completes. (Currently impossible — outer cap.)
- [ ] A connection that genuinely stalls (e.g. server crash mid-stream) is detected within ≤ stall_timeout (default 45s) and either auto-retries or surfaces a clean error to the user.
- [ ] A network blip mid-stream auto-recovers via stream retry — user sees no disruption beyond a brief "reconnecting" badge.
- [ ] A 4xx response (e.g. wrong API key) fails immediately with a clear error and no retries.
- [ ] Streaming UX is preserved on retry — the user does not see thinking/tool blocks vanish and reappear.
- [ ] Outer agent-loop timeout is configurable via settings; default 600s.
- [ ] Both OpenAI and Anthropic providers are fixed in lockstep.
- [ ] No regression in fast-failure for genuinely broken setups (DNS error, firewall, wrong base URL).

## 6. Estimated scope

This is one focused plan, not a roadmap entry on its own. Breakdown:

| Slice | Files | Effort |
|---|---|---|
| HTTP layer (Bug A) | `openai.rs`, `anthropic.rs`, `error.rs` | 1d |
| Stream classifier + retry (Bug B) | `dispatcher.rs` | 0.5d |
| Outer timeout config (Bug C) | `tauri_commands.rs`, `memubot_config.rs`, settings UI | 0.5d |
| Frontend error categorization (§3.4) | `useGlobalAgentListeners.ts`, atoms, UI | 0.5d |
| Tests | new `src-tauri/tests/llm_timeout.rs` integration tests | 1d |
| **Total** | | **3.5 days** |

Should fit a single PR (sized like P5/P6 in the roadmap, not a mega-plan).

## 7. Decision

This document is the spec. Once signed off, the next step is to expand it into an actionable `docs/superpowers/plans/uclaw-llm-timeout-fix.md` plan with TDD-style steps and execute via subagent-driven-development like P1.

Pending answer to one open question before plan expansion:

**Q1:** Should the stream-stall retry retain partial output, or restart the stream from the beginning of the round?
- **Option A (restart):** simpler; user sees the stream clear and re-fill. ✓ recommended for first cut.
- **Option B (resume):** requires server-side support for resuming from a sequence cursor; OpenAI does not expose this. Skip unless we add our own diff-based recovery.

Default unless told otherwise: **Option A**.
