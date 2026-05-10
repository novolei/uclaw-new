# Permission Mode Process Flow

How uClaw's 5 permission modes decide what happens when the agent tries to call a tool.

**Audience**: developers + advanced users debugging "why didn't the popup appear" / "why did agent get stuck".

---

## 1. Selecting a Mode

| UI | Action |
|---|---|
| Input bar dropdown (bottom of chat) | Click → 5 options |
| Keyboard | `Shift+Cmd+M` (Mac) / `Shift+Ctrl+M` (Win/Linux) opens menu; then `1`-`5` picks |
| Settings → 通用 | Default mode for new sessions |

The chosen mode is **global** — persisted to `~/.uclaw/safety_policy.json` field `globalMode`. All agent sessions use it until you change it. (Per-session override exists in code but no UI yet — `exit_plan_mode accept_and_auto` uses it under the hood.)

---

## 2. The Resolution Pipeline (every tool call)

Same 7-step pipeline runs for **every** tool invocation in **every** mode. The mode only affects the final step.

```
Agent calls a tool with args
        ↓
┌───────────────────────────────────────────────────────────────────┐
│ STEP 1: Tool blocked?                                              │
│   safety_policy.blockedTools.contains(tool_name) → BLOCK          │
└───────────────────────────────────────────────────────────────────┘
        ↓ no
┌───────────────────────────────────────────────────────────────────┐
│ STEP 2: Tool is intrinsically safe?                                │
│   Tool's requires_approval() == Never → AUTO-APPROVE              │
│   (read_file, grep, glob, search; bash with safe command)         │
└───────────────────────────────────────────────────────────────────┘
        ↓ no
┌───────────────────────────────────────────────────────────────────┐
│ STEP 3: Tool in global allow-list?                                 │
│   safety_policy.autoApprovedTools.contains(tool_name) → AUTO       │
│   (legacy "始终允许" non-bash buttons land here)                   │
└───────────────────────────────────────────────────────────────────┘
        ↓ no
┌───────────────────────────────────────────────────────────────────┐
│ STEP 4: V14 session rule?  (Settings → 工具权限 → 权限规则)        │
│   tool_permission_rules WHERE scope='session' AND session_id=?    │
│   AND tool_name=?  → use rule.mode (allow/block/ask)               │
└───────────────────────────────────────────────────────────────────┘
        ↓ no match
┌───────────────────────────────────────────────────────────────────┐
│ STEP 5: V14 pattern rule? (longest target prefix wins)             │
│   tool_permission_rules WHERE scope='pattern' AND tool_name=?      │
│   AND args' command starts_with target → use rule.mode             │
└───────────────────────────────────────────────────────────────────┘
        ↓ no match
┌───────────────────────────────────────────────────────────────────┐
│ STEP 6: Per-tool override?                                         │
│   safety_policy.toolOverrides.get(tool_name) → use that mode       │
└───────────────────────────────────────────────────────────────────┘
        ↓ no
┌───────────────────────────────────────────────────────────────────┐
│ STEP 7: Global SafetyMode (←the dropdown selector)                 │
│   See per-mode behavior table below                                │
└───────────────────────────────────────────────────────────────────┘
```

Every decision is also logged to `permission_audit_log` (Settings → 工具权限 → 审计日志).

---

## 3. The 5 Modes — What Step 7 Does

`tool.requires_approval()` returns one of three labels per the tool author:
- **`Never`** — read tools, safe shell commands
- **`UnlessAutoApproved`** — file edits (write_file, edit), web fetches
- **`Always`** — dangerous shell commands (rm, curl, sudo, npm install, ...)

Step 7 maps `(SafetyMode, ApprovalRequirement) → Decision`:

| Mode | `Never` tool | `UnlessAutoApproved` tool | `Always` tool |
|---|---|---|---|
| **🛡️ Ask permissions** | (auto via Step 2) | RequireApproval (popup) | RequireApproval (popup) |
| **✏️ Accept edits** | (auto via Step 2) | **AutoApprove if tool ∈ {edit, write_file}**, else RequireApproval | RequireApproval (popup) |
| **🗺️ Plan mode** | (auto via Step 2) | **Block** ("Plan mode — execution blocked") | **Block** |
| **🧭 Auto mode** (default) | (auto via Step 2) | AutoApprove | RequireApproval (popup) |
| **⚡ Bypass permissions** | (auto via Step 2) | AutoApprove | AutoApprove |

> Note: `Never` tools always auto-pass (Step 2 short-circuits). Even in Ask mode, read_file doesn't trigger 50 popups when the agent investigates a codebase. This is intentional UX.

---

## 4. What the User Sees Per Mode

| Mode | Banner at session top | Approval modal | Frequency of interruption |
|---|---|---|---|
| Ask | none (frequent prompts make state self-evident) | Every non-Never tool | High |
| Accept edits | 🔵 "Accept edits — file edits auto-pass; other tools ask" | Every non-edit, non-Never tool | Medium |
| Plan mode | 🟣 "Plan mode — investigating only, no execution" | None (writes blocked outright; no popup needed) | Low (via `exit_plan_mode` modal at end) |
| Auto | none (default state) | Only `Always` tools (dangerous bash) | Low |
| Bypass | none (user picked, accepts risk) | Never | Zero |

---

## 5. Plan Mode Lifecycle (Most Complex Mode)

The flow that wasn't obvious — what happens after the plan is "ready":

```
USER: switches to Plan mode (Shift+Cmd+M → 3)
USER: types task ("帮我规划如何加 X 功能")
        ↓
AGENT receives system prompt (4 layers):
  1. global system prompt
  2. workspace/uclaw.md
  3. Karpathy baseline
  4. PLAN MODE addition ← critical: tells agent to call exit_plan_mode
        ↓
AGENT investigates (read_file / grep / glob / safe bash all auto-pass)
        ↓
AGENT might use ask_user({...}) for clarification
  → Backend register oneshot + emit `agent:ask_user_request`
  → Frontend AskUserBanner renders questions
  → User answers → respond_ask_user IPC
  → Agent receives answer, continues investigation
        ↓
AGENT writes plan in its head → calls:
  exit_plan_mode({
    plan: "...markdown plan with steps...",
    allowed_prompts: ["bash cargo build", "bash cargo test"]
  })
        ↓
Backend register oneshot in PendingExitPlans + emit `agent:exit_plan_request`
Agent loop blocks awaiting user decision
        ↓
Frontend ExitPlanModeBanner renders the plan markdown + 3 buttons:

  ┌──────────────────────────────────────────────────────┐
  │ Agent's plan                                         │
  │ ────────────                                         │
  │ 1. Step → verify: ...                                │
  │ 2. Step → verify: ...                                │
  │                                                      │
  │ allowed_prompts (will auto-pass if you stay):        │
  │ • bash cargo build                                   │
  │ • bash cargo test                                    │
  │                                                      │
  │ [接受 + 切到 Auto] [接受 + 留 Plan] [拒绝并反馈]    │
  └──────────────────────────────────────────────────────┘
```

### Three buttons → three different state changes

#### 接受 + 切到 Auto 执行
```
USER clicks → respond_exit_plan_mode({decision: "accept_and_auto"})
            ↓
Backend: SafetyManager::set_global_mode(Supervised)
            ↓
Backend: resolve oneshot(success)
            ↓
Agent's exit_plan_mode tool returns success "Plan accepted; mode switched"
            ↓
AGENT continues — next iteration, new system prompt has Auto-mode addition
                  (no constraint), write_file / edit / bash all work
            ↓
AGENT actually writes the code
```

This is the **happy path**. Most users want this — "OK, do it."

#### 接受 + 留 Plan
```
USER clicks → respond_exit_plan_mode({
                decision: "accept_keep_plan",
                allowedPrompts: ["bash cargo build", "bash cargo test"]
              })
            ↓
Backend: for each allowed_prompt, parse "<tool> <target>"
         → V14 session pattern rule (scope=session, tool, target, mode='allow')
            ↓
Backend: resolve oneshot(success), mode stays Plan
            ↓
AGENT continues — still in Plan mode, BUT cargo build / cargo test now
                  auto-pass via V14 rules (Step 5 of pipeline)
            ↓
AGENT can run those specific commands without exiting Plan mode
USER can review build output / test results before committing to "real" execution
```

For "test the build but don't change code yet" reviews.

#### 拒绝并反馈
```
USER opens textarea, types feedback like "the test plan is too thin"
USER clicks → respond_exit_plan_mode({
                decision: "reject",
                feedback: "the test plan is too thin"
              })
            ↓
Backend: resolve oneshot with ExitPlanDecision::Reject{feedback}
            ↓
Agent's exit_plan_mode tool returns ToolError with feedback as message
            ↓
AGENT sees "User rejected the plan. Feedback: the test plan is too thin"
                  as a tool error in its conversation
            ↓
AGENT re-plans incorporating the feedback, calls exit_plan_mode again
```

---

## 6. ask_user — Independent of Mode

`ask_user` works in **any mode**. It's a way for the agent to get structured input from the user without leaving the current session flow.

```
AGENT calls ask_user({ questions: [
  {
    question: "Which template?",
    multi_select: false,
    options: [
      {label: "React + TS", description: "Recommended for this stack"},
      {label: "Plain JS", description: "Simpler, no build step"}
    ]
  }
]})
        ↓
Backend register oneshot + emit `agent:ask_user_request`
Agent loop blocks awaiting user input
        ↓
Frontend AskUserBanner renders the question(s) inline at session top
        ↓
USER picks "React + TS" → respond_ask_user({
                            requestId: "...",
                            answers: { "question_0": "React + TS" }
                          })
        ↓
Backend resolve oneshot
        ↓
Agent's tool returns: { "answers": { "question_0": "React + TS" } }
        ↓
AGENT continues with the user's chosen direction
```

Use cases:
- "Should I overwrite this file or create a backup?" (binary)
- "Which of these 5 functions did you mean?" (single-select)
- "What's your project's name?" (free-form, empty options array)
- "Pick all the modules to refactor" (multi-select)

---

## 7. Common Failure Modes

### Symptom: Agent in Plan mode, but writes succeed anyway
**Cause**: V14 rule allowed it (Step 4 or 5 of pipeline) — check Settings → 工具权限 → 权限规则.
**Fix**: delete the offending rule.

### Symptom: Agent in Plan mode never calls exit_plan_mode
**Possible causes**:
1. System prompt didn't reach LLM in Plan-mode form. Check by enabling debug log on dispatcher (`tracing::info!` already wired). The `PLAN MODE` substring should appear in the system prompt.
2. LLM confused `plan_write` (markdown journaling tool) with `exit_plan_mode`. Updated prompt explicitly disambiguates them.
3. Conversation length blew context budget so the system prompt was truncated. Try a fresh session.

### Symptom: User clicks 接受 + 切到 Auto, agent doesn't continue executing
**Cause**: backend's `respond_exit_plan_mode` only updates `safety_policy.json::globalMode` to Supervised. The dispatcher then needs to re-read it on next LLM call. With the dispatcher hotfix (PR #50), every `call_llm` re-resolves the mode → next iteration uses fresh Auto mode prompt + new tool calls auto-pass.

If you saw this fail before the hotfix: dispatcher cached the original Plan mode at session start. Reload by clicking the input box and re-prompting.

### Symptom: Approval modal doesn't appear when expected
**Cause**: was the famous bug from PR #45 — `<ApprovalModal />` never mounted at AppShell root. Fixed.
**Verify**: `grep -n "ApprovalModal" ui/src/components/app-shell/AppShell.tsx` should show one mount.

### Symptom: ask_user banner doesn't appear
**Cause**: was Proma-leftover unmount, fixed in PR #49 task T9.
**Verify**: AppShell mounts `<AskUserBanner />` + `<ExitPlanModeBanner />` next to `<ApprovalModal />`.

---

## 8. Where to Look in the Code

| Concern | File |
|---|---|
| SafetyMode enum + serde | `src-tauri/src/safety/mod.rs` |
| Resolution pipeline (steps 1-7) | `src-tauri/src/safety/permissions.rs::resolve_decision` |
| V14 rules CRUD | `src-tauri/src/safety/permissions.rs` (lookup_session_rule, lookup_pattern_rule) |
| Audit log | `permission_audit_log` table; `Settings → 工具权限 → 审计日志` |
| System prompt composition | `src-tauri/src/agent/mode_prompts.rs::compose_system_prompt` |
| Mode-specific prompts | `src-tauri/src/agent/prompts/*.md` |
| ask_user tool | `src-tauri/src/agent/tools/builtin/ask_user.rs` |
| exit_plan_mode tool | `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs` |
| Pending registries (oneshot) | `src-tauri/src/app.rs::PendingApprovals/PendingAskUsers/PendingExitPlans` |
| Frontend mode dropdown | `ui/src/components/agent/PermissionModeMenu.tsx` |
| Frontend banners | `ui/src/components/agent/{ModeBanner, AskUserBanner, ExitPlanModeBanner}.tsx` |
| Settings → 提示词 | `ui/src/components/settings/PromptsSettings.tsx` |
| Settings → 工具权限 | `ui/src/components/settings/PermissionsSettings.tsx` |
