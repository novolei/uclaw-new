pub mod cache;
pub mod halo_adapter;
mod skill_install;
pub mod types;

pub use cache::category_counts_cached;
pub use types::{
    EntryI18n, MarketplaceDetail, MarketplaceInstallProgress, MarketplaceItem,
    MarketplaceQueryResult, MarketplaceUpdate, RegistryEntry, RegistryIndex, RegistrySource,
};

use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::automation::manager::HumaneSpecRow;
use crate::automation::runtime::AppRuntimeService;
use crate::skills::SkillsRegistry;

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

/// Synchronous core of `list_installed_marketplace_automations` — separated so
/// unit tests can drive it without an AppRuntimeService.
///
/// Joins `automation_specs` (marketplace rows only) × `automation_installed_skills`
/// × `automation_marketplace_items` (for icon + category). Capability resolution
/// is performed in-process via `capability_map`.
pub fn list_installed_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
) -> Result<Vec<types::InstalledAutomation>> {
    use crate::automation::capability_map;
    use crate::automation::protocol::parse::parse_humane_v1;
    use types::{CapabilityCheck, CapabilityStatus, InstalledAutomation, InstalledSkillBrief};

    // source_ref shape: `marketplace://halo/<slug>` — we extract the slug as the
    // last path segment. LEFT JOIN against the registry cache to recover icon +
    // category (those fields live in the index, not in the stored spec YAML).
    let mut stmt = conn.prepare(
        "SELECT
            s.source_ref,
            s.name,
            s.version,
            s.spec_yaml,
            COALESCE(m.icon,     NULL)    AS icon,
            COALESCE(m.category, 'other') AS category
         FROM automation_specs s
         LEFT JOIN automation_marketplace_items m
             ON m.registry_id = 'halo'
             AND m.slug = SUBSTR(s.source_ref, LENGTH('marketplace://halo/') + 1)
         WHERE s.source = 'marketplace'
         ORDER BY s.updated_at DESC",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (source_ref, name, version, spec_yaml, icon, category) = row?;
        // Extract slug from the last '/' segment of source_ref.
        let slug = source_ref
            .rsplit('/')
            .next()
            .ok_or_else(|| anyhow!("malformed source_ref: {}", source_ref))?
            .to_string();

        // Parse the stored YAML to reach requires.mcps. Failure is non-fatal:
        // return empty capability list so AppsView can still render the card.
        let mcp_ids: Vec<String> = match parse_humane_v1(&spec_yaml) {
            Ok(parsed) => parsed
                .spec
                .requires
                .as_ref()
                .and_then(|r| r.get("mcps").and_then(|s| s.as_array()))
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            Err(e) => {
                tracing::warn!(slug = %slug, error = %e, "list_installed: parse failed — capability list empty");
                vec![]
            }
        };

        let required_capabilities: Vec<CapabilityCheck> = mcp_ids
            .into_iter()
            .map(|mcp_id| match capability_map::resolve_capability(&mcp_id) {
                Some(cap) => CapabilityCheck {
                    mcp_id,
                    status: CapabilityStatus::Mapped,
                    mapped_to: Some(capability_map::human_label(cap).to_string()),
                },
                None => CapabilityCheck {
                    mcp_id,
                    status: CapabilityStatus::Missing,
                    mapped_to: None,
                },
            })
            .collect();

        // Bundled skills — one row per skill in automation_installed_skills.
        let mut skills_stmt = conn.prepare(
            "SELECT skill_id, file_count FROM automation_installed_skills \
                WHERE automation_slug = ? ORDER BY skill_id",
        )?;
        let skill_rows = skills_stmt.query_map(rusqlite::params![&slug], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut bundled_skills: Vec<InstalledSkillBrief> = Vec::new();
        for s in skill_rows {
            let (skill_id, file_count) = s?;
            let install_path = skills_root
                .join("_marketplace")
                .join(&slug)
                .join(&skill_id)
                .to_string_lossy()
                .to_string();
            bundled_skills.push(InstalledSkillBrief {
                skill_id,
                description: None, // Phase 3b-β reads SKILL.md
                install_path,
                file_count,
            });
        }

        out.push(InstalledAutomation {
            slug,
            name,
            version,
            icon,
            category,
            bundled_skills,
            required_capabilities,
        });
    }
    Ok(out)
}

/// Async wrapper consumed by the Tauri command.
pub async fn list_installed(
    runtime: &crate::automation::runtime::AppRuntimeService,
) -> Result<Vec<types::InstalledAutomation>> {
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    let conn = runtime.db.lock().unwrap();
    list_installed_inner(&conn, &skills_root)
}

/// Synchronous core of uninstall — separated so unit tests can drive it
/// without an AppRuntimeService.
pub fn uninstall_human_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
    slug: &str,
) -> Result<()> {
    let source_ref = format!("marketplace://halo/{}", slug);
    // Delete spec first — a subsequent FS error leaves no ghost "installed" spec row.
    conn.execute(
        "DELETE FROM automation_specs WHERE source = 'marketplace' AND source_ref = ?1",
        rusqlite::params![source_ref],
    )?;
    conn.execute(
        "DELETE FROM automation_installed_skills WHERE automation_slug = ?1",
        rusqlite::params![slug],
    )?;
    let dir = skills_root.join("_marketplace").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("remove {}", dir.display()))?;
    }
    Ok(())
}

/// Public async entry point used by the Tauri command. Resolves runtime
/// resources, calls the inner sync core, then drops the SkillsRegistry
/// scan dir and triggers rediscovery.
pub async fn uninstall_human(
    runtime: &crate::automation::runtime::AppRuntimeService,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    slug: &str,
) -> Result<()> {
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    {
        let conn = runtime.db.lock().unwrap();
        uninstall_human_inner(&conn, &skills_root, slug)?;
    }
    {
        let mut reg = skills_registry.write().await;
        reg.remove_scan_dir(&skills_root.join("_marketplace").join(slug));
        let _ = reg.discover();
    }
    Ok(())
}

/// Install a single registry entry. Returns the installed HumaneSpecRow.
/// source_ref takes the form `marketplace://halo/{slug}` per spec § 5 URI convention.
pub async fn install_human(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
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

    // NEW: fetching_skills phase
    emit("fetching_skills", 25, Some("拉取依赖 skill 文件"));
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    // Parse spec here so we can inspect requires.skills before installing.
    let parsed = match crate::automation::protocol::parse::parse_humane_v1(&yaml) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("spec.yaml 解析失败：{:#}", e);
            emit("fetching_skills", 25, Some(&msg));
            return Err(anyhow!("parse spec.yaml for {} during fetching_skills phase: {:#}", slug, e));
        }
    };
    let staged = match skill_install::fetch_bundled_skills(&source, &{
        let path = format!("packages/digital-humans/{}", slug);
        RegistryEntry {
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
        }
    }, &parsed.spec, &skills_root).await {
        Ok(s) => s,
        Err(e) => {
            skill_install::cleanup_staging(
                &skills_root.join(".staging").join(slug),
            );
            return Err(e);
        }
    };
    tracing::info!(slug = %slug, count = staged.len(), "bundled skills staged");

    // NEW: validating_caps phase
    emit("validating_caps", 50, Some("校验能力依赖"));
    let mcp_ids: Vec<String> = parsed
        .spec
        .requires
        .as_ref()
        .and_then(|r| r.get("mcps").and_then(|s| s.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut missing_caps: Vec<String> = Vec::new();
    for mcp_id in &mcp_ids {
        if crate::automation::capability_map::resolve_capability(mcp_id).is_none() {
            missing_caps.push(mcp_id.clone());
        }
    }
    if !missing_caps.is_empty() {
        // Warn but don't abort — Phase 3b-γ will offer a real install path.
        let msg = format!(
            "automation 声明依赖 MCP {:?}，但 uClaw 暂不支持，安装完成但可能无法运行",
            missing_caps
        );
        emit("validating_caps", 55, Some(&msg));
        tracing::warn!(missing = ?missing_caps, slug = %slug, "capability validation warnings");
    }

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

    // NEW: registering_skills phase — commit staged skills + register with SkillsRegistry.
    emit("registering_skills", 80, Some("注册 skill 与扫描目录"));
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Atomic promotion of staged skills into the real tree.
    let _final_dir = skill_install::commit_staged_skills(slug, &skills_root)
        .with_context(|| "commit staged skills")?;

    // Record one row per staged skill into automation_installed_skills (V22).
    // Best-effort: the V22 table is diagnostic-only; the runtime behaviour
    // (skill discoverability via SkillsRegistry scan dir) does not depend on
    // these rows. A failed insert must not roll back the already-committed
    // staging dir rename or the automation_specs row.
    {
        let conn = runtime.db.lock().unwrap();
        for s in &staged {
            if let Err(e) = conn.execute(
                "INSERT OR REPLACE INTO automation_installed_skills \
                    (automation_slug, skill_id, installed_at, file_count) \
                    VALUES (?, ?, ?, ?)",
                rusqlite::params![slug, s.skill_id, now_secs, s.file_count],
            ) {
                tracing::error!(
                    slug = %slug,
                    skill_id = %s.skill_id,
                    error = %e,
                    "failed to record installed skill — install continues, AppsTab may show stale state until reinstall"
                );
            }
        }
    }

    // Register the per-automation scan root with SkillsRegistry so the
    // freshly installed skills become discoverable without an app restart.
    if !staged.is_empty() {
        let scan_root = skills_root.join("_marketplace").join(slug);
        let mut reg = skills_registry.write().await;
        reg.add_scan_dir(scan_root, crate::skills::SkillProvenance::Marketplace);
        // Trigger rediscovery so the new skills are active in this process.
        let _ = reg.discover();
    }

    emit("activating", 90, Some("激活订阅"));
    let _ = runtime.activate(&row.id).await;

    emit("complete", 100, Some("已安装"));
    let _ = app_handle.emit("chat:pet-celebrate", serde_json::json!({"slug": slug}));

    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::types::*;

    #[test]
    fn uninstall_removes_rows_and_files() {
        // Set up: temp skills root + in-memory DB with V22 + a fake _marketplace/<slug>/ tree.
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        let target = skills_root.join("_marketplace").join("auto-x").join("skill-a");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), b"# A").unwrap();
        // Untouched user-written skill — must survive uninstall.
        let user_skill = skills_root.join("hand-written").join("SKILL.md");
        std::fs::create_dir_all(user_skill.parent().unwrap()).unwrap();
        std::fs::write(&user_skill, b"# H").unwrap();

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO automation_installed_skills VALUES (?, ?, ?, ?)",
            rusqlite::params!["auto-x", "skill-a", 0_i64, 1_i64],
        ).unwrap();
        // V20 schema requires: id, name, version, author, description, system_prompt,
        // spec_yaml, spec_json, created_at, updated_at (all NOT NULL); others have defaults.
        conn.execute(
            "INSERT INTO automation_specs \
                (id, name, version, author, description, system_prompt, spec_yaml, spec_json, \
                 source, source_ref, created_at, updated_at) \
             VALUES ('auto-x-id', 'X', '1.0.0', 'test', 'desc', '', '', '{}', \
                     'marketplace', 'marketplace://halo/auto-x', 0, 0)",
            [],
        ).expect("insert spec — V20 schema columns");

        // Drive the uninstall — pass in the connection + skills_root directly.
        super::uninstall_human_inner(&conn, &skills_root, "auto-x").unwrap();

        // Assert: rows gone, marketplace dir gone, user skill untouched.
        let spec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM automation_specs WHERE source_ref = 'marketplace://halo/auto-x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(spec_count, 0);
        let inst_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM automation_installed_skills WHERE automation_slug = 'auto-x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(inst_count, 0);
        assert!(!skills_root.join("_marketplace").join("auto-x").exists());
        assert!(user_skill.exists(), "user-written skill must survive");
    }

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
    fn marketplace_item_carries_full_i18n_map() {
        let idx: RegistryIndex = serde_json::from_str(SAMPLE_INDEX).unwrap();
        let item: MarketplaceItem = (&idx.apps[0]).into();
        assert_eq!(
            item.i18n.get("en-US").and_then(|x| x.name.as_deref()),
            Some("AI Daily News Digest")
        );
        assert_eq!(
            item.i18n.get("en-US").and_then(|x| x.description.as_deref()),
            Some("Auto-curates daily AI news.")
        );
    }

    #[test]
    fn marketplace_item_handles_missing_i18n() {
        let idx: RegistryIndex = serde_json::from_str(SAMPLE_INDEX).unwrap();
        let item: MarketplaceItem = (&idx.apps[1]).into();
        assert!(item.i18n.is_empty());
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

    #[test]
    fn capability_validation_collects_missing_ids() {
        // We test the matching logic directly (not via the async install_human
        // which would require a live HTTP server). install_human's loop is a
        // thin wrapper around resolve_capability — proving the wrapper here is
        // sufficient given Task 2 already covers resolve_capability itself.
        use crate::automation::capability_map::resolve_capability;
        let inputs = vec!["ai-browser", "foo", "bar", "ai-browser"];
        let missing: Vec<&str> = inputs
            .iter()
            .copied()
            .filter(|id| resolve_capability(id).is_none())
            .collect();
        assert_eq!(missing, vec!["foo", "bar"]);
    }

    #[test]
    fn list_installed_joins_specs_and_skills_correctly() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        // YAML with two requires.mcps entries: one known (ai-browser) and one unknown.
        let spec_yaml = r#"
spec_version: "1"
name: X
version: 1.0.0
author: t
description: t
type: automation
system_prompt: x
config_schema: []
requires:
  mcps:
    - id: ai-browser
      reason: r
    - id: nonexistent-mcp
      reason: r
"#;

        conn.execute(
            "INSERT INTO automation_specs \
                (id, name, version, author, description, system_prompt, spec_yaml, spec_json, \
                 source, source_ref, created_at, updated_at) \
             VALUES ('a-id', 'X', '1.0.0', 't', 't', 'x', ?1, '{}', \
                     'marketplace', 'marketplace://halo/a', 0, 0)",
            rusqlite::params![spec_yaml],
        ).expect("insert spec — V20 schema columns");

        conn.execute(
            "INSERT INTO automation_installed_skills VALUES (?, ?, ?, ?)",
            rusqlite::params!["a", "skill-1", 0_i64, 2_i64],
        ).unwrap();

        let result = super::list_installed_inner(&conn, std::path::Path::new("/tmp/uclaw-test"))
            .expect("query ok");

        assert_eq!(result.len(), 1);
        let r = &result[0];
        assert_eq!(r.slug, "a");
        assert_eq!(r.name, "X");
        assert_eq!(r.bundled_skills.len(), 1);
        assert_eq!(r.bundled_skills[0].skill_id, "skill-1");
        assert_eq!(r.required_capabilities.len(), 2);
        // ai-browser is a known capability → Mapped
        assert!(matches!(
            r.required_capabilities[0].status,
            super::types::CapabilityStatus::Mapped
        ));
        // nonexistent-mcp is unknown → Missing
        assert!(matches!(
            r.required_capabilities[1].status,
            super::types::CapabilityStatus::Missing
        ));
        // With no marketplace cache row, category defaults to 'other'.
        assert_eq!(r.category, "other");
    }
}
