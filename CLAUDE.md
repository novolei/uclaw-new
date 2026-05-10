# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

This file has two halves:

1. **Working Style** — behavioral guardrails that bias toward correctness over speed (adapted from [andrej-karpathy-skills/CLAUDE.md](https://github.com/forrestchang/andrej-karpathy-skills/blob/main/CLAUDE.md), with uClaw-specific examples).
2. **Project Reference** — architecture, commands, and recurring gotchas you need to be effective in this codebase.

Read both before any non-trivial edit. For genuinely trivial work (one-line fix, doc typo) use judgment; the Working Style still applies.

---

# Part 1 — Working Style

## 1. Think before coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask one question and wait — don't fan out into speculative work.
- If multiple interpretations exist, present them — don't silently pick one.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

**uClaw-specific surfaces to check before assuming:**
- **Migration version numbers.** New schema work picks the next free integer in `src-tauri/src/db/migrations.rs` AND must coordinate with any open PR that's claimed a number — see *Active migration registry* below. Two PRs reusing the same V-number is the most common merge accident in this repo.
- **The agent loop is pure Rust.** There is no Claude Code SDK / Anthropic SDK in the agent path. If you find frontend code that looks SDK-shaped (`SDKMessage`, `useSDKRenderer`, etc.), that's Proma-migration leftover — verify whether it actually executes before relying on it.
- **Two storage tables per domain.** Chat lives in `messages`; agent lives in `agent_messages` (the visible conversation) **and** `agent_turns` (per-tool-call breakdown). Search/index/migration work must touch the right one — check counts: a typical dev DB has ≫ rows in `agent_messages` and `agent_turns`, often 0 in `messages`.

## 2. Simplicity first

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

**uClaw-specific:** when extending a feature that already has a flat shape (e.g. the existing `search_conversations` UNION-of-branches pattern), add another branch in the same file rather than introducing a new abstraction layer. The codebase favors flat enumeration over generic dispatchers — match it.

## 3. Surgical changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that **your** changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: every changed line should trace directly to the user's request.

**uClaw-specific exceptions where extra changes ARE warranted:**
- A new Tauri command requires **two** edits: define in `tauri_commands.rs` AND register in the `invoke_handler!` macro in `main.rs`. Forgetting the macro entry compiles fine but fails at runtime.
- A new background service requires registration in the `[Stage 3]` block in `main.rs`.
- A new built-in agent tool requires registration in `agent/dispatcher.rs` and, if destructive, in `SafetyManager`.

If you have to make these adjacent edits, call them out in the commit body — they look like scope creep but aren't.

## 4. Goal-driven execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let the loop run independently. Weak criteria ("make it work") force constant clarification.

**uClaw-specific:** the verification commands you should reach for first:
- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — full backend compile, errors only
- `cd src-tauri && cargo test --lib [filter]` — unit tests (defined inline; no integration dir)
- `cd ui && npx tsc --noEmit 2>&1 | head -10` — TS check
- `cd ui && npm test -- --run 2>&1 | tail -10` — Vitest, jsdom

Bisectability: prefer one logical change per commit. The plans in `docs/superpowers/plans/*.md` follow this — match the pattern when you ship.

## 5. Workflow skills

This repo uses the [superpowers](https://github.com/anthropics/superpowers) skill set. For non-trivial work — anything that would touch more than a couple of files or introduce a new concept — use the skill workflow rather than ad-hoc implementation:

1. **`superpowers:brainstorming`** — turn an idea into a design doc in `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`.
2. **`superpowers:writing-plans`** — turn the spec into a task-by-task plan in `docs/superpowers/plans/<feature>.md` with bite-sized steps and exact code.
3. **`superpowers:subagent-driven-development`** — execute the plan via fresh subagents, one task per dispatch, with two-stage review (spec compliance, then code quality).

Skip the workflow only for: typos, single-line fixes, doc-only changes, or hotfixes that have an obvious root cause and a localized fix (≤ 1 file).

The PR pattern that's worked well in this repo: one branch per plan, one commit per plan task, opened as one PR with a `## Commits (bisectable)` table in the description. See PRs #29, #31, #33, #35, #36 for shape.

## 6. When you find a real bug

If you discover a bug that's outside the current task's scope but you're confident is real (root cause identified, low-risk fix), spin it off as its own small PR rather than:

- Folding it into the current task (scope creep + bisectability loss), or
- Leaving it for later (it'll get forgotten).

If you can't unambiguously identify the root cause, surface it in your status report instead of patching symptoms.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, clarifying questions come before implementation rather than after mistakes, and migration version numbers stop colliding across open PRs.

---

# Part 2 — Project Reference

## Project Overview

**uClaw** is an AI-powered desktop coworker built as a Tauri v2 application. The Rust backend (`uclaw_core` crate) hosts the agent, LLM providers, MCP integration, memory subsystems, and a local HTTP API. The React + Vite frontend (`ui/`) builds into `static/` and is served by Tauri. A bundled Python runtime (`src-tauri/pyembed/`) drives the **memU** memory service via a JSON-RPC stdio bridge.

The original migration target documented in `docs/uclaw-migration-plan.md` mentions Svelte 5, but the implementation is React 18 + TypeScript with Tailwind and Radix UI primitives — trust the code, not that doc.

## Common Commands

Tauri orchestrates dev/build via `src-tauri/tauri.conf.json`, which calls the `ui/` npm scripts itself. From `src-tauri/`:

- `cargo tauri dev` — runs `cd ../ui && npm run dev` (Vite at `:5173`) and starts the Rust app pointed at `devUrl`.
- `cargo tauri build` — runs `cd ../ui && npm run build` (outputs to `../static`) and produces a release bundle.
- `cargo build` / `cargo build --release` — Rust-only build of the `uclaw` binary and `uclaw_core` library.
- `cargo test [-- <filter>]` — runs Rust unit tests (defined inline with `#[cfg(test)]` across modules like `infra/service.rs`, `providers/*`, `memory.rs`, `skills.rs`, `agent/tools/builtin/shell.rs`). There is no integration-test directory.

From `ui/` (only when iterating on the frontend in isolation):
- `npm run dev` / `npm run build` / `npm run preview`
- `npm test -- --run` — Vitest suite (jsdom environment). Tests live next to the components they exercise: `*.test.tsx`. The setup file at `ui/src/test-utils/setup.ts` shims `localStorage` for `atomWithStorage` under jsdom — don't remove it.

Bootstrap of the embedded Python (required before first run if `src-tauri/pyembed/` is empty — the directory is gitignored):
- `./scripts/setup-python-env.sh` — downloads python-build-standalone (Python 3.13) matching the host arch into `src-tauri/pyembed/python/` and pip-installs `memu` (preferring a local checkout at `~/Documents/memU` if present) plus `fastembed`.
- `./scripts/setup-python-env.sh --optimize` — same, then strips `__pycache__`, tests, idle/turtle to shrink the bundle.
- `./scripts/setup-python-env.sh --clean` — wipes `pyembed/`.

## Runtime Layout

On first launch the Rust side creates:
- `~/.uclaw/` — `config.json`, `llm_config.json`, `uclaw.db` (main SQLite), plus per-feature DBs (`memorization.db`, `proactive.db`).
- `~/Documents/workground/` — workspace root for artifacts/files exposed to the agent.

A local HTTP API listens on `127.0.0.1:27270` (Axum, with WebSocket support). It is spun up in a dedicated thread with its own Tokio runtime in `main.rs` and is independent of Tauri's async runtime — keep that boundary in mind when adding handlers.

## Backend Architecture (`src-tauri/src/`)

`main.rs` is intentionally thin: it builds `AppState`, spawns the HTTP server thread, then drives a **phased boot** sequence inside `tauri::async_runtime::spawn` against the `ServiceManager`. Stages are logged with `[Stage 3]` (registration) and `[Stage 4]` (start). The `WindowEvent::Destroyed` hook stops services and shuts down the memU client synchronously on quit.

`AppState` (`app.rs`) is the central DI container managed via `tauri::Manager`. It owns the SQLite connection, settings, `SessionManager`, `ProviderService`, `SafetyManager`, `MemoryStore`, `MemoryGraphStore`, `SkillsRegistry`, `SharedMcpManager`, `ChannelManager`, the optional `MemUClient`, the `InfraService` message bus, the `ServiceManager`, and a `PendingApprovals` map (oneshot channels that gate tool execution on user approval).

Module roles:
- `agent/` — agentic loop (`agentic_loop.rs`), tool dispatcher, sessions, teams (`agent/teams/`), and built-in tools (`tools/builtin/`: file, edit, search, shell, web, plan, self_eval) plus MCP and memU tool adapters. The loop follows: `check_signals → compress_context → before_llm → call_llm → handle_response → after_iteration`. **Cost capture** lives at `agent/dispatcher.rs::emit_turn_cost` — it both emits the IPC event AND persists to `cost_records` (V13).
- `llm/` and `providers/` — two layers: `llm/` provides the lower-level provider trait + `anthropic`/`openai` clients; `providers/` is the higher-level configuration/registry/service wrapping multiple providers with credential storage. `rig-core` is also a dependency. Allowed connect-src origins are pinned in `tauri.conf.json`'s CSP (Anthropic, OpenAI, DeepSeek, Gemini, Groq, OpenRouter). **Streaming has tiered timeouts** — `connect_timeout=15s`, `STREAM_STALL_TIMEOUT=45s` per chunk, `COMPLETE_TIMEOUT=120s` overall. See `llm/stream_error.rs::classify_stream_error` for the retry-vs-fail decision.
- `api/` — HTTP/WebSocket layer (`router.rs`, `handlers/`, `auth.rs`, `ws.rs`) serving the local API on port 27270. JWT secret is generated at startup, not persisted. Handler modules cover: agent, artifacts, auth, chat, config, spaces.
- `local_api/` — separate HTTP API server module (`mod.rs`, `routes.rs`, `server.rs`); distinct from `api/`.
- `mcp.rs` — Model Context Protocol server management (add/remove/connect/restart, tool listing).
- `skills.rs` + `proactive/scenarios/skill_extraction.rs` — Skills are both **static** (declared in registry) and **learned** (extracted by the skill-extraction proactive scenario). Top-level `skills/` directory holds skill definition files.
- `memory.rs` (key-value memory store), `memory_graph/` (Steward-style graph memory exposed via `memory_graph_*` Tauri commands), and `memu/` (Python bridge — `client.rs` is the Rust side, `bridge.rs` manages the subprocess, `memu_bridge.py` is the Python entrypoint bundled as a Tauri resource).
- `proactive/` — background scenario runner with four scenarios: `conversation_learning`, `skill_extraction`, `multimodal_context`, `types`. Each implements the `Scenario` trait registered into a `ScenarioManager` and gated on `MemubotConfig` flags.
- `automation/` — automation runtime, specs, and service (Phase 3 browser automation via `chromiumoxide`). Also see `browser/`.
- `observability/` — metrics and tracing infrastructure.
- `workspace/` — workspace management for `~/Documents/workground/`.
- `harness/` — evaluation harness for agent testing.
- `services/` + `infra/` — `ServiceManager` is a generic lifecycle manager (`register`, `start_all`, `stop_all`) and `InfraService` is the in-process message bus that services subscribe to. `PowerService`, `MemorizationService`, `ProactiveService`, `LocalApiService` all plug in here.
- `safety/` — `SafetyManager` enforces tool policies; risky tool calls go through `pending_approvals` and require a `approve_tool_call` Tauri command response.
- `tauri_commands.rs` — single flat module exposing every IPC command. Adding a new command requires both defining it here **and** listing it in the `invoke_handler!` macro in `main.rs`.
- `cost_store.rs` — per-turn cost persistence into `cost_records`. Best-effort — failures logged + swallowed so cost capture never fails the agent loop.
- `db/migrations.rs` — embedded migrations run on every startup against the opened connection. Each migration is idempotent (uses `IF NOT EXISTS` or wraps statements in error-tolerant loops). The top-level `migrations/` directory is empty/unused.
- `memubot_config.rs` — config struct controlling which proactive scenarios and services are enabled. Boot is data-driven from this config.
- `secrets/` — credential management for provider API keys.

### Active migration registry

Track which V-number is claimed by which open PR before starting schema work:

| V | What | Status |
|---|---|---|
| V1–V10 | Initial schema → V10 messages_fts (unicode61) | merged |
| V11 | trigram tokenizer for messages_fts + agent_turns_fts | **PR #33** (open) |
| V12 | agent_messages_fts (trigram) + sync triggers + backfill | merged |
| V13 | cost_records + indexes | merged (PR #39) |
| V14 | tool_permission_rules + permission_audit_log | **in flight** (P6) |

If you're adding a migration: pick the next number after both merged AND open PRs to avoid conflicts. Update this table in your PR.

## Frontend Architecture (`ui/src/`)

- React 18 + TypeScript, Vite (port `5173`, strict), Tailwind, Radix UI, Jotai for state, `react-markdown` + `shiki` for rendering, `sonner` for toasts.
- `@/*` path alias maps to `ui/src/*` (see `vite.config.ts` and `tsconfig.json`).
- Build output goes to `../static`, which Tauri serves as `frontendDist`.
- Manual chunk splitting in `vite.config.ts`: `react`, `tauri`, `vendor` (jotai/clsx/tailwind-merge).
- Components are organized by feature (`agent/`, `chat/`, `artifacts/`, `automation/`, `memory/`, `mcp` lives under `config/`, `settings/`, etc.); UI primitives live in `components/ui/`.
- State is managed via Jotai atoms (`atoms/` — 27+ atom files organized by feature).
- All backend interaction is via `@tauri-apps/api` `invoke()` against the commands listed in `tauri_commands.rs`. Lower-level IPC types are in `lib/tauri-bridge.ts`.
- **Theming**: 11 themes defined in `ui/src/styles/globals.css` as CSS variables (`--popover`, `--accent`, `--border`, etc.). New components must use the theme tokens (`bg-popover`, `text-muted-foreground`) rather than hardcoded colors (`bg-zinc-900`, `text-gray-500`) — hardcoded values break under warm-paper / qingye / forest-* themes.
- **Tests**: Vitest + React Testing Library + jsdom. `renderWithProviders` from `ui/src/test-utils/render.tsx` wraps in JotaiProvider + Tooltip + a fresh store. Recharts is finicky under jsdom — mock it in tests for components that use it.

## Working in This Repo

When changing IPC surface area: add the command function in the appropriate `tauri_commands.rs` section, register it in the `invoke_handler!` macro in `main.rs`, and call it from the frontend with `invoke('command_name', { ... })`.

When adding a background service: implement the service, register it inside the `[Stage 3]` block in `main.rs` (gated on the relevant `memubot_config` flag), and `service_manager.start_all()` will pick it up.

When adding a built-in agent tool: drop it under `agent/tools/builtin/` and register through the dispatcher in `agent/dispatcher.rs`. If the tool can be destructive, surface it through `SafetyManager` so the approval flow works.

When adding FTS coverage of a new table: don't forget the **backfill** step (`INSERT INTO …_fts(rowid, …) SELECT … FROM source WHERE rowid NOT IN (SELECT rowid FROM …_fts)`). Without it, search misses everything that pre-dates the migration.

The CSP in `tauri.conf.json` restricts `connect-src` to the allow-listed LLM provider domains. Adding a new provider requires updating that header along with `providers/registry.rs`.

The embedded Python directory is gitignored — assume it's missing on a fresh checkout and tell users to run `scripts/setup-python-env.sh` before `cargo tauri dev`. If `MemUClient` fails to start, `AppState.memu_client` is `None` and memU-dependent features degrade gracefully rather than aborting boot.
