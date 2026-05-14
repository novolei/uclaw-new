pub mod cache;
pub mod halo_adapter;
mod skill_install;
mod standalone_install;
pub mod types;

pub use cache::category_counts_cached;
pub use types::{
    EntryI18n, InstallOutcome, MarketplaceDetail, MarketplaceInstallProgress, MarketplaceItem,
    MarketplaceQueryResult, MarketplaceUpdate, RegistryEntry, RegistryIndex, RegistrySource,
    StandaloneInstall,
};

use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::automation::runtime::AppRuntimeService;
use crate::skills::SkillsRegistry;

/// List all entries from a registry. Defaults to the DHP registry.
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

/// Replace all `automation_installed_skills` rows for a slug with the given set.
/// DELETE-then-INSERT inside one connection is effectively atomic for our
/// single-writer SQLite. Best-effort: the V22 table is diagnostic-only, so a
/// failure here is logged but never rolls back the already-committed install.
fn write_installed_skill_rows(
    conn: &rusqlite::Connection,
    slug: &str,
    staged: &[(String, i64)], // (skill_id, file_count)
    now_secs: i64,
) {
    if let Err(e) = conn.execute(
        "DELETE FROM automation_installed_skills WHERE automation_slug = ?1",
        rusqlite::params![slug],
    ) {
        tracing::error!(slug = %slug, error = %e, "failed to clear prior installed-skill rows");
    }
    for (skill_id, file_count) in staged {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params![slug, skill_id, now_secs, file_count],
        ) {
            tracing::error!(
                slug = %slug,
                skill_id = %skill_id,
                error = %e,
                "failed to record installed skill — install continues, AppsTab may show stale state until reinstall"
            );
        }
    }
}

/// Best-effort read of a bundled skill's `description` from its SKILL.md
/// YAML frontmatter. Returns None on any problem — a bad SKILL.md must never
/// break the AppsTab list. Deliberately does NOT reuse skills::parse_skill_md,
/// which validates the skill name + enforces activation limits and would
/// reject otherwise-fine marketplace skills.
fn read_skill_description(skill_dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).ok()?;
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let trimmed = content.trim_start_matches(['\n', '\r']);
    let after_open = trimmed.strip_prefix("---")?;
    // Skip to the end of the opening fence line.
    let after_open_line = &after_open[after_open.find('\n')? + 1..];
    // Find the closing fence: a line that is exactly "---".
    let close = after_open_line
        .lines()
        .scan(0usize, |offset, line| {
            let here = *offset;
            *offset += line.len() + 1; // +1 for the '\n'
            Some((here, line))
        })
        .find(|(_, line)| line.trim() == "---")
        .map(|(here, _)| here)?;
    let yaml_str = &after_open_line[..close];

    #[derive(serde::Deserialize)]
    struct Frontmatter {
        description: Option<String>,
    }
    let fm: Frontmatter = serde_yml::from_str(yaml_str).ok()?;
    fm.description.filter(|d| !d.trim().is_empty())
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
            let install_dir = skills_root
                .join("_marketplace")
                .join(&slug)
                .join(&skill_id);
            let description = read_skill_description(&install_dir);
            bundled_skills.push(InstalledSkillBrief {
                skill_id,
                description,
                install_path: install_dir.to_string_lossy().to_string(),
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

/// Build a RegistryEntry from a slug + MarketplaceItem — deduplicates the inline
/// RegistryEntry literals that appear in install_automation and the standalone fns.
fn registry_entry_for(slug: &str, item: &MarketplaceItem) -> RegistryEntry {
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
    }
}

/// Given the MCP ids an automation requires, return the ones uClaw cannot
/// satisfy — i.e. not resolvable via the built-in capability_map AND not
/// present as an installed standalone MCP. (3b-δ replaces capability_map with
/// a configurable table; this function's installed-MCP check is additive.)
fn missing_capabilities(conn: &rusqlite::Connection, mcp_ids: &[String]) -> Vec<String> {
    let installed: std::collections::HashSet<String> = conn
        .prepare("SELECT slug FROM marketplace_standalone_installs WHERE item_type = 'mcp'")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();
    mcp_ids
        .iter()
        .filter(|id| {
            crate::automation::capability_map::resolve_capability(id).is_none()
                && !installed.contains(*id)
        })
        .cloned()
        .collect()
}

#[derive(Debug, PartialEq)]
enum InstallRoute {
    Automation,
    Skill,
    Mcp,
    Unsupported,
}

fn route_install_type(app_type: &str) -> InstallRoute {
    match app_type {
        "automation" => InstallRoute::Automation,
        "skill" => InstallRoute::Skill,
        "mcp" => InstallRoute::Mcp,
        _ => InstallRoute::Unsupported,
    }
}

/// Install dispatcher — resolves the registry item, routes by type.
pub async fn install_marketplace_item(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    mcp_manager: crate::mcp::SharedMcpManager,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    let source = RegistrySource::default();
    let _ = cache::sync_registry(&runtime.db, &source, false).await;
    let item = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found in registry: {}", slug))?
            .0
    };
    match route_install_type(item.app_type.as_str()) {
        InstallRoute::Automation => {
            install_automation(runtime, app_handle, slug, space_id, user_config, skills_registry, progress_channel).await
        }
        InstallRoute::Skill => {
            install_standalone_skill(runtime, app_handle, slug, skills_registry, progress_channel).await
        }
        InstallRoute::Mcp => {
            install_standalone_mcp(runtime, app_handle, slug, user_config, mcp_manager, progress_channel).await
        }
        InstallRoute::Unsupported => Err(anyhow!("marketplace item type '{}' is not installable", item.app_type)),
    }
}

async fn install_standalone_skill(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    use tauri::Emitter;
    let source = RegistrySource::default();
    let emit = |phase: &str, percent: u8, message: Option<&str>| {
        if let Some(ch) = &progress_channel {
            let _ = app_handle.emit(ch, MarketplaceInstallProgress {
                phase: phase.into(), slug: slug.to_string(), percent,
                message: message.map(String::from),
            });
        }
    };

    emit("fetching_spec", 20, Some("拉取 spec.yaml"));
    let (item, cached_yaml, _, _) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found: {}", slug))?
    };
    let yaml = match cached_yaml {
        Some(y) => y,
        None => {
            let entry = registry_entry_for(slug, &item);
            halo_adapter::fetch_spec_yaml(&source, &entry).await?
        }
    };

    emit("parsing", 40, Some("解析 skill spec"));
    let spec: crate::automation::protocol::humane_v1::HumaneAutomationSpec =
        serde_yml::from_str(&yaml).with_context(|| format!("parse spec.yaml for skill {}", slug))?;
    crate::automation::protocol::humane_v1::validate_common(&spec)
        .map_err(|e| anyhow!("invalid skill spec for {}: {}", slug, e))?;

    emit("installing", 70, Some("写入 SKILL.md"));
    let skill_md = standalone_install::render_skill_md(&spec);
    let skills_root = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw").join("skills");
    let install_dir = standalone_install::install_skill_files(slug, &skill_md, &skills_root)?;

    emit("registering_skills", 85, Some("注册 skill 扫描目录"));
    {
        let standalone_root = skills_root.join("_marketplace").join("_standalone");
        let mut reg = skills_registry.write().await;
        reg.add_scan_dir(standalone_root, crate::skills::SkillProvenance::Marketplace);
        let _ = reg.discover();
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    {
        let conn = runtime.db.lock().unwrap();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,NULL)",
            rusqlite::params![slug, "skill", item.version, now_secs],
        ) {
            tracing::error!(slug = %slug, error = %e, "failed to record standalone skill install");
        }
    }

    emit("complete", 100, Some("完成"));
    Ok(InstallOutcome::Skill { slug: slug.to_string(), install_path: install_dir.to_string_lossy().to_string() })
}

async fn install_standalone_mcp(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    user_config: Option<serde_json::Value>,
    mcp_manager: crate::mcp::SharedMcpManager,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    use tauri::Emitter;
    let source = RegistrySource::default();
    let emit = |phase: &str, percent: u8, message: Option<&str>| {
        if let Some(ch) = &progress_channel {
            let _ = app_handle.emit(ch, MarketplaceInstallProgress {
                phase: phase.into(), slug: slug.to_string(), percent,
                message: message.map(String::from),
            });
        }
    };

    emit("fetching_spec", 20, Some("拉取 spec.yaml"));
    let (item, cached_yaml, _, _) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found: {}", slug))?
    };
    let yaml = match cached_yaml {
        Some(y) => y,
        None => {
            let entry = registry_entry_for(slug, &item);
            halo_adapter::fetch_spec_yaml(&source, &entry).await?
        }
    };

    emit("parsing", 50, Some("解析 mcp spec"));
    let spec: crate::automation::protocol::humane_v1::HumaneAutomationSpec =
        serde_yml::from_str(&yaml).with_context(|| format!("parse spec.yaml for mcp {}", slug))?;
    crate::automation::protocol::humane_v1::validate_common(&spec)
        .map_err(|e| anyhow!("invalid mcp spec for {}: {}", slug, e))?;
    let block = spec.mcp_server.clone()
        .ok_or_else(|| anyhow!("mcp spec {} missing mcp_server block", slug))?;

    emit("installing", 75, Some("注册 MCP server"));
    let cfg = standalone_install::build_mcp_config(
        slug, &spec, &block, &user_config.unwrap_or(serde_json::Value::Null),
    );
    let mcp_server_id = cfg.id.clone();
    {
        let mut mgr = mcp_manager.write().await;
        mgr.add_server(cfg).map_err(|e| anyhow!("MCP manager add_server failed: {}", e))?;
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    {
        let conn = runtime.db.lock().unwrap();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,?)",
            rusqlite::params![slug, "mcp", item.version, now_secs, mcp_server_id],
        ) {
            tracing::error!(slug = %slug, error = %e, "failed to record standalone mcp install");
        }
    }

    emit("complete", 100, Some("完成"));
    Ok(InstallOutcome::Mcp { slug: slug.to_string(), mcp_server_id })
}

/// Install a single automation registry entry. Returns InstallOutcome::Automation.
/// source_ref takes the form `marketplace://halo/{slug}` per spec § 5 URI convention.
pub async fn install_automation(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
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
        let entry = registry_entry_for(slug, &item);
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
    let staged = match skill_install::fetch_bundled_skills(&source, &registry_entry_for(slug, &item), &parsed.spec, &skills_root).await {
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

    let missing_caps: Vec<String> = {
        let conn = runtime.db.lock().unwrap();
        missing_capabilities(&conn, &mcp_ids)
    };
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

    {
        let conn = runtime.db.lock().unwrap();
        let rows: Vec<(String, i64)> = staged
            .iter()
            .map(|s| (s.skill_id.clone(), s.file_count))
            .collect();
        write_installed_skill_rows(&conn, slug, &rows, now_secs);
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

    Ok(InstallOutcome::Automation { spec: row })
}

/// Query all rows from `marketplace_standalone_installs`, newest first.
pub fn list_standalone_inner(conn: &rusqlite::Connection) -> Result<Vec<types::StandaloneInstall>> {
    let mut stmt = conn.prepare(
        "SELECT slug, item_type, version, installed_at, mcp_server_id \
            FROM marketplace_standalone_installs ORDER BY installed_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(types::StandaloneInstall {
            slug: r.get(0)?,
            item_type: r.get(1)?,
            version: r.get(2)?,
            installed_at: r.get(3)?,
            mcp_server_id: r.get(4)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Sync core of standalone-skill uninstall — testable without runtime handles.
pub fn uninstall_standalone_skill_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
    slug: &str,
) -> Result<()> {
    let dir = skills_root.join("_marketplace").join("_standalone").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
    }
    conn.execute(
        "DELETE FROM marketplace_standalone_installs WHERE slug = ?1",
        rusqlite::params![slug],
    )?;
    Ok(())
}

/// Uninstall dispatcher — routes by standalone type, falls through to automation uninstall.
pub async fn uninstall_marketplace_item(
    runtime: &AppRuntimeService,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    mcp_manager: crate::mcp::SharedMcpManager,
    slug: &str,
) -> Result<()> {
    use rusqlite::OptionalExtension;
    // Look up the slug in marketplace_standalone_installs.
    let standalone: Option<(String, Option<String>)> = {
        let conn = runtime.db.lock().unwrap();
        conn.query_row(
            "SELECT item_type, mcp_server_id FROM marketplace_standalone_installs WHERE slug = ?1",
            rusqlite::params![slug],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        ).optional()?
    };
    match standalone {
        Some((ref item_type, _)) if item_type == "skill" => {
            let skills_root = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?
                .join(".uclaw").join("skills");
            {
                let conn = runtime.db.lock().unwrap();
                uninstall_standalone_skill_inner(&conn, &skills_root, slug)?;
            }
            let mut reg = skills_registry.write().await;
            let _ = reg.discover();
            Ok(())
        }
        Some((ref item_type, ref mcp_server_id)) if item_type == "mcp" => {
            if let Some(id) = mcp_server_id {
                let mut mgr = mcp_manager.write().await;
                let _ = mgr.remove_server(id); // best-effort: server may already be gone
            }
            let conn = runtime.db.lock().unwrap();
            conn.execute(
                "DELETE FROM marketplace_standalone_installs WHERE slug = ?1",
                rusqlite::params![slug],
            )?;
            Ok(())
        }
        Some((item_type, _)) => Err(anyhow!("unknown standalone item_type '{}'", item_type)),
        None => {
            // Not a standalone install — fall through to the automation uninstall path.
            uninstall_human(runtime, skills_registry, slug).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::types::*;

    #[test]
    fn route_install_type_classifies_all_cases() {
        assert_eq!(super::route_install_type("automation"), super::InstallRoute::Automation);
        assert_eq!(super::route_install_type("skill"), super::InstallRoute::Skill);
        assert_eq!(super::route_install_type("mcp"), super::InstallRoute::Mcp);
        assert_eq!(super::route_install_type("extension"), super::InstallRoute::Unsupported);
        assert_eq!(super::route_install_type("garbage"), super::InstallRoute::Unsupported);
    }

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
    fn registering_skills_clears_stale_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        // Simulate a prior install with skills {A, B}.
        conn.execute(
            "INSERT INTO automation_installed_skills VALUES ('auto-x', 'skill-a', 0, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_installed_skills VALUES ('auto-x', 'skill-b', 0, 1)",
            [],
        ).unwrap();

        // Simulate an upgrade whose staged set is only {A}: the registering_skills
        // logic must DELETE all prior rows for the slug, then re-insert just {A}.
        super::write_installed_skill_rows(&conn, "auto-x", &[("skill-a".to_string(), 2_i64)], 1715000000);

        let rows: Vec<(String, i64)> = conn
            .prepare("SELECT skill_id, file_count FROM automation_installed_skills WHERE automation_slug = 'auto-x' ORDER BY skill_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(rows, vec![("skill-a".to_string(), 2_i64)], "stale skill-b row must be gone, skill-a refreshed");
    }

    #[test]
    fn read_skill_description_parses_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skill-a");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // Happy path: frontmatter with a description.
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: skill-a\ndescription: Collects search data\n---\n\nBody.\n",
        ).unwrap();
        assert_eq!(
            super::read_skill_description(&skill_dir),
            Some("Collects search data".to_string()),
        );

        // Frontmatter without a description field → None.
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: skill-a\n---\n\nBody.\n",
        ).unwrap();
        assert_eq!(super::read_skill_description(&skill_dir), None);

        // Malformed frontmatter (no closing fence) → None, never panics.
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: skill-a\nBody with no close").unwrap();
        assert_eq!(super::read_skill_description(&skill_dir), None);

        // Missing SKILL.md entirely → None.
        let empty_dir = tmp.path().join("skill-empty");
        std::fs::create_dir_all(&empty_dir).unwrap();
        assert_eq!(super::read_skill_description(&empty_dir), None);
    }

    #[test]
    fn capability_check_recognises_installed_mcp() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES ('postgres-mcp', 'mcp', '1.0.0', 0, 'srv-1')",
            [],
        ).unwrap();

        // ai-browser resolves via capability_map; postgres-mcp via installed table;
        // unknown-mcp resolves nowhere → reported missing.
        let missing = super::missing_capabilities(
            &conn,
            &["ai-browser".to_string(), "postgres-mcp".to_string(), "unknown-mcp".to_string()],
        );
        assert_eq!(missing, vec!["unknown-mcp".to_string()]);
    }

    #[test]
    fn list_standalone_inner_returns_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO marketplace_standalone_installs VALUES ('s1','skill','1.0.0',100,NULL)", [],
        ).unwrap();
        conn.execute(
            "INSERT INTO marketplace_standalone_installs VALUES ('m1','mcp','2.0.0',200,'srv-9')", [],
        ).unwrap();
        let list = super::list_standalone_inner(&conn).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].slug, "m1"); // ordered by installed_at DESC
        assert_eq!(list[0].mcp_server_id.as_deref(), Some("srv-9"));
        assert_eq!(list[1].slug, "s1");
        assert_eq!(list[1].mcp_server_id, None);
    }

    #[test]
    fn uninstall_standalone_skill_removes_files_and_row() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        let dir = skills_root.join("_marketplace").join("_standalone").join("s1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), b"---\nname: s1\n---\n").unwrap();

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO marketplace_standalone_installs VALUES ('s1','skill','1.0.0',0,NULL)", [],
        ).unwrap();

        super::uninstall_standalone_skill_inner(&conn, &skills_root, "s1").unwrap();

        assert!(!dir.exists(), "skill dir removed");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM marketplace_standalone_installs WHERE slug='s1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(n, 0, "V25 row removed");
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
