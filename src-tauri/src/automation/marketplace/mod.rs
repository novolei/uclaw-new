pub mod halo_adapter;
pub mod types;

pub use types::{MarketplaceItem, RegistryEntry, RegistryIndex, RegistrySource};

#[cfg(test)]
mod tests {
    use super::types::*;

    // Fixture mirrors the real index.json shape from
    // https://raw.githubusercontent.com/novolei/digital-human-protocol/main/index.json
    const SAMPLE_INDEX: &str = r#"{
        "version": 1,
        "generated_at": "2026-05-10T14:21:37.196Z",
        "source": "https://openkursar.github.io/digital-human-protocol",
        "apps": [
            {
                "slug": "ai-daily-news",
                "name": "AI 每日新闻播报",
                "version": "1.0.0",
                "author": "openkursar",
                "description": "每天自动搜集 AI/大模型/Agent 领域最新新闻。",
                "type": "automation",
                "format": "bundle",
                "path": "packages/digital-humans/ai-daily-news",
                "size_bytes": 5674,
                "category": "content",
                "tags": ["AI", "news"],
                "icon": "news",
                "i18n": {
                    "en-US": {
                        "name": "AI Daily News Digest",
                        "description": "Auto-curates daily AI news."
                    }
                }
            },
            {
                "slug": "boss-job-monitor",
                "name": "Boss Job Monitor",
                "version": "1.0.0",
                "author": "openkursar",
                "description": "Monitors Boss Zhipin daily for new job listings.",
                "type": "automation",
                "format": "bundle",
                "path": "packages/digital-humans/boss-job-monitor",
                "category": "productivity",
                "requires_mcps": ["ai-browser"]
            }
        ]
    }"#;

    #[test]
    fn parses_real_index_shape() {
        let idx: RegistryIndex = serde_json::from_str(SAMPLE_INDEX).unwrap();
        assert_eq!(idx.version, 1);
        assert_eq!(idx.apps.len(), 2);
        assert_eq!(idx.apps[0].slug, "ai-daily-news");
        assert_eq!(idx.apps[0].app_type, "automation");
        assert_eq!(idx.apps[1].requires_mcps, vec!["ai-browser".to_string()]);
    }

    #[test]
    fn marketplace_item_resolves_en_us_i18n() {
        let idx: RegistryIndex = serde_json::from_str(SAMPLE_INDEX).unwrap();
        let item: MarketplaceItem = (&idx.apps[0]).into();
        assert_eq!(item.i18n_name.as_deref(), Some("AI Daily News Digest"));
        assert_eq!(
            item.i18n_description.as_deref(),
            Some("Auto-curates daily AI news.")
        );
    }

    #[test]
    fn marketplace_item_handles_missing_i18n() {
        let idx: RegistryIndex = serde_json::from_str(SAMPLE_INDEX).unwrap();
        let item: MarketplaceItem = (&idx.apps[1]).into();
        assert_eq!(item.i18n_name, None);
    }

    #[test]
    fn default_registry_source_points_to_dhp() {
        let s = RegistrySource::default();
        assert_eq!(s.id, "halo");
        assert!(s.url.contains("digital-human-protocol"));
    }

    #[test]
    fn entry_defaults_kick_in_for_optional_fields() {
        let minimal = r#"{
            "slug": "x", "name": "X", "version": "1.0", "author": "a",
            "description": "d", "type": "automation", "path": "p"
        }"#;
        let entry: RegistryEntry = serde_json::from_str(minimal).unwrap();
        assert_eq!(entry.category, "other");
        assert!(entry.tags.is_empty());
        assert!(entry.requires_mcps.is_empty());
    }
}
