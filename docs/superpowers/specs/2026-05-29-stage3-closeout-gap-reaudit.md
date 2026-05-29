# 阶段 3 Close-out — Gap Re-audit vs. `2026-05-27-pi-convergence-gap-audit.md`

> **Status:** Deep re-audit of §1 (`分模块缺陷清单`) against current main `c7386a7f` (2026-05-29, post-PR #579). Conducted by 2 parallel opus agents with full file-level verification + Pi source cross-reference. Reports what 阶段-3 actually closed, what landed before 阶段-3 (and didn't get credit), what regressed, and the real 阶段-4+ backlog.

## TL;DR

阶段-3 closed **less than the closeout commit messages claim**, but two pre-阶段-3 cleanup PRs and one Slice (1b safety chokepoint) closed **more than expected** at the architectural level. Concrete results:

| Status | Count | Notable items |
|---|---:|---|
| ✅ **Fixed** | 5 of 17 | §1.3 (both); §1.6 harness rename; §1.4 RegistryHub (deleted); P3-6 dynamic-context seam (partial) |
| 🟡 **Partially fixed** | 8 of 17 | §1.1 god object + cancellation; §1.2 4 layers + snapshot tests + 4 compose variants; §1.4 plugin installer + subsystems; §1.6 supervision vocab (4→2); §1.7 browser nested loop (Evaluate gated); §1.8 the headline CRITICAL |
| ❌ **Unaddressed** | 3 of 17 | §1.1 compaction-reload; §1.1 TurnSnapshot hollow; §1.2 A4 injection; §1.5 most items; §1.9 coding |
| ⚠️ **Regressed** | 1 of 17 | §1.5 #4 — `importance_decay` now ACTIVELY scheduled against deprecated memory_nodes |

**3 critical surprises**:
1. **§1.8 (Safety, 全栈最高风险) saw a major pre-阶段-3 fix**: `build_automation_chokepoint` is wired through 3 production sites (general automation, Symphony, IM/channels) via shared `ToolDispatcher` + `SafetyManager` + `PendingApprovals`. NOT a closeout 阶段-3 PR, but real progress. Caveat: `PermissionSet::Allowed → SafetyMode::Yolo` is a declarative bypass by design — bash registered in automation can still auto-approve if the spec declares Shell.
2. **§1.5 #4 regressed**: `importance_decay` is no longer dormant — `proactive/service.rs:1364` calls `batch_recompute_importance` on a schedule, actively buffing the deprecated `memory_nodes` path the audit said was dead.
3. **§1.1 Cancellation is the highest-leverage bug-shaped finding**: infrastructure landed (`llm_stream` + `tool_dispatch` both have biased `select!` on `CancellationToken`), but the 2 primary user entry points (`tauri_commands.rs:1961 send_message`, `:11334 agent_send_message`) **never install a token**. UI "stop" can't interrupt LLM stream or bash in production. ~50-line fix; highest user-visible value.

---

## Method

- Two opus agents in parallel against main at `c7386a7f`. Agent A: §1.1-§1.4 (agent kernel quadrant). Agent B: §1.5-§1.9 (data/safety/coding quadrant).
- Each agent independently grep+read+verified every claim in the audit against current files. Where audit line numbers drifted (typically 15-30 lines due to refactors), agents cited current location.
- Pi source at `/Users/ryanliu/Documents/pi` cross-referenced for §1.1 cancellation (Pi threads `signal: AbortSignal` structurally through every layer — uClaw's `ReasoningContext.cancellation_token` is field-shaped, hence the "never installed at entry" gap).
- Status codes: ✅ Fixed / 🟡 Partial / ❌ Unaddressed / ⚠️ Regressed.

---

## Per-item findings

### §1.1 Agent Loop

#### 🔴 CRITICAL — CancellationToken not threaded to await points → 🟡 **Partially fixed**

**What landed:**
- `llm_stream::stream_completion` accepts `cancel: Option<&CancellationToken>`. Every `stream.next()` wrapped in `tokio::select! { biased; _ = tok.cancelled() => break; r = timeout(...) => r }` ([`llm_stream.rs:107-119`](../../src-tauri/src/agent/llm_stream.rs)). Locked by `cancellation_token_aborts_in_flight_stream` test.
- `ToolDispatcher::dispatch` wraps `dispatch_inner` in biased `select!` against `ctx.cancel`, short-circuits batch to `cancelled_outcome` ([`tool_dispatch/mod.rs:178-208`](../../src-tauri/src/agent/tool_dispatch/mod.rs)). Test `dispatch_short_circuits_when_cancelled` locks this.
- Mid-flight cancel-during-`call_llm` locked by `agentic_loop.rs::cancellation_contract_tests::fired_token_yields_cancelled_outcome` with `AtomicBool` regression-proof.

**What's broken:**
- **The 2 primary production loop entry points install NO cancellation token.** Grep `with_cancellation\|cancellation_token =` in `tauri_commands.rs` → zero matches in:
  - Chat-mode `send_message` ([`tauri_commands.rs:1961`](../../src-tauri/src/tauri_commands.rs))
  - Agent-mode `agent_send_message` ([`tauri_commands.rs:11334`](../../src-tauri/src/tauri_commands.rs))
- Only `RegularTask::run_with_cancellation` ([`agent/regular_task.rs:128`](../../src-tauri/src/agent/regular_task.rs)) and tests install the token. That path is used by rollout/automation runtime, **not** chat or agent UI sends.
- `Tool::execute` trait signature ([`tools/tool.rs:224`](../../src-tauri/src/agent/tools/tool.rs)) has no `CancellationToken` parameter. Individual tools can't release resources gracefully; only `BashTool` survives via `kill_on_drop(true)`.

**User-visible**: pressing "stop" in chat UI today cannot interrupt a long LLM stream or bash. The stop signal flows through the older `Arc<AtomicBool>` polling path (observed between iterations only).

**Pi reference**: `packages/agent/src/agent-loop.ts` threads `signal: AbortSignal | undefined` through EVERY layer's function signature — `runLoop`, `streamAssistantResponse`, `executeToolCalls`, `prepareToolCall`/`executePreparedToolCall`, even `transformContext(messages, signal)`. Signal is plumbed structurally, not side-channeled through a context object.

**Fix scope**: ~50 lines. Install per-conversation `CancellationToken` in chat/agent send paths + wire UI "stop" button to fire it. Optionally add `cancel: &CancellationToken` to `Tool::execute` for graceful tool cleanup.

#### 🟠 MAJOR — Iterative compaction reload degrades to O(N) → ❌ **Unaddressed**

- `CompactionState.previous_fold: Option<StructuredFold>` ([`agent/compaction.rs:14-19`](../../src-tauri/src/agent/compaction.rs)) is **in-memory only**.
- `ReasoningContext::new(...)` always starts with `compaction_state: CompactionState::default()` (`previous_fold: None`); 12 construction sites verified.
- `tauri_commands.rs:1962-1975` session restore only restores `messages` + token counters; no fold persistence.
- First compaction after every session resume takes the full-history path. The iterative-fold win is invalidated on long sessions.

**Fix scope**: schema fix (column on `agent_sessions` / `conversations`) + restore in `tauri_commands.rs:1961`. Medium PR.

#### 🟠 MAJOR — TurnSnapshot isolation hollow → ❌ **Unaddressed**

- `TurnSnapshot` correctly freezes `(turn_index, model, system_prompt, dynamic_context, tools, force_text)` ([`agent/turn.rs:12-24`](../../src-tauri/src/agent/turn.rs)).
- **But every cost/log/context-stats reader still uses `self.model`**:
  - `dispatcher/observability.rs:205, 229, 264` — cost calc + cost_store::record + get_model_context_length
  - `dispatcher/turn_runner.rs:650, 832, 1306, 1326, 1413` — every `CompletionConfig { model: self.model.clone() }` and trajectory record path
  - `dispatcher/mod.rs:328` — image-policy capability check at boot
- Today dormant because `NextTurnPatch.model` has no producer. Tomorrow when hot-swap lands, snapshot says model X, cost calc says model Y — silent correctness bug.

**Fix scope**: thread `snapshot: &TurnSnapshot` into `on_usage`, `emit_turn_cost`, `emit_context_stats`, trajectory record. Coordinated 10-file edit.

#### 🟠 MAJOR — dispatcher.rs god object → 🟡 **Partially fixed**

- ✅ The 3,853-LoC single file is gone (P3-5a → 6-file module).
- ✅ ChatDelegate **53 → 34 fields** (P3-5b1 + P3-5b2).
- ✅ 6 inline ContentBlock duplications consolidated into `ChatMessage::assistant_from_response` (P3-5b3).
- 🟡 `turn_runner.rs` is still 1,942 LoC. `content_assembler.rs` is 1,573 LoC. Further splits possible.
- 🟡 34 fields is still well above "small object". Visible bundling opportunities remain (`gep` + `recent_tool_errors` + `last_error_kind`; `is_first_act_turn` + `skill_search_used` + `last_tool_defs_hash`).

---

### §1.2 Prompt construction

#### 🟠 MAJOR — 4 implicit layers → 🟡 **Partially fixed (and STRUCTURALLY STILL TRUE)**

**What P3-6 unified**: `assemble_system_prompt(SystemPromptContext) -> AssembledPrompt` ([`content_assembler.rs:58-86`](../../src-tauri/src/agent/dispatcher/content_assembler.rs)) is a pure single-seam function returning BOTH halves. `create_turn_snapshot` calls `assemble_prompt(&effective_mode)` ONCE per turn; both halves stored on `TurnSnapshot`.

**What was NOT unified** — `create_turn_snapshot` still **post-appends 4 layers** after the unified function returns ([`turn_runner.rs:486-569`](../../src-tauri/src/agent/dispatcher/turn_runner.rs)):
- Layer A (GEP genes) — `:486-519` `full_system_prompt.push_str(&gene_block)`
- Layer B (plan-suggest aggregate hint) — `:521-555` 
- Layer C (project rules) — `:557-566` `RuleContextBuilder::build_context`
- Layer D (ladder pad) — `:568-569` `pad_to_ladder` for cache alignment

So the LLM sees: `assemble_prompt.system + gene_block + plan_suggest_hint + project_rules + pad_to_ladder` — **5 implicit layers, just with different layer names**.

The audit's structural complaint stands; the obvious-to-the-eye half was unified.

#### 🟠 MAJOR — compose_system_prompt 4 combinatorial variants → 🟡 **Partially fixed (3 of 4 dead)**

- All 4 `pub fn` still in `mode_prompts.rs` (`compose_system_prompt`, `_with_persona`, `_with_injection`, `_with_injection_and_persona`).
- Grep production callers: **only `:191` `_with_injection_and_persona` has a live caller** (`content_assembler.rs:60`). The other 3 are tests-only.
- Internally all 4 delegate to one function (`compose_with_baseline_and_persona`), so this is shadowed-by-4-entry-points, not 4-implementations.

**Fix scope**: 30-line cleanup PR — delete the 3 dead pubs + their tests.

#### 🟡 MINOR — A4 injection broken → ❌ **Unaddressed**

- `estimate_context_pressure_ratio` still hardcoded `0.0` ([`content_assembler.rs:357-361`](../../src-tauri/src/agent/dispatcher/content_assembler.rs)). Comment admits "Stubbed; follow-up wires the M2-J TokenBudgetSnapshot ratio here" — no follow-up landed.
- All 10 production baseline blocks override with `InjectionPolicy::Always` (no `OnContextPressure` block exists in production).
- `is_first_act_turn` flips to `false` on first read and never resets on Plan→Auto toggle. Comment at `dispatcher/mod.rs:198-202`: "TODO(M2-A) proper mode-transition tracking ... lands with M2-A finalization".

The injection-aware machinery is wired structurally but **operationally inert**. Locked by `compose_with_injection_is_byte_stable_across_inj_variants` test which guarantees this state.

#### 🟠 MAJOR — No snapshot tests on real prompt → 🟡 **Partially fixed**

- ✅ 5 golden snapshot tests added in P3-6 (`assemble_snapshot_tests` module).
- 🟡 Tests call `assemble_system_prompt(ctx)` directly with synthetic `SystemPromptContext`. They do NOT cover the 4 post-appends in `create_turn_snapshot` (GEP genes, plan-suggest, project rules, ladder pad). A regression in those 4 layers will NOT break any snapshot.
- 🟡 No "given ChatDelegate state X, the LLM sees prompt Y" test — the snapshots stop at the single-seam exit, not at the `TurnSnapshot.system_prompt` final.

**Fix scope**: either move post-appends INTO `assemble_system_prompt` (add fields to `SystemPromptContext`) or add second tier of snapshots at `create_turn_snapshot` exit.

---

### §1.3 Skill system

#### 🔴 CRITICAL — select_top_k dead code → ✅ **Fixed**
- Grep across `src-tauri/`: **zero matches**.
- Commit `8debd782` (PR #566 "kill dead skill_selection module + dead skill renderers"), pre-阶段-3.

#### 🟠 MAJOR — 3 competing skill renderers → ✅ **Fixed**
- Grep `build_skill_prompt`, `combined_system_prompt`: zero matches (both deleted in #566).
- `format_for_system_prompt_xml` is now the sole renderer.
- Note: `build_skills_manifest` lingers as test-only — small follow-up cleanup candidate.

---

### §1.4 Plugin uniformity

#### 🔴 CRITICAL — plugin_manifest no installer → 🟡 **Partially fixed**

**Discovery side** (✅ landed via P3-4):
- `plugins/{discovery, registration, uclaw_extension}.rs` scans `$DATA_DIR/plugins/<id>/plugin.toml` and routes `manifest.contributes.tools` → `AgentApi.register_tool` via `McpToolProxy::for_plugin`.
- Boot wiring in `app.rs:901-942` runs the scan after builtin descriptor registration.

**Install side** (❌ never landed):
- [`plugin_manifest/mod.rs:1-19`](../../src-tauri/src/plugin_manifest/mod.rs) explicitly admits: "The TOML loader (`load_plugin_manifest`) and the `.plugin` zip installer were removed in P2 cleanup — installer commit 2 never landed".
- No `install_plugin(zip_path)`, no UI affordance, no signature verification. Plugins must be manually `cp -r`'d into `$DATA_DIR/plugins/` before next boot.
- Only `tools` field of `PluginContribution` is fully functional. `commands` / `mcp_servers` / `skills` / `themes` are **recorded but not actually registered** (`registration.rs:76-88`).

The audit's complaint was about no installer; that gap is **structurally unchanged**.

#### 🔴 CRITICAL — 4 plugin subsystems, no shared seam → 🟡 **Partially fixed**

**Q1: Does the agent loop now query plugins via `agent_api.tool(name)`?**
- **NO.** Grep `agent_api.tool(` in production: **zero hits** (3 in test code). Runtime tool lookup is `ToolDispatcher.tools.get(&tc.name)` — the per-session frozen `ToolRegistry`. AgentApi participates only at boot/session-build time, never at dispatch time.

**Q2: Is `registry_hub::resolve` still called from zero sites in agent/?**
- **The module is deleted entirely** (PR #569, commit `210a8be7` "kill M3-T1 RegistryHub"). Grep: zero matches anywhere. Closed by deletion, not integration.

**Q3: Skills + Automation specs + MCP unified through AgentApi?**
- **Effectively still siloed.** Per [`tools/registry_build.rs:29-194`](../../src-tauri/src/agent/tools/registry_build.rs):

| Source | Path | Through AgentApi? |
|---|---|---|
| 17 builtin tools | `state.agent_api.build_session_registry(&ctx)` | ✅ Yes |
| memu tools | `memu_tools::register_memu_tools(&mut tools, ...)` | ❌ Inline |
| ~30 browser tools | `tools.register(BrowserXxxTool { ... })` | ❌ Inline |
| MCP tool proxies | `McpManager::create_tool_proxies(...) + tools.register(p)` | ❌ Inline |
| Plugin tools (P3-4) | `PluginRegistrar::register(&mut api, &loaded)` | ✅ Yes |
| Skills | `format_for_system_prompt_xml() → set_skills_manifest_block` | ❌ Prompt fragment, not tool |
| Automation specs | `automation_specs` SQLite + capability_map | ❌ Separate runtime |

**Evidence of zero AgentApi runtime callers**: `agent_api.tool/command/renderer(` — 0 production hits. Only 4 production `agent_api` accesses: 1× `build_session_registry`, 3× `hook_bus()`.

So at the unified `ToolRegistry` the dispatcher sees, all tools are present — but they arrived via 4-5 silo'd registration paths. AgentApi is **structurally present, runtime-bypassed**.

**Fix scope**: migrate browser/memu/MCP-proxy registration through `AgentApi.register_tool` (requires extending `SessionContext` to expose mcp_manager / browser_context_manager / memu_client so descriptor builders can construct concrete tools). Then route dispatch through `agent_api.tool(name)`.

---

### §1.5 Memory

#### 🔴 CRITICAL — 8 parallel memory stores + inline assembly → ❌ **Unaddressed** (downstream seam closed via P3-6; upstream multi-store fan-in unchanged)

- AppState still holds 8 distinct memory subsystems ([`app.rs:185-258`](../../src-tauri/src/app.rs)): `memory_store`, `memory_graph_store`, `memu_client`, `wiki_synthesizer`, `lint_analyzer`, `entity_synthesizer`, `brain_watcher`, `learning_llm`. Plus dead `memory_contract/`, `memory_policy/`, `world/`.
- `tauri_commands.rs:2151-2215` Memory Recall Integration block still inline-assembles 4-5 stores into a single `memory_ctx` string passed to `delegate.set_memory_context(memory_ctx)`. Audit's structural diagnosis intact.
- P3-6 single seam consumes the pre-blended `Option<String>` — the seam unification is downstream of the fan-in problem.

This is exactly the 阶段-4 scope per the recon doc.

#### 🔴 CRITICAL — gbrain master / memory_graph freeze is fake → ❌ **Unaddressed** (warn-only runtime guard added; static hook still naïve)

- Pre-commit hook regex still ONLY catches literal `memory_graph::write*/insert*/update*/delete*` ([`check-memory-graph-freeze.sh:29`](../../scripts/git-hooks/checks/check-memory-graph-freeze.sh)). Real write APIs (`create_node`, `create_entity_page`, `create_link`, `upsert_*`, `record_*`) PASS THE HOOK FREELY.
- `memory_graph/mod.rs:76 enforce_freeze` is **warn-only by default** — `tracing::warn!` once per callsite, deduped. Production write happens.
- **14 mutating methods** on `MemoryGraphStore` ([`store.rs:65, 115, 150, 547, 648, 750, 882, 943, 967, 1015, 1031, 1055, 1085, 1160`](../../src-tauri/src/memory_graph/store.rs)) each call `enforce_freeze` — small observability win.
- **12 production callsites still write to memory_graph** (outside `legacy_migration` + tests):
  - `tauri_commands.rs:6575, 6615, 7035` — 3 IPC commands let UI write
  - `agent/tools/builtin/load_skill.rs:171, 330` — 2 inside load_skill tool
  - `proactive/task_memory.rs:174, tool_memory.rs:384, hybrid_search.rs:593, skill_parser.rs:428, service.rs:3300` — 5 in proactive scenarios
  - `memory_graph/reflection.rs:637, environment.rs:288` — 2 internal
- `MemoryRecallEngine` still reads frozen store every chat turn (`tauri_commands.rs:2165`).

**Fix options**: (a) make `enforce_freeze` panic-by-default with explicit allowlist of 12 writes; (b) extend hook regex to catch `create_*`/`upsert_*`; (c) extract `MemoryGraphWriteApi` chokepoint. (b) or (c) is right; (a) breaks production today.

#### 🟠 MAJOR — memory_contract + MemoryPolicyExecutor dead → ❌ **Unaddressed**

- `memory_contract/` directory exists, only `impl MemoryAdapter for ...` is `FakeMemory` in test block.
- `MemoryPolicyExecutor` instantiated in **6 places, all tests**.
- **But** surrounding `memory_policy/types.rs` + `receipts.rs` ARE actively used by `runtime/context_memory_policy.rs`, `browser/runtime_memory_policy.rs`, `eval/adapters/memory_policy.rs`. So dropping the executor is safe; dropping the policy types is not.

**Fix scope**: tiny cleanup PR — drop `memory_contract/`, drop `MemoryPolicyExecutor` impl + 6 tests, keep `memory_policy::types` + `receipts`.

#### 🟠 MAJOR — importance_decay buffing deprecated path → ⚠️ **REGRESSED**

The audit said `importance_decay.rs` was dormant. **It's no longer dormant.**
- `proactive/service.rs:1364` calls `batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, batch_size, now_ms)` as a scheduled task.
- `tauri_commands.rs:7585 list_decay_candidates` exposes it to UI.
- Writes to `memory_importance_scores` (V44 migration) — namespaced under the frozen `memory_graph` subsystem.

**Decision needed before 阶段 4 picks this up by mistake**: (a) port to new adapter, (b) freeze the scenario, or (c) accept as transitional buff.

---

### §1.6 Harness / Runtime

#### 🔴 CRITICAL — harness/ offline-eval-not-supervisor → ✅ **Rename fixed**, ❌ supervisor gap remains

**Rename**: ✅ Done. `src-tauri/src/harness/` → `src-tauri/src/eval/` (commits `d25aeae3`, `9be7cfdd` / PR #568). `HarnessRuntime` → `EvalRuntime`. **CLAUDE.md "rename eval harness/" hint is stale** — that work landed pre-阶段-3.

**Supervisor gap**: ❌ The renamed `EvalRuntime` is still a pure-memory offline runner. No production ingest. Real production supervision = only `heartbeat.rs` (5s emits + flight recorder + stall detection).

The structural critique stands. The renamed `eval/` is honestly named now — making the absence of an actual runtime supervisor more visible, not less.

#### 🟠 MAJOR — TaskScheduler / workers / task_scheduler unwired → 🟡 **Partially fixed**

- `runtime/task.rs:1-12` doc: "The `TaskScheduler` preemption scaffold was removed in P2 of 阶段 2 skeleton cleanup". ✅
- `workers/` and `task_scheduler/` directories **deleted entirely**. ✅
- `runtime/contracts.rs` types (`HookDecision`, `RiskClass`, `Constraint`, `TaskEvent`, etc.) now widely adopted across `policy_eval/`, `intent_classifier/`, `agent/`, etc. ✅
- 🟡 BUT `runtime/task.rs::SessionTask` trait + `RegularTask` impl: 6 `RegularTask::new` callsites, **all in test blocks**. Zero production callers. Production agent path still `run_agentic_loop` directly via `tauri_commands.rs:2353/2363`.

Skeletons cleaned; types adopted; `SessionTask` execution surface remains test-only. The "orchestrator" still doesn't exist in production.

#### 🟠 MAJOR — 4 parallel supervision vocabs → 🟡 **Partially fixed (4→2 + bridge)**

- `workers/` vocab — gone with the directory.
- `harness/` event types — gone via rename to `eval/` (now `EvalEvent`).
- `runtime/contracts::TaskEvent` + `TaskEventSource` — alive, broadly adopted.
- `eval/case.rs::EvalSubject` + `eval/trace.rs::EvalEvent` — alive (eval-only).
- Bridge: `eval/case.rs:157 impl From<EvalSubject> for TaskEventSource` + roundtrip test still exists. Same shape as audit's citation.

4→2 vocabs with one bridge. Maintenance cost real but bounded.

---

### §1.7 Browser

#### 🟠 MAJOR — browser_task nested 2nd agent loop → 🟡 **Partially fixed** (loop persists; single high-risk action gated)

- `browser/agent_loop.rs:90 pub struct BrowserAgentLoop` still exists with its own LLM, ask_user, memory.
- `browser/agent_loop.rs:244 run` runs nested loop with `'segments: loop { for _ in 0..segment_steps {` — up to **25 inner navigations per outer `browser_task` tool call**.
- **Slice 1b plumbing landed** ([`browser/agent_loop.rs:116-121`](../../src-tauri/src/browser/agent_loop.rs)): adds `safety_manager`, `tool_dispatcher`, `approval_handler` fields + builders. Pushed into `BrowserRuntimeActionExecutor`.
- **The Evaluate-gate** ([`browser/runtime_execution.rs:138-247`](../../src-tauri/src/browser/runtime_execution.rs)): when `request.action == BrowserAction::Evaluate { script }` AND all three Slice 1b fields present, script routed through `SafetyManager.should_approve("browser_evaluate", ...)` and (if RequireApproval) through `handler.handle_ask` with `ApprovalOrigin::BrowserSubLoop`. Block/Deny/Escalate path to `Blocked` outcome.
- **Scope**: only `BrowserAction::Evaluate` (arbitrary JS). Other 11 actions (Navigate, Click, Type, Scroll, SendKeys, GetState, Screenshot, ListTabs, SwitchTab, CloseTab, UploadFile) **bypass** SafetyManager → direct to `provider_executor.execute_routed_with_identity(...)`.

The audit's "outer SafetyManager doesn't see inner navigations" is **still true for 11 of 12 actions**. The most dangerous single action (arbitrary JS) is now gated. Architectural concern (deep recursive risk inside one tool call) unchanged.

#### 🟡 MINOR — browser/ scale → ❌ **Unaddressed (slightly worse)**

- 67 modules now (was 60+). `tools.rs` 2823 LOC, `agent_loop.rs` 2227 LOC, `recipes.rs` 2317 LOC. `loop_detector.rs` is now a real ~95-line module (no longer stub) but ADR §4.5 "thin CDP lane" alternative path not implemented.

---

### §1.8 Safety — 全栈最高风险

#### 🔴 CRITICAL — 3 parallel safety models / automation bypass → 🟡 **Partially fixed** (significant chokepoint plumbing landed pre-阶段-3, but declarative bypass is by design)

**Audit grep #1**: `SafetyManager` in `automation/` → now 11 refs in 2 files. Audit's "automation/runtime/ zero SafetyManager refs" is **no longer true.**

`automation/runtime/mod.rs:25-56 build_automation_chokepoint` constructs a `ToolDispatcher` using AppState's `safety_manager`, `pending_approvals`, `hook_bus` singletons, paired with `AutomationApprovalHandler` that persists requests to `automation_approval_requests` table + emits Tauri event `automation:approval-needed`.

Wired in 3 production sites:
- `automation/runtime/service.rs:836-853` (general automation runs)
- `symphony_graph/runtime/node_run.rs:176-195` (Symphony nodes)
- `channels/dispatcher.rs:392-400` (IM chat dispatch)

`agent/headless.rs:283-333 execute_tool_calls` — when `self.tool_dispatcher.is_some()`, routes through dispatcher with `ApprovalOriginKind::Automation { activity_id }`. Otherwise falls back to bespoke path (now confined to tests/legacy).

**Audit grep #2**: `register_base_tools` still gives every automation run a `BashTool` → **still TRUE** ([`automation/runtime/tool_registry.rs:177-190`](../../src-tauri/src/automation/runtime/tool_registry.rs)).

**BUT** the dispatcher intervenes at `tool_dispatch/mod.rs:346-359`:
- `Coverage::Denied` → reject (no SafetyManager) — IM chat case explicitly denies Shell + AiBrowser
- `Coverage::Allowed` → `permission_mode_override = Some(SafetyMode::Yolo)` → AutoApprove at `:918`, **bypassing SafetyManager**
- `Coverage::FallThrough` → uses `ctx.safety_mode` → SafetyManager normal flow

**This means: automation specs that declare Shell permission get bash auto-approved with no SafetyManager logic running.** The chokepoint is declarative-bypassable by design.

**Audit grep #3 (user-asked)**: Does P3-5b1's `app_state()` accessor refactor change the threat model? **No.** P3-5b1 replaces 8 cloned fields with lazy `self.try_app_state()` lookups. Read path changes; dispatcher logic doesn't. Same `ToolDispatcher::approve` chain with same inputs.

**Net**:
- ✅ Automation runs no longer fully bypass chat SafetyManager — chokepoint plumbed.
- 🟡 Chokepoint is **declarative**: `PermissionSet::Allowed → SafetyMode::Yolo → AutoApprove`.
- 🟡 Browser sub-loop: only Evaluate gated.
- ❌ "3 parallel safety models" architectural claim still partially true: `SafetyManager` + `PermissionSet` + `boundary.rs` broker. Coordinated via shared chokepoint, not unified.

**Recommended remaining**:
1. Doc the `Allowed/Yolo` semantic in an ADR.
2. Extend browser Evaluate-gate to other risky actions (Navigate non-allowlisted, UploadFile, SendKeys to password inputs).
3. Audit `headless.rs:340 execute_tool_calls_bespoke` — restrict to tests only.

---

### §1.9 Coding — generic, not specialized

#### 🟠 MAJOR — generic run_agentic_loop + builtin edit/shell/search → ❌ **Unaddressed**

- `agent/tools/builtin/edit.rs:355-372` uses exact `content.find(old_text.as_str())`. On miss: `"old_text '...' not found in file. Make sure the text matches exactly..."` — exact failure mode hermes's 9-strategy fuzzy chain absorbs.
- Grep `fuzzy\|line_trimmed\|whitespace_norm` in `agent/tools/builtin/`: empty.
- Grep `worktree\|git_worktree` in `agent/`: empty.
- Grep `CodingTask\|CodingHarness\|CapabilityProfile` in `src-tauri/`: empty.
- `agent/code_rescue.rs` (523 LOC) is different — scavenges markdown code blocks from LLM text output, synthesizes `write_file` ToolCalls. NOT a shadow-git checkpoint store.

What edit tool DOES have (worth crediting):
- Anchor-based edits (`AnchoredEditType::Replace/InsertAfter/InsertBefore`) — uClaw mitigation for whitespace drift via stable line tokens.
- Pre-edit `GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path)` gate at `edit.rs:389-391`.
- Batch validation phase before apply phase (multi-edit batch fails atomically).

Real mitigations but different failure modes than hermes. The audit's port-recommendation stands.

---

## Prioritized backlog (post 阶段-3, pre 阶段-4)

### Tier 1 — dormant bugs / user-visible value

1. **Install CancellationToken on chat-mode + agent-mode entry points** (`tauri_commands.rs:1961, 11334`). Wire UI "stop" button to fire. ~50 LoC. **Highest leverage**.
2. **Persist + restore `CompactionState.previous_fold`** across session loads. Without it, every long-session resume pays O(N) cost on first compaction.
3. **Thread `snapshot: &TurnSnapshot` into cost/observability paths**. Kill every `self.model` read inside per-turn flow.

### Tier 2 — 阶段-3 closeout polish

4. **Move 4 post-appends** (gene_block, plan_suggest, project_rules, ladder_pad) INTO `SystemPromptContext` so `assemble_system_prompt` is genuinely the only seam. Then extend snapshot tests to cover them.
5. **Delete 3 dead `compose_system_prompt` variants** (30-line PR).
6. **Wire `estimate_context_pressure_ratio` to TokenBudgetSnapshot** + drop unused `OnContextPressure` policy variants OR ship one real conditional block.
7. **Reset `is_first_act_turn` on Plan→Auto toggle** (existing TODO at `dispatcher/mod.rs:198-202`).

### Tier 3 — pre-阶段-4 cleanup (1 PR, ~100 LoC)

8. **Drop `memory_contract/`** (dead).
9. **Drop `MemoryPolicyExecutor` impl + 6 tests** (dead). KEEP `memory_policy::types` + `receipts` (alive).
10. **Decide importance_decay** (regression): port to adapter, freeze, or accept transitional buff. **Decide before 阶段 4 picks it up by mistake.**

### Tier 4 — 阶段-4 main work (recon doc covers this)

11. Memory adapter unification per `2026-05-29-stage4-memory-adapter-recon.md`. Addresses §1.5 #1 + #3.
12. Real freeze enforcement — extend hook regex to catch `create_*`/`upsert_*` OR extract `MemoryGraphWriteApi` chokepoint.

### Tier 5 — finish AgentApi unification

13. Migrate browser/memu/MCP-proxy tool registration through `AgentApi.register_tool` with `ToolDescriptor` builders.
14. Route runtime tool lookup through `agent_api.tool(name)` (kill frozen `ToolRegistry`).
15. Build `.plugin` zip installer (Tauri command + signature check + extraction).

### Tier 6 — domain-specific (separate stages)

16. **阶段 5 (coding reliability)**: port hermes 9-strategy fuzzy match chain into `edit.rs`. Highest-ROI change in §1.9.
17. **Browser nested loop**: extend Slice 1b gate to Navigate (allowlist) / UploadFile / SendKeys. Doc the architecture as bounded blast radius (clamp 25).
18. **Supervisor reality check**: either update CLAUDE.md/ADR to call `heartbeat.rs` the production supervisor, or wire `SessionTask` into production.

---

## Acknowledgements

This audit was conducted by 2 parallel opus agents (`a3a1992e6198dbcb8` for §1.1-§1.4, `a472065d6310a22b5` for §1.5-§1.9) with ~13M tokens of file reads across the uClaw + Pi codebases. Findings verified against `c7386a7f` HEAD on 2026-05-29. The original audit `2026-05-27-pi-convergence-gap-audit.md` was approximately 2 days old at re-audit time; most line-number drifts are 15-30 lines from the recent 阶段-3 refactors.
