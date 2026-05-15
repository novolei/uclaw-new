# CLAUDE.md

Guidance for Claude Code when working in this repository. Part 1 is uClaw-specific working style; Part 2 is project reference. Read both before any non-trivial edit.

General defaults (surface assumptions, prefer the minimum change, touch only what's required, define verifiable success criteria) are enforced by the `superpowers:*` skills — invoke them on non-trivial work. The notes below are the uClaw-specific overlay.

---

# Part 1 — Working Style

## Surfaces to check before assuming

- **Migration version numbers.** New schema work picks the next free integer in `src-tauri/src/db/migrations.rs` AND must coordinate with any open PR that's claimed a number — see *Active migration registry* in Part 2. Two PRs reusing the same V-number is the most common merge accident in this repo.
- **The agent loop is pure Rust.** No Claude Code SDK / Anthropic SDK in the agent path. Frontend code that looks SDK-shaped (`SDKMessage`, `useSDKRenderer`, etc.) is Proma-migration leftover — verify it actually executes before relying on it.
- **Two storage tables per domain.** Chat lives in `messages`; agent lives in `agent_messages` (the visible conversation) **and** `agent_turns` (per-tool-call breakdown). Search/index/migration work must touch the right one — a typical dev DB has ≫ rows in `agent_messages` and `agent_turns`, often 0 in `messages`.

## Match the codebase shape

When extending a feature that already has a flat shape (e.g. the existing `search_conversations` UNION-of-branches pattern), add another branch in the same file rather than introducing a new abstraction layer. uClaw favors flat enumeration over generic dispatchers — match it.

## Adjacent edits that look like scope creep but aren't

- **New Tauri command** → define in `tauri_commands.rs` AND register in the `invoke_handler!` macro in `main.rs`. Forgetting the macro entry compiles fine but fails at runtime.
- **New background service** → register in the `[Stage 3]` block in `main.rs`.
- **New built-in agent tool** → register in `agent/dispatcher.rs` and, if destructive, in `SafetyManager`.
- **Chat-composer behavior change** → uClaw has **two parallel composers** that wrap the same `RichTextInput`: `ui/src/components/chat/ChatInput.tsx` (Chat mode) and `ui/src/components/agent/AgentView.tsx` (Agent mode). Each owns its own `handlePasteFiles` / `handleDrop` / send wiring. Any paste / drop / attachment / submit behavior change must be applied to **both** files. Verifying only Chat mode hides regressions in the more common Agent mode (and vice versa). The shared `RichTextInput` is a [PLACEHOLDER] textarea today — a real TipTap port is scheduled for W4 of the Proma preview port; until then, prop wiring lives in the composers, not in RichTextInput.

Call these out in the commit body so they're not mistaken for scope creep.

## Verification commands

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — backend compile, errors only
- `cd src-tauri && cargo test --lib [filter]` — unit tests (inline `#[cfg(test)]`; no integration dir)
- `cd ui && npx tsc --noEmit 2>&1 | head -10` — TS check
- `cd ui && npm test -- --run 2>&1 | tail -10` — Vitest, jsdom

Bisectability: one logical change per commit. Match the plans in `docs/superpowers/plans/*.md`.

## Workflow

Non-trivial work goes through `superpowers:brainstorming` → `writing-plans` → `subagent-driven-development`, producing a spec in `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` and a plan in `docs/superpowers/plans/<feature>.md`. Skip only for typos, single-line fixes, doc-only changes, or hotfixes with an obvious root cause and a ≤ 1-file fix.

PR shape: one branch per plan, one commit per plan task, one PR with a `## Commits (bisectable)` table. See PRs #29, #31, #33, #35, #36.

### Skill entry points

Beyond the superpowers loop, reach for these at the matching stage:

- **Entering ideation** — `to-prd` turns the current conversation into a PRD on the GitHub issue tracker (alternative to going straight into `brainstorming`). `grill-me` interviews the user to stress-test a half-formed plan before committing.
- **Aligning with domain** — `grill-with-docs` challenges a plan against `CONTEXT.md` + `docs/adr/`. uClaw has neither today; bootstrap them (or skip this skill) before invoking.
- **Investigation** — `zoom-out` builds system-level context before changing an unfamiliar module (`automation/`, `memu/`, `proactive/`, `harness/`, `memory_graph/`). `prototype` runs a throwaway design validation (state machine, data model, or several UI variants on one route) before locking the plan.
- **Planning fan-out** — `to-issues` slices a plan into independently-grabbable GitHub issues (tracer-bullet vertical slices). Use only when work must be parallelized or assigned out — the default remains one PR with bisectable commits.
- **Refactor pass** — `improve-codebase-architecture` hunts consolidation / testability / AI-navigability wins. Also reads `CONTEXT.md` + `docs/adr/`; degrades gracefully without them.
- **Inbox** — `triage` walks incoming GitHub issues through a state machine to prepare them for AFK agents.
- **Comms** — `handoff` compacts the current conversation into a doc for a fresh agent (use before context overflow). `caveman` switches to ultra-compressed style when token budget matters.

Overlaps with already-loaded skills: prefer `superpowers:test-driven-development` over `tdd`, `superpowers:systematic-debugging` over `diagnose`, `superpowers:writing-skills` over `write-a-skill` — unless the mattpocock variant's tighter ritual is clearly the better fit (e.g. `diagnose` for a gnarly reproduce-minimize-hypothesize bug). Both are loaded; pick consciously.

## Real bugs found mid-task

If you discover a bug outside the current task's scope with a confident root cause and a low-risk fix, spin it off as its own small PR — don't fold it in (scope creep + bisectability loss) and don't leave it for later (it'll get forgotten). If the root cause isn't clear, surface it in your status report rather than patching symptoms.

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
| V14 | tool_permission_rules + permission_audit_log | merged (PR #41) |
| V15 | agent_messages metrics columns (duration_ms, token counts, cost) | merged |
| V16 | persist 'default' workspace + heal orphan agent_sessions | merged (PR #75) |
| V17 | spaces.sort_order + spaces.attached_dirs + agent_sessions.attached_dirs | merged (PR #76) |
| V18 | agent_sessions.pinned_at — canonical pin state for the agent UI | merged (PR #92) |
| V19 | spaces.skill_tags — per-workspace skill scoping (JSON tag array) | merged |
| V20 | rewrite automation_specs + activities + migrate legacy TOML | merged |
| V21 | automation_subscriptions + automation_memory + automation_escalations | merged |
| V22 | automation_installed_skills + idx_aut_inst_skills_slug | merged (PR #160) |
| V23a | Marketplace cache (Phase 3a) | merged |
| V24 | automation_activities +session_id +report_artifacts_json -tool_calls_json; agent_sessions +archived_at | PR #172 (Automation Phase 2a) |
| V25 | marketplace_standalone_installs (standalone skill/MCP install tracking) | merged (Phase 3b-γ) |
| V26 | conversations.archived + conversations.archived_at | in progress |

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

## Gotchas

The registration mechanics for Tauri commands, background services, and built-in agent tools are listed in *Part 1 — Adjacent edits*. From the frontend, call commands with `invoke('command_name', { ... })`. Background-service registration is gated on the relevant `memubot_config` flag.

- **FTS backfill.** When adding FTS coverage of a new table, don't forget `INSERT INTO …_fts(rowid, …) SELECT … FROM source WHERE rowid NOT IN (SELECT rowid FROM …_fts)`. Without it, search misses everything that pre-dates the migration.
- **CSP + providers.** Adding a new LLM provider requires updating both `providers/registry.rs` and the `connect-src` allow-list in `tauri.conf.json`'s CSP.
- **Embedded Python is gitignored.** Assume `src-tauri/pyembed/` is missing on a fresh checkout — run `scripts/setup-python-env.sh` before `cargo tauri dev`. If `MemUClient` fails to start, `AppState.memu_client` is `None` and memU-dependent features degrade gracefully rather than aborting boot.
