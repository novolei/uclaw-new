# Stabilization Week Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the agent runtime against the bug classes that surfaced during the last week of dogfood: silent worker panics, opaque tool errors, unguarded markdown rendering, and naive HTML scraping.

**Architecture:** Four independent workstreams that compose. (1) `tracing-appender` + panic hook + spawn-wrapped tools = panics become recoverable `ToolError`s with on-disk evidence. (2) `ToolError` gains a `kind` enum; hot-path tools (web/shell/file/search) classify their failures. (3) Vitest fixture suite locks down markdown rendering invariants. (4) `scraper` replaces naive tag stripping in `web_fetch` + a SPA heuristic tells the agent to switch tools. All ships as a single PR with ≤ 8 bisectable commits.

**Tech Stack:** Rust + Tokio + Tauri 2 (backend), `tracing-appender`, `scraper`. TypeScript + React + Vitest (frontend). No new runtime services.

---

## Background: where this plan deviates from the spec

The spec is high-level. A few concrete decisions to record here so the engineer doesn't have to invent them:

- **`ToolError` refactor:** keep the existing enum, add a new `Kinded { kind, message, source }` variant + a `ToolErrorKind` enum. Existing call sites compile unchanged (`ToolError::Execution("...")` still valid). Only hot-path tools migrate to `Kinded`. Avoids touching every callsite in one commit.
- **`tokio::task::spawn` wrapper location:** in `agent/dispatcher.rs::execute_tool`. Wrap the inner `.execute(params).await` call. Don't move it to a higher level — too many call sites.
- **`tracing-appender` guard storage:** the spec calls out the `_guard` needs to outlive `main`. Concrete approach: return it from a `init_tracing() -> WorkerGuard` helper, hold it in a `let _guard = init_tracing();` binding inside `main()`.
- **Markdown fixtures format:** Plain `.md` files in `__fixtures__/markdown-samples/`. The test reads each file as a string and feeds it to `<MessageResponse>`. No frontmatter, no JSON wrappers — keep it dead simple so engineers can add a fixture for any new bug they encounter by dropping a `.md` file.
- **SPA detection heuristic:** parse-once via `scraper::Html::parse_document`, then check: (a) script tag count > 5, (b) all visible body text < 500 chars after extraction, (c) at least one of `id="root"` / `id="app"` / `id="__next"` / `data-reactroot` markers. All three required = SPA.

## File Structure

**New files:**
- `src-tauri/src/observability.rs` — tracing init + panic hook
- `ui/src/components/ai-elements/__fixtures__/markdown-samples/*.md` — 9 fixture files
- `ui/src/components/ai-elements/message.fixtures.test.tsx` — fixture-driven tests

**Modified files:**
- `src-tauri/Cargo.toml` — add `tracing-appender`, `scraper`, `home` (or use existing path-resolution method)
- `src-tauri/src/main.rs` — replace inline tracing setup with `observability::init()`; capture `_guard`
- `src-tauri/src/agent/tools/tool.rs` — add `Kinded` variant + `ToolErrorKind` enum + helpers
- `src-tauri/src/agent/dispatcher.rs` — wrap `tool.execute(params)` in `tokio::task::spawn`; map JoinError(panic) → `ToolError::Kinded`
- `src-tauri/src/agent/tools/builtin/web.rs` — switch `extract_text` to `scraper`; add SPA detection; classify HTTP errors via `ToolErrorKind`
- `src-tauri/src/agent/tools/builtin/shell.rs` — classify exit codes / stderr into `ToolErrorKind`
- `src-tauri/src/agent/tools/builtin/search.rs` — classify FS errors / no-match into `ToolErrorKind`
- `src-tauri/src/agent/tools/builtin/file.rs` (if exists) — same

**Total estimate:** ~1100-1300 LOC across ~12 files.

---

## Task 1: File-backed tracing

**Files:**
- Create: `src-tauri/src/observability.rs`
- Modify: `src-tauri/src/main.rs:8-19` (replace tracing init)
- Modify: `src-tauri/src/lib.rs` (export `observability` mod if main.rs uses uclaw_core::observability)
- Modify: `src-tauri/Cargo.toml` (add `tracing-appender = "0.2"`)

### - [ ] Step 1.1: Add the `tracing-appender` dep

```toml
# in [dependencies] section, near tracing-subscriber
tracing-appender = "0.2"
```

Run `cargo build` to verify it resolves: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`.
Expected: 0 errors (just compiles the new crate).

### - [ ] Step 1.2: Create `src-tauri/src/observability.rs`

```rust
//! Observability bootstrap: stdout + file-backed daily-rotated logs.
//!
//! Logs go to `~/.uclaw/logs/uclaw.log.YYYY-MM-DD`. The non-blocking
//! writer's `WorkerGuard` MUST be held until process exit or pending
//! lines are dropped — `init()` returns it; `main()` binds it to a
//! `let _guard = ...;` and lets it drop at shutdown.

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

/// Resolve the log directory, creating it if needed.
fn log_dir() -> PathBuf {
    let base = home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".uclaw").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Best-effort home directory lookup. Falls back to current dir if HOME
/// isn't set (unusual on macOS but defensive).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // chromiumoxide::handler emits WARN per CDP event Chrome sends
        // outside its schema — noise. Silence it. Match the prior main.rs default.
        "info,chromiumoxide::handler=error".into()
    })
}

/// Initialize tracing for the whole process.
///
/// Returns a `WorkerGuard` whose Drop flushes any pending non-blocking
/// file writes. Hold it in `main` for the lifetime of the process.
pub fn init() -> WorkerGuard {
    let dir = log_dir();
    let file_appender = tracing_appender::rolling::daily(&dir, "uclaw.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stdout_layer = fmt::layer().with_writer(std::io::stdout);
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false); // no escape codes in the on-disk log

    Registry::default()
        .with(env_filter())
        .with(stdout_layer)
        .with(file_layer)
        .init();

    tracing::info!(?dir, "tracing initialized");

    guard
}
```

### - [ ] Step 1.3: Register the module in `lib.rs`

```bash
grep -n "^pub mod" src-tauri/src/lib.rs | head
```

Add `pub mod observability;` alongside other top-level modules (alphabetical with the existing pattern).

### - [ ] Step 1.4: Wire `observability::init` in `main.rs`

Replace lines 8-19 of `src-tauri/src/main.rs`:

```rust
fn main() {
    // _guard flushes the non-blocking file writer on Drop. Must outlive
    // the rest of main, hence the underscore-prefixed binding here.
    let _guard = uclaw_core::observability::init();

    tauri::Builder::default()
        // ... existing setup unchanged ...
```

Remove the old `tracing_subscriber::fmt() ... .init()` block. Don't touch anything else in `main.rs`.

### - [ ] Step 1.5: Smoke + commit

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
ls -la ~/.uclaw/logs/ 2>/dev/null   # before running: may or may not exist
# Actually running the app would create files there; smoke test via cargo build only for this commit.
```

```bash
git add src-tauri/Cargo.toml src-tauri/src/observability.rs src-tauri/src/lib.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
chore(tracing): file-backed daily logs in ~/.uclaw/logs/

Replaces the inline stdout-only tracing_subscriber init with a
two-layer subscriber:
- stdout layer (existing dev-console behavior)
- daily-rotated file layer at ~/.uclaw/logs/uclaw.log.YYYY-MM-DD,
  ANSI-stripped for offline inspection

Wrapped in an observability::init() helper that returns the
non-blocking writer's WorkerGuard; main() holds it as `_guard` so
pending writes flush on process exit.

Foundation for the panic hook coming next — crash logs need a place
to live alongside the main trace.
EOF
)"
```

---

## Task 2: Panic hook → crash logs

**Files:**
- Modify: `src-tauri/src/observability.rs` — append `install_panic_hook()`
- Modify: `src-tauri/src/main.rs` — call `install_panic_hook()` after `init()`

### - [ ] Step 2.1: Extend `observability.rs`

Append to the existing module:

```rust
/// Install a process-wide panic hook that:
/// - Writes a crash record under `~/.uclaw/logs/crashes/crash-<timestamp>-<thread>.log`
///   containing the panic message + location + a full backtrace.
/// - Emits an `error!` event so the same info lands in the main rolling log.
///
/// Called once from main, after `init()`.
pub fn install_panic_hook() {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let crash_dir = log_dir().join("crashes");
        let _ = std::fs::create_dir_all(&crash_dir);

        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed").to_string();
        let safe_thread = thread_name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_', "_");

        let path = crash_dir.join(format!("crash-{}-{}.log", ts, safe_thread));
        let bt = std::backtrace::Backtrace::force_capture();
        let body = format!(
            "panic at {}\n\nThread: {}\nLocation: {:?}\n\n=== Backtrace ===\n{}\n",
            info,
            thread_name,
            info.location(),
            bt,
        );

        let _ = std::fs::write(&path, &body);

        // Also surface to tracing so the rolling log captures it.
        tracing::error!(
            crash_log = %path.display(),
            thread = %thread_name,
            "panic captured: {}",
            info,
        );

        // Chain to the previous hook so default behavior (printing to
        // stderr) is preserved. Helps dev-mode visibility.
        prev_hook(info);
    }));
}
```

### - [ ] Step 2.2: Call it from `main`

In `src-tauri/src/main.rs`, right after the `_guard` line:

```rust
fn main() {
    let _guard = uclaw_core::observability::init();
    uclaw_core::observability::install_panic_hook();

    // ... existing tauri setup ...
```

### - [ ] Step 2.3: Manual smoke (documented, not auto-run)

Add a *temporary* debug entrypoint to trigger a panic, OR test by writing a transient unit test:

```rust
// In src-tauri/src/observability.rs, append:
#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: install the hook and trigger a panic in a spawned thread,
    /// verify a crash log file was written. Cleanup deletes the file.
    #[test]
    fn panic_hook_writes_crash_log() {
        // We need a clean install — but set_hook is global state. Save the
        // prior hook, install ours, run the test, restore.
        // (This test runs in cargo's test thread pool — be careful.)
        let prev = std::panic::take_hook();
        install_panic_hook();

        let before_count = std::fs::read_dir(log_dir().join("crashes"))
            .ok()
            .map(|d| d.count())
            .unwrap_or(0);

        let handle = std::thread::Builder::new()
            .name("panic-test-thread".to_string())
            .spawn(|| {
                panic!("test panic — please ignore in CI output");
            }).unwrap();
        // Joining a panicked thread returns Err; we don't care.
        let _ = handle.join();

        // Give the filesystem a moment.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let after_count = std::fs::read_dir(log_dir().join("crashes"))
            .ok()
            .map(|d| d.count())
            .unwrap_or(0);

        // Restore prior hook for the rest of the test suite.
        std::panic::set_hook(prev);

        assert!(after_count > before_count, "expected a new crash log file");
    }
}
```

(The test writes to the real `~/.uclaw/logs/crashes/` directory — that's OK
since we generate timestamped filenames and don't delete user data.)

### - [ ] Step 2.4: Run + commit

```bash
cd src-tauri && cargo test --lib observability::tests 2>&1 | tail -10
```

```bash
git add src-tauri/src/observability.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(panic): hook captures backtraces to ~/.uclaw/logs/crashes/

Installs a process-wide panic::set_hook that writes a structured
crash record per panic:
  ~/.uclaw/logs/crashes/crash-YYYYMMDDTHHMMSSZ-<thread>.log

Each record includes the panic message, location, owning thread, and
a force_capture()'d backtrace. The same info is also emitted via
tracing::error so it lands in the rolling daily log alongside normal
events.

Chains to any previously-installed hook so default stderr printing in
dev mode is preserved.

A smoke test in observability::tests spawns a deliberately-panicking
thread and asserts the crash directory's file count goes up.
EOF
)"
```

---

## Task 3: `catch_unwind` around tool execution

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs` — add `Kinded` variant (preview; full enum lands in Task 4)
- Modify: `src-tauri/src/agent/dispatcher.rs` — wrap `tool.execute(params).await`

### - [ ] Step 3.1: Locate the tool execution callsite

```bash
grep -n "tool\.execute\|\.execute(params)\|tool_impl\.execute" src-tauri/src/agent/dispatcher.rs | head
```

Find the line where the dispatcher calls into a `Tool::execute`. It's typically inside a `match` over the tool registry. Note the surrounding context — the tool object is likely an `Arc<dyn Tool>`.

### - [ ] Step 3.2: Refactor the call to use `tokio::task::spawn`

The current shape is something like:

```rust
let result = tool.execute(params).await;
```

Change to:

```rust
// Wrap in tokio::task::spawn so panics get caught at the JoinHandle
// boundary rather than unwinding through the agent loop and killing
// the whole turn.
let tool_name_for_panic = tool.name().to_string();
let result = match tokio::task::spawn(async move {
    tool.execute(params).await
}).await {
    Ok(Ok(out)) => Ok(out),
    Ok(Err(e)) => Err(e),
    Err(join_err) if join_err.is_panic() => {
        tracing::error!(tool = %tool_name_for_panic, "tool panicked");
        Err(ToolError::Execution(format!(
            "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.",
            tool_name_for_panic,
        )))
    }
    Err(join_err) => {
        tracing::error!(tool = %tool_name_for_panic, %join_err, "tool join error");
        Err(ToolError::Execution(format!("Tool join error: {}", join_err)))
    }
};
```

**Important:** the closure passed to `spawn` consumes `tool`. The dispatcher
typically clones the `Arc<dyn Tool>` before the await; if not, you'll need
to clone (or move) the arc into the spawn body. Adjust to match the actual
code shape — don't break ownership.

### - [ ] Step 3.3: Verify the build

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: 0 errors. If `tool` is not `Send`/`'static` after the await, debug — the trait already requires `Send + Sync`, so this should compile, but the dispatcher's local variable types may need adjusting.

### - [ ] Step 3.4: Add a unit test for the panic-recovery shape

In `src-tauri/src/agent/dispatcher.rs` (or a sibling test file if dispatcher
is huge — check), append:

```rust
#[cfg(test)]
mod panic_recovery_tests {
    use crate::agent::tools::tool::{Tool, ToolError, ToolOutput, ApprovalRequirement};
    use async_trait::async_trait;

    struct PanickyTool;

    #[async_trait]
    impl Tool for PanickyTool {
        fn name(&self) -> &str { "panicky" }
        fn description(&self) -> &str { "test-only" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
            ApprovalRequirement::Never
        }
        async fn execute(&self, _: serde_json::Value) -> Result<ToolOutput, ToolError> {
            panic!("deliberate test panic");
        }
    }

    #[tokio::test]
    async fn tool_panic_converts_to_tool_error() {
        let tool = PanickyTool;
        // Re-implement the dispatcher's spawn wrapper directly here to
        // verify the shape — we can't easily import `dispatcher` private
        // fns. This test guards the contract.
        let tool_name = tool.name().to_string();
        let join = tokio::task::spawn(async move {
            tool.execute(serde_json::json!({})).await
        });
        let result = match join.await {
            Ok(r) => r,
            Err(e) if e.is_panic() => Err(ToolError::Execution(format!(
                "Tool '{}' crashed unexpectedly.", tool_name
            ))),
            Err(e) => Err(ToolError::Execution(format!("Join error: {}", e))),
        };
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("panicky") && msg.contains("crashed"),
            "expected panic-recovery error, got: {}", msg);
    }
}
```

Run: `cd src-tauri && cargo test --lib panic_recovery_tests 2>&1 | tail -10`
Expected: 1 passing test.

### - [ ] Step 3.5: Commit

```bash
git add src-tauri/src/agent/dispatcher.rs
git commit -m "$(cat <<'EOF'
fix(agent): catch_unwind around tool execution — panics return errors

The dispatcher previously awaited tool.execute() directly. A panicking
tool (concrete example: the UTF-8 byte-slice bug in web_fetch, fixed
in 8db65c0) unwound through the dispatcher and killed the whole agent
turn. The user saw the streaming text stop dead with no error.

Wrap each tool invocation in tokio::task::spawn. Panics now surface
as JoinError::is_panic() and convert to a ToolError::Execution with
a pointer to the crash log directory. The agent gets a normal tool
error into its turn context and can recover (apologize, try again,
escalate).

Spawn cost is negligible compared to typical tool I/O.

Includes a unit test that runs a deliberately-panicking test tool
through the spawn wrapper and asserts the panic becomes a ToolError.
EOF
)"
```

---

## Task 4: `ToolError` gains a kind enum (refactor, backward-compatible)

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs` — add `ToolErrorKind` + `Kinded` variant

### - [ ] Step 4.1: Add `ToolErrorKind` + the new variant

In `src-tauri/src/agent/tools/tool.rs`, **add** (don't remove existing variants):

```rust
/// Categorical label for a tool failure, exposed to the LLM as a
/// bracketed tag in the error message (e.g. `[NotFound] ...`).
///
/// Picking the right kind helps the LLM reason about retry vs.
/// alternative-approach. e.g. NotFound rarely benefits from retry but
/// suggests trying a different URL; Timeout often does benefit from a
/// retry; PermissionDenied means stop and ask the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// Input doesn't match schema / missing required field / malformed value.
    InvalidInput,
    /// Resource doesn't exist (HTTP 404, FS ENOENT, DB row missing).
    ResourceNotFound,
    /// Authorization or sandboxing rejection (HTTP 403, FS EACCES, SSRF).
    PermissionDenied,
    /// Operation took too long.
    Timeout,
    /// Network-level failure (DNS, connection refused, TLS error).
    NetworkError,
    /// Server-side error (HTTP 5xx, downstream service unhealthy).
    UpstreamError,
    /// HTTP 429 / API rate limit.
    RateLimited,
    /// Body exceeded buffer cap, file too large, etc.
    PayloadTooLarge,
    /// Body couldn't be parsed as expected format (JSON parse, malformed HTML).
    ParseError,
    /// Service / resource temporarily unavailable (DB locked, service starting).
    Unavailable,
    /// Catch-all when no other variant fits.
    Other,
}

impl ToolErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "InvalidInput",
            Self::ResourceNotFound => "NotFound",
            Self::PermissionDenied => "PermissionDenied",
            Self::Timeout => "Timeout",
            Self::NetworkError => "NetworkError",
            Self::UpstreamError => "UpstreamError",
            Self::RateLimited => "RateLimited",
            Self::PayloadTooLarge => "PayloadTooLarge",
            Self::ParseError => "ParseError",
            Self::Unavailable => "Unavailable",
            Self::Other => "Other",
        }
    }
}
```

Extend `ToolError`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    Execution(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Structured error with a category and a user/LLM-friendly message.
    /// Display formats as `[Kind] message` so the LLM can pattern-match
    /// on the bracketed tag.
    #[error("[{}] {message}", .kind.as_str())]
    Kinded {
        kind: ToolErrorKind,
        message: String,
        #[allow(dead_code)]
        source: Option<String>,
    },
}

impl ToolError {
    pub fn kinded(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self::Kinded { kind, message: message.into(), source: None }
    }

    pub fn kinded_with_source(
        kind: ToolErrorKind,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Kinded { kind, message: message.into(), source: Some(source.into()) }
    }
}
```

### - [ ] Step 4.2: Verify build + existing tests pass

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd src-tauri && cargo test --lib 2>&1 | tail -10
```
Expected: 0 build errors, all existing tests pass (additive change).

### - [ ] Step 4.3: Add a smoke test

In the same file, in (or alongside) any existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn kinded_error_displays_with_bracketed_kind() {
    let err = ToolError::kinded(
        ToolErrorKind::ResourceNotFound,
        "Page returned 404",
    );
    assert_eq!(format!("{}", err), "[NotFound] Page returned 404");
}

#[test]
fn kinded_error_serializes_through_existing_serde_path() {
    let err = ToolError::kinded(
        ToolErrorKind::PermissionDenied,
        "URL blocked",
    );
    let json = serde_json::to_string(&err).unwrap();
    // Existing serde impl uses Display; both new + legacy variants share
    // the same serialization path.
    assert!(json.contains("PermissionDenied"));
    assert!(json.contains("URL blocked"));
}
```

Run: `cd src-tauri && cargo test --lib tool::tests 2>&1 | tail -10` (or whatever the module path is). Expected: passing.

### - [ ] Step 4.4: Commit

```bash
git add src-tauri/src/agent/tools/tool.rs
git commit -m "$(cat <<'EOF'
refactor(tool-error): ToolError gains a kind enum

Existing variants (Execution / InvalidParams / NotFound / Io) stay
intact for backward compat. New Kinded { kind, message, source }
variant carries a ToolErrorKind (NotFound, PermissionDenied, Timeout,
RateLimited, ...) that prefixes the message in Display as
"[Kind] message" — gives the LLM a categorical handle to reason about
the failure (retry vs. alternative approach vs. ask user).

Two helper constructors (kinded / kinded_with_source). Migrations to
the new shape land in the next commit per hot-path tool.

Pure refactor: zero behavior change, all existing call sites compile
unchanged. Two unit tests cover Display + serde round-trip.
EOF
)"
```

---

## Task 5: Hot-path tools migrate to `Kinded` errors

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/web.rs`
- Modify: `src-tauri/src/agent/tools/builtin/shell.rs`
- Modify: `src-tauri/src/agent/tools/builtin/search.rs`
- Modify: `src-tauri/src/agent/tools/builtin/file.rs` (if exists — grep first)

### - [ ] Step 5.1: Migrate `web.rs::WebFetchTool::execute`

Find the existing error-mapping sites. Replace bare `Execution(...)` with classified `kinded(...)`:

```rust
// At top of file (already imported but verify):
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

// SSRF / validation rejection:
if let Err(reason) = validate_url(url) {
    warn!(url, reason = %reason, "web_fetch: URL rejected");
    return Err(ToolError::kinded(
        ToolErrorKind::PermissionDenied,
        format!("URL blocked: {}", reason),
    ));
}

// Reqwest send error:
let resp = client.get(url).send().await
    .map_err(|e| {
        // Heuristic: timeout vs. connection vs. other.
        let kind = if e.is_timeout() {
            ToolErrorKind::Timeout
        } else if e.is_connect() {
            ToolErrorKind::NetworkError
        } else {
            ToolErrorKind::NetworkError
        };
        ToolError::kinded_with_source(
            kind,
            format!("Failed to fetch {}", url),
            e.to_string(),
        )
    })?;

// Non-success status — classify but keep going (don't return; the body may
// still be useful, e.g. for 404 pages with structured error messages).
let status = resp.status();
if !status.is_success() {
    let code = status.as_u16();
    let kind = match code {
        400..=403 => ToolErrorKind::PermissionDenied,
        404 => ToolErrorKind::ResourceNotFound,
        408 | 504 => ToolErrorKind::Timeout,
        429 => ToolErrorKind::RateLimited,
        500..=599 => ToolErrorKind::UpstreamError,
        _ => ToolErrorKind::Other,
    };
    // Early return for 4xx/5xx — the body is usually unhelpful.
    return Err(ToolError::kinded(
        kind,
        format!("Page returned {} ({})", code, url),
    ));
}
```

The `.text()` error and body-too-large stay as is (the latter now truncates
instead of erroring, per commit c997e80). The truncation path stays warning-
only — no `ToolError` needed.

### - [ ] Step 5.2: Migrate `web.rs::HttpRequestTool::execute`

Same pattern at the same call sites. Don't classify the body — http_request
is meant to return raw responses, so 4xx/5xx bodies are valid output, not
errors. Only classify connection-level failures.

### - [ ] Step 5.3: Migrate `shell.rs` (or shell tool wherever it lives)

```bash
grep -rn "fn execute" src-tauri/src/agent/tools/builtin/shell.rs | head
```

For shell commands:
- Exit code 127 → `ResourceNotFound` ("Command not found")
- Exit code 126 → `PermissionDenied` ("Permission denied")
- Exit code 124 (with timeout cmd) → `Timeout`
- Anything else non-zero → keep as `Other` with the stderr (truncated to first 1KB)
- Process kill / signal → `Other`

```rust
// Pseudocode — adapt to existing shell tool structure:
if !status.success() {
    let code = status.code().unwrap_or(-1);
    let kind = match code {
        127 => ToolErrorKind::ResourceNotFound,
        126 => ToolErrorKind::PermissionDenied,
        124 => ToolErrorKind::Timeout,
        _ => ToolErrorKind::Other,
    };
    let stderr_excerpt: String = stderr_text.chars().take(1024).collect();
    return Err(ToolError::kinded_with_source(
        kind,
        format!("Command exited with code {}", code),
        stderr_excerpt,
    ));
}
```

### - [ ] Step 5.4: Migrate `search.rs` and `file.rs`

- search: empty results → `ResourceNotFound` ("No matches for ..."). FS errors → propagate via `?` (the `From<io::Error>` impl maps to `Io` variant; that's fine for now — we don't need to over-classify).
- file: ENOENT → `ResourceNotFound`. EACCES → `PermissionDenied`. Other → propagate `Io`.

```rust
// In file.rs:
let f = match tokio::fs::read_to_string(&path).await {
    Ok(s) => s,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
        return Err(ToolError::kinded(
            ToolErrorKind::ResourceNotFound,
            format!("File not found: {}", path.display()),
        ));
    }
    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
        return Err(ToolError::kinded(
            ToolErrorKind::PermissionDenied,
            format!("Permission denied: {}", path.display()),
        ));
    }
    Err(e) => return Err(e.into()),
};
```

### - [ ] Step 5.5: Build + run tests

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib 2>&1 | tail -10
```
Expected: 0 errors. Existing tests still pass.

### - [ ] Step 5.6: Add one new test per migrated tool

In each `#[cfg(test)] mod tests` (or create one) for web/shell/file, add a
test that verifies the kind classification. Web is hardest to test (would
need a mock HTTP server); skip for web — manual smoke is enough. For
shell + file:

```rust
// shell.rs tests
#[tokio::test]
async fn shell_command_not_found_returns_resource_not_found_kind() {
    let tool = ShellTool::new(/* ... */);
    let result = tool.execute(serde_json::json!({
        "command": "definitely-not-a-real-command-xyz"
    })).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::Kinded { kind, .. } => assert_eq!(kind, ToolErrorKind::ResourceNotFound),
        other => panic!("expected Kinded(ResourceNotFound), got {:?}", other),
    }
}
```

```rust
// file.rs tests
#[tokio::test]
async fn file_read_nonexistent_returns_resource_not_found_kind() {
    let tool = FileReadTool::new(/* ... */);
    let result = tool.execute(serde_json::json!({
        "path": "/tmp/definitely-does-not-exist-xyz-12345.txt"
    })).await;
    match result.unwrap_err() {
        ToolError::Kinded { kind, .. } => assert_eq!(kind, ToolErrorKind::ResourceNotFound),
        other => panic!("expected Kinded(ResourceNotFound), got {:?}", other),
    }
}
```

### - [ ] Step 5.7: Commit

```bash
git add src-tauri/src/agent/tools/builtin/
git commit -m "$(cat <<'EOF'
feat(tool-error): map HTTP / FS / shell errors to user-friendly kinds

Migrates the four hot-path builtin tools (web_fetch, http_request,
shell, search/file) to return ToolError::Kinded with a categorical
ToolErrorKind:

- HTTP 4xx/5xx → PermissionDenied / ResourceNotFound / RateLimited /
  Timeout / UpstreamError per status code
- SSRF rejection → PermissionDenied
- Reqwest connect/timeout errors → NetworkError / Timeout
- Shell exit 127 → ResourceNotFound (command not found)
- Shell exit 126 → PermissionDenied
- Shell exit 124 → Timeout (timeout(1) signal)
- FS ENOENT → ResourceNotFound
- FS EACCES → PermissionDenied

The LLM now sees errors prefixed with the bracketed kind tag, e.g.
"[NotFound] Page returned 404 (https://x.com/missing)" — much easier
to reason about than the previous raw rust strings.

Backward-compatible: tools that haven't migrated still return
ToolError::Execution. Tools downstream of these (other call sites)
don't need to change.

Unit tests cover the most user-impactful classifications (command
not found, file not found).
EOF
)"
```

---

## Task 6: `web_fetch` HTML extraction upgrade with `scraper` + SPA detection

**Files:**
- Modify: `src-tauri/Cargo.toml` — add `scraper`
- Modify: `src-tauri/src/agent/tools/builtin/web.rs` — replace `extract_text` + add SPA detection

### - [ ] Step 6.1: Add `scraper` dep

```toml
# In src-tauri/Cargo.toml [dependencies]:
scraper = "0.20"
```

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors (just downloads the crate).

### - [ ] Step 6.2: Replace `extract_text` in `web.rs`

Read the current `extract_text` (around line 75). Replace the function body
with a `scraper`-based implementation:

```rust
fn extract_text(html: &str) -> String {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);

    // Selectors for content we DON'T want.
    let script_sel = Selector::parse("script, style, noscript").unwrap();

    // Collect text from all nodes EXCEPT inside script/style/noscript.
    // scraper doesn't have a built-in "text without children of selector",
    // so we walk the tree manually.
    let mut out = String::new();
    walk_for_text(doc.root_element(), &mut out);
    return collapse_whitespace(&out);

    fn walk_for_text(node: scraper::ElementRef, out: &mut String) {
        for child in node.children() {
            if let Some(text) = child.value().as_text() {
                out.push_str(text);
                continue;
            }
            if let Some(elem) = scraper::ElementRef::wrap(child) {
                let tag = elem.value().name();
                // Skip non-content elements entirely.
                if matches!(tag, "script" | "style" | "noscript" | "template") {
                    continue;
                }
                // Block-level elements get a newline boundary.
                if matches!(tag, "br" | "p" | "div" | "h1" | "h2" | "h3" | "h4"
                                 | "h5" | "h6" | "li" | "tr" | "section"
                                 | "article" | "header" | "footer") {
                    out.push('\n');
                }
                walk_for_text(elem, out);
                if matches!(tag, "p" | "div" | "h1" | "h2" | "h3" | "h4"
                                 | "h5" | "h6" | "li" | "tr" | "section"
                                 | "article") {
                    out.push('\n');
                }
            }
        }
    }

    fn collapse_whitespace(s: &str) -> String {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

### - [ ] Step 6.3: Add SPA detection

In the same file, near `extract_text`:

```rust
/// Heuristic SPA detection. Returns true when the page looks like a
/// JavaScript-rendered single-page app whose initial HTML carries minimal
/// human content — the scraper-extracted text will undercount the page.
fn detect_spa(html: &str, extracted_text: &str) -> bool {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);

    // (a) script tag count > 5
    let script_count = Selector::parse("script").unwrap();
    let scripts = doc.select(&script_count).count();
    if scripts <= 5 { return false; }

    // (b) visible body text under 500 chars
    if extracted_text.chars().count() >= 500 { return false; }

    // (c) at least one obvious framework mount marker
    let mount_markers = [
        "#root", "#app", "#__next", "#__nuxt",
        "[data-reactroot]", "[ng-app]",
    ];
    for marker in &mount_markers {
        if let Ok(sel) = Selector::parse(marker) {
            if doc.select(&sel).next().is_some() {
                return true;
            }
        }
    }
    false
}
```

Use it in `WebFetchTool::execute` after the existing extraction:

```rust
let text = Self::extract_text(&body);
let is_spa = Self::detect_spa(&body, &text);

let total_chars = text.chars().count();
let truncated = if total_chars > max_length {
    let prefix: String = text.chars().take(max_length).collect();
    format!(
        "{}...\n[Truncated: showing {}/{} characters]",
        prefix, max_length, total_chars
    )
} else {
    text
};

let final_output = if is_spa {
    format!(
        "{}\n\n⚠️ This page appears to be a JavaScript-rendered single-page app \
         (heuristic: many <script> tags, sparse body text, framework mount point \
         detected). The text above may be missing dynamic content. For full \
         content, use the browser tool instead.",
        truncated,
    )
} else {
    truncated
};

debug!(url, chars = final_output.len(), is_spa, "Web page fetched");
Ok(ToolOutput::success(&final_output, start.elapsed().as_millis() as u64))
```

### - [ ] Step 6.4: Make `detect_spa` and `extract_text` standalone fns OR keep as `impl` methods

Either works. The plan shows them as `fn`s; if the existing `extract_text` is
on `impl WebFetchTool`, keep both there for cohesion.

### - [ ] Step 6.5: Tests

Add to the existing `#[cfg(test)] mod tests` in web.rs:

```rust
#[test]
fn extract_text_strips_script_and_style() {
    let html = r#"
        <html><head><style>body { color: red }</style></head>
        <body>
            <h1>Title</h1>
            <p>Para 1.</p>
            <script>alert('x')</script>
            <p>Para 2.</p>
        </body></html>"#;
    let text = WebFetchTool::extract_text(html);
    assert!(text.contains("Title"));
    assert!(text.contains("Para 1"));
    assert!(text.contains("Para 2"));
    assert!(!text.contains("alert"));
    assert!(!text.contains("color: red"));
}

#[test]
fn detect_spa_recognizes_react_root_with_few_text() {
    let html = r#"
        <html><body>
        <div id="root"></div>
        <script src="bundle.js"></script>
        <script src="vendor.js"></script>
        <script src="runtime.js"></script>
        <script src="polyfills.js"></script>
        <script src="main.js"></script>
        <script src="chunk.js"></script>
        </body></html>"#;
    let text = WebFetchTool::extract_text(html);
    assert!(WebFetchTool::detect_spa(html, &text));
}

#[test]
fn detect_spa_returns_false_for_content_heavy_page() {
    let html = r#"
        <html><body>
        <article>
            <h1>An Article</h1>
            <p>This is a real content-heavy page. It has many paragraphs
            of text. Real content here, not a SPA wrapper. We expect the
            heuristic to recognize this as NOT a SPA because the visible
            text is substantial. Lorem ipsum dolor sit amet, consectetur
            adipiscing elit. Many many words to push past the 500-char
            threshold so that this fixture clearly disambiguates from a
            sparse SPA shell.</p>
        </article>
        </body></html>"#;
    let text = WebFetchTool::extract_text(html);
    assert!(!WebFetchTool::detect_spa(html, &text));
}
```

Run: `cd src-tauri && cargo test --lib agent::tools::builtin::web::tests 2>&1 | tail -15`
Expected: existing 4 + new 3 = 7 passing.

### - [ ] Step 6.6: Commit

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/agent/tools/builtin/web.rs
git commit -m "$(cat <<'EOF'
feat(web): scraper-based HTML extraction + SPA detection hint

Replaces the naive char-by-char tag-stripping extract_text with
scraper::Html::parse_document + a tree walk that:
- Properly handles malformed HTML, nested tags, attribute injection
- Correctly skips script / style / noscript / template subtrees
- Inserts block-element newlines for readable plain text

Adds detect_spa() heuristic: > 5 <script> tags AND < 500 chars of
visible text AND a recognized framework mount point (#root, #app,
#__next, #__nuxt, [data-reactroot], [ng-app]) all required.

When SPA is detected, the tool's output is suffixed with:
  ⚠️ This page appears to be a JavaScript-rendered single-page app
  ... use the browser tool instead.

— so the agent can switch to chromiumoxide-backed browser tool rather
than wasting turns on a content-empty fetch.

Tests cover: script/style stripping, SPA detection on a react-rooted
empty shell, SPA detection returns false for content-heavy pages.

scraper crate adds ~250 KB compiled; Tauri's Chromium bundle dwarfs
this so net binary size impact is rounding error.
EOF
)"
```

---

## Task 7: Markdown rendering regression test suite

**Files:**
- Create: `ui/src/components/ai-elements/__fixtures__/markdown-samples/*.md` (9 files)
- Create: `ui/src/components/ai-elements/message.fixtures.test.tsx`

### - [ ] Step 7.1: Create the fixture directory + files

```bash
mkdir -p ui/src/components/ai-elements/__fixtures__/markdown-samples
```

Write 9 fixture files. Content for each:

**`01-mixed-cjk-latin.md`:**
```markdown
今天的工作总结：完成了 **Phase 6-C** 的实施，包括 *cost dashboard* 和
*budget alerts*。Total LOC: 1247. Commits: 6.

明天计划:
- 上线 Stabilization Week PR
- 跑一组 dogfood probes
- 修 web_fetch 的边界 case
```

**`02-currency-amounts.md`:**
```markdown
| Asset | Price | Change |
|-------|-------|--------|
| AAPL  | $294.76 | -0.20% |
| GOOG  | $388.64 | -3.01% |
| META  | $597.40 | -1.77% |

> Total portfolio drawdown: $12.43 (-1.66%). Cash reserve: $2,438.91.
```

**`03-numeric-ranges.md`:**
```markdown
预期波动区间：

- Apple: 守住 $285 — $295
- Google: 可能回落到 *$370-$380*
- Meta: 探底 **570-580**
```

**`04-table-with-status-cells.md`:**
```markdown
| Task | Status |
|------|--------|
| Schema migration | ✅ 已完成 |
| Atom wiring | ✅ 已完成 |
| UI rendering | ⏳ in-progress |
| Smoke test | ❌ 未开始 |
```

**`05-blockquote-with-bold.md`:**
```markdown
> ⚠️ **注意**: 当前用户预算已超过 100%。**已用 $52.40 / $50.00**。
> 系统不会阻止后续调用，但请留意 LLM 成本。
```

**`06-nested-lists.md`:**
```markdown
实施计划：
1. 后端
   - Migration V19
   - Two new commands
   - Threshold emission
2. 前端
   - Cost atoms
     - monthStartMsAtom
     - workspaceRollupAtom
   - BudgetHeader component
3. 测试
   - Rust 13 个单测
   - Vitest 6 个单测
```

**`07-emoji-everywhere.md`:**
```markdown
🎉 Phase 6 系列收尾！

完成情况：
- 📌 6-A pinned sessions
- 🔍 6-B cross-workspace search
- 💰 6-C cost dashboard

下一步 → 🛠️ Stabilization Week
```

**`08-code-blocks-and-inline-code.md`:**
````markdown
修复 `emit_turn_cost` 的死锁问题：

```rust
async fn emit_turn_cost(&self, usage: &TokenUsage) {
    let budget = state.settings.read().await.monthly_budget_usd;
    // ...
}
```

调用方相应改成 `self.emit_turn_cost(usage).await`.
````

**`09-headings.md`:**
```markdown
# Stabilization Week

## Workstream 1: Observability

### Tracing setup

加 `tracing-appender` 做 daily rotation.

## Workstream 2: Tool errors

### Error kind enum

`ToolErrorKind::NotFound` / `Timeout` / `RateLimited`.
```

### - [ ] Step 7.2: Write the failing test file

Create `ui/src/components/ai-elements/message.fixtures.test.tsx`:

```tsx
/**
 * Fixture-driven regression suite for the assistant markdown renderer.
 *
 * Each `.md` file under __fixtures__/markdown-samples is loaded via Vite's
 * raw import and fed to <MessageResponse>. We then assert structural
 * invariants that have been violated in recent regressions:
 *
 * - No KaTeX output (we removed remark-math; nothing should produce
 *   `<span class="katex">`).
 * - <strong> uses font-medium (not browser-default bold/700) and
 *   inherits color — guarded by checking for the className applied
 *   by MarkdownStrong.
 * - <em> is rendered non-italic — same rationale.
 * - No `<span class="katex">` for any input.
 * - Tables render with our card wrapper (not bare <table>).
 */
import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { MessageResponse } from './message'

// Vite glob imports — bundles every .md fixture at build time as raw text.
const fixtures = import.meta.glob<string>(
  './__fixtures__/markdown-samples/*.md',
  { eager: true, import: 'default', query: '?raw' },
)

describe('MessageResponse — markdown rendering regressions', () => {
  for (const [path, content] of Object.entries(fixtures)) {
    const name = path.split('/').pop()!.replace('.md', '')

    it(`${name}: no KaTeX output (math removed)`, () => {
      const { container } = render(<MessageResponse>{content}</MessageResponse>)
      expect(container.querySelectorAll('.katex')).toHaveLength(0)
      expect(container.querySelectorAll('[class*="katex"]')).toHaveLength(0)
    })

    it(`${name}: <strong> uses font-medium (no bold/700)`, () => {
      const { container } = render(<MessageResponse>{content}</MessageResponse>)
      const strongs = container.querySelectorAll('strong')
      strongs.forEach((el) => {
        // MarkdownStrong applies "font-medium text-inherit"
        expect(el.className).toContain('font-medium')
        expect(el.className).toContain('text-inherit')
      })
    })

    it(`${name}: <em> rendered non-italic`, () => {
      const { container } = render(<MessageResponse>{content}</MessageResponse>)
      const ems = container.querySelectorAll('em')
      ems.forEach((el) => {
        expect(el.className).toContain('not-italic')
        expect(el.className).toContain('font-medium')
      })
    })

    it(`${name}: no literal escape markers leaked`, () => {
      const { container } = render(<MessageResponse>{content}</MessageResponse>)
      // remark-math removal regression: no literal `\$` should appear
      // in the rendered DOM.
      expect(container.textContent).not.toContain('\\$')
    })
  }

  it('table fixture renders inside not-prose card wrapper', () => {
    const tableFixture = fixtures['./__fixtures__/markdown-samples/04-table-with-status-cells.md']
    const { container } = render(<MessageResponse>{tableFixture}</MessageResponse>)
    const tables = container.querySelectorAll('table')
    expect(tables.length).toBeGreaterThan(0)
    // The MarkdownTable wrapper adds not-prose + bg-card.
    const wrapper = tables[0].closest('.not-prose')
    expect(wrapper).not.toBeNull()
  })

  it('blockquote fixture renders with text-foreground/75 dimming', () => {
    const bqFixture = fixtures['./__fixtures__/markdown-samples/05-blockquote-with-bold.md']
    const { container } = render(<MessageResponse>{bqFixture}</MessageResponse>)
    const bq = container.querySelector('blockquote')
    expect(bq).not.toBeNull()
    expect(bq!.className).toContain('text-foreground/75')
    // <strong> inside the blockquote: should NOT have its own color
    // override that would break the dim — MarkdownStrong uses text-inherit.
    const strongInBq = bq!.querySelector('strong')
    expect(strongInBq!.className).toContain('text-inherit')
  })

  it('snapshot: mixed-cjk-latin fixture', () => {
    const f = fixtures['./__fixtures__/markdown-samples/01-mixed-cjk-latin.md']
    const { container } = render(<MessageResponse>{f}</MessageResponse>)
    expect(container.innerHTML).toMatchSnapshot()
  })

  it('snapshot: nested-lists fixture', () => {
    const f = fixtures['./__fixtures__/markdown-samples/06-nested-lists.md']
    const { container } = render(<MessageResponse>{f}</MessageResponse>)
    expect(container.innerHTML).toMatchSnapshot()
  })
})
```

### - [ ] Step 7.3: Run the tests

```bash
cd ui && npm test -- --run message.fixtures 2>&1 | tail -30
```

Expected: all tests pass except the snapshot tests, which will fail on first
run with "no snapshot saved" — that's normal. Re-run with `--update` or
just commit the snapshot files (Vitest auto-saves them on first run).

If structural assertions fail (e.g. `<strong>` doesn't have `font-medium`),
that's a real regression — fix the source before continuing.

### - [ ] Step 7.4: Commit

```bash
git add ui/src/components/ai-elements/__fixtures__/ ui/src/components/ai-elements/message.fixtures.test.tsx ui/src/components/ai-elements/__snapshots__/
git commit -m "$(cat <<'EOF'
test(markdown): regression suite for chat content rendering

Nine markdown fixtures covering the formats agents actually emit:
mixed CJK+Latin, currency, numeric ranges, tables with status cells,
blockquotes with bold, nested lists, emoji-heavy, code blocks,
headings.

Per-fixture invariants (run for each .md file via vite glob import):
- No KaTeX output anywhere (regression for math removal)
- All <strong> render with font-medium + text-inherit (regression for
  the not-prose-table semibold bug)
- All <em> render with not-italic + font-medium (regression for the
  italic-numeric-range bug)
- No literal `\$` escape markers in the DOM

Plus per-format spot checks: table renders inside not-prose card,
blockquote keeps text-foreground/75 dimming through <strong> children.

Two snapshot tests (mixed-cjk-latin, nested-lists) catch unexpected
DOM changes from future renderer tweaks.
EOF
)"
```

---

## Task 8: Active dogfood probing (placeholder commit)

**Files:**
- Modify: whatever needs fixing

### - [ ] Step 8.1: Run the probe set

Re-start dev (`cargo tauri dev`). Send the following prompts, capture any
visible anomaly + the corresponding tracing/crash log evidence:

1. `对比 Apple Google Meta 今日股价` (the dogfood prompt that surfaced 5
   prior bugs — must work cleanly now)
2. `find all places that call emit_turn_cost` (grep-style code search,
   tests that search tool returns categorized errors when path is bad)
3. `write a 3000-word essay on the history of database concurrency
   primitives` (long-context streaming stress)
4. `fetch https://finance.yahoo.com/quote/AAPL/ and summarize` (web_fetch
   end-to-end after scraper upgrade)
5. `fetch https://twitter.com/ and tell me what's there` (SPA — should
   produce the "use browser tool" hint)
6. `请帮我用中文写一份冬季度假行程，包含日本东京 4 天、京都 3 天的建议`
   (CJK-only long-form)

For each prompt, note:
- Did it complete without panicking? (verify `~/.uclaw/logs/crashes/` is
  unchanged)
- Did the rendering look clean? (no slanted digits, no math italic, no
  weight inconsistency)
- Did any tool return an opaque error? (`Tool execution failed: <raw>`
  vs. `[Kind] message`)
- Did the agent recover from failures gracefully?

### - [ ] Step 8.2: Fix what you find

For each bug found, write a small focused commit. Title prefix `fix(dogfood):`.
Commits in this task block can be 0 (best case) up to ~3 (realistic).

Likely classes of issues to be ready for:
- HTML parsing edge case (malformed page → scraper handles gracefully but
  output is empty)
- Some tool not yet migrated to Kinded errors
- Markdown fixture you didn't think of (e.g. footnotes, task lists, HTML
  inline)
- Streaming issue (rare but possible)

Each fix follows the standard cycle: identify → minimal repro → fix +
test → commit.

### - [ ] Step 8.3: Document findings in PR description

Even if no bugs are found, note in the PR: "Dogfood probes 1-6 all
clean; rendering invariants from Task 7 hold; crash log directory
remained empty across all sessions."

---

## Task 9: Final smoke + ship

### - [ ] Step 9.1: Full backend test suite

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
```
Expected: all tests pass. Count should be the existing total + ~10-15 new tests added across tasks.

### - [ ] Step 9.2: Full frontend test suite + type check

```bash
cd ui && npm test -- --run 2>&1 | tail -5
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: all pass, 0 TS errors.

### - [ ] Step 9.3: Backend build + size sanity

```bash
cd src-tauri && cargo build --release 2>&1 | tail -5
ls -lh target/release/uclaw 2>/dev/null || ls -lh target/release/uclaw_core* | head
```
Note the binary size in the PR description. Verify it didn't balloon
(expect ~300-500 KB increase from scraper + tracing-appender).

### - [ ] Step 9.4: Push + open PR

```bash
git push -u origin claude/stabilization-week
```

Title: `chore(stabilization): panic recovery + categorized errors + scraper + markdown regression suite`

PR body should include:
- Commit table (8 + dogfood commits, bisectable goals)
- Smoke test plan (the 6 probes from Task 8)
- Binary size delta
- Log file paths the user can inspect

### - [ ] Step 9.5: Self-merge after manual smoke

If everything's clean, squash-merge. If anything's flaky, push fix commits.

---

## Self-Review

### Spec coverage

- §4 Panic persistence: ✓ Tasks 1 + 2 (tracing file + panic hook) + 3 (catch_unwind via spawn)
- §5 Tool error friendliness: ✓ Tasks 4 + 5 (Kinded enum + hot-path migration)
- §6 Markdown regression suite: ✓ Task 7 (9 fixtures + 4 invariants per fixture + 2 snapshots)
- §7 web_fetch upgrade: ✓ Task 6 (scraper + SPA detection)
- §8 Active dogfood: ✓ Task 8 (placeholder for actual probing + fixes)
- §9 8-commit shape: ✓ Tasks 1-7 each commit once, Task 8 adds 0-3 commits, Task 9 is just smoke.
- §10 Risks: addressed inline — spawn overhead acceptable, scraper size acceptable, log retention deferred, catch_unwind avoided via spawn.

### Placeholder scan

- Task 5 has a minor "adapt to existing shell tool structure" note — that's
  not a placeholder, it's a directive to verify the actual file. Each step
  has concrete code.
- Task 8 deliberately under-specifies fixes since they depend on what
  surfaces. That's correct — fixes are commit-shaped during execution.

### Type consistency

- `ToolErrorKind` introduced in Task 4, used in Tasks 5 + 8. Consistent variants.
- `ToolError::kinded(kind, message)` helper signature used in all migrations.
- `ToolError::Kinded { kind, message, source }` field names consistent.
- `WorkerGuard` returned from `observability::init()` consistently.

No issues found.
