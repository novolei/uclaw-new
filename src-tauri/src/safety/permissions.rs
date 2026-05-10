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
use crate::ipc::{CreatePermissionRuleInput, PermissionAuditEntry, PermissionRule};
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
        "block" => ApprovalDecision::Block {
            reason: format!("Blocked by rule for '{}'", tool_name),
        },
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
    )
    .ok()
}

fn lookup_pattern_rule(
    db: &Arc<Mutex<Connection>>,
    tool_name: &str,
    target: &str,
) -> Option<PermissionRule> {
    let conn = db.lock().ok()?;
    // Match the LONGEST target that is a prefix of the call's target.
    let mut stmt = conn
        .prepare(
            "SELECT id, scope, session_id, tool_name, target, mode, created_at
         FROM tool_permission_rules
         WHERE scope = 'pattern' AND tool_name = ?1
         ORDER BY length(target) DESC",
        )
        .ok()?;
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
        Err(e) => {
            tracing::warn!("permissions: DB lock for audit: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO permission_audit_log (id, session_id, tool_name, args_hash, decision, rule_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            session_id,
            tool_name,
            args_hash(arguments),
            decision_str,
            rule_id,
            chrono::Utc::now().timestamp_millis(),
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
         FROM tool_permission_rules ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_rule)?;
    Ok(rows.flatten().collect())
}

pub fn create_rule(
    db: &Arc<Mutex<Connection>>,
    input: CreatePermissionRuleInput,
) -> Result<PermissionRule, rusqlite::Error> {
    let conn = db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO tool_permission_rules (id, scope, session_id, tool_name, target, mode, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            input.scope,
            input.session_id,
            input.tool_name,
            input.target,
            input.mode,
            created_at
        ],
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
    let n = conn.execute(
        "DELETE FROM tool_permission_rules WHERE id = ?1",
        rusqlite::params![id],
    )?;
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
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
        Ok(PermissionAuditEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            tool_name: row.get(2)?,
            args_hash: row.get(3)?,
            decision: row.get(4)?,
            rule_id: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.flatten().collect())
}
