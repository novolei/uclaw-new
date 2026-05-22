# 03. Performance Comparison And Optimization Plan

Status: analysis document, no implementation changes.
Date: 2026-05-23
Scope: runtime performance, build performance, streaming, search, persistence, memory, and benchmarks.

## Evidence

Primary uClaw evidence:

- `/Users/ryanliu/Documents/uclaw/CONTEXT.md`
- `/Users/ryanliu/Documents/uclaw/Cargo.toml`
- `/Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml`
- `/Users/ryanliu/Documents/uclaw/ui/vite.config.ts`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/services/manager.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/infra/service.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/llm/providers/anthropic.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/llm/providers/openai.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/dispatcher.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/llm_stream.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/cache_policy/policy.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/builtin/search.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/builtin/shell.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/runtime.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/budget.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs`

Primary jcode evidence:

- `/Users/ryanliu/Documents/jcode/Cargo.toml`
- `/Users/ryanliu/Documents/jcode/src/main.rs`
- `/Users/ryanliu/Documents/jcode/src/perf.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/grep.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/glob.rs`
- `/Users/ryanliu/Documents/jcode/src/session/persistence.rs`
- `/Users/ryanliu/Documents/jcode/src/agent/turn_streaming_broadcast.rs`
- `/Users/ryanliu/Documents/jcode/src/provider/openai_stream_runtime.rs`
- `/Users/ryanliu/Documents/jcode/src/bin/session_memory_bench.rs`
- `/Users/ryanliu/Documents/jcode/scripts/bench_startup.py`
- `/Users/ryanliu/Documents/jcode/scripts/bench_compile.sh`
- `/Users/ryanliu/Documents/jcode/scripts/benchmark_tools.sh`
- `/Users/ryanliu/Documents/jcode/scripts/bench_memory_cli.py`
- `/Users/ryanliu/Documents/jcode/scripts/desktop_perf_report.py`

No dynamic benchmarks were run. Any speedup numbers in this document are hypotheses requiring benchmark validation.

## Executive Judgment

jcode's performance advantage is not one magic optimization. It has many small hot-path decisions:

- fast file search with `ignore::WalkBuilder`,
- split prompt streaming and cache-preserving suffixes,
- keepalive/fallback stream handling,
- session snapshot plus journal,
- explicit perf modes,
- startup/tool/memory benchmark scripts,
- allocator hooks and memory diagnostics.

uClaw's advantage is broader observability and runtime ambition:

- token budget collector,
- tracing/metrics,
- crash recovery,
- harness,
- tool output budget/overflow artifacts,
- Context Fabric and Capability Mesh direction.

uClaw should import jcode's hot-path engineering without collapsing its broader Agent OS runtime.

## Performance Matrix

| Dimension | jcode | uClaw | Gap | uClaw Improvement | Risk |
|---|---|---|---|---|---|
| Startup | Thin CLI/server entry, explicit startup timing | Tauri setup, AppState, HTTP runtime, service boot, gbrain/memU integration | uClaw has more unavoidable work | Add stage timing, visible-ready split, lazy heavy bridges | Boot races |
| Service lifecycle | Server runtime owns daemon lifecycle | ServiceManager starts/stops sequentially with 5s stop timeout | Dependency graph not explicit | Add service dependency groups and timings | Wrong parallelization |
| File search | `ignore::WalkBuilder`, gitignore aware, parallel, blocking pool | Custom recursive search in Rust tools | uClaw likely slower on large repos | Replace grep/glob internals with ignore walker under `spawn_blocking` | Permission/path semantic drift |
| Streaming | Keepalive, split prompt, provider session, WS/fallback patterns | Provider-specific streaming, Anthropic cache controls, retry | No unified stream supervisor | Add provider-agnostic stream supervisor | Conflicting retry semantics |
| Prompt cache | Split prompt and cache-preserving memory suffix | Byte-stable prompt, cache policy, token snapshots | Both strong; regression tests lacking | Add cache regression harness | Cache markers are order-sensitive |
| Persistence | Snapshot + append journal + startup stub | SQLite/FTS and rollout JSONL | Rich DB but possible hot mutex | Add projection journal for task/session runtime | DB/projection divergence |
| DB concurrency | File/JSON/session patterns, less central DB pressure | `Arc<std::sync::Mutex<rusqlite::Connection>>` central connection | Possible p95 stalls under mixed UI/agent load | Measure before pool/actor refactor | Migration/transaction complexity |
| Process execution | Background/progress/checkpoint conventions | Shell timeout, output cap, `kill_on_drop`, approvals | uClaw shell is safe but progress protocol weaker | Add process registry and progress/checkpoint events | Leaked process state |
| Event bus | Domain-specific stream/server events | Single broadcast bus plus recent ring | High-frequency lag/drop not visible | Add lag/drop counters and topic QoS | Overbuilt event plane |
| Memory/RSS | jemalloc feature, process memory diagnostics, session memory bench | Tauri + WebView + Python + Bun + Chromiumoxide | uClaw footprint naturally larger | Lazy heavy bridges; RSS scorecards | First-use latency |
| Build profiles | selfdev/release-lto/debug profile tuning | Release mostly `strip = true`; dev profile less tuned | jcode build profile more deliberate | Add measured dev/selfdev profiles | Debuggability loss |
| Bench discipline | Startup/compile/tool/memory scripts | Harness mostly functional, not perf-focused | uClaw lacks app-level performance gates | Add ignored perf harness and JSON artifacts | CI noise |

## Immediate High-Value Work

### 1. Replace grep/glob internals

Target:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/builtin/search.rs`

Import pattern:

- jcode `src/tool/grep.rs` and `src/tool/glob.rs`.
- `ignore::WalkBuilder`.
- Parallel walker.
- Respect `.gitignore`.
- Early quit.
- Avoid async recursive directory walking for CPU/filesystem-heavy search.
- Run blocking filesystem traversal inside `spawn_blocking`.

Verification:

```bash
cargo test -p uclaw --lib agent::tools::builtin::search
```

Benchmark:

```bash
cargo test -p uclaw --test perf_file_search -- --ignored --nocapture
```

Risk:

- Search result ordering changes.
- Ignore behavior changes.
- Path approval integration must remain intact.

### 2. Add stream supervisor

Target:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/llm_stream.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/llm/providers/*`

Import pattern:

- connect timeout,
- first-event timeout,
- per-chunk stall timeout,
- complete timeout,
- keepalive,
- retry budget,
- provider transport fallback,
- provider lifecycle events.

Do not copy OpenAI persistent WebSocket behavior as a universal default. Put transport support behind provider capability metadata.

Verification:

```bash
cargo test -p uclaw --lib agent::llm_stream llm::providers
```

### 3. Add task/session journal for projections

Target:

- `runtime/rollout.rs`
- `runtime/task.rs`
- `agent/session.rs`
- `harness/trace.rs`

Import pattern:

- snapshot + append-only journal,
- startup stub for fast listing,
- slow-save telemetry,
- corruption recovery.

Do not replace SQLite. Journal should accelerate task projection and recovery, not become a second canonical session store.

Verification:

```bash
cargo test -p uclaw --lib runtime::rollout agent::session harness::trace
```

### 4. Add performance harness

Target:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness`
- new ignored tests or benchmark binaries.

Scorecards:

- startup stages,
- file search,
- DB contention,
- stream stall/retry,
- prompt cache,
- tool output budget,
- RSS for main/gbrain/memU/browser.

Output:

- JSON artifact per run.
- p50/p95/p99 where meaningful.
- commit hash, OS, machine, feature flags.

### 5. Lazy heavy bridges

Candidates:

- memU Python bridge,
- gbrain Bun bridge,
- browser context,
- embedding model,
- local API handlers that pull heavy dependencies.

Before changing behavior, measure:

- visible-ready time,
- first-use latency,
- memory/RSS,
- failure rate.

## Build Profile Recommendations

jcode has more deliberate Cargo profile tuning. uClaw should add profiles only after measuring local impact.

Candidate direction:

```toml
[profile.dev]
incremental = true

[profile.selfdev]
inherits = "dev"
debug = 0
incremental = true

[profile.release-lto]
inherits = "release"
lto = "thin"
codegen-units = 16
strip = true
```

Risks:

- Less debug info can hurt crash diagnosis.
- Tauri/macOS symbolication may get worse.
- CI and local developer profile needs differ.

Recommendation:

- Add profiles behind documented commands.
- Do not force all developers into low-debug profiles.
- Measure `cargo check`, `cargo test --no-run`, and `cargo tauri build`.

## Allocator Recommendations

jcode has optional jemalloc support and allocator diagnostics. uClaw should treat allocator changes as experimental.

Why cautious:

- uClaw is multi-process: Tauri/WebView, Rust backend, Python memU, Bun/gbrain, browser contexts.
- Main Rust allocator changes may not solve total app memory.
- macOS allocator behavior differs from Linux.

Safe first step:

- Add process/RSS sampling by component.
- Track Rust main process separately from child processes.
- Only then test jemalloc feature.

Potential verification:

```bash
cargo build -p uclaw --features jemalloc
```

Only add this if the feature exists and is gated per platform.

## Hypotheses Requiring Benchmarks

| Hypothesis | Benchmark |
|---|---|
| `ignore::WalkBuilder` improves uClaw file search significantly | large/medium/small repo, cold/warm FS cache, regex/glob/CJK |
| `Arc<Mutex<rusqlite::Connection>>` causes p95 stalls | mixed reader/writer benchmark with wait time and query time |
| ServiceManager serial boot delays visible-ready | per-service timing; compare serial vs dependency groups |
| Split prompt reduces Anthropic cost | repeated 10-turn tool loop with cache read/create tokens |
| Lazy gbrain/memU lowers cold RSS | eager vs lazy cold-start and first-use latency |
| Tool overflow artifacts reduce token use without hurting success | tasks with large outputs, compare success and token cost |
| Broadcast bus drops/lag under high event rates | N subscribers, event rate ladder, lag/drop counters |
| Vite manual chunks cause route-open jank | Playwright route timing plus long-task sampling |

## Additional ADR-Aligned Performance Work

Second-pass review adds four performance areas that matter for Agent OS v2 but were thin in the first report.

### 1. Team and subagent runtime

jcode references:

- `/Users/ryanliu/Documents/jcode/src/tool/task.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/batch.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/communicate.rs`
- `/Users/ryanliu/Documents/jcode/scripts/benchmark_swarm.py`

uClaw references:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/teams/orchestrator.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/teams/channel.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/workers/spec.rs`

Performance gaps:

- uClaw team workers currently risk unbounded channel/event growth without a formal p95/p99 team-run scorecard.
- jcode has explicit parallel batch limits and swarm benchmark scripts.
- uClaw should benchmark worker spawn latency, worker turn budget enforcement, channel fanout lag, reviewer-cycle latency, and cleanup time.

Recommended scorecard:

```bash
cargo test -p uclaw --lib workers agent::teams -- --nocapture
cargo test -p uclaw --test perf_team_runtime -- --ignored --nocapture
```

### 2. Browser provider runtime

jcode references:

- `/Users/ryanliu/Documents/jcode/src/tool/browser.rs`
- `/Users/ryanliu/Documents/jcode/src/browser.rs`

uClaw references:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/context_manager.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/agent_loop.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/task_store.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/adapters/browser.rs`

Performance gaps:

- uClaw has the richer browser runtime, but needs provider-neutral latency metrics: launch, auth-profile apply, observe, screenshot, action, boundary detection, checkpoint, resume.
- jcode has useful readiness probes and setup status messages; uClaw should add readiness/status timing to the future `BrowserProvider` trait.

Recommended scorecard:

```bash
cargo test -p uclaw --lib browser::agent_loop browser::task_store harness::adapters::browser
cargo test -p uclaw --test perf_browser_provider -- --ignored --nocapture
```

### 3. Ambient and scheduled work

jcode references:

- `/Users/ryanliu/Documents/jcode/src/ambient/runner.rs`
- `/Users/ryanliu/Documents/jcode/src/ambient/scheduler.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/ambient.rs`

uClaw references:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/automation/runtime/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/automation/sources/schedule.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/proactive/*`

Performance gaps:

- jcode ambient reserves user token headroom and adapts wake intervals after rate-limit pressure.
- uClaw automation should measure wake latency, queue delay, escalation latency, cost-cap stop time, and whether scheduled work starves active user sessions.

Recommended scorecard:

```bash
cargo test -p uclaw --lib automation::runtime automation::sources::schedule proactive
cargo test -p uclaw --test perf_scheduled_worker -- --ignored --nocapture
```

### 4. Harness as a performance gate

jcode references:

- `/Users/ryanliu/Documents/jcode/src/bin/harness.rs`
- `/Users/ryanliu/Documents/jcode/scripts/benchmark_tools.sh`
- `/Users/ryanliu/Documents/jcode/scripts/bench_startup.py`
- `/Users/ryanliu/Documents/jcode/scripts/benchmark_swarm.py`

uClaw references:

- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/runtime.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/trace.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/self_improvement.rs`

uClaw harness is architecturally stronger, but jcode's CLI harness is easier to run quickly. Add a thin uClaw tool-smoke harness that runs without models and emits JSON artifacts for tool latency, output size, and failure category.

## Not Recommended

Do not:

- Replace uClaw SQLite persistence with jcode JSON session storage.
- Copy jcode TUI redraw/FPS policy directly into React.
- Make OpenAI persistent WebSocket a generic provider assumption.
- Enable jemalloc by default before measuring uClaw's multi-process RSS.
- Add a second event bus that bypasses TaskEvent/WorldProjection.

## Suggested Priority Order

1. File search benchmark and replacement.
2. Performance harness JSON scorecards.
3. DB contention measurement.
4. Stream supervisor.
5. Task/session projection journal.
6. Lazy heavy bridges.
7. Cargo profile tuning.
8. Allocator experiments.

## Bottom Line

jcode's performance advantage is practical and grounded in hot paths. uClaw's best move is to copy the discipline:

- benchmark first,
- optimize measured hot paths,
- keep the Agent OS runtime contracts as the integration layer.
