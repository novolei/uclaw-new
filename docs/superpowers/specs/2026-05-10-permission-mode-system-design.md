# Permission Mode System Redesign

**Date**: 2026-05-10
**Status**: Approved (brainstorm complete) — pending implementation plan
**Replaces**: ad-hoc 2-mode toggle landed in PR #42

---

## Goal

Extend uClaw's permission system from the current 2-button toggle (Auto / Bypass) to a 5-mode design matching Claude Code's selector. The new modes give users finer control over agent autonomy, plus introduce two new mechanisms for richer agent↔user communication:

1. **`ask_user` tool** — agent can pause and ask the user clarifying questions with multiple-choice or free-form options (any mode).
2. **`exit_plan_mode` tool** — agent presents a structured plan; user confirms with one of three decisions before any execution begins.

The system also introduces **layered system prompts**: behavioral guardrails (Karpathy's 4 principles) plus mode-specific operating constraints, composed with the user's existing global prompt and a new workspace-level `uclaw.md`.

---

## Non-goals

- No new agent loop architecture (still pure-Rust per CLAUDE.md §1.2).
- No automatic invocation of `superpowers:writing-plans` skill in plan mode (orthogonal layer; agent decides per-task).
- No regex/glob matching in V14 permission rules (current prefix-matching is sufficient).
- No bulk import/export of prompts or rules.

---

## The 5 Modes

Mapped from Claude Code's selector (Mac shortcut: `Shift+Cmd+M`, then `1`-`5`):

| # | Display label | Backend `SafetyMode` | Default for new users? |
|---|---|---|---|
| 1 | 🛡️ Ask permissions | `Ask` | — |
| 2 | ✏️ Accept edits | `AcceptEdits` (new) | — |
| 3 | 🗺️ Plan mode | `Plan` (new) | — |
| 4 | 🧭 Auto mode | `Supervised` | ✅ default |
| 5 | ⚡ Bypass permissions | `Yolo` | — |

**Resolver behavior table** (extension to existing `permissions::resolve_decision`):

| Effective mode \ tool's `ApprovalRequirement` | `Never` (read_file, grep, safe bash) | `UnlessAutoApproved` (edit, write_file, web_*) | `Always` (dangerous bash) |
|---|---|---|---|
| **Ask** | RequireApproval | RequireApproval | RequireApproval |
| **AcceptEdits** | RequireApproval | **AutoApprove** if tool ∈ `{edit, write_file}`, else RequireApproval | RequireApproval |
| **Supervised** (Auto) | AutoApprove | AutoApprove | RequireApproval |
| **Plan** | AutoApprove | **Block** ("Plan mode — execution blocked") | **Block** |
| **Yolo** (Bypass) | AutoApprove | AutoApprove | AutoApprove |

**Resolution priority within `should_approve` is unchanged**: blocked_tools → Never tool → auto_approved whitelist → V14 session/pattern rules → tool override → effective mode (above table). This means V14 rules still let users carve exceptions out of any mode (e.g. plan mode + V14 pattern rule allowing `bash cargo test`).

`AcceptEdits` hardcodes `EDIT_TOOLS: &[&str] = &["edit", "write_file"]` — the two stable built-in tool names.

`Plan` returns `ApprovalDecision::Block { reason: "Plan mode — execution blocked. Use exit_plan_mode to propose plan." }` for non-read tools. Agent observes this error and adjusts (calls `exit_plan_mode` or continues investigating).

---

## System Prompt Layering

Per LLM call, the dispatcher composes 4 layers (joined with `\n\n---\n\n`):

```
┌──────────────────────────────────────────────────────────┐
│ 1. Global system prompt (existing — Settings → 通用)      │  user-editable, ~/.uclaw/
├──────────────────────────────────────────────────────────┤
│ 2. uclaw.md (NEW — workspace level)                      │  user-editable, <workspace>/uclaw.md
├──────────────────────────────────────────────────────────┤
│ 3. Karpathy baseline (compile-time include_str!)         │  always injected
├──────────────────────────────────────────────────────────┤
│ 4. Mode-specific addition (compile-time, by SafetyMode)  │  empty for Auto mode
└──────────────────────────────────────────────────────────┘
                             ↓
                      [user message]
```

Empty layers are skipped (no trailing `---`).

### Layer 3 — Karpathy baseline (~200 tokens)

Adapted from [forrestchang/andrej-karpathy-skills](https://github.com/forrestchang/andrej-karpathy-skills) (MIT). 4 principles:

1. **Think before coding** — surface assumptions, call `ask_user` instead of guessing
2. **Simplicity first** — minimum code, no speculation
3. **Surgical changes** — touch only what's asked
4. **Goal-driven execution** — verifiable steps + verify checks

Always injected. License + source attribution in `prompts/baseline.md` header comment.

### Layer 4 — Mode-specific (variable size)

| Mode | Size | Emphasis |
|---|---|---|
| Ask | ~50 tokens | "Each prompt has UI cost — apply guardrail #2 (simplicity) ruthlessly" |
| AcceptEdits | ~80 tokens | "Apply guardrail #3 (surgical) intensely; if exploring, switch to Auto" |
| Plan | ~220 tokens | Full PLAN_MODE specification including `ask_user` and `exit_plan_mode` usage; emphasizes #1 (think) and #4 (goal-driven) |
| Auto | 0 tokens | (baseline alone) |
| Bypass | ~100 tokens | "Apply #2 + #3 with extreme rigor; before destructive ops, state intent in plain text; for unrecoverable ops, call `ask_user` first even though approval gate is off" |

**Token cost per LLM call**: +200 (Auto) to +420 (Plan). Acceptable.

### `uclaw.md` (Layer 2)

User-editable workspace-level prompt at `<workspace_root>/uclaw.md`. Same placement convention as Claude Code's `CLAUDE.md`.

- File doesn't exist or is empty → layer skipped
- Read fresh each LLM call (small file, OS file cache handles)
- Edited via Settings → 提示词 tab (textarea + 保存 button) **or** external editor
- First-time placeholder template includes section headers: 项目约定 / Do / Don't / 常用命令

---

## `ask_user` Tool

**Purpose**: agent can pause execution and ask the user clarifying questions, with structured options or free-form text. Available in **all modes** (not gated by Plan mode).

### Schema

Matches existing TS types in `ui/src/lib/agent-types.ts:199` (Proma-leftover, never wired backend):

```rust
pub struct AskUserParams {
    pub questions: Vec<AskUserQuestion>,
}

pub struct AskUserQuestion {
    pub question: String,
    pub header: Option<String>,
    pub multi_select: bool,
    pub options: Vec<AskUserOption>,  // empty → free-form text
}

pub struct AskUserOption {
    pub label: String,
    pub description: Option<String>,
    pub preview: Option<String>,
}
```

### Flow

1. Agent calls `ask_user({ questions: [...] })`
2. Backend: register oneshot in `PendingAskUsers` registry + emit IPC event `agent:ask_user_request` with payload matching `AskUserRequest` TS type
3. Agent loop **blocks** awaiting oneshot (same pattern as `PendingApprovals`)
4. Frontend `AskUserBanner` (existing component, 461 lines, Proma-leftover) renders questions inline at session top with multi-choice buttons / radio / checkbox / text input
5. User answers + clicks confirm → frontend calls `respond_ask_user({ requestId, answers })` Tauri command
6. Backend resolves oneshot with `{ "answers": { "question_0": "...", "question_1": "..." } }`
7. Agent receives answer as tool result, continues

### `respond_ask_user` Tauri command

```rust
pub async fn respond_ask_user(
    state: State<'_, AppState>,
    input: RespondAskUserInput,
) -> Result<(), Error>;

pub struct RespondAskUserInput {
    pub request_id: String,
    pub answers: serde_json::Map<String, serde_json::Value>,
}
```

---

## `exit_plan_mode` Tool

**Purpose**: agent declares "plan ready" and presents it for user approval. Plan mode-specific (other modes' system prompts don't mention it; if agent invokes anyway, returns "exit_plan_mode is meaningful only in Plan mode" error).

### Schema

Extends existing TS `ExitPlanModeRequest`:

```rust
pub struct ExitPlanModeParams {
    pub plan: String,                          // markdown
    pub allowed_prompts: Option<Vec<String>>,  // ["bash cargo build", ...]
}
```

### Flow

1. Agent calls `exit_plan_mode({ plan: "...", allowed_prompts: [...] })`
2. Backend: register oneshot + emit `agent:exit_plan_request`, agent loop blocks
3. Frontend `ExitPlanModeBanner` (existing 334 lines, Proma-leftover) renders the plan markdown + 3 buttons:
   - **接受 + 切到 Auto 执行** → `respond_exit_plan_mode({ requestId, decision: "accept_and_auto" })`
   - **接受 + 留 plan**（带 allowed_prompts 时） → `decision: "accept_keep_plan"`
   - **拒绝并反馈** → 弹文本框 → `decision: "reject", feedback: "..."`
4. Backend handles per decision:
   - `accept_and_auto`: set per-session `safety_mode` override to `Supervised` + resolve oneshot(success)
   - `accept_keep_plan`: write each entry in `allowed_prompts` as a V14 session pattern rule (scope='session', session_id=current, tool_name='bash', target=<prompt>, mode='allow') so those specific commands auto-pass + resolve oneshot(success)
   - `reject`: resolve oneshot returning `{ rejected: true, feedback: "..." }` → agent sees feedback as tool error → re-plans
5. Agent receives result, continues

### `respond_exit_plan_mode` Tauri command

```rust
pub async fn respond_exit_plan_mode(
    state: State<'_, AppState>,
    input: RespondExitPlanInput,
) -> Result<(), Error>;

pub struct RespondExitPlanInput {
    pub request_id: String,
    pub decision: String,        // "accept_and_auto" | "accept_keep_plan" | "reject"
    pub feedback: Option<String>,
}
```

---

## Frontend UI

### 5-mode dropdown

Replaces current cycle button. Position: same as today (input bar bottom, near model selector).

```
┌──────────────────────────────────────┐
│ Mode               [⇧] [⌘] [M]       │
├──────────────────────────────────────┤
│ 🛡️  Ask permissions              1   │
│ ✏️  Accept edits                 2   │
│ 🗺️  Plan mode                    3   │
│ 🧭  Auto mode                    4   │
│ ⚡  Bypass permissions       ✓  5   │
└──────────────────────────────────────┘
                ▲
   [⚡ Bypass permissions ▾]
```

- Implementation: Radix Popover + manual keyboard handler (not cmdk — only 5 items, no search needed)
- Keyboard: `Shift+Cmd+M` opens; while open, `1`-`5` selects, `↑↓` navigates, `Enter` confirms, `Esc` closes
- Trigger button highlights when mode != Auto (visual cue "current state is non-default")

### Mode banner

Inline pill at session top, only shown for Plan / AcceptEdits (other 3 modes are self-evident: Auto = default, Ask = obvious from frequent prompts, Bypass = user explicitly chose):

```
┌──────────────────────────────────────────────────────────┐
│ ✏️  Accept edits — file edits auto-pass; other tools ask │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│ 🗺️  Plan mode — investigating only, no execution        │
└──────────────────────────────────────────────────────────┘
```

Plan mode uses purple accent (`bg-purple-500/8 border-purple-500/30 text-purple-700`) for higher visibility.

### Settings → 提示词 tab (NEW)

Three sections:

1. **全局系统提示词** — link to existing 通用 tab (don't duplicate)
2. **项目说明 (uclaw.md)** — textarea + 保存 button + 打开外部编辑器 button + path hint + last-modified timestamp
3. **uClaw 内置行为护栏 (只读)** — collapsed by default; expanding shows the Karpathy baseline + current mode addition for transparency

Textarea is plain HTML `<textarea>` with monospace font + line numbers (no monaco/codemirror — YAGNI for v1).

### Reused (Proma-leftover) components

These already exist in `ui/src/components/agent/` but are not wired to backend:

- `AskUserBanner.tsx` (461 lines)
- `ExitPlanModeBanner.tsx` (334 lines)

Will be wired up via existing atoms (`allPendingAskUserRequestsAtom`, `allPendingExitPlanRequestsAtom`) once backend emits the IPC events. Visual / structural changes minimal — confirm prop shape matches new IPC payload.

---

## Workspace `uclaw.md` Backend

3 new Tauri commands:

```rust
#[tauri::command]
pub async fn read_workspace_uclaw_md(state: State<'_, AppState>) -> Result<String, Error>;
//   Read <active_workspace>/uclaw.md, return "" if missing

#[tauri::command]
pub async fn write_workspace_uclaw_md(
    state: State<'_, AppState>,
    content: String,
) -> Result<(), Error>;
//   Write <active_workspace>/uclaw.md, create parent if needed

#[tauri::command]
pub async fn read_default_prompts() -> Result<DefaultPromptsResponse, Error>;
//   Return KARPATHY_BASELINE + 5 mode prompts for the read-only preview UI
```

---

## Migration / Back-compat

**Schema**: zero. No DB migration needed for any of this.

**`safety_policy.json::globalMode`**: serde untagged enum extension. New values `"acceptedits" / "plan"` accepted; old values `"ask" / "supervised" / "yolo"` continue to work. Default still `Supervised`.

**Existing V14 rules**: untouched. All 5 modes still consult them in the same order (V14 rules layer is between auto_approved whitelist and tool override).

**`dispatcher.safety_mode: Option<SafetyMode>` field**: continues serving as per-session override. Used by `exit_plan_mode accept_and_auto` to switch the current session to Supervised without globally changing `safety_policy.json`.

**Existing `ApprovalModal`**: unchanged. Ask / AcceptEdits / Plan modes all trigger it via `RequireApproval`.

---

## Files to touch

### New backend files

- `src-tauri/src/agent/mode_prompts.rs` — `compose_system_prompt()` + `mode_addition()`
- `src-tauri/src/agent/prompts/baseline.md` — Karpathy baseline (MIT attribution)
- `src-tauri/src/agent/prompts/mode_ask.md`
- `src-tauri/src/agent/prompts/mode_accept_edits.md`
- `src-tauri/src/agent/prompts/mode_plan.md`
- `src-tauri/src/agent/prompts/mode_bypass.md`
- `src-tauri/src/agent/tools/builtin/ask_user.rs`
- `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs`
- `src-tauri/src/app.rs` — extend with `PendingAskUsers` + `PendingExitPlans` registries (mirroring `PendingApprovals`)

### Modified backend files

- `src-tauri/src/safety/mod.rs` — add `AcceptEdits` and `Plan` SafetyMode variants
- `src-tauri/src/safety/permissions.rs` — extend `resolve_decision` for new modes
- `src-tauri/src/agent/dispatcher.rs` — `before_llm` calls `compose_system_prompt`; register new tools
- `src-tauri/src/tauri_commands.rs` — `respond_ask_user`, `respond_exit_plan_mode`, `read_workspace_uclaw_md`, `write_workspace_uclaw_md`, `read_default_prompts`
- `src-tauri/src/main.rs` — register 5 new commands in `invoke_handler!`
- `src-tauri/src/ipc.rs` — `RespondAskUserInput`, `RespondExitPlanInput`, `DefaultPromptsResponse`

### New frontend files

- `ui/src/components/settings/PromptsSettings.tsx` — new tab content
- `ui/src/components/agent/PermissionModeMenu.tsx` — new dropdown (replaces current `PermissionModeSelector` content)
- `ui/src/components/agent/ModeBanner.tsx` — Plan/AcceptEdits banner

### Modified frontend files

- `ui/src/components/agent/PermissionModeSelector.tsx` — rewrite to use new dropdown
- `ui/src/components/agent/AskUserBanner.tsx` — verify/adapt prop shape (Proma-leftover, already present)
- `ui/src/components/agent/ExitPlanModeBanner.tsx` — same
- `ui/src/atoms/safety-atoms.ts` — `safetyModeAtom` value type expanded
- `ui/src/lib/tauri-bridge.ts` — drop the silent `.catch(() => {})` on `respondAskUser` / `respondExitPlanMode` / `respondPermission`; wire to real handlers
- `ui/src/components/settings/SettingsPanel.tsx` — add 提示词 tab nav entry
- `ui/src/atoms/settings-tab.ts` — add `'prompts'` variant

---

## Testing

### Backend (≥10 cases in `safety::permissions::tests` + `agent::mode_prompts::tests`)

| Test | Asserts |
|---|---|
| `accept_edits_passes_edit_blocks_other` | edit/write_file → AutoApprove; bash → RequireApproval |
| `plan_mode_blocks_writes_passes_reads` | read_file → AutoApprove; edit → Block |
| `plan_mode_passes_safe_bash_blocks_dangerous_bash` | `bash ls` → AutoApprove; `bash rm` → Block |
| `compose_system_prompt_includes_baseline_and_mode` | Plan mode → output contains both KARPATHY + PLAN_MODE markers |
| `compose_system_prompt_auto_mode_omits_addition` | Auto → output has base + KARPATHY only, no mode section |
| `compose_system_prompt_includes_uclaw_md` | workspace has uclaw.md → output includes its content |
| `compose_system_prompt_skips_missing_uclaw_md` | workspace has no uclaw.md → 4 sections, no empty separator |
| `ask_user_pending_resolves_oneshot` | tool call → IPC emit → respond_ask_user → tool result returned |
| `exit_plan_accept_and_auto_switches_session_safety_mode` | tool call → respond accept_and_auto → dispatcher.safety_mode set to Supervised |
| `exit_plan_accept_keep_plan_writes_session_rules` | accept_keep_plan + allowed_prompts → V14 session pattern rules created |
| `exit_plan_reject_returns_feedback` | reject + feedback → tool result contains feedback string |
| `v14_pattern_rule_overrides_plan_mode_block` | plan mode + V14 pattern rule allow `bash cargo test` → AutoApprove (escape hatch works) |

### Frontend (≥5 cases)

| Test | Asserts |
|---|---|
| `mode_dropdown_renders_5_options_with_shortcuts` | 5 rows, each shows number 1-5 |
| `mode_dropdown_keyboard_1_to_5_selects` | press 1-5 selects corresponding mode |
| `mode_banner_shows_for_plan_and_accept_edits_only` | Plan/AcceptEdits show banner; Ask/Auto/Bypass don't |
| `ask_user_banner_renders_questions_and_options` | mock ask_user request → banner renders |
| `exit_plan_modal_3_decisions_route_correctly` | 3 buttons each call respond with right decision string |
| `prompts_tab_loads_and_saves_uclaw_md` | textarea shows fetched content; save calls `writeWorkspaceUclawMd` IPC |

---

## Estimated scope

| Component | Backend LOC | Frontend LOC |
|---|---|---|
| 1. SafetyMode + resolver extension | ~80 | — |
| 2. 5-mode dropdown UI + banner | — | ~250 |
| 3. mode_prompts module + 5 prompt md files | ~80 + 5 md files | — |
| 4. ask_user + exit_plan_mode tools + IPC | ~250 | ~80 (banner wire-up) |
| 6. uclaw.md UI + 3 Tauri commands + read-only preview | ~60 | ~180 |
| 7. Tests | ~180 | ~100 |
| **Total** | **~650** | **~610** |

Plus 5 markdown prompt files (~150 lines total).

Roughly aligned with the brainstorm's "C tier — ~2 days" estimate.

---

## Acceptance criteria

- [ ] 5-mode dropdown opens via `Shift+Cmd+M`, selects via 1-5
- [ ] All 5 SafetyMode variants persist to `safety_policy.json` and round-trip cleanly
- [ ] Plan mode banner appears only in Plan/AcceptEdits sessions
- [ ] `bash echo hi > /tmp/x` in Plan mode → blocked with informative error message
- [ ] `bash cargo build` in Plan mode → blocked
- [ ] `read_file foo.rs` in Plan mode → auto-passes
- [ ] Agent calls `ask_user` → banner appears, user answers, agent receives result
- [ ] Agent calls `exit_plan_mode` with plan → modal shows; accept_and_auto switches to Supervised; accept_keep_plan creates V14 rules; reject returns feedback as tool error
- [ ] `<workspace>/uclaw.md` content appears in agent's effective system prompt (verifiable via debug log of composed prompt)
- [ ] Settings → 提示词 tab loads existing uclaw.md, edits save to file, "打开外部编辑器" opens system file browser
- [ ] All existing 50 frontend + 195 backend tests still pass
- [ ] New tests added (≥12 backend, ≥6 frontend) all pass

---

## Out of scope (deferred follow-ups)

- Plan mode automatic invocation of `superpowers:writing-plans` skill (orthogonal, agent-decided)
- Per-session `uclaw.md` (only workspace-level for now)
- In-app monaco/codemirror editor (plain textarea sufficient for v1)
- Bulk import/export of prompts / mode rules
- Mode-specific UI theming beyond Plan banner color
- Audit log "awaiting_user" status (separate V16 migration follow-up)
- Per-mode `safety_policy.json` defaults (e.g. "in Plan mode, also block tool X by default")
- exit_plan_mode allowed_prompts rule cleanup on session end
