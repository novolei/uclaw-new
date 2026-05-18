# Plan-mode auto-suggest + decision-banner UX uplift

**Status:** Design v1, brainstorming gate passed 2026-05-18.
**Successor PRs:** Implementation plan TBD via `superpowers:writing-plans`.
**Prior art:** PR #183 (relevance gate hotfix), PR #184 (session-aware plan tracking).

## Problem

uClaw has a richly designed 5-mode safety system (Ask / AcceptEdits / Plan /
Supervised / Yolo) and a clean `exit_plan_mode` modal flow, but **nothing
nudges users toward Plan mode for the workloads that benefit from it most**.
In practice users sit in Yolo (Bypass permissions) by default and never
discover Plan mode — and the agent has no way to suggest the switch even when
it's the right call.

Separately, both `ExitPlanModeBanner` and `AskUserBanner` are
**double-mounted**: once globally in `AppShell.tsx` (L394, L397) and
once inline in `AgentView.tsx` (L1541, L1553). Result: the agent's
question shows up TWICE on screen simultaneously — the inline one above
the input bar, and a floating duplicate in the top-right of the layout
shell. User-reported via screenshot 2026-05-18.

(Smaller a11y gaps also exist — no focus trap, no `role="alertdialog"`,
no ARIA — these are kept in scope as objective fixes since they require
no visible UI change. Bigger redesigns like a shared `DecisionBanner`
primitive, dismiss-confirm popovers, editable `allowed_prompts`, and
markdown-in-question-text are **deliberately deferred** per user
feedback "现有的UI还可以".)

This spec covers: a new auto-suggest mechanism for Plan mode, plus a
surgical de-duplication of the existing banners and minimal a11y
hardening.

## Goals

1. When a user asks for multi-step / architectural work in Supervised or Yolo
   mode, surface a one-click "Switch to Plan mode" affordance — without
   asking the LLM to spell it out unprompted.
2. Make agent-initiated tool prompts (`ask_user`) and plan submissions
   (`exit_plan_mode`) safely cancellable, screen-reader-correct, and
   keyboard-navigable.
3. Capture per-suggestion telemetry so the keyword/threshold mix can be
   tuned empirically rather than by guesswork — and close the loop via the
   existing GEP scenario infrastructure.
4. Leave existing manual mode-switching (Shift+Cmd+M) and the
   `safety_policy.json` global default fully intact.

## Non-goals (out of scope)

- Per-workspace mode preferences (currently global; could come later).
- A keyword editor UI for the suggestion patterns (settings toggle only).
- Re-skinning every Radix component in the app — this spec touches only
  the three decision banners.
- Multimodal ask_user inputs (image/audio uploads in the modal).
- Mobile/responsive overhaul; uClaw is desktop-Tauri.

## Architecture overview

```
┌────────────────────── send_agent_message (Rust) ───────────────────────┐
│                                                                        │
│   user_msg ─► mode_suggest::suggest_plan_mode(user_msg, current_mode)  │
│               │   keyword match + gates (mode, len, dedupe)            │
│               └─► Some(reason) → emit agent:plan_mode_suggest          │
│                                  (source=keyword, pattern=…)           │
│                                  set ReasoningContext flag             │
│                                                                        │
│   agentic_loop ───► …LLM call…                                          │
│                                                                        │
│   tool result: request_plan_mode_switch                                │
│               │   gates: mode in {Supervised,Yolo}, dedupe flag false  │
│               └─► emit agent:plan_mode_suggest                         │
│                   (source=agent, reason=…, preview_steps=[…])          │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────── UI (React) ──────────────────────────────────────┐
│                                                                        │
│   PlanModeSuggestBanner   listens agent:plan_mode_suggest              │
│       │   [Switch] → invoke set_safety_mode (Plan, session-override)   │
│       │   [Skip]   → ack + persist to plan_suggest_events              │
│       └   [Never]  → settings.plan_mode_suggest_enabled = false        │
│                                                                        │
│   ExitPlanModeBanner v2   editable allowed_prompts, focus trap, ARIA  │
│   AskUserBanner v2        markdown q text, focus trap, split-dismiss   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────── Calibration loop (proactive scenario) ───────────┐
│                                                                        │
│   plan_suggest_events ──► plan_mode_calibration scenario               │
│                            • per-pattern accept rate                    │
│                            • after N=20 firings: <30% → silence       │
│                            • aggregate accept rate ─► LLM sys-prompt  │
│                              hint via GEP gene                          │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

## Detailed design

### 1. Backend keyword detector

New module `src-tauri/src/agent/mode_suggest.rs`. Pure function, no I/O:

```rust
pub struct PlanModeHint {
    pub pattern: &'static str,    // for telemetry
    pub display_reason: &'static str, // shown in banner if no agent reason
}

pub fn suggest_plan_mode(
    user_msg: &str,
    current_mode: &SafetyMode,
    already_suggested_this_session: bool,
) -> Option<PlanModeHint> {
    // Gates first, in order of cheapness
    if already_suggested_this_session { return None; }
    if !matches!(current_mode, SafetyMode::Supervised | SafetyMode::Yolo) {
        return None;  // already in Plan / AcceptEdits / Ask — no-op
    }
    if user_msg.chars().count() < 15 { return None; }

    // High-recall starter pattern table (P1a)
    static PATTERNS: &[(&str, &str)] = &[
        // Chinese verbs
        ("计划", "建议先在 Plan 模式过一遍方案"),
        ("规划", "建议先在 Plan 模式过一遍方案"),
        ("设计", "设计类任务先 Plan 一下结构更稳"),
        ("实现", "多步实现先 Plan 一下"),
        ("搭建", "搭建类任务建议先 Plan"),
        ("构建", "构建类任务建议先 Plan"),
        ("重构", "重构涉及多文件，建议先 Plan"),
        ("开发", "开发任务先 Plan 一下"),
        // Chinese "how should we" questions
        ("怎么实现", "建议先 Plan 一下实现路径"),
        ("如何实现", "建议先 Plan 一下实现路径"),
        ("怎么搭", "建议先 Plan 一下搭建步骤"),
        ("怎么做", "如果涉及多步，建议先 Plan"),
        ("怎么搞", "如果涉及多步，建议先 Plan"),
        // English
        ("plan", "Worth planning first?"),
        ("design", "Design-heavy — try Plan mode?"),
        ("refactor", "Refactor — try Plan mode?"),
        ("how should", "Sounds like planning — try Plan mode?"),
        ("how do I", "Sounds like planning — try Plan mode?"),
        ("how to ", "Sounds like planning — try Plan mode?"),
        ("let's build", "Build it — Plan mode first?"),
    ];
    for (pat, reason) in PATTERNS {
        if user_msg.to_lowercase().contains(pat) || user_msg.contains(pat) {
            return Some(PlanModeHint { pattern: pat, display_reason: reason });
        }
    }
    None
}
```

**False-positive philosophy:** start permissive, calibrate via telemetry
(see §5). Initial gates (mode + length + session dedupe) cut the obvious
FP cases (short interjections, already-in-Plan).

Wired into `send_agent_message` (tauri_commands.rs L6912): after the
ENTRY log and BEFORE the agent loop starts. Emits
`agent:plan_mode_suggest` IPC event with payload
`{ source: "keyword", pattern, reason, session_id, user_msg_id }`.

### 2. LLM tool `request_plan_mode_switch`

New built-in at `src-tauri/src/agent/tools/builtin/plan_mode.rs`. Schema:

```json
{
  "type": "object",
  "properties": {
    "reason": { "type": "string",
        "description": "Why Plan mode would help here. 1-2 sentences." },
    "preview_steps": { "type": "array", "items": { "type": "string" },
        "description": "Optional initial step sketch to show in the banner." }
  },
  "required": ["reason"]
}
```

Tool body:

1. If `already_suggested_this_session` flag in ReasoningContext → return
   `Err(ToolError::Execution("plan mode already suggested this session"))`.
   The error is informational, not a soft-block — the LLM sees it and
   moves on.
2. If current effective mode is Plan/AcceptEdits/Ask → return error
   "already in safer mode".
3. Otherwise: emit same `agent:plan_mode_suggest` event with payload
   `{ source: "agent", reason, preview_steps, … }`, set the dedupe flag,
   and return `Ok("suggestion shown; agent continues in current mode
   until user acts")`.

This tool **does NOT block** the agent — it's fire-and-forget. The user
sees the banner; if they click [Switch], a session-mode-override event
arrives from the frontend and takes effect on the NEXT iteration. If they
click [Skip], nothing changes. This avoids stalling the agent while
waiting for a decision the user may never make.

System prompt addition (in `prompts/baseline.md`, just below the time-
injection block):

```
You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work and they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.
```

### 3. `ReasoningContext` dedupe state

Two new fields on `ReasoningContext`:

```rust
/// Set true when EITHER the backend keyword detector OR the LLM tool has
/// fired a plan-mode suggestion for this session. Cleared only when the
/// user explicitly changes mode (accept or manual). Prevents double-
/// banners between the two paths.
pub plan_mode_suggested_in_session: bool,
/// Wall-clock instant of the last suggestion fire, used by the
/// calibration loop to compute time-to-decision metrics. None until the
/// first fire.
pub plan_mode_suggested_at: Option<std::time::Instant>,
```

Reset triggers:
- Frontend [Switch] click → backend sets mode + clears flag (so a future
  decline-back-to-Yolo flow can re-suggest)
- Frontend "Never" click → flips `planModeSuggestEnabledAtom` to false;
  the flag itself stays set but is no longer checked (the feature gate
  short-circuits before reaching it)
- Manual mode change via the existing PermissionModeSelector → clears flag

### 4. ask_user system prompt guidance

ask_user is **purely LLM-triggered** — no backend keyword detector for it,
since the user typing IS the user-initiated turn. The change is system-
prompt-only, added to `prompts/baseline.md`:

```
Call `ask_user` when you need a decision from the user before continuing:
  - The request has 2+ plausible interpretations and your guess could be
    wrong by 50%+
  - You're about to do something destructive (delete, force-push, drop
    table) without an explicit prior OK
  - A critical design choice depends on user preference (library choice,
    API contract shape, file structure)

Do NOT call ask_user for:
  - Trivial yes/no answerable from project context (CLAUDE.md, code)
  - Clarifying typos or grammar
  - Asking permission for things that are already auto-approved by mode
```

### 5. Telemetry table V34

New SQLite table, registered in `src-tauri/src/db/migrations.rs`:

```sql
CREATE TABLE plan_suggest_events (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    message_id      TEXT NOT NULL,     -- the user message that triggered
    source          TEXT NOT NULL,     -- 'keyword' | 'agent'
    matched_pattern TEXT,              -- pattern str when source=keyword
    reason          TEXT,              -- when source=agent
    user_msg_preview TEXT NOT NULL,    -- first 200 chars for context
    outcome         TEXT NOT NULL DEFAULT 'pending',
        -- 'pending' | 'accepted' | 'skipped' | 'silenced' | 'aborted'
    decline_reason  TEXT,              -- optional user-provided note
    fired_at        INTEGER NOT NULL,  -- ms epoch
    decided_at      INTEGER,           -- ms epoch
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_plan_suggest_session ON plan_suggest_events(session_id);
CREATE INDEX idx_plan_suggest_pattern ON plan_suggest_events(matched_pattern)
    WHERE matched_pattern IS NOT NULL;
```

Lightweight: rows < 1 KB, only created when a suggestion fires. New
backend module `src-tauri/src/agent/mode_suggest_store.rs` provides
`record_fired`, `record_outcome`, `query_per_pattern_accept_rate`.

### 6. GEP calibration scenario

New proactive scenario `plan_mode_calibration` at
`src-tauri/src/proactive/scenarios/plan_mode_calibration.rs`:

- Runs on the standard proactive cadence (30s poll).
- Reads `plan_suggest_events` from the last 7 days.
- For each pattern with `firings >= 20`: compute accept rate (accepted /
  (accepted + skipped + silenced)). If `< 0.30`, write a per-user
  override in a new key-value table `mode_suggest_overrides` marking the
  pattern as `disabled_until = now + 14d`.
- Compute aggregate accept rate across all patterns; if `< 0.20`, inject
  a Gene control signal into the LLM system prompt: "Your
  request_plan_mode_switch calls have been declined frequently. Be more
  conservative about when to call it." This rides on the existing
  GeneRetriever + format_gene_injection path (dispatcher L909).
- Per-pattern overrides loaded by `mode_suggest.rs` at the start of each
  `suggest_plan_mode` call — patterns marked disabled are skipped.

**Cold-start handling:** the calibration only activates patterns with
`firings >= 20`. Before that, all starter patterns are live as written.

### 7. UI banners

**7a. De-duplicate existing banner mounts (the actual bug fix)**

Both `AskUserBanner` and `ExitPlanModeBanner` currently render in two
places:

- `ui/src/components/app-shell/AppShell.tsx` L394 + L397 — global mount
  (renders as a floating panel in the top-right of the shell layout).
  **REMOVE these two lines + the two imports at L18-19.**
- `ui/src/components/agent/AgentView.tsx` L1541 + L1553 — inline mount
  just above the chat input. **KEEP these.** Inline is the correct
  UX home: conversation context is contiguous, the banner replaces the
  input area (`hasBannerOverlay` already gates the composer at L1560).

Net delta: 4 lines deleted from AppShell + 2 import lines = 6 lines.
The duplicate floating panel disappears; the inline banner is unaffected.

**7b. New PlanModeSuggestBanner**

Brand-new advisory banner for the plan-mode auto-suggest feature.
Mounted only in `AgentView.tsx` (alongside the other two), positioned
just above the input. Lighter visual weight than the decision banners
since it's a suggestion, not a required decision.

ASCII layout:

```
┌────────────────────────────────────────────────────────────┐
│ 💡 这个任务看起来是多步骤构建。先切到 Plan 模式让 agent   │
│   把方案敲定再执行？                                        │
│                                                            │
│ [切到 Plan 模式]   [本次不用]   不再建议 ▾                │
└────────────────────────────────────────────────────────────┘
```

Three actions:
- **切到 Plan 模式** (primary) — invokes `set_safety_mode` session
  override; banner closes; agent loop next iteration runs in Plan mode
- **本次不用** (secondary) — banner closes; persists `outcome=skipped`
  to telemetry; session dedupe flag stays set so it won't refire this
  session
- **不再建议 ▾** (tertiary, ghost) — opens small confirm popover; on
  confirm, sets `planModeSuggestEnabledAtom = false`; persists
  `outcome=silenced`

**7c. A11y baseline for all three banners** (no visible UI change)

- Wrap each banner root in Radix Dialog primitives so focus is trapped
  while the banner is open
- Set `role="alertdialog"` on ExitPlan + AskUser (blocking decisions);
  `role="status"` + `aria-live="polite"` on PlanModeSuggest (advisory)
- `aria-modal="true"` on the two alertdialogs
- Add `aria-label` to icon-only buttons (X, ▾)
- Add visible focus rings (Tailwind `focus-visible:ring-2`)

The decision-banner-rewrite items deferred per user feedback include:
shared `DecisionBanner` primitive, dismiss-confirm flow on X, editable
`allowed_prompts` chip editor, markdown rendering in AskUser question
text, channel-drop "agent disconnected" state. These are tracked as
follow-up work, not in this PR.

### 8. Settings surface

Single boolean in the existing settings store:

```ts
// ui/src/atoms/settings-atoms.ts
export const planModeSuggestEnabledAtom = atomWithStorage(
  'planModeSuggestEnabled', true
);
```

Backend mirrors via existing settings sync. The "Never" button in the
PlanModeSuggest banner flips this. Settings page adds a single toggle in
the existing Agent section: "Suggest Plan mode for complex tasks
(default: on)".

### 9. File map (implementation reference)

| File | What |
|---|---|
| `src-tauri/src/agent/mode_suggest.rs` | NEW — pattern table + `suggest_plan_mode` fn |
| `src-tauri/src/agent/mode_suggest_store.rs` | NEW — telemetry recorder |
| `src-tauri/src/agent/tools/builtin/plan_mode.rs` | NEW — `request_plan_mode_switch` tool |
| `src-tauri/src/agent/prompts/baseline.md` | MODIFY — append guidance for both tools |
| `src-tauri/src/agent/types.rs` | MODIFY — 2 new fields on `ReasoningContext` |
| `src-tauri/src/agent/dispatcher.rs` | MODIFY — register tool, clear dedupe on mode change |
| `src-tauri/src/proactive/scenarios/plan_mode_calibration.rs` | NEW — GEP loop |
| `src-tauri/src/proactive/scenarios/mod.rs` | MODIFY — register scenario |
| `src-tauri/src/db/migrations.rs` | MODIFY — add V34 |
| `src-tauri/src/tauri_commands.rs` | MODIFY — `send_agent_message` keyword hook; `respond_plan_mode_suggest` command; settings sync |
| `src-tauri/src/main.rs` | MODIFY — register new Tauri commands |
| `ui/src/components/agent/PlanModeSuggestBanner.tsx` | NEW |
| `ui/src/components/agent/ExitPlanModeBanner.tsx` | MINOR — wrap in Radix Dialog for focus trap + ARIA; no visible change |
| `ui/src/components/agent/AskUserBanner.tsx` | MINOR — same a11y wrap; no visible change |
| `ui/src/components/app-shell/AppShell.tsx` | MODIFY — **DELETE** the duplicate AskUser+ExitPlan mounts (L394, L397) and imports; do NOT mount the new PlanModeSuggestBanner here |
| `ui/src/components/agent/AgentView.tsx` | MODIFY — mount PlanModeSuggestBanner alongside the other two |
| `ui/src/atoms/safety-atoms.ts` | MODIFY — add plan-mode-suggest event subscription |
| `ui/src/atoms/settings-atoms.ts` | MODIFY — add `planModeSuggestEnabledAtom` |
| `ui/src/components/settings/...` | MODIFY — add toggle row |
| Tests | new modules in dispatcher, mode_suggest, plan_mode_calibration; banner RTL tests |

### 10. Migration registry update

After implementation, update CLAUDE.md's Active migration registry:

```
| V34 | plan_suggest_events (telemetry for plan-mode auto-suggest) | merged |
```

(V33 stays "in progress" until Symphony lands.)

## Testing strategy

- `mode_suggest`: unit tests for every pattern in the starter list +
  negative cases (already in Plan mode → None, msg too short → None,
  already suggested this session → None, "我有计划了" / "today's weather"
  false-positive proofs)
- `plan_mode` tool: returns ok when suggestion fires; returns Err when
  already-suggested or mode is already safe
- `mode_suggest_store`: roundtrip insert/query of `plan_suggest_events`
- `plan_mode_calibration`: feed synthetic event history → assert
  silencing kicks in at <30% / N>=20
- Banner UI: vitest + RTL — focus trap on open, Esc fires dismiss-
  confirm (not silent abort), markdown renders in q text, allowed_prompts
  chip editor add/remove, channel-drop state visible
- Integration: send "帮我做个 X" → expect agent:plan_mode_suggest fires
  before any LLM call; click [Switch] → mode changes; agent loop
  continues in Plan mode
- Regression: existing `text_signals_plan_work`, `plan_state`,
  `extract_active_plan_from_history` tests (from #183 / #184) must
  remain green

## Risks & open questions

1. **Banner fatigue.** If keyword recall is too high, users see the
   PlanModeSuggest banner on most messages → ignore → click "Never" →
   feature dies. The calibration loop is the main defense; the secondary
   defense is the session-level dedupe (max 1 banner per session). We
   accept that v1 may overshoot and rely on v2 telemetry to settle.
2. **GEP loop cold-start.** Until ~20 events per pattern accumulate, the
   calibrator silently no-ops. For private/personal use this may take
   weeks. Acceptable since starter patterns are deliberately high recall.
3. **Mode override clearing.** When the frontend clears
   `plan_mode_suggested_in_session` after a manual mode change, there's a
   small race: the dispatcher might already be deep into a tool call
   when the clear arrives. Race is benign — the worst case is one
   delayed suggestion next message.
4. **`request_plan_mode_switch` rate-limiting.** A misbehaving LLM could
   loop-call the tool every turn. The session-dedupe flag prevents
   duplicate banners but the LLM still wastes tokens on the error
   response. Acceptable for v1.
5. **Banner vs true modal trade-off.** We keep the banner shape (fixed
   bottom) for visual continuity with the existing UX. A future
   experiment could try a true centered modal for ExitPlanMode
   specifically — but adding focus trap + ARIA gets us 90% of the modal
   benefits without breaking the established positioning.
6. **i18n.** Starter pattern table is bilingual but hard-coded. A future
   PR could move patterns into the existing prompt config.

## Implementation order (input to writing-plans)

Likely commit shape (bisectable, one task per commit):

1. **`fix(ui): de-duplicate ask_user + exit_plan_mode banner mounts`** —
   delete L394/L397 + imports L18-19 from `AppShell.tsx`. **Smallest
   commit, highest immediate UX win, ships even if the rest stalls.**
2. V34 schema + `mode_suggest_store` (DB only, no behavior)
3. `mode_suggest` module + unit tests (logic only, not wired)
4. `request_plan_mode_switch` tool + registration + dispatcher hook
5. `ReasoningContext` dedupe fields + reset wiring
6. `send_agent_message` keyword hook (wires §1 to event)
7. `PlanModeSuggestBanner` component + AgentView mount + RTL tests
8. ExitPlanModeBanner + AskUserBanner a11y baseline (Radix Dialog wrap,
   focus trap, `role="alertdialog"`, `aria-modal`, `aria-label`s).
   No visible UI change.
9. `plan_mode_calibration` scenario + registration
10. Settings toggle + Gene injection wiring
11. Baseline prompt updates (both tools' guidance)
12. Integration tests + CLAUDE.md migration registry update

writing-plans takes this as input; final commit count may differ. The
de-dup fix is intentionally Commit 1 — if anyone reverts the rest, the
duplication bug stays fixed.
