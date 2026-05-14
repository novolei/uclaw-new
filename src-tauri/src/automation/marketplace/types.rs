use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySource {
    pub id: String,
    /// Primary base URL. The adapter appends `/index.json` and
    /// `/{entry.path}/spec.yaml` to this.
    pub url: String,
    /// Optional fallback base URLs tried in order when `url` fails (TLS
    /// handshake, 5xx, timeout). All must use the same path layout as `url`.
    /// Currently used to add the Gitee mirror for GFW-affected users.
    #[serde(default)]
    pub fallback_urls: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
}

impl RegistrySource {
    /// Iterator over `url` then each `fallback_urls` in order — the order
    /// the adapter should try them.
    pub fn url_candidates(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.url.as_str())
            .chain(self.fallback_urls.iter().map(String::as_str))
    }
}

impl Default for RegistrySource {
    fn default() -> Self {
        Self {
            id: "halo".into(),
            url: "https://raw.githubusercontent.com/novolei/digital-human-protocol/main".into(),
            // Gitee mirror — uses the same {base}/index.json + {base}/{path}/spec.yaml
            // layout. Added because raw.githubusercontent.com is unreliable from
            // China (GFW intermittently kills TLS handshakes to it).
            fallback_urls: vec![
                "https://gitee.com/novolei/digital-human-protocol/raw/main".into(),
            ],
            name: Some("Digital Human Protocol (官方)".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryI18n {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryEntry {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(rename = "type")]
    pub app_type: String,
    #[serde(default)]
    pub format: Option<String>,
    pub path: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub requires_mcps: Vec<String>,
    #[serde(default)]
    pub requires_skills: Vec<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub i18n: HashMap<String, EntryI18n>,
    #[serde(default)]
    pub meta: serde_json::Value,
}

fn default_category() -> String {
    "other".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryIndex {
    pub version: u32,
    pub generated_at: String,
    pub source: String,
    pub apps: Vec<RegistryEntry>,
}

/// Trimmed-down view for frontend consumption — camelCase for TS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceItem {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub app_type: String,
    pub category: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub size_bytes: Option<u64>,
    pub min_app_version: Option<String>,
    pub locale: Option<String>,
    /// Full per-locale overlay map carried from the registry index entry.
    /// Keys are locale codes (e.g. "zh-CN", "en-US"); values carry name + description.
    pub i18n: std::collections::HashMap<String, EntryI18n>,
}

impl From<&RegistryEntry> for MarketplaceItem {
    fn from(e: &RegistryEntry) -> Self {
        Self {
            slug: e.slug.clone(),
            name: e.name.clone(),
            version: e.version.clone(),
            author: e.author.clone(),
            description: e.description.clone(),
            app_type: e.app_type.clone(),
            category: e.category.clone(),
            icon: e.icon.clone(),
            tags: e.tags.clone(),
            size_bytes: e.size_bytes,
            min_app_version: e.min_app_version.clone(),
            locale: e.locale.clone(),
            i18n: e.i18n.clone(),
        }
    }
}

/// Page result from `query_marketplace` Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceQueryResult {
    pub items: Vec<MarketplaceItem>,
    pub total: u32,
    pub has_more: bool,
}

/// Full detail surface for the StoreDetail view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDetail {
    pub item: MarketplaceItem,
    /// Raw YAML — populated lazily from the registry on first detail view.
    pub spec_yaml: String,
    /// `spec_yaml` parsed via `parse_humane_v1` — None if parse failed
    /// (UI shows the YAML in a "needs review" box).
    pub parsed_spec_json: Option<serde_json::Value>,
    pub requires_mcps: Vec<String>,
    pub requires_skills: Vec<String>,
    /// Current installed version, if any. None means not installed.
    pub installed_version: Option<String>,
}

/// One row from `check_marketplace_updates`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceUpdate {
    pub slug: String,
    pub installed_version: String,
    pub latest_version: String,
}

/// Tauri install_progress event payload streamed during install_marketplace_human.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInstallProgress {
    pub phase: String,        // 'fetching_spec' | 'parsing' | 'installing' | 'activating' | 'complete'
    pub slug: String,
    pub percent: u8,          // 0..=100
    pub message: Option<String>,
}

/// Result of installing any marketplace item. The automation path carries the
/// installed spec row; skill/mcp paths carry a lighter confirmation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum InstallOutcome {
    Automation { spec: crate::automation::manager::HumaneSpecRow },
    Skill { slug: String, install_path: String },
    Mcp { slug: String, mcp_server_id: String },
}

// ─── Installed Automations (for AppsView card list) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkillBrief {
    pub skill_id: String,
    /// Populated from SKILL.md in Phase 3b-β; None for now.
    pub description: Option<String>,
    /// Absolute FS path where the skill directory lives.
    pub install_path: String,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityStatus {
    Mapped,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCheck {
    pub mcp_id: String,
    pub status: CapabilityStatus,
    /// Human-readable label when status = Mapped (e.g. "uClaw 内建浏览器").
    pub mapped_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAutomation {
    pub slug: String,
    pub name: String,
    pub version: String,
    /// Icon identifier from the registry entry (not present in parsed spec).
    pub icon: Option<String>,
    /// Category from registry entry; defaults to "other" when absent.
    pub category: String,
    pub bundled_skills: Vec<InstalledSkillBrief>,
    pub required_capabilities: Vec<CapabilityCheck>,
}
