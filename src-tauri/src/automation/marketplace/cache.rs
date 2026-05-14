//! Marketplace cache layer — V23a-backed SQLite store + FTS5 search.
//!
//! Replaces Phase 1's "hit the network on every list" model. Sync is
//! lazy + ETag-aware: a query that finds stale sync state triggers a
//! refresh in-band; explicit `refresh_marketplace` forces it.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::collections::HashSet;

use super::halo_adapter;
use super::types::{EntryI18n, MarketplaceItem, MarketplaceQueryResult, RegistryEntry, RegistrySource};

/// Cache TTL — 1 hour. Within TTL, queries hit SQLite only; past it,
/// queries trigger a sync first (if force=false the sync is best-effort
/// and falls back to stale data on failure).
pub const CACHE_TTL_MS: i64 = 60 * 60 * 1000;

/// Default page size if none specified.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Maximum page size to prevent memory blow-up on pathological queries.
pub const MAX_PAGE_SIZE: u32 = 200;

/// Sync a single registry into the local cache. Idempotent.
///
/// - If `force` is false and `last_synced_at < TTL` ago → no-op (returns Ok
///   with current `item_count`).
/// - Otherwise → fetch index.json via halo_adapter (which handles Gitee
///   fallback), upsert all entries, prune removed slugs, update sync state.
/// - On network failure with cached data present → returns Ok with stale data
///   (logs warn). Only first-sync failures return Err.
pub async fn sync_registry(
    conn: &std::sync::Mutex<Connection>,
    source: &RegistrySource,
    force: bool,
) -> Result<u32> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    // 1. Read existing sync state
    let existing = {
        let c = conn.lock().unwrap();
        c.query_row(
            "SELECT last_synced_at, last_etag, last_modified, item_count FROM automation_registry_sync WHERE registry_id = ?1",
            params![source.id],
            |r| Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            )),
        ).ok()
    };

    if !force {
        if let Some((Some(last_synced), _, _, item_count)) = &existing {
            if now_ms - last_synced < CACHE_TTL_MS {
                return Ok(*item_count as u32);
            }
        }
    }

    // 2. Fetch index via the existing halo_adapter (Gitee fallback)
    let index = match halo_adapter::fetch_index(source).await {
        Ok(i) => i,
        Err(e) => {
            // Best-effort fallback: if we have ANY cached rows, surface stale data
            if let Some((_, _, _, item_count)) = &existing {
                if *item_count > 0 {
                    tracing::warn!(error = %e, "marketplace sync failed — serving stale cache");
                    let c = conn.lock().unwrap();
                    let _ = c.execute(
                        "UPDATE automation_registry_sync SET last_error = ?1 WHERE registry_id = ?2",
                        params![e.to_string(), source.id],
                    );
                    return Ok(*item_count as u32);
                }
            }
            return Err(e);
        }
    };

    // 3. Upsert all entries in a transaction. Prune slugs no longer present.
    let mut c = conn.lock().unwrap();
    let tx = c.transaction()?;

    let new_slugs: HashSet<&str> = index.apps.iter().map(|e| e.slug.as_str()).collect();

    for entry in &index.apps {
        upsert_entry(&tx, &source.id, entry, now_ms)?;
    }

    // Find removed slugs (in DB but not in new index)
    let mut to_delete: Vec<String> = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT slug FROM automation_marketplace_items WHERE registry_id = ?1",
        )?;
        let rows = stmt.query_map(params![source.id], |r| r.get::<_, String>(0))?;
        for row in rows {
            let slug = row?;
            if !new_slugs.contains(slug.as_str()) {
                to_delete.push(slug);
            }
        }
    }
    for slug in &to_delete {
        tx.execute(
            "DELETE FROM automation_marketplace_items WHERE registry_id = ?1 AND slug = ?2",
            params![source.id, slug],
        )?;
        tx.execute(
            "DELETE FROM automation_marketplace_fts WHERE registry_id = ?1 AND slug = ?2",
            params![source.id, slug],
        )?;
    }

    // 4. Update sync state
    tx.execute(
        "INSERT OR REPLACE INTO automation_registry_sync (registry_id, last_synced_at, last_etag, last_modified, last_error, item_count) VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
        params![
            source.id,
            now_ms,
            None::<String>, // ETag not yet captured — halo_adapter doesn't surface response headers. Phase 3b adds this.
            None::<String>,
            index.apps.len() as i64,
        ],
    )?;

    tx.commit()?;
    Ok(index.apps.len() as u32)
}

fn upsert_entry(
    tx: &rusqlite::Transaction,
    registry_id: &str,
    entry: &RegistryEntry,
    cached_at_ms: i64,
) -> rusqlite::Result<()> {
    let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".into());
    let requires_json = serde_json::to_string(&serde_json::json!({
        "mcps": entry.requires_mcps,
        "skills": entry.requires_skills,
    }))
    .unwrap_or_else(|_| "{}".into());
    let i18n_json = serde_json::to_string(&entry.i18n).unwrap_or_else(|_| "{}".into());

    tx.execute(
        "INSERT OR REPLACE INTO automation_marketplace_items
         (registry_id, slug, name, version, author, description, item_type,
          category, icon, tags_json, locale, min_app_version, size_bytes,
          checksum, requires_json, i18n_json, spec_yaml, updated_at_index, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL, ?17, ?18)",
        params![
            registry_id,
            entry.slug,
            entry.name,
            entry.version,
            entry.author,
            entry.description,
            entry.app_type,
            entry.category,
            entry.icon,
            tags_json,
            entry.locale,
            entry.min_app_version,
            entry.size_bytes.map(|n| n as i64),
            entry.checksum,
            requires_json,
            i18n_json,
            entry.updated_at,
            cached_at_ms,
        ],
    )?;

    // Refresh FTS row
    tx.execute(
        "DELETE FROM automation_marketplace_fts WHERE registry_id = ?1 AND slug = ?2",
        params![registry_id, entry.slug],
    )?;
    tx.execute(
        "INSERT INTO automation_marketplace_fts (slug, registry_id, name, description, author, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.slug,
            registry_id,
            entry.name,
            entry.description,
            entry.author,
            entry.tags.join(" "),
        ],
    )?;

    Ok(())
}

/// Paged query over the cache.
pub fn query_items(
    conn: &Connection,
    search: Option<&str>,
    item_type: Option<&str>,
    category: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<MarketplaceQueryResult> {
    let page_size = page_size.clamp(1, MAX_PAGE_SIZE);
    let offset = page * page_size;

    let mut where_clauses: Vec<String> = Vec::new();
    let mut bind_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = item_type {
        where_clauses.push("i.item_type = ?".into());
        bind_params.push(Box::new(t.to_string()));
    }
    if let Some(c) = category {
        where_clauses.push("i.category = ?".into());
        bind_params.push(Box::new(c.to_string()));
    }

    // SQLite FTS5's MATCH operator requires the bare virtual-table name —
    // it does NOT accept aliases. Phase 3a's earlier "JOIN ... fts ON" with
    // "fts MATCH ?" threw `no such column: fts` at runtime. Use the full
    // table name in both the JOIN columns and the MATCH clause.
    let (fts_join, fts_where, fts_order) = match search {
        Some(q) if !q.trim().is_empty() => {
            bind_params.push(Box::new(q.trim().to_string()));
            (
                "JOIN automation_marketplace_fts \
                   ON automation_marketplace_fts.slug = i.slug \
                  AND automation_marketplace_fts.registry_id = i.registry_id",
                Some("automation_marketplace_fts MATCH ?"),
                "automation_marketplace_fts.rank",
            )
        }
        _ => ("", None, "i.updated_at_index DESC, i.name ASC"),
    };
    if let Some(w) = fts_where {
        where_clauses.push(w.into());
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Total count
    let count_sql = format!(
        "SELECT COUNT(*) FROM automation_marketplace_items i {} {}",
        fts_join, where_sql
    );
    let total: i64 = conn.query_row(
        &count_sql,
        rusqlite::params_from_iter(bind_params.iter().map(|b| b.as_ref())),
        |r| r.get(0),
    )?;

    // Items page — bind page_size+1 to detect hasMore + offset
    let items_sql = format!(
        "SELECT i.slug, i.name, i.version, i.author, i.description, i.item_type,
                i.category, i.icon, i.tags_json, i.size_bytes, i.min_app_version,
                i.locale, i.i18n_json
         FROM automation_marketplace_items i {} {}
         ORDER BY {} LIMIT ? OFFSET ?",
        fts_join, where_sql, fts_order
    );

    bind_params.push(Box::new((page_size + 1) as i64));
    bind_params.push(Box::new(offset as i64));

    let mut stmt = conn.prepare(&items_sql)?;
    let raw_rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        String,
    )> = stmt
        .query_map(
            rusqlite::params_from_iter(bind_params.iter().map(|b| b.as_ref())),
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let has_more = raw_rows.len() > page_size as usize;
    let rows: Vec<MarketplaceItem> = raw_rows
        .into_iter()
        .take(page_size as usize)
        .map(|r| {
            let tags: Vec<String> = serde_json::from_str(&r.8).unwrap_or_default();
            let i18n_map: std::collections::HashMap<String, EntryI18n> =
                serde_json::from_str(&r.12).unwrap_or_default();
            MarketplaceItem {
                slug: r.0,
                name: r.1,
                version: r.2,
                author: r.3,
                description: r.4,
                app_type: r.5,
                category: r.6,
                icon: r.7,
                tags,
                size_bytes: r.9.map(|n| n as u64),
                min_app_version: r.10,
                locale: r.11,
                i18n: i18n_map,
            }
        })
        .collect();

    Ok(MarketplaceQueryResult {
        items: rows,
        total: total as u32,
        has_more,
    })
}

/// Return category → count map for the entire cached registry, optionally
/// filtered to a single item_type. Used by StoreHeader to show counts
/// alongside category chips.
pub fn category_counts(
    conn: &Connection,
    item_type: Option<&str>,
) -> Result<std::collections::HashMap<String, u32>> {
    let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = if let Some(t) = item_type {
        (
            "SELECT category, COUNT(*) FROM automation_marketplace_items WHERE item_type = ?1 GROUP BY category",
            vec![Box::new(t.to_string())],
        )
    } else {
        (
            "SELECT category, COUNT(*) FROM automation_marketplace_items GROUP BY category",
            vec![],
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)),
    )?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (cat, cnt) = row?;
        out.insert(cat, cnt);
    }
    Ok(out)
}

/// Fetch a single item's full detail row, including cached spec_yaml.
/// If spec_yaml is None, caller should fetch it via halo_adapter and cache it.
pub fn get_item_with_spec(
    conn: &Connection,
    registry_id: &str,
    slug: &str,
) -> Result<Option<(MarketplaceItem, Option<String>, String, String)>> {
    let row = conn
        .query_row(
            "SELECT slug, name, version, author, description, item_type, category, icon,
                tags_json, size_bytes, min_app_version, locale, i18n_json,
                spec_yaml, requires_json
         FROM automation_marketplace_items
         WHERE registry_id = ?1 AND slug = ?2",
            params![registry_id, slug],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, Option<i64>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, Option<String>>(13)?,
                    r.get::<_, String>(14)?,
                ))
            },
        )
        .ok();

    let Some((
        slug,
        name,
        version,
        author,
        description,
        app_type,
        category,
        icon,
        tags_json,
        size_bytes,
        min_app_version,
        locale,
        i18n_json,
        spec_yaml,
        requires_json,
    )) = row
    else {
        return Ok(None);
    };

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let i18n_map: std::collections::HashMap<String, EntryI18n> =
        serde_json::from_str(&i18n_json).unwrap_or_default();

    let item = MarketplaceItem {
        slug,
        name,
        version,
        author,
        description,
        app_type,
        category,
        icon,
        tags,
        size_bytes: size_bytes.map(|n| n as u64),
        min_app_version,
        locale,
        i18n: i18n_map,
    };

    Ok(Some((item, spec_yaml, requires_json, i18n_json)))
}

/// Cache the fetched spec.yaml for a slug. Called lazily on first detail view.
pub fn cache_spec_yaml(
    conn: &Connection,
    registry_id: &str,
    slug: &str,
    yaml: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE automation_marketplace_items SET spec_yaml = ?1 WHERE registry_id = ?2 AND slug = ?3",
        params![yaml, registry_id, slug],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn query_items_empty_returns_zero_total() {
        let conn = setup();
        let result = query_items(&conn, None, None, None, 0, 20).unwrap();
        assert_eq!(result.total, 0);
        assert!(result.items.is_empty());
        assert!(!result.has_more);
    }

    #[test]
    fn category_counts_returns_grouped_counts() {
        let conn = setup();
        conn.execute(
            "INSERT INTO automation_marketplace_items (registry_id, slug, name, version, author, description, item_type, category, cached_at) VALUES ('halo', 's1', 'A', '1.0.0', 'x', 'd', 'automation', 'social', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_marketplace_items (registry_id, slug, name, version, author, description, item_type, category, cached_at) VALUES ('halo', 's2', 'B', '1.0.0', 'x', 'd', 'automation', 'productivity', 1)",
            [],
        ).unwrap();
        let counts = category_counts(&conn, Some("automation")).unwrap();
        assert_eq!(counts.get("social"), Some(&1));
        assert_eq!(counts.get("productivity"), Some(&1));
    }
}
