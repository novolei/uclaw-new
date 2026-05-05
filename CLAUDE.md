# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- There is no lint or test script wired up in the UI.

Bootstrap of the embedded Python (required before first run if `src-tauri/pyembed/` is empty — the directory is gitignored):
- `./scripts/setup-python-env.sh` — downloads python-build-standalone matching the host arch into `src-tauri/pyembed/python/` and pip-installs `memu` (preferring a local checkout at `~/Documents/memU` if present) plus `fastembed`.
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
- `agent/` — agentic loop (`agentic_loop.rs`), tool dispatcher, sessions, and built-in tools (`tools/builtin/`: file, edit, search, shell, web) plus MCP and memU tool adapters.
- `llm/` and `providers/` — two layers: `llm/` provides the lower-level provider trait + `anthropic`/`openai` clients; `providers/` is the higher-level configuration/registry/service wrapping multiple providers with credential storage. `rig-core` is also a dependency. Allowed connect-src origins are pinned in `tauri.conf.json`'s CSP.
- `mcp.rs` — Model Context Protocol server management (add/remove/connect/restart, tool listing).
- `skills.rs` + `proactive/scenarios/skill_extraction.rs` — Skills are both **static** (declared in registry) and **learned** (extracted by the skill-extraction proactive scenario).
- `memory.rs` (key-value memory store), `memory_graph/` (Steward-style graph memory exposed via `memory_graph_*` Tauri commands), and `memu/` (Python bridge — `client.rs` is the Rust side, `bridge.rs` manages the subprocess, `memu_bridge.py` is the Python entrypoint bundled as a Tauri resource).
- `proactive/` — background scenario runner. Three scenarios are registered conditionally based on `MemubotConfig`: `conversation_learning`, `skill_extraction`, `multimodal_context`. Each implements the `Scenario` trait registered into a `ScenarioManager`.
- `services/` + `infra/` — `ServiceManager` is a generic lifecycle manager (`register`, `start_all`, `stop_all`) and `InfraService` is the in-process message bus that services subscribe to. `PowerService`, `MemorizationService`, `ProactiveService`, `LocalApiService` all plug in here.
- `safety/` — `SafetyManager` enforces tool policies; risky tool calls go through `pending_approvals` and require a `approve_tool_call` Tauri command response.
- `tauri_commands.rs` — single flat module exposing every IPC command. Adding a new command requires both defining it here **and** listing it in the `invoke_handler!` macro in `main.rs`.
- `api/` — HTTP/WebSocket layer (`router.rs`, `handlers/`, `auth.rs`, `ws.rs`). JWT secret is generated at startup, not persisted.
- `db/migrations.rs` — embedded migrations run on every startup against the opened connection. The top-level `migrations/` directory is empty/unused.
- `memubot_config.rs` — config struct controlling which proactive scenarios and services are enabled. Boot is data-driven from this config.

## Frontend Architecture (`ui/src/`)

- React 18 + TypeScript, Vite (port `5173`, strict), Tailwind, Radix UI, Jotai for state, `react-markdown` + `shiki` for rendering, `sonner` for toasts.
- `@/*` path alias maps to `ui/src/*` (see `vite.config.ts` and `tsconfig.json`).
- Build output goes to `../static`, which Tauri serves as `frontendDist`.
- Manual chunk splitting in `vite.config.ts`: `react`, `tauri`, `vendor` (jotai/clsx/tailwind-merge).
- Components are organized by feature (`agent/`, `chat/`, `artifacts/`, `memory/`, `mcp` lives under `config/`, `settings/`, etc.); UI primitives live in `components/ui/`.
- All backend interaction is via `@tauri-apps/api` `invoke()` against the commands listed in `tauri_commands.rs`.

## Working in This Repo

When changing IPC surface area: add the command function in the appropriate `tauri_commands.rs` section, register it in the `invoke_handler!` macro in `main.rs`, and call it from the frontend with `invoke('command_name', { ... })`.

When adding a background service: implement the service, register it inside the `[Stage 3]` block in `main.rs` (gated on the relevant `memubot_config` flag), and `service_manager.start_all()` will pick it up.

When adding a built-in agent tool: drop it under `agent/tools/builtin/` and register through the dispatcher in `agent/dispatcher.rs`. If the tool can be destructive, surface it through `SafetyManager` so the approval flow works.

The CSP in `tauri.conf.json` restricts `connect-src` to the allow-listed LLM provider domains. Adding a new provider requires updating that header along with `providers/registry.rs`.

The embedded Python directory is gitignored — assume it's missing on a fresh checkout and tell users to run `scripts/setup-python-env.sh` before `cargo tauri dev`. If `MemUClient` fails to start, `AppState.memu_client` is `None` and memU-dependent features degrade gracefully rather than aborting boot.
