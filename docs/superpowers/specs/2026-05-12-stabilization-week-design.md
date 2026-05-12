# Stabilization Week — Design

> One-week effort to harden the agent runtime, surface bugs that have been
> hiding in dogfood, and make the user-facing experience of failures
> survivable rather than catastrophic. Ships as a single PR.

## 1. Problem

The last two weeks of dogfood surfaced **6 user-impacting bugs** across
narrow paths the user happened to exercise:

| # | Bug | Symptom |
|---|---|---|
| 1 | Budget input rejected decimals | `step="1"` blocked `$2.10` |
| 2 | `block_on` inside async deadlocked emit_turn_cost | Agent text streams, "thinking" never clears |
| 3 | `font-feature-settings: tnum` produced mismatched digits | Numbers between CJK chars rendered in PingFang SC |
| 4 | `<strong>` inside `not-prose` table = browser-default bold (700) | Boxed cells looked like a different typeface |
| 5 | `remark-math` ate currency `$X.XX` and ranges `$$X-Y$$` | Plain text rendered as italic-serif KaTeX |
| 6 | `web_fetch` slicing `[..N]` on UTF-8 panicked tokio worker | Whole agent loop crashed silently |

These weren't unrelated freak occurrences — they share a pattern: **none of
them had observable failure modes before the user reported them**. Panics
crashed worker threads silently. Tool errors surfaced as raw Rust strings.
Markdown regressions had zero test coverage. Each bug took us 5-30 minutes
to fix but 2-5 minutes of user dogfood to find.

The implication: there are **more bugs in the same neighborhoods** that
we haven't tripped over yet. This week's work invests in **observability,
recoverability, and proactive coverage** so the next 6 bugs surface
faster and don't take down the agent loop when they do.

## 2. Goals

Four workstreams that ship together as one PR. Each is a small,
self-contained improvement; together they raise the floor of the
runtime's robustness.

1. **Panic persistence + crash recovery.** Tauri worker panics no longer
   silently kill the agent loop; they write a structured crash record and
   the agent emits a user-visible error.
2. **Tool error user-friendliness.** Common tool failures (HTTP non-2xx,
   timeouts, SSRF rejections, DB busy) get mapped to actionable messages
   instead of raw Rust strings.
3. **Markdown rendering regression test suite.** A fixture file with N
   real-world agent output samples gets snapshot-tested every PR; any
   change that breaks rendering surfaces in CI.
4. **`web_fetch` HTML extraction upgrade.** Switch from naive tag-stripping
   to `scraper`-based parsing + SPA detection that explicitly tells the
   agent "this page is JS-rendered, use a browser tool instead".

Plus active dogfood probing: I run a series of representative agent
tasks (different language mixes, page types, request shapes) during
implementation and fold any newly-discovered bugs into the appropriate
workstream.

## 3. Non-Goals

- No new features. No new agent tools. No UI redesigns. Pure hardening.
- No telemetry to external services. Crash logs stay on disk locally.
- No automatic crash report submission. The user remains in control of
  whether to share logs.
- No retry/circuit-breaker logic on tool failures (out of scope; that's a
  Phase 2 reliability theme).
- No JavaScript execution for SPA pages. We **detect** SPAs and tell the
  agent to switch tools; we don't render them ourselves.

## 4. Workstream 1 — Panic persistence + crash recovery

### Current state

- `main.rs` initializes `tracing_subscriber::fmt()` to stdout only. No
  file output. Once the user exits the dev console, logs are gone.
- No `std::panic::set_hook` registered. Panics in tokio workers print to
  stderr (visible only in `cargo tauri dev` console) and silently kill
  the worker thread. The agent loop calling that worker has no way to
  observe the panic — `JoinHandle::await` returns `Err(JoinError)` but
  in the existing dispatcher most tool execution paths don't even
  `spawn` — they `.await` directly on the same task, so a tool panic
  unwinds up through `dispatcher::execute_tool` and crashes the agent
  turn entirely.
- The web_fetch panic (commit 8db65c0) is the clearest example: a single
  UTF-8 slice bug killed the agent run with no on-disk evidence.

### Design

**A. File-backed tracing.** Add `tracing-appender` for rolling daily logs:

```rust
// main.rs
let log_dir = home::home_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join(".uclaw").join("logs");
std::fs::create_dir_all(&log_dir).ok();
let file_appender = tracing_appender::rolling::daily(&log_dir, "uclaw.log");
let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
// Keep guard alive for entire process — drop on shutdown flushes pending logs.

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
    .with(tracing_subscriber::fmt::layer().with_writer(file_writer).with_ansi(false))
    .with(EnvFilter::from_default_env().unwrap_or_else(|_| "info,...".into()))
    .init();
```

Files go to `~/.uclaw/logs/uclaw.log.YYYY-MM-DD`, rotated daily, ANSI-stripped
for offline inspection. The `_guard` MUST be returned from `main` (held until
process exit) or the non-blocking writer drops messages on shutdown.

**B. Panic hook.** Captures panic info to a separate crash log:

```rust
std::panic::set_hook(Box::new(|info| {
    let crash_dir = home::home_dir().unwrap_or_default()
        .join(".uclaw").join("logs").join("crashes");
    std::fs::create_dir_all(&crash_dir).ok();
    let now = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let thread = std::thread::current();
    let path = crash_dir.join(format!("crash-{}-{}.log",
        now, thread.name().unwrap_or("unnamed")));
    let backtrace = std::backtrace::Backtrace::force_capture();
    let _ = std::fs::write(&path, format!(
        "{}\n\nThread: {:?}\nLocation: {:?}\n\n{:?}\n",
        info, thread.name(), info.location(), backtrace,
    ));
    // Also emit to tracing so it ends up in the main log too.
    tracing::error!(?info, %backtrace, "process panic");
}));
```

**C. `catch_unwind` around tool execution.** In `agent/dispatcher.rs`'s tool
execution path, wrap the `.execute()` call in `tokio::task::spawn` so panics
get caught at the JoinHandle layer instead of unwinding through the agent
loop:

```rust
let join_handle = tokio::task::spawn(async move {
    tool.execute(params).await
});
let result = match join_handle.await {
    Ok(Ok(out)) => Ok(out),
    Ok(Err(tool_err)) => Err(tool_err),
    Err(join_err) if join_err.is_panic() => {
        Err(ToolError::Execution(format!(
            "Tool '{}' crashed unexpectedly. Crash log written to ~/.uclaw/logs/crashes/.",
            tool_name,
        )))
    }
    Err(join_err) => Err(ToolError::Execution(format!("Tool join error: {}", join_err))),
};
```

This converts "tokio worker dies + agent loop crashes" into "tool returns
error, agent gets the failure into its turn context and can recover".

### Tests

Rust unit tests for the panic hook are tricky (it's a global hook). Cover:
- A `#[tokio::test]` that spawns a panicking future, awaits its JoinHandle,
  and asserts the dispatcher-style error wrapper returns the right shape.
- A manual smoke (documented in the PR): trigger a deliberate panic from a
  test tool, verify the crash log appears under `~/.uclaw/logs/crashes/`,
  AND the agent loop continues instead of dying.

## 5. Workstream 2 — Tool error user-friendliness

### Current state

`ToolError` is a 3-variant enum: `InvalidParams(String)`, `Execution(String)`,
`Approval(String)`. Every call site builds the message ad-hoc:

```rust
.map_err(|e| ToolError::Execution(format!("Failed to fetch {}: {}", url, e)))?;
```

The agent receives `"Failed to fetch https://x.com/: error decoding response
body: invalid utf-8 sequence of 1 bytes from index 0"` — verbose, low-signal,
hard for the LLM to reason about ("did the URL not exist? did the network
fail? is the page binary?").

### Design

Add a structured **error category** layer on top of the existing string
message. The category is what the LLM keys off; the string is the diagnostic
detail.

```rust
// agent/tools/tool.rs
#[derive(Debug, Clone)]
pub enum ToolErrorKind {
    InvalidParams,       // user/agent gave bad input
    NotFound,            // resource doesn't exist (404, no FS entry)
    PermissionDenied,    // 403, FS permission, SSRF block
    Timeout,             // request/operation exceeded budget
    NetworkError,        // connection refused, DNS, TLS
    UpstreamError,       // 5xx, server-side failure
    RateLimited,         // 429
    PayloadTooLarge,     // 413, body cap exceeded (now truncate, but if reject…)
    ParseError,          // body decoding, JSON parse
    Unavailable,         // DB locked, service down
    Approval,            // user denied or pending
    Other,               // fallback
}

pub struct ToolError {
    pub kind: ToolErrorKind,
    /// Short user/agent-friendly summary. e.g. "Page returned 403 — may
    /// need authentication."
    pub message: String,
    /// Optional raw diagnostic (Rust error chain). Logged but NOT shown
    /// to the LLM directly.
    pub source: Option<String>,
}
```

Each call site classifies its failure:

```rust
// web_fetch
if status == 403 {
    return Err(ToolError {
        kind: ToolErrorKind::PermissionDenied,
        message: format!("Page returned 403 ({url}) — the site may require authentication or block automated requests."),
        source: None,
    });
}
if status >= 500 {
    return Err(ToolError {
        kind: ToolErrorKind::UpstreamError,
        message: format!("Page returned {} — server error, try again later or use a different source.", status),
        source: None,
    });
}
```

The tool dispatcher serializes `kind` into the message the LLM sees (e.g.
`[NotFound] Page returned 404 — the URL doesn't exist or the path is wrong.`)
so the agent can reason about it categorically.

### Scope of mappings (this PR)

Apply the new shape across the **hot path** tools — the ones we've already
seen produce confusing errors:

- `web_fetch`, `http_request` — HTTP status codes → categories
- `shell` — exit codes + stderr classification (currently dumps raw stderr)
- `search_files` — no-match → NotFound, FS errors → Other
- `file_read` — ENOENT → NotFound, EACCES → PermissionDenied
- `web_search` (if exists) — empty results → NotFound, rate limits → RateLimited

Other tools keep returning the existing `Execution(String)` via a `From`
impl that maps to `kind: Other`. Backward-compatible refactor.

### Tests

- `ToolErrorKind` round-trips through serde (for the dispatcher to embed
  the kind in the LLM-facing message).
- Per-tool mapping tests: e.g. mock HTTP 403 response → expect
  `kind == PermissionDenied`, message contains "may require authentication".

## 6. Workstream 3 — Markdown rendering regression test suite

### Current state

`ui/src/components/ai-elements/message.tsx` has zero render tests. The 5
markdown rendering bugs we hit in dogfood (tnum/strong/em/math/blockquote
color) had no regression coverage. Future renderer tweaks have nothing
preventing identical regressions.

### Design

A fixture-driven snapshot test suite. Each fixture is a markdown string +
an expected pattern (or a snapshot file). Cover the formats the agent
actually emits:

```
ui/src/components/ai-elements/
├── message.tsx
├── message.test.tsx                   (existing — limited)
└── __fixtures__/markdown-samples/
    ├── 01-mixed-cjk-latin.md
    ├── 02-currency-amounts.md
    ├── 03-numeric-ranges.md
    ├── 04-table-with-status-cells.md
    ├── 05-blockquote-with-bold.md
    ├── 06-nested-lists.md
    ├── 07-emoji-everywhere.md
    ├── 08-code-blocks-and-inline-code.md
    └── 09-headings.md
```

The test file `message.test.tsx` gains a suite that:

1. Reads each fixture
2. Renders via `MessageResponse`
3. Asserts on specific structural / class invariants:
   - No `<span class="katex">` in any output (regression for math removal)
   - All `<strong>` and `<em>` rendered without `font-weight: 700` /
     `font-style: italic`
   - Numbers in table cells render with no `font-feature-settings: tnum`
     applied
   - No literal `$` math markers escaped (`\$`) in the DOM

Plus 2-3 Vitest `toMatchSnapshot()` calls on representative fixtures so
any unexpected DOM change shows up as a snapshot diff.

### What's NOT in scope

- Cross-browser pixel-level testing. We can't easily test font rendering
  in jsdom; the typographic bugs (which font WebKit picks for a digit
  span) require a real browser. We accept that visual font issues need
  manual smoke; the suite catches DOM / class regressions.
- E2E tests. Pure unit-level renders only.

## 7. Workstream 4 — `web_fetch` HTML extraction upgrade

### Current state

`web.rs::WebFetchTool::extract_text` is a naive char-by-char scanner that:
- Strips `<script>` / `<style>` blocks
- Removes all tags
- Decodes 5 HTML entities
- Joins non-empty lines

Failure modes observed in dogfood:
1. **JS-rendered SPAs** (react/vue/angular bundles): the initial HTML is
   ~95% `<script>` references; `extract_text` returns empty or a copyright
   footer. Agent thinks the page is blank.
2. **Inline styles and event attributes**: `<div style="..." onclick="...">`
   leak literal CSS / JS strings when our parser hits malformed nesting.
3. **`<noscript>` content** sometimes leaks through.
4. **Meaningful content stripped**: `<button>Submit</button>` becomes
   "Submit" with no surrounding context — agent doesn't know it was
   interactive.

### Design

Switch the parser to **`scraper`** (already battle-tested in the Rust HTML
parsing ecosystem; ~250 KB compiled). Use `Html::parse_document` + walk
the tree with explicit handling:

```rust
fn extract_text_v2(html: &str) -> ExtractResult {
    let doc = Html::parse_document(html);
    let mut text = String::new();
    walk_node(doc.root_element(), &mut text);

    // SPA detection heuristic (separately from extraction):
    // - More than 5 <script> tags AND
    // - Visible body text under 500 chars AND
    // - At least one obvious framework marker (id="root"/"app"/"__next")
    let is_spa = detect_spa(&doc);

    ExtractResult { text, is_spa }
}
```

When SPA is detected, the tool wraps the (likely sparse) extracted text
with a hint at the END:

```
<extracted text — probably incomplete>

⚠️ This page appears to be a JavaScript-rendered single-page app
(detected: empty body + <script> tags + #root mount point). The text
above may be missing dynamic content. For full content, use the
browser tool instead.
```

The agent sees this and can decide to switch tools.

### What we DON'T do

- Run a headless browser. We have `browser` tool for that (separate
  `chromiumoxide` integration). Web fetch stays sync + lightweight.
- Implement Readability-style article extraction. That's a project. The
  simpler "extract all text + flag SPA" is enough for now.

### Tests

- Static HTML fixture → expected extracted text (1-2 examples)
- SPA fixture (mostly `<script>` + empty body) → `is_spa == true`, hint
  appears in output
- Malformed HTML (unclosed tags, attribute injection) → no panic, best-effort
  extraction

## 8. Active dogfood probing

During implementation I run a set of representative agent prompts and
capture every anomaly. Probes include:

- **Mixed-language financial query**: "对比 Apple Google Meta 今日股价"
  (the prompt that surfaced 5 of the 6 known bugs)
- **Code search across the workspace**: "find all places that call
  `emit_turn_cost`"
- **Long-context generation**: "write a 3000-word essay on X" (stress
  test for streaming + tnum-style regressions)
- **Tool-heavy task**: "fetch Yahoo Finance Apple page and summarize"
  (web_fetch end-to-end after this PR)
- **Permission flow**: trigger a `bash` command that requires approval
- **CJK-only conversation**: full Chinese conversation start-to-finish

Bugs found go into the workstreams above. If a bug doesn't fit any
workstream (e.g. an entirely new failure mode), it gets a separate
commit at the end of the PR with `fix(dogfood): ...` prefix.

## 9. Commit Shape (~8 commits)

1. `chore(tracing): file-backed daily logs in ~/.uclaw/logs/`
2. `feat(panic): hook captures backtraces to ~/.uclaw/logs/crashes/`
3. `fix(agent): catch_unwind around tool execution — panics return errors`
4. `refactor(tool-error): ToolError gains a kind enum`
5. `feat(tool-error): map HTTP / FS / DB errors to user-friendly kinds`
6. `feat(web): scraper-based HTML extraction + SPA detection hint`
7. `test(markdown): regression suite for chat content rendering`
8. `fix(dogfood): <whatever bugs probe finds>` (may be 0-3 commits)

Bisectable goals:
- After 1: log files exist, no behavior change.
- After 2: crashes leave evidence on disk.
- After 3: agent survives panicking tools.
- After 4: ToolError API extended (backward-compatible).
- After 5: HTTP / FS tools return categorized errors.
- After 6: web_fetch returns useful output for SPA pages.
- After 7: rendering regressions get caught in CI.
- After 8: any dogfood-found bugs are fixed.

## 10. Risks

- **`tokio::task::spawn` for every tool call adds overhead.** Real tools
  are I/O-bound and take ≫ task spawn cost (~µs vs. ~ms). Acceptable.
- **`scraper` crate adds ~250 KB to the binary.** Worth it for the
  parsing correctness; bundle size for a Tauri desktop app is dominated
  by Chromium anyway.
- **Daily log files accumulate.** `tracing-appender::rolling::daily`
  doesn't auto-delete old files. Mitigation: in this PR we just rotate;
  if the user's `~/.uclaw/logs/` grows uncomfortably, follow-up PR adds
  retention. (`du -sh ~/.uclaw/logs/` after a month of heavy use will
  tell us if it's a real problem.)
- **`catch_unwind` requires `UnwindSafe`.** Most futures aren't strictly
  `UnwindSafe`; `tokio::task::spawn` doesn't require it, so spawning is
  the cleaner path (and that's what the design uses). No
  `AssertUnwindSafe` hacks needed.
- **Snapshot tests are noisy.** Updates surface as snapshot diffs. We
  use them sparingly — most assertions are structural (CSS class absent,
  no `<span class="katex">`), not snapshot. Snapshots only for 2-3
  representative fixtures.

## 11. What success looks like

After this PR:

- Restart Vite, send the same financial comparison prompt that surfaced
  5 bugs last week. Zero rendering anomalies. `~/.uclaw/logs/uclaw.log.YYYY-MM-DD`
  exists and contains structured logs.
- Trigger a deliberate panic in a tool (e.g. via a `panic_test` debug
  tool, added then removed in same PR). Verify:
  - `~/.uclaw/logs/crashes/crash-*.log` file exists with backtrace.
  - Agent run continues; gets a `ToolError` instead of dying.
- Fetch `https://twitter.com/` (heavy SPA). Output includes "this page
  appears to be a JavaScript-rendered SPA" hint.
- Run `cd ui && npm test -- markdown` — full markdown regression suite
  passes; any future renderer change that breaks a fixture surfaces.
- A week of normal dogfood produces ≤ 1 new visible bug (vs. 6 last week).
