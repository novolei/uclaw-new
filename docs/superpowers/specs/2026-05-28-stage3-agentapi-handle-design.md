# 阶段 3 — Pi 单 `AgentApi` 句柄 · Design Spec

- **Status:** Design approved (2026-05-28 via superpowers:brainstorming + visual companion). Ready for writing-plans.
- **Strategic baseline:** [`docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`](../../adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md) — Pi-lightweight kernel.
- **Source audit:** [`2026-05-27-pi-convergence-gap-audit.md`](2026-05-27-pi-convergence-gap-audit.md) §4 阶段 3 + §3.1 (Pi 借鉴) + §1.1 (prompt seam) + §1.2 MAJOR (dispatcher god object).
- **Prior cycle:** [`2026-05-28-skeleton-cleanup-stage2-closeout.md`](2026-05-28-skeleton-cleanup-stage2-closeout.md) — closed at P4, −3,475 LoC of dead skeleton removed; freed `harness/` namespace + extracted `tool_families.rs` as future schema reservation for this cycle.
- **Base commit:** `d5de1e21` (main after closeout).
- **Decided by:** Ryan (user) + claude-opus-4-7 (4 grill decisions logged in §10).

---

## 1. Goal

Collapse the 4-Registry pattern + scattered subsystem registration into a **single `AgentApi` handle** that materializes the Pi `ExtensionAPI` shape in Rust, generalize MCP's subprocess-stdio plugin loading into a **dual-tier (vanilla MCP + uClaw-extended) plugin protocol**, split the `dispatcher.rs` 3,859-LoC god object into 5 focused modules, and unify the inline-assembled `effective_system_prompt` into one canonical `assemble_system_prompt(ctx)` function.

Concretely: `AppState`'s ~64 pub fields collapse to data-deps-only; tool / provider / command / renderer / hook registration goes through one `Arc<AgentApi>` handle; one real uClaw-extended demo plugin + one vanilla MCP server both run end-to-end through the new subprocess plugin manager; dispatcher facade ≤ 200 LoC; prompt assembly is type-safe and single-site.

---

## 2. Scope (5 sub-projects, 1 design cycle)

Per audit §4 阶段 3. The user explicitly chose the ambitious cut covering all 5 — single design, 6 bisectable PRs.

| # | Sub-project | Status today | Target |
|---|---|---|---|
| 3a | Define `AgentApi` handle + builder | nonexistent | new `agent/api/` module |
| 3b | Migrate existing tools / providers / hooks through it | scattered in `ToolDispatch` / `ProviderService` / `HookBus` | unified through `AgentApi` |
| 3c | Subprocess RPC plugin loader (1 real plugin end-to-end) | only MCP (`McpManager`) | generalized `SubprocessPluginManager` + 2 demo plugins |
| 3d | Unpack `dispatcher.rs` god object (3,859 LoC / 71 fields, audit MAJOR 1.2) | one file | 5 focused modules + ≤200 LoC facade |
| 3e | Unify `effective_system_prompt` single seam (audit 1.1) | 5 dup ContentBlock sites + inline assembly across 8+ memory stores | one `assemble_system_prompt(ctx)` function |

---

## 3. Architecture (post-阶段 3)

The Pi-lightweight ADR §5 architecture layers materialize like this after 阶段 3 lands:

```
┌──────────────────────────────────────────────────────────────────┐
│  AppState (slim — data deps only)                                 │
│    db / settings / llm_config / workspace_root / data_dir         │
│    agent_api: Arc<AgentApi>   ←──── NEW; one handle, everywhere   │
├──────────────────────────────────────────────────────────────────┤
│  AgentApi (Pi ExtensionAPI shape)                                 │
│    register_tool / register_provider / register_command /         │
│    register_renderer  +  on(event, handler)                       │
│    plugin_index: PluginId → PluginRegistrationSet (subprocess     │
│                                                     attribution)  │
├──────────────────────────────────────────────────────────────────┤
│  SubprocessPluginManager (generalizes McpManager)                 │
│    discover() → spawn() → initialize() → register() → health()    │
│    Dual tier:                                                     │
│      vanilla MCP   (existing servers Just Work, "tools" only)     │
│      uclaw-extended (capabilities.uclaw.{providers/renderers/     │
│                                          hooks/commands})         │
├──────────────────────────────────────────────────────────────────┤
│  Dispatcher (split — facade ≤200 LoC)                             │
│    turn_runner / content_assembler / model_io / safety_gate /     │
│    observability                                                  │
├──────────────────────────────────────────────────────────────────┤
│  agent/prompt/assemble.rs                                         │
│    assemble_system_prompt(SystemPromptContext) → String           │
│    (single canonical site; order encoded in code, not comments;   │
│     Memory OS v2 B+D plugs into ctx.memory_load slot)             │
├──────────────────────────────────────────────────────────────────┤
│  agent loop (run_agentic_loop — pure, untouched)                  │
└──────────────────────────────────────────────────────────────────┘
```

Pi parallels (audit §3.1):
- `register_*` / `on(event)` ←→ Pi `packages/coding-agent/.../extensions/types.ts:1084` ExtensionAPI
- subprocess RPC ←→ Pi extension factory pattern, generalized for cross-language plugins via MCP-superset

---

## 4. `AgentApi` materialized

### 4.1 Module location

New module: `src-tauri/src/agent/api/`
- `mod.rs` — struct + impl block + boot/runtime phase types
- `events.rs` — `EventKind` enum + `Event` payload variants + `EventOutcome`
- `tool.rs` — `Tool` definition (re-exports / adapts existing `agent::tools::*`)
- `provider.rs` — `Provider` definition (re-exports `ProviderEntry` etc.)
- `command.rs` — `Command` definition (slash commands, CLI flags)
- `renderer.rs` — `RendererFn` + custom-type registry shape
- `plugin.rs` — `PluginId` + `PluginRegistrationSet` (for subprocess attribution)
- `tests.rs` — unit tests for the handle alone

### 4.2 Struct shape (Option 1 — single struct, phase via `&mut`/`Arc`)

Verbatim from brainstorm decision:

```rust
//! Single handle replacing the 4-Registry pattern. Pi ExtensionAPI shape.
//! Boot: register builtins, then Arc::new(api) to seal. Runtime: queries via &self.

pub struct AgentApi {
    tools:     HashMap<String, Arc<Tool>>,
    providers: HashMap<String, Arc<Provider>>,
    commands:  HashMap<String, Arc<Command>>,
    renderers: HashMap<&'static str, RendererFn>,
    hooks:     HashMap<EventKind, Vec<HookFn>>,
    plugin_index: HashMap<PluginId, PluginRegistrationSet>,
}

impl AgentApi {
    pub fn new() -> Self { /* empty handle */ }

    // ── boot-time registration (&mut self) ──────────────────────────
    pub fn register_tool(&mut self, t: Tool) { ... }
    pub fn register_provider(&mut self, p: Provider) { ... }
    pub fn register_command(&mut self, c: Command) { ... }
    pub fn register_renderer(&mut self, custom_type: &'static str, r: RendererFn) { ... }
    pub fn on<F>(&mut self, ev: EventKind, h: F)
        where F: Fn(&Event) -> BoxFuture<'static, Result<EventOutcome>>
                 + Send + Sync + 'static { ... }

    // ── plugin registration (subprocess RPC handshake calls these) ──
    pub(crate) fn register_plugin(&mut self, id: PluginId, set: PluginRegistrationSet) { ... }
    pub(crate) fn unregister_plugin(&mut self, id: PluginId) { ... }

    // ── runtime queries (&self) ─────────────────────────────────────
    pub fn tool(&self, name: &str) -> Option<&Tool> { ... }
    pub fn provider(&self, id: &str) -> Option<&Provider> { ... }
    pub fn command(&self, name: &str) -> Option<&Command> { ... }
    pub fn renderer(&self, custom_type: &str) -> Option<&RendererFn> { ... }
    pub async fn emit(&self, ev: Event) -> Result<EventOutcome> {
        // run all hooks for ev.kind in registration order; fold patches
    }
}

// AppState gets one new field:
//     pub agent_api: Arc<AgentApi>,
// AppState's existing per-subsystem fields (ProviderService, HookBus,
// ToolDispatch, etc.) STAY — Cut A keeps them as the underlying storage;
// what changes in P3-2/P3-3 is that registration call paths route THROUGH
// AgentApi instead of touching those subsystems directly. Read paths can
// stay direct in P3 or migrate piecemeal in a later cycle.
```

### 4.3 Event surface

`EventKind` is intentionally smaller than Pi's 32-event surface — uClaw-essential only:

```rust
pub enum EventKind {
    SessionStart, SessionShutdown,
    TurnStart, TurnEnd,
    BeforeProviderRequest, AfterProviderResponse,
    ToolCall, ToolResult,
    MessageStart, MessageEnd,
    BeforeContextAssembly,   // hook seam for prompt building (audit 1.1)
    BeforeCancellation,      // hook seam for graceful shutdown (Slice 1a wiring)
    PluginShutdown,          // subprocess plugin teardown
}
```

`Event`, `EventPayload`, `EventOutcome`:

```rust
pub struct Event {
    pub kind: EventKind,
    pub payload: EventPayload,        // enum variant matching kind
    pub session_id: SessionId,
    pub cancellation_token: CancellationToken,    // Slice 1a integration
}

pub enum EventOutcome {
    Continue,                          // no-op / passthrough
    Patch(EventPatch),                 // ToolResultPatch / ContextPatch / MessagePatch
    Abort(String),                     // hook vetoes — surfaces as safety/policy denial
}
```

Hooks are `Box<dyn Fn>` closures (per audit + Pi parallel), NOT a 30-method `trait Plugin`.

---

## 5. Subprocess RPC plugin protocol

### 5.1 Manifest — `plugins/<id>/plugin.toml`

Reuses `plugin_manifest/schema.rs` (preserved in P2 for exactly this purpose). Example:

```toml
[plugin]
id           = "uclaw.echo"
name         = "Echo Plugin"
version      = "0.1.0"
description  = "Demo plugin proving the uClaw-extended tier"
author       = "uClaw team"

[runtime]
kind         = "subprocess"          # future: "compile-time" | "declarative-file"
executable   = "./echo-plugin"
args         = []
protocol     = "uclaw-mcp"           # "mcp" = vanilla; "uclaw-mcp" = MCP+extensions

[permissions]
needs_network = false
needs_fs      = ["read:plugin_dir"]
events        = ["TurnEnd"]          # declared up-front; loader gate-checks at registration

[contributes]
tools         = ["echo"]
renderers     = ["echo.detail"]
```

### 5.2 Lifecycle

`SubprocessPluginManager` (new module, generalizes `McpManager`):

```
1. discover()    scan plugins/ for plugin.toml manifests
2. spawn(id)     fork subprocess with stdio piped (MCP transport reused)
3. initialize    JSON-RPC handshake: { protocolVersion, capabilities }
4. register      AgentApi.register_plugin(id, set);
                 each declared tool → Tool whose dispatch routes RPC to subprocess
                 each declared hook → emit-proxy
5. health        heartbeat / readiness watch (configurable timeout)
6. shutdown      AgentApi.unregister_plugin(id);
                 JSON-RPC shutdown notification;
                 SIGTERM → drain → SIGKILL backoff.
                 Triggers: app exit, subprocess crash, user disable.
```

Existing `McpManager` becomes a thin compatibility adapter: for `[runtime].protocol = "mcp"` manifests, the new manager treats them as vanilla MCP servers; existing `~/.config/uclaw/mcp.json` continues to work via a discovery shim that synthesizes manifests from the config entries (so users don't need to migrate their MCP configs).

### 5.3 Capability handshake

```
# Vanilla MCP server reply (existing playwright-mcp etc.):
{ "protocolVersion":"2025-03-26",
  "capabilities":{ "tools":{"listChanged":true} } }
→ loader registers tools only via MCP tools/list.

# uClaw-extended plugin reply:
{ "protocolVersion":"2025-03-26",
  "capabilities":{
    "tools":      {"listChanged":true},                  // standard MCP
    "uclaw": {                                           // extension namespace
      "providers": {"listChanged":false},
      "renderers": ["echo.detail"],
      "hooks":     ["TurnEnd"],
      "commands":  []
    }
  } }
→ loader additionally queries:
    uclaw/providers/list, uclaw/renderers/list, uclaw/hooks/list,
    uclaw/commands/list, uclaw/hooks/invoke (on matching events).
```

**Critical rule**: a plugin missing the `"uclaw"` capability key MUST fall back to vanilla MCP. That is what makes existing MCP servers Just Work — and is the load-bearing guarantee for the P3-4 backward-compat regression test.

### 5.4 Two end-to-end demos (P3-4 deliverable)

Per brainstorm choice D:

1. **Echo plugin** (uClaw-extended) — `plugins/uclaw-echo/`:
   - Registers 1 tool (`echo`), 1 renderer (`echo.detail`), 1 hook (`TurnEnd` for logging).
   - Subprocess written in Rust (for simplicity); manifest `protocol = "uclaw-mcp"`.
   - Lives in the repo as a worked example for plugin authors.
   - Loaded at boot; the `echo` tool appears in the agent's tool list.

2. **One existing MCP server** (vanilla) — the user picks one from their current `~/.config/uclaw/mcp.json` (e.g., `playwright-mcp` if configured).
   - Loaded via discovery shim → spawned → handshake → registers only tools (no `uclaw` capability).
   - Regression test: same tool names + same dispatch behavior as before the migration.

---

## 6. Dispatcher split (P3-5)

### 6.1 Module layout

```
agent/dispatcher.rs              (~200 LoC)   facade only — public surface
agent/dispatcher/
├── mod.rs                       (~50 LoC)    re-exports + module wiring
├── turn_runner.rs               (~600 LoC)   run_turn_body — the main loop step
├── content_assembler.rs         (~500 LoC)   THE single ContentBlock assembly site
│                                              (collapses 5 dup sites in current code)
├── model_io.rs                  (~400 LoC)   stream_completion + provider IO,
│                                              talks via AgentApi.provider()
├── safety_gate.rs               (~150 LoC)   delegates to SafetyManager
│                                              (already unified in Slice 1b)
└── observability.rs             (~300 LoC)   trace + telemetry + AgentApi.emit()
```

Total ~2,200 LoC of focused modules + ~200 LoC dispatcher facade ≈ 2,400 LoC — vs. the current 3,859 LoC, ~38% reduction comes from collapsing the 5 ContentBlock dups + deleting hook-emit sprinkles + removing safety check sprinkles.

### 6.2 Field-count reduction (71 → ~15)

The 71-field `ChatDelegate` god object holds direct references to most subsystems. Strategy:
- Replace per-subsystem fields with two handle references: `Arc<AgentApi>` + `Arc<AppState>`.
- Query subsystems through the handles instead of holding pointers.
- Keep on `ChatDelegate` only the fields that are turn-scoped state (current turn ID, accumulator buffers, cancellation token, etc.) — those are state, not subsystem refs.

Expected result: ~15 fields, all turn-scoped state.

### 6.3 Collapsed ContentBlock assembly

`content_assembler::assemble(ctx) -> Vec<ContentBlock>` is the single canonical assembly site. The 5 current dup sites (run_turn_body lines 99-428 per audit MAJOR) all call into this one function with different `AssemblyContext` arguments. Order invariant is encoded in the function body, not in `// step 3 must come before step 5` comments.

---

## 7. Prompt single seam (P3-6)

### 7.1 New module

`src-tauri/src/agent/prompt/assemble.rs`:

```rust
pub struct SystemPromptContext<'a> {
    pub session: &'a Session,
    pub mode: &'a Mode,
    pub user_settings: &'a UserSettings,
    pub agent_api: &'a AgentApi,
    pub workspace_root: &'a Path,
    pub recent_files: &'a [PathBuf],
    pub memory_load: MemoryLoadResult,   // ONE call to memory.load_context()
                                          // Memory OS v2 B+D plugs in here
}

pub fn assemble_system_prompt(ctx: &SystemPromptContext) -> String {
    // single canonical assembly site. Steps in code, not comments:
    //   1. mode preamble
    //   2. capability descriptor (from ctx.agent_api.tools())
    //   3. memory context (placeholder until Memory OS v2 B lands)
    //   4. workspace context
    //   5. recent files
    //   6. skill cards (from ctx.agent_api re-exposed skills if any)
    let mut out = String::new();
    write_mode_preamble(&mut out, ctx.mode);
    write_capability_descriptor(&mut out, ctx.agent_api);
    write_memory_context(&mut out, &ctx.memory_load);
    write_workspace_context(&mut out, ctx.workspace_root, ctx.session);
    write_recent_files(&mut out, ctx.recent_files);
    write_skill_cards(&mut out, ctx.agent_api);
    out
}
```

### 7.2 What this replaces

Per audit 1.1: the 5 sites in `dispatcher.rs` all calling into 8+ memory stores + concatenating ContentBlocks with order maintained by comments. After P3-6, those 5 sites become 5 calls to `assemble_system_prompt(...)` with different `SystemPromptContext` inputs.

The `MemoryLoadResult` placeholder is the Memory OS v2 B+D plug-in seam. Today it can be a thin shim that calls the existing memory stores in their current order; once B+D lands, it becomes a single `memory.load_context(query) -> 2000 char hard budget` call per the ADR §6.7 + openhuman architecture.

### 7.3 Golden snapshot tests

`assemble_system_prompt` gets snapshot tests covering 5 inputs (different modes / workspaces / memory results). These guard the order invariant during P3-5's dispatcher split (which moves the call sites) and during future Memory OS v2 B integration.

---

## 8. PR shape (6 PRs)

In dependency order:

| # | Scope | Size | Key gate |
|---|---|---|---|
| **P3-1** | `AgentApi` handle skeleton — new `agent/api/` module, struct + 5 register methods + EventKind enum + `on(event)`. AppState gets `agent_api: Arc<AgentApi>` field. NO migration of existing call sites yet. | small | `cargo build` clean; agent:: 764/2 baseline holds; new unit tests for the handle alone. |
| **P3-2** | Migrate `ToolDispatch.register` call sites → `AgentApi.register_tool`. ToolDispatch becomes a thin lookup layer using `agent_api.tool(name)`. | medium | Same baseline. ToolDispatch shrinks; all existing tool tests still pass. |
| **P3-3** | Migrate `ProviderService` + `HookBus` through AgentApi. `provider()` / `emit()` query routes. ProviderService becomes thin lookup; HookBus becomes thin emit-fanout (or absorbed entirely). | medium | Same baseline. Browser/automation/agent hooks still fire in registration order. |
| **P3-4** | `SubprocessPluginManager` (new module, generalizes `McpManager`) + echo demo plugin (uClaw-extended) + verify 1 vanilla MCP server still works. Discovery shim synthesizes manifests from existing `~/.config/uclaw/mcp.json`. | large | Plugin lifecycle integration test (spawn→handshake→register→tool-call→shutdown) + MCP backward-compat regression test. |
| **P3-5** | Dispatcher.rs split into 5 focused modules. 71 fields → ~15. 5 ContentBlock dup sites → 1 (`content_assembler::assemble`). | large | Same baseline; cargo build warning count unchanged or reduced. |
| **P3-6** | `effective_system_prompt` → `assemble_system_prompt(SystemPromptContext)` single seam. Golden snapshot tests for 5 known inputs. | small | Snapshot tests + baseline. |

Each PR is independently bisectable + revertable. Each follows the 阶段 2 cadence: 1 worktree → N commits → 1 PR → squash-merge. Total estimate per audit: 2-3 weeks at sonnet-implementer pace.

---

## 9. Testing strategy

### 9.1 Baselines to hold

- `cargo test --lib agent::` ≥ **764 passed / 2 pre-existing failed** (post-阶段 2 baseline). May GROW as P3-1 / P3-4 / P3-6 add new tests for new modules (api, plugin lifecycle, prompt golden snapshots). Never shrinks (no live-test deletions).
- `cargo test --lib` total ≥ **3,008 passed / 7 pre-existing failed** — must hold or grow.
- `cargo build` at **48-49 warnings** (post-阶段 2 baseline) — no net increase.

### 9.2 New test surface

| PR | New tests |
|---|---|
| P3-1 | `agent::api::tests` — register/query for each of the 5 register methods; on/emit ordering; plugin_index attribution. |
| P3-2 | No new tests (migration; existing tool tests cover behavior). |
| P3-3 | No new tests (migration; existing hook/provider tests cover behavior). |
| P3-4 | `plugin::tests::lifecycle` — spawn→handshake→register→call→shutdown integration test against the echo plugin. `plugin::tests::mcp_compat` — verifies vanilla MCP discovery shim works against a known-good config fixture. |
| P3-5 | No new tests (refactor; existing dispatcher tests cover behavior). Module structure verified by build alone. |
| P3-6 | `agent::prompt::tests::golden` — 5 snapshot tests for `assemble_system_prompt` across modes/workspaces/memory results. |

### 9.3 Cross-cycle regression guard

The MCP backward-compat regression test in P3-4 is critical. It is the only test that proves "existing MCP servers Just Work" — without it, P3-4 could ship a regression that's only caught when a user opens uClaw with their actual MCP config.

---

## 10. Open decisions (logged from brainstorm grilling, 2026-05-28)

| # | Question | Decision | Reasoning |
|---|---|---|---|
| 1 | 阶段 3 scope — split or single design? | Single design covering all 5 (3a–3e). | User ambition preference; sub-projects tightly coupled (you can't ship plugin loader without API defined). |
| 2 | Migration cut — which subsystems route through AgentApi? | Cut A — minimal Pi-aligned. Tools / providers / commands / renderers / hooks only. McpManager generalizes separately into subprocess RPC; skills + Tauri commands stay current shape. | Tightest blast radius; Cut B's maximal cut muddles "register_command" (Tauri vs slash) + "skills-as-tools" semantics. |
| 3 | Rust shape of AgentApi | Option 1 — single struct, `&mut self` registration during boot, `Arc::new(api)` seal, `&self` queries at runtime. Borrow checker enforces phase. | Simplest; closest to Pi ExtensionAPI; subprocess RPC plugin loader (separate) handles dynamic plugins, so Option 3's `Arc<RwLock<Inner>>` interior mutability is overkill. |
| 4 | Plugin protocol shape | Option B — MCP base + uClaw extensions. Dual tier loader. | Existing MCP servers Just Work; new uClaw plugins opt into register_provider/renderer/hooks via the `"uclaw"` capability key. Lowest migration cost per audit. |
| 5 | One-plugin demo target | Option D — echo plugin (uClaw-extended) + verify existing MCP server. Two plugins, both end-to-end. | Only way to honor Cut B's "two tiers" claim within 阶段 3. |

---

## 11. Non-goals

Explicitly deferred to other cycles:

- **Memory `MemoryAdapter` spine** — Memory OS v2 B+D (separate cycle per closeout §5). `SystemPromptContext.memory_load` is a placeholder seam.
- **Capability presets (office vs vibe coding)** — 阶段 6 (later).
- **Autonomy supervisor in the freed `harness/` namespace** — later cycle; P3 leaves the namespace ready.
- **Safety chokepoint** — DONE (Slice 1b, PR #564 / #565). 阶段 3's `safety_gate.rs` is a thin delegate.
- **`CancellationToken` flight-point wiring** — DONE (Slice 1a). 阶段 3 just threads the token through `Event.cancellation_token`.
- **UI / Tauri command changes** — `tauri::generate_handler!` entries stay as they are. AgentApi-routed commands are *slash commands* (in-session), not Tauri commands (IPC).
- **DB schema / migration changes** — AgentApi is in-memory; no migration version reserved.
- **Hermes-borrowed coding reliability** — 阶段 5 (later).

---

## 12. Lessons-learned forecast (where 阶段 2 closeout §6 applies)

阶段 2 closeout §6 distilled 4 lessons. 阶段 3 should pre-apply them:

1. **Audit recency check before kill** — N/A here (阶段 3 is build, not kill). But P3-2/P3-3 migrate registration; before each migration, grep the call site count + verify no in-flight feature branch touches it.
2. **Plan recon should be exhaustive grep, not hand-curated tables** — the writing-plans cycle for P3-1 must use `grep -rn "register_tool\|register_provider\|...\b" src/` to enumerate the *actual* call site count rather than a manually-curated list. Apply to every migration PR.
3. **Subagent-driven loop held up well** — continue the 阶段 2 cadence: implementer (haiku for mechanical / sonnet for surface design / opus for cumulative review per PR) + per-PR spec compliance review + code quality review + final cumulative review.
4. **Cancelling P5 was the right call** — N/A here (no cancellation predicted). But: if P3-4's plugin lifecycle is harder than expected, P3-4 can split into "P3-4a manager skeleton + echo demo" and "P3-4b MCP backward-compat verification" without losing bisectability. Pre-document this in the writing-plans cycle.

---

## 13. Closing

This design closes the brainstorm cycle for 阶段 3. Next step per the superpowers workflow:

1. **User reviews this spec** — feedback to adjust before plan-writing.
2. **`superpowers:writing-plans`** — produces 6 bisectable per-PR plans (one for each of P3-1 through P3-6). Per the closeout §6.ii lesson, each plan begins with an exhaustive grep recon, not a hand-curated table.
3. **`superpowers:subagent-driven-development`** — executes each PR using the same haiku-mechanical / sonnet-design / opus-review cadence that landed P1–P4.

阶段 3 completion is forecast at 2-3 weeks (per audit §4). PRs P3-1, P3-2, P3-3, P3-6 are small-to-medium (1-2 days each); P3-4 and P3-5 are large (3-5 days each).
