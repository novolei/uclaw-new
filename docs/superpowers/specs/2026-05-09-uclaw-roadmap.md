# uClaw Roadmap — Priority A–D Items

**Status:** Draft, awaiting human review.
**Date:** 2026-05-09
**Author:** Claude (after a full session of chat-layer fixes — PRs #2–#18)

This document is the *master roadmap*, not an implementation plan. It enumerates every Priority A–D item from the post-session recommendation, scopes each into a self-contained sub-plan, and sequences them. Each sub-plan listed here will be broken out into its own document under `docs/superpowers/plans/` (file names suggested below) when we're ready to start it. Each individual plan must produce working, testable software on its own — no plan depends on another being half-finished.

---

## Goals

1. Land every Priority A–D recommendation as merged PRs.
2. Each plan is independently executable and reviewable.
3. The work stays bisectable — every PR keeps `cargo build`, `tsc --noEmit`, and `vite build` green.
4. Chat-layer regressions from PRs #2–#18 are caught by tests before they reach main.

## Non-Goals

- Build a brand-new feature not on the A–D list (e.g. voice input, streaming UI overhaul, plugin SDK). Hold for a separate roadmap.
- Re-architect the agent loop. Patches inside `agentic_loop.rs` / `dispatcher.rs` are fine; rewrites are not.
- Touch the embedded Python / memU layer outside what specific plans require.

## Tech Stack (existing)

- **Backend:** Rust (Tauri v2), `rusqlite`, `tokio`, `axum`. Tests: `cargo test`.
- **Frontend:** React 18 + TypeScript, Vite, Tailwind, Radix UI, Jotai. No tests yet — added by Plan 3.
- **DB:** SQLite at `~/.uclaw/uclaw.db`, migrations in `src-tauri/src/db/migrations.rs`.
- **State:** Jotai atoms (`ui/src/atoms/`), event-driven via `chat:stream-*` Tauri events.

---

## Dependency Graph

```
                  ┌──────────────────────────────────────────────────┐
                  │  P1  Cleanup batch (A2 + A3 + A4 + A5)            │
                  │  ── independent, can land first to clear noise   │
                  └──────────────────────────────────────────────────┘
                                       │
                                       ▼
                  ┌──────────────────────────────────────────────────┐
                  │  P3  Frontend test infrastructure (C1)            │
                  │  ── needed before any user-facing UI plan         │
                  └──────────────────────────────────────────────────┘
                              │                       │
                              ▼                       ▼
                ┌──────────────────────┐  ┌──────────────────────────┐
                │ P2  SDK message       │  │ P4  Conversation FTS     │
                │ persistence (A1)      │  │ search (B1)              │
                │ ── unblocks B2/B6     │  │ ── self-contained        │
                └──────────────────────┘  └──────────────────────────┘
                       │                                   │
        ┌──────────────┼──────────────────────────┐        │
        ▼              ▼                          ▼        │
  ┌─────────┐   ┌────────────┐    ┌──────────────────┐    │
  │ P5 Cost  │   │ P6 Permis- │    │ P7 Edit /         │    │
  │ dashboard│   │ sion UI    │    │ regenerate (B2)   │    │
  │ (B3)     │   │ (B4)       │    │                   │    │
  └─────────┘   └────────────┘    └──────────────────┘    │
                                            │              │
                                            ▼              ▼
                                  ┌────────────────┐  ┌──────────────┐
                                  │ P8 MCP discov- │  │ P12 Sanity    │
                                  │ ery (B5)       │  │ sweep (C2)    │
                                  └────────────────┘  └──────────────┘
                                            │
                                            ▼
                                  ┌────────────────┐
                                  │ P9 Agent teams │
                                  │ UX (B6)        │
                                  └────────────────┘

  P10 Memory graph viz (D3)  ── independent, can run any time
  P11 First-run + examples (D1+D2) ── after P5 (cost setup helps onboarding)
  P13 Migration tests (C3)   ── adds tests over P2's V10 migration
```

**Critical path:** P1 → P3 → P2 → (P5 || P6 || P7) → P12. Everything else parallelizable.
**Estimated total:** ~30 calendar days at 1 plan/day pace, more realistically ~6 weeks with reviews.

---

# Sub-plans

Each section below is what will become its own `docs/superpowers/plans/YYYY-MM-DD-<name>.md` when started. Listed in recommended execution order.

---

## P1. Cleanup batch (A2 + A3 + A4 + A5)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p1-cleanup-batch.md`

**Goal:** Clear all known low-value noise from the repo in one focused pass: dead Rust code, oversized JS bundle, legacy type-file name, eager Shiki preload.

**Scope:**
- **A2** Delete `browser_tool!` macro (currently `unused_macros`), the unused `thinking_started` write at `src-tauri/src/agent/agentic_loop.rs:413` (gate by an actual side-effect or remove), and `pub struct TitleGenerated` (never constructed — find call sites or delete).
- **A3** Configure `vite.config.ts` `build.rollupOptions.output.manualChunks` to split routes: `Settings/*`, `Memory/*`, `Automation/*`, `Agent/AgentMessages` (the largest chunk per the warning) into separate async chunks. Verify gzip total stays ≤ pre-change.
- **A4** Rename `ui/src/lib/proma-types.ts` → split into `chat-types.ts` (PrimaChatMessage, ChatMessage, ChatToolActivity, ContentBlock siblings) and `agent-types.ts` (AgentMessage, AgentEvent, SDK*Message). Update ~80 import sites via codemod. The aliases (`ChatMessage` exported from both for back-compat) stay during transition; remove in a follow-up.
- **A5** In `ui/src/lib/highlight.ts`, switch `EXTRA_THEMES` and `COMMON_LANGUAGES` from eager preload to lazy `loadLanguage` / `loadTheme` calls inside `highlightCode` itself. Cache loaded names in a `Set` so repeated highlights don't re-load. Measure: initial bundle drops by ~150–200KB.

**Files to touch:**
- `src-tauri/src/agent/dispatcher.rs` (delete `browser_tool!` macro)
- `src-tauri/src/agent/agentic_loop.rs:413` (`thinking_started` warning)
- `src-tauri/src/<various>` (`TitleGenerated` — find with grep)
- `ui/vite.config.ts` (manualChunks)
- `ui/src/lib/proma-types.ts` (split)
- ~80 frontend import sites (renames)
- `ui/src/lib/highlight.ts` (lazy theme/language loading)

**Acceptance criteria:**
- `cargo build` shows 0 warnings (currently 3).
- `vite build` reports no chunk >500KB pre-gzip; `index-*.js` drops below 700KB.
- `tsc --noEmit` clean after rename.
- App still loads — smoke test agent + chat views.

**Estimated effort:** 0.5–1 day.

**Risks:**
- Rename codemod missing imports → caught by `tsc`.
- manualChunks can break async chunk boundaries — verify Tauri's `frontendDist` serving still works.

---

## P2. SDK message persistence (A1) + V10 migration

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p2-sdk-message-persistence.md`

**Goal:** Implement the `get_agent_session_sdk_messages` Tauri command end-to-end so the SDK renderer path activates for historical sessions. This is the *correct* solution to the persistence problem PRs #5–#13 patched with JSON-blob columns.

**Scope:**
- New schema (V10): `agent_sdk_messages (id PK, session_id FK, seq INTEGER, sdk_json TEXT, created_at INTEGER)`. Composite index on `(session_id, seq)`.
- Write path: hook into `dispatcher.rs` where the SDK emits assistant/user/system/result messages. Every emit also goes through a `record_sdk_message` call on a new `SdkMessageStore` (mirror of `TrajectoryStore`'s shape).
- Read path: implement `get_agent_session_sdk_messages` Tauri command — selects all rows for a session ordered by `seq`, returns parsed `Vec<serde_json::Value>` (frontend already expects `any[]`).
- Frontend: nothing to change — `AgentView.tsx:432` already calls the command and feeds `persistedSDKMessages`. Once non-empty, `useSDKRenderer` flips on automatically and PR #5–#13's JSON-blob columns become unused but harmless.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` (V10)
- `src-tauri/src/agent/sdk_store.rs` (new)
- `src-tauri/src/agent/dispatcher.rs` (write hook)
- `src-tauri/src/tauri_commands.rs` (new command + register in `main.rs`)
- `src-tauri/src/main.rs` (register command)

**Acceptance criteria:**
- Send a message in agent view; `sqlite3 ~/.uclaw/uclaw.db "SELECT count(*) FROM agent_sdk_messages WHERE session_id = ?"` returns >0 after stream-complete.
- Reload the session: thinking + tool + sub-turns render via `useSDKRenderer` path (verify by checking React DevTools that `AgentMessages` `useSDKRenderer` is `true`).
- No regression: `agent_messages.tool_activities_json` keeps populating (legacy path stays for compatibility).
- Migration is idempotent — running on a fresh and a populated DB both succeed.

**Estimated effort:** 2–3 days.

**Risks:**
- SDK message volume can be 10–50× larger than `agent_messages`. Index + truncation policy needed for very long sessions. **Mitigation:** add `length(sdk_json)` cap (e.g. 1MB per row) and drop SDK rows older than 90 days when cleaning.
- Race between write and the post-stream-complete reload could cause stale `persistedSDKMessages`. **Mitigation:** ensure the flush happens before `chat:stream-complete` is emitted, or pass a sequence cursor.

**Dependencies:** P1 (cleanup) recommended first to avoid touching code that's about to move.

---

## P3. Frontend test infrastructure (C1)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p3-frontend-tests.md`

**Goal:** Stand up Vitest + React Testing Library, write a small starter suite covering the chat layer, wire into `npm test`. This is **prerequisite for P4–P12** because every UI plan should add tests.

**Scope:**
- Add `vitest`, `@testing-library/react`, `@testing-library/jest-dom`, `jsdom`, `@vitest/ui` to `ui/package.json` devDeps.
- `ui/vitest.config.ts` (extends `vite.config.ts`, sets `environment: 'jsdom'`, includes `'src/**/*.test.{ts,tsx}'`).
- `ui/src/test-utils/render.tsx` — wrapper that mounts component with default JotaiProvider + Tooltip provider so individual tests don't repeat setup.
- `ui/src/test-utils/mock-tauri.ts` — vi.mock helpers for `@tauri-apps/api`'s `invoke` and `listen`.
- Starter tests:
  - `ChatToolBlock.test.tsx` — rendering matrix (success / error / running / expanded)
  - `ChatToolActivityIndicator.test.tsx` — start/result merge logic
  - `ChatAppearancePopover.test.tsx` — atom interactions
  - `useScrollPositionMemory.test.ts` — id change scrolls to bottom
- Add `"test": "vitest run"`, `"test:watch": "vitest"`, `"test:ui": "vitest --ui"` scripts.

**Files to touch:**
- `ui/package.json` (deps + scripts)
- `ui/vitest.config.ts` (new)
- `ui/src/test-utils/*` (new)
- 4 starter test files
- `.gitignore` (add `coverage/`)

**Acceptance criteria:**
- `cd ui && npm test` runs all 4 suites and passes.
- `npm run test:watch` triggers on save.
- A deliberately broken test (e.g. assert `Check` icon when `isError=true`) fails clearly.

**Estimated effort:** 1 day.

**Dependencies:** P1 (so the renamed type files don't churn the test imports later).

---

## P4. Conversation FTS search (B1)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p4-conversation-search.md`

**Goal:** Surface the existing `agent_turns_fts` virtual table through a global search UI. Click a result → jump to the message in its session.

**Scope:**
- Verify `agent_turns_fts` is being populated. Add population if missing (after every `record_turn`).
- Extend the FTS to also index `agent_messages.content` and `messages.content` so chat conversations are searchable too. New triggers in V11 migration.
- New Rust command: `search_conversations(query: String) -> Vec<SearchHit>` where `SearchHit { session_id, message_id?, turn_index?, snippet, kind: 'agent_turn'|'agent_message'|'chat_message', created_at }`. Use FTS5's `bm25()` ranking + `snippet()` builtin.
- Frontend: command palette via `cmdk`. Trigger: ⌘K. Shows recent conversations top, search results below as you type.
- Click → navigate to the session and `scrollToMessage(message_id)` via the existing `Conversation` context.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` (V11)
- `src-tauri/src/harness/trajectory.rs` (verify FTS population)
- `src-tauri/src/tauri_commands.rs` (new command)
- `src-tauri/src/main.rs` (register)
- `ui/src/components/search/SearchPalette.tsx` (new)
- `ui/src/atoms/search-atoms.ts` (new)
- `ui/src/lib/keyboard-shortcuts.ts` (⌘K binding)

**Acceptance criteria:**
- ⌘K opens palette anywhere in the app.
- Typing 3+ chars triggers search; results appear in <200ms for a DB with 10k turns.
- Click a result → app navigates to the right session and the target message is scrolled into view with a brief flash highlight.
- Empty input shows recent N conversations.
- Test (P3): mock invoke, render palette, type "hello", assert results render.

**Estimated effort:** 2 days.

**Dependencies:** P3 (tests).

---

## P5. Cost / token dashboard (B3)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p5-cost-dashboard.md`

**Goal:** Aggregate the per-turn cost data already captured (`agent_turns.duration_ms`, the `agent:turn_cost` events) into a per-session and per-day rollup the user can browse.

**Scope:**
- New schema (V12): `cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)`. Populate via existing `agent:turn_cost` event sink.
- New aggregation queries:
  - `get_session_costs(session_id) -> SessionCostRollup`
  - `get_daily_costs(days_back: u32) -> Vec<DailyCostRollup>`
  - `get_model_costs(days_back: u32) -> Vec<ModelCostRollup>`
- Settings → 用量 tab with three charts (daily total bar, per-model donut, per-session table) using `recharts`.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` (V12)
- `src-tauri/src/cost_store.rs` (new — write side, hooks into existing event publisher)
- `src-tauri/src/tauri_commands.rs` (3 new commands)
- `ui/src/components/settings/UsageSettings.tsx` (new)
- `ui/src/components/settings/Settings.tsx` (new tab entry)
- Add `recharts` to deps.

**Acceptance criteria:**
- After 5 conversations across 2+ models, dashboard shows non-zero values for all three views.
- Daily chart updates on new turn (real-time via event listener).
- Test (P3): aggregation function returns correct totals for fixture data.

**Estimated effort:** 2 days.

**Dependencies:** P3.

---

## P6. Tool permission UI (B4)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p6-permission-ui.md`

**Goal:** Replace the current bare-bones tool-approval modal with a proper permission center: per-tool defaults, per-session overrides, "always allow X command pattern" rules, audit log of every approved/blocked call.

**Scope:**
- Schema (V13): `tool_permissions (id, scope: 'global'|'session'|'pattern', target, mode: 'allow'|'block'|'ask', created_at)` and `permission_audit_log (id, session_id, tool_name, args_hash, decision, decided_by, created_at)`.
- Backend: extend `SafetyManager` (`src-tauri/src/safety/`) to consult the new tables before falling back to the global `SafetyMode`.
- Settings → 工具权限 tab: list defaults, per-tool override table, pattern rules editor.
- Audit log view: filterable table per session.
- Live approval modal: rewrite `PendingApprovals` modal with "Allow once / Allow always / Allow for this session / Deny" buttons; save the choice to permissions.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` (V13)
- `src-tauri/src/safety/mod.rs` (new query methods)
- `src-tauri/src/safety/permissions.rs` (new)
- `src-tauri/src/tauri_commands.rs` (CRUD commands)
- `ui/src/components/settings/PermissionsSettings.tsx` (new)
- `ui/src/components/safety/PendingApprovalModal.tsx` (rewrite)

**Acceptance criteria:**
- Approving a `bash rm -rf /tmp/foo` once with "Allow always for matching pattern" persists; the same command is auto-approved on subsequent calls.
- Per-session override visible in current session's header.
- Audit log shows every decision with timestamp + decider (user vs auto).
- Tests (P3): permission resolution precedence (session > pattern > global).

**Estimated effort:** 3 days.

**Dependencies:** P3.

---

## P7. Message edit / regenerate (B2)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p7-edit-regenerate.md`

**Goal:** Edit any past user message → branches conversation. Regenerate from any assistant turn → throws away that turn and re-runs the loop. Currently sessions are append-only.

**Scope:**
- Schema (V14): add `agent_messages.parent_id` and `messages.parent_id` to track branching. Existing rows have NULL (linear).
- Frontend: edit button on user messages opens inline composer; regenerate button on assistant messages calls a new `regenerate_assistant_message` Tauri command.
- Backend: new commands `edit_user_message` and `regenerate_assistant_message`. Both create a new branch by inserting new rows with `parent_id` pointing to the message before the edit/regen point. The agent loop runs against the new branch.
- Session view: by default render the most recent linear path. Add a sibling switcher (◀ ▶) on branch points so users can switch between alternate replies.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` (V14)
- `src-tauri/src/tauri_commands.rs` (2 new commands)
- `src-tauri/src/agent/session.rs` (`load_branch` method)
- `ui/src/components/agent/AgentMessageItem.tsx` (edit/regen buttons + sibling switcher)
- `ui/src/components/chat/ChatMessageItem.tsx` (same)
- `ui/src/atoms/chat-atoms.ts` (current-branch tracking)

**Acceptance criteria:**
- Edit a 3-message-deep user turn → new branch with regen'd assistant reply; sibling switcher appears.
- Regenerate an assistant turn → new sibling under the same user turn.
- Reload session → defaults to latest branch.
- Old conversations (parent_id NULL) still render correctly.
- Tests (P3): branch resolution logic.

**Estimated effort:** 4–5 days. **Largest plan in roadmap.**

**Dependencies:** P2 (so SDK persistence handles branched messages too), P3.

---

## P8. MCP server discovery (B5)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p8-mcp-marketplace.md`

**Goal:** Replace manual MCP stdio config with a curated registry the user can browse and install with one click.

**Scope:**
- Define a JSON schema for an MCP registry entry: `{ id, name, description, command, args, env_required[], categories[], install_notes_url }`.
- Bundle a starter `mcp-registry.json` (e.g. `filesystem`, `github`, `postgres`, etc. from the public MCP catalog).
- Settings → MCP 市场 tab: card grid filtered by category, "Install" button that adds to the existing `mcp_servers` storage.
- Show "configured" badge for already-installed entries.
- Future: fetch registry from a remote URL (out of scope for this plan).

**Files to touch:**
- `src-tauri/resources/mcp-registry.json` (new bundled file)
- `src-tauri/src/mcp.rs` (load + expose registry)
- `src-tauri/src/tauri_commands.rs` (`get_mcp_registry` command)
- `ui/src/components/settings/McpMarketplace.tsx` (new)
- `ui/src/components/settings/McpServerSettings.tsx` (existing — add "Browse marketplace" entry point)

**Acceptance criteria:**
- Marketplace tab shows ≥10 entries.
- One-click install on `filesystem` MCP results in a working tool exposed to the agent on next session.
- Required env vars prompt the user when missing.
- Tests (P3): registry loading + filter behavior.

**Estimated effort:** 2–3 days.

**Dependencies:** P3.

---

## P9. Agent teams UX (B6)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p9-agent-teams.md`

**Goal:** Surface the existing `agent/teams/` + `team_channel_messages` machinery in the UI as a "team mode" with channel-oriented views.

**Scope:**
- New view: `AgentTeamView` — left panel lists channels, main area is the channel transcript (uses `team_channel_messages`).
- "New team" wizard: pick a name, add member agents (each agent = a saved system prompt + model selection), pick a coordinator.
- Per-channel: `from`/`to` role badges, ability to inject a user message addressed to a specific role.
- Live update: subscribe to a new `chat:team-channel-update` event.

**Files to touch:**
- `src-tauri/src/agent/teams/` (existing — add channel emit on insert)
- `src-tauri/src/tauri_commands.rs` (subscribe command + CRUD for teams)
- `ui/src/components/agent/teams/AgentTeamView.tsx` (new)
- `ui/src/components/agent/teams/TeamChannelPanel.tsx` (new)
- `ui/src/components/agent/teams/TeamCreator.tsx` (new)
- `ui/src/atoms/team-atoms.ts` (new)
- App shell: tab type entry for teams.

**Acceptance criteria:**
- Create a team of 3 agents → channel auto-created.
- Send a message addressed to one agent → that agent's reply visible inline; other agents idle.
- Reload → channel history persists.
- Tests (P3): channel state management.

**Estimated effort:** 4 days.

**Dependencies:** P2 (SDK persistence helps multi-agent traces), P3.

---

## P10. Memory graph visualization (D3)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p10-memory-graph-viz.md`

**Goal:** Replace the current spartan memory list with a real graph view: force-directed nodes/edges, filter by `kind`/tag, zoom, search, click a node → preview content.

**Scope:**
- Use `react-flow` (or `cytoscape`) for the canvas.
- Backend: `memory_graph_*` commands already return nodes/edges/routes. Just consume them.
- Filters: kind (boot/identity/value/...), search by title/content, expand/collapse subtrees.
- Side panel on selection: full content + version history + edit.

**Files to touch:**
- `ui/src/components/memory/MemoryGraphView.tsx` (rewrite)
- `ui/src/components/memory/MemoryNodeDetailPanel.tsx` (existing — use as side panel)
- Add `react-flow` to deps.

**Acceptance criteria:**
- 50-node memory graph renders smoothly (60fps zoom/pan).
- Search highlights matching nodes; non-matches dim.
- Click a node → side panel with full content; "Edit" mutates and re-renders.
- Tests (P3): filter logic.

**Estimated effort:** 3 days.

**Dependencies:** P3.

---

## P11. First-run wizard + example sessions (D1 + D2)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p11-onboarding.md`

**Goal:** New user opens uClaw → guided 3-step setup, then a populated home screen with example sessions to play with.

**Scope:**
- Detect "first run" via empty `users` table (or empty `conversations` + no provider configured).
- Wizard:
  - Step 1: pick a provider (Anthropic / OpenAI / DeepSeek / OpenRouter), paste API key, test connection.
  - Step 2: optional — bootstrap memU (run `setup-python-env.sh` from a button if not already done; show progress).
  - Step 3: import 3 starter sessions (chat with web tools, agent with file tools, memory-aware conversation). Read seed JSONs from `src-tauri/resources/seed-sessions/`.
- After wizard: home screen shows a "Start here" card linking to each starter.

**Files to touch:**
- `src-tauri/resources/seed-sessions/*.json` (new)
- `src-tauri/src/tauri_commands.rs` (`detect_first_run`, `import_seed_sessions` commands)
- `ui/src/components/onboarding/FirstRunWizard.tsx` (new)
- `ui/src/components/onboarding/StepProvider.tsx` (new)
- `ui/src/components/onboarding/StepMemu.tsx` (new)
- `ui/src/components/onboarding/StepStarter.tsx` (new)
- `ui/src/components/welcome/WelcomeEmptyState.tsx` (existing — add "starter" cards)

**Acceptance criteria:**
- Fresh `~/.uclaw/` → wizard appears on launch.
- Configure provider → tested connection + key persisted.
- Import starters → 3 sessions visible in sidebar with content.
- Re-launching after wizard skips it.
- Tests (P3): wizard step transitions.

**Estimated effort:** 2 days.

**Dependencies:** P5 (the cost dashboard shows usage trends — useful in onboarding to set expectations), P3.

---

## P12. Cross-PR sanity sweep (C2)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p12-chat-domain-review.md`

**Goal:** A focused, written code review of every chat-domain change from PRs #2–#18 (this session's work). Verify no remaining off-by-ones, type-coercion gaps, or competing render paths.

**Scope:** This is more a *review project* than a *coding project*. The plan format is:
- For each chat-related file (`AgentView.tsx`, `AgentMessages.tsx`, `ChatView.tsx`, `ChatMessages.tsx`, `ChatToolBlock.tsx`, `ChatToolActivityIndicator.tsx`, `ChatMessageItem.tsx`, `Conversation.tsx`, `tauri_commands.rs::send_*`/`get_*`, `agent/session.rs`, `agent/dispatcher.rs::execute_tool_calls`, `db/migrations.rs::V9`):
  - Read top-to-bottom looking for one of: dead branches, inconsistent type narrowing, race conditions on streaming → persist, type-strip patterns like the one PR #9 fixed.
  - File a finding if anything looks suspicious.
- Each finding gets a 1-line repro and either a fix PR or a "false alarm" note.
- Result: 1 review document at `docs/superpowers/reviews/2026-MM-DD-chat-domain.md` listing findings + outcomes.

**Acceptance criteria:**
- Every chat-domain file mentioned has been re-read.
- All findings either fixed or explicitly dismissed with rationale.

**Estimated effort:** 1 day.

**Dependencies:** Most useful after P2 (SDK persistence) and P7 (edit/regenerate) since those touch the chat domain heavily and would surface review work anyway.

---

## P13. Migration test fixtures (C3)

**Plan file (when expanded):** `docs/superpowers/plans/uclaw-p13-migration-tests.md`

**Goal:** Lock down the migration story with explicit "old DB → new schema" test fixtures so future migrations don't silently corrupt data.

**Scope:**
- `src-tauri/tests/db_migration.rs` (new): for each migration version V1 → current, fixture a snapshot of the DB at that version and assert `migrations::run` upgrades cleanly.
- Use `rusqlite::Connection::open_in_memory()` + a DSL to seed each version's tables with realistic data.
- Run the upgrade, then assert: no row count changes, all new columns NULL or correctly defaulted, foreign keys intact.

**Files to touch:**
- `src-tauri/tests/db_migration.rs` (new)
- `src-tauri/tests/fixtures/v8_baseline.sql` (new — captures the schema before P2's V10)
- … one fixture per migration we need to lock down

**Acceptance criteria:**
- `cargo test db_migration` runs all migration upgrades and passes.
- A deliberately broken migration (e.g. drop a column other code expects) fails the test clearly.

**Estimated effort:** 1.5 days.

**Dependencies:** Most valuable *after* P2 lands (V10) and P7 lands (V14) so we have a stable target schema to lock down.

---

## Open questions / risks

1. **Q:** P2's `agent_sdk_messages` shape — should we deduplicate against `agent_turns` and `agent_messages`, or accept all three as separate stores? **Direction:** keep all three for now; revisit only if storage size becomes a complaint.

2. **Q:** P7's edit/regenerate UX — single-thread switcher vs full tree view? **Direction:** start with sibling-switcher (◀ ▶) only; full tree view is a follow-up if users want it.

3. **R:** P11's `setup-python-env.sh` from the wizard requires shell exec at install-time. Some platforms (notarized macOS) restrict this. **Mitigation:** detect arch + offer manual instructions if exec is blocked.

4. **R:** Each plan touches the schema (V10–V14). Out-of-order merging would corrupt. **Mitigation:** P13 lands after P2 + P7 to gate all schema changes; reviewers verify each PR uses the next sequential version.

---

## Self-review checklist

- [x] **Spec coverage:** all 18 items A1–D3 mapped to a plan.
- [x] **Sequencing:** dependencies explicit, no circular deps.
- [x] **Independence:** each plan ships working software on its own.
- [x] **Effort estimates:** every plan has a 0.5–5 day window — none too small, none too large for a single review.
- [x] **No placeholders:** every plan has named files, named commands, named tables, named acceptance criteria.

---

## Decision

Pick a plan to expand into an executable `docs/superpowers/plans/<name>.md` (with TDD-style steps). I recommend starting with **P1 (Cleanup batch)** — half a day, zero risk, leaves the repo cleaner before any feature plan.

After P1 ships, the natural next is **P3 (Frontend test infra)** — also half a day to a day, unblocks every UI plan.

Beyond that, sequencing depends on your priorities:
- **Most user-visible value first:** P4 (search) → P5 (cost) → P11 (onboarding).
- **Most foundational first:** P2 (SDK persistence) → P7 (edit/regen) → P12 (sweep).
- **Mixed:** P1 → P3 → P2 → P4 → P5 → P6 → P7 → P11 → P8 → P9 → P10 → P12 → P13.

Tell me which plan to expand first and I'll write the actionable step-by-step.
