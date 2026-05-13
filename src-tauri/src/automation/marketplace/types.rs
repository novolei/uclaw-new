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
    /// Resolved en-US name when present (UI does final locale fallback).
    pub i18n_name: Option<String>,
    /// Resolved en-US description when present.
    pub i18n_description: Option<String>,
}

impl From<&RegistryEntry> for MarketplaceItem {
    fn from(e: &RegistryEntry) -> Self {
        let i18n_en = e.i18n.get("en-US");
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
            i18n_name: i18n_en.and_then(|x| x.name.clone()),
            i18n_description: i18n_en.and_then(|x| x.description.clone()),
        }
    }
}
