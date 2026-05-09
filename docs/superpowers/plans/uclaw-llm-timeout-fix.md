# LLM Timeout / Stream-Error Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing 60s-total-timeout-on-streaming + auto-fallback-to-non-streaming + 180s-outer-cap with the structurally correct three-layer model from the RCA: short connect timeout + per-chunk stall timeout + classified stream-error retry + configurable outer agent-loop cap.

**Architecture:** Bottom-up — fix the HTTP layer first (`openai.rs` / `anthropic.rs`), then the dispatcher's classify+retry logic, then the outer Tauri-command timeout, then the frontend error UX. Each task is a single commit; bisectable on every commit `cargo build` and `tsc --noEmit` stay green.

**Tech Stack:** Rust (`reqwest 0.12`, `tokio`, `thiserror`, `futures`), TypeScript / React. No new dependencies for the Rust side; frontend uses existing `sonner` toasts and Jotai atoms.

**Reference spec:** `docs/superpowers/specs/2026-05-09-llm-timeout-rca.md` — read it first; all tasks below assume that context.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p-llm-timeout-fix
```

- [ ] **Step 0.2: Capture baseline behavior (so we can compare after)**

```bash
cd src-tauri && cargo build 2>&1 | tail -3
grep -nE "DEFAULT_TIMEOUT_SECS|MAX_RETRIES|tokio::time::timeout" src/llm/providers/openai.rs src/llm/providers/anthropic.rs src/agent/dispatcher.rs src/tauri_commands.rs > /tmp/llm-timeout-baseline.txt
wc -l /tmp/llm-timeout-baseline.txt
```

Save this snapshot — useful for the PR description's "before/after" section.

---

## Task 1: Add `Error::StreamStalled` variant + classifier helper

**Files:**
- Modify: `src-tauri/src/error.rs` (add variant)
- Create: `src-tauri/src/llm/stream_error.rs` (classifier)
- Modify: `src-tauri/src/llm/mod.rs` (re-export the classifier)

The classifier is the foundation everything else depends on. Build it in isolation, test-first, before touching providers or dispatcher.

- [ ] **Step 1.1: Add the new error variant**

Edit `src-tauri/src/error.rs`. Find the `pub enum Error { ... }` block (around line 37) and add this variant just before `Internal(String)`:

```rust
    #[error("LLM stream stalled — no data received in {duration:?}")]
    StreamStalled { duration: std::time::Duration },
```

- [ ] **Step 1.2: Verify it compiles**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|warning:" | head -3
```
Expected: 0 errors. (Build may show a missing-match-arm warning if any exhaustive `match Error` exists; if so, fix those at the same time before the commit.)

- [ ] **Step 1.3: Create the classifier module**

Create `src-tauri/src/llm/stream_error.rs`:

```rust
//! Classification of stream errors so the dispatcher can decide
//! whether to retry the stream, fail loudly, or treat as transient.
//!
//! See docs/superpowers/specs/2026-05-09-llm-timeout-rca.md §3.2.

use crate::error::Error;

/// Categorizes a stream error from an LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorKind {
    /// No bytes received within the stall window — connection healthy
    /// in some abstract sense but server stopped emitting. Always retry.
    Stalled,
    /// Connection reset, broken pipe, body decode error mid-stream, etc.
    /// Almost always recoverable on a fresh attempt. Retry up to N times.
    TransientNetwork,
    /// HTTP 4xx, model-not-found, auth failures, malformed requests.
    /// Will not succeed on retry. Surface immediately.
    Fatal,
}

/// Look at an `Error` and decide what kind of recovery (if any) makes sense.
pub fn classify_stream_error(err: &Error) -> StreamErrorKind {
    match err {
        Error::StreamStalled { .. } => StreamErrorKind::Stalled,
        Error::Internal(msg) => {
            let lower = msg.to_ascii_lowercase();
            // 4xx and explicit auth failures are fatal
            if lower.contains("status 400")
                || lower.contains("status 401")
                || lower.contains("status 403")
                || lower.contains("status 404")
                || lower.contains("status 422")
                || lower.contains("api error:") // OpenAI/Anthropic-side message
                || lower.contains("invalid api key")
                || lower.contains("model_not_found")
                || lower.contains("invalid_request_error")
            {
                return StreamErrorKind::Fatal;
            }
            // Body / connection errors → transient
            if lower.contains("error decoding response body")
                || lower.contains("connection reset")
                || lower.contains("broken pipe")
                || lower.contains("connection closed")
                || lower.contains("stream read error")
                || lower.contains("connection error")
                || lower.contains("timed out")
            {
                return StreamErrorKind::TransientNetwork;
            }
            // Default: treat unknowns as transient. We'd rather retry once
            // and lose a few seconds than surface an opaque failure to the
            // user that a retry would have cured.
            StreamErrorKind::TransientNetwork
        }
        Error::Llm(_) => StreamErrorKind::Fatal,
        // Default: anything else is transient.
        _ => StreamErrorKind::TransientNetwork,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stalled_is_stalled() {
        let err = Error::StreamStalled { duration: Duration::from_secs(45) };
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Stalled);
    }

    #[test]
    fn body_decode_is_transient() {
        let err = Error::Internal("OpenAI stream read error: error decoding response body".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn connection_reset_is_transient() {
        let err = Error::Internal("Anthropic connection reset by peer".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn auth_is_fatal() {
        let err = Error::Internal("OpenAI API error: invalid api key".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }

    #[test]
    fn status_401_is_fatal() {
        let err = Error::Internal("OpenAI API returned status 401".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }

    #[test]
    fn status_500_is_transient_via_default() {
        // 5xx isn't explicitly fatal; default fallthrough → transient.
        let err = Error::Internal("OpenAI API returned status 500".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn unknown_is_transient_default() {
        let err = Error::Internal("some other weird thing happened".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn llm_subtype_is_fatal() {
        let err = Error::Llm(crate::error::LlmError::ProviderNotConfigured("openai".into()));
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }
}
```

- [ ] **Step 1.4: Wire the new module into the llm module**

Edit `src-tauri/src/llm/mod.rs`. Add (or merge with existing module list):

```rust
pub mod stream_error;

pub use stream_error::{classify_stream_error, StreamErrorKind};
```

If `mod.rs` already has a `pub mod xxx; pub use xxx::Y;` pattern for sibling modules, follow that pattern.

- [ ] **Step 1.5: Run the tests**

```bash
cd src-tauri && cargo test --lib stream_error 2>&1 | tail -15
```
Expected: 8 passed (one per test function).

- [ ] **Step 1.6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/error.rs src-tauri/src/llm/stream_error.rs src-tauri/src/llm/mod.rs
git commit -m "feat(llm): add Error::StreamStalled + classify_stream_error helper

Foundation for the timeout-fix work (RCA spec
docs/superpowers/specs/2026-05-09-llm-timeout-rca.md §3.2).

- New Error::StreamStalled variant carries the stall duration so
  callers know how long they waited before giving up.
- classify_stream_error inspects an Error and returns one of
  Stalled / TransientNetwork / Fatal so the dispatcher can pick the
  right recovery (Tasks 7+).
- 8 unit tests covering stalled / body-decode / connection-reset /
  auth-error / status-4xx / status-5xx / unknown / Llm sub-error."
```

---

## Task 2: OpenAI provider — split timeout config into connect / stream-stall / complete-total

**Files:**
- Modify: `src-tauri/src/llm/providers/openai.rs`

Replace the single `timeout` field with three named durations: `connect_timeout` (used at client build time), `stream_stall_timeout` (per-chunk), `complete_timeout` (total, only for `complete()`).

- [ ] **Step 2.1: Update the constructor**

Edit `src-tauri/src/llm/providers/openai.rs`. Replace the constants and constructor:

Before:
```rust
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_RETRIES: u32 = 3;

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAIProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let raw = base_url.unwrap_or_else(|| "https://api.openai.com".into());
        let base = normalize_base_url(&raw);
        Self {
            api_key,
            base_url: base,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }
```

After:
```rust
/// Connect+TLS budget. Healthy OpenAI endpoints respond in <1s; longer means
/// firewall / DNS / proxy issue — fail fast so the user can fix it.
const CONNECT_TIMEOUT_SECS: u64 = 15;
/// Per-chunk stall timeout for streaming. If the server emits no bytes for
/// this long, declare the stream dead. Bounded by silence, not generation
/// length, so long-running streams are fine as long as they keep flowing.
const STREAM_STALL_TIMEOUT_SECS: u64 = 45;
/// Total request timeout for non-streaming complete(). Single round-trip;
/// no progress signal; bounded by what the model could realistically
/// generate in one shot.
const COMPLETE_TIMEOUT_SECS: u64 = 120;
const MAX_RETRIES: u32 = 3;

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    /// Per-chunk stall timeout. Used by the streaming SSE state machine.
    stream_stall_timeout: Duration,
    /// Total request timeout for non-streaming requests only.
    complete_timeout: Duration,
}

impl OpenAIProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let raw = base_url.unwrap_or_else(|| "https://api.openai.com".into());
        let base = normalize_base_url(&raw);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            // Client::builder() failing here would mean a fundamentally broken
            // tokio/reqwest install; not a runtime condition we should handle.
            .expect("reqwest::Client should build with default config");
        Self {
            api_key,
            base_url: base,
            client,
            stream_stall_timeout: Duration::from_secs(STREAM_STALL_TIMEOUT_SECS),
            complete_timeout: Duration::from_secs(COMPLETE_TIMEOUT_SECS),
        }
    }
```

- [ ] **Step 2.2: Update `send_with_retry` to take `is_stream` and only apply the total timeout for non-streams**

Find the `send_with_retry` function (around line 209). Change the signature + the `.timeout(...)` call:

```rust
async fn send_with_retry(
    &self,
    body: &serde_json::Value,
    is_stream: bool,
) -> Result<reqwest::Response, Error> {
    let mut last_error = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tracing::info!(attempt, delay_ms = delay.as_millis(), "Retrying OpenAI request");
            tokio::time::sleep(delay).await;
        }

        let mut req = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(body);

        // Only apply the total timeout for non-streaming requests.
        // Streaming requests rely on the per-chunk stall timeout enforced
        // by the SSE state machine — using a total timeout here would kill
        // the connection mid-stream regardless of model progress.
        if !is_stream {
            req = req.timeout(self.complete_timeout);
        }

        let result = req.send().await;
        // ... rest of the function body unchanged
    }
    // ...
}
```

(Leave the inner `match result { ... }` and retry classification logic exactly as it was.)

- [ ] **Step 2.3: Update the two callers**

In `complete()` (around line 273):
```rust
let resp = self.send_with_retry(&body, /* is_stream = */ false).await?;
```

In `stream()` (around line 354):
```rust
let resp = self.send_with_retry(&body, /* is_stream = */ true).await?;
```

- [ ] **Step 2.4: Pass `stream_stall_timeout` into the SSE state**

Find the `OpenAISseStream::new` call (around line 378-379) and the `OpenAiSseState::new` definition (around line 437). Currently both don't carry a stall duration. Update:

In `stream()`:
```rust
let byte_stream = resp.bytes_stream();
let stream = OpenAISseStream::new(byte_stream, self.stream_stall_timeout);
```

In `OpenAISseStream::new` definition:
```rust
fn new(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    stall_timeout: Duration,
) -> Self {
    Self {
        state: OpenAiSseState::new(byte_stream, stall_timeout),
    }
}
```

In `OpenAiSseState`:
```rust
struct OpenAiSseState {
    byte_stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    pending_finish_reason: Option<Option<String>>,
    accumulated_usage: Option<TokenUsage>,
    done: bool,
    stall_timeout: Duration,    // NEW
}

impl OpenAiSseState {
    fn new(
        byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
        stall_timeout: Duration,
    ) -> Self {
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            pending_finish_reason: None,
            accumulated_usage: None,
            done: false,
            stall_timeout,
        }
    }
```

- [ ] **Step 2.5: Add the per-chunk stall timeout in `poll_next`**

Find the `match self.byte_stream.next().await { ... }` block (around line 499). Wrap it in `tokio::time::timeout`:

Before:
```rust
match self.byte_stream.next().await {
    Some(Ok(bytes)) => {
        let text = String::from_utf8_lossy(&bytes);
        self.buffer.push_str(&text);
    }
    Some(Err(e)) => {
        self.done = true;
        return Some(Err(Error::Internal(format!(
            "OpenAI stream read error: {}",
            e
        ))));
    }
    None => {
        if !self.done {
            self.done = true;
            let finish_reason = self.pending_finish_reason.take()
                .flatten()
                .or_else(|| Some("stream_ended".into()));
            return Some(Ok(StreamDelta::Done {
                finish_reason,
                usage: self.accumulated_usage.take(),
            }));
        }
        return None;
    }
}
```

After:
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
        if !self.done {
            self.done = true;
            let finish_reason = self.pending_finish_reason.take()
                .flatten()
                .or_else(|| Some("stream_ended".into()));
            return Some(Ok(StreamDelta::Done {
                finish_reason,
                usage: self.accumulated_usage.take(),
            }));
        }
        return None;
    }
    Err(_elapsed) => {
        // Stall: server emitted no bytes within stall_timeout. Declare dead
        // so the dispatcher can decide to retry (it will, see Task 5).
        self.done = true;
        tracing::warn!(
            stall_secs = self.stall_timeout.as_secs(),
            "OpenAI stream stalled — no bytes received"
        );
        return Some(Err(Error::StreamStalled {
            duration: self.stall_timeout,
        }));
    }
}
```

- [ ] **Step 2.6: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|warning:" | head -5
```
Expected: 0 errors, 0 NEW warnings (existing warnings unchanged from baseline).

- [ ] **Step 2.7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/llm/providers/openai.rs
git commit -m "feat(llm/openai): split timeout into connect/stall/complete

Per RCA §3.1, replace the single 60s timeout (which was applied to
streaming requests as a TOTAL budget, killing healthy long
generations at 60s) with three structurally-correct durations:

- connect_timeout = 15s, set on the reqwest::Client itself. TCP+TLS
  must complete fast or the endpoint is misconfigured; fail-fast.
- stream_stall_timeout = 45s, enforced per-chunk in the SSE state
  machine via tokio::time::timeout(byte_stream.next()). Bounded by
  silence, not generation length — long streams that keep flowing
  succeed regardless of total duration.
- complete_timeout = 120s, applied per-request via .timeout() ONLY
  for non-streaming complete(). The single round-trip has no
  progress signal so a total cap is appropriate there.

send_with_retry now takes is_stream and applies .timeout() conditionally.

Stalls produce Error::StreamStalled (added Task 1) which the
dispatcher will classify and retry (Task 5)."
```

---

## Task 3: Anthropic provider — apply the same split

**Files:**
- Modify: `src-tauri/src/llm/providers/anthropic.rs`

Same changes as Task 2 but for Anthropic. The structure mirrors OpenAI almost exactly.

- [ ] **Step 3.1: Apply the constructor + send_with_retry + SSE state changes**

Apply all the same edits from Task 2 (Steps 2.1–2.5) to `src-tauri/src/llm/providers/anthropic.rs`, substituting:
- "OpenAI" → "Anthropic" in error messages
- The HTTP path is `/v1/messages` instead of `/v1/chat/completions`
- The state struct is `AnthropicSseState` (or whatever the existing name is)
- The response parsing is different but unrelated to timeouts — leave it

For the stall-error message:
```rust
return Some(Err(Error::StreamStalled {
    duration: self.stall_timeout,
}));
```

(Same variant as OpenAI — providers agree on the error type.)

- [ ] **Step 3.2: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|warning:" | head -5
```

- [ ] **Step 3.3: Commit**

```bash
git add src-tauri/src/llm/providers/anthropic.rs
git commit -m "feat(llm/anthropic): apply same connect/stall/complete split

Mirrors the OpenAI provider changes from the previous commit.
Both providers now produce Error::StreamStalled on per-chunk
stalls and use a 15s connect timeout."
```

---

## Task 4: Dispatcher — replace the bad fallback with classify+retry

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs`

Drop the auto-fallback to non-streaming `complete()`. Instead, classify the stream error and either retry the stream (transient/stalled) or surface immediately (fatal).

- [ ] **Step 4.1: Add the imports + constants**

At the top of `dispatcher.rs`, add:
```rust
use crate::llm::stream_error::{classify_stream_error, StreamErrorKind};
```

Near the other constants in the file (or at module top), add:
```rust
/// Maximum number of stream retries before surfacing the error.
/// Each retry only fires after a real stall or transient network error,
/// not on every iteration — so the worst-case wall time is bounded by
/// (stall_timeout + retry_overhead) × this many.
const MAX_STREAM_RETRIES: u32 = 2;
```

- [ ] **Step 4.2: Refactor the stream loop in `respond_to_user`**

Find the `match self.llm.stream(messages.clone(), tools.clone(), &config).await { ... }` block (around line 360-466 — the giant block containing the stream consumption + the `Err(e) => { fall back to complete }` arm).

The existing structure is:
```rust
match self.llm.stream(...).await {
    Ok(mut stream) => {
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => { ... }
                Err(e) => {
                    tracing::warn!("Stream error, falling back to complete: {}", e);
                    self.emit_stream_reset();
                    break;
                }
            }
        }
    }
    Err(e) => {
        tracing::warn!("Stream setup failed, using complete: {}", e);
    }
}

// Fallback to non-streaming complete
self.llm.complete(messages, tools, &config).await
```

Replace with a labeled retry loop:

```rust
let mut stream_retries: u32 = 0;
'stream_attempt: loop {
    match self.llm.stream(messages.clone(), tools.clone(), &config).await {
        Ok(mut stream) => {
            // Per-attempt accumulators — reset on each retry so we don't
            // mix partial output from a failed attempt into the next.
            let mut full_text = String::new();
            let mut full_thinking = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool: Option<(String, String, String)> = None;
            let mut thinking_started = false;
            let mut thinking_start_time: Option<std::time::Instant> = None;
            let mut metadata: Option<ResponseMetadata> = None;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(delta) => {
                        // ── existing per-delta handling unchanged ──
                        // (TextDelta / ThinkingDelta / ToolCallDelta / Done)
                        // copy verbatim from the current implementation
                    }
                    Err(e) => {
                        // Decide how to recover.
                        let kind = classify_stream_error(&e);
                        match kind {
                            StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork
                                if stream_retries < MAX_STREAM_RETRIES =>
                            {
                                tracing::warn!(
                                    error = %e,
                                    kind = ?kind,
                                    attempt = stream_retries + 1,
                                    max = MAX_STREAM_RETRIES,
                                    "Stream interrupted, retrying with a fresh stream"
                                );
                                self.emit_stream_reset();
                                stream_retries += 1;
                                // Brief backoff before retry
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    500 * 2u64.pow(stream_retries - 1),
                                )).await;
                                continue 'stream_attempt;
                            }
                            StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                tracing::error!(
                                    error = %e,
                                    retries = stream_retries,
                                    "Stream failed after exhausting retries"
                                );
                                self.emit_stream_reset();
                                return Err(e);
                            }
                            StreamErrorKind::Fatal => {
                                tracing::error!(error = %e, "Stream failed with fatal error");
                                self.emit_stream_reset();
                                return Err(e);
                            }
                        }
                    }
                }
            }

            // Stream completed successfully. Build and return the response.
            let metadata = metadata.unwrap_or_else(|| ResponseMetadata {
                model: self.model.clone(),
                finish_reason: Some("stream_ended".into()),
                usage: None,
            });
            let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };

            if !tool_calls.is_empty() {
                return Ok(RespondOutput::ToolCalls {
                    tool_calls,
                    text: if full_text.is_empty() { None } else { Some(full_text) },
                    thinking,
                    metadata,
                });
            } else {
                return Ok(RespondOutput::Text { text: full_text, thinking, metadata });
            }
        }
        Err(e) => {
            // stream() failed before producing any deltas. This is a setup
            // problem (auth, model-not-found, etc.) — classify and either
            // retry or fail. Do NOT auto-fallback to complete() — we want
            // the user to see the real error, not a workaround.
            let kind = classify_stream_error(&e);
            match kind {
                StreamErrorKind::TransientNetwork
                    if stream_retries < MAX_STREAM_RETRIES =>
                {
                    tracing::warn!(
                        error = %e,
                        attempt = stream_retries + 1,
                        "Stream setup failed transiently, retrying"
                    );
                    stream_retries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        500 * 2u64.pow(stream_retries - 1),
                    )).await;
                    continue 'stream_attempt;
                }
                _ => {
                    tracing::error!(error = %e, "Stream setup failed, surfacing error");
                    return Err(e);
                }
            }
        }
    }
}
```

**Important:** Copy the existing per-delta handling (TextDelta / ThinkingDelta / ToolCallDelta / Done arms) verbatim into the new `Ok(delta) => { ... }` arm. Don't rewrite that logic in this task — it's working correctly post-PR-#13.

- [ ] **Step 4.3: Remove the now-dead non-streaming fallback at the bottom of the function**

The `// Fallback to non-streaming complete\n self.llm.complete(messages, tools, &config).await` line (around old line 469) is no longer reachable (every path inside the loop now returns). Delete it entirely.

- [ ] **Step 4.4: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|warning:" | head -10
```
Expected: 0 errors. May see "unused variable" warnings if the refactor accidentally left some — fix in place before committing.

- [ ] **Step 4.5: Add unit tests for the dispatcher's classify+retry**

The existing dispatcher logic doesn't have tests because it depends on a real `LlmProvider` trait + Tauri AppHandle. Adding integration-grade tests is too much scope for this task — the classifier itself IS tested in Task 1. We're verifying via the build + the smoke test in Task 8. (P3 will eventually add proper React-side tests; the Rust dispatcher gains its tests when we have a mock provider trait, which is its own follow-up.)

If a reviewer wants a deterministic check that the new logic compiles and the classifier matches, point them at `src-tauri/src/llm/stream_error.rs::tests` — those exhaustively cover the kind classification.

- [ ] **Step 4.6: Commit**

```bash
git add src-tauri/src/agent/dispatcher.rs
git commit -m "feat(dispatcher): classify+retry stream errors instead of falling back

Per RCA §3.2. The previous behavior on stream error was to silently
fall back to non-streaming complete() — same underlying timeout,
double wall time, erased streaming UX. Replace with:

- classify_stream_error categorizes the error: Stalled /
  TransientNetwork / Fatal
- Stalled/TransientNetwork retry the *stream* itself (up to
  MAX_STREAM_RETRIES = 2, exponential backoff) — preserves UX, bounded
  wall time
- Fatal returns immediately (4xx auth / model errors won't recover)
- complete() is no longer auto-invoked on stream failure; it remains
  available for callers that explicitly want non-streaming

emit_stream_reset still fires on retry so the frontend can clear
partial deltas before the new stream starts."
```

---

## Task 5: Make the outer agent-loop timeout configurable

**Files:**
- Modify: `src-tauri/src/memubot_config.rs` (add config field)
- Modify: `src-tauri/src/tauri_commands.rs` (use the config)

The 180s outer cap is the wrong number. Bump default to 600s (10 min) and make it configurable via the existing config struct.

- [ ] **Step 5.1: Add the config field**

Edit `src-tauri/src/memubot_config.rs`. Locate the main config struct (`MemubotConfig` or similar) and add a field:

```rust
/// Maximum wall-clock seconds the agent loop may run for a single
/// user message before forcibly terminating. Default 600s (10 min).
/// Override via settings → Advanced.
#[serde(default = "default_agent_loop_timeout_secs")]
pub agent_loop_timeout_secs: u64,
```

Add the default fn near the other defaults:
```rust
fn default_agent_loop_timeout_secs() -> u64 { 600 }
```

If the struct uses `#[derive(Default)]` and an explicit `impl Default`, also add the field initializer there:
```rust
agent_loop_timeout_secs: 600,
```

Locate the config-read site in `tauri_commands.rs` or wherever `MemubotConfig` is parsed; ensure deserializing an existing config without this field uses the default (the `#[serde(default)]` covers that).

- [ ] **Step 5.2: Use the config value in send_agent_message**

Edit `src-tauri/src/tauri_commands.rs`. Find the existing 180s timeout (around line 2747):

```rust
let outcome = tokio::select! {
    result = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut ctx, &config)
    ) => match result {
        Ok(o) => o,
        Err(_) => {
            tracing::error!(session_id = %session_id, "Agentic loop timed out after 180s");
            // ...
        }
    },
```

Replace `180` with the config value. Since this code is inside `send_agent_message` which already has `state: State<AppState>`, read the config:

```rust
let agent_loop_timeout_secs = state.memubot_config.read().await.agent_loop_timeout_secs;

let outcome = tokio::select! {
    result = tokio::time::timeout(
        std::time::Duration::from_secs(agent_loop_timeout_secs),
        crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut ctx, &config)
    ) => match result {
        Ok(o) => o,
        Err(_) => {
            tracing::error!(
                session_id = %session_id,
                timeout_secs = agent_loop_timeout_secs,
                "Agentic loop timed out"
            );
            let _ = app_handle.emit("chat:stream-error", serde_json::json!({
                "conversationId": session_id,
                "error": format!(
                    "Request timed out after {}s. The agent may have been working on a complex task; try increasing the timeout in Settings → Advanced.",
                    agent_loop_timeout_secs
                ),
                "kind": "outer_timeout",
                "timeoutSecs": agent_loop_timeout_secs,
            }));
            // ... rest of the timeout-handling block unchanged
        }
    },
```

(The `kind` and `timeoutSecs` fields are new — used by the frontend in Task 6.)

If `state.memubot_config` is not the actual field name, search for how the config is currently accessed:
```bash
grep -n "memubot_config\|MemubotConfig" src-tauri/src/tauri_commands.rs | head -5
```
…and use whatever pattern is established (RwLock vs. Mutex, `read()` vs. `lock()`, etc.).

- [ ] **Step 5.3: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|warning:" | head -5
```

- [ ] **Step 5.4: Commit**

```bash
git add src-tauri/src/memubot_config.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(agent): make outer loop timeout configurable, default 600s

Per RCA §3.3. The hard-coded 180s cap is shorter than the worst-
case retry math (60s × 3 = 180s for one fallback cycle), so users
always see the outer 'Request timed out' message regardless of
the actual underlying cause.

- New MemubotConfig.agent_loop_timeout_secs (default 600)
- Read at send_agent_message entry; no need to restart for new
  values to take effect on next message
- Timeout error payload now includes kind: 'outer_timeout' and
  timeoutSecs so the frontend can render a more helpful message
  (Task 6) and offer a 'continue' button"
```

---

## Task 6: Frontend — categorize stream errors + render improved UX

**Files:**
- Modify: `ui/src/hooks/useGlobalAgentListeners.ts` (or wherever `chat:stream-error` is consumed)
- Modify: `ui/src/atoms/agent-atoms.ts` (extend `agentStreamErrorsAtom` to carry `kind`)
- Modify: `ui/src/components/agent/AgentMessages.tsx` (render the differentiated states)

Three states to distinguish in UX (currently they collapse to a generic toast):

| Backend `kind` | UX |
|---|---|
| `outer_timeout` | Inline error message on the streaming bubble + "继续运行" button (re-trigger same prompt). |
| `stream_stalled` (auto-retrying) | Inline "重新连接中..." badge while retry runs. No toast. |
| `stream_failed` (after retries exhausted) | Inline error with "重试" button. |
| `fatal` (auth / 4xx) | Toast with link to Settings. |

For now, implement the simplest version: read the new `kind` field, render different inline messages, and keep the existing toast as the fallback for `fatal`.

- [ ] **Step 6.1: Extend the error atom**

Edit `ui/src/atoms/agent-atoms.ts`. Find `agentStreamErrorsAtom`:

Before:
```ts
export const agentStreamErrorsAtom = atom<Map<string, string>>(new Map())
```

After:
```ts
export interface AgentStreamErrorPayload {
  message: string
  kind?: 'outer_timeout' | 'stream_stalled' | 'stream_failed' | 'fatal'
  timeoutSecs?: number
}

export const agentStreamErrorsAtom = atom<Map<string, AgentStreamErrorPayload>>(new Map())
```

Search for callers that read this atom and update them to handle the new shape. Most should just use `.message`:
```bash
grep -rn "agentStreamErrorsAtom" ui/src/
```

For each match, replace `error: string | undefined` style usages with `payload?.message`.

- [ ] **Step 6.2: Update the listener to capture `kind`**

Edit `ui/src/hooks/useGlobalAgentListeners.ts`. Find the `chat:stream-error` handler:

Before:
```ts
listen<{ conversationId: string; error: string }>('chat:stream-error', ({ payload }) => {
  // ... store.set(agentStreamErrorsAtom, ...) with payload.error
})
```

After:
```ts
listen<{
  conversationId: string
  error: string
  kind?: 'outer_timeout' | 'stream_stalled' | 'stream_failed' | 'fatal'
  timeoutSecs?: number
}>('chat:stream-error', ({ payload }) => {
  const sid = payload.conversationId
  store.set(agentStreamErrorsAtom, (prev) => {
    const next = new Map(prev)
    next.set(sid, {
      message: payload.error,
      kind: payload.kind,
      timeoutSecs: payload.timeoutSecs,
    })
    return next
  })
  // existing streaming-state cleanup unchanged
})
```

- [ ] **Step 6.3: Render the differentiated UX in AgentMessages**

Find the existing error rendering in `AgentMessages.tsx` (search for `agentStreamErrorsAtom`). Render based on `kind`:

```tsx
{streamError && (
  <div className="rounded-md border border-destructive/40 bg-destructive/[0.04] p-3 mt-2">
    <div className="flex items-start gap-2">
      <AlertTriangle className="size-4 text-destructive shrink-0 mt-0.5" />
      <div className="flex-1 text-sm text-foreground/85">
        {streamError.message}
        {streamError.kind === 'outer_timeout' && streamError.timeoutSecs && (
          <div className="mt-1 text-xs text-muted-foreground">
            提示：可在 设置 → 高级 中调整 Agent 循环超时（当前 {streamError.timeoutSecs}s）。
          </div>
        )}
      </div>
      {(streamError.kind === 'outer_timeout' || streamError.kind === 'stream_failed') && onRetry && (
        <Button variant="outline" size="sm" onClick={onRetry}>
          重试
        </Button>
      )}
    </div>
  </div>
)}
```

The `streamError` value is `agentStreamErrorsAtom.get(sessionId)`, with the new shape. `onRetry` is the existing callback the parent already passes.

- [ ] **Step 6.4: Type-check + build**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
npx vite build 2>&1 | tail -3
```

- [ ] **Step 6.5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/agent-atoms.ts ui/src/hooks/useGlobalAgentListeners.ts ui/src/components/agent/AgentMessages.tsx
git commit -m "feat(ui): differentiate stream-error UX by kind

Reads the new 'kind' field on chat:stream-error events (added in
backend Tasks 4 + 5) and renders a more helpful inline message:

- outer_timeout → message + hint about Settings → Advanced timeout
  + 重试 button
- stream_failed (after retries) → message + 重试 button
- fatal → message (no retry; 4xx won't recover)
- stream_stalled (auto-retrying) — backend doesn't surface this to
  the user since retries are silent; reserved field

agentStreamErrorsAtom now stores AgentStreamErrorPayload (was string)
to carry kind + timeoutSecs alongside the message."
```

---

## Task 7: Final verification

- [ ] **Step 7.1: Cargo build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -c "^warning:" 
```
Expected: 0 (or matches baseline before this work).

```bash
cargo test --lib stream_error 2>&1 | tail -5
```
Expected: 8 passed.

- [ ] **Step 7.2: TS clean + Vite build**

```bash
cd ../ui && npx tsc --noEmit 2>&1 | head -3 && npx vite build 2>&1 | tail -3
```
Expected: zero errors, build succeeds.

- [ ] **Step 7.3: Verify no lingering references to the old fallback path**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -n "Stream error, falling back to complete\|fallback to non-streaming complete" src-tauri/src/agent/dispatcher.rs
```
Expected: empty output.

```bash
grep -n "DEFAULT_TIMEOUT_SECS" src-tauri/src/llm/providers/
```
Expected: empty (the constant was renamed and split).

- [ ] **Step 7.4: Manual smoke test**

(Optional — covers behavior the unit tests can't verify against a real provider.)

```bash
cd src-tauri && cargo tauri dev
```

In the running app:
1. Send a message that triggers a quick LLM call (e.g. "what is 2+2"). Expect it succeeds in <5s as before.
2. Send a message that triggers a long agent task ("write a fibonacci function in 5 different languages with detailed comments"). Expect it runs to completion past the old 60s ceiling.
3. Disconnect from network mid-stream (turn off Wi-Fi for 5s, then back on). Expect the stream retries automatically once and continues. (Up to 2 retries.)
4. Set `agent_loop_timeout_secs = 30` in `~/.uclaw/config.json`. Send a long task. Expect the new error UX shows the timeout-secs hint.

- [ ] **Step 7.5: Push branch + open PR**

```bash
git push -u origin claude/p-llm-timeout-fix
gh pr create --title "Fix: LLM timeout / stream-error structural fix (RCA-driven)" --body "$(cat <<'EOF'
## Summary

Implements the fix specified in `docs/superpowers/specs/2026-05-09-llm-timeout-rca.md`. Replaces the broken "60s total timeout on streaming + auto-fallback to non-streaming + 180s outer cap" with three structurally-correct timeout layers + classified stream-error retry.

## Roll-up

| Concern | Before | After |
|---|---|---|
| Streaming generations >60s | always fail | succeed indefinitely (bounded only by per-chunk stalls) |
| Stream connection blip | erases streaming UX, fallback to complete() | auto-retries stream up to 2x, UX preserved |
| Connect to bad endpoint | could hang for 60s | fails fast in 15s |
| Agent loop wall-clock | hard 180s | configurable, default 600s |
| Auth / 4xx errors | retried 3x then surfaced as opaque "API error" | surfaced immediately as Fatal with no retry |

## What's in this batch

| Task | Hash | What |
|---|---|---|
| 1 | TBD | feat(llm): Error::StreamStalled + classify_stream_error helper |
| 2 | TBD | feat(llm/openai): split timeout into connect/stall/complete |
| 3 | TBD | feat(llm/anthropic): same split |
| 4 | TBD | feat(dispatcher): classify+retry instead of fallback |
| 5 | TBD | feat(agent): make outer loop timeout configurable, default 600s |
| 6 | TBD | feat(ui): differentiate stream-error UX by kind |

## Test plan

Unit tests (Task 1): 8 tests for classify_stream_error covering Stalled / body-decode / connection-reset / auth / status-4xx / status-5xx / unknown / Llm sub-error.

Integration (manual smoke):
- [ ] Quick prompt completes as before
- [ ] Long agent task (multi-iteration code generation) succeeds past 60s
- [ ] Network blip mid-stream auto-recovers within ≤45s
- [ ] Wrong API key fails immediately (no 3-retry wait)
- [ ] Custom `agent_loop_timeout_secs` in config takes effect on next message

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria (cumulative)

- ✅ `cargo build` clean
- ✅ `cargo test --lib stream_error` — 8 tests passing
- ✅ `tsc --noEmit` clean
- ✅ `vite build` succeeds
- ✅ No remaining `"Stream error, falling back to complete"` log line
- ✅ No remaining `DEFAULT_TIMEOUT_SECS` constant in provider files
- ✅ A 90-second streaming generation completes
- ✅ A network blip mid-stream auto-recovers
- ✅ A 4xx response fails immediately (no retry)
- ✅ Outer timeout configurable via `agent_loop_timeout_secs`
- ✅ Frontend renders a distinct UX for `outer_timeout` errors with a settings hint

## Out of scope

- Settings UI for `agent_loop_timeout_secs` — config can be edited in `~/.uclaw/config.json` for now; UI exposure is a follow-up (~30min).
- Stream resume from sequence cursor — RCA Q1 was answered "restart on retry"; no resume layer needed.
- Provider-side mock-server integration tests — too much infra for this PR. The classifier is unit-tested; the dispatcher relies on the smoke test for now. P3 (frontend tests) doesn't cover Rust, but a future Rust mock-provider test plan would.
- Per-tool / per-step timeout granularity — this PR's outer cap is per-message-loop. Per-step timing belongs to a separate observability plan if it becomes a need.
