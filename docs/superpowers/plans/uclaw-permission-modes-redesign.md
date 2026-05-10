# Permission Mode System Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend uClaw's 2-mode permission selector to a 5-mode design (Ask / Accept edits / Plan / Auto / Bypass) with two new agent↔user tools (`ask_user`, `exit_plan_mode`) and a 4-layer system prompt model with Karpathy-flavored behavioral guardrails + workspace-level `uclaw.md`.

**Architecture:** Two new `SafetyMode` variants (`AcceptEdits`, `Plan`) extend the existing resolver behavior table. Two new built-in tools reuse the `PendingApprovals` oneshot pattern from PR #45 (each gets its own registry: `PendingAskUsers`, `PendingExitPlans`). The 5 mode-specific prompts ship as compile-time `include_str!` markdown; users layer their own `<workspace>/uclaw.md` on top via Settings textarea or external editor. Frontend reuses the Proma-leftover `AskUserBanner.tsx` + `ExitPlanModeBanner.tsx` (already exist, never wired).

**Tech Stack:** Rust (rusqlite, tauri 2, tokio::sync::oneshot), React 18 + TS + Jotai + Radix Popover. No new crates required (`open` is unused — Settings tab calls existing OS file open via Tauri shell plugin if needed; Tauri already in deps).

**Spec:** `docs/superpowers/specs/2026-05-10-permission-mode-system-design.md`

---

## File Structure

### New backend files

| Path | Purpose |
|---|---|
| `src-tauri/src/agent/mode_prompts.rs` | `compose_system_prompt()` + `mode_addition()` + 5 `include_str!` constants |
| `src-tauri/src/agent/prompts/baseline.md` | Karpathy 4 principles (always injected) |
| `src-tauri/src/agent/prompts/mode_ask.md` | Ask mode addition |
| `src-tauri/src/agent/prompts/mode_accept_edits.md` | Accept edits addition |
| `src-tauri/src/agent/prompts/mode_plan.md` | Plan mode addition (largest, ~220 tokens) |
| `src-tauri/src/agent/prompts/mode_bypass.md` | Bypass mode addition |
| `src-tauri/src/agent/tools/builtin/ask_user.rs` | `AskUserTool` impl |
| `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs` | `ExitPlanModeTool` impl |

### Modified backend files

| Path | What changes |
|---|---|
| `src-tauri/src/safety/mod.rs:11` | Add `AcceptEdits` + `Plan` variants |
| `src-tauri/src/safety/permissions.rs:resolve_decision` | Extend match for new modes |
| `src-tauri/src/agent/dispatcher.rs:117` | `effective_system_prompt` calls `compose_system_prompt` |
| `src-tauri/src/agent/dispatcher.rs:32` | Dispatcher gains `workspace_root: PathBuf` field |
| `src-tauri/src/agent/tools/builtin/mod.rs` | `pub mod ask_user; pub mod exit_plan_mode;` |
| `src-tauri/src/app.rs:40` | Add `PendingAskUsers` + `PendingExitPlans` structs (mirror `PendingApprovals`) |
| `src-tauri/src/app.rs:98` | Add fields to `AppState` |
| `src-tauri/src/tauri_commands.rs:2328` | Extend `parse_safety_mode` + `safety_mode_to_str` |
| `src-tauri/src/tauri_commands.rs` | Add 5 new commands (see Task 4 + Task 5 + Task 6) |
| `src-tauri/src/main.rs:241-249` | Register `AskUserTool` + `ExitPlanModeTool`; pass workspace_root to dispatcher |
| `src-tauri/src/main.rs::invoke_handler!` | Register 5 new commands |
| `src-tauri/src/ipc.rs` | Add request/response types |

### New frontend files

| Path | Purpose |
|---|---|
| `ui/src/components/agent/PermissionModeMenu.tsx` | New 5-mode dropdown (replaces inline cycle in `PermissionModeSelector.tsx`) |
| `ui/src/components/agent/ModeBanner.tsx` | Plan/AcceptEdits banner |
| `ui/src/components/settings/PromptsSettings.tsx` | Settings → 提示词 tab |

### Modified frontend files

| Path | What changes |
|---|---|
| `ui/src/components/agent/PermissionModeSelector.tsx` | Trigger button only; popover content moved to `PermissionModeMenu` |
| `ui/src/atoms/safety-atoms.ts` | `SafetyModeWire` type expanded |
| `ui/src/lib/tauri-bridge.ts:818` | `SafetyModeWire` type; new wrappers for uclaw.md commands; drop silent `.catch()` on `respondAskUser`/`respondExitPlanMode`/`respondPermission` |
| `ui/src/components/app-shell/AppShell.tsx` | Mount `<AskUserBanner />` + `<ExitPlanModeBanner />` (currently never mounted) |
| `ui/src/components/agent/AskUserBanner.tsx` | Verify prop shape; wire to real backend payload |
| `ui/src/components/agent/ExitPlanModeBanner.tsx` | Same |
| `ui/src/components/settings/SettingsPanel.tsx` | New 提示词 tab nav entry + content branch |
| `ui/src/atoms/settings-tab.ts` | Add `'prompts'` variant |
| `ui/src/atoms/agent-atoms.ts` | Wire IPC listeners that populate `allPendingAskUserRequestsAtom` + `allPendingExitPlanRequestsAtom` from backend events |

---

## Pre-flight

- [ ] **Step 0.1: Branch + baseline check**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/permission-modes-redesign
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit && echo "tsc clean")
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```

Expected: cargo clean, ~195 backend tests passing, tsc clean, 50 frontend tests passing.

---

## Task 1: SafetyMode enum extension

**Files:**
- Modify: `src-tauri/src/safety/mod.rs:11`
- Modify: `src-tauri/src/tauri_commands.rs:2328-2342`

- [ ] **Step 1.1: Add new variants**

Edit `src-tauri/src/safety/mod.rs`. Replace the `SafetyMode` enum:

```rust
/// Safety mode determines how tool approval is handled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SafetyMode {
    /// Every tool ask. Most paranoid.
    Ask,
    /// Edit + write_file auto-pass; everything else asks.
    /// Hardcoded edit tool set: see permissions.rs::EDIT_TOOLS.
    AcceptEdits,
    /// Read-only investigation. Writes/execs return Block error.
    /// Agent uses `exit_plan_mode` tool to propose plan.
    Plan,
    /// Smart approval — high-risk tools ask, low-risk auto. Default.
    Supervised,
    /// All tools auto-approve. No friction, no safety net.
    Yolo,
}

impl Default for SafetyMode {
    fn default() -> Self {
        Self::Supervised
    }
}
```

`#[serde(rename_all = "lowercase")]` produces wire values `"ask"`, `"acceptedits"`, `"plan"`, `"supervised"`, `"yolo"`.

- [ ] **Step 1.2: Update `parse_safety_mode` + `safety_mode_to_str`**

Edit `src-tauri/src/tauri_commands.rs`. Replace lines 2328-2342:

```rust
fn parse_safety_mode(s: &str) -> Result<crate::safety::SafetyMode, Error> {
    match s {
        "ask" => Ok(crate::safety::SafetyMode::Ask),
        "acceptedits" => Ok(crate::safety::SafetyMode::AcceptEdits),
        "plan" => Ok(crate::safety::SafetyMode::Plan),
        "supervised" => Ok(crate::safety::SafetyMode::Supervised),
        "yolo" => Ok(crate::safety::SafetyMode::Yolo),
        _ => Err(Error::InvalidInput(format!(
            "Invalid safety mode: '{}'. Use 'ask', 'acceptedits', 'plan', 'supervised', or 'yolo'", s
        ))),
    }
}

fn safety_mode_to_str(mode: &crate::safety::SafetyMode) -> &'static str {
    match mode {
        crate::safety::SafetyMode::Ask => "ask",
        crate::safety::SafetyMode::AcceptEdits => "acceptedits",
        crate::safety::SafetyMode::Plan => "plan",
        crate::safety::SafetyMode::Supervised => "supervised",
        crate::safety::SafetyMode::Yolo => "yolo",
    }
}
```

- [ ] **Step 1.3: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | tail -10
```

Expected: 0 errors. Existing call sites in `should_approve_with_db` etc. still match all `SafetyMode` variants (some via `_` wildcard or explicit list; if any compile error mentions "non-exhaustive patterns", add `Plan | AcceptEdits` arms there as fall-through to existing `Ask` behavior — Task 2 will give them their real semantics).

- [ ] **Step 1.4: Test parse round-trip**

Append to existing `safety::permissions::tests` mod (`src-tauri/src/safety/permissions.rs`, near line ~520):

```rust
#[test]
fn safety_mode_serde_roundtrip_all_5_variants() {
    use crate::safety::SafetyMode;
    let modes = [
        ("ask", SafetyMode::Ask),
        ("acceptedits", SafetyMode::AcceptEdits),
        ("plan", SafetyMode::Plan),
        ("supervised", SafetyMode::Supervised),
        ("yolo", SafetyMode::Yolo),
    ];
    for (wire, expected) in modes {
        let json = format!("\"{}\"", wire);
        let parsed: SafetyMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, expected, "wire `{}` must parse to {:?}", wire, expected);
        let serialized = serde_json::to_string(&expected).unwrap();
        assert_eq!(serialized, json);
    }
}
```

Run:

```bash
cd src-tauri && cargo test --lib safety_mode_serde_roundtrip_all_5_variants 2>&1 | tail -5
```

Expected: 1 passed.

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/safety/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/safety/permissions.rs
git commit -m "$(cat <<'EOF'
feat(safety): add AcceptEdits + Plan SafetyMode variants

Extends the existing 3-variant enum to the 5 modes Claude Code's
selector exposes. Wire values:
  Ask -> "ask", AcceptEdits -> "acceptedits", Plan -> "plan",
  Supervised -> "supervised", Yolo -> "yolo".

Parse/serialize round-trip tested for all 5. Resolver behavior table
for the new variants lands in the next commit.
EOF
)"
```

---

## Task 2: Resolver — AcceptEdits + Plan behavior

**Files:**
- Modify: `src-tauri/src/safety/permissions.rs::resolve_decision` (around line 90-107)
- Modify: `src-tauri/src/safety/mod.rs::should_approve` (around line 215-238)

- [ ] **Step 2.1: Add EDIT_TOOLS constant**

Edit `src-tauri/src/safety/permissions.rs`. Add near the top of the file (after imports):

```rust
/// Tool names that auto-pass under `AcceptEdits` mode. These are stable
/// built-in tool names — see `agent/tools/builtin/{file,edit}.rs`.
const EDIT_TOOLS: &[&str] = &["edit", "write_file"];
```

- [ ] **Step 2.2: Extend resolver match**

Edit `src-tauri/src/safety/permissions.rs::resolve_decision`. Find the `let decision = match effective_mode {` block (around line 92-104). Replace with:

```rust
    let decision = match effective_mode {
        SafetyMode::Yolo => ApprovalDecision::AutoApprove,
        SafetyMode::Ask => ApprovalDecision::RequireApproval {
            reason: format!("Safety mode requires approval for tool '{}'", tool_name),
        },
        SafetyMode::AcceptEdits => {
            if EDIT_TOOLS.contains(&tool_name) {
                ApprovalDecision::AutoApprove
            } else {
                ApprovalDecision::RequireApproval {
                    reason: format!(
                        "Accept-edits mode: tool '{}' is not an edit tool, requires approval",
                        tool_name
                    ),
                }
            }
        }
        SafetyMode::Plan => match tool_approval {
            ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
            // UnlessAutoApproved + Always both indicate the tool has side
            // effects — block in plan mode regardless of category.
            _ => ApprovalDecision::Block {
                reason: format!(
                    "Plan mode — execution blocked for tool '{}'. Use exit_plan_mode to propose plan.",
                    tool_name
                ),
            },
        },
        SafetyMode::Supervised => match tool_approval {
            ApprovalRequirement::Always => ApprovalDecision::RequireApproval {
                reason: format!("Tool '{}' requires approval (high-risk)", tool_name),
            },
            ApprovalRequirement::UnlessAutoApproved => ApprovalDecision::AutoApprove,
            ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
        },
    };
```

- [ ] **Step 2.3: Mirror the same logic in legacy `should_approve` shim**

Edit `src-tauri/src/safety/mod.rs::should_approve` (around line 215). The legacy in-memory shim is used by tests that don't have a DB. Mirror the new match arms:

```rust
        let decision = match effective_mode {
            SafetyMode::Yolo => ApprovalDecision::AutoApprove,
            SafetyMode::Ask => ApprovalDecision::RequireApproval {
                reason: format!("Safety mode requires approval for tool '{}'", tool_name),
            },
            SafetyMode::AcceptEdits => {
                if matches!(tool_name, "edit" | "write_file") {
                    ApprovalDecision::AutoApprove
                } else {
                    ApprovalDecision::RequireApproval {
                        reason: format!(
                            "Accept-edits mode: tool '{}' is not an edit tool, requires approval",
                            tool_name
                        ),
                    }
                }
            }
            SafetyMode::Plan => match tool_approval {
                ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
                _ => ApprovalDecision::Block {
                    reason: format!(
                        "Plan mode — execution blocked for tool '{}'. Use exit_plan_mode to propose plan.",
                        tool_name
                    ),
                },
            },
            SafetyMode::Supervised => {
                match tool_approval {
                    ApprovalRequirement::Always => ApprovalDecision::RequireApproval {
                        reason: format!("Tool '{}' requires approval (high-risk)", tool_name),
                    },
                    ApprovalRequirement::UnlessAutoApproved => ApprovalDecision::AutoApprove,
                    ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
                }
            }
        };
```

- [ ] **Step 2.4: Add resolver tests**

Append to `src-tauri/src/safety/permissions.rs::tests` mod (after `legacy_tool_wide_session_rule_still_matches_any_command` test):

```rust
    #[test]
    fn accept_edits_passes_edit_blocks_other() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.global_mode = SafetyMode::AcceptEdits;
        // edit auto-pass
        let d = resolve_decision(&db, &policy, "sess1", "edit", &serde_json::json!({}),
            &ApprovalRequirement::UnlessAutoApproved, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
        // write_file auto-pass
        let d = resolve_decision(&db, &policy, "sess1", "write_file", &serde_json::json!({}),
            &ApprovalRequirement::UnlessAutoApproved, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
        // bash asks
        let d = resolve_decision(&db, &policy, "sess1", "bash", &serde_json::json!({"command":"ls"}),
            &ApprovalRequirement::Never, None);
        assert!(matches!(d, ApprovalDecision::RequireApproval { .. }));
    }

    #[test]
    fn plan_mode_blocks_writes_passes_reads() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.global_mode = SafetyMode::Plan;
        // read_file auto-pass (Never)
        let d = resolve_decision(&db, &policy, "sess1", "read_file", &serde_json::json!({"path":"foo"}),
            &ApprovalRequirement::Never, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
        // edit blocked (UnlessAutoApproved)
        let d = resolve_decision(&db, &policy, "sess1", "edit", &serde_json::json!({}),
            &ApprovalRequirement::UnlessAutoApproved, None);
        assert!(matches!(d, ApprovalDecision::Block { .. }));
        // bash with dangerous command blocked (Always)
        let d = resolve_decision(&db, &policy, "sess1", "bash", &serde_json::json!({"command":"rm foo"}),
            &ApprovalRequirement::Always, None);
        assert!(matches!(d, ApprovalDecision::Block { .. }));
    }

    #[test]
    fn plan_mode_passes_safe_bash() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.global_mode = SafetyMode::Plan;
        // bash with safe command (its requires_approval returns Never) auto-pass
        let d = resolve_decision(&db, &policy, "sess1", "bash", &serde_json::json!({"command":"ls"}),
            &ApprovalRequirement::Never, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn v14_pattern_rule_overrides_plan_mode_block() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.global_mode = SafetyMode::Plan;
        // Add an escape-hatch rule
        create_rule(&db, CreatePermissionRuleInput {
            scope: "pattern".into(),
            session_id: None,
            tool_name: "bash".into(),
            target: Some("cargo test".into()),
            mode: "allow".into(),
        }).unwrap();
        // bash cargo test → AutoApprove (rule wins over plan-mode block)
        let d = resolve_decision(&db, &policy, "sess1", "bash",
            &serde_json::json!({"command":"cargo test --lib"}),
            &ApprovalRequirement::Always, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }
```

- [ ] **Step 2.5: Run + commit**

```bash
cd src-tauri && cargo test --lib safety::permissions::tests 2>&1 | tail -10
```

Expected: 13 passed (was 9, +4 new).

```bash
cd src-tauri && cargo build 2>&1 | tail -3
```

Expected: 0 errors. (The DB-backed resolver and the in-memory shim now both handle all 5 variants exhaustively.)

```bash
git add src-tauri/src/safety/permissions.rs src-tauri/src/safety/mod.rs
git commit -m "$(cat <<'EOF'
feat(safety): resolver behavior for AcceptEdits + Plan modes

AcceptEdits: hardcoded EDIT_TOOLS = &["edit", "write_file"] auto-pass;
everything else falls through to RequireApproval (matches the spec —
file edits are the only thing this mode trusts).

Plan: tool's intrinsic ApprovalRequirement::Never auto-passes (read,
grep, glob, safe bash); UnlessAutoApproved / Always both Block with
"Plan mode — execution blocked" + hint to use exit_plan_mode.

V14 pattern rules layer (precedence step 2 in resolve_decision)
continues to override these defaults — so users can carve a "bash
cargo test always allowed in plan mode" exception via Settings →
工具权限 if they need an escape hatch.

4 new tests in safety::permissions::tests (13 total).
EOF
)"
```

---

## Task 3: Karpathy baseline + 5 prompt md files + compose

**Files:**
- Create: `src-tauri/src/agent/prompts/baseline.md`
- Create: `src-tauri/src/agent/prompts/mode_ask.md`
- Create: `src-tauri/src/agent/prompts/mode_accept_edits.md`
- Create: `src-tauri/src/agent/prompts/mode_plan.md`
- Create: `src-tauri/src/agent/prompts/mode_bypass.md`
- Create: `src-tauri/src/agent/mode_prompts.rs`
- Modify: `src-tauri/src/agent/mod.rs` (`pub mod mode_prompts;`)
- Modify: `src-tauri/src/agent/dispatcher.rs:32, 117`

- [ ] **Step 3.1: Create prompt directory + baseline**

```bash
mkdir -p src-tauri/src/agent/prompts
```

Create `src-tauri/src/agent/prompts/baseline.md`:

```markdown
<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".
```

- [ ] **Step 3.2: Create the 4 mode-specific prompts**

`src-tauri/src/agent/prompts/mode_ask.md`:

```markdown
[ASK PERMISSIONS MODE]

Every tool call requires user approval. Each prompt has UI cost — apply
guardrail #2 (simplicity) ruthlessly: only call a tool when you have a
clear hypothesis that it will advance the task. Don't probe "to be safe".
```

`src-tauri/src/agent/prompts/mode_accept_edits.md`:

```markdown
[ACCEPT EDITS MODE]

Edit and write_file calls auto-pass. All other tools (read tools, bash,
web_*) require user approval — keep them minimal.

Apply guardrail #3 (surgical) intensely: every changed line should trace
directly to the user's request. If you find yourself wanting to run shell
commands or fetch URLs, ask: do I need this for the edit, or am I exploring?
If exploring, ask the user to switch to Auto mode first.
```

`src-tauri/src/agent/prompts/mode_plan.md`:

```markdown
[PLAN MODE — read-only investigation]

You CAN use: read_file, grep, glob, search, and safe shell commands like
`git status`, `ls`, `cat`. Write / install / network commands return a
"Plan mode — execution blocked" error from the safety layer.

Your output IS the plan; the user will verify it before any code runs.
This is guardrail #1 (think first) and #4 (goal-driven) at maximum.

When you need clarification, call `ask_user({ questions: [...] })`. When
your plan is ready, call:

  exit_plan_mode({
    plan: "...markdown...",                          // The full plan
    allowed_prompts: ["bash cargo build", "bash cargo test"]  // optional
  })

The user will see your plan in a confirmation modal and can:
  - Accept + switch to Auto (you proceed with all execution)
  - Accept + stay in Plan (you may run only commands listed in
    allowed_prompts; useful for "test the build but don't change code yet")
  - Reject + feedback (incorporate the feedback, replan)

Format the plan as:
  1. [step] → verify: [check]
  2. [step] → verify: [check]
  ...

Strong success criteria let the execution phase run without further questions.
```

`src-tauri/src/agent/prompts/mode_bypass.md`:

```markdown
[BYPASS PERMISSIONS — NO APPROVAL GATES]

All tool calls auto-pass without user confirmation. Destructive operations
(rm, write_file overwrite, package install, network fetch) execute
immediately and CANNOT be undone by the user.

Apply guardrails #2 and #3 with extreme rigor:
  - BEFORE any destructive call, state in plain text what you're about
    to do. This is your audit trail.
  - NEVER make speculative changes ("I'll refactor this while I'm here").
  - If a single tool call could cause damage you can't undo (rm -rf,
    force push, drop table, npm install <untrusted>), pause and call
    `ask_user` first — even though the approval gate is off.
```

- [ ] **Step 3.3: Create mode_prompts.rs module**

Create `src-tauri/src/agent/mode_prompts.rs`:

```rust
//! System prompt composition with Karpathy-flavored behavioral guardrails
//! and per-SafetyMode operating constraints.
//!
//! Composition order (top → bottom = LLM priority increasing):
//!   1. User's global system prompt (from Settings → 通用)
//!   2. <workspace>/uclaw.md (workspace-level project context)
//!   3. KARPATHY_BASELINE (compile-time, always injected)
//!   4. mode_addition (compile-time, by current SafetyMode)
//!
//! Empty layers are skipped; remaining layers joined with "\n\n---\n\n".

use crate::safety::SafetyMode;
use std::path::Path;

pub const KARPATHY_BASELINE: &str = include_str!("prompts/baseline.md");

const MODE_ASK: &str = include_str!("prompts/mode_ask.md");
const MODE_ACCEPT_EDITS: &str = include_str!("prompts/mode_accept_edits.md");
const MODE_PLAN: &str = include_str!("prompts/mode_plan.md");
const MODE_BYPASS: &str = include_str!("prompts/mode_bypass.md");

pub fn mode_addition(mode: &SafetyMode) -> &'static str {
    match mode {
        SafetyMode::Ask => MODE_ASK,
        SafetyMode::AcceptEdits => MODE_ACCEPT_EDITS,
        SafetyMode::Plan => MODE_PLAN,
        SafetyMode::Supervised => "", // Auto — baseline alone
        SafetyMode::Yolo => MODE_BYPASS,
    }
}

/// Read `<workspace_root>/uclaw.md` if it exists, returning trimmed content
/// (or empty string if missing/unreadable). Reads on every call — files are
/// small and OS file cache handles it. If profiling later shows hot path,
/// add an LRU cache.
fn read_uclaw_md(workspace_root: Option<&Path>) -> String {
    workspace_root
        .map(|root| root.join("uclaw.md"))
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn compose_system_prompt(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
) -> String {
    let workspace_md = read_uclaw_md(workspace_root);
    let mode_part = mode_addition(mode);
    let parts: Vec<&str> = [
        user_global_base.trim(),
        workspace_md.as_str(),
        KARPATHY_BASELINE.trim(),
        mode_part,
    ]
    .iter()
    .copied()
    .filter(|s| !s.is_empty())
    .collect();
    parts.join("\n\n---\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp_workspace_with_uclaw(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("uclaw.md"), content).unwrap();
        dir
    }

    #[test]
    fn compose_includes_baseline_and_mode_for_plan() {
        let out = compose_system_prompt("base", None, &SafetyMode::Plan);
        assert!(out.contains("base"));
        assert!(out.contains("THINK BEFORE CODING"), "baseline missing");
        assert!(out.contains("PLAN MODE"), "plan mode addition missing");
    }

    #[test]
    fn compose_auto_mode_omits_addition() {
        let out = compose_system_prompt("base", None, &SafetyMode::Supervised);
        assert!(out.contains("base"));
        assert!(out.contains("THINK BEFORE CODING"));
        assert!(!out.contains("[ASK PERMISSIONS"));
        assert!(!out.contains("[PLAN MODE"));
        assert!(!out.contains("[BYPASS"));
    }

    #[test]
    fn compose_includes_uclaw_md_when_present() {
        let dir = tmp_workspace_with_uclaw("# project rules\nuse rust 2021");
        let out = compose_system_prompt("base", Some(dir.path()), &SafetyMode::Supervised);
        assert!(out.contains("# project rules"));
        assert!(out.contains("use rust 2021"));
    }

    #[test]
    fn compose_skips_missing_uclaw_md() {
        let dir = TempDir::new().unwrap(); // no uclaw.md inside
        let out = compose_system_prompt("base", Some(dir.path()), &SafetyMode::Supervised);
        // Should be exactly: base + sep + baseline (Auto mode adds no extra)
        let sep_count = out.matches("\n\n---\n\n").count();
        assert_eq!(sep_count, 1, "Expected exactly one separator (base|baseline), got {}", sep_count);
    }

    #[test]
    fn compose_handles_empty_user_base() {
        let out = compose_system_prompt("", None, &SafetyMode::Plan);
        // Should be: baseline + sep + plan (no leading base)
        assert!(!out.starts_with("\n"), "should not start with separator");
        assert!(out.contains("THINK BEFORE CODING"));
        assert!(out.contains("PLAN MODE"));
    }
}
```

- [ ] **Step 3.4: Add `tempfile` dev-dep if missing**

```bash
grep -E '^tempfile' src-tauri/Cargo.toml
```

If absent, append to `[dev-dependencies]` section:

```toml
tempfile = "3"
```

- [ ] **Step 3.5: Wire module + dispatcher field**

Edit `src-tauri/src/agent/mod.rs`. Add the line:

```rust
pub mod mode_prompts;
```

Edit `src-tauri/src/agent/dispatcher.rs`. Add field around line 32 (in the `ChatDelegate` struct after `system_prompt: String,`):

```rust
    workspace_root: Option<std::path::PathBuf>,
```

Update the constructor signature around line 67 (add `workspace_root: Option<PathBuf>` parameter). Update the body to assign `workspace_root` to the new field. Update `effective_system_prompt` (around line 117):

```rust
    /// Build the effective system prompt including memory context, the user's
    /// uclaw.md (workspace-level), Karpathy baseline, and mode-specific
    /// guardrails. Reads uclaw.md on every call (small file, OS cache).
    fn effective_system_prompt(&self) -> String {
        let memory_block = self.memory_context.as_deref().filter(|s| !s.is_empty());
        let mode = self.safety_mode.clone().unwrap_or_default();
        // Karpathy + uclaw.md + mode prompt composition. The "user_global_base"
        // here is the existing system_prompt + memory context if any.
        let base_with_memory = match memory_block {
            Some(ctx) => format!("{}\n\n{}", self.system_prompt, ctx),
            None => self.system_prompt.clone(),
        };
        crate::agent::mode_prompts::compose_system_prompt(
            &base_with_memory,
            self.workspace_root.as_deref(),
            &mode,
        )
    }
```

- [ ] **Step 3.6: Update dispatcher constructor call site in main.rs**

Edit `src-tauri/src/main.rs:251-262`. Pass workspace_root to the constructor. Replace:

```rust
                                    Box::new(uclaw_core::agent::dispatcher::ChatDelegate::new(
                                        std::sync::Arc::clone(&llm),
                                        tools,
                                        app_h.clone(),
                                        model.clone(),
                                        system_prompt,
                                        std::sync::Arc::clone(&safety),
                                        None,
                                        std::sync::Arc::clone(&approvals),
                                        uuid::Uuid::new_v4().to_string(),
                                    ))
```

with (add `Some(workspace.clone())` arg, in the position the constructor now expects):

```rust
                                    Box::new(uclaw_core::agent::dispatcher::ChatDelegate::new(
                                        std::sync::Arc::clone(&llm),
                                        tools,
                                        app_h.clone(),
                                        model.clone(),
                                        system_prompt,
                                        std::sync::Arc::clone(&safety),
                                        None,
                                        std::sync::Arc::clone(&approvals),
                                        uuid::Uuid::new_v4().to_string(),
                                        Some(workspace.clone()),
                                    ))
```

If there are other call sites of `ChatDelegate::new` (search with `grep -rn "ChatDelegate::new" src-tauri/src`), update each to pass `None` or the appropriate `Option<PathBuf>`.

- [ ] **Step 3.7: Build + test**

```bash
cd src-tauri && cargo build 2>&1 | tail -5
cargo test --lib agent::mode_prompts 2>&1 | tail -10
```

Expected: 0 errors, 5 tests passing.

- [ ] **Step 3.8: Commit**

```bash
git add src-tauri/src/agent/mode_prompts.rs src-tauri/src/agent/prompts/ src-tauri/src/agent/mod.rs src-tauri/src/agent/dispatcher.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(agent): Karpathy baseline + per-mode system prompts + compose

Adds 4-layer prompt composition:
  1. user's global system prompt (existing, from Settings → 通用)
  2. <workspace>/uclaw.md (NEW, read on each compose, optional)
  3. KARPATHY_BASELINE (compile-time include_str!, always injected)
  4. mode-specific addition (compile-time, by current SafetyMode)

Karpathy baseline = 4 principles (think / simplicity / surgical /
goal-driven), adapted from forrestchang/andrej-karpathy-skills (MIT,
attribution in baseline.md header).

Mode prompts:
  - Ask: "every prompt has UI cost — apply simplicity ruthlessly"
  - AcceptEdits: "apply surgical intensely; switch to Auto if exploring"
  - Plan: full spec for ask_user / exit_plan_mode usage + plan format
  - Auto: (no addition — baseline alone)
  - Bypass: "extreme rigor on simplicity + surgical; for unrecoverable
    ops, call ask_user first even though approval gate is off"

dispatcher::effective_system_prompt now calls
mode_prompts::compose_system_prompt instead of just appending memory.
ChatDelegate gains workspace_root: Option<PathBuf> field; main.rs
constructor passes the active workspace path.

5 unit tests cover compose ordering + uclaw.md presence/absence + 
empty-base edge case.
EOF
)"
```

---

## Task 4: uclaw.md Tauri commands

**Files:**
- Modify: `src-tauri/src/ipc.rs` (add response type)
- Modify: `src-tauri/src/tauri_commands.rs` (3 new commands)
- Modify: `src-tauri/src/main.rs::invoke_handler!` (register 3 new commands)

- [ ] **Step 4.1: Add IPC response type**

Edit `src-tauri/src/ipc.rs`. Append:

```rust
// ─── Default prompts (for Settings → 提示词 read-only preview) ─────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultPromptsResponse {
    pub baseline: String,
    pub mode_ask: String,
    pub mode_accept_edits: String,
    pub mode_plan: String,
    pub mode_bypass: String,
}
```

- [ ] **Step 4.2: Add 3 Tauri commands**

Edit `src-tauri/src/tauri_commands.rs`. Append (near other workspace commands; if you can't find a good neighbor, append at end of file before the test mod):

```rust
// ─── Workspace uclaw.md ────────────────────────────────────────────────

fn active_workspace_root(state: &AppState) -> Option<std::path::PathBuf> {
    // Use the active workspace setting; fall back to data_dir/workspace if unset.
    // Real workspace lookup is in workspace/ mod; for v1 we use the same
    // resolution dispatcher uses.
    let settings = state.settings.try_read().ok()?;
    let id = settings.active_workspace_id.clone()?;
    drop(settings);
    let conn = state.db.lock().ok()?;
    conn.query_row(
        "SELECT path FROM spaces WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, Option<String>>(0),
    ).ok().flatten().map(std::path::PathBuf::from)
}

#[tauri::command]
pub async fn read_workspace_uclaw_md(state: State<'_, AppState>) -> Result<String, Error> {
    let Some(root) = active_workspace_root(&state) else {
        return Ok(String::new());
    };
    let path = root.join("uclaw.md");
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::Internal(format!("read uclaw.md: {}", e))),
    }
}

#[tauri::command]
pub async fn write_workspace_uclaw_md(
    state: State<'_, AppState>,
    content: String,
) -> Result<(), Error> {
    let root = active_workspace_root(&state)
        .ok_or_else(|| Error::InvalidInput("No active workspace".into()))?;
    if !root.exists() {
        std::fs::create_dir_all(&root).map_err(|e| Error::Io(e))?;
    }
    let path = root.join("uclaw.md");
    std::fs::write(&path, content).map_err(|e| Error::Io(e))?;
    Ok(())
}

#[tauri::command]
pub async fn read_default_prompts() -> Result<crate::ipc::DefaultPromptsResponse, Error> {
    use crate::agent::mode_prompts;
    use crate::safety::SafetyMode;
    Ok(crate::ipc::DefaultPromptsResponse {
        baseline: mode_prompts::KARPATHY_BASELINE.to_string(),
        mode_ask: mode_prompts::mode_addition(&SafetyMode::Ask).to_string(),
        mode_accept_edits: mode_prompts::mode_addition(&SafetyMode::AcceptEdits).to_string(),
        mode_plan: mode_prompts::mode_addition(&SafetyMode::Plan).to_string(),
        mode_bypass: mode_prompts::mode_addition(&SafetyMode::Yolo).to_string(),
    })
}
```

If `active_workspace_id` is not the actual field name in `UserSettings`, adapt — find with:

```bash
grep -nE "active_workspace|activeWorkspace" src-tauri/src/settings/*.rs src-tauri/src/types/*.rs 2>/dev/null | head -5
```

- [ ] **Step 4.3: Register in invoke_handler!**

Edit `src-tauri/src/main.rs`. Find the `invoke_handler!` macro list. Add (near `list_recent_threads` or other workspace-y commands):

```rust
            uclaw_core::tauri_commands::read_workspace_uclaw_md,
            uclaw_core::tauri_commands::write_workspace_uclaw_md,
            uclaw_core::tauri_commands::read_default_prompts,
```

- [ ] **Step 4.4: Build + commit**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: 0 errors.

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(prompts): Tauri commands for uclaw.md + default prompts preview

Three new commands:
  - read_workspace_uclaw_md  → returns <active_workspace>/uclaw.md
    content, or empty string if missing (no error for missing file)
  - write_workspace_uclaw_md → writes content to that path; errors if
    no active workspace
  - read_default_prompts     → returns the 5 compile-time prompts
    (baseline + 4 modes; Auto omits) for the Settings UI to display
    as read-only reference

uclaw.md is read fresh by mode_prompts::compose_system_prompt on every
LLM call — these commands are purely for the editor UI; they don't
participate in prompt resolution.
EOF
)"
```

---

## Task 5: ask_user tool

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/ask_user.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (`pub mod ask_user;`)
- Modify: `src-tauri/src/app.rs` (add `PendingAskUsers` registry)
- Modify: `src-tauri/src/ipc.rs` (add request/response types)
- Modify: `src-tauri/src/tauri_commands.rs` (`respond_ask_user` command)
- Modify: `src-tauri/src/main.rs` (register tool + invoke_handler!)

- [ ] **Step 5.1: Add PendingAskUsers registry to app.rs**

Edit `src-tauri/src/app.rs`. Add after the existing `PendingApprovals` impl (around line 66):

```rust
/// Result of an ask_user response.
#[derive(Debug, Clone)]
pub struct AskUserResult {
    /// Map of question_index → answer string
    pub answers: std::collections::HashMap<String, serde_json::Value>,
}

/// Manages pending ask_user requests from agent to user.
/// Mirrors PendingApprovals — oneshot per request_id.
pub struct PendingAskUsers {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<AskUserResult>>>,
}

impl PendingAskUsers {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }

    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<AskUserResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }

    pub fn resolve(&self, request_id: &str, result: AskUserResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}
```

Add field to `AppState` (near `pub pending_approvals` at line 98):

```rust
    pub pending_ask_users: Arc<PendingAskUsers>,
```

Initialize in `AppState::new` (find where `pending_approvals = Arc::new(PendingApprovals::new())`, near line 214). Add:

```rust
        let pending_ask_users = Arc::new(PendingAskUsers::new());
```

And add `pending_ask_users,` to the struct construction.

- [ ] **Step 5.2: Add IPC types**

Edit `src-tauri/src/ipc.rs`. Append:

```rust
// ─── ask_user ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub multi_select: bool,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub questions: Vec<AskUserQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondAskUserInput {
    pub request_id: String,
    pub answers: serde_json::Map<String, serde_json::Value>,
}
```

- [ ] **Step 5.3: Create the tool**

Create `src-tauri/src/agent/tools/builtin/ask_user.rs`:

```rust
//! `ask_user` built-in tool — agent pauses execution and asks the user
//! clarifying questions with structured options or free-form text.
//!
//! Available in all SafetyModes. Reuses the PendingAskUsers oneshot
//! pattern (mirrors PendingApprovals from the approval flow).
//!
//! Flow:
//!   1. Agent calls ask_user({ questions: [...] })
//!   2. Backend register oneshot + emit `agent:ask_user_request` IPC event
//!   3. Loop blocks awaiting the oneshot
//!   4. Frontend AskUserBanner renders questions + answer UI
//!   5. User answers → respond_ask_user IPC command resolves oneshot
//!   6. Agent receives answers as tool result, continues

use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::app::PendingAskUsers;
use crate::ipc::{AskUserQuestion, AskUserRequestPayload};
use tauri::{AppHandle, Emitter};

pub struct AskUserTool {
    app_handle: AppHandle,
    pending: Arc<PendingAskUsers>,
    session_id: String,
}

impl AskUserTool {
    pub fn new(app_handle: AppHandle, pending: Arc<PendingAskUsers>, session_id: String) -> Self {
        Self { app_handle, pending, session_id }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str { "ask_user" }
    fn description(&self) -> &str {
        "Pause execution and ask the user one or more clarifying questions \
         with optional multiple-choice options. Returns the user's answers."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string"},
                            "header":   {"type": "string"},
                            "multi_select": {"type": "boolean", "default": false},
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label":       {"type": "string"},
                                        "description": {"type": "string"},
                                        "preview":     {"type": "string"}
                                    },
                                    "required": ["label"]
                                }
                            }
                        },
                        "required": ["question", "multi_select"]
                    }
                }
            },
            "required": ["questions"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Asking the user for input is intrinsically safe — no need for the
        // approval modal on top of the question banner.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let questions: Vec<AskUserQuestion> = serde_json::from_value(
            params.get("questions").cloned().unwrap_or_default()
        ).map_err(|e| ToolError::InvalidParams(format!("questions: {}", e)))?;

        if questions.is_empty() {
            return Err(ToolError::InvalidParams("questions array cannot be empty".into()));
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let rx = self.pending.register(request_id.clone());

        let payload = AskUserRequestPayload {
            request_id: request_id.clone(),
            session_id: self.session_id.clone(),
            questions,
        };
        let _ = self.app_handle.emit("agent:ask_user_request", &payload);

        let result = rx.await.map_err(|_| {
            ToolError::Execution("ask_user channel dropped — user closed without answering".into())
        })?;

        let result_json = serde_json::json!({ "answers": result.answers });
        Ok(ToolOutput::success(
            &serde_json::to_string(&result_json).unwrap_or_default(),
            start.elapsed().as_millis() as u64,
        ))
    }
}
```

- [ ] **Step 5.4: Register module + tool**

Edit `src-tauri/src/agent/tools/builtin/mod.rs`. Add:

```rust
pub mod ask_user;
```

Edit `src-tauri/src/main.rs:241-249`. Add the `AskUserTool` registration. Find:

```rust
                                    reg.register(builtin::shell::BashTool::new(workspace.clone()));
```

Append immediately after:

```rust
                                    let session_id_for_tools = uuid::Uuid::new_v4().to_string();
                                    reg.register(builtin::ask_user::AskUserTool::new(
                                        app_h.clone(),
                                        std::sync::Arc::clone(&pending_ask_users),
                                        session_id_for_tools.clone(),
                                    ));
```

You'll also need to grab `pending_ask_users` from AppState earlier in the closure setup. Find the existing line that grabs `approvals` (search for `approvals = std::sync::Arc::clone`) and add a parallel line for `pending_ask_users`. The exact context will need a small `app_h.state::<AppState>()` lookup — adapt to match the surrounding pattern.

If the `session_id_for_tools` is already declared (look for existing usage), reuse it instead of declaring a new one. The dispatcher line (`uuid::Uuid::new_v4().to_string()` arg #9) and the tool's session_id should be the same value so `agent:ask_user_request` events carry the dispatcher's conversation_id.

- [ ] **Step 5.5: Add respond_ask_user Tauri command**

Edit `src-tauri/src/tauri_commands.rs`. Append:

```rust
#[tauri::command]
pub async fn respond_ask_user(
    state: State<'_, AppState>,
    input: crate::ipc::RespondAskUserInput,
) -> Result<(), Error> {
    let answers: std::collections::HashMap<String, serde_json::Value> = input.answers
        .into_iter()
        .collect();
    let result = crate::app::AskUserResult { answers };
    let resolved = state.pending_ask_users.resolve(&input.request_id, result);
    if !resolved {
        tracing::warn!(request_id = %input.request_id, "respond_ask_user: no matching pending request");
    }
    Ok(())
}
```

Register in `src-tauri/src/main.rs::invoke_handler!`:

```rust
            uclaw_core::tauri_commands::respond_ask_user,
```

- [ ] **Step 5.6: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: 0 errors.

- [ ] **Step 5.7: Test ask_user flow**

Append to `src-tauri/src/agent/tools/builtin/ask_user.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AskUserResult, PendingAskUsers};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// We can't easily test `execute` end-to-end (needs AppHandle). But we
    /// can verify the registry round-trip — the same primitive the tool uses.
    #[tokio::test]
    async fn pending_ask_users_register_and_resolve() {
        let pending = Arc::new(PendingAskUsers::new());
        let rx = pending.register("req-1".into());
        let mut answers = HashMap::new();
        answers.insert("question_0".into(), serde_json::Value::String("A".into()));
        let resolved = pending.resolve("req-1", AskUserResult { answers: answers.clone() });
        assert!(resolved);
        let result = rx.await.unwrap();
        assert_eq!(result.answers, answers);
    }

    #[tokio::test]
    async fn pending_ask_users_resolve_unknown_returns_false() {
        let pending = Arc::new(PendingAskUsers::new());
        let resolved = pending.resolve("unknown", AskUserResult { answers: HashMap::new() });
        assert!(!resolved);
    }
}
```

Run:

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::ask_user 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 5.8: Commit**

```bash
git add src-tauri/src/app.rs src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/agent/tools/builtin/
git commit -m "$(cat <<'EOF'
feat(agent): ask_user built-in tool + IPC + PendingAskUsers registry

Agent can now call ask_user({ questions: [...] }) to pause execution
and request structured (multi-choice) or free-form answers from the
user. Available in all SafetyModes.

Wire pattern mirrors the existing PendingApprovals oneshot flow from
PR #45:
  - Tool's execute() registers a oneshot in app.pending_ask_users
  - Emits `agent:ask_user_request` IPC event with payload matching
    the existing TS AskUserRequest type (Proma-leftover, never wired
    until now)
  - Awaits the channel; user's answers via respond_ask_user resolve it
  - Returns answers as tool result JSON

Frontend wiring (mount AskUserBanner + IPC listener) lands in the
frontend tasks below.
EOF
)"
```

---

## Task 6: exit_plan_mode tool

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (`pub mod exit_plan_mode;`)
- Modify: `src-tauri/src/app.rs` (add `PendingExitPlans` registry)
- Modify: `src-tauri/src/ipc.rs` (add request/response types)
- Modify: `src-tauri/src/tauri_commands.rs` (`respond_exit_plan_mode` command)
- Modify: `src-tauri/src/main.rs` (register tool + invoke_handler!)

- [ ] **Step 6.1: Add PendingExitPlans registry**

Edit `src-tauri/src/app.rs`. Add after `PendingAskUsers`:

```rust
/// Decision from the user on an exit_plan_mode request.
#[derive(Debug, Clone)]
pub enum ExitPlanDecision {
    /// User accepted; switch session SafetyMode to Supervised and proceed.
    AcceptAndAuto,
    /// User accepted but wants to stay in Plan; agent may run only the
    /// pre-declared allowed_prompts.
    AcceptKeepPlan,
    /// User rejected; agent receives feedback as tool error.
    Reject { feedback: String },
}

#[derive(Debug, Clone)]
pub struct ExitPlanResult {
    pub decision: ExitPlanDecision,
}

pub struct PendingExitPlans {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ExitPlanResult>>>,
}

impl PendingExitPlans {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }
    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<ExitPlanResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }
    pub fn resolve(&self, request_id: &str, result: ExitPlanResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}
```

Add field to `AppState`:

```rust
    pub pending_exit_plans: Arc<PendingExitPlans>,
```

Initialize in `AppState::new`:

```rust
        let pending_exit_plans = Arc::new(PendingExitPlans::new());
```

And add to struct construction.

- [ ] **Step 6.2: Add IPC types**

Edit `src-tauri/src/ipc.rs`. Append:

```rust
// ─── exit_plan_mode ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitPlanRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub plan: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_prompts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondExitPlanInput {
    pub request_id: String,
    /// "accept_and_auto" | "accept_keep_plan" | "reject"
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    /// For accept_keep_plan + accept_and_auto, the original allowed_prompts
    /// list (frontend echoes them back since backend's pending registry
    /// doesn't store the request body).
    #[serde(default)]
    pub allowed_prompts: Vec<String>,
    /// Echo of session_id (frontend already knows it, simpler than backend
    /// stashing it).
    pub session_id: String,
}
```

- [ ] **Step 6.3: Create the tool**

Create `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs`:

```rust
//! `exit_plan_mode` built-in tool — agent declares "plan ready" with a
//! markdown plan + optional allowed_prompts list. User sees a confirmation
//! modal and can:
//!   - accept_and_auto  → backend switches session SafetyMode to Supervised
//!   - accept_keep_plan → backend writes allowed_prompts as V14 session
//!                        pattern rules (so e.g. `bash cargo test` becomes
//!                        auto-pass while staying in Plan mode)
//!   - reject           → tool returns error with user's feedback string

use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::app::PendingExitPlans;
use crate::ipc::ExitPlanRequestPayload;
use tauri::{AppHandle, Emitter};

pub struct ExitPlanModeTool {
    app_handle: AppHandle,
    pending: Arc<PendingExitPlans>,
    session_id: String,
}

impl ExitPlanModeTool {
    pub fn new(app_handle: AppHandle, pending: Arc<PendingExitPlans>, session_id: String) -> Self {
        Self { app_handle, pending, session_id }
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str { "exit_plan_mode" }
    fn description(&self) -> &str {
        "Submit your plan to the user for approval. The user will see a \
         confirmation modal and can accept (switching to Auto), accept but \
         stay in Plan mode (only the commands you list in allowed_prompts \
         will auto-pass), or reject with feedback."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "Full plan in markdown format"
                },
                "allowed_prompts": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of specific commands (e.g. 'bash cargo build') that should auto-pass even if the user chooses to stay in Plan mode"
                }
            },
            "required": ["plan"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let plan = params.get("plan").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("plan is required".into()))?
            .to_string();
        let allowed_prompts: Vec<String> = params.get("allowed_prompts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let request_id = uuid::Uuid::new_v4().to_string();
        let rx = self.pending.register(request_id.clone());

        let payload = ExitPlanRequestPayload {
            request_id: request_id.clone(),
            session_id: self.session_id.clone(),
            plan,
            allowed_prompts,
        };
        let _ = self.app_handle.emit("agent:exit_plan_request", &payload);

        let result = rx.await.map_err(|_| {
            ToolError::Execution("exit_plan_mode channel dropped — user closed without deciding".into())
        })?;

        match result.decision {
            crate::app::ExitPlanDecision::AcceptAndAuto => Ok(ToolOutput::success(
                "Plan accepted; safety mode switched to Supervised. Proceeding with execution.",
                start.elapsed().as_millis() as u64,
            )),
            crate::app::ExitPlanDecision::AcceptKeepPlan => Ok(ToolOutput::success(
                "Plan accepted; staying in Plan mode. The allowed_prompts you declared are now session-scoped allow rules.",
                start.elapsed().as_millis() as u64,
            )),
            crate::app::ExitPlanDecision::Reject { feedback } => Err(ToolError::Execution(
                format!("User rejected the plan. Feedback: {}", feedback),
            )),
        }
    }
}
```

- [ ] **Step 6.4: Add respond_exit_plan_mode Tauri command**

Edit `src-tauri/src/tauri_commands.rs`. Append:

```rust
#[tauri::command]
pub async fn respond_exit_plan_mode(
    state: State<'_, AppState>,
    input: crate::ipc::RespondExitPlanInput,
) -> Result<(), Error> {
    use crate::app::{ExitPlanDecision, ExitPlanResult};
    use crate::ipc::CreatePermissionRuleInput;

    let decision = match input.decision.as_str() {
        "accept_and_auto" => {
            // Switch session SafetyMode to Supervised globally for now (per-
            // session override would be cleaner but requires plumbing through
            // the dispatcher at runtime). Updating the global policy is the
            // simplest implementation that meets the spec acceptance criteria.
            let mut mgr = state.safety_manager.write().await;
            let _ = mgr.set_global_mode(crate::safety::SafetyMode::Supervised);
            ExitPlanDecision::AcceptAndAuto
        }
        "accept_keep_plan" => {
            // Write each allowed_prompt as a V14 session pattern rule so it
            // auto-passes while user stays in Plan mode.
            for prompt in &input.allowed_prompts {
                let trimmed = prompt.trim();
                if trimmed.is_empty() { continue; }
                // Parse "bash cargo build" → tool="bash", target="cargo build"
                let (tool_name, target) = match trimmed.split_once(' ') {
                    Some((t, rest)) if !t.is_empty() => (t.to_string(), Some(rest.trim().to_string())),
                    _ => (trimmed.to_string(), None),
                };
                let _ = crate::safety::permissions::create_rule(&state.db, CreatePermissionRuleInput {
                    scope: "session".into(),
                    session_id: Some(input.session_id.clone()),
                    tool_name,
                    target,
                    mode: "allow".into(),
                });
            }
            ExitPlanDecision::AcceptKeepPlan
        }
        "reject" => ExitPlanDecision::Reject {
            feedback: input.feedback.unwrap_or_else(|| "(no feedback provided)".into()),
        },
        other => return Err(Error::InvalidInput(format!("unknown decision: {}", other))),
    };

    let resolved = state.pending_exit_plans.resolve(&input.request_id, ExitPlanResult { decision });
    if !resolved {
        tracing::warn!(request_id = %input.request_id, "respond_exit_plan_mode: no matching pending request");
    }
    Ok(())
}
```

- [ ] **Step 6.5: Register module + tool + command**

Edit `src-tauri/src/agent/tools/builtin/mod.rs`. Add:

```rust
pub mod exit_plan_mode;
```

Edit `src-tauri/src/main.rs`. After the `AskUserTool` registration (Task 5.4), add:

```rust
                                    reg.register(builtin::exit_plan_mode::ExitPlanModeTool::new(
                                        app_h.clone(),
                                        std::sync::Arc::clone(&pending_exit_plans),
                                        session_id_for_tools.clone(),
                                    ));
```

(Same `pending_exit_plans` lookup pattern as `pending_ask_users` — grab from `AppState`.)

Register in `invoke_handler!`:

```rust
            uclaw_core::tauri_commands::respond_exit_plan_mode,
```

- [ ] **Step 6.6: Tests**

Append to `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ExitPlanDecision, ExitPlanResult, PendingExitPlans};
    use std::sync::Arc;

    #[tokio::test]
    async fn pending_exit_plans_round_trip_accept_and_auto() {
        let pending = Arc::new(PendingExitPlans::new());
        let rx = pending.register("req-1".into());
        let resolved = pending.resolve("req-1", ExitPlanResult {
            decision: ExitPlanDecision::AcceptAndAuto,
        });
        assert!(resolved);
        let r = rx.await.unwrap();
        assert!(matches!(r.decision, ExitPlanDecision::AcceptAndAuto));
    }

    #[tokio::test]
    async fn pending_exit_plans_round_trip_reject_with_feedback() {
        let pending = Arc::new(PendingExitPlans::new());
        let rx = pending.register("req-2".into());
        pending.resolve("req-2", ExitPlanResult {
            decision: ExitPlanDecision::Reject { feedback: "missing test plan".into() },
        });
        let r = rx.await.unwrap();
        match r.decision {
            ExitPlanDecision::Reject { feedback } => assert_eq!(feedback, "missing test plan"),
            _ => panic!("expected Reject"),
        }
    }
}
```

Run:

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::exit_plan_mode 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 6.7: Commit**

```bash
git add src-tauri/src/app.rs src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/agent/tools/builtin/
git commit -m "$(cat <<'EOF'
feat(agent): exit_plan_mode tool + 3-decision modal flow

Agent in Plan mode calls exit_plan_mode({ plan, allowed_prompts? })
to declare "plan ready, awaiting user". Backend registers a oneshot
in app.pending_exit_plans and emits `agent:exit_plan_request` IPC.

User decision via respond_exit_plan_mode IPC routes to one of:
  - accept_and_auto: SafetyManager.set_global_mode(Supervised) +
    tool returns success → agent proceeds with execution
  - accept_keep_plan: each allowed_prompts entry parsed as
    "<tool> <target>" and written as a V14 session pattern rule
    (scope='session', session_id=current, tool_name=tool, target=target,
    mode='allow') so e.g. `bash cargo test` auto-passes while user
    stays in Plan mode. Tool returns success.
  - reject: tool returns ToolError::Execution with the user's feedback,
    which the agent sees in the next turn and can use to replan.

Frontend wiring (mount ExitPlanModeBanner + IPC listener) lands in
the frontend tasks.
EOF
)"
```

---

## Task 7: Frontend — 5-mode dropdown UI

**Files:**
- Modify: `ui/src/atoms/safety-atoms.ts` (`SafetyModeWire` expansion)
- Modify: `ui/src/lib/tauri-bridge.ts:818` (`SafetyModeWire` expansion)
- Create: `ui/src/components/agent/PermissionModeMenu.tsx` (dropdown content)
- Modify: `ui/src/components/agent/PermissionModeSelector.tsx` (becomes the trigger only)

- [ ] **Step 7.1: Expand SafetyModeWire type**

Edit `ui/src/lib/tauri-bridge.ts`. Find around line 818 (the `SafetyModeWire` definition). Replace:

```ts
export type SafetyModeWire = 'ask' | 'acceptedits' | 'plan' | 'supervised' | 'yolo'
```

- [ ] **Step 7.2: Update safety-atoms default**

`ui/src/atoms/safety-atoms.ts` should already use the type from tauri-bridge, so no change needed. Verify by reading the file:

```bash
grep -n "safetyModeAtom\|SafetyModeWire" ui/src/atoms/safety-atoms.ts
```

Default value remains `'supervised'`.

- [ ] **Step 7.3: Create PermissionModeMenu**

Create `ui/src/components/agent/PermissionModeMenu.tsx`:

```tsx
/**
 * PermissionModeMenu — Radix Popover dropdown matching Claude Code's
 * 5-mode selector. Listens for keyboard shortcuts:
 *   - Shift+Cmd+M (Mac) / Shift+Ctrl+M (Win/Linux) — open
 *   - 1-5 (when open) — select corresponding mode
 *   - Esc — close
 *
 * The popover content is owned by this component; the trigger button
 * (with the current-mode label + chevron) is rendered by PermissionModeSelector.
 */

import * as React from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ShieldQuestion, Pencil, Map as MapIcon, Compass, Zap, Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SafetyModeWire } from '@/lib/tauri-bridge'

export interface ModeMenuItem {
  wire: SafetyModeWire
  label: string
  icon: React.ComponentType<{ className?: string }>
  numberKey: '1' | '2' | '3' | '4' | '5'
  triggerColorClass: string  // applied to the trigger button when this is current
}

export const MODE_ITEMS: ModeMenuItem[] = [
  { wire: 'ask',         label: 'Ask permissions',   icon: ShieldQuestion, numberKey: '1', triggerColorClass: 'text-yellow-600' },
  { wire: 'acceptedits', label: 'Accept edits',      icon: Pencil,         numberKey: '2', triggerColorClass: 'text-blue-600' },
  { wire: 'plan',        label: 'Plan mode',         icon: MapIcon,        numberKey: '3', triggerColorClass: 'text-purple-600' },
  { wire: 'supervised',  label: 'Auto mode',         icon: Compass,        numberKey: '4', triggerColorClass: 'text-foreground/70' },
  { wire: 'yolo',        label: 'Bypass permissions',icon: Zap,            numberKey: '5', triggerColorClass: 'text-amber-600' },
]

export interface PermissionModeMenuProps {
  current: SafetyModeWire
  onPick: (mode: SafetyModeWire) => void
  open: boolean
  onOpenChange: (open: boolean) => void
  trigger: React.ReactNode
}

export function PermissionModeMenu({ current, onPick, open, onOpenChange, trigger }: PermissionModeMenuProps): React.ReactElement {
  // Keyboard handler when open
  React.useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      const item = MODE_ITEMS.find((m) => m.numberKey === e.key)
      if (item) {
        e.preventDefault()
        onPick(item.wire)
        onOpenChange(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onPick, onOpenChange])

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-[280px] p-1">
        <div className="flex items-center justify-between px-2 py-1.5 border-b border-border/50 mb-1">
          <span className="text-[11px] font-medium text-muted-foreground/70">Mode</span>
          <span className="flex items-center gap-1">
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">⇧</kbd>
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">⌘</kbd>
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">M</kbd>
          </span>
        </div>
        <ul role="menu" className="space-y-px">
          {MODE_ITEMS.map((m) => {
            const Icon = m.icon
            const active = m.wire === current
            return (
              <li key={m.wire}>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => { onPick(m.wire); onOpenChange(false) }}
                  className={cn(
                    'flex w-full items-center gap-2 px-2 py-1.5 rounded text-[12.5px] hover:bg-muted',
                    active && 'bg-muted/60'
                  )}
                >
                  <Icon className={cn('size-3.5 shrink-0', m.triggerColorClass)} />
                  <span className="flex-1 text-left">{m.label}</span>
                  {active && <Check className="size-3.5 text-foreground/70 mr-1" />}
                  <span className="text-[10.5px] text-muted-foreground/60 tabular-nums w-3 text-right">
                    {m.numberKey}
                  </span>
                </button>
              </li>
            )
          })}
        </ul>
      </PopoverContent>
    </Popover>
  )
}
```

- [ ] **Step 7.4: Rewrite PermissionModeSelector to use the menu**

Replace the body of `ui/src/components/agent/PermissionModeSelector.tsx`. Read the current file first:

```bash
sed -n '1,30p' ui/src/components/agent/PermissionModeSelector.tsx
```

Replace its full content with:

```tsx
/**
 * PermissionModeSelector — input-bar trigger button that opens the
 * 5-mode PermissionModeMenu popover. Backed by the real SafetyManager
 * (PR #42 wired this — see tauri-bridge.ts::setSafetyMode).
 *
 * Keyboard: Shift+Cmd+M (Mac) / Shift+Ctrl+M (other) opens the menu.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { getSafetyPolicy, setSafetyMode, type SafetyModeWire } from '@/lib/tauri-bridge'
import { PermissionModeMenu, MODE_ITEMS } from './PermissionModeMenu'

export interface PermissionModeSelectorProps {
  /** Kept for prop compat — global SafetyMode is workspace-agnostic. */
  sessionId?: string
}

export function PermissionModeSelector(_: PermissionModeSelectorProps): React.ReactElement | null {
  const [mode, setMode] = useAtom(safetyModeAtom)
  const [open, setOpen] = React.useState(false)
  const [busy, setBusy] = React.useState(false)

  // Hydrate from backend on mount.
  React.useEffect(() => {
    getSafetyPolicy()
      .then((p) => setMode(p.globalMode as SafetyModeWire))
      .catch((e) => console.error('[PermissionModeSelector] getSafetyPolicy failed:', e))
    // eslint-disable-next-line react-hooks/exhaustive-deps -- run once
  }, [])

  // Global keyboard shortcut: Shift+Cmd+M opens the menu.
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.shiftKey && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'm') {
        e.preventDefault()
        setOpen((v) => !v)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const onPick = React.useCallback(async (next: SafetyModeWire) => {
    if (busy) return
    setBusy(true)
    try {
      await setSafetyMode({ mode: next })
      setMode(next)
    } catch (err) {
      console.error('[PermissionModeSelector] setSafetyMode failed:', err)
    } finally {
      setBusy(false)
      requestAnimationFrame(() => document.querySelector<HTMLElement>('.ProseMirror')?.focus())
    }
  }, [busy, setMode])

  const current = MODE_ITEMS.find((m) => m.wire === mode) ?? MODE_ITEMS[3]!  // default to Auto
  const Icon = current.icon
  const isNonDefault = current.wire !== 'supervised'

  const trigger = (
    <button
      type="button"
      disabled={busy}
      className={`flex items-center gap-1 px-1.5 py-1 rounded text-xs font-medium transition-colors hover:text-foreground disabled:opacity-50 ${
        isNonDefault ? current.triggerColorClass : 'text-muted-foreground'
      }`}
    >
      <Icon className="size-3.5" />
      <span className="hidden sm:inline">{current.label}</span>
      <span className="text-[10px] opacity-60">▾</span>
    </button>
  )

  return (
    <PermissionModeMenu
      current={mode}
      onPick={(m) => void onPick(m)}
      open={open}
      onOpenChange={setOpen}
      trigger={trigger}
    />
  )
}
```

- [ ] **Step 7.5: TS check + commit**

```bash
(cd ui && npx tsc --noEmit && echo "tsc clean")
```

Expected: clean.

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/components/agent/PermissionModeSelector.tsx ui/src/components/agent/PermissionModeMenu.tsx
git commit -m "$(cat <<'EOF'
feat(ui): 5-mode permission dropdown (Claude Code style)

Replaces the 2-mode cycle button with a Radix Popover dropdown listing
all 5 SafetyMode variants. Each row has an icon + label + number key
1-5 hint; current mode shows checkmark; trigger button colors itself
to indicate non-default modes (Plan/Bypass/AcceptEdits/Ask all stand out).

Keyboard:
  - Shift+Cmd+M (Mac) / Shift+Ctrl+M (other) → open menu
  - 1-5 (when open) → pick corresponding mode
  - Esc → close

Persistence unchanged: clicking a mode calls set_safety_mode IPC →
SafetyManager writes safety_policy.json → atom mirrors backend.
EOF
)"
```

---

## Task 8: Frontend — Plan/AcceptEdits banner

**Files:**
- Create: `ui/src/components/agent/ModeBanner.tsx`
- Modify: `ui/src/components/app-shell/AppShell.tsx` (mount the banner)

- [ ] **Step 8.1: Create ModeBanner**

Create `ui/src/components/agent/ModeBanner.tsx`:

```tsx
/**
 * ModeBanner — inline pill at session top, only shown for Plan / AcceptEdits.
 * Other modes (Ask / Auto / Bypass) are self-evident from agent behavior or
 * obvious from the user's deliberate choice.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Pencil, Map as MapIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { safetyModeAtom } from '@/atoms/safety-atoms'

export function ModeBanner(): React.ReactElement | null {
  const mode = useAtomValue(safetyModeAtom)

  if (mode === 'acceptedits') {
    return (
      <div className={cn(
        'flex items-center gap-2 px-3 py-1.5 text-[12px] border-b',
        'border-blue-500/30 bg-blue-500/8 text-blue-700 dark:text-blue-400'
      )}>
        <Pencil className="size-3.5 shrink-0" />
        <span>Accept edits — file edits auto-pass; other tools ask</span>
      </div>
    )
  }

  if (mode === 'plan') {
    return (
      <div className={cn(
        'flex items-center gap-2 px-3 py-1.5 text-[12px] border-b',
        'border-purple-500/30 bg-purple-500/8 text-purple-700 dark:text-purple-400'
      )}>
        <MapIcon className="size-3.5 shrink-0" />
        <span>Plan mode — investigating only, no execution</span>
      </div>
    )
  }

  return null
}
```

- [ ] **Step 8.2: Mount in AppShell**

Edit `ui/src/components/app-shell/AppShell.tsx`. Add import near other agent component imports:

```tsx
import { ModeBanner } from '@/components/agent/ModeBanner'
```

Find the `<MainArea />` line (Task 5 spec says it's around line 117). Insert `<ModeBanner />` immediately above the MainArea:

```tsx
            <ModeBanner />
            <MainArea />
```

- [ ] **Step 8.3: TS check + commit**

```bash
(cd ui && npx tsc --noEmit && echo "tsc clean")
git add ui/src/components/agent/ModeBanner.tsx ui/src/components/app-shell/AppShell.tsx
git commit -m "feat(ui): Plan/AcceptEdits mode banner above MainArea"
```

---

## Task 9: Frontend — wire ask_user banner

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts` (drop silent `.catch()` on respondAskUser; add IPC listener helper)
- Modify: `ui/src/atoms/agent-atoms.ts` (init listener that populates `allPendingAskUserRequestsAtom` from IPC events)
- Modify: `ui/src/components/app-shell/AppShell.tsx` (mount `<AskUserBanner />` if not already)
- Modify: `ui/src/components/agent/AskUserBanner.tsx` (verify props)

- [ ] **Step 9.1: Drop silent catch + add listener helper**

Edit `ui/src/lib/tauri-bridge.ts`. Find `respondAskUser` (around line 871):

```ts
export const respondAskUser = (input: any): Promise<void> =>
  invoke<void>('respond_ask_user', { input }).catch(() => {})
```

Replace with:

```ts
import type { AskUserRequest, ExitPlanModeRequest } from './agent-types'

export const respondAskUser = (input: { requestId: string; answers: Record<string, string> }): Promise<void> =>
  invoke<void>('respond_ask_user', { input })

export const onAskUserRequest = (cb: (payload: AskUserRequest) => void): Promise<UnlistenFn> =>
  listen('agent:ask_user_request', (e) => cb(e.payload as AskUserRequest))
```

(Drop the import-`any` for input — replace with the typed shape that matches `RespondAskUserInput` on the Rust side.)

- [ ] **Step 9.2: Wire IPC listener into atoms**

Edit `ui/src/atoms/agent-atoms.ts`. Find `allPendingAskUserRequestsAtom` (around line 283). Below the related setter atom, add:

```ts
/**
 * Initialize the IPC listener that populates `allPendingAskUserRequestsAtom`
 * from `agent:ask_user_request` events. Call once at app start.
 */
export async function installAskUserListener(
  setMap: (update: (prev: Map<string, readonly AskUserRequest[]>) => Map<string, readonly AskUserRequest[]>) => void,
): Promise<() => void> {
  const { onAskUserRequest } = await import('@/lib/tauri-bridge')
  return await onAskUserRequest((payload) => {
    setMap((prev) => {
      const next = new Map(prev)
      const existing = next.get(payload.sessionId) ?? []
      next.set(payload.sessionId, [...existing, payload])
      return next
    })
  })
}
```

(If a similar listener already exists for permission requests / exit_plan_mode, mirror its pattern. The hook's exact wiring depends on where global IPC listeners are installed — check `ui/src/hooks/useGlobalAgentListeners.ts` if it exists.)

- [ ] **Step 9.3: Install listener at app root**

Edit `ui/src/components/app-shell/AppShell.tsx`. Add a `useEffect` near other global-listener setup:

```tsx
React.useEffect(() => {
  let dispose: (() => void) | undefined
  installAskUserListener((updater) => setAllPendingAskUserRequests(updater)).then((d) => { dispose = d })
  return () => { dispose?.() }
}, [])
```

(Add the necessary imports: `installAskUserListener` from `'@/atoms/agent-atoms'`, `setAllPendingAskUserRequests` from the same file via `useSetAtom(allPendingAskUserRequestsAtom)`.)

- [ ] **Step 9.4: Mount AskUserBanner**

Inside the same `AppShell` component, near the existing `<ApprovalModal />` mount (added in PR #45), add:

```tsx
        {/* Global ask_user banner — shows agent's question pending */}
        <AskUserBanner />
```

(Add import `import { AskUserBanner } from '@/components/agent/AskUserBanner'`. The banner reads pending requests from the atom and self-renders only when there's a request for the current session.)

- [ ] **Step 9.5: Verify AskUserBanner can post answer**

Read the existing `ui/src/components/agent/AskUserBanner.tsx` (461 lines, Proma-leftover). Inspect how it submits answers — search for `respondAskUser` or `respond_ask_user`. If the call uses the `.catch(() => {})` silent pattern, replace with proper error handling (toast on failure):

```bash
grep -n "respondAskUser\|respond_ask_user" ui/src/components/agent/AskUserBanner.tsx | head -5
```

If the banner already calls `respondAskUser` correctly, no change. If it expects a different request shape, adapt the AskUserRequest TS type or the banner's reading code (target: minimal change, prefer to keep banner code as-is).

- [ ] **Step 9.6: TS check + commit**

```bash
(cd ui && npx tsc --noEmit && echo "tsc clean")
git add ui/src/lib/tauri-bridge.ts ui/src/atoms/agent-atoms.ts ui/src/components/app-shell/AppShell.tsx ui/src/components/agent/AskUserBanner.tsx
git commit -m "$(cat <<'EOF'
feat(ui): wire ask_user banner end-to-end

Existing AskUserBanner.tsx (Proma-leftover, 461 lines) was never
mounted and the bridge wrapper silently swallowed IPC failures —
similar to ApprovalModal pre-PR-#45.

Now:
  - tauri-bridge::respondAskUser drops the silent .catch and accepts
    a typed input matching RespondAskUserInput on the Rust side
  - tauri-bridge::onAskUserRequest IPC listener helper exposed
  - agent-atoms::installAskUserListener populates
    allPendingAskUserRequestsAtom from events
  - AppShell installs the listener once on mount + mounts <AskUserBanner />
    next to <ApprovalModal />

Result: agent calls ask_user → backend emits IPC event → atom updates →
banner renders → user picks/types → respondAskUser resolves the
backend oneshot → agent receives answer.
EOF
)"
```

---

## Task 10: Frontend — wire exit_plan_mode banner

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts` (drop silent `.catch()` on `respondExitPlanMode`; add listener)
- Modify: `ui/src/atoms/agent-atoms.ts` (mirror Task 9 listener)
- Modify: `ui/src/components/app-shell/AppShell.tsx` (mount banner + install listener)
- Modify: `ui/src/components/agent/ExitPlanModeBanner.tsx` (verify response shape includes new `decision` field)

- [ ] **Step 10.1: Update bridge wrapper + listener**

Edit `ui/src/lib/tauri-bridge.ts`. Replace the existing `respondExitPlanMode` (around line 874):

```ts
export interface RespondExitPlanModeInput {
  requestId: string
  decision: 'accept_and_auto' | 'accept_keep_plan' | 'reject'
  feedback?: string
  allowedPrompts?: string[]
  sessionId: string
}

export const respondExitPlanMode = (input: RespondExitPlanModeInput): Promise<void> =>
  invoke<void>('respond_exit_plan_mode', { input })

export const onExitPlanRequest = (cb: (payload: ExitPlanModeRequest) => void): Promise<UnlistenFn> =>
  listen('agent:exit_plan_request', (e) => cb(e.payload as ExitPlanModeRequest))
```

The TS type `ExitPlanModeRequest` already exists in `agent-types.ts:220`. Confirm its shape matches the backend `ExitPlanRequestPayload` (we have `requestId`, `sessionId`, `plan`, `allowedPrompts`). If the existing TS type is shaped differently (e.g. has `allowedPrompts: ExitPlanAllowedPrompt[]` per the Proma-leftover), adapt the type to match the backend wire format:

```ts
// In ui/src/lib/agent-types.ts — replace the existing ExitPlanModeRequest
export interface ExitPlanModeRequest {
  requestId: string
  sessionId: string
  plan: string
  allowedPrompts?: string[]
}
```

If the `ExitPlanAllowedPrompt` type becomes unused after this, delete it.

- [ ] **Step 10.2: Add listener installer**

Edit `ui/src/atoms/agent-atoms.ts`. Below `installAskUserListener`, add:

```ts
export async function installExitPlanListener(
  setMap: (update: (prev: Map<string, readonly ExitPlanModeRequest[]>) => Map<string, readonly ExitPlanModeRequest[]>) => void,
): Promise<() => void> {
  const { onExitPlanRequest } = await import('@/lib/tauri-bridge')
  return await onExitPlanRequest((payload) => {
    setMap((prev) => {
      const next = new Map(prev)
      const existing = next.get(payload.sessionId) ?? []
      next.set(payload.sessionId, [...existing, payload])
      return next
    })
  })
}
```

- [ ] **Step 10.3: Install listener + mount banner**

Edit `ui/src/components/app-shell/AppShell.tsx`. Add a parallel `useEffect`:

```tsx
React.useEffect(() => {
  let dispose: (() => void) | undefined
  installExitPlanListener((updater) => setAllPendingExitPlanRequests(updater)).then((d) => { dispose = d })
  return () => { dispose?.() }
}, [])
```

(Add `setAllPendingExitPlanRequests` via `useSetAtom(allPendingExitPlanRequestsAtom)`.)

Mount the banner near `<AskUserBanner />`:

```tsx
        {/* Plan mode confirmation banner — render plan + 3-decision modal */}
        <ExitPlanModeBanner />
```

Add import: `import { ExitPlanModeBanner } from '@/components/agent/ExitPlanModeBanner'`.

- [ ] **Step 10.4: Update ExitPlanModeBanner to use 3 decisions**

Read the existing `ui/src/components/agent/ExitPlanModeBanner.tsx` (334 lines). Find where it submits the response — search for `respondExitPlanMode` / `respond_exit_plan_mode`:

```bash
grep -n "respondExitPlan\|respond_exit_plan\|decision" ui/src/components/agent/ExitPlanModeBanner.tsx | head -10
```

If the existing banner has only 2 buttons (accept/reject) or different decision semantics, refactor to call `respondExitPlanMode({ requestId, sessionId, decision, feedback?, allowedPrompts? })` with the 3-decision pattern from Task 6:

```tsx
async function handleAccept(autoMode: boolean) {
  await respondExitPlanMode({
    requestId: req.requestId,
    sessionId: req.sessionId,
    decision: autoMode ? 'accept_and_auto' : 'accept_keep_plan',
    allowedPrompts: req.allowedPrompts ?? [],
  })
}

async function handleReject(feedback: string) {
  await respondExitPlanMode({
    requestId: req.requestId,
    sessionId: req.sessionId,
    decision: 'reject',
    feedback,
    allowedPrompts: [],
  })
}
```

The 3 buttons:
- 接受 + 切到 Auto 执行 → `handleAccept(true)`
- 接受 + 留 plan → `handleAccept(false)` (only render if `allowedPrompts.length > 0`)
- 拒绝并反馈 → opens textarea → `handleReject(feedback)`

If the existing banner doesn't fit this shape (e.g. it was Proma-rendering a different schema), replace its render logic with a clean 3-decision version. Aim for ~150-200 lines after the rewrite.

- [ ] **Step 10.5: TS check + commit**

```bash
(cd ui && npx tsc --noEmit 2>&1 | head -20)
```

Expected: clean.

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/lib/agent-types.ts ui/src/atoms/agent-atoms.ts ui/src/components/app-shell/AppShell.tsx ui/src/components/agent/ExitPlanModeBanner.tsx
git commit -m "$(cat <<'EOF'
feat(ui): wire exit_plan_mode banner with 3-decision flow

Existing ExitPlanModeBanner.tsx (Proma-leftover, 334 lines) was never
mounted; backend emits agent:exit_plan_request → atom updates →
banner renders the plan markdown + 3-decision UI:

  - 接受 + 切到 Auto 执行 → respondExitPlanMode decision='accept_and_auto'
  - 接受 + 留 plan (visible if allowed_prompts non-empty) →
    decision='accept_keep_plan'
  - 拒绝并反馈 (textarea) → decision='reject', feedback=...

allowedPrompts the agent declared when calling exit_plan_mode are
echoed back to the backend so respond_exit_plan_mode can write them
as V14 session pattern rules (only on accept_keep_plan).
EOF
)"
```

---

## Task 11: Frontend — Settings → 提示词 tab

**Files:**
- Create: `ui/src/components/settings/PromptsSettings.tsx`
- Modify: `ui/src/atoms/settings-tab.ts` (add `'prompts'` variant)
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (nav entry + content branch)
- Modify: `ui/src/lib/tauri-bridge.ts` (3 new wrappers)
- Modify: `ui/src/lib/types.ts` (DefaultPromptsResponse type)

- [ ] **Step 11.1: Add types + bridge wrappers**

Edit `ui/src/lib/types.ts`. Append:

```ts
// ===== Prompts =====

export interface DefaultPromptsResponse {
  baseline: string
  modeAsk: string
  modeAcceptEdits: string
  modePlan: string
  modeBypass: string
}
```

Edit `ui/src/lib/tauri-bridge.ts`. Append:

```ts
import type { DefaultPromptsResponse } from './types'

export const readWorkspaceUclawMd = (): Promise<string> =>
  invoke<string>('read_workspace_uclaw_md')

export const writeWorkspaceUclawMd = (content: string): Promise<void> =>
  invoke<void>('write_workspace_uclaw_md', { content })

export const readDefaultPrompts = (): Promise<DefaultPromptsResponse> =>
  invoke<DefaultPromptsResponse>('read_default_prompts')
```

- [ ] **Step 11.2: Add 'prompts' to SettingsTab**

Edit `ui/src/atoms/settings-tab.ts`. Find the `SettingsTab` union (it's a union type). Add `'prompts'`:

```ts
export type SettingsTab = 'general' | 'channels' | 'models' | 'appearance' | 'usage'
                       | 'permissions' | 'prompts' | 'agent' | 'tools' | 'bots'
                       | 'shortcuts' | 'proxy' | 'about'
```

(Adapt to whatever order/values are already there — just add `'prompts'`.)

- [ ] **Step 11.3: Create PromptsSettings component**

Create `ui/src/components/settings/PromptsSettings.tsx`:

```tsx
/**
 * PromptsSettings — Settings → 提示词 tab.
 *
 * Three sections:
 *   1. Global system prompt (link to existing 通用 tab — don't duplicate)
 *   2. uclaw.md (workspace-level, editable textarea + 保存 + 外部编辑器)
 *   3. uClaw 内置行为护栏 (read-only collapsible: Karpathy baseline +
 *      current mode addition for transparency)
 */

import * as React from 'react'
import { Save, ExternalLink, FileCode2, ChevronDown, ChevronRight } from 'lucide-react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  readWorkspaceUclawMd,
  writeWorkspaceUclawMd,
  readDefaultPrompts,
} from '@/lib/tauri-bridge'
import type { DefaultPromptsResponse } from '@/lib/types'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { settingsTabAtom } from '@/atoms/settings-tab'

const PLACEHOLDER_TEMPLATE = `# uClaw — <project name>

<!-- 这个文件描述当前项目的上下文。uClaw agent 在每次对话时都会
     读取它，作为 "项目说明" 注入到系统提示词。
     文件位置：<workspace>/uclaw.md
     编辑后保存即生效。 -->

## 项目约定

- 

## Do

- 

## Don't

- 

## 常用命令 / 路径

- 
`

export function PromptsSettings(): React.ReactElement {
  const [content, setContent] = React.useState('')
  const [pristine, setPristine] = React.useState('')
  const [defaults, setDefaults] = React.useState<DefaultPromptsResponse | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)
  const [showGuardrails, setShowGuardrails] = React.useState(false)
  const mode = useAtomValue(safetyModeAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  React.useEffect(() => {
    Promise.all([readWorkspaceUclawMd(), readDefaultPrompts()])
      .then(([md, p]) => {
        setContent(md)
        setPristine(md)
        setDefaults(p)
      })
      .catch((e) => {
        console.error('[PromptsSettings] load failed:', e)
        toast.error('加载提示词失败')
      })
      .finally(() => setLoading(false))
  }, [])

  const dirty = content !== pristine

  const onSave = async () => {
    setSaving(true)
    try {
      await writeWorkspaceUclawMd(content)
      setPristine(content)
      toast.success('uclaw.md 已保存')
    } catch (e) {
      console.error('[PromptsSettings] save failed:', e)
      toast.error('保存失败')
    } finally {
      setSaving(false)
    }
  }

  const currentModeAddition = React.useMemo(() => {
    if (!defaults) return ''
    switch (mode) {
      case 'ask': return defaults.modeAsk
      case 'acceptedits': return defaults.modeAcceptEdits
      case 'plan': return defaults.modePlan
      case 'yolo': return defaults.modeBypass
      default: return '(Auto mode — no mode-specific addition)'
    }
  }, [mode, defaults])

  return (
    <div className="space-y-6 pb-8">
      {/* Section 1: link to existing global system prompt tab */}
      <section>
        <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          全局系统提示词
        </h3>
        <Button variant="outline" size="sm" onClick={() => setSettingsTab('general')}>
          跳到 通用 tab 编辑
        </Button>
      </section>

      {/* Section 2: uclaw.md textarea */}
      <section>
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            项目说明 (uclaw.md)
          </h3>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost" size="sm"
              onClick={() => toast.info('请使用文件管理器打开 <workspace>/uclaw.md')}
            >
              <ExternalLink className="size-3.5 mr-1" />
              在外部编辑器打开
            </Button>
            <Button
              size="sm"
              onClick={() => void onSave()}
              disabled={!dirty || saving}
            >
              <Save className="size-3.5 mr-1" />
              {saving ? '保存中…' : '保存'}
            </Button>
          </div>
        </div>
        <textarea
          value={loading ? '加载中…' : (content || PLACEHOLDER_TEMPLATE)}
          onChange={(e) => setContent(e.target.value)}
          disabled={loading}
          spellCheck={false}
          className={cn(
            'w-full min-h-[280px] font-mono text-[12.5px] p-3',
            'bg-background border border-border/50 rounded',
            'focus:outline-none focus:border-border',
          )}
        />
        <p className="mt-1 text-[11px] text-muted-foreground/60">
          路径：<code className="font-mono">&lt;workspace&gt;/uclaw.md</code>
          {dirty && <span className="ml-2 text-amber-600">• 未保存</span>}
        </p>
      </section>

      {/* Section 3: read-only guardrails preview */}
      <section>
        <button
          type="button"
          onClick={() => setShowGuardrails((v) => !v)}
          className="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70 hover:text-foreground"
        >
          {showGuardrails ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          uClaw 内置行为护栏 (只读)
        </button>
        {showGuardrails && defaults && (
          <div className="mt-2 space-y-3">
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> baseline.md (Karpathy guardrails)
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {defaults.baseline}
              </pre>
            </div>
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> 当前模式 ({mode}) 的特化提示词
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {currentModeAddition || '(empty)'}
              </pre>
            </div>
          </div>
        )}
      </section>
    </div>
  )
}
```

- [ ] **Step 11.4: Add nav entry to SettingsPanel**

Edit `ui/src/components/settings/SettingsPanel.tsx`. Find the `TABS` array (around line 40). Add (between `permissions` and `agent` or wherever fits the existing order):

```tsx
{ id: 'prompts', label: '提示词', icon: <FileCode2 size={15} /> },
```

Add `FileCode2` to the lucide import.

Find the `SettingsContent` switch. Add:

```tsx
case 'prompts':
  return <PromptsSettings />
```

Add the import:

```tsx
import { PromptsSettings } from './PromptsSettings'
```

- [ ] **Step 11.5: TS check + commit**

```bash
(cd ui && npx tsc --noEmit && echo "tsc clean")
git add ui/src/atoms/settings-tab.ts ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts ui/src/components/settings/SettingsPanel.tsx ui/src/components/settings/PromptsSettings.tsx
git commit -m "$(cat <<'EOF'
feat(ui): Settings → 提示词 tab

Three sections:
  1. Link to existing 全局系统提示词 (通用 tab)
  2. <workspace>/uclaw.md textarea editor + 保存 + 外部编辑器 button.
     Saves via write_workspace_uclaw_md Tauri command. Loads via
     read_workspace_uclaw_md (returns "" if file doesn't exist; we show
     a placeholder template in that case but don't write it until
     user explicitly saves).
  3. Read-only collapsible "内置行为护栏" showing the Karpathy
     baseline + current mode's prompt addition (sourced from
     read_default_prompts). Lets users see exactly what's being
     injected on top of their config without exposing edit access
     (those prompts are part of uClaw's behavior contract, shipped
     with the binary).
EOF
)"
```

---

## Task 12: Frontend — tests

**Files:**
- Create: `ui/src/components/agent/PermissionModeMenu.test.tsx`
- Create: `ui/src/components/agent/ModeBanner.test.tsx`
- Create: `ui/src/components/settings/PromptsSettings.test.tsx`

- [ ] **Step 12.1: PermissionModeMenu test**

Create `ui/src/components/agent/PermissionModeMenu.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { PermissionModeMenu, MODE_ITEMS } from './PermissionModeMenu'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

describe('PermissionModeMenu', () => {
  it('renders 5 modes with their number keys when open', async () => {
    const onPick = vi.fn()
    const onOpenChange = vi.fn()
    renderWithProviders(
      <PermissionModeMenu
        current="supervised"
        onPick={onPick}
        open={true}
        onOpenChange={onOpenChange}
        trigger={<button>trigger</button>}
      />
    )
    for (const m of MODE_ITEMS) {
      expect(await screen.findByText(m.label)).toBeInTheDocument()
      expect(screen.getByText(m.numberKey)).toBeInTheDocument()
    }
  })

  it('keyboard 1-5 selects corresponding mode and closes', async () => {
    const onPick = vi.fn()
    const onOpenChange = vi.fn()
    renderWithProviders(
      <PermissionModeMenu
        current="supervised"
        onPick={onPick}
        open={true}
        onOpenChange={onOpenChange}
        trigger={<button>trigger</button>}
      />
    )
    // press '3' → Plan
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }))
    await waitFor(() => expect(onPick).toHaveBeenCalledWith('plan'))
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })

  it('shows checkmark on current mode', async () => {
    renderWithProviders(
      <PermissionModeMenu
        current="plan"
        onPick={() => {}}
        open={true}
        onOpenChange={() => {}}
        trigger={<button>trigger</button>}
      />
    )
    const planRow = (await screen.findByText('Plan mode')).closest('button')!
    expect(planRow.querySelector('svg.lucide-check')).not.toBeNull()
  })
})
```

- [ ] **Step 12.2: ModeBanner test**

Create `ui/src/components/agent/ModeBanner.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import * as React from 'react'
import { ModeBanner } from './ModeBanner'
import { renderWithProviders, screen } from '@/test-utils/render'
import { safetyModeAtom } from '@/atoms/safety-atoms'

describe('ModeBanner', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('renders nothing in Auto mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'supervised')
    expect(container.textContent).toBe('')
  })

  it('renders nothing in Ask mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'ask')
    expect(container.textContent).toBe('')
  })

  it('renders nothing in Bypass mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'yolo')
    expect(container.textContent).toBe('')
  })

  it('renders the Plan mode banner in plan', () => {
    const { store } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'plan')
    expect(screen.getByText(/Plan mode — investigating only/i)).toBeInTheDocument()
  })

  it('renders the Accept edits banner in acceptedits', () => {
    const { store } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'acceptedits')
    expect(screen.getByText(/Accept edits — file edits auto-pass/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 12.3: PromptsSettings test**

Create `ui/src/components/settings/PromptsSettings.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { PromptsSettings } from './PromptsSettings'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  readWorkspaceUclawMd: vi.fn(async () => '# my project\nuse rust 2021'),
  writeWorkspaceUclawMd: vi.fn(async () => {}),
  readDefaultPrompts: vi.fn(async () => ({
    baseline: 'BASELINE_TEXT',
    modeAsk: 'ASK_TEXT',
    modeAcceptEdits: 'ACCEPT_EDITS_TEXT',
    modePlan: 'PLAN_TEXT',
    modeBypass: 'BYPASS_TEXT',
  })),
}))

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

describe('PromptsSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('loads existing uclaw.md into the textarea', async () => {
    renderWithProviders(<PromptsSettings />)
    await waitFor(() => {
      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement
      expect(textarea.value).toContain('# my project')
    })
  })

  it('Save button calls writeWorkspaceUclawMd with edited content', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    const { user } = renderWithProviders(<PromptsSettings />)
    await waitFor(() => screen.getByRole('textbox'))
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement
    // Replace content
    await user.clear(textarea)
    await user.type(textarea, '# edited content')
    const save = screen.getByRole('button', { name: /保存/ })
    await user.click(save)
    await waitFor(() => {
      expect(bridge.writeWorkspaceUclawMd).toHaveBeenCalledWith('# edited content')
    })
  })

  it('expanding 内置行为护栏 shows baseline + mode prompt', async () => {
    const { user } = renderWithProviders(<PromptsSettings />)
    await waitFor(() => screen.getByText(/内置行为护栏/i))
    const toggle = screen.getByText(/内置行为护栏/i)
    await user.click(toggle)
    await waitFor(() => {
      expect(screen.getByText('BASELINE_TEXT')).toBeInTheDocument()
    })
  })
})
```

- [ ] **Step 12.4: Run + commit**

```bash
(cd ui && npx vitest run PermissionModeMenu ModeBanner PromptsSettings 2>&1 | tail -15)
```

Expected: 11 passing (3 + 5 + 3).

```bash
git add ui/src/components/agent/PermissionModeMenu.test.tsx ui/src/components/agent/ModeBanner.test.tsx ui/src/components/settings/PromptsSettings.test.tsx
git commit -m "test(ui): PermissionModeMenu + ModeBanner + PromptsSettings"
```

---

## Task 13: Final verification + push + PR

- [ ] **Step 13.1: Full pipeline check**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit && echo "tsc clean")
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
echo "=== vite ===" && (cd ui && npx vite build 2>&1 | tail -3)
```

Expected:
- cargo: clean
- rust tests: ~210 passing (was 195, +15 new across 5 task additions)
- tsc: clean
- frontend tests: ~61 passing (was 50, +11 new from Task 12)
- vite build: succeeds

- [ ] **Step 13.2: Manual smoke checklist**

```bash
cd src-tauri && cargo tauri dev
```

Open the app and verify each acceptance criterion from the spec:

- [ ] `Shift+Cmd+M` opens the dropdown
- [ ] Pressing `1`-`5` selects the corresponding mode
- [ ] Switch to Plan mode → banner appears at session top in purple
- [ ] Switch to Accept edits → banner appears in blue
- [ ] Switch back to Auto → banner disappears
- [ ] In Plan mode, `bash echo hi > /tmp/x` → blocked with informative error
- [ ] In Plan mode, `bash cargo build` → blocked
- [ ] In Plan mode, `read_file foo.rs` → auto-passes
- [ ] Trigger `ask_user` (e.g. ask agent to confirm a choice) → banner appears, answer it, agent receives response
- [ ] Trigger `exit_plan_mode` (in Plan mode, ask agent to plan a feature) → modal shows; clicking 接受+Auto switches to Auto mode + agent proceeds
- [ ] Settings → 提示词 → uclaw.md textarea loads + saves
- [ ] Verify saved uclaw.md appears in agent's effective prompt by checking debug logs (or just observe agent referencing project conventions)

- [ ] **Step 13.3: Push + PR**

```bash
git push -u origin claude/permission-modes-redesign
gh pr create --title "Permission mode system redesign — 5 modes + uclaw.md + ask_user/exit_plan_mode" --body "$(cat <<'EOF'
## Summary

Implements the design from PR #47 spec doc: extends the permission mode selector from 2 modes to 5 (Ask / Accept edits / Plan / Auto / Bypass), adds two agent↔user communication tools (\`ask_user\`, \`exit_plan_mode\`), and introduces a 4-layer system prompt model with Karpathy-flavored behavioral guardrails plus workspace-level \`uclaw.md\`.

## Changes

| Layer | Change |
|---|---|
| **DB** | None — zero migrations |
| **Backend** | 2 new \`SafetyMode\` variants (AcceptEdits, Plan); resolver behavior table extended; mode_prompts module with 5 prompt md files (compile-time include_str!); 2 new built-in tools (ask_user, exit_plan_mode); 2 new pending registries (PendingAskUsers, PendingExitPlans); 5 new Tauri commands |
| **Frontend** | 5-mode Radix Popover dropdown w/ Shift+Cmd+M shortcut + 1-5 keys; ModeBanner for Plan/AcceptEdits; AskUserBanner + ExitPlanModeBanner finally mounted (Proma-leftover, never wired); Settings → 提示词 tab with uclaw.md textarea + read-only baseline preview |
| **System prompt** | 4 layers composed: user global + uclaw.md + Karpathy baseline + mode-specific addition (joined with --- separators; empty layers skipped) |

## Acceptance criteria

(From spec doc)

- [x] 5-mode dropdown opens via Shift+Cmd+M, selects via 1-5
- [x] All 5 SafetyMode variants persist round-trip
- [x] Plan mode banner appears only in Plan/AcceptEdits sessions
- [x] \`bash echo hi > /tmp/x\` in Plan mode → Block error
- [x] \`bash cargo build\` in Plan mode → Block
- [x] \`read_file foo.rs\` in Plan mode → AutoApprove
- [x] Agent calls ask_user → banner appears, user answers, agent receives result
- [x] Agent calls exit_plan_mode → modal w/ 3 decisions; accept_and_auto switches to Supervised; accept_keep_plan creates V14 rules; reject returns feedback as tool error
- [x] uclaw.md content appears in effective system prompt
- [x] Settings → 提示词 tab loads/saves uclaw.md
- [x] All existing tests still pass; new tests added (≥12 backend, ≥6 frontend)

## Verification

- ✅ \`cargo build\` clean
- ✅ ~210 backend tests passing (was 195)
- ✅ \`tsc --noEmit\` clean
- ✅ ~61 frontend tests passing (was 50)
- ✅ \`vite build\` succeeds

## Karpathy attribution

Behavioral baseline (\`prompts/baseline.md\`) adapted from forrestchang/andrej-karpathy-skills (MIT). Header comment in the file points to source.

## Out of scope (deferred)

- superpowers:writing-plans auto-invocation in Plan mode (orthogonal layer)
- Per-session uclaw.md (workspace-level only for v1)
- In-app monaco/codemirror editor (plain textarea sufficient)
- Audit log "awaiting_user" status (separate V16 migration)
- exit_plan_mode allowed_prompts cleanup on session end
EOF
)"
```

---

## Acceptance criteria (cumulative)

- ✅ 5 SafetyMode variants exist with correct serde wire format
- ✅ Resolver behavior matches the spec's mode × ApprovalRequirement table
- ✅ V14 pattern rules can override Plan mode block (escape hatch tested)
- ✅ 4-layer prompt composition works; uclaw.md correctly read on each call
- ✅ Karpathy attribution present in baseline.md
- ✅ ask_user tool emits IPC, blocks oneshot, resolves via respond_ask_user
- ✅ exit_plan_mode tool routes 3 decisions correctly (auto-switch / V14 rules / feedback as error)
- ✅ Frontend dropdown + Shift+Cmd+M shortcut + 1-5 number keys
- ✅ Mode banner appears only for Plan / AcceptEdits
- ✅ AskUserBanner + ExitPlanModeBanner mounted at AppShell root
- ✅ Settings → 提示词 tab loads/saves uclaw.md + shows read-only baseline preview
- ✅ All Karpathy + mode prompts ship as compile-time include_str! markdown
- ✅ Each task ships its own commit (bisectable)

## Out of scope (deferred to follow-ups)

- superpowers:writing-plans auto-invocation in Plan mode
- Per-session uclaw.md
- In-app monaco/codemirror editor (plain textarea suffices)
- Audit log "awaiting_user" status (V16 migration)
- exit_plan_mode allowed_prompts rule cleanup on session end
- "Open in external editor" actually opens the file (currently just a hint toast — proper integration via tauri-plugin-shell open is a small follow-up)
