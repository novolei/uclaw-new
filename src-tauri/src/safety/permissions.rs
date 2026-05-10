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

/// Tool names that auto-pass under `AcceptEdits` mode. These are stable
/// built-in tool names — see `agent/tools/builtin/{file,edit}.rs`.
const EDIT_TOOLS: &[&str] = &["edit", "write_file"];

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

    // Compute the call's command/arg-prefix once. Both session-rule and
    // pattern-rule lookups consult it (session rules with a target use prefix
    // matching just like pattern rules; session rules without a target are
    // tool-wide for backward compatibility).
    let call_target = pattern_target(arguments);

    // 1. Session rule
    if let Some(rule) = lookup_session_rule(db, session_id, tool_name, call_target.as_deref()) {
        let decision = mode_to_decision(&rule.mode, tool_name);
        log_audit(db, session_id, tool_name, arguments, &decision, Some(&rule.id));
        return decision;
    }

    // 2. Pattern rule
    if let Some(ref target) = call_target {
        if let Some(rule) = lookup_pattern_rule(db, tool_name, target) {
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

/// Find the most-specific session rule for (session, tool, call_target).
///
/// A rule with a non-null `target` only matches when the call's `call_target`
/// (extracted via `pattern_target` from the arguments) starts with it —
/// same prefix-match semantics as pattern rules. A rule with NULL target is
/// tool-wide and matches any call (legacy "本次会话允许" semantics; kept for
/// back-compat with rules created before targeted session rules existed).
///
/// Longest target wins (so a more specific rule shadows a broader one).
fn lookup_session_rule(
    db: &Arc<Mutex<Connection>>,
    session_id: &str,
    tool_name: &str,
    call_target: Option<&str>,
) -> Option<PermissionRule> {
    let conn = db.lock().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, scope, session_id, tool_name, target, mode, created_at
         FROM tool_permission_rules
         WHERE scope = 'session' AND session_id = ?1 AND tool_name = ?2
         ORDER BY length(COALESCE(target, '')) DESC, created_at DESC",
        )
        .ok()?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, tool_name], row_to_rule)
        .ok()?;
    for r in rows.flatten() {
        match (r.target.as_deref(), call_target) {
            // Tool-wide session rule (legacy) — matches any call.
            (None, _) | (Some(""), _) => return Some(r),
            // Targeted session rule — only match if call target starts with it.
            (Some(t), Some(c)) if c.starts_with(t) => return Some(r),
            _ => continue,
        }
    }
    None
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
    // NOTE: This logs at the moment the resolver decides, BEFORE the user has
    // actually confirmed. `user_approve` here really means "RequireApproval
    // was returned to the dispatcher" — the actual user outcome is not yet
    // known. A real "awaiting" status would need a CHECK-constraint migration
    // — left as TODO. See P6 §audit-log followup.
    let decision_str = match decision {
        ApprovalDecision::AutoApprove => "auto_approve",
        ApprovalDecision::RequireApproval { .. } => "user_approve",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tool::ApprovalRequirement;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V14_PERMISSION_TABLES)
            .unwrap();
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
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::RequireApproval { .. }));
    }

    #[test]
    fn session_rule_wins_over_pattern() {
        let db = fresh_db();
        let policy = baseline_policy();
        // Session rule allow
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "session".into(),
                session_id: Some("sess1".into()),
                tool_name: "bash".into(),
                target: None,
                mode: "allow".into(),
            },
        )
        .unwrap();
        // Pattern rule block (lower precedence)
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "pattern".into(),
                session_id: None,
                tool_name: "bash".into(),
                target: Some("rm".into()),
                mode: "block".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"command": "rm -rf /tmp/foo"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn pattern_rule_uses_longest_match() {
        let db = fresh_db();
        let policy = baseline_policy();
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "pattern".into(),
                session_id: None,
                tool_name: "bash".into(),
                target: Some("git ".into()),
                mode: "ask".into(),
            },
        )
        .unwrap();
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "pattern".into(),
                session_id: None,
                tool_name: "bash".into(),
                target: Some("git status".into()),
                mode: "allow".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"command": "git status"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
        // Longest match "git status" wins → allow
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn blocked_list_overrides_everything() {
        let db = fresh_db();
        let mut policy = baseline_policy();
        policy.blocked_tools.insert("bash".into());
        // Even a session-allow rule shouldn't override blocked_tools
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "session".into(),
                session_id: Some("sess1".into()),
                tool_name: "bash".into(),
                target: None,
                mode: "allow".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"command": "ls"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::Block { .. }));
    }

    #[test]
    fn audit_log_records_each_decision() {
        let db = fresh_db();
        let policy = baseline_policy();
        let args = serde_json::json!({"command": "ls"});
        let _ = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
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

    /// A targeted session rule (target = "git status") must NOT auto-pass an
    /// unrelated bash call (e.g. "rm README.md"). Falls through to the global
    /// SafetyMode (Supervised + Always → RequireApproval).
    #[test]
    fn targeted_session_rule_does_not_match_unrelated_command() {
        let db = fresh_db();
        let policy = baseline_policy();
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "session".into(),
                session_id: Some("sess1".into()),
                tool_name: "bash".into(),
                target: Some("git status".into()),
                mode: "allow".into(),
            },
        )
        .unwrap();
        // Matching command → AutoApprove
        let ok_args = serde_json::json!({"command": "git status -uall"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &ok_args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::AutoApprove));
        // Different command → not matched, falls through to RequireApproval.
        let bad_args = serde_json::json!({"command": "rm README.md"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &bad_args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::RequireApproval { .. }));
    }

    /// A legacy tool-wide session rule (target = NULL) keeps matching every
    /// call in the session. This is the pre-fix behavior of "本次会话允许"
    /// preserved for back-compat with rules created before the targeted
    /// version landed.
    #[test]
    fn legacy_tool_wide_session_rule_still_matches_any_command() {
        let db = fresh_db();
        let policy = baseline_policy();
        create_rule(
            &db,
            CreatePermissionRuleInput {
                scope: "session".into(),
                session_id: Some("sess1".into()),
                tool_name: "bash".into(),
                target: None,
                mode: "allow".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"command": "anything goes"});
        let d = resolve_decision(
            &db,
            &policy,
            "sess1",
            "bash",
            &args,
            &ApprovalRequirement::Always,
            None,
        );
        assert!(matches!(d, ApprovalDecision::AutoApprove));
    }

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
        // bash asks (use Always so it doesn't trip the early Never→AutoApprove)
        let d = resolve_decision(&db, &policy, "sess1", "bash", &serde_json::json!({"command":"ls"}),
            &ApprovalRequirement::Always, None);
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
}
