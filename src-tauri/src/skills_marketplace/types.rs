//! Wire types shared by the marketplace clients, install service and commands.

use serde::{Deserialize, Serialize};

/// One search/list row, normalized across providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillSummary {
    /// skills.sh: `owner/repo/path`; skillsmp: its own id.
    pub id: String,
    pub slug: String,
    pub name: String,
    /// `owner/repo` (skills.sh) or author (skillsmp).
    pub source: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(rename = "sourceType", default)]
    pub source_type: String,
    /// What install/detail fetch from: `owner/repo/path` or a GitHub URL.
    #[serde(rename = "installUrl", default)]
    pub install_url: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub description: String,
}

/// One file of a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillFile {
    pub path: String,
    pub contents: String,
}

/// A skill with its files inline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillDetail {
    pub id: String,
    pub source: String,
    pub slug: String,
    /// Version marker (SKILL.md blob sha); stored in V25 `version`.
    #[serde(default)]
    pub hash: String,
    pub files: Vec<SkillFile>,
}

/// skills.sh security audit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SkillAudit {
    #[serde(default)]
    pub audits: Vec<SkillAuditEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillAuditEntry {
    #[serde(default)]
    pub status: String,
    /// "LOW" | "MEDIUM" | "HIGH"
    #[serde(rename = "riskLevel", default)]
    pub risk_level: String,
    #[serde(default)]
    pub summary: String,
}

/// Where to install a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    Global,
    Workspace,
}

/// Which marketplace a call targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceProvider {
    /// skills.sh — anonymous search; key unlocks list/detail/audit.
    #[default]
    SkillsSh,
    /// skillsmp.com — search only; install/preview via GitHub.
    Skillsmp,
}

/// Errors surfaced to the UI. Messages are user-facing (Chinese).
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("需要先在设置中填写该市场的 API Key")]
    MissingApiKey,
    #[error("请求太频繁，请稍后再试或在设置中填写 API Key")]
    RateLimited,
    #[error("市场请求失败：{0}")]
    Http(String),
    #[error("安装失败：{0}")]
    Install(String),
    #[error("无效的技能标识或路径：{0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_summary_parses_search_item() {
        let v = serde_json::json!({
            "id": "expo/skills/react-native", "slug": "react-native", "name": "React Native",
            "source": "expo/skills", "installs": 3842, "sourceType": "github",
            "installUrl": "https://github.com/expo/skills", "url": "https://skills.sh/expo/skills/react-native"
        });
        let s: SkillSummary = serde_json::from_value(v).unwrap();
        assert_eq!(s.id, "expo/skills/react-native");
        assert_eq!(s.installs, 3842);
        assert_eq!(s.install_url, "https://github.com/expo/skills");
        assert_eq!(s.description, "");
    }

    #[test]
    fn provider_and_scope_wire_names() {
        assert_eq!(serde_json::to_string(&MarketplaceProvider::SkillsSh).unwrap(), "\"skills_sh\"");
        assert_eq!(serde_json::to_string(&MarketplaceProvider::Skillsmp).unwrap(), "\"skillsmp\"");
        assert_eq!(serde_json::to_string(&InstallScope::Workspace).unwrap(), "\"workspace\"");
        assert_eq!(MarketplaceProvider::default(), MarketplaceProvider::SkillsSh);
    }
}
