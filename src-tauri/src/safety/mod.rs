use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::agent::tools::tool::ApprovalRequirement;

pub mod approval;
pub mod decision;
pub mod path_policy;
pub mod permissions;

pub use approval::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome, ChatApprovalHandler};
pub use decision::{
    SafetyDecisionSource, SafetyToolDecision, SafetyToolDecisionRequest, ToolPermissionCoverage,
};

// ─── Types ──────────────────────────────────────────────────────────────

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

/// Risk level for command analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Approval decision made by the SafetyManager
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ApprovalDecision {
    AutoApprove,
    RequireApproval { reason: String },
    Block { reason: String },
}

/// Command risk assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRiskAssessment {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
    pub suggested_action: ApprovalDecision,
}

/// Safety policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyPolicy {
    pub global_mode: SafetyMode,
    #[serde(default)]
    pub tool_overrides: HashMap<String, SafetyMode>,
    #[serde(default)]
    pub auto_approved_tools: HashSet<String>,
    #[serde(default)]
    pub blocked_tools: HashSet<String>,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        let mut auto_approved = HashSet::new();
        // Read-only tools are safe by default
        auto_approved.insert("read_file".to_string());
        auto_approved.insert("grep".to_string());
        auto_approved.insert("glob".to_string());

        Self {
            global_mode: SafetyMode::Supervised,
            tool_overrides: HashMap::new(),
            auto_approved_tools: auto_approved,
            blocked_tools: HashSet::new(),
        }
    }
}

// ─── SafetyManager ─────────────────────────────────────────────────────

/// SafetyManager handles tool approval decisions and command risk assessment
pub struct SafetyManager {
    policy: SafetyPolicy,
    config_path: PathBuf,
    path_policy: path_policy::PathPolicy,
    path_policy_path: PathBuf,
}

impl SafetyManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let config_path = data_dir.join("safety_policy.json");
        let policy = Self::load_policy(&config_path).unwrap_or_default();

        let path_policy_path = data_dir.join("path_policy.json");
        let path_policy = Self::load_path_policy(&path_policy_path)
            .map(path_policy::PathPolicy::from_persisted)
            .unwrap_or_else(path_policy::PathPolicy::empty);

        tracing::info!("SafetyManager initialized with mode: {:?}", policy.global_mode);
        Self { policy, config_path, path_policy, path_policy_path }
    }

    fn load_policy(path: &std::path::Path) -> Option<SafetyPolicy> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn load_path_policy(path: &std::path::Path) -> Option<path_policy::PathPolicyPersisted> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_path_policy(&self) -> Result<(), crate::error::Error> {
        if let Some(parent) = self.path_policy_path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::error::Error::Io)?;
        }
        let content = serde_json::to_string_pretty(&self.path_policy.to_persisted())
            .map_err(crate::error::Error::Serde)?;
        std::fs::write(&self.path_policy_path, content).map_err(crate::error::Error::Io)?;
        Ok(())
    }

    pub fn save_policy(&self) -> Result<(), crate::error::Error> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::error::Error::Io)?;
        }
        let content = serde_json::to_string_pretty(&self.policy)
            .map_err(crate::error::Error::Serde)?;
        std::fs::write(&self.config_path, content).map_err(crate::error::Error::Io)?;
        Ok(())
    }

    /// Get the current safety policy
    pub fn policy(&self) -> &SafetyPolicy {
        &self.policy
    }

    /// Update the entire safety policy
    pub fn set_policy(&mut self, policy: SafetyPolicy) -> Result<(), crate::error::Error> {
        self.policy = policy;
        self.save_policy()
    }

    /// Set the global safety mode
    pub fn set_global_mode(&mut self, mode: SafetyMode) -> Result<(), crate::error::Error> {
        self.policy.global_mode = mode;
        self.save_policy()
    }

    /// Set a tool-level override
    pub fn set_tool_override(&mut self, tool_name: &str, mode: SafetyMode) -> Result<(), crate::error::Error> {
        self.policy.tool_overrides.insert(tool_name.to_string(), mode);
        self.save_policy()
    }

    /// Remove a tool-level override
    pub fn remove_tool_override(&mut self, tool_name: &str) -> Result<(), crate::error::Error> {
        self.policy.tool_overrides.remove(tool_name);
        self.save_policy()
    }

    /// Add a tool to the auto-approved whitelist
    pub fn add_auto_approved(&mut self, tool_name: &str) -> Result<(), crate::error::Error> {
        self.policy.auto_approved_tools.insert(tool_name.to_string());
        self.save_policy()
    }

    /// Remove a tool from the auto-approved whitelist
    pub fn remove_auto_approved(&mut self, tool_name: &str) -> Result<(), crate::error::Error> {
        self.policy.auto_approved_tools.remove(tool_name);
        self.save_policy()
    }

    /// Block a tool entirely
    pub fn block_tool(&mut self, tool_name: &str) -> Result<(), crate::error::Error> {
        self.policy.blocked_tools.insert(tool_name.to_string());
        self.save_policy()
    }

    /// Unblock a tool
    pub fn unblock_tool(&mut self, tool_name: &str) -> Result<(), crate::error::Error> {
        self.policy.blocked_tools.remove(tool_name);
        self.save_policy()
    }

    /// Determine whether a tool call should be approved, require approval, or be blocked.
    /// This integrates with the existing ApprovalRequirement from the Tool trait.
    ///
    /// `mode_override` allows a session-level safety mode to take precedence over
    /// the global policy mode and tool-level overrides.
    pub fn should_approve(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
        tool_approval: &ApprovalRequirement,
        mode_override: Option<&SafetyMode>,
    ) -> ApprovalDecision {
        let decision = self
            .decide_tool_call(decision::SafetyToolDecisionRequest {
                db: None,
                session_id: "",
                tool_name,
                arguments: _arguments,
                tool_approval,
                mode_override,
                permission_coverage: None,
            })
            .decision;

        tracing::info!(
            tool = %tool_name,
            decision = ?decision,
            "Safety decision result"
        );

        decision
    }

    pub fn decide_tool_call(
        &self,
        request: decision::SafetyToolDecisionRequest<'_>,
    ) -> decision::SafetyToolDecision {
        decision::decide_tool_call(&self.policy, request)
    }

    /// DB-backed approval resolver. Replaces the in-memory `should_approve`
    /// flow with one that consults `tool_permission_rules` (session + pattern
    /// scopes) before falling through to the legacy global tier, and writes
    /// one row to `permission_audit_log` per call.
    ///
    /// Existing `should_approve` is kept for tests / call sites that don't
    /// have a DB handle.
    pub fn should_approve_with_db(
        &self,
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        session_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
        tool_approval: &ApprovalRequirement,
        mode_override: Option<&SafetyMode>,
    ) -> ApprovalDecision {
        self.decide_tool_call(decision::SafetyToolDecisionRequest {
            db: Some(db),
            session_id,
            tool_name,
            arguments,
            tool_approval,
            mode_override,
            permission_coverage: None,
        })
        .decision
    }

    /// Assess the risk of a shell command
    pub fn assess_command_risk(&self, command: &str) -> CommandRiskAssessment {
        let mut reasons = Vec::new();
        let mut level = RiskLevel::Low;

        // Normalize for analysis
        let cmd_lower = command.to_lowercase();
        let parts: Vec<&str> = command.split_whitespace().collect();

        // ── File system risk detection ──
        let fs_dangerous = ["rm ", "rm\t", "rmdir", "format", "mkfs", "dd ", "shred"];
        for pattern in &fs_dangerous {
            if cmd_lower.contains(pattern) {
                reasons.push(format!("Destructive filesystem command detected: {}", pattern.trim()));
                level = std::cmp::max(level, RiskLevel::High);
            }
        }

        // rm -rf is critical
        if cmd_lower.contains("rm") && (cmd_lower.contains("-rf") || cmd_lower.contains("-fr") || cmd_lower.contains("--force")) {
            reasons.push("Recursive force delete detected (rm -rf)".to_string());
            level = RiskLevel::Critical;
        }

        // ── Network risk detection ──
        let net_suspicious = ["nmap", "netcat", "nc ", "nc\t"];
        for pattern in &net_suspicious {
            if cmd_lower.contains(pattern) {
                reasons.push(format!("Suspicious network tool detected: {}", pattern.trim()));
                level = std::cmp::max(level, RiskLevel::High);
            }
        }

        // curl/wget to external URLs
        if cmd_lower.contains("curl") || cmd_lower.contains("wget") {
            reasons.push("External network request detected".to_string());
            level = std::cmp::max(level, RiskLevel::Medium);

            // POST with curl is higher risk (potential data exfiltration)
            if cmd_lower.contains("-x post") || cmd_lower.contains("--data") || cmd_lower.contains("-d ") {
                reasons.push("Data being sent to external URL (potential exfiltration)".to_string());
                level = std::cmp::max(level, RiskLevel::High);
            }
        }

        // ── Privilege escalation detection ──
        if parts.first().map(|s| *s == "sudo" || *s == "su").unwrap_or(false) {
            reasons.push("Privilege escalation attempt detected".to_string());
            level = std::cmp::max(level, RiskLevel::Critical);
        }

        // chmod 777
        if cmd_lower.contains("chmod") && cmd_lower.contains("777") {
            reasons.push("World-writable permission change detected (chmod 777)".to_string());
            level = std::cmp::max(level, RiskLevel::High);
        }

        // chmod +s (setuid)
        if cmd_lower.contains("chmod") && cmd_lower.contains("+s") {
            reasons.push("Setuid bit change detected".to_string());
            level = std::cmp::max(level, RiskLevel::Critical);
        }

        // ── Data exfiltration patterns ──
        // Piping file contents to network commands
        if (cmd_lower.contains("cat") || cmd_lower.contains("base64"))
            && (cmd_lower.contains("curl") || cmd_lower.contains("wget") || cmd_lower.contains("nc "))
        {
            reasons.push("Potential data exfiltration: file content piped to network".to_string());
            level = std::cmp::max(level, RiskLevel::Critical);
        }

        // ── Environment/system modification ──
        if cmd_lower.contains("export") && cmd_lower.contains("path=") {
            reasons.push("PATH modification detected".to_string());
            level = std::cmp::max(level, RiskLevel::Medium);
        }

        // ── Package installation ──
        let pkg_cmds = ["pip install", "npm install", "brew install", "apt install", "yum install", "cargo install"];
        for pattern in &pkg_cmds {
            if cmd_lower.contains(pattern) {
                reasons.push(format!("Package installation detected: {}", pattern));
                level = std::cmp::max(level, RiskLevel::Medium);
            }
        }

        // ── Determine suggested action ──
        let suggested_action = match level {
            RiskLevel::Low => ApprovalDecision::AutoApprove,
            RiskLevel::Medium => ApprovalDecision::RequireApproval {
                reason: reasons.join("; "),
            },
            RiskLevel::High | RiskLevel::Critical => ApprovalDecision::RequireApproval {
                reason: format!("High-risk command: {}", reasons.join("; ")),
            },
        };

        if !reasons.is_empty() {
            tracing::info!(
                "Command risk assessment: level={:?}, reasons={:?}, cmd={}",
                level,
                reasons,
                &command[..command.len().min(100)]
            );
        }

        CommandRiskAssessment {
            level,
            reasons,
            suggested_action,
        }
    }

    // ─── PathPolicy proxy ─────────────────────────────────────────────

    /// Decide whether the given candidate paths can be accessed without a prompt.
    /// `mode_override` of `Yolo` short-circuits to Allow (matches existing
    /// should_approve semantics).
    pub fn check_paths(
        &self,
        session_id: &str,
        workspace_root: &std::path::Path,
        workspace_attached: &[PathBuf],
        session_attached: &[PathBuf],
        candidates: &[PathBuf],
        mode_override: Option<&SafetyMode>,
    ) -> path_policy::PathDecision {
        if matches!(mode_override, Some(SafetyMode::Yolo)) || matches!(self.policy.global_mode, SafetyMode::Yolo) {
            return path_policy::PathDecision::Allow;
        }
        for c in candidates {
            match self.path_policy.check(
                session_id,
                workspace_root,
                workspace_attached,
                session_attached,
                c,
            ) {
                path_policy::PathDecision::Allow => continue,
                other => return other,
            }
        }
        path_policy::PathDecision::Allow
    }

    pub fn list_always_allowed_paths(&self) -> &[PathBuf] {
        self.path_policy.list_global()
    }

    pub fn add_always_allowed_path(&mut self, p: PathBuf) -> Result<(), crate::error::Error> {
        self.path_policy.add_global(p);
        self.save_path_policy()
    }

    pub fn remove_always_allowed_path(&mut self, p: &std::path::Path) -> Result<(), crate::error::Error> {
        self.path_policy.remove_global(p);
        self.save_path_policy()
    }

    pub fn list_session_allowed_paths(&self, sid: &str) -> Vec<PathBuf> {
        self.path_policy.list_for_session(sid)
    }

    pub fn allow_path_for_session(&mut self, sid: &str, p: PathBuf) {
        // Session grants are in-memory only — no save.
        self.path_policy.allow_for_session(sid, p);
    }

    pub fn promote_session_path_to_global(&mut self, sid: &str, p: &std::path::Path) -> Result<(), crate::error::Error> {
        self.path_policy.promote_session_to_global(sid, p);
        self.save_path_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_policy_persists_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        {
            let mut mgr = SafetyManager::new(tmp.path());
            mgr.add_always_allowed_path(outside.path().to_path_buf()).unwrap();
            assert_eq!(mgr.list_always_allowed_paths(), &[outside.path().to_path_buf()]);
        }
        // Re-open: the file at <tmp>/path_policy.json should round-trip.
        let mgr2 = SafetyManager::new(tmp.path());
        assert_eq!(mgr2.list_always_allowed_paths(), &[outside.path().to_path_buf()]);
    }

    #[test]
    fn check_paths_inside_workspace_allows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let target = ws.path().join("a.txt");
        std::fs::write(&target, "x").unwrap();
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[target],
            None,
        );
        assert_eq!(decision, crate::safety::path_policy::PathDecision::Allow);
    }

    #[test]
    fn check_paths_yolo_mode_short_circuits() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap().path().join("b.txt");
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[outside],
            Some(&SafetyMode::Yolo),
        );
        assert_eq!(decision, crate::safety::path_policy::PathDecision::Allow);
    }

    #[test]
    fn check_paths_outside_prompts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap().path().join("c.txt");
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[outside],
            None,
        );
        assert!(matches!(decision, crate::safety::path_policy::PathDecision::Prompt { .. }));
    }
}
