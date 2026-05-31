use serde::{Deserialize, Serialize};

use crate::plugins::LoadedPlugin;

pub const PLUGIN_PREFLIGHT_SCHEMA: &str = "uclaw.plugin.preflight.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPreflightVerdict {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPreflightSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPreflightCategory {
    Runtime,
    Permission,
    Compatibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPreflightFinding {
    pub severity: PluginPreflightSeverity,
    pub category: PluginPreflightCategory,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPreflightSummary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPreflightReport {
    pub schema: String,
    pub plugin_id: String,
    pub verdict: PluginPreflightVerdict,
    pub findings: Vec<PluginPreflightFinding>,
    pub summary: PluginPreflightSummary,
}

impl PluginPreflightReport {
    pub fn from_findings(
        plugin_id: impl Into<String>,
        findings: Vec<PluginPreflightFinding>,
    ) -> Self {
        let mut summary = PluginPreflightSummary::default();
        for finding in &findings {
            match finding.severity {
                PluginPreflightSeverity::Error => summary.errors += 1,
                PluginPreflightSeverity::Warning => summary.warnings += 1,
                PluginPreflightSeverity::Info => summary.info += 1,
            }
        }

        let verdict = if summary.errors > 0 {
            PluginPreflightVerdict::Fail
        } else if summary.warnings > 0 {
            PluginPreflightVerdict::Warn
        } else {
            PluginPreflightVerdict::Pass
        };

        Self {
            schema: PLUGIN_PREFLIGHT_SCHEMA.to_string(),
            plugin_id: plugin_id.into(),
            verdict,
            findings,
            summary,
        }
    }

    pub fn for_loaded_plugin(loaded: &LoadedPlugin) -> Self {
        let mut findings = Vec::new();
        let manifest = &loaded.manifest;
        let contributes_mcp = !manifest.contributes.mcp_servers.is_empty();

        if contributes_mcp && !manifest.permissions.run_subprocess {
            findings.push(PluginPreflightFinding {
                severity: PluginPreflightSeverity::Error,
                category: PluginPreflightCategory::Permission,
                message: "plugin declares mcp_servers but is missing run_subprocess permission"
                    .to_string(),
            });
        }

        if contributes_mcp && manifest.runtime.executable.is_none() {
            findings.push(PluginPreflightFinding {
                severity: PluginPreflightSeverity::Error,
                category: PluginPreflightCategory::Runtime,
                message: "plugin declares mcp_servers but has no runtime.executable".to_string(),
            });
        }

        if contributes_mcp {
            if let Some(kind) = &manifest.runtime.kind {
                if kind != "subprocess" {
                    findings.push(PluginPreflightFinding {
                        severity: PluginPreflightSeverity::Error,
                        category: PluginPreflightCategory::Runtime,
                        message: format!("unsupported runtime kind `{kind}` for MCP plugin"),
                    });
                }
            }
        }

        if contributes_mcp {
            if let Some(executable) = &manifest.runtime.executable {
                let exe_path = std::path::Path::new(executable);
                let resolved = if exe_path.is_absolute() {
                    exe_path.to_path_buf()
                } else {
                    loaded.plugin_dir.join(exe_path)
                };
                if !resolved.exists() {
                    findings.push(PluginPreflightFinding {
                        severity: PluginPreflightSeverity::Warning,
                        category: PluginPreflightCategory::Runtime,
                        message: format!(
                            "runtime.executable does not exist yet: {}",
                            resolved.display()
                        ),
                    });
                }
            }
        }

        Self::from_findings(manifest.id.clone(), findings)
    }

    pub fn can_contribute_mcp_config(&self) -> bool {
        !matches!(self.verdict, PluginPreflightVerdict::Fail)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTrustState {
    Pending,
    Acknowledged,
    Trusted,
    Killed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeStatusKind {
    Loaded,
    Skipped,
    Failed,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRuntimeStatus {
    pub plugin_id: String,
    pub trust_state: PluginTrustState,
    pub status: PluginRuntimeStatusKind,
    pub reason: Option<String>,
}

impl PluginRuntimeStatus {
    pub fn loaded(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            trust_state: PluginTrustState::Pending,
            status: PluginRuntimeStatusKind::Loaded,
            reason: None,
        }
    }

    pub fn skipped(plugin_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            trust_state: PluginTrustState::Pending,
            status: PluginRuntimeStatusKind::Skipped,
            reason: Some(reason.into()),
        }
    }

    pub fn killed(plugin_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            trust_state: PluginTrustState::Killed,
            status: PluginRuntimeStatusKind::Killed,
            reason: Some(reason.into()),
        }
    }
}
