pub mod cache;
pub mod halo_adapter;
pub mod types;

pub use types::{
    EntryI18n, MarketplaceDetail, MarketplaceInstallProgress, MarketplaceItem,
    MarketplaceQueryResult, MarketplaceUpdate, RegistryEntry, RegistryIndex, RegistrySource,
};

use anyhow::{anyhow, Context, Result};

use crate::automation::manager::HumaneSpecRow;
use crate::automation::runtime::AppRuntimeService;

/// List all automation-type entries from a registry. Defaults to the DHP registry.
/// Non-automation entries (skill / mcp / extension) are filtered out — Phase 1 only
/// installs full automations.
pub async fn list_humans(registry_url: Option<String>) -> Result<Vec<MarketplaceItem>> {
    let source = match registry_url {
        Some(url) => RegistrySource {
            id: "custom".into(),
            url,
            fallback_urls: vec![],
            name: None,
        },
        None => RegistrySource::default(),
    };
    let index = halo_adapter::fetch_index(&source).await?;
    Ok(index.apps.iter()
        .filter(|e| e.app_type == "automation")
        .map(MarketplaceItem::from)
        .collect())
}

/// Paged query backed by the V23a SQLite cache. Triggers a sync first if
/// the cache is stale (or empty); falls back to stale data on sync failure.
pub async fn query_marketplace_cached(
    runtime: &AppRuntimeService,
    search: Option<String>,
    item_type: Option<String>,
    category: Option<String>,
    page: u32,
    page_size: u32,
) -> Result<MarketplaceQueryResult> {
    let source = RegistrySource::default();
    let _ = cache::sync_registry(&runtime.db, &source, false).await;

    let conn = runtime.db.lock().unwrap();
    cache::query_items(
        &conn,
        search.as_deref(),
        item_type.as_deref(),
        category.as_deref(),
        page,
        page_size,
    )
}

/// Full detail surface — pulls cached metadata, lazy-fetches spec_yaml on
/// first view, attempts to parse + identifies the installed version.
pub async fn get_marketplace_detail_cached(
    runtime: &AppRuntimeService,
    slug: &str,
) -> Result<MarketplaceDetail> {
    let source = RegistrySource::default();
    let _ = cache::sync_registry(&runtime.db, &source, false).await;

    // Look up the row + cached spec
    let (item, cached_yaml, requires_json, _i18n_json) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found in cache: {}", slug))?
    };

    // Fetch + cache spec_yaml if not present
    let spec_yaml = if let Some(y) = cached_yaml {
        y
    } else {
        let path = format!("packages/digital-humans/{}", slug);
        let entry = RegistryEntry {
            slug: slug.to_string(),
            name: item.name.clone(),
            version: item.version.clone(),
            author: item.author.clone(),
            description: item.description.clone(),
            app_type: item.app_type.clone(),
            format: None,
            path,
            download_url: None,
            size_bytes: item.size_bytes,
            checksum: None,
            category: item.category.clone(),
            tags: item.tags.clone(),
            icon: item.icon.clone(),
            locale: item.locale.clone(),
            min_app_version: item.min_app_version.clone(),
            requires_mcps: vec![],
            requires_skills: vec![],
            updated_at: None,
            i18n: Default::default(),
            meta: serde_json::Value::Null,
        };
        let yaml = halo_adapter::fetch_spec_yaml(&source, &entry).await?;
        let conn = runtime.db.lock().unwrap();
        cache::cache_spec_yaml(&conn, &source.id, slug, &yaml)?;
        yaml
    };

    // Parse the YAML (best-effort — None if parse fails so UI can still render
    // the rest of the detail). Surface the error to cargo logs so we can see
    // which marketplace specs trip our schema and fix the parser tactically.
    let parsed_spec_json = match crate::automation::protocol::parse::parse_humane_v1(&spec_yaml) {
        Ok(p) => match serde_json::to_value(&p.spec) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(slug = %slug, error = %e, "spec.yaml parsed but serde_json::to_value failed");
                None
            }
        },
        Err(e) => {
            tracing::warn!(slug = %slug, error = %e, "spec.yaml parse failed — config preview will fall back");
            None
        }
    };

    // Extract dependencies from cached requires_json
    let requires: serde_json::Value =
        serde_json::from_str(&requires_json).unwrap_or(serde_json::json!({}));
    let requires_mcps = requires
        .get("mcps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let requires_skills = requires
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Look up installed version (if installed)
    let installed_version = {
        let conn = runtime.db.lock().unwrap();
        let source_ref = format!("marketplace://{}/{}", source.id, slug);
        conn.query_row(
            "SELECT version FROM automation_specs WHERE source = 'marketplace' AND source_ref = ?1",
            rusqlite::params![source_ref],
            |r| r.get::<_, String>(0),
        )
        .ok()
    };

    Ok(MarketplaceDetail {
        item,
        spec_yaml,
        parsed_spec_json,
        requires_mcps,
        requires_skills,
        installed_version,
    })
}

/// Compare every installed marketplace spec against the cached registry.
pub async fn check_updates_cached(
    runtime: &AppRuntimeService,
) -> Result<Vec<MarketplaceUpdate>> {
    let source = RegistrySource::default();
    let _ = cache::sync_registry(&runtime.db, &source, false).await;

    let conn = runtime.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT s.source_ref, s.version, m.version
         FROM automation_specs s
         JOIN automation_marketplace_items m ON m.registry_id = ?1
            AND ('marketplace://' || ?1 || '/' || m.slug) = s.source_ref
         WHERE s.source = 'marketplace' AND s.version != m.version",
    )?;
    let registry_id = source.id.clone();
    let rows = stmt.query_map(rusqlite::params![registry_id], move |r| {
        let source_ref: String = r.get(0)?;
        let installed: String = r.get(1)?;
        let latest: String = r.get(2)?;
        let slug = source_ref
            .strip_prefix(&format!("marketplace://{}/", source.id))
            .unwrap_or("")
            .to_string();
        Ok(MarketplaceUpdate {
            slug,
            installed_version: installed,
            latest_version: latest,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Install a single registry entry. Returns the installed HumaneSpecRow.
/// source_ref takes the form `marketplace://halo/{slug}` per spec § 5 URI convention.
pub async fn install_human(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    progress_channel: Option<String>,
) -> Result<HumaneSpecRow> {
    use tauri::Emitter;

    let source = RegistrySource::default();
    let emit = |phase: &str, percent: u8, message: Option<&str>| {
        if let Some(ch) = &progress_channel {
            let _ = app_handle.emit(
                ch,
                MarketplaceInstallProgress {
                    phase: phase.into(),
                    slug: slug.to_string(),
                    percent,
                    message: message.map(String::from),
                },
            );
        }
    };

    emit("fetching_spec", 10, Some("拉取 spec.yaml"));
    let _ = cache::sync_registry(&runtime.db, &source, false).await;
    let (item, cached_yaml, _, _) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found in registry: {}", slug))?
    };
    if item.app_type != "automation" {
        return Err(anyhow!(
            "entry '{}' is type '{}', only 'automation' installable in Phase 3a",
            slug,
            item.app_type
        ));
    }

    let yaml = if let Some(y) = cached_yaml {
        y
    } else {
        let path = format!("packages/digital-humans/{}", slug);
        let entry = RegistryEntry {
            slug: slug.to_string(),
            name: item.name.clone(),
            version: item.version.clone(),
            author: item.author.clone(),
            description: item.description.clone(),
            app_type: item.app_type.clone(),
            format: None,
            path,
            download_url: None,
            size_bytes: None,
            checksum: None,
            category: "other".into(),
            tags: vec![],
            icon: None,
            locale: None,
            min_app_version: None,
            requires_mcps: vec![],
            requires_skills: vec![],
            updated_at: None,
            i18n: Default::default(),
            meta: serde_json::Value::Null,
        };
        let yaml = halo_adapter::fetch_spec_yaml(&source, &entry).await?;
        let conn = runtime.db.lock().unwrap();
        cache::cache_spec_yaml(&conn, &source.id, slug, &yaml)?;
        yaml
    };

    emit("installing", 60, Some("安装到数据库"));
    let source_ref = format!("marketplace://{}/{}", source.id, slug);

    // Re-install dedup: delete any existing row pointing at this source_ref.
    // install_humane_spec_from_source generates a fresh UUID per call, so
    // without this we'd accumulate duplicates every time the user clicks
    // "重新安装". CASCADE on the V20 schema handles activities / V21 tables.
    {
        let conn = runtime.db.lock().unwrap();
        let removed: i64 = conn
            .execute(
                "DELETE FROM automation_specs WHERE source = 'marketplace' AND source_ref = ?1",
                rusqlite::params![source_ref],
            )
            .unwrap_or(0) as i64;
        if removed > 0 {
            tracing::info!(slug = %slug, removed, "marketplace re-install: dropped existing spec row before reinstall");
        }
    }

    let row = runtime
        .install_humane_spec_from_source(&yaml, "marketplace", Some(source_ref))
        .await
        .with_context(|| format!("install_humane_spec failed for '{}'", slug))?;

    if let Some(cfg) = user_config {
        let conn = runtime.db.lock().unwrap();
        let _ = conn.execute(
            "UPDATE automation_specs SET user_config_values = ?1 WHERE id = ?2",
            rusqlite::params![cfg.to_string(), row.id],
        );
    }
    let _ = space_id; // Phase 3b will wire this

    emit("activating", 90, Some("激活订阅"));
    let _ = runtime.activate(&row.id).await;

    emit("complete", 100, Some("已安装"));
    let _ = app_handle.emit("chat:pet-celebrate", serde_json::json!({"slug": slug}));

    Ok(row)
}

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
