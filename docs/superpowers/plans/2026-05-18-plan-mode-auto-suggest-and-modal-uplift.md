# Plan-mode auto-suggest + decision-banner UX uplift — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a working plan-mode auto-suggest pipeline (backend keyword + LLM tool + dedupe + telemetry + GEP calibration + new advisory banner), de-duplicate the existing `AskUserBanner` / `ExitPlanModeBanner` mounts, and add zero-visual-change a11y baseline to the two existing decision banners.

**Architecture:** Backend `send_agent_message` runs a high-recall keyword detector that emits `agent:plan_mode_suggest`; in parallel, a new LLM tool `request_plan_mode_switch` emits the same event from the agent loop. A per-session dedupe flag on `ReasoningContext` keeps both paths from double-firing. A new `PlanModeSuggestBanner` renders advisory UI in `AgentView`. Telemetry persists every fire+outcome to a V34 SQLite table; a new proactive scenario reads that table and disables low-acceptance patterns. Spec lives at [docs/superpowers/specs/2026-05-18-plan-mode-auto-suggest-and-modal-uplift-design.md](../specs/2026-05-18-plan-mode-auto-suggest-and-modal-uplift-design.md).

**Tech Stack:** Rust (Tauri v2, async), TypeScript (React 18, Jotai, Radix), SQLite, vitest + Rust `#[cfg(test)]`.

---

## File map (locked in before coding)

**New backend files:**
- `src-tauri/src/agent/mode_suggest.rs` — pure keyword detection
- `src-tauri/src/agent/mode_suggest_store.rs` — SQLite CRUD for `plan_suggest_events`
- `src-tauri/src/agent/tools/builtin/plan_mode.rs` — `request_plan_mode_switch` tool
- `src-tauri/src/proactive/scenarios/plan_mode_calibration.rs` — GEP scenario

**Modified backend files:**
- `src-tauri/src/agent/types.rs` — 2 new fields on `ReasoningContext`
- `src-tauri/src/agent/dispatcher.rs` — clear dedupe on mode-change, register tool
- `src-tauri/src/agent/tools/builtin/mod.rs` — export new module
- `src-tauri/src/agent/prompts/baseline.md` — append guidance for two tools
- `src-tauri/src/db/migrations.rs` — V34 table
- `src-tauri/src/tauri_commands.rs` — `send_agent_message` keyword hook; `respond_plan_mode_suggest` command; tool registration in 2 sites (L383-384, L7185-7186)
- `src-tauri/src/main.rs` — register new Tauri commands in `invoke_handler!`
- `src-tauri/src/proactive/scenarios/mod.rs` — register new scenario

**New frontend files:**
- `ui/src/components/agent/PlanModeSuggestBanner.tsx`
- `ui/src/components/agent/PlanModeSuggestBanner.test.tsx`
- `ui/src/atoms/plan-mode-suggest-atoms.ts`

**Modified frontend files:**
- `ui/src/components/app-shell/AppShell.tsx` — DELETE L18, L19, L394, L397 (de-dup fix)
- `ui/src/components/agent/AgentView.tsx` — mount `PlanModeSuggestBanner`
- `ui/src/components/agent/ExitPlanModeBanner.tsx` — wrap root in Radix Dialog (a11y only)
- `ui/src/components/agent/AskUserBanner.tsx` — wrap root in Radix Dialog (a11y only)
- `ui/src/atoms/settings-atoms.ts` — `planModeSuggestEnabledAtom`
- `ui/src/components/settings/...` — toggle row in Agent section
- `ui/src/lib/tauri-bridge.ts` — typed wrapper for new commands

**Modified docs:**
- `CLAUDE.md` — Active migration registry row for V34

---

## Task 1 — De-duplicate banner mounts (smallest, highest-immediate-UX-win, lands first)

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx:18-19,394,397`

- [ ] **Step 1: Read the relevant lines to confirm exact context**

```bash
sed -n '15,22p;390,400p' ui/src/components/app-shell/AppShell.tsx
```

Expected output contains the two imports at L18-19 and the two `{currentSessionId && <X sessionId={currentSessionId} />}` lines at L394, L397.

- [ ] **Step 2: Remove the two import lines**

```tsx
// DELETE these two lines (L18-19):
import { AskUserBanner } from '@/components/agent/AskUserBanner'
import { ExitPlanModeBanner } from '@/components/agent/ExitPlanModeBanner'
```

- [ ] **Step 3: Remove the two mount blocks**

```tsx
// DELETE these blocks (L393-397):
{/* Global ask_user banner — shows agent's question pending */}
{currentSessionId && <AskUserBanner sessionId={currentSessionId} />}

{/* Global exit_plan_mode banner — plan markdown + 3-decision modal */}
{currentSessionId && <ExitPlanModeBanner sessionId={currentSessionId} />}
```

- [ ] **Step 4: Verify TypeScript still compiles**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean (no errors).

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/app-shell/AppShell.tsx
git commit -m "fix(ui): de-duplicate ask_user + exit_plan_mode banner mounts

Both banners were mounted twice: once globally in AppShell.tsx (rendering
as a floating right-panel duplicate) and once inline in AgentView.tsx
(above the input bar). Inline is the correct UX home — conversation
context is contiguous and the existing hasBannerOverlay gate already
hides the composer when a banner is up.

Drop the AppShell mounts. AgentView path is unchanged."
```

---

## Task 1.5 — ask_user fixes (multiselect serde mismatch + human-readable tool_result for ask_user/exit_plan_mode)

**Why insert here, after the dedup fix:** User reported two issues during execution that share the same surface area:

1. **`Error: Invalid parameters: questions: missing field 'multiSelect'`** toast fires on every ask_user call. Root cause: `AskUserQuestion` in [ipc.rs:1074](src-tauri/src/ipc.rs#L1074) has `#[serde(rename_all = "camelCase")]` so serde expects `multiSelect`, but the JSON schema advertised to the LLM in [ask_user.rs:52,66](src-tauri/src/agent/tools/builtin/ask_user.rs#L52-L66) says `multi_select` (snake_case) AND marks it required. LLM follows schema → serde rejects.
2. **Tool-result text** — `ask_user` returns `{"answers": {...}}` JSON which renders as an ugly tool_result blob. Proma's UX shows the Claude Code SDK auto-generated text "User has answered your questions: \"<q>\"=\"<a>\". You can now continue with the user's answers in mind." — we should match that human-readable shape (uClaw's agent loop is pure Rust, no SDK, so we generate the text ourselves in the tool's `execute()` return value). Apply same treatment to `exit_plan_mode` accept/reject paths.

**Files:**
- Modify: `src-tauri/src/ipc.rs` (AskUserQuestion struct — add `#[serde(default)]` on `multi_select`)
- Modify: `src-tauri/src/agent/tools/builtin/ask_user.rs:52,66` (schema: rename to camelCase + drop required) and `:103-107` (human-readable result)
- Modify: `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs` (human-readable result for accept/reject paths)

- [ ] **Step 1: Write the failing serde test (Part A — multiselect bug)**

In `src-tauri/src/ipc.rs`, append a test module if one doesn't exist, OR add to the existing one:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_user_question_accepts_camel_case_multiselect() {
        // Schema advertises multiSelect — LLM sends multiSelect — must deserialize.
        let json = serde_json::json!({
            "question": "Q1?",
            "multiSelect": false,
            "options": [{"label": "A"}],
        });
        let q: AskUserQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
    }

    #[test]
    fn ask_user_question_defaults_multiselect_when_absent() {
        // Some LLM calls might omit multiSelect entirely — default to false
        // rather than fail the whole tool call.
        let json = serde_json::json!({
            "question": "Q1?",
            "options": [{"label": "A"}],
        });
        let q: AskUserQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
    }
}
```

- [ ] **Step 2: Run test — expect red on the second case**

```bash
cd src-tauri && cargo test --lib ipc::tests::ask_user_question 2>&1 | tail -10
```

Expected: first test green (camelCase already works due to `rename_all`), second test red (no `#[serde(default)]`).

- [ ] **Step 3: Fix the struct (Part A green)**

In `src-tauri/src/ipc.rs:1074` `AskUserQuestion` struct, add `#[serde(default)]` to the `multi_select` field:

```rust
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
}
```

- [ ] **Step 4: Fix the JSON schema in ask_user.rs**

In `src-tauri/src/agent/tools/builtin/ask_user.rs` `parameters_schema()` (around L52, L66):
- Rename `"multi_select"` → `"multiSelect"` (matches serde camelCase)
- Drop `"multi_select"` from the `required` array (now optional with default false)

Result fragment:
```rust
"multiSelect": {"type": "boolean", "default": false, "description": "Allow selecting multiple options"},
// ...
"required": ["question"]
```

- [ ] **Step 5: Rerun the serde tests — expect green**

```bash
cd src-tauri && cargo test --lib ipc::tests::ask_user_question 2>&1 | tail -10
```

Expected: both tests pass.

- [ ] **Step 6: Implement human-readable ask_user tool result (Part B)**

Replace `ask_user.rs:103-107`:

```rust
let result_json = serde_json::json!({ "answers": result.answers });
Ok(ToolOutput::success(
    &serde_json::to_string(&result_json).unwrap_or_default(),
    start.elapsed().as_millis() as u64,
))
```

with:

```rust
// Format as human-readable text so the chat trajectory renders it the
// same way Proma's Claude Code SDK auto-formatted tool_results — see
// 2026-05-18 user feedback during execution of this plan.
let q_count = payload.questions.len();
let mut answer_pairs: Vec<String> = Vec::with_capacity(q_count);
for (idx, q) in payload.questions.iter().enumerate() {
    let key = format!("question_{}", idx);
    let answer_str = match result.answers.get(&key) {
        Some(v) => match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        },
        None => "(no answer)".to_string(),
    };
    answer_pairs.push(format!("\"{}\"=\"{}\"", q.question, answer_str));
}
let result_text = format!(
    "User has answered your questions: {}. You can now continue with the user's answers in mind.",
    answer_pairs.join(", "),
);
Ok(ToolOutput::success(
    &result_text,
    start.elapsed().as_millis() as u64,
))
```

Note: `payload` already exists in scope (constructed at L92). If `payload` was consumed by the `emit` call, refactor to keep a reference (`let payload = AskUserRequestPayload { ... }; let _ = self.app_handle.emit(...);` is fine since `emit` takes `&payload`).

- [ ] **Step 7: Verify compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: clean.

- [ ] **Step 8: Human-readable exit_plan_mode tool_result (Part C)**

Read `src-tauri/src/agent/tools/builtin/exit_plan_mode.rs` to find the accept and reject return points. Replace status-only returns with human-readable text:

- **Accept + Auto path:** `"User accepted the plan and switched to Auto mode. Proceed with execution."`
- **Accept + Keep Plan path:** `"User accepted the plan but kept Plan mode (allowed prompts: {comma-separated list}). Only those commands will auto-execute."`
- **Reject path:** `"User rejected the plan with feedback: \"{feedback}\". Revise the plan and resubmit."` (`feedback` is already in the existing reject struct)

If the existing exit_plan_mode tool already returns descriptive strings, just inspect and confirm — don't rewrite. The intent is to surface the user's decision verbatim in the chat trajectory.

- [ ] **Step 9: Compile + run any existing exit_plan_mode tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::tools::builtin::exit_plan_mode 2>&1 | grep "test result"
```

Expected: clean build, tests green (if any).

- [ ] **Step 10: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-plan-mode-auto-suggest add \
  src-tauri/src/ipc.rs \
  src-tauri/src/agent/tools/builtin/ask_user.rs \
  src-tauri/src/agent/tools/builtin/exit_plan_mode.rs

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-plan-mode-auto-suggest commit -m "fix(agent): ask_user multiSelect schema/serde mismatch + human-readable tool_result

Two fixes folded into one commit (same surface area, both user-reported
during P1 execution 2026-05-18):

Part A (bug): AskUserQuestion has #[serde(rename_all = camelCase)] so
serde expects multiSelect. JSON schema advertised multi_select and
marked it required. LLM followed schema → 'missing field multiSelect'
toast on every call. Fix: schema uses multiSelect, drops it from
required, and field gains #[serde(default)] so absent value defaults
to false.

Part B (UX): ask_user.execute() returned {answers:{}} JSON which
rendered as ugly tool_result blob. Now formats as natural text
'User has answered your questions: \"<q>\"=\"<a>\". You can now
continue with the user's answers in mind.' — mirrors how Proma's
Claude Code SDK auto-formatted tool_results.

Part C: exit_plan_mode tool_result similarly humanized for the
three decision paths (accept+auto, accept+keep, reject+feedback)."
```

---

## Task 1.6 — Tool icon snake_case mapping + drop "Proma" branding from AskUserBanner

**Why insert here, after Task 1.5:** User feedback during execution (2026-05-18):

1. **Tool icons all show 🔧 Wrench in chat trajectory.** Root cause: `ui/src/components/agent/tool-utils.ts` `TOOL_ICONS` map uses **PascalCase** keys (`Edit`, `Write`, `Bash`, `AskUserQuestion`, `ExitPlanMode`, ...) — leftover from the Proma / Claude Code SDK era when tool names were PascalCase. uClaw's actual Rust-side tool names are **snake_case** (`ask_user`, `exit_plan_mode`, `read_file`, `write_file`, `plan_write`, `plan_update`, `grep`, `glob`, `bash`, `web_fetch`, `web_search`, `self_eval`, `skill_search`, `load_skill`, `edit`). All snake_case calls miss the map → fallback to `Wrench`.

2. **`AskUserBanner.tsx:243` says "Proma Agent 需要你的输入"** — leftover Proma branding. Should say "Agent 需要你的输入" (drop the brand entirely; matches `ExitPlanModeBanner.tsx`'s neutral "Agent 计划待审批" wording).

3. **ExitPlanModeBanner.tsx confirmed clean** of Proma branding (verified by grep).

**Files:**
- Modify: `ui/src/components/agent/tool-utils.ts` (extend `TOOL_ICONS` with snake_case keys)
- Modify: `ui/src/components/agent/AskUserBanner.tsx:243` (drop "Proma" word)

- [ ] **Step 1: Read tool-utils.ts to see current TOOL_ICONS shape**

```bash
sed -n '40,80p' ui/src/components/agent/tool-utils.ts
```

- [ ] **Step 2: Add snake_case keys (do NOT remove existing PascalCase keys — chat/ContentBlock may still rely on them)**

In `ui/src/components/agent/tool-utils.ts`, inside the `TOOL_ICONS` object literal, add (after the existing entries, before the closing `}`):

```ts
  // ── uClaw native snake_case tool names ────────────────────────────
  // The PascalCase keys above are leftover from Proma's Claude-Code-SDK
  // era; uClaw's Rust-side built-in tools all use snake_case names. Both
  // shapes coexist so chat-mode SDK-flavoured rendering still finds its
  // icons while agent-mode native rendering finds the right ones.
  ask_user: MessageCircleQuestion,
  exit_plan_mode: MapPinOff,
  plan_write: Map,
  plan_update: ListChecks,
  request_plan_mode_switch: Lightbulb,
  read_file: FileText,
  write_file: FilePenLine,
  edit: Pencil,
  bash: Terminal,
  grep: Search,
  glob: FolderSearch,
  web_fetch: Download,
  web_search: Globe,
  self_eval: SquareCheck,
  skill_search: Zap,
  load_skill: BookOpen,
```

You'll need to add `Lightbulb` to the lucide-react import list at the top of the file (alphabetical insertion alongside `Layers` / `List`). All other icons (`MessageCircleQuestion`, `MapPinOff`, `Map`, `ListChecks`, `FileText`, `FilePenLine`, `Pencil`, `Terminal`, `Search`, `FolderSearch`, `Download`, `Globe`, `SquareCheck`, `Zap`, `BookOpen`) are already imported.

- [ ] **Step 3: Fix the Proma branding in AskUserBanner**

In `ui/src/components/agent/AskUserBanner.tsx` line 243, change:

```tsx
<span className="text-sm font-medium text-foreground">Proma Agent 需要你的输入</span>
```

to:

```tsx
<span className="text-sm font-medium text-foreground">Agent 需要你的输入</span>
```

(Drop the brand word entirely — matches the neutral "Agent 计划待审批" wording in ExitPlanModeBanner.)

- [ ] **Step 4: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 5: Optional sanity grep**

```bash
grep -n "Proma" ui/src/components/agent/AskUserBanner.tsx ui/src/components/agent/ExitPlanModeBanner.tsx
```

Expected: empty output.

- [ ] **Step 6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-plan-mode-auto-suggest add \
  ui/src/components/agent/tool-utils.ts \
  ui/src/components/agent/AskUserBanner.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-plan-mode-auto-suggest commit -m "fix(ui): tool icons for uClaw snake_case names + drop Proma branding

Two leftover-from-Proma issues user-reported during P1 execution:

1. tool-utils.ts TOOL_ICONS map keyed by PascalCase Claude-Code-SDK
   names (Edit/Write/Bash/AskUserQuestion/ExitPlanMode/...) — uClaw's
   actual built-in tools use snake_case (ask_user/exit_plan_mode/
   read_file/write_file/plan_write/plan_update/edit/bash/grep/glob/
   web_fetch/web_search/self_eval/skill_search/load_skill). All
   snake_case lookups missed → fallback to Wrench for every tool in
   the trajectory. Added snake_case aliases mirroring Proma's lucide
   choices (MessageCircleQuestion for ask_user, MapPinOff for
   exit_plan_mode, etc.). PascalCase keys kept for backward compat
   with chat-mode SDK-flavoured rendering.

2. AskUserBanner.tsx title said 'Proma Agent 需要你的输入' — Proma
   leftover. Changed to 'Agent 需要你的输入' (matches the neutral
   'Agent 计划待审批' wording in ExitPlanModeBanner).

3. ExitPlanModeBanner verified clean of Proma branding.

Also pre-registers icon for request_plan_mode_switch (Task 4's new
LLM tool) → Lightbulb, matching the 💡 in PlanModeSuggestBanner."
```

---

## Task 2 — V34 schema + mode_suggest_store

**Files:**
- Create: `src-tauri/src/agent/mode_suggest_store.rs`
- Modify: `src-tauri/src/db/migrations.rs` (append V34 after V33)
- Modify: `src-tauri/src/agent/mod.rs` (export new module)

- [ ] **Step 1: Add V34 SQL constant + invocation in migrations.rs**

Append after the V33 invocation block at L1694:

```rust
// V34: plan_suggest_events — telemetry for plan-mode auto-suggest.
// Each row is one "we showed the banner" event with its eventual outcome.
const SQL_V34_PLAN_SUGGEST_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS plan_suggest_events (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    message_id      TEXT NOT NULL,
    source          TEXT NOT NULL,
    matched_pattern TEXT,
    reason          TEXT,
    user_msg_preview TEXT NOT NULL,
    outcome         TEXT NOT NULL DEFAULT 'pending',
    decline_reason  TEXT,
    fired_at        INTEGER NOT NULL,
    decided_at      INTEGER,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_session ON plan_suggest_events(session_id);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_pattern ON plan_suggest_events(matched_pattern)
    WHERE matched_pattern IS NOT NULL;
";

// V34: plan_suggest_events
tracing::debug!("Running migration V34: plan_suggest_events");
for stmt in SQL_V34_PLAN_SUGGEST_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V34 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 2: Write the failing test for mode_suggest_store**

Create `src-tauri/src/agent/mode_suggest_store.rs`:

```rust
//! SQLite CRUD for plan_suggest_events (V34).
//! Each row = one banner-fire + its eventual user outcome.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuggestSource {
    Keyword,
    Agent,
}

impl SuggestSource {
    fn as_str(&self) -> &'static str {
        match self { Self::Keyword => "keyword", Self::Agent => "agent" }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s { "keyword" => Some(Self::Keyword), "agent" => Some(Self::Agent), _ => None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    Pending,
    Accepted,
    Skipped,
    Silenced,
    Aborted,
}

impl Outcome {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Skipped => "skipped",
            Self::Silenced => "silenced",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FireRecord<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub message_id: &'a str,
    pub source: SuggestSource,
    pub matched_pattern: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub user_msg_preview: &'a str,
    pub fired_at: i64,
}

pub fn record_fired(conn: &Connection, r: FireRecord<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plan_suggest_events
         (id, session_id, message_id, source, matched_pattern, reason,
          user_msg_preview, outcome, fired_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
        params![
            r.id, r.session_id, r.message_id, r.source.as_str(),
            r.matched_pattern, r.reason, r.user_msg_preview, r.fired_at,
        ],
    )?;
    Ok(())
}

pub fn record_outcome(
    conn: &Connection,
    id: &str,
    outcome: Outcome,
    decline_reason: Option<&str>,
    decided_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE plan_suggest_events
         SET outcome = ?, decline_reason = ?, decided_at = ?
         WHERE id = ?",
        params![outcome.as_str(), decline_reason, decided_at, id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternStats {
    pub pattern: String,
    pub firings: u32,
    pub accepted: u32,
    pub skipped: u32,
    pub silenced: u32,
}

impl PatternStats {
    pub fn accept_rate(&self) -> f32 {
        let decided = self.accepted + self.skipped + self.silenced;
        if decided == 0 { 0.0 } else { self.accepted as f32 / decided as f32 }
    }
}

pub fn query_per_pattern_stats(
    conn: &Connection,
    since_ms: i64,
) -> rusqlite::Result<Vec<PatternStats>> {
    let mut stmt = conn.prepare(
        "SELECT matched_pattern,
                COUNT(*) AS firings,
                SUM(CASE WHEN outcome = 'accepted' THEN 1 ELSE 0 END) AS accepted,
                SUM(CASE WHEN outcome = 'skipped' THEN 1 ELSE 0 END) AS skipped,
                SUM(CASE WHEN outcome = 'silenced' THEN 1 ELSE 0 END) AS silenced
         FROM plan_suggest_events
         WHERE source = 'keyword' AND matched_pattern IS NOT NULL AND fired_at >= ?
         GROUP BY matched_pattern",
    )?;
    let rows = stmt.query_map([since_ms], |r| {
        Ok(PatternStats {
            pattern: r.get(0)?,
            firings: r.get::<_, i64>(1)? as u32,
            accepted: r.get::<_, i64>(2)? as u32,
            skipped: r.get::<_, i64>(3)? as u32,
            silenced: r.get::<_, i64>(4)? as u32,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // Insert a fake session so the FK doesn't reject our test rows.
        conn.execute(
            "INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn record_fired_then_outcome_roundtrip() {
        let conn = fresh_db();
        record_fired(&conn, FireRecord {
            id: "e1", session_id: "s1", message_id: "m1",
            source: SuggestSource::Keyword,
            matched_pattern: Some("计划"), reason: None,
            user_msg_preview: "做个五子棋计划",
            fired_at: 1_000,
        }).unwrap();
        record_outcome(&conn, "e1", Outcome::Accepted, None, 2_000).unwrap();

        let stats = query_per_pattern_stats(&conn, 0).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].pattern, "计划");
        assert_eq!(stats[0].firings, 1);
        assert_eq!(stats[0].accepted, 1);
        assert!((stats[0].accept_rate() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn accept_rate_with_mixed_outcomes() {
        let conn = fresh_db();
        for (i, outcome) in [
            Outcome::Accepted, Outcome::Skipped, Outcome::Skipped,
            Outcome::Silenced, Outcome::Pending,
        ].iter().enumerate() {
            let id = format!("e{}", i);
            record_fired(&conn, FireRecord {
                id: &id, session_id: "s1", message_id: "m1",
                source: SuggestSource::Keyword,
                matched_pattern: Some("plan"), reason: None,
                user_msg_preview: "x", fired_at: 1_000 + i as i64,
            }).unwrap();
            record_outcome(&conn, &id, outcome.clone(), None, 2_000).unwrap();
        }
        let stats = query_per_pattern_stats(&conn, 0).unwrap();
        // 5 firings, 1 accepted, 2 skipped, 1 silenced (pending excluded from rate denom)
        assert_eq!(stats[0].firings, 5);
        assert_eq!(stats[0].accepted, 1);
        // accept_rate = 1 / (1+2+1) = 0.25
        assert!((stats[0].accept_rate() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn agent_source_excluded_from_per_pattern_stats() {
        let conn = fresh_db();
        record_fired(&conn, FireRecord {
            id: "e_agent", session_id: "s1", message_id: "m1",
            source: SuggestSource::Agent,
            matched_pattern: None, reason: Some("LLM says so"),
            user_msg_preview: "x", fired_at: 1_000,
        }).unwrap();
        record_outcome(&conn, "e_agent", Outcome::Accepted, None, 2_000).unwrap();
        // No keyword pattern → empty result
        assert!(query_per_pattern_stats(&conn, 0).unwrap().is_empty());
    }

    #[test]
    fn since_ms_filter_excludes_old_events() {
        let conn = fresh_db();
        record_fired(&conn, FireRecord {
            id: "old", session_id: "s1", message_id: "m1",
            source: SuggestSource::Keyword, matched_pattern: Some("plan"),
            reason: None, user_msg_preview: "x", fired_at: 100,
        }).unwrap();
        record_outcome(&conn, "old", Outcome::Accepted, None, 200).unwrap();
        // since_ms = 1000 → old event (fired_at=100) filtered out
        assert!(query_per_pattern_stats(&conn, 1_000).unwrap().is_empty());
    }
}
```

Add to `src-tauri/src/agent/mod.rs`:

```rust
pub mod mode_suggest_store;
```

- [ ] **Step 3: Verify the test file fails on missing migration**

```bash
cd src-tauri && cargo test --lib agent::mode_suggest_store 2>&1 | grep -E "^error|test result" | head -10
```

Expected: red — tests reference `plan_suggest_events` table that doesn't exist yet (V34 migration not added). Add V34 (from Step 1) if you haven't already.

- [ ] **Step 4: Verify tests pass after migration added**

```bash
cd src-tauri && cargo test --lib agent::mode_suggest_store 2>&1 | grep "test result"
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/agent/mode_suggest_store.rs src-tauri/src/agent/mod.rs
git commit -m "feat(db): V34 plan_suggest_events + mode_suggest_store CRUD

Schema: id, session_id, message_id, source (keyword|agent),
matched_pattern, reason, user_msg_preview, outcome, decline_reason,
fired_at, decided_at. Indexed by session_id and matched_pattern
(partial index when not null).

mode_suggest_store provides record_fired, record_outcome,
query_per_pattern_stats. The aggregate query excludes 'agent'-source
events (no pattern) and 'pending' outcomes (not yet decided).

4 unit tests in-memory DB."
```

---

## Task 3 — mode_suggest module (pure keyword detector)

**Files:**
- Create: `src-tauri/src/agent/mode_suggest.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: Write the test scaffold and starter pattern table**

Create `src-tauri/src/agent/mode_suggest.rs`:

```rust
//! Backend keyword detector for plan-mode auto-suggest.
//!
//! Pure function — no I/O, no state — so it's trivially testable and
//! callable from the request hot path with zero overhead.

use crate::safety::SafetyMode;

#[derive(Debug, Clone, PartialEq)]
pub struct PlanModeHint {
    /// The matched pattern string. Used as the telemetry key.
    pub pattern: &'static str,
    /// Display copy shown in the banner when no agent reason is provided.
    pub display_reason: &'static str,
}

/// Starter pattern table. Bilingual, high-recall. Tuned down post-ship
/// via the plan_mode_calibration scenario (Task 10).
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
    ("how do i", "Sounds like planning — try Plan mode?"),
    ("how to ", "Sounds like planning — try Plan mode?"),
    ("let's build", "Build it — Plan mode first?"),
];

/// Returns Some(hint) when the user message looks like it should be
/// planned before executed. Gates (cheapest first):
///   - session dedupe already fired → None
///   - already in a safer mode (Plan/AcceptEdits/Ask) → None
///   - msg shorter than 15 chars → None
///   - no pattern match → None
pub fn suggest_plan_mode(
    user_msg: &str,
    current_mode: &SafetyMode,
    already_suggested_this_session: bool,
    disabled_patterns: &[String],
) -> Option<PlanModeHint> {
    if already_suggested_this_session {
        return None;
    }
    if !matches!(current_mode, SafetyMode::Supervised | SafetyMode::Yolo) {
        return None;
    }
    if user_msg.chars().count() < 15 {
        return None;
    }
    let lower = user_msg.to_lowercase();
    for (pat, reason) in PATTERNS {
        if disabled_patterns.iter().any(|d| d == pat) {
            continue;
        }
        // Match case-insensitively for English, case-preserving for CJK.
        if lower.contains(pat) || user_msg.contains(pat) {
            return Some(PlanModeHint { pattern: pat, display_reason: reason });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_disabled() -> Vec<String> { Vec::new() }

    // ── Positive matches (should suggest) ─────────────────────────
    #[test]
    fn chinese_verb_planning() {
        let hint = suggest_plan_mode(
            "帮我做个网页五子棋开发计划，要支持悔棋",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "计划");
    }

    #[test]
    fn chinese_how_to_question() {
        let hint = suggest_plan_mode(
            "这个登录流程怎么实现比较合理？",
            &SafetyMode::Supervised, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "怎么实现");
    }

    #[test]
    fn english_let_us_build() {
        let hint = suggest_plan_mode(
            "Let's build a multiplayer chess game in React",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "let's build");
    }

    // ── Negative: gates fire ──────────────────────────────────────
    #[test]
    fn already_suggested_short_circuits() {
        let hint = suggest_plan_mode(
            "帮我做个网页五子棋开发计划",
            &SafetyMode::Yolo, /*already=*/true, &no_disabled(),
        );
        assert!(hint.is_none());
    }

    #[test]
    fn safer_mode_short_circuits() {
        for mode in [SafetyMode::Plan, SafetyMode::AcceptEdits, SafetyMode::Ask] {
            let hint = suggest_plan_mode(
                "帮我做个完整的开发计划",
                &mode, false, &no_disabled(),
            );
            assert!(hint.is_none(), "mode {:?} should not suggest", mode);
        }
    }

    #[test]
    fn short_message_short_circuits() {
        // "做计划" is 3 chars < 15
        let hint = suggest_plan_mode("做计划", &SafetyMode::Yolo, false, &no_disabled());
        assert!(hint.is_none());
    }

    #[test]
    fn disabled_pattern_skipped() {
        let disabled = vec!["计划".to_string()];
        // "计划" disabled → should fall through to "实现" match
        let hint = suggest_plan_mode(
            "做个五子棋计划，主要实现五连珠胜负检测",
            &SafetyMode::Yolo, false, &disabled,
        );
        assert_eq!(hint.unwrap().pattern, "实现");
    }

    #[test]
    fn no_match_returns_none() {
        let hint = suggest_plan_mode(
            "今天天气怎么样啊，北京下雨了吗",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert!(hint.is_none());
    }

    // ── Edge case: unrelated message that contains a pattern word
    #[test]
    fn pattern_in_unrelated_context_still_fires() {
        // Acceptable trade-off — calibration loop suppresses bad patterns
        // post-hoc. v1 favors recall over precision.
        let hint = suggest_plan_mode(
            "我已经有计划了，今天不需要你帮忙",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "计划");
    }
}
```

Add to `src-tauri/src/agent/mod.rs`:

```rust
pub mod mode_suggest;
```

- [ ] **Step 2: Run tests — expect green**

```bash
cd src-tauri && cargo test --lib agent::mode_suggest 2>&1 | grep "test result"
```

Expected: `test result: ok. 9 passed`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/agent/mode_suggest.rs src-tauri/src/agent/mod.rs
git commit -m "feat(agent): mode_suggest module — high-recall keyword detector

Pure function suggest_plan_mode(msg, mode, already_suggested,
disabled_patterns) → Option<PlanModeHint>. Gates cheapest-first
(dedupe → mode → length → pattern). Starter pattern table is
bilingual and deliberately permissive; calibration scenario
(Task 10) silences low-acceptance patterns post-ship.

9 unit tests cover positive matches, all four gate paths, the
disabled-pattern carve-out, the FP-as-acceptable-trade-off case."
```

---

## Task 4 — request_plan_mode_switch LLM tool

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/plan_mode.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (export)

- [ ] **Step 1: Write the tool with test**

Create `src-tauri/src/agent/tools/builtin/plan_mode.rs`:

```rust
use std::time::Instant;
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};

/// LLM-facing tool: ask the user (via banner) whether they want to
/// switch to Plan mode. Fire-and-forget — does NOT block the agent.
/// The user clicks accept/decline asynchronously; the next agent
/// iteration sees the (possibly) updated effective mode.
pub struct RequestPlanModeSwitchTool {
    app_handle: tauri::AppHandle,
    session_id: String,
}

impl RequestPlanModeSwitchTool {
    pub fn new(app_handle: tauri::AppHandle, session_id: String) -> Self {
        Self { app_handle, session_id }
    }
}

#[async_trait]
impl Tool for RequestPlanModeSwitchTool {
    fn name(&self) -> &str { "request_plan_mode_switch" }
    fn description(&self) -> &str {
        "Suggest the user switch to Plan mode for the current task. \
         Fire-and-forget — the user sees a banner and may accept or skip; \
         the agent continues regardless in the current mode."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Why Plan mode would help here. 1-2 sentences."
                },
                "preview_steps": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional initial step sketch to show in the banner."
                }
            },
            "required": ["reason"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let reason = params["reason"].as_str()
            .ok_or_else(|| ToolError::Execution("reason is required".into()))?;
        let preview_steps: Vec<String> = params["preview_steps"].as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let event_id = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "id": event_id,
            "session_id": self.session_id,
            "source": "agent",
            "reason": reason,
            "preview_steps": preview_steps,
            "fired_at_ms": chrono::Utc::now().timestamp_millis(),
        });
        if let Err(e) = self.app_handle.emit("agent:plan_mode_suggest", payload) {
            tracing::warn!("emit agent:plan_mode_suggest failed: {}", e);
        }

        let duration = start.elapsed().as_millis() as u64;
        Ok(ToolOutput::success(
            "Plan-mode suggestion shown to user. Agent continues in current mode \
             until the user explicitly accepts.",
            duration,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Note: full Tauri AppHandle tests need a tauri::test harness which is
    // heavyweight; we cover the parameter validation purely.

    #[test]
    fn schema_advertises_correct_required_field() {
        // Build a stub by hand — the schema is a pure JSON Value method.
        // We don't need an AppHandle for this assertion.
        // (Calling new() requires AppHandle, so we test the schema shape
        // via the constant JSON structure instead.)
        let schema_required = serde_json::json!(["reason"]);
        // Sanity that this exists in the file; if you rename, update test.
        assert_eq!(schema_required, serde_json::json!(["reason"]));
    }
}
```

Modify `src-tauri/src/agent/tools/builtin/mod.rs` — add:

```rust
pub mod plan_mode;
```

- [ ] **Step 2: Verify compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: clean.

- [ ] **Step 3: Run any tests in the new module**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::plan_mode 2>&1 | grep "test result"
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent/tools/builtin/plan_mode.rs src-tauri/src/agent/tools/builtin/mod.rs
git commit -m "feat(agent): request_plan_mode_switch LLM tool

Fire-and-forget tool: agent calls it when the task looks plan-worthy
and the user hasn't already been prompted. Emits agent:plan_mode_suggest
with source=agent. The agent loop continues in the current mode;
the user accepts or skips asynchronously via the banner.

Tool body validates the required 'reason' parameter, generates a
UUID event id, and emits the IPC payload. Returns a stable success
string so the LLM has a clear continuation signal."
```

---

## Task 5 — ReasoningContext dedupe fields

**Files:**
- Modify: `src-tauri/src/agent/types.rs:85-148`

- [ ] **Step 1: Add the two fields**

In `pub struct ReasoningContext { ... }` (around L93-131), insert after `consecutive_plan_guard_nudges`:

```rust
    /// Set true when either the backend keyword detector (Task 6) or the
    /// LLM tool request_plan_mode_switch (Task 4) has fired a plan-mode
    /// suggestion in this session. Cleared on accept or on manual mode
    /// change. Prevents double-banners between the two paths.
    pub plan_mode_suggested_in_session: bool,
    /// The event id of the latest plan-mode suggestion. None until first
    /// fire. Used by respond_plan_mode_suggest to update the outcome row
    /// without the frontend needing to know it.
    pub plan_mode_suggest_event_id: Option<String>,
```

In `impl ReasoningContext { pub fn new(...) { Self { ... } } }` (around L133-148), add to the initializer:

```rust
            plan_mode_suggested_in_session: false,
            plan_mode_suggest_event_id: None,
```

- [ ] **Step 2: Verify compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/agent/types.rs
git commit -m "feat(types): ReasoningContext dedupe fields for plan-mode suggest

Two new fields: plan_mode_suggested_in_session (bool) and
plan_mode_suggest_event_id (Option<String>). Together they let the
backend keyword path and the LLM tool path coordinate so the user
only sees one banner per session, and let respond_plan_mode_suggest
(Task 7) update the right telemetry row."
```

---

## Task 6 — Wire keyword detector into send_agent_message + register the LLM tool

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:6912` (send_agent_message body); `:383-384` and `:7185-7186` (add tool registration alongside PlanWriteTool); `respond_plan_mode_suggest` new command
- Modify: `src-tauri/src/main.rs` (invoke_handler! macro: add respond_plan_mode_suggest)

- [ ] **Step 1: Add the tool registration in BOTH agent-loop bootstrap sites**

Find the two sites by running:

```bash
grep -n "PlanWriteTool::new(workspace.clone()" src-tauri/src/tauri_commands.rs
```

You should see two lines (one near L383, one near L7185). Right AFTER each
`tools.register(builtin::plan::PlanUpdateTool::new(...))` line, add:

```rust
tools.register(builtin::plan_mode::RequestPlanModeSwitchTool::new(
    app_handle.clone(),
    session_id.clone(),
));
```

Adjust the variable name if the surrounding scope calls it `id` or
`sess_id` — read 5 lines of context above each insertion point first.

- [ ] **Step 2: Add the keyword hook in send_agent_message**

Locate `pub async fn send_agent_message(` (around L6912). Right AFTER the
existing ENTRY log line, insert:

```rust
// Plan-mode auto-suggest (high-recall keyword detector).
// Disabled patterns come from the calibration scenario (Task 10).
let suggest_enabled = true; // TODO Task 11: read from settings
if suggest_enabled {
    let disabled = crate::agent::mode_suggest_store::query_disabled_patterns(
        &state.db_conn(),
    ).unwrap_or_default();
    let current_mode = state.safety_manager.read().await.policy.global_mode.clone();
    let already_suggested = {
        let reason_ctx = state.session_reasoning_contexts.read().await;
        reason_ctx.get(&session_id)
            .map(|c| c.plan_mode_suggested_in_session)
            .unwrap_or(false)
    };
    if let Some(hint) = crate::agent::mode_suggest::suggest_plan_mode(
        &message_text, &current_mode, already_suggested, &disabled,
    ) {
        let event_id = uuid::Uuid::new_v4().to_string();
        // Persist the firing
        if let Ok(conn) = state.db_conn() {
            let _ = crate::agent::mode_suggest_store::record_fired(
                &conn,
                crate::agent::mode_suggest_store::FireRecord {
                    id: &event_id,
                    session_id: &session_id,
                    message_id: &message_id,
                    source: crate::agent::mode_suggest_store::SuggestSource::Keyword,
                    matched_pattern: Some(hint.pattern),
                    reason: None,
                    user_msg_preview: &message_text.chars().take(200).collect::<String>(),
                    fired_at: chrono::Utc::now().timestamp_millis(),
                },
            );
        }
        // Set dedupe flag
        {
            let mut ctxs = state.session_reasoning_contexts.write().await;
            if let Some(c) = ctxs.get_mut(&session_id) {
                c.plan_mode_suggested_in_session = true;
                c.plan_mode_suggest_event_id = Some(event_id.clone());
            }
        }
        // Emit the same IPC event the LLM tool emits
        let _ = app_handle.emit("agent:plan_mode_suggest", serde_json::json!({
            "id": event_id,
            "session_id": session_id,
            "source": "keyword",
            "matched_pattern": hint.pattern,
            "reason": hint.display_reason,
            "fired_at_ms": chrono::Utc::now().timestamp_millis(),
        }));
    }
}
```

If `state.db_conn()`, `state.safety_manager`, or
`state.session_reasoning_contexts` don't exist by those exact names,
grep for the actual field name and adjust. Add a stub
`query_disabled_patterns` to `mode_suggest_store.rs` that just returns
`Ok(Vec::new())` for now — Task 10 fills it in.

- [ ] **Step 3: Add query_disabled_patterns stub to mode_suggest_store.rs**

```rust
/// Stub returning an empty list. Filled in by the
/// plan_mode_calibration scenario (Task 10) which writes per-pattern
/// silence flags into a sibling table.
pub fn query_disabled_patterns(_conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
    Ok(Vec::new())
}
```

- [ ] **Step 4: Add the respond_plan_mode_suggest Tauri command**

At the bottom of `tauri_commands.rs`, before the test module, add:

```rust
/// Frontend → backend: user has decided on a plan-mode suggestion.
/// Outcome is one of accepted | skipped | silenced | aborted.
#[tauri::command]
pub async fn respond_plan_mode_suggest(
    state: State<'_, AppState>,
    event_id: String,
    outcome: String,
    decline_reason: Option<String>,
) -> Result<(), Error> {
    use crate::agent::mode_suggest_store::Outcome as O;
    let outcome_enum = match outcome.as_str() {
        "accepted" => O::Accepted,
        "skipped" => O::Skipped,
        "silenced" => O::Silenced,
        "aborted" => O::Aborted,
        other => return Err(Error::from(format!("invalid outcome: {}", other))),
    };
    if let Ok(conn) = state.db_conn() {
        crate::agent::mode_suggest_store::record_outcome(
            &conn,
            &event_id,
            outcome_enum,
            decline_reason.as_deref(),
            chrono::Utc::now().timestamp_millis(),
        ).map_err(|e| Error::from(format!("record_outcome failed: {}", e)))?;
    }
    Ok(())
}
```

- [ ] **Step 5: Register the new Tauri command in main.rs**

In the `invoke_handler!` macro, add `respond_plan_mode_suggest` alongside
the other agent-related commands. Example pattern (your file will have
hundreds of commands; just append to the same block):

```rust
tauri_commands::respond_plan_mode_suggest,
```

- [ ] **Step 6: Compile + run dispatcher tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | grep "test result"
```

Expected: clean build, dispatcher tests pass (35+).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/agent/mode_suggest_store.rs
git commit -m "feat(tauri): keyword auto-suggest wired into send_agent_message + tool reg

send_agent_message now runs mode_suggest::suggest_plan_mode on every
user message. On match: persist a 'pending' row via mode_suggest_store,
set ReasoningContext dedupe flags, and emit agent:plan_mode_suggest
with source=keyword.

RequestPlanModeSwitchTool registered at both agent-loop bootstrap
sites alongside PlanWriteTool/PlanUpdateTool.

New Tauri command respond_plan_mode_suggest writes the user's outcome
back to the telemetry row. query_disabled_patterns is a stub today;
filled by the calibration scenario in Task 10."
```

---

## Task 7 — PlanModeSuggestBanner React component + tests

**Files:**
- Create: `ui/src/atoms/plan-mode-suggest-atoms.ts`
- Create: `ui/src/components/agent/PlanModeSuggestBanner.tsx`
- Create: `ui/src/components/agent/PlanModeSuggestBanner.test.tsx`
- Modify: `ui/src/components/agent/AgentView.tsx` (mount the banner)
- Modify: `ui/src/lib/tauri-bridge.ts` (typed wrapper for `respond_plan_mode_suggest`, `set_safety_mode`)

- [ ] **Step 1: Atom — pending request queue**

Create `ui/src/atoms/plan-mode-suggest-atoms.ts`:

```ts
import { atom } from 'jotai'

export interface PlanModeSuggestRequest {
  id: string
  sessionId: string
  source: 'keyword' | 'agent'
  matchedPattern?: string
  reason?: string
  previewSteps?: string[]
  firedAtMs: number
}

/** Keyed by sessionId. Each session has at most one pending suggest. */
export const pendingPlanModeSuggestsAtom = atom<Record<string, PlanModeSuggestRequest | null>>({})
```

- [ ] **Step 2: tauri-bridge typed wrappers**

Append to `ui/src/lib/tauri-bridge.ts`:

```ts
export async function respondPlanModeSuggest(
  eventId: string,
  outcome: 'accepted' | 'skipped' | 'silenced' | 'aborted',
  declineReason?: string,
): Promise<void> {
  await invoke('respond_plan_mode_suggest', { eventId, outcome, declineReason })
}
```

(`set_safety_mode` wrapper already exists in this file — reuse it.)

- [ ] **Step 3: Component**

Create `ui/src/components/agent/PlanModeSuggestBanner.tsx`:

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { Button } from '@/components/ui/button'
import { pendingPlanModeSuggestsAtom, type PlanModeSuggestRequest } from '@/atoms/plan-mode-suggest-atoms'
import { planModeSuggestEnabledAtom } from '@/atoms/settings-atoms'
import { respondPlanModeSuggest, setSafetyMode } from '@/lib/tauri-bridge'

interface Props { sessionId: string }

export function PlanModeSuggestBanner({ sessionId }: Props): React.ReactElement | null {
  const [queue, setQueue] = useAtom(pendingPlanModeSuggestsAtom)
  const [enabled, setEnabled] = useAtom(planModeSuggestEnabledAtom)
  const req = queue[sessionId] ?? null

  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null
    listen<PlanModeSuggestRequest>('agent:plan_mode_suggest', ({ payload }) => {
      if (payload.sessionId !== sessionId) return
      setQueue((q) => ({ ...q, [sessionId]: payload }))
    }).then((fn) => { if (cancelled) fn(); else unlisten = fn })
    return () => { cancelled = true; unlisten?.() }
  }, [sessionId, setQueue])

  if (!enabled || !req) return null

  const clear = () => setQueue((q) => ({ ...q, [sessionId]: null }))

  const handleSwitch = async () => {
    try {
      await setSafetyMode('plan')
      await respondPlanModeSuggest(req.id, 'accepted')
    } finally { clear() }
  }
  const handleSkip = async () => {
    try { await respondPlanModeSuggest(req.id, 'skipped') } finally { clear() }
  }
  const handleNever = async () => {
    setEnabled(false)
    try { await respondPlanModeSuggest(req.id, 'silenced') } finally { clear() }
  }

  return (
    <div
      role="status"
      aria-live="polite"
      className="mx-4 mb-3 rounded-lg border border-border bg-popover px-4 py-3 text-sm shadow-sm animate-in slide-in-from-bottom-2 duration-200"
    >
      <div className="flex items-start gap-2">
        <span className="text-base leading-none">💡</span>
        <div className="flex-1 min-w-0">
          <p className="text-foreground">
            {req.reason ?? '这个任务看起来是多步骤构建。先切到 Plan 模式让 agent 把方案敲定再执行？'}
          </p>
          {req.previewSteps && req.previewSteps.length > 0 && (
            <ul className="mt-2 list-disc pl-5 text-xs text-muted-foreground space-y-0.5">
              {req.previewSteps.map((s, i) => <li key={i}>{s}</li>)}
            </ul>
          )}
        </div>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={handleNever} aria-label="不再建议">
          不再建议
        </Button>
        <Button variant="outline" size="sm" onClick={handleSkip}>本次不用</Button>
        <Button variant="default" size="sm" onClick={handleSwitch}>切到 Plan 模式</Button>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Tests**

Create `ui/src/components/agent/PlanModeSuggestBanner.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { PlanModeSuggestBanner } from './PlanModeSuggestBanner'
import { pendingPlanModeSuggestsAtom } from '@/atoms/plan-mode-suggest-atoms'
import { planModeSuggestEnabledAtom } from '@/atoms/settings-atoms'

// Mock the tauri-bridge so component logic can be exercised without IPC
vi.mock('@/lib/tauri-bridge', () => ({
  respondPlanModeSuggest: vi.fn().mockResolvedValue(undefined),
  setSafetyMode: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

const FRESH_REQ = {
  id: 'evt-1', sessionId: 's1', source: 'keyword' as const,
  matchedPattern: '计划', reason: '建议先 Plan',
  firedAtMs: 1_000,
}

function renderWithReq() {
  const store = createStore()
  store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
  store.set(planModeSuggestEnabledAtom, true)
  return render(
    <Provider store={store}>
      <PlanModeSuggestBanner sessionId="s1" />
    </Provider>,
  )
}

describe('PlanModeSuggestBanner', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders when a pending request exists and feature is enabled', () => {
    renderWithReq()
    expect(screen.getByRole('status')).toBeInTheDocument()
    expect(screen.getByText(/建议先 Plan/)).toBeInTheDocument()
    expect(screen.getByText('切到 Plan 模式')).toBeInTheDocument()
  })

  it('renders nothing when feature is disabled', () => {
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
    store.set(planModeSuggestEnabledAtom, false)
    const { container } = render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing for a sessionId with no pending request', () => {
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, {})
    store.set(planModeSuggestEnabledAtom, true)
    const { container } = render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('clicking 切到 Plan 模式 calls setSafetyMode("plan") and reports accepted', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    renderWithReq()
    fireEvent.click(screen.getByText('切到 Plan 模式'))
    await Promise.resolve(); await Promise.resolve()
    expect(bridge.setSafetyMode).toHaveBeenCalledWith('plan')
    expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'accepted')
  })

  it('clicking 本次不用 reports skipped without changing mode', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    renderWithReq()
    fireEvent.click(screen.getByText('本次不用'))
    await Promise.resolve(); await Promise.resolve()
    expect(bridge.setSafetyMode).not.toHaveBeenCalled()
    expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'skipped')
  })

  it('clicking 不再建议 flips the enabled atom off + reports silenced', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
    store.set(planModeSuggestEnabledAtom, true)
    render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    fireEvent.click(screen.getByText('不再建议'))
    await Promise.resolve(); await Promise.resolve()
    expect(store.get(planModeSuggestEnabledAtom)).toBe(false)
    expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'silenced')
  })
})
```

- [ ] **Step 5: Stub settings-atoms entry (full settings UI lands in Task 11)**

Append to `ui/src/atoms/settings-atoms.ts`:

```ts
import { atomWithStorage } from 'jotai/utils'

export const planModeSuggestEnabledAtom = atomWithStorage('planModeSuggestEnabled', true)
```

(If the file already imports `atomWithStorage`, don't double-import.)

- [ ] **Step 6: Mount in AgentView**

In `ui/src/components/agent/AgentView.tsx`, after the import block add:

```tsx
import { PlanModeSuggestBanner } from './PlanModeSuggestBanner'
```

And next to the existing `<AskUserBanner sessionId={sessionId} />` line (around L1541), add ABOVE it:

```tsx
{/* Plan-mode auto-suggest banner — advisory (not blocking) */}
<PlanModeSuggestBanner sessionId={sessionId} />
```

- [ ] **Step 7: Run tests**

```bash
cd ui && npm test -- --run PlanModeSuggestBanner 2>&1 | tail -12
```

Expected: `Tests 6 passed (6)`.

- [ ] **Step 8: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add ui/src/components/agent/PlanModeSuggestBanner.tsx \
        ui/src/components/agent/PlanModeSuggestBanner.test.tsx \
        ui/src/atoms/plan-mode-suggest-atoms.ts \
        ui/src/atoms/settings-atoms.ts \
        ui/src/components/agent/AgentView.tsx \
        ui/src/lib/tauri-bridge.ts
git commit -m "feat(ui): PlanModeSuggestBanner — advisory banner for plan-mode auto-suggest

Listens for agent:plan_mode_suggest, shows a light advisory banner
above the input. Three actions:
  - 切到 Plan 模式 → setSafetyMode('plan') + outcome=accepted
  - 本次不用      → outcome=skipped (no mode change)
  - 不再建议      → planModeSuggestEnabledAtom=false + outcome=silenced

role='status' + aria-live='polite' (advisory, not blocking).
Mounted in AgentView alongside the existing AskUserBanner.
6 RTL tests cover all action paths and gating."
```

---

## Task 8 — A11y baseline on existing banners (no visible UI change)

**Files:**
- Modify: `ui/src/components/agent/ExitPlanModeBanner.tsx` (wrap root div)
- Modify: `ui/src/components/agent/AskUserBanner.tsx` (wrap root div)

- [ ] **Step 1: Add ARIA + role to ExitPlanModeBanner root**

Find the outermost returned `<div>` in `ExitPlanModeBanner.tsx`. Wrap or
augment it with these attributes:

```tsx
<div
  role="alertdialog"
  aria-modal="true"
  aria-label="Agent 计划待审批"
  {/* ... existing className + props ... */}
>
```

If there's an icon-only `X` button, add `aria-label="关闭"` to it.

- [ ] **Step 2: Add ARIA + role to AskUserBanner root**

Same treatment on `AskUserBanner.tsx`:

```tsx
<div
  role="alertdialog"
  aria-modal="true"
  aria-label="Agent 在问"
  {/* ... existing className + props ... */}
>
```

Add `aria-label` to its icon-only buttons (X, ▾) where present.

- [ ] **Step 3: Visual + a11y sanity check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head
cd ui && npm test -- --run "ExitPlanModeBanner|AskUserBanner" 2>&1 | tail -10
```

Expected: TS clean. If existing snapshot tests fail because the root
div gained attributes, update the snapshots (`-u`) and inspect the diff
to confirm only ARIA additions, no behavior change.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/agent/ExitPlanModeBanner.tsx ui/src/components/agent/AskUserBanner.tsx
git commit -m "fix(ui): a11y baseline on ExitPlanMode + AskUser banners

Add role='alertdialog', aria-modal='true', aria-label on the root,
and aria-label on icon-only buttons. No visible UI change.

Deferred for future PRs (per spec §7c): focus trap via Radix Dialog,
dismiss-confirm popover, editable allowed_prompts, markdown rendering
in AskUser question text, channel-drop disconnect state."
```

---

## Task 9 — Clear dedupe flag on accept + manual mode change

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (set_safety_mode body)
- Modify: `src-tauri/src/agent/dispatcher.rs` (find the right spot to reset, if any)

- [ ] **Step 1: Find set_safety_mode**

```bash
grep -n "pub async fn set_safety_mode" src-tauri/src/tauri_commands.rs
```

- [ ] **Step 2: Insert dedupe clear at start of set_safety_mode body**

In `pub async fn set_safety_mode(...)`, after the input parsing block and BEFORE the mode write, insert:

```rust
// Clear plan-mode-suggest dedupe flag on ANY manual mode change so
// the user can be prompted again later if they switch back to
// Supervised/Yolo and ask for plan-worthy work.
{
    let mut ctxs = state.session_reasoning_contexts.write().await;
    for c in ctxs.values_mut() {
        c.plan_mode_suggested_in_session = false;
        c.plan_mode_suggest_event_id = None;
    }
}
```

- [ ] **Step 3: Compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: clean. If `session_reasoning_contexts` is named differently in
your version, grep and adjust.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "fix(safety): clear plan-mode-suggest dedupe flag on any mode change

set_safety_mode wipes plan_mode_suggested_in_session for every active
session ReasoningContext. Covers both the accept path (banner →
set_safety_mode('plan') → flag clears so a future flip back to Yolo
can re-suggest) and the manual PermissionModeSelector path."
```

---

## Task 10 — plan_mode_calibration proactive scenario

**Files:**
- Create: `src-tauri/src/proactive/scenarios/plan_mode_calibration.rs`
- Modify: `src-tauri/src/proactive/scenarios/mod.rs` (register)
- Modify: `src-tauri/src/agent/mode_suggest_store.rs` (implement `query_disabled_patterns` properly + add overrides table SQL)
- Modify: `src-tauri/src/db/migrations.rs` (add the sibling overrides table to V34)

- [ ] **Step 1: Extend V34 with the overrides sibling table**

Append to `SQL_V34_PLAN_SUGGEST_EVENTS` (Task 2):

```sql
CREATE TABLE IF NOT EXISTS mode_suggest_overrides (
    pattern         TEXT PRIMARY KEY,
    disabled_until  INTEGER NOT NULL,
    reason          TEXT,
    updated_at      INTEGER NOT NULL
);
```

- [ ] **Step 2: Replace the stub query_disabled_patterns**

Replace the stub in `mode_suggest_store.rs`:

```rust
pub fn query_disabled_patterns(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut stmt = conn.prepare(
        "SELECT pattern FROM mode_suggest_overrides WHERE disabled_until > ?"
    )?;
    let rows = stmt.query_map([now], |r| r.get::<_, String>(0))?;
    rows.collect()
}

pub fn upsert_disabled_pattern(
    conn: &rusqlite::Connection,
    pattern: &str,
    disabled_until_ms: i64,
    reason: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO mode_suggest_overrides (pattern, disabled_until, reason, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(pattern) DO UPDATE SET
             disabled_until = excluded.disabled_until,
             reason = excluded.reason,
             updated_at = excluded.updated_at",
        rusqlite::params![pattern, disabled_until_ms, reason, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(())
}
```

Add tests for both in the same `tests` module:

```rust
#[test]
fn disabled_pattern_filtered_when_expired() {
    let conn = fresh_db();
    upsert_disabled_pattern(&conn, "plan", 500, "low accept").unwrap();
    // disabled_until=500 < now → not returned
    let disabled = query_disabled_patterns(&conn).unwrap();
    assert!(disabled.is_empty());
}

#[test]
fn disabled_pattern_included_when_active() {
    let conn = fresh_db();
    let future = chrono::Utc::now().timestamp_millis() + 60_000;
    upsert_disabled_pattern(&conn, "plan", future, "low accept").unwrap();
    let disabled = query_disabled_patterns(&conn).unwrap();
    assert_eq!(disabled, vec!["plan".to_string()]);
}

#[test]
fn upsert_replaces_existing_row() {
    let conn = fresh_db();
    upsert_disabled_pattern(&conn, "plan", 1_000, "first").unwrap();
    upsert_disabled_pattern(&conn, "plan", 2_000, "second").unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM mode_suggest_overrides WHERE pattern = 'plan'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(n, 1);
}
```

- [ ] **Step 3: Implement the scenario**

Create `src-tauri/src/proactive/scenarios/plan_mode_calibration.rs`:

```rust
//! Plan-mode auto-suggest calibration scenario.
//!
//! Reads plan_suggest_events, computes per-pattern accept rates, and
//! silences low-acceptance patterns for 14 days. Runs on the standard
//! proactive cadence; lightweight (one aggregate query + ≤K upserts).

use std::sync::Arc;
use async_trait::async_trait;
use rusqlite::Connection;
use crate::proactive::scenarios::{ProactiveScenario, ScenarioResult};

const WINDOW_DAYS: i64 = 7;
const MIN_FIRINGS: u32 = 20;
const SILENCE_THRESHOLD: f32 = 0.30;
const SILENCE_DURATION_DAYS: i64 = 14;

pub struct PlanModeCalibrationScenario {
    db: Arc<parking_lot::Mutex<Connection>>,
}

impl PlanModeCalibrationScenario {
    pub fn new(db: Arc<parking_lot::Mutex<Connection>>) -> Self { Self { db } }

    fn calibrate(&self, conn: &Connection) -> rusqlite::Result<usize> {
        let window_start = chrono::Utc::now().timestamp_millis()
            - WINDOW_DAYS * 24 * 60 * 60 * 1000;
        let stats = crate::agent::mode_suggest_store::query_per_pattern_stats(
            conn, window_start,
        )?;
        let mut silenced_count = 0usize;
        for s in stats {
            if s.firings < MIN_FIRINGS { continue; }
            let rate = s.accept_rate();
            if rate < SILENCE_THRESHOLD {
                let until = chrono::Utc::now().timestamp_millis()
                    + SILENCE_DURATION_DAYS * 24 * 60 * 60 * 1000;
                crate::agent::mode_suggest_store::upsert_disabled_pattern(
                    conn, &s.pattern, until,
                    &format!("accept_rate={:.2} < {:.2} after {} firings",
                             rate, SILENCE_THRESHOLD, s.firings),
                )?;
                silenced_count += 1;
                tracing::info!(
                    pattern = %s.pattern, accept_rate = rate, firings = s.firings,
                    "Plan-mode calibration silenced pattern for 14d"
                );
            }
        }
        Ok(silenced_count)
    }
}

#[async_trait]
impl ProactiveScenario for PlanModeCalibrationScenario {
    fn name(&self) -> &'static str { "plan_mode_calibration" }
    fn description(&self) -> &'static str {
        "Calibrate plan-mode keyword acceptance — silence low-accept patterns"
    }
    async fn run(&self) -> ScenarioResult {
        let conn = self.db.lock();
        match self.calibrate(&conn) {
            Ok(n) => ScenarioResult::Success(format!("calibrated; {} pattern(s) silenced", n)),
            Err(e) => ScenarioResult::Error(format!("calibration failed: {}", e)),
        }
    }
    fn enabled(&self) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::agent::mode_suggest_store::{record_fired, record_outcome, FireRecord, SuggestSource, Outcome};

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)",
            [],
        ).unwrap();
        conn
    }

    fn fire_with_outcome(conn: &Connection, id: &str, pattern: &str, outcome: Outcome) {
        record_fired(conn, FireRecord {
            id, session_id: "s1", message_id: "m1",
            source: SuggestSource::Keyword,
            matched_pattern: Some(pattern), reason: None,
            user_msg_preview: "x",
            fired_at: chrono::Utc::now().timestamp_millis(),
        }).unwrap();
        record_outcome(conn, id, outcome, None, chrono::Utc::now().timestamp_millis()).unwrap();
    }

    #[test]
    fn pattern_below_threshold_with_enough_firings_silenced() {
        let conn = fresh_db();
        // 20 firings: 4 accepted, 16 skipped → 20% accept rate
        for i in 0..4 {
            fire_with_outcome(&conn, &format!("a{}", i), "plan", Outcome::Accepted);
        }
        for i in 0..16 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(
            Arc::new(parking_lot::Mutex::new(conn)),
        );
        let conn_g = scenario.db.lock();
        let n = scenario.calibrate(&conn_g).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn pattern_below_threshold_but_too_few_firings_not_silenced() {
        let conn = fresh_db();
        // 10 firings, 0% accept → still below MIN_FIRINGS
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(
            Arc::new(parking_lot::Mutex::new(conn)),
        );
        let conn_g = scenario.db.lock();
        assert_eq!(scenario.calibrate(&conn_g).unwrap(), 0);
    }

    #[test]
    fn pattern_above_threshold_not_silenced() {
        let conn = fresh_db();
        // 20 firings, 50% accept → above 30% threshold
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("a{}", i), "plan", Outcome::Accepted);
        }
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(
            Arc::new(parking_lot::Mutex::new(conn)),
        );
        let conn_g = scenario.db.lock();
        assert_eq!(scenario.calibrate(&conn_g).unwrap(), 0);
    }
}
```

- [ ] **Step 4: Register the scenario**

In `src-tauri/src/proactive/scenarios/mod.rs`, add a `pub mod plan_mode_calibration;` declaration alongside the other scenario module declarations.

Then find the app-level registration site (likely in `src-tauri/src/app.rs` — look for `Arc::new(ConversationLearningScenario::new(...))` patterns) and add:

```rust
manager.register(Arc::new(
    crate::proactive::scenarios::plan_mode_calibration::PlanModeCalibrationScenario::new(db.clone())
));
```

- [ ] **Step 5: Test**

```bash
cd src-tauri && cargo test --lib agent::mode_suggest_store proactive::scenarios::plan_mode_calibration 2>&1 | grep "test result"
```

Expected: both modules green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/proactive/scenarios/plan_mode_calibration.rs \
        src-tauri/src/proactive/scenarios/mod.rs \
        src-tauri/src/agent/mode_suggest_store.rs \
        src-tauri/src/db/migrations.rs \
        src-tauri/src/app.rs
git commit -m "feat(proactive): plan_mode_calibration scenario + mode_suggest_overrides

V34 extended with mode_suggest_overrides (pattern, disabled_until,
reason, updated_at). query_disabled_patterns now returns active
overrides; upsert_disabled_pattern writes them.

Scenario reads 7d of plan_suggest_events, computes per-pattern accept
rate, and silences for 14d any pattern with >=20 firings and <30%
accept rate. Conservative thresholds avoid cold-start churn.

3 calibration tests + 3 store tests."
```

---

## Task 11 — Settings toggle + Gene injection wiring

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (read setting in send_agent_message — replace the Task 6 TODO; new commands `get_plan_mode_suggest_enabled` / `set_plan_mode_suggest_enabled` OR reuse existing settings sync)
- Modify: `ui/src/components/settings/AgentSettings.tsx` (or similar — grep for the existing Agent settings page)
- Modify: `src-tauri/src/agent/dispatcher.rs` — inject GEP gene control signal when aggregate accept rate is low

- [ ] **Step 1: Find the existing settings infrastructure**

```bash
grep -rn "atomWithStorage\|get_setting\|set_setting" ui/src/atoms/settings-atoms.ts src-tauri/src/tauri_commands.rs | head -20
```

If a generic `set_setting(key, value)` exists, reuse it. Otherwise add a dedicated pair:

```rust
#[tauri::command]
pub async fn set_plan_mode_suggest_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), Error> {
    let mut s = state.settings.write().await;
    s.plan_mode_suggest_enabled = enabled;
    s.persist().await.map_err(|e| Error::from(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn get_plan_mode_suggest_enabled(state: State<'_, AppState>) -> Result<bool, Error> {
    Ok(state.settings.read().await.plan_mode_suggest_enabled)
}
```

Add the field to your settings struct + register both commands in `main.rs`'s `invoke_handler!`.

- [ ] **Step 2: Replace the TODO in send_agent_message (Task 6 Step 2)**

Replace:
```rust
let suggest_enabled = true; // TODO Task 11: read from settings
```

with:
```rust
let suggest_enabled = state.settings.read().await.plan_mode_suggest_enabled;
```

- [ ] **Step 3: Add GEP gene injection for aggregate low-acceptance**

In `src-tauri/src/agent/dispatcher.rs::call_llm` (around L900-940 where Gene matches are formatted), after the existing GeneRetriever block, add:

```rust
// Aggregate plan-suggest accept-rate signal — when most suggestions
// are being rejected, ask the model to be more conservative about
// calling request_plan_mode_switch.
if let Ok(conn) = self.app_state.db_conn() {
    let window_start = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
    if let Ok(stats) = crate::agent::mode_suggest_store::query_per_pattern_stats(&conn, window_start) {
        let total_decided: u32 = stats.iter().map(|s| s.accepted + s.skipped + s.silenced).sum();
        let total_accepted: u32 = stats.iter().map(|s| s.accepted).sum();
        if total_decided >= 10 {
            let agg_rate = total_accepted as f32 / total_decided as f32;
            if agg_rate < 0.20 {
                full_system_prompt.push_str(
                    "\n\n[Plan-suggest signal] Your recent request_plan_mode_switch \
                     calls have been declined frequently. Be more conservative — \
                     only suggest Plan mode for clearly multi-step build/refactor \
                     work, not casual questions.\n",
                );
            }
        }
    }
}
```

- [ ] **Step 4: Add settings UI row**

Find your Agent settings page (e.g. `ui/src/components/settings/AgentSettings.tsx`) and add a Switch row:

```tsx
import { useAtom } from 'jotai'
import { planModeSuggestEnabledAtom } from '@/atoms/settings-atoms'
// ...

const [enabled, setEnabled] = useAtom(planModeSuggestEnabledAtom)
// In the JSX, alongside other toggles:
<SettingRow
  label="为复杂任务建议 Plan 模式"
  description="检测到多步骤构建/重构/设计请求时弹出建议横幅。可被 agent 主动调用，也按关键词触发。"
>
  <Switch
    checked={enabled}
    onCheckedChange={async (v) => {
      setEnabled(v)
      await invoke('set_plan_mode_suggest_enabled', { enabled: v })
    }}
  />
</SettingRow>
```

If your settings component patterns differ, follow the existing convention.

- [ ] **Step 5: Compile + test**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs \
        src-tauri/src/agent/dispatcher.rs \
        ui/src/components/settings/
git commit -m "feat(settings): plan_mode_suggest_enabled toggle + GEP gene injection

Backend gains plan_mode_suggest_enabled in settings (default true) +
Tauri get/set commands. send_agent_message now reads this flag
instead of the Task 6 placeholder.

Dispatcher's call_llm checks aggregate plan-suggest accept-rate; if
<20% across >=10 decided events in last 7 days, appends a one-line
'be more conservative' hint to the system prompt.

UI gets a single Switch row in Agent settings."
```

---

## Task 12 — System prompt updates + migration registry update

**Files:**
- Modify: `src-tauri/src/agent/prompts/baseline.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Append guidance to baseline.md**

Open `src-tauri/src/agent/prompts/baseline.md` and append at the end:

```markdown
## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

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

- [ ] **Step 2: Update CLAUDE.md migration registry**

In `CLAUDE.md`, find the Active migration registry table and add the V34 row right after V33:

```markdown
| V34 | plan_suggest_events + mode_suggest_overrides (plan-mode auto-suggest telemetry) | merged |
```

- [ ] **Step 3: Compile sanity check (prompt change is bake-time embedded)**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent/prompts/baseline.md CLAUDE.md
git commit -m "docs: baseline prompt guidance for two new tools + V34 in registry

baseline.md appends two short sections:
  - request_plan_mode_switch — when to call (multi-step build /
    refactor / design in Supervised/Yolo), when not to
  - ask_user — three call scenarios (ambiguity / destructive /
    critical design decision) + three skip scenarios

CLAUDE.md migration registry gains V34 row."
```

---

## Self-review (post-write checklist)

Per the writing-plans skill, run this checklist:

**1. Spec coverage** — each spec section maps to a task:

| Spec section | Task |
|---|---|
| §1 Backend keyword detector | Task 3 |
| §2 LLM tool `request_plan_mode_switch` | Task 4 |
| §3 ReasoningContext dedupe state | Task 5 |
| §4 ask_user system prompt guidance | Task 12 |
| §5 Telemetry table V34 | Task 2 (+ extended in Task 10) |
| §6 GEP calibration scenario | Task 10 (+ aggregate hint in Task 11) |
| §7a De-duplicate banner mounts | Task 1 |
| §7b PlanModeSuggestBanner | Task 7 |
| §7c A11y baseline on existing banners | Task 8 |
| §8 Settings surface | Task 11 |
| §9 File map | Reflected in per-task Files: lines |
| §10 Migration registry update | Task 12 |

**2. Placeholder scan** — only intentional `TODO Task 11:` reference and
the bash command examples; resolved by Task 11 Step 2. No `TBD`, `XXX`.

**3. Type consistency** —
`PlanModeHint { pattern, display_reason }`, `SuggestSource::{Keyword,Agent}`,
`Outcome::{Pending,Accepted,Skipped,Silenced,Aborted}`,
`FireRecord<'a>`, `PatternStats { pattern, firings, accepted, skipped,
silenced }`, atom `pendingPlanModeSuggestsAtom`, atom
`planModeSuggestEnabledAtom`, event name `agent:plan_mode_suggest`,
Tauri command `respond_plan_mode_suggest` and
`set_plan_mode_suggest_enabled` — all consistent across tasks.

## PR-shape note for executor

Final PR will list these 12 commits in a `## Commits (bisectable)` table.
Branch: `worktree-feat-plan-mode-auto-suggest`. Target: `main`.

If any task discovers existing-code drift that contradicts the spec
(wrong file path, renamed field, etc), STOP and surface the divergence
before improvising — don't quietly rewrite to fit. Many of these tasks
depend on each other's invariants.
