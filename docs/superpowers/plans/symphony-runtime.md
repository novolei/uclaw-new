# Symphony Runtime — Implementation Plan

One PR, one branch (`feat/symphony-runtime`), bisectable commits. Each task below is one commit; the table at the bottom of the PR body lists them in order. Plan target: design spec at `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md`.

## Tasks

### T1. Correct the migration registry in CLAUDE.md

- **File:** `CLAUDE.md`
- **Change:** Replace the "Active migration registry" table with the merged-up-to-V32 reality and append V33 (this PR). The current table stops at V26 and is seven migrations stale.
- **Verify:** `git diff CLAUDE.md` shows only the migration table changed; the surrounding "Adjacent edits" prose is untouched.
- **Why first:** Stops the next person who reads this PR from arguing about V-number conflicts.

### T2. Add `SymphonyConfig` to `MemubotConfig`

- **Files:**
  - `src-tauri/src/memubot_config.rs` — append `SymphonyConfig` struct + `pub symphony: SymphonyConfig` field on `MemubotConfig`; provide `Default`.
  - `src-tauri/src/memubot_config.rs::Default for MemubotConfig` — add `symphony: SymphonyConfig::default()`.
- **Defaults:** as listed in spec §7.
- **Verify:**
  - `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — empty.
  - `cd src-tauri && cargo test --lib memubot_config` — passes (existing tests; add one round-trip JSON test for `SymphonyConfig`).

### T3. V33 migration: Symphony schema

- **File:** `src-tauri/src/db/migrations.rs` — append `const SQL_V33_SYMPHONY: &str = "..."` (verbatim from spec §8.2) and a tolerant-per-statement runner in `pub fn run(...)` after the V32b block.
- **Tests in the same commit:** mirror `v25_*`, `v26_*` tests at the bottom of the file:
  - `v33_creates_symphony_tables` — assert each table + each index + the seeded `symphonies` row exist after `super::run(&conn)`.
  - `v33_is_idempotent` — two consecutive `run(&conn)` calls succeed.
  - `v33_symphony_runs_fk_to_workflows_cascades` — delete a workflow, runs are gone.
- **Verify:** `cd src-tauri && cargo test --lib migrations::tests::v33` passes.

### T4. `symphony/protocol/` types + parser

- **Files (new):**
  - `src-tauri/src/symphony/mod.rs`
  - `src-tauri/src/symphony/protocol/mod.rs`
  - `src-tauri/src/symphony/protocol/types.rs` — `NodeStatus`, `RunStatus`, `NodeKind`, `RetryPolicy`, `FailureMode`, `SymphonyWorkflowDef`, `SymphonyNode`, `SymphonyEdge`, `NodeOutcome`, `RunOutcome`, `NodeOutputMap`.
  - `src-tauri/src/symphony/protocol/parse.rs` — `parse_workflow_md(s: &str) -> Result<SymphonyWorkflowDef>` (YAML front matter via `serde_yml` — same crate the Humane parser at `automation/protocol/parse.rs:50` already uses; body kept as prompt template).
  - `src-tauri/src/symphony/protocol/normalize.rs` — `def_to_rows(def) -> (workflow_row, version_row)` and `version_row_to_def(row) -> SymphonyWorkflowDef`. Cycle check in `validate_dag(def) -> Result<()>` using Kahn's algorithm.
- **Module wiring:** add `pub mod symphony;` to `src-tauri/src/lib.rs`.
- **Tests:** parse a 3-node WORKFLOW.md fixture; reject a cyclic edge set; round-trip `def_to_rows` ↔ `version_row_to_def`.
- **Verify:** `cd src-tauri && cargo test --lib symphony::protocol` passes.

### T5. `symphony/manager.rs` — workflow CRUD

- **File (new):** `src-tauri/src/symphony/manager.rs` — `SymphonyManager` over `Arc<StdMutex<Connection>>`. Methods: `list_workflows`, `get_workflow_with_current_version`, `save_workflow(def) -> (workflow_id, version)` (validates + persists new version + bumps `current_version`), `delete_workflow`, `import_md`, `export_md`.
- **Tests:** create-fetch-update-delete; saving twice writes two `symphony_workflow_versions` rows; `import_md` then `export_md` round-trips a canonical workflow.
- **Verify:** `cd src-tauri && cargo test --lib symphony::manager` passes.

### T6. `symphony/runtime/cost.rs` — caps + day-total helper

- **File (new):** `src-tauri/src/symphony/runtime/cost.rs` — reuse `automation::runtime::cost::CostCapState` directly; add `pub fn symphony_day_total_usd(conn: &Connection, since_ms: i64) -> f64` filtering `cost_records` by `metadata_json LIKE '%"origin":"symphony:%'`.
- **Tests:** insert 3 cost rows tagged symphony, 1 tagged automation; assert helper returns only the symphony total.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::cost` passes.

### T7. `symphony/runtime/retry.rs` — backoff helper

- **File (new):** `src-tauri/src/symphony/runtime/retry.rs` — `pub fn backoff_ms(attempt: u32, max_ms: u64) -> u64` implementing `min(10_000 * 2^(attempt-1), max_ms)` (verbatim from Symphony SPEC).
- **Tests:** attempt=1 → 10_000; attempt=2 → 20_000; attempt=10 capped at `max_ms`.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::retry` passes.

### T8. `symphony/runtime/node_run.rs` — per-node HeadlessDelegate bridge

- **File (new):** `src-tauri/src/symphony/runtime/node_run.rs` — `execute_node(...)` per spec §5.3. New types: `NodeExecutionDeps`, `SymphonyHeartbeatSink` (impls `channels::types::StreamingHandle`, calls `Heartbeat::touch(node_id)` + emits throttled `symphony:node_log`).
- **Reuse:** `HeadlessDelegate` is imported, not re-declared; `persist_transcript` from `automation::runtime::run_session`; `create_run_session` adapted with `origin = "symphony:<node_id>"`.
- **New helper:** `symphony/runtime/run_session.rs` — `SYMPHONIES_SPACE_ID`, `ensure_symphonies_space`, `create_node_session` (a 30-line analog of the automation version, with `metadata.workflow_id`, `metadata.run_id`, `metadata.node_id`).
- **Tests:** with a no-op `LlmProvider` (copy `automation/runtime/execute.rs` test harness), a node executes, persists a transcript, emits one `symphony:node_log`, and writes `cost_records` tagged with origin `symphony:<node>`.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::node_run` passes.

### T9. `symphony/runtime/run_actor.rs` — DAG scheduler

- **File (new):** `src-tauri/src/symphony/runtime/run_actor.rs` — `RunActor` per spec §5.2. Owns: `state: RwLock<RunState>`, `per_workflow_sem`, `cancel: CancellationToken`, node `JoinHandle` map.
- **Algorithm:** topological dispatch, `tokio::select!` over (ready node, completed node, cancel, stall tick). On node failure → retry per `node.retry_policy`; on retries exhausted → apply `workflow.failure_mode` (`Abort` | `ContinueOthers` | `BranchOnly`).
- **Persistence:** every state transition writes through to `symphony_runs` / `symphony_node_runs`. Recovery uses these rows.
- **Tests:** 3-node linear chain succeeds; 3-node diamond with one transient failure retries and succeeds; circular dep rejected at save (covered in T4); explicit cancel mid-run produces `Cancelled` outcome and aborts the inner agent loops.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::run_actor` passes.

### T10. `symphony/runtime/stall.rs` — stall detection

- **File (new):** `src-tauri/src/symphony/runtime/stall.rs` — `Heartbeat` (a `RwLock<HashMap<NodeId, i64>>`). Called from `SymphonyHeartbeatSink`. `check_stalls(now_ms, threshold)` returns node-ids whose `last_heartbeat_ms < now_ms - threshold`.
- **Tests:** heartbeat updates, threshold expiry detected, cleared heartbeats don't trip.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::stall` passes.

### T11. `symphony/runtime/recovery.rs` — restart reconciliation

- **File (new):** `src-tauri/src/symphony/runtime/recovery.rs` — `reconcile(conn, config) -> Vec<RunResumeBlueprint>` per spec §5.5. Marks orphaned `running` / `ready` rows as `stalled`; returns blueprints for active runs to resume.
- **Tests:** inject a `running` run + a `running` node with `last_heartbeat_ms = now - 10min`; assert reconcile transitions the node to `stalled` and returns a blueprint.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::recovery` passes.

### T12. `symphony/runtime/service.rs` — `SymphonyService: ManagedService`

- **File (new):** `src-tauri/src/symphony/runtime/service.rs` — per spec §5.1. Wires trigger channel, global semaphore, tick loop, recovery on start.
- **InfraService:** add two `InfraEventType` variants (`SymphonyRunCompleted`, `SymphonyNodeCompleted`) in `src-tauri/src/infra/types.rs`; emit them at run/node completion.
- **Tests:** `start()` runs reconcile + spins up the tick loop; `stop()` cancels in-flight runs and reports `Stopped` within 5s.
- **Verify:** `cd src-tauri && cargo test --lib symphony::runtime::service` passes.

### T13. Register `SymphonyService` in `main.rs` Stage 3

- **File:** `src-tauri/src/main.rs` — between the `AppRuntimeService` registration (line 271) and the `ImChannelManager` start (line 281), add the gated registration block from spec §3.2.
- **Verify:** `cd src-tauri && cargo build` passes; manual smoke (`cargo tauri dev`) shows the `[Stage 3] SymphonyService registered` line in logs.

### T14. Tauri commands + `invoke_handler!` entries

- **Files:**
  - `src-tauri/src/tauri_commands.rs` — append the eight `pub async fn symphony_*` commands listed in spec §4.3.
  - `src-tauri/src/main.rs` — add each command name to `invoke_handler!` at line 409. **CLAUDE.md Part 1 Adjacent edits — forgetting this compiles fine but fails at runtime.**
- **Tests:** unit tests at the bottom of each command function exercising the happy path with an in-memory DB.
- **Verify:**
  - `cd src-tauri && cargo build` passes.
  - `cd src-tauri && cargo test --lib tauri_commands::symphony` passes.

### T15. Frontend types + atoms

- **Files:**
  - `ui/src/lib/tauri-bridge.ts` — add typed `invoke<T>` wrappers for each `symphony_*` command + payload types (`SymphonyWorkflowRow`, `SymphonyRunRow`, `SymphonyNodeRunRow`, `SymphonyEdgeRow`, `NodeUpdate`, `NodeLog`, etc.).
  - `ui/src/atoms/symphony.ts` (new) — atoms per spec §6.4.
  - `ui/src/atoms/symphony-canvas.ts` (new) — viewport, selection, palette state; `atomWithStorage` keyed per workflow.
  - `ui/src/atoms/app-mode.ts` — widen `AppMode` to `'chat' | 'agent' | 'symphony'`.
  - `ui/src/atoms/tab-atoms.ts` — widen `TabType`.
- **Verify:** `cd ui && npx tsc --noEmit 2>&1 | head -10` empty.

### T16. ModeSwitcher + TabContent wiring

- **Files:**
  - `ui/src/components/app-shell/ModeSwitcher.tsx` — three-entry mode array; slider width `w-[calc(33.333%-4px)]`; `restoreSession` generalized to a `switch` over the three modes.
  - `ui/src/components/tabs/TabContent.tsx` — add the `tab.type === 'symphony'` branch from spec §6.1.
- **Test:** existing `ModeSwitcher.test.tsx` updated; `TabContent` doesn't have a test today but add a minimal one covering the symphony branch.
- **Verify:** `cd ui && npm test -- --run ModeSwitcher TabContent 2>&1 | tail -10` passes.

### T17. `@xyflow/react` dependency + Vite chunk

- **Files:**
  - `ui/package.json` — add `@xyflow/react@^12`.
  - `ui/vite.config.ts` — add manualChunks entry per spec §6.5.
- **Verify:**
  - `cd ui && npm install` succeeds.
  - `cd ui && npm run build 2>&1 | tail -20` shows an `xyflow-*.js` chunk emitted.

### T18. `SymphonyCanvas` (Design + Run views)

- **Files (new under `ui/src/components/symphony/`):**
  - `index.ts`
  - `SymphonyCanvas.tsx` — top-level view; subscribes to IPC events per spec §6.3.
  - `canvas/WorkflowCanvas.tsx` — `ReactFlow` root, custom node + edge types.
  - `canvas/NodeCard.tsx` — status pill, cost chip, iteration progress; uses theme tokens per spec §6.2.
  - `canvas/EdgeWire.tsx` — animated when source node is `Running`.
  - `canvas/CanvasToolbar.tsx` — run / cancel / fit-view / export.
  - `canvas/PaletteSidebar.tsx`, `canvas/InspectorPanel.tsx`.
- **Tests:** `SymphonyCanvas.test.tsx` — atoms wire, status changes re-render, click-to-trigger-run calls `symphony_trigger_run` with the right payload. Mock `@xyflow/react` exports (jsdom struggles with measurements, same as Recharts).
- **Verify:** `cd ui && npm test -- --run symphony 2>&1 | tail -10` passes.

### T19. `WorkflowMarkdownEditor` (Raw view)

- **File (new):** `ui/src/components/symphony/WorkflowMarkdownEditor.tsx` — CodeMirror over `definition_md`; YAML + Markdown syntax; debounced save to `symphony_save_workflow`.
- **Tests:** typing fires the debounce; invalid YAML surfaces an error banner.
- **Verify:** `cd ui && npm test -- --run WorkflowMarkdownEditor` passes.

### T20. `RunHistoryPanel`

- **File (new):** `ui/src/components/symphony/RunHistoryPanel.tsx` — per-run rollup: status, total cost, duration, leaf nodes. Click a row → `symphony_get_run` populates the Run view of the canvas.
- **Verify:** `cd ui && npm test -- --run RunHistoryPanel` passes.

### T21. Smoke integration test

- **File:** `src-tauri/src/symphony/integration_test.rs` (under `#[cfg(test)]` in `mod.rs`) — three-node linear workflow; trigger; await; assert two `agent_sessions` rows + transcripts persisted + `symphony_runs.outcome = 'completed'` + `cost_records` rows tagged symphony.
- **Verify:** `cd src-tauri && cargo test --lib symphony::integration_test` passes.

### T22. Manual verification + screenshots

- Spin up `cargo tauri dev`; create a 2-node workflow in the canvas; run; verify Agent view can be opened on either node's transcript via `symphony_get_node_session_id`.
- Capture screenshots for the PR body (Design + Run + Raw + RunHistory).

## Commit table (paste into PR body)

| # | Title                                                          | Files touched (approx)                 | Verified by                                                |
| - | -------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------- |
| 1 | docs: correct migration registry to current state              | CLAUDE.md                              | `git diff` review                                          |
| 2 | symphony: add `SymphonyConfig` to `MemubotConfig`              | memubot_config.rs                      | `cargo test --lib memubot_config`                          |
| 3 | db: V33 — symphony schema                                      | db/migrations.rs                       | `cargo test --lib migrations::tests::v33`                  |
| 4 | symphony: protocol types + WORKFLOW.md parser                  | symphony/protocol/**                   | `cargo test --lib symphony::protocol`                      |
| 5 | symphony: workflow manager (CRUD)                              | symphony/manager.rs                    | `cargo test --lib symphony::manager`                       |
| 6 | symphony: cost helpers (caps + day total)                      | symphony/runtime/cost.rs               | `cargo test --lib symphony::runtime::cost`                 |
| 7 | symphony: retry backoff per Symphony SPEC                      | symphony/runtime/retry.rs              | `cargo test --lib symphony::runtime::retry`                |
| 8 | symphony: per-node executor (HeadlessDelegate bridge)          | symphony/runtime/node_run.rs, run_session.rs | `cargo test --lib symphony::runtime::node_run`        |
| 9 | symphony: DAG scheduler (`RunActor`)                            | symphony/runtime/run_actor.rs          | `cargo test --lib symphony::runtime::run_actor`            |
| 10 | symphony: stall detection                                     | symphony/runtime/stall.rs              | `cargo test --lib symphony::runtime::stall`                |
| 11 | symphony: restart reconciliation                              | symphony/runtime/recovery.rs           | `cargo test --lib symphony::runtime::recovery`             |
| 12 | symphony: `SymphonyService` (ManagedService)                  | symphony/runtime/service.rs, infra/types.rs | `cargo test --lib symphony::runtime::service`         |
| 13 | symphony: register service in main.rs Stage 3                 | main.rs                                | `cargo tauri dev` log line                                 |
| 14 | symphony: Tauri commands + invoke_handler entries             | tauri_commands.rs, main.rs             | `cargo test --lib tauri_commands::symphony`                |
| 15 | ui: symphony types, atoms, AppMode/TabType widened            | ui atoms + tauri-bridge.ts             | `npx tsc --noEmit`                                          |
| 16 | ui: ModeSwitcher + TabContent symphony branch                 | app-shell/**, tabs/**                  | `npm test -- --run ModeSwitcher TabContent`                |
| 17 | ui: add @xyflow/react + Vite chunk                            | ui/package.json, vite.config.ts        | `npm run build` chunk inspection                            |
| 18 | ui: SymphonyCanvas (Design + Run views)                       | ui/components/symphony/canvas/**       | `npm test -- --run symphony`                                |
| 19 | ui: WorkflowMarkdownEditor (Raw view)                         | ui/components/symphony/WorkflowMarkdownEditor.tsx | `npm test -- --run WorkflowMarkdownEditor`     |
| 20 | ui: RunHistoryPanel                                            | ui/components/symphony/RunHistoryPanel.tsx | `npm test -- --run RunHistoryPanel`                   |
| 21 | symphony: smoke integration test                              | symphony/integration_test.rs           | `cargo test --lib symphony::integration_test`              |
| 22 | docs: screenshots, PR walkthrough                              | docs/superpowers/specs/...             | reviewer eyeballs                                          |

## Out-of-scope follow-ups (separate PRs)

- `LinearSource` / `GitHubIssueSource` adapters.
- `record_handoff` Symphony tool (schema-only registry entry, intercepted by `RunActor`).
- Hot-reload of running workflows (Symphony spec MUST item we ship cold-reload of first).
- Proactive scenario: post-run review ingesting workflow outcomes into MemoryGraph.
- FTS over node prompts (V34).
- Workflow templates marketplace tile in `automation/marketplace`.

## Bug-spinoff slot

Per CLAUDE.md Part 1, if any out-of-scope bug surfaces during this work with a confident root cause, spin a small separate PR. **Do not fold it in here.**
