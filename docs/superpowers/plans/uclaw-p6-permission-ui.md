# P6 — Tool Permission UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the tool-approval surface from "global allow-list + per-tool override" to a real permission center: per-session overrides, command-pattern allow rules ("always allow `bash rm -rf /tmp/*`"), and a queryable audit log of every decision. Land a Settings → 工具权限 tab to manage rules + browse the audit log, plus a 4-button approval modal.

**What already exists** (don't rebuild):

| Feature | Status |
|---|---|
| `SafetyManager` + `SafetyMode` (Ask/Supervised/Yolo) | ✅ `src-tauri/src/safety/mod.rs` |
| Per-tool `tool_overrides` (HashMap) | ✅ persisted in `safety_policy.json` |
| `auto_approved_tools` whitelist | ✅ same |
| `blocked_tools` blocklist | ✅ same |
| `should_approve` resolution chain | ✅ blocked → Never → whitelist → mode |
| 3-button approval modal (拒绝 / 始终允许 / 批准) | ✅ `ApprovalModal.tsx` — `alwaysAllow=true` adds tool to whitelist |
| `approve_tool_call` IPC + `pending_approvals` channel | ✅ `tauri_commands.rs:2487` |

**What's missing:**

| Feature | Where it lands |
|---|---|
| Per-session overrides | `tool_permission_rules.scope='session'` |
| Command-pattern rules ("bash starting with `git status`") | `tool_permission_rules.scope='pattern'` |
| Audit log of decisions | `permission_audit_log` |
| Settings tab to manage all of the above | `PermissionsSettings.tsx` |
| 4th button — "本次会话允许" | `ApprovalModal.tsx` rewrite |

**Architecture:**

- **DB**: V14 adds `tool_permission_rules` and `permission_audit_log`. Both indexed on the columns the resolver / table view need.
- **Backend**: new `safety/permissions.rs` module with `resolve_decision(state, session_id, tool_name, command, ApprovalRequirement) → ApprovalDecision`. Resolution precedence: **session rule → pattern rule → tool override → global mode**. Each call writes one row to `permission_audit_log`. The existing `SafetyPolicy` JSON (tool_overrides + auto_approved + blocked) is **kept** — it's the "global tier" of the new resolver, no migration of values needed.
- **Wiring**: `should_approve` becomes a thin shim that calls `resolve_decision`. Existing call sites unchanged.
- **Frontend**: Settings → 工具权限 tab with three sub-sections (rules table, audit log table, current-session chip). Modal grows a 4th button "本次会话允许" that creates a session rule for the matched tool.

**Tech Stack:** No new deps — reuses existing rusqlite + Tauri commands + cmdk-style settings panel. Audit log table uses native `<table>` like the cost-dashboard session table.

**Reference:** Roadmap §P6 at `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md:286`.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p6-permission-ui
```

- [ ] **Step 0.2: Baseline pipeline**

```bash
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, ~188 backend tests passing, 0 TS errors, ~45 frontend tests passing.

---

## Task 1: V14 schema — rules + audit log

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `V14_PERMISSION_TABLES` + apply in `run`)

- [ ] **Step 1.1: Add migration constant**

Edit `src-tauri/src/db/migrations.rs`. Add **after** `V13_COST_RECORDS`:

```rust
/// V14: tool permission rules + audit log.
///
/// `tool_permission_rules` extends the existing safety_policy.json model
/// (which stays as the "global tier") with two new scopes:
///   - 'session' — only for the named session_id; cleared on session delete
///   - 'pattern' — for tools whose first-arg / command matches `target` as a
///     simple prefix (kept simple on purpose; regex is YAGNI)
/// Resolution precedence in safety/permissions.rs: session > pattern > tool > global.
///
/// `permission_audit_log` records every decision the resolver makes so the
/// settings UI can show a per-session table.
pub const V14_PERMISSION_TABLES: &str = "
CREATE TABLE IF NOT EXISTS tool_permission_rules (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL CHECK(scope IN ('session', 'pattern')),
    session_id  TEXT,
    tool_name   TEXT NOT NULL,
    target      TEXT,
    mode        TEXT NOT NULL CHECK(mode IN ('allow', 'block', 'ask')),
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_session
    ON tool_permission_rules(session_id, tool_name);
CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_pattern
    ON tool_permission_rules(scope, tool_name)
    WHERE scope = 'pattern';

CREATE TABLE IF NOT EXISTS permission_audit_log (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    args_hash   TEXT NOT NULL,
    decision    TEXT NOT NULL CHECK(decision IN ('auto_approve', 'user_approve', 'user_deny', 'blocked')),
    rule_id     TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_permission_audit_session ON permission_audit_log(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_permission_audit_tool    ON permission_audit_log(tool_name, created_at DESC);
";
```

- [ ] **Step 1.2: Apply in `run`**

Edit `run` in the same file. Add **after** the V13 block:

```rust
    // V14: per-session + pattern rules + audit log.
    tracing::debug!("Running migration V14: permission tables");
    for stmt in V14_PERMISSION_TABLES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V14 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 1.3: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors.

- [ ] **Step 1.4: Update CLAUDE.md migration registry**

Edit `CLAUDE.md`. Find the "Active migration registry" table (Part 2). Add row:

```markdown
| V14 | tool_permission_rules + permission_audit_log | **PR (this one)** |
```

(After the PR merges, the next contributor will mark it `merged`.)

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/db/migrations.rs CLAUDE.md
git commit -m "$(cat <<'EOF'
feat(safety): V14 — tool_permission_rules + permission_audit_log

Two new tables backing P6's permission center:

  - tool_permission_rules — extends safety_policy.json (which stays as
    the "global tier") with two new scopes: 'session' (per-conversation)
    and 'pattern' (tool + arg-prefix). Resolution precedence
    session > pattern > tool > global lands in the next commit.
  - permission_audit_log — one row per decision (auto / user-approve /
    user-deny / blocked) so the settings UI can show per-session history.

Idempotent CREATE IF NOT EXISTS. No data migration — existing
safety_policy.json survives unchanged.
EOF
)"
```

---

## Task 2: Backend — `safety/permissions.rs` resolver

**Files:**
- Create: `src-tauri/src/safety/permissions.rs`
- Modify: `src-tauri/src/safety/mod.rs` (`pub mod permissions;` + thread-through)
- Modify: `src-tauri/src/agent/dispatcher.rs` (call site that already calls `should_approve`)

The resolver replaces the inline `should_approve` flow without breaking call sites. Existing `SafetyManager::should_approve` becomes a shim that delegates to `permissions::resolve_decision` if a DB handle is available, falling back to the old in-memory logic if not (keeps unit tests of `should_approve` working).

- [ ] **Step 2.1: Add IPC types**

Edit `src-tauri/src/ipc.rs`. Add at the end:

```rust
// ─── Permission rules ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub id: String,
    /// "session" | "pattern"
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub tool_name: String,
    /// For scope='pattern': the arg/command prefix to match. None for session scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// "allow" | "block" | "ask"
    pub mode: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAuditEntry {
    pub id: String,
    pub session_id: String,
    pub tool_name: String,
    pub args_hash: String,
    /// "auto_approve" | "user_approve" | "user_deny" | "blocked"
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePermissionRuleInput {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub mode: String,
}
```

- [ ] **Step 2.2: Resolver module**

Create `src-tauri/src/safety/permissions.rs`:

```rust
//! DB-backed rule resolver for tool permissions.
//!
//! Resolution precedence (first match wins):
//!   1. Session rule for this (session_id, tool_name)
//!   2. Pattern rule whose `target` is a prefix of the call's first-arg /
//!      command and whose `tool_name` matches
//!   3. Tool-level override (existing safety_policy.tool_overrides)
//!   4. Global mode (existing safety_policy.global_mode)
//!
//! Each `resolve_decision` call writes one row to `permission_audit_log`.
//! Audit failures are logged + swallowed — they must never break the agent loop.

use crate::agent::tools::tool::ApprovalRequirement;
use crate::ipc::{PermissionAuditEntry, PermissionRule, CreatePermissionRuleInput};
use crate::safety::{ApprovalDecision, SafetyMode, SafetyPolicy};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Hash the call's arguments for the audit log. SHA-256 truncated to 16 hex
/// chars — enough to dedupe similar calls in the UI without storing PII.
fn args_hash(args: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(args).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    hash.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// Extract the "command-like" prefix from a tool call for pattern matching.
/// For shell tools we look at `command`; otherwise we use the first string-
/// valued argument. Returns None if nothing meaningful is present.
fn pattern_target(args: &serde_json::Value) -> Option<String> {
    let obj = args.as_object()?;
    // Prefer common command-y keys
    for k in &["command", "cmd", "input", "path", "url"] {
        if let Some(v) = obj.get(*k).and_then(|v| v.as_str()) {
            return Some(v.to_string());
        }
    }
    // Fall back to the first string value
    obj.values().find_map(|v| v.as_str().map(String::from))
}

pub fn resolve_decision(
    db: &Arc<Mutex<Connection>>,
    policy: &SafetyPolicy,
    session_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    tool_approval: &ApprovalRequirement,
    session_mode_override: Option<&SafetyMode>,
) -> ApprovalDecision {
    // 0. Hard checks that always win — block list + Never tool
    if policy.blocked_tools.contains(tool_name) {
        let decision = ApprovalDecision::Block {
            reason: format!("Tool '{}' is blocked by safety policy", tool_name),
        };
        log_audit(db, session_id, tool_name, arguments, &decision, None);
        return decision;
    }
    if *tool_approval == ApprovalRequirement::Never {
        let decision = ApprovalDecision::AutoApprove;
        log_audit(db, session_id, tool_name, arguments, &decision, None);
        return decision;
    }
    if policy.auto_approved_tools.contains(tool_name) {
        let decision = ApprovalDecision::AutoApprove;
        log_audit(db, session_id, tool_name, arguments, &decision, None);
        return decision;
    }

    // 1. Session rule
    if let Some(rule) = lookup_session_rule(db, session_id, tool_name) {
        let decision = mode_to_decision(&rule.mode, tool_name);
        log_audit(db, session_id, tool_name, arguments, &decision, Some(&rule.id));
        return decision;
    }

    // 2. Pattern rule
    if let Some(target) = pattern_target(arguments) {
        if let Some(rule) = lookup_pattern_rule(db, tool_name, &target) {
            let decision = mode_to_decision(&rule.mode, tool_name);
            log_audit(db, session_id, tool_name, arguments, &decision, Some(&rule.id));
            return decision;
        }
    }

    // 3. Tool override + 4. Global — same as before, but routed here for audit
    let effective_mode = session_mode_override
        .or_else(|| policy.tool_overrides.get(tool_name))
        .unwrap_or(&policy.global_mode);

    let decision = match effective_mode {
        SafetyMode::Yolo => ApprovalDecision::AutoApprove,
        SafetyMode::Ask => ApprovalDecision::RequireApproval {
            reason: format!("Safety mode requires approval for tool '{}'", tool_name),
        },
        SafetyMode::Supervised => match tool_approval {
            ApprovalRequirement::Always => ApprovalDecision::RequireApproval {
                reason: format!("Tool '{}' requires approval (high-risk)", tool_name),
            },
            ApprovalRequirement::UnlessAutoApproved => ApprovalDecision::AutoApprove,
            ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
        },
    };
    log_audit(db, session_id, tool_name, arguments, &decision, None);
    decision
}

fn mode_to_decision(mode: &str, tool_name: &str) -> ApprovalDecision {
    match mode {
        "allow" => ApprovalDecision::AutoApprove,
        "block" => ApprovalDecision::Block { reason: format!("Blocked by rule for '{}'", tool_name) },
        _ /* "ask" */ => ApprovalDecision::RequireApproval {
            reason: format!("Rule requires approval for '{}'", tool_name),
        },
    }
}

fn lookup_session_rule(
    db: &Arc<Mutex<Connection>>,
    session_id: &str,
    tool_name: &str,
) -> Option<PermissionRule> {
    let conn = db.lock().ok()?;
    conn.query_row(
        "SELECT id, scope, session_id, tool_name, target, mode, created_at
         FROM tool_permission_rules
         WHERE scope = 'session' AND session_id = ?1 AND tool_name = ?2
         ORDER BY created_at DESC LIMIT 1",
        rusqlite::params![session_id, tool_name],
        row_to_rule,
    ).ok()
}

fn lookup_pattern_rule(
    db: &Arc<Mutex<Connection>>,
    tool_name: &str,
    target: &str,
) -> Option<PermissionRule> {
    let conn = db.lock().ok()?;
    // Match the LONGEST target that is a prefix of the call's target.
    let mut stmt = conn.prepare(
        "SELECT id, scope, session_id, tool_name, target, mode, created_at
         FROM tool_permission_rules
         WHERE scope = 'pattern' AND tool_name = ?1
         ORDER BY length(target) DESC"
    ).ok()?;
    let rows = stmt.query_map(rusqlite::params![tool_name], row_to_rule).ok()?;
    for r in rows.flatten() {
        if let Some(t) = &r.target {
            if !t.is_empty() && target.starts_with(t.as_str()) {
                return Some(r);
            }
        }
    }
    None
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> Result<PermissionRule, rusqlite::Error> {
    Ok(PermissionRule {
        id: row.get(0)?,
        scope: row.get(1)?,
        session_id: row.get(2)?,
        tool_name: row.get(3)?,
        target: row.get(4)?,
        mode: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn log_audit(
    db: &Arc<Mutex<Connection>>,
    session_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    decision: &ApprovalDecision,
    rule_id: Option<&str>,
) {
    let decision_str = match decision {
        ApprovalDecision::AutoApprove => "auto_approve",
        ApprovalDecision::RequireApproval { .. } => "user_approve", // resolver returned "ask" — UI still has to confirm
        ApprovalDecision::Block { .. } => "blocked",
    };
    let conn = match db.lock() {
        Ok(c) => c,
        Err(e) => { tracing::warn!("permissions: DB lock for audit: {}", e); return; }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO permission_audit_log (id, session_id, tool_name, args_hash, decision, rule_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            session_id, tool_name, args_hash(arguments),
            decision_str, rule_id, chrono::Utc::now().timestamp_millis(),
        ],
    ) {
        tracing::warn!("permissions: audit insert failed: {}", e);
    }
}

// ── CRUD on rules (used by Tauri commands in tauri_commands.rs) ─────────

pub fn list_rules(db: &Arc<Mutex<Connection>>) -> Result<Vec<PermissionRule>, rusqlite::Error> {
    let conn = db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let mut stmt = conn.prepare(
        "SELECT id, scope, session_id, tool_name, target, mode, created_at
         FROM tool_permission_rules ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map([], row_to_rule)?;
    Ok(rows.flatten().collect())
}

pub fn create_rule(db: &Arc<Mutex<Connection>>, input: CreatePermissionRuleInput) -> Result<PermissionRule, rusqlite::Error> {
    let conn = db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO tool_permission_rules (id, scope, session_id, tool_name, target, mode, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, input.scope, input.session_id, input.tool_name, input.target, input.mode, created_at],
    )?;
    Ok(PermissionRule {
        id,
        scope: input.scope,
        session_id: input.session_id,
        tool_name: input.tool_name,
        target: input.target,
        mode: input.mode,
        created_at,
    })
}

pub fn delete_rule(db: &Arc<Mutex<Connection>>, id: &str) -> Result<bool, rusqlite::Error> {
    let conn = db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let n = conn.execute("DELETE FROM tool_permission_rules WHERE id = ?1", rusqlite::params![id])?;
    Ok(n > 0)
}

pub fn list_audit(
    db: &Arc<Mutex<Connection>>,
    session_id: Option<&str>,
    limit: u32,
) -> Result<Vec<PermissionAuditEntry>, rusqlite::Error> {
    let conn = db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let limit = limit.clamp(1, 500) as i64;
    let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match session_id {
        Some(sid) => (
            "SELECT id, session_id, tool_name, args_hash, decision, rule_id, created_at
             FROM permission_audit_log WHERE session_id = ?1
             ORDER BY created_at DESC LIMIT ?2",
            vec![Box::new(sid.to_string()), Box::new(limit)],
        ),
        None => (
            "SELECT id, session_id, tool_name, args_hash, decision, rule_id, created_at
             FROM permission_audit_log
             ORDER BY created_at DESC LIMIT ?1",
            vec![Box::new(limit)],
        ),
    };
    let mut stmt = conn.prepare(sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| Ok(PermissionAuditEntry {
        id: row.get(0)?,
        session_id: row.get(1)?,
        tool_name: row.get(2)?,
        args_hash: row.get(3)?,
        decision: row.get(4)?,
        rule_id: row.get(5)?,
        created_at: row.get(6)?,
    }))?;
    Ok(rows.flatten().collect())
}
```

Add `sha2 = "0.10"` to `src-tauri/Cargo.toml` if not present (verify with `grep '"sha2"' src-tauri/Cargo.toml`).

- [ ] **Step 2.3: Wire from `should_approve`**

Edit `src-tauri/src/safety/mod.rs`. Add at the top:

```rust
pub mod permissions;
```

Then change `should_approve`'s signature to accept `session_id: &str` + a DB handle. **Bigger concern:** existing call sites in `agent/dispatcher.rs` already pass `tool_name`, `arguments`, `tool_approval`, `mode_override`. Find them:

```bash
grep -rnE "should_approve\(" src-tauri/src | head
```

For each call site, thread `session_id` (already in scope as `self.conversation_id`) and the DB handle (`state.db.clone()`). The cleanest approach: keep `should_approve` as the in-memory shim **and** add a new `should_approve_with_db(&self, db, session_id, ...)` that delegates to `permissions::resolve_decision`. Migrate dispatcher to call the new method; leave the old one for any tests that exercise it without a DB.

```rust
// In `impl SafetyManager`:
pub fn should_approve_with_db(
    &self,
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    session_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    tool_approval: &crate::agent::tools::tool::ApprovalRequirement,
    mode_override: Option<&SafetyMode>,
) -> ApprovalDecision {
    permissions::resolve_decision(db, &self.policy, session_id, tool_name, arguments, tool_approval, mode_override)
}
```

- [ ] **Step 2.4: Update dispatcher call site**

Edit `src-tauri/src/agent/dispatcher.rs`. Find every `should_approve(` call. Replace each with `should_approve_with_db(&state.db, &self.conversation_id, ...)`. The dispatcher needs a `state: &AppState` reference at the call site — check whether it already has one (it had `app_handle.try_state::<AppState>()` from P5; reuse that pattern).

- [ ] **Step 2.5: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: 0 errors.

- [ ] **Step 2.6: Commit**

```bash
git add src-tauri/src/safety/permissions.rs src-tauri/src/safety/mod.rs src-tauri/src/ipc.rs src-tauri/src/agent/dispatcher.rs src-tauri/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(safety): permissions::resolve_decision + audit log

DB-backed resolver replaces the in-memory should_approve flow without
changing call sites' contract. Resolution precedence:

  1. Session rule  (tool_permission_rules WHERE scope='session')
  2. Pattern rule  (longest target prefix, scope='pattern')
  3. Tool override (existing safety_policy.tool_overrides)
  4. Global mode   (existing safety_policy.global_mode)

Every call writes one row to permission_audit_log with a truncated
SHA-256 hash of the arguments — enough to dedupe similar calls in the
UI without storing PII verbatim.

Existing safety_policy.json (auto-approved + blocked + tool overrides)
is the new "global tier" — no migration of values, no behavior change
for users who haven't created any rules yet.
EOF
)"
```

---

## Task 3: Tauri commands — CRUD + list

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add 4 commands)
- Modify: `src-tauri/src/main.rs` (`invoke_handler!` registration)

- [ ] **Step 3.1: Add commands**

Edit `src-tauri/src/tauri_commands.rs`. Add near the existing `approve_tool_call`:

```rust
#[tauri::command]
pub async fn list_permission_rules(
    state: State<'_, AppState>,
) -> Result<Vec<PermissionRule>, Error> {
    crate::safety::permissions::list_rules(&state.db)
        .map_err(|e| Error::Internal(format!("list_permission_rules: {}", e)))
}

#[tauri::command]
pub async fn create_permission_rule(
    state: State<'_, AppState>,
    input: CreatePermissionRuleInput,
) -> Result<PermissionRule, Error> {
    crate::safety::permissions::create_rule(&state.db, input)
        .map_err(|e| Error::Internal(format!("create_permission_rule: {}", e)))
}

#[tauri::command]
pub async fn delete_permission_rule(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, Error> {
    crate::safety::permissions::delete_rule(&state.db, &id)
        .map_err(|e| Error::Internal(format!("delete_permission_rule: {}", e)))
}

#[tauri::command]
pub async fn list_permission_audit(
    state: State<'_, AppState>,
    session_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<PermissionAuditEntry>, Error> {
    crate::safety::permissions::list_audit(&state.db, session_id.as_deref(), limit.unwrap_or(100))
        .map_err(|e| Error::Internal(format!("list_permission_audit: {}", e)))
}
```

Imports needed at the top:
```rust
use crate::ipc::{PermissionRule, PermissionAuditEntry, CreatePermissionRuleInput};
```

- [ ] **Step 3.2: Register in `invoke_handler!`**

Edit `src-tauri/src/main.rs`. Add to the `invoke_handler!` macro list (near `approve_tool_call`):
```rust
uclaw_core::tauri_commands::list_permission_rules,
uclaw_core::tauri_commands::create_permission_rule,
uclaw_core::tauri_commands::delete_permission_rule,
uclaw_core::tauri_commands::list_permission_audit,
```

- [ ] **Step 3.3: Build clean + commit**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(safety): CRUD commands for permission rules + audit log"
```

---

## Task 4: Backend tests — resolver precedence

**Files:**
- Modify: `src-tauri/src/safety/permissions.rs` (append `#[cfg(test)] mod tests`)

- [ ] **Step 4.1: Write tests**

Append to `src-tauri/src/safety/permissions.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tool::ApprovalRequirement;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V14_PERMISSION_TABLES).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn baseline_policy() -> SafetyPolicy {
        SafetyPolicy {
            global_mode: SafetyMode::Supervised,
            tool_overrides: Default::default(),
            auto_approved_tools: Default::default(),
            blocked_tools: Default::default(),
        }
    }

    #[test]
    fn no_rules_falls_through_to_global() {
        let db = fresh_db();
        let policy = baseline_policy();
        let args = serde_json::json!({});
        let d = resolve_decision(&db, &policy, "sess1", "bash", &args, &ApprovalRequirement::Always, None);
        assert!(matches!(d, ApprovalDecision::RequireApproval { .. }));
    }

    #[test]
    fn session_rule_wins_over_pattern() {
        let db = fresh_db();
        let policy = baseline_policy();
        // Session rule allow
        create_rule(&db, CreatePermissionRuleInput {
            scope: "session".into(), session_id: Some("sess1".into()),
            tool_name: "bash".into(), target: None, mode: "allow".into(),
        }).unwrap();
        // Pattern rule block (lower precedence)
        create_rule(&db, CreatePermissionRuleInput {
            scope: "pattern".into(), session_id: None,
            tool_name: "bash".into(), target: Some("rm".into()), mode: "block".into(),
        }).unwrap();
        let args = serde_json::json!({"command": "rm -rf /tmp/foo"});
        let d = resolve_decision(&db, &policy, "sess1", "bash", &args, &ApprovalRequirement::Always, None);
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn pattern_rule_uses_longest_match() {
        let db = fresh_db();
        let policy = baseline_policy();
        create_rule(&db, CreatePermissionRuleInput {
            scope: "pattern".into(), session_id: None,
            tool_name: "bash".into(), target: Some("git ".into()), mode: "ask".into(),
        }).unwrap();
        create_rule(&db, CreatePermissionRuleInput {
            scope: "pattern".into(), session_id: None,
            tool_name: "bash".into(), target: Some("git status".into()), mode: "allow".into(),
        }).unwrap();
        let args = serde_json::json!({"command": "git status"});
        let d = resolve_decision(&db, &policy, "sess1", "bash", &args, &ApprovalRequirement::Always, None);
        // Longest match "git status" wins → allow
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn blocked_list_overrides_everything() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.blocked_tools.insert("bash".into());
        // Even a session-allow rule shouldn't override blocked_tools
        create_rule(&db, CreatePermissionRuleInput {
            scope: "session".into(), session_id: Some("sess1".into()),
            tool_name: "bash".into(), target: None, mode: "allow".into(),
        }).unwrap();
        let args = serde_json::json!({"command": "ls"});
        let d = resolve_decision(&db, &policy, "sess1", "bash", &args, &ApprovalRequirement::Always, None);
        assert!(matches!(d, ApprovalDecision::Block { .. }));
    }

    #[test]
    fn audit_log_records_each_decision() {
        let db = fresh_db();
        let policy = baseline_policy();
        let args = serde_json::json!({"command": "ls"});
        let _ = resolve_decision(&db, &policy, "sess1", "bash", &args, &ApprovalRequirement::Always, None);
        let log = list_audit(&db, Some("sess1"), 10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].tool_name, "bash");
    }

    #[test]
    fn pattern_target_extracts_command_first() {
        let args = serde_json::json!({"command": "ls -la", "cwd": "/tmp"});
        assert_eq!(pattern_target(&args).as_deref(), Some("ls -la"));
    }

    #[test]
    fn args_hash_is_stable_and_short() {
        let h1 = args_hash(&serde_json::json!({"a": 1}));
        let h2 = args_hash(&serde_json::json!({"a": 1}));
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }
}
```

- [ ] **Step 4.2: Run + commit**

```bash
cd src-tauri && cargo test --lib safety::permissions::tests 2>&1 | tail -10
```
Expected: 7 passed.

```bash
git add src-tauri/src/safety/permissions.rs
git commit -m "test(safety): resolver precedence + audit log + pattern matching"
```

---

## Task 5: Frontend — types + bridge + atom

**Files:**
- Modify: `ui/src/lib/types.ts` (3 new interfaces)
- Modify: `ui/src/lib/tauri-bridge.ts` (4 invoke wrappers)

- [ ] **Step 5.1: TS types**

Append to `ui/src/lib/types.ts`:

```ts
// ===== Permission rules =====

export interface PermissionRule {
  id: string
  scope: 'session' | 'pattern'
  sessionId?: string
  toolName: string
  /** For pattern scope: argument prefix to match. Undefined for session scope. */
  target?: string
  mode: 'allow' | 'block' | 'ask'
  createdAt: number
}

export interface PermissionAuditEntry {
  id: string
  sessionId: string
  toolName: string
  argsHash: string
  decision: 'auto_approve' | 'user_approve' | 'user_deny' | 'blocked'
  ruleId?: string
  createdAt: number
}

export interface CreatePermissionRuleInput {
  scope: 'session' | 'pattern'
  sessionId?: string
  toolName: string
  target?: string
  mode: 'allow' | 'block' | 'ask'
}
```

- [ ] **Step 5.2: Bridge wrappers**

Append to `ui/src/lib/tauri-bridge.ts`:

```ts
import type { PermissionRule, PermissionAuditEntry, CreatePermissionRuleInput } from './types'

export const listPermissionRules = (): Promise<PermissionRule[]> =>
  invoke<PermissionRule[]>('list_permission_rules')

export const createPermissionRule = (input: CreatePermissionRuleInput): Promise<PermissionRule> =>
  invoke<PermissionRule>('create_permission_rule', { input })

export const deletePermissionRule = (id: string): Promise<boolean> =>
  invoke<boolean>('delete_permission_rule', { id })

export const listPermissionAudit = (sessionId?: string, limit = 100): Promise<PermissionAuditEntry[]> =>
  invoke<PermissionAuditEntry[]>('list_permission_audit', { sessionId, limit })
```

- [ ] **Step 5.3: TS check + commit**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts
git commit -m "feat(safety): TS types + bridge for permission rules / audit log"
```

---

## Task 6: Frontend — Settings → 工具权限 tab

**Files:**
- Create: `ui/src/components/settings/PermissionsSettings.tsx`
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (nav entry + content branch)
- Modify: `ui/src/atoms/settings-tab.ts` (add `'permissions'`)

- [ ] **Step 6.1: Component**

Create `ui/src/components/settings/PermissionsSettings.tsx`:

```tsx
/**
 * PermissionsSettings — Settings → 工具权限 tab.
 *
 * Two sections:
 *   1. Rules table — add / delete session + pattern rules
 *   2. Audit log table — most-recent decisions across all sessions
 *
 * Live update: re-fetch on mount; manual refresh button.
 */

import * as React from 'react'
import { ShieldCheck, ShieldAlert, ShieldOff, Trash2, Plus, RefreshCw } from 'lucide-react'
import {
  listPermissionRules,
  createPermissionRule,
  deletePermissionRule,
  listPermissionAudit,
} from '@/lib/tauri-bridge'
import type { PermissionRule, PermissionAuditEntry, CreatePermissionRuleInput } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const MODE_BADGE: Record<string, { label: string; className: string }> = {
  allow: { label: '允许', className: 'bg-green-500/15 text-green-600 dark:text-green-400 border-green-500/30' },
  block: { label: '阻止', className: 'bg-red-500/15 text-red-600 dark:text-red-400 border-red-500/30' },
  ask:   { label: '询问', className: 'bg-yellow-500/15 text-yellow-600 dark:text-yellow-400 border-yellow-500/30' },
}

const DECISION_BADGE: Record<string, { label: string; className: string }> = {
  auto_approve:  { label: '自动允许', className: 'bg-green-500/10 text-green-600 dark:text-green-400' },
  user_approve:  { label: '用户允许', className: 'bg-blue-500/10 text-blue-600 dark:text-blue-400' },
  user_deny:     { label: '用户拒绝', className: 'bg-orange-500/10 text-orange-600 dark:text-orange-400' },
  blocked:       { label: '已阻止',   className: 'bg-red-500/10 text-red-600 dark:text-red-400' },
}

function formatTime(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

export function PermissionsSettings(): React.ReactElement {
  const [rules, setRules] = React.useState<PermissionRule[]>([])
  const [audit, setAudit] = React.useState<PermissionAuditEntry[]>([])
  const [loading, setLoading] = React.useState(true)
  const [draft, setDraft] = React.useState<CreatePermissionRuleInput>({
    scope: 'pattern', toolName: '', target: '', mode: 'allow',
  })

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [r, a] = await Promise.all([
        listPermissionRules(),
        listPermissionAudit(undefined, 100),
      ])
      setRules(r)
      setAudit(a)
    } finally {
      setLoading(false)
    }
  }, [])
  React.useEffect(() => { void refetch() }, [refetch])

  const onAddRule = async () => {
    if (!draft.toolName.trim()) return
    await createPermissionRule({
      scope: draft.scope,
      sessionId: draft.scope === 'session' ? draft.sessionId : undefined,
      toolName: draft.toolName.trim(),
      target: draft.scope === 'pattern' ? (draft.target?.trim() || undefined) : undefined,
      mode: draft.mode,
    })
    setDraft({ scope: 'pattern', toolName: '', target: '', mode: 'allow' })
    await refetch()
  }

  const onDelete = async (id: string) => {
    await deletePermissionRule(id)
    await refetch()
  }

  return (
    <div className="space-y-6 pb-8">
      {/* Rules section */}
      <section>
        <div className="mb-2.5 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            权限规则
          </h3>
          <Button size="sm" variant="ghost" onClick={() => void refetch()} disabled={loading}>
            <RefreshCw className="size-3.5" />
          </Button>
        </div>

        {/* Rule editor */}
        <div className="mb-3 rounded-lg border border-border/50 bg-muted/20 p-3 space-y-2">
          <div className="grid grid-cols-12 gap-2 text-[12px]">
            <select
              value={draft.scope}
              onChange={(e) => setDraft((d) => ({ ...d, scope: e.target.value as 'session' | 'pattern' }))}
              className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
            >
              <option value="pattern">模式</option>
              <option value="session">会话</option>
            </select>
            <input
              placeholder="tool_name (例如 bash)"
              value={draft.toolName}
              onChange={(e) => setDraft((d) => ({ ...d, toolName: e.target.value }))}
              className="col-span-3 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
            />
            <input
              placeholder={draft.scope === 'pattern' ? '目标前缀 (例如 git status)' : 'session_id'}
              value={draft.scope === 'pattern' ? (draft.target ?? '') : (draft.sessionId ?? '')}
              onChange={(e) => setDraft((d) => draft.scope === 'pattern'
                ? { ...d, target: e.target.value }
                : { ...d, sessionId: e.target.value })}
              className="col-span-4 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
            />
            <select
              value={draft.mode}
              onChange={(e) => setDraft((d) => ({ ...d, mode: e.target.value as 'allow' | 'block' | 'ask' }))}
              className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
            >
              <option value="allow">允许</option>
              <option value="block">阻止</option>
              <option value="ask">询问</option>
            </select>
            <Button size="sm" onClick={onAddRule} className="col-span-1" disabled={!draft.toolName.trim()}>
              <Plus className="size-3.5" />
            </Button>
          </div>
        </div>

        {/* Rules table */}
        <div className="rounded-lg border border-border/50 bg-muted/20 max-h-64 overflow-y-auto">
          {rules.length === 0 ? (
            <div className="p-6 text-center text-[12px] text-muted-foreground/60">
              {loading ? '加载中…' : '暂无规则'}
            </div>
          ) : (
            <table className="w-full text-[12px]">
              <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                <tr className="text-left text-muted-foreground/70">
                  <th className="px-3 py-2 font-normal">范围</th>
                  <th className="px-3 py-2 font-normal">工具</th>
                  <th className="px-3 py-2 font-normal">目标 / 会话</th>
                  <th className="px-3 py-2 font-normal">模式</th>
                  <th className="px-3 py-2 font-normal w-8" />
                </tr>
              </thead>
              <tbody>
                {rules.map((r) => {
                  const mb = MODE_BADGE[r.mode] ?? MODE_BADGE.ask
                  return (
                    <tr key={r.id} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5">{r.scope === 'session' ? '会话' : '模式'}</td>
                      <td className="px-3 py-1.5 font-mono">{r.toolName}</td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/85 truncate max-w-[200px]">
                        {r.target ?? r.sessionId ?? ''}
                      </td>
                      <td className="px-3 py-1.5">
                        <span className={cn('inline-flex items-center rounded border px-1.5 py-0.5 text-[10.5px]', mb.className)}>
                          {mb.label}
                        </span>
                      </td>
                      <td className="px-3 py-1.5">
                        <Button size="sm" variant="ghost" onClick={() => void onDelete(r.id)} className="h-6 w-6 p-0">
                          <Trash2 className="size-3 text-muted-foreground/70" />
                        </Button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </div>
      </section>

      {/* Audit log */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          审计日志（最近 100 条）
        </h3>
        <div className="rounded-lg border border-border/50 bg-muted/20 max-h-72 overflow-y-auto">
          {audit.length === 0 ? (
            <div className="p-6 text-center text-[12px] text-muted-foreground/60">
              {loading ? '加载中…' : '暂无审计记录'}
            </div>
          ) : (
            <table className="w-full text-[12px]">
              <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                <tr className="text-left text-muted-foreground/70">
                  <th className="px-3 py-2 font-normal">时间</th>
                  <th className="px-3 py-2 font-normal">工具</th>
                  <th className="px-3 py-2 font-normal">会话</th>
                  <th className="px-3 py-2 font-normal">参数 hash</th>
                  <th className="px-3 py-2 font-normal">决定</th>
                </tr>
              </thead>
              <tbody>
                {audit.map((a) => {
                  const db = DECISION_BADGE[a.decision] ?? { label: a.decision, className: 'bg-muted text-muted-foreground' }
                  return (
                    <tr key={a.id} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5 text-muted-foreground/70 tabular-nums">{formatTime(a.createdAt)}</td>
                      <td className="px-3 py-1.5 font-mono">{a.toolName}</td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/70 truncate max-w-[100px]">
                        {a.sessionId.slice(0, 8)}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/70">{a.argsHash}</td>
                      <td className="px-3 py-1.5">
                        <span className={cn('inline-flex items-center rounded px-1.5 py-0.5 text-[10.5px]', db.className)}>
                          {db.label}
                        </span>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </div>
      </section>
    </div>
  )
}
```

- [ ] **Step 6.2: Wire into nav**

Edit `ui/src/atoms/settings-tab.ts`. Add `'permissions'` to the `SettingsTab` union type (between `'tools'` and `'usage'` or wherever it fits semantically).

Edit `ui/src/components/settings/SettingsPanel.tsx`:
- Add `import { ShieldCheck } from 'lucide-react'` (merge into existing lucide import).
- Add `import { PermissionsSettings } from './PermissionsSettings'`.
- Add to the `TABS` array (between `tools` and `usage`):
  ```tsx
  { id: 'permissions', label: '工具权限', icon: <ShieldCheck size={15} /> },
  ```
- Add to `SettingsContent` switch:
  ```tsx
  case 'permissions':
    return <PermissionsSettings />
  ```

- [ ] **Step 6.3: TS check + commit**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/atoms/settings-tab.ts ui/src/components/settings/SettingsPanel.tsx ui/src/components/settings/PermissionsSettings.tsx
git commit -m "$(cat <<'EOF'
feat(safety): Settings → 工具权限 tab

Two sections:
  - Rules table with inline add/delete (scope=pattern|session, mode=allow|block|ask)
  - Audit log showing the 100 most-recent decisions across all sessions

Uses native <table> like the cost-dashboard session table; no chart deps.
EOF
)"
```

---

## Task 7: Modal rewrite — 4 buttons + session-allow

**Files:**
- Modify: `ui/src/components/ApprovalModal.tsx`

The existing modal has 3 buttons (拒绝 / 始终允许 / 批准). Add a 4th — **本次会话允许** — which creates a session-scope rule for `(currentSession, request.toolName, mode='allow')` and approves once.

- [ ] **Step 7.1: Read current sessionId from atom**

Add the imports + atom read:
```tsx
import { useAtomValue } from 'jotai'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { createPermissionRule } from '@/lib/tauri-bridge'
```

Inside the component:
```tsx
const appMode = useAtomValue(appModeAtom)
const currentAgentSessionId = useAtomValue(currentAgentSessionIdAtom)
const currentConversationId = useAtomValue(currentConversationIdAtom)
const activeSessionId = appMode === 'agent' ? currentAgentSessionId : currentConversationId
```

- [ ] **Step 7.2: Add session-allow handler**

Above `respond`:

```tsx
const respondWithSessionRule = async () => {
  if (!request || loading || !activeSessionId) return
  setLoading(true)
  try {
    await createPermissionRule({
      scope: 'session',
      sessionId: activeSessionId,
      toolName: request.toolName,
      mode: 'allow',
    })
    await approveToolCall({
      sessionId: request.sessionId,
      toolId: request.toolId,
      approved: true,
      alwaysAllow: false,
    })
  } catch (err) {
    console.error('[ApprovalModal] session-allow failed:', err)
  } finally {
    setLoading(false)
    setRequest(null)
  }
}
```

- [ ] **Step 7.3: Add the 4th button**

Replace the `AlertDialogFooter` content:

```tsx
<AlertDialogFooter className="flex-col sm:flex-row gap-2 flex-wrap">
  <AlertDialogCancel asChild>
    <Button
      variant="outline"
      onClick={() => respond(false)}
      disabled={loading}
      className="border-red-500/30 text-red-500 hover:bg-red-500/10"
    >
      拒绝
    </Button>
  </AlertDialogCancel>
  {activeSessionId && (
    <Button
      variant="outline"
      onClick={respondWithSessionRule}
      disabled={loading}
      className="border-muted-foreground/30"
      title="为当前会话创建一条 allow 规则"
    >
      本次会话允许
    </Button>
  )}
  <Button
    variant="outline"
    onClick={() => respond(true, true)}
    disabled={loading}
    className="border-muted-foreground/30"
    title="把工具加入全局白名单"
  >
    始终允许
  </Button>
  <AlertDialogAction asChild>
    <Button onClick={() => respond(true)} disabled={loading}>
      批准
    </Button>
  </AlertDialogAction>
</AlertDialogFooter>
```

- [ ] **Step 7.4: TS check + commit**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/components/ApprovalModal.tsx
git commit -m "$(cat <<'EOF'
feat(safety): ApprovalModal — add 本次会话允许 button

Fourth button creates a session-scope rule for (currentSession, toolName,
mode='allow') and approves the call once. Only visible when an active
chat or agent session is in focus (no session id → button hidden).

Existing 始终允许 path (alwaysAllow=true → global whitelist) preserved
as the broader-scope option.
EOF
)"
```

---

## Task 8: Frontend test — PermissionsSettings smoke

**Files:**
- Create: `ui/src/components/settings/PermissionsSettings.test.tsx`

Light test — the component is mostly tabular; cover the "renders rules" / "renders audit" / "empty state" paths.

- [ ] **Step 8.1: Write tests**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { PermissionsSettings } from './PermissionsSettings'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  listPermissionRules: vi.fn(async () => [
    { id: 'r1', scope: 'pattern', toolName: 'bash', target: 'git status', mode: 'allow', createdAt: 1715000000000 },
  ]),
  listPermissionAudit: vi.fn(async () => [
    { id: 'a1', sessionId: 'sess-aaa', toolName: 'bash', argsHash: 'abc1', decision: 'auto_approve', createdAt: 1715000000000 },
  ]),
  createPermissionRule: vi.fn(async (i) => ({ ...i, id: 'new', createdAt: Date.now() })),
  deletePermissionRule: vi.fn(async () => true),
}))

describe('PermissionsSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders the rules table row', async () => {
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('git status')).toBeInTheDocument()
      expect(screen.getByText('bash')).toBeInTheDocument()
    })
  })

  it('renders the audit log row', async () => {
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('自动允许')).toBeInTheDocument()
      expect(screen.getByText('abc1')).toBeInTheDocument()
    })
  })

  it('renders empty states when both lists are empty', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.listPermissionRules).mockResolvedValueOnce([])
    vi.mocked(bridge.listPermissionAudit).mockResolvedValueOnce([])
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('暂无规则')).toBeInTheDocument()
      expect(screen.getByText('暂无审计记录')).toBeInTheDocument()
    })
  })
})
```

- [ ] **Step 8.2: Run + commit**

```bash
cd ui && npx vitest run PermissionsSettings 2>&1 | tail -15
```
Expected: 3/3 passing.

```bash
git add ui/src/components/settings/PermissionsSettings.test.tsx
git commit -m "test(safety): PermissionsSettings — rules table / audit log / empty states"
```

---

## Task 9: Final verification + push + PR

- [ ] **Step 9.1: Full pipeline**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, ~195 backend tests (188 prior + 7 new), 0 TS errors, ~48 frontend tests (45 prior + 3 new).

- [ ] **Step 9.2: Push + PR**

```bash
git push -u origin claude/p6-permission-ui
gh pr create --title "P6: tool permission UI — rules + audit log + 4-button modal" --body "$(cat <<'EOF'
## Summary

Upgrades the tool-approval surface from "global allow-list + per-tool override" to a real permission center.

| Capability | Before | After |
|---|---|---|
| Per-tool defaults | global \`auto_approved_tools\` whitelist (\`safety_policy.json\`) | unchanged — kept as the "global tier" |
| Per-session overrides | — | \`tool_permission_rules.scope='session'\` |
| Command-pattern rules ("always allow \`bash git status\`") | — | \`tool_permission_rules.scope='pattern'\` |
| Audit log | only \`tracing\` logs | \`permission_audit_log\` table + Settings UI |
| Approval modal | 3 buttons (拒绝 / 始终允许 / 批准) | 4 buttons (拒绝 / 本次会话允许 / 始终允许 / 批准) |

## Architecture

| Layer | Change |
|---|---|
| **DB** | V14: \`tool_permission_rules\` (scope/session_id/tool_name/target/mode) + \`permission_audit_log\` (decision history with SHA-256 args_hash) |
| **Backend** | New \`safety/permissions.rs\` resolver. Precedence: **session > pattern > tool override > global**. Each call writes one audit row (best-effort; failures logged + swallowed) |
| **Backend** | \`SafetyManager::should_approve_with_db\` is the new entry point; existing \`should_approve\` kept for tests that don't have a DB |
| **Backend** | 4 CRUD Tauri commands |
| **Backend tests** | 7 unit tests on resolver precedence + audit log + pattern matching |
| **Frontend** | \`PermissionsSettings.tsx\` — rules table with inline add/delete + audit log table |
| **Frontend** | New "工具权限" nav entry (between 工具 and 用量) |
| **Frontend** | \`ApprovalModal.tsx\` gets a 4th button "本次会话允许" that creates a session rule |
| **Frontend tests** | 3 cases — rules render, audit render, empty states |

## Verification

- ✅ \`cargo build\` clean
- ✅ ~195 backend tests passing (188 prior + 7 new)
- ✅ \`tsc --noEmit\` clean
- ✅ ~48 frontend tests passing (45 prior + 3 new)
- ✅ Manual: approve once with "本次会话允许" → next call in same session auto-approves; switch session → asks again

## Migration version registry

V14 claimed by this PR. Updated \`CLAUDE.md\` registry table.

## Out of scope (deferred)

- Regex / glob pattern matching (currently simple prefix; YAGNI for now)
- Per-workspace rules
- Bulk import / export of rules
- Audit log filtering UI (currently shows last 100, no per-tool / per-session filter)
- Migrating existing \`safety_policy.json\` whitelist into V14 rules — left as legacy "global tier" so users' existing config keeps working

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria

- ✅ V14 migration creates both tables + indexes; idempotent
- ✅ Resolver precedence: session > pattern > tool > global, with longest-prefix winning within pattern scope
- ✅ Each tool decision writes one audit row
- ✅ Settings → 工具权限 lists rules + audit, supports add/delete inline
- ✅ Approval modal's 4th button creates a session rule + approves once
- ✅ ~7 backend resolver tests + ~3 frontend smoke tests passing
- ✅ Each task is its own commit (bisectable)
- ✅ \`safety_policy.json\` (existing whitelist / blocklist / tool overrides) keeps working unchanged

## Out of scope (deferred)

- Regex / glob pattern matching
- Per-workspace rules
- Bulk import/export
- Audit log filtering / search
- Per-rule expiration
- Migrating safety_policy.json values into V14 rules
