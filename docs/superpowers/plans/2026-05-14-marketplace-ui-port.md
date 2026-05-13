# Marketplace UI Port — Phase 3a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port hello-halo's marketplace UI to uClaw with uClaw design DNA and 8 native innovations (Install Wizard, Sandbox try, Featured row, smart filter counts, Pet awareness, sticky CTA, detail sub-tabs, WelcomeView empty states). Single registry (DHP via Gitee fallback). Multi-registry / proxy adapters stay in Phase 3b.

**Architecture:**
- Backend gains a SQLite cache layer (`automation_marketplace_items` + FTS5 + sync state) so browsing is fast and offline-safe.
- Existing `list_marketplace_humans` / `install_marketplace_human` Tauri commands get upgraded shapes (paged query + scope + config + progress event).
- Frontend gains 6 new components inside `ui/src/components/automation/` + 1 new atom slice. The current `MarketplaceModal` + `MarketplaceCard` get retired. AutomationHub becomes a 3-sub-view container (我的数字人 / 我的应用 / 应用商店) instead of a single panel.

**Tech Stack:**
- Rust + rusqlite + reqwest + Tauri 2
- React 18 + TypeScript + Jotai + Tailwind (theme tokens) + motion/react + lucide-react
- Vitest + jsdom for FE tests; cargo test for BE tests

**Design refs:**
- Spec: `docs/superpowers/specs/2026-05-14-marketplace-ui-port-design.md` (especially § 13 uClaw Design DNA)
- Phase 1 baseline: `docs/superpowers/specs/2026-05-13-humane-automation-design.md`
- hello-halo reference impl: `/Users/ryanliu/Documents/hello-halo/src/{renderer,main}/store/`

**Constraints:**
- All UI must use theme tokens (`bg-content-area`, `text-foreground`, `border-border/50`, `text-success`/`-bg`, etc.) — NO hardcoded `bg-zinc-X` / `text-gray-X` / `text-green-500`
- Card radius `rounded-xl`, main panel `rounded-2xl`, never `rounded-lg` on cards
- Motion uses `motion/react` (Framer), `duration: 0.22`, ease `[0.32, 0.72, 0, 1]` for state transitions
- Bisectable commits — each task = one commit, working build + green tests

---

## File map

### Backend (Rust)

| File | Status | Responsibility |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | modify | Add V23a constant + `run_v23a()` + register in `run()` |
| `src-tauri/src/automation/marketplace/cache.rs` | **create** | SQLite sync (ETag-based) + query (FTS5 + filter) |
| `src-tauri/src/automation/marketplace/types.rs` | modify | Add `MarketplaceQueryResult`, `MarketplaceDetail`, `MarketplaceUpdate`, `MarketplaceInstallProgress` |
| `src-tauri/src/automation/marketplace/mod.rs` | modify | Re-export new types; replace `list_humans()` body with cache-backed query; add `install_human` upgrades |
| `src-tauri/src/tauri_commands.rs` | modify | Add 3 new commands (`query_marketplace`, `get_marketplace_detail`, `check_marketplace_updates`); upgrade `install_marketplace_human` shape |
| `src-tauri/src/main.rs` | modify | Register new commands in `invoke_handler!` |
| `src-tauri/src/automation/runtime/service.rs` | modify | Add `try_install_to_sandbox()` for sandbox feature |
| `src-tauri/src/automation/runtime/sandbox.rs` | **create** | Ephemeral sandbox workspace lifecycle (create / cleanup) |

### Frontend (TypeScript/React)

| File | Status | Responsibility |
|---|---|---|
| `ui/src/atoms/marketplace.ts` | modify | Add new atoms: filters, items, hasMore, detail, sandboxState, availableUpdates, installProgress |
| `ui/src/lib/tauri-bridge.ts` | modify | 3 new typed wrappers + types |
| `ui/src/components/automation/AppTypeBadge.tsx` | **create** | 4-color type pill with hover tooltip |
| `ui/src/components/automation/StoreHeader.tsx` | **create** | Search input + type tabs + category chips |
| `ui/src/components/automation/StoreCard.tsx` | **create** | Grid card (replaces MarketplaceCard) |
| `ui/src/components/automation/StoreGrid.tsx` | **create** | Card grid with Load More + empty state |
| `ui/src/components/automation/StoreFeaturedRow.tsx` | **create** | Horizontal-scroll hero row |
| `ui/src/components/automation/StoreDetail.tsx` | **create** | Full detail view with 4 sub-tabs |
| `ui/src/components/automation/InstallWizard.tsx` | **create** | 3-step install flow (scope → config → confirm) |
| `ui/src/components/automation/StoreView.tsx` | **create** | Top-level marketplace container (header + featured + grid OR detail) |
| `ui/src/components/automation/AutomationHub.tsx` | modify | Becomes the "我的数字人" sub-view body |
| `ui/src/components/automation/AppsTab.tsx` | **create** | Phase 3a stub for "我的应用" tab (empty state + explainer) |
| `ui/src/views/AutomationsView.tsx` | **create** | New view replacing direct AutomationHub render in MainArea; owns the 3-sub-view nav |
| `ui/src/components/tabs/MainArea.tsx` | modify | Render `AutomationsView` instead of `AutomationHub` when atom is open |
| `ui/src/components/automation/MarketplaceModal.tsx` | **delete** | Replaced by StoreView |
| `ui/src/components/automation/MarketplaceCard.tsx` | **delete** | Replaced by StoreCard |
| `ui/src/components/automation/MarketplaceModal.test.tsx` | **delete** | Coverage moves to StoreView.test.tsx |
| `ui/src/components/automation/MarketplaceCard.test.tsx` | **delete** | Coverage moves to StoreCard.test.tsx |
| `ui/src/components/agent/PetWidget.tsx` | modify | Listen for `chat:pet-celebrate` event → run celebration frame |

---

## Task 1: V23a migration — marketplace cache schema

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

**Spec reference:** § 4 (Database schema V23 partial)

- [ ] **Step 1: Add the V23A_MARKETPLACE_CACHE constant**

Add near the V21/V22 constants (top of file, around line 1028 after `SQL_V21`):

```rust
const V23A_MARKETPLACE_CACHE: &str = "
CREATE TABLE IF NOT EXISTS automation_marketplace_items (
    registry_id      TEXT NOT NULL,
    slug             TEXT NOT NULL,
    name             TEXT NOT NULL,
    version          TEXT NOT NULL,
    author           TEXT NOT NULL,
    description      TEXT NOT NULL,
    item_type        TEXT NOT NULL,
    category         TEXT NOT NULL DEFAULT 'other',
    icon             TEXT,
    tags_json        TEXT NOT NULL DEFAULT '[]',
    locale           TEXT,
    min_app_version  TEXT,
    size_bytes       INTEGER,
    checksum         TEXT,
    requires_json    TEXT NOT NULL DEFAULT '{}',
    i18n_json        TEXT NOT NULL DEFAULT '{}',
    spec_yaml        TEXT,
    updated_at_index TEXT,
    cached_at        INTEGER NOT NULL,
    PRIMARY KEY (registry_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_marketplace_type     ON automation_marketplace_items(item_type);
CREATE INDEX IF NOT EXISTS idx_marketplace_category ON automation_marketplace_items(category);

CREATE VIRTUAL TABLE IF NOT EXISTS automation_marketplace_fts USING fts5(
    slug UNINDEXED,
    registry_id UNINDEXED,
    name,
    description,
    author,
    tags,
    tokenize = 'trigram'
);

CREATE TABLE IF NOT EXISTS automation_registry_sync (
    registry_id    TEXT PRIMARY KEY,
    last_synced_at INTEGER,
    last_etag      TEXT,
    last_modified  TEXT,
    last_error     TEXT,
    item_count     INTEGER NOT NULL DEFAULT 0
);
";

pub fn run_v23a(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(V23A_MARKETPLACE_CACHE)
}
```

- [ ] **Step 2: Wire V23a into the migration chain**

Find `run()` in the same file (around line 1213 where V21 is run). Add:

```rust
    // V23a: Marketplace cache (Phase 3a). Phase 3b extends to add the
    // automation_registries table for multi-source support.
    tracing::info!("Running migration V23a: marketplace cache (items + FTS + sync state)");
    if let Err(e) = run_v23a(conn) {
        tracing::error!(error = %e, "V23a FAILED — marketplace cache unavailable");
        return Err(e);
    }
    tracing::info!("Database migrations complete");
    Ok(())
```

Right BEFORE the existing `Ok(())`. Remove the old `tracing::info!("Database migrations complete")` line that was there before since we moved it past V23a.

- [ ] **Step 3: Add a smoke test for V23a**

In the `#[cfg(test)] mod tests` section at the bottom of `migrations.rs` (around line 1218), add:

```rust
    #[test]
    fn v23a_creates_marketplace_cache_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Tables exist
        for tbl in ["automation_marketplace_items", "automation_marketplace_fts", "automation_registry_sync"] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM sqlite_master WHERE type IN ('table','virtual table') AND name = '{}'", tbl),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            assert!(count >= 1, "{} should exist after V23a", tbl);
        }
        // FTS5 trigram tokenizer works
        conn.execute("INSERT INTO automation_marketplace_fts(slug, registry_id, name, description, author, tags) VALUES('s', 'halo', 'AI News', 'curates news', 'a', 'ai,news')", []).unwrap();
        let hits: i64 = conn.query_row(
            "SELECT count(*) FROM automation_marketplace_fts WHERE automation_marketplace_fts MATCH 'news'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(hits, 1, "FTS5 trigram should match");
    }
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib db::migrations 2>&1 | tail -5
```

Expected: 15 tests pass (was 14 — added one).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V23a — marketplace cache (items + FTS5 trigram + sync state)

Phase 3a backbone. Three tables:

  automation_marketplace_items     # cached registry entries (PK: registry_id, slug)
  automation_marketplace_fts       # FTS5 trigram for search (matches Phase 1's messages_fts)
  automation_registry_sync         # ETag/Last-Modified per registry for incremental sync

Single registry_id in Phase 3a ('halo'). Phase 3b adds automation_registries
to formalise multi-source. The trigram tokenizer mirrors Phase 1 choice
for mixed Chinese/English search.

V23 number was reserved in Phase 1's spec for marketplace — claiming the
'a' half now; Phase 3b takes 'b' for automation_registries."
```

---

## Task 2: Marketplace cache module — sync + query

**Files:**
- Create: `src-tauri/src/automation/marketplace/cache.rs`

**Spec reference:** § 5.2 (Sync logic)

This task is the workhorse. ~300 LOC.

- [ ] **Step 1: Create the module skeleton with constants and types**

Create `src-tauri/src/automation/marketplace/cache.rs`:

```rust
//! Marketplace cache layer — V23a-backed SQLite store + FTS5 search.
//!
//! Replaces Phase 1's "hit the network on every list" model. Sync is
//! lazy + ETag-aware: a query that finds stale sync state triggers a
//! refresh in-band; explicit `refresh_marketplace` forces it.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::collections::HashSet;

use super::halo_adapter;
use super::types::{MarketplaceItem, MarketplaceQueryResult, RegistryEntry, RegistrySource};

/// Cache TTL — 1 hour. Within TTL, queries hit SQLite only; past it,
/// queries trigger a sync first (if force=false the sync is best-effort
/// and falls back to stale data on failure).
pub const CACHE_TTL_MS: i64 = 60 * 60 * 1000;

/// Default page size if none specified.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Maximum page size to prevent memory blow-up on pathological queries.
pub const MAX_PAGE_SIZE: u32 = 200;
```

- [ ] **Step 2: Add the sync function (one big block since it's interconnected)**

Append to `cache.rs`:

```rust
/// Sync a single registry into the local cache. Idempotent.
///
/// - If `force` is false and `last_synced_at < TTL` ago → no-op (returns Ok with
///   current `item_count`).
/// - Otherwise → fetch index.json with If-None-Match / If-Modified-Since headers,
///   upsert all entries, prune removed slugs, update sync state.
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

    // 2. Fetch index via the existing halo_adapter (which already handles Gitee fallback)
    let index = match halo_adapter::fetch_index(source).await {
        Ok(i) => i,
        Err(e) => {
            // Best-effort fallback: if we have ANY cached rows, surface stale data
            if let Some((_, _, _, item_count)) = &existing {
                if *item_count > 0 {
                    tracing::warn!(error = %e, "marketplace sync failed — serving stale cache");
                    let mut c = conn.lock().unwrap();
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
            None::<String>, // ETag not yet captured — reqwest in halo_adapter doesn't surface response headers. Phase 3b adds this.
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
```

- [ ] **Step 3: Add the query function**

Append to `cache.rs`:

```rust
/// Paged query over the cache.
///
/// - `search` (optional): FTS5 trigram query over name + description + author + tags
/// - `item_type` (optional): exact match, e.g. "automation", "skill", "mcp"
/// - `category` (optional): exact match
/// - `page` (0-indexed) + `page_size` (clamped to MAX_PAGE_SIZE)
///
/// Returns up to `page_size + 1` rows to know if `has_more` is true, then
/// trims back to `page_size`.
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

    // Build the WHERE clause + params dynamically. FTS5 join when search is present.
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

    let (fts_join, fts_where, fts_order) = match search {
        Some(q) if !q.trim().is_empty() => {
            bind_params.push(Box::new(q.trim().to_string()));
            (
                "JOIN automation_marketplace_fts fts ON fts.slug = i.slug AND fts.registry_id = i.registry_id",
                Some("fts MATCH ?"),
                "fts.rank",
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

    // Total count (separate query — small, runs against the same WHERE)
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
    let raw_rows: Vec<(String, String, String, String, String, String, String, Option<String>, String, Option<i64>, Option<String>, Option<String>, String)> = stmt
        .query_map(
            rusqlite::params_from_iter(bind_params.iter().map(|b| b.as_ref())),
            |r| Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?,
            )),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let has_more = raw_rows.len() > page_size as usize;
    let rows: Vec<MarketplaceItem> = raw_rows.into_iter().take(page_size as usize).map(|r| {
        let tags: Vec<String> = serde_json::from_str(&r.8).unwrap_or_default();
        let i18n_map: std::collections::HashMap<String, super::types::EntryI18n> =
            serde_json::from_str(&r.12).unwrap_or_default();
        let i18n_en = i18n_map.get("en-US");
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
            i18n_name: i18n_en.and_then(|x| x.name.clone()),
            i18n_description: i18n_en.and_then(|x| x.description.clone()),
        }
    }).collect();

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
    let row = conn.query_row(
        "SELECT slug, name, version, author, description, item_type, category, icon,
                tags_json, size_bytes, min_app_version, locale, i18n_json,
                spec_yaml, requires_json
         FROM automation_marketplace_items
         WHERE registry_id = ?1 AND slug = ?2",
        params![registry_id, slug],
        |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?,
            r.get::<_, String>(6)?, r.get::<_, Option<String>>(7)?,
            r.get::<_, String>(8)?, r.get::<_, Option<i64>>(9)?,
            r.get::<_, Option<String>>(10)?, r.get::<_, Option<String>>(11)?,
            r.get::<_, String>(12)?, r.get::<_, Option<String>>(13)?,
            r.get::<_, String>(14)?,
        )),
    ).ok();

    let Some((slug, name, version, author, description, app_type, category, icon, tags_json, size_bytes, min_app_version, locale, i18n_json, spec_yaml, requires_json)) = row else {
        return Ok(None);
    };

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let i18n_map: std::collections::HashMap<String, super::types::EntryI18n> =
        serde_json::from_str(&i18n_json).unwrap_or_default();
    let i18n_en = i18n_map.get("en-US");

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
        i18n_name: i18n_en.and_then(|x| x.name.clone()),
        i18n_description: i18n_en.and_then(|x| x.description.clone()),
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
```

- [ ] **Step 4: Register the new module in mod.rs**

Edit `src-tauri/src/automation/marketplace/mod.rs` — add after the existing `pub mod` declarations near the top:

```rust
pub mod cache;
```

- [ ] **Step 5: Smoke-test the cache module — write a small test in cache.rs**

Append to `cache.rs`:

```rust
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
        // Insert two items manually for the test
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
```

- [ ] **Step 6: Run the tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd src-tauri && cargo test --lib automation::marketplace::cache 2>&1 | tail -5
```

Expected: build succeeds, 2 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/marketplace/cache.rs src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): cache layer — TTL sync + FTS5 query + category counts

V23a-backed SQLite cache replaces Phase 1's every-call HTTP fetch:

  sync_registry(source, force)        -> u32 (item count). TTL 1h, returns
                                        stale data on net failure (best-effort).
  query_items(conn, search?, type?,
              category?, page, page_size) -> MarketplaceQueryResult
  category_counts(conn, type?)        -> HashMap<category, count>
  get_item_with_spec(conn, slug)      -> (item, spec_yaml?, requires, i18n)
  cache_spec_yaml(conn, slug, yaml)   -> ()

Trigram FTS5 indexes name + description + author + tags (joined). Sync
prunes removed slugs in a single transaction. ETag/Last-Modified columns
exist but Phase 3a doesn't populate them (halo_adapter doesn't surface
response headers yet; Phase 3b reqwest-level change).

Two unit tests (empty query + category counts)."
```

---

## Task 3: Backend types — MarketplaceQueryResult / Detail / Update

**Files:**
- Modify: `src-tauri/src/automation/marketplace/types.rs`

- [ ] **Step 1: Add the three new types**

At the bottom of `types.rs`:

```rust
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
```

- [ ] **Step 2: Re-export from mod.rs**

Edit `src-tauri/src/automation/marketplace/mod.rs`. Find the `pub use types::{...}` line and add the new types:

```rust
pub use types::{
    MarketplaceItem, MarketplaceQueryResult, MarketplaceDetail,
    MarketplaceUpdate, MarketplaceInstallProgress,
    RegistryEntry, RegistryIndex, RegistrySource,
};
```

- [ ] **Step 3: Verify build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/marketplace/types.rs src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): types — MarketplaceQueryResult / Detail / Update / InstallProgress

Adds the four new wire types the Phase 3a Tauri commands need:
  MarketplaceQueryResult — items + total + has_more (camelCase for TS)
  MarketplaceDetail      — full detail incl. spec_yaml + installed_version
  MarketplaceUpdate      — {slug, installed_version, latest_version}
  MarketplaceInstallProgress — Tauri event payload during install

All re-exported from marketplace::mod for clean import paths."
```

---

## Task 4: Backend Tauri commands — query / get_detail / check_updates + upgrade install_marketplace_human

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

This is a big task. Three new commands + one shape upgrade. ~250 LOC across the three files.

- [ ] **Step 1: Add `query_marketplace_cached` orchestration function in mod.rs**

In `src-tauri/src/automation/marketplace/mod.rs`, after the existing `list_humans` function, add:

```rust
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
    // Best-effort sync (logs warn on failure if we have cached data)
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
        // Reconstruct a RegistryEntry from the cached item so halo_adapter can
        // build the right download URL (path = packages/digital-humans/{slug})
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

    // Parse the YAML (best-effort — None if parse fails so UI can still render)
    let parsed_spec_json = crate::automation::protocol::parse::parse_humane_v1(&spec_yaml)
        .ok()
        .and_then(|p| serde_json::to_value(&p.spec).ok());

    // Extract dependencies from cached requires_json
    let requires: serde_json::Value = serde_json::from_str(&requires_json).unwrap_or(serde_json::json!({}));
    let requires_mcps = requires.get("mcps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let requires_skills = requires.get("skills")
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
        ).ok()
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
/// Returns rows where the registry version differs from installed version.
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
    let rows = stmt.query_map(rusqlite::params![source.id], |r| {
        let source_ref: String = r.get(0)?;
        let installed: String = r.get(1)?;
        let latest: String = r.get(2)?;
        // Extract slug from "marketplace://halo/{slug}"
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
```

- [ ] **Step 2: Upgrade install_human to take user_config + progress channel**

Replace the existing `install_human` function in `mod.rs` with:

```rust
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
    let row = runtime
        .install_humane_spec_from_source(&yaml, "marketplace", Some(source_ref))
        .await
        .with_context(|| format!("install_humane_spec failed for '{}'", slug))?;

    // Phase 3a: user_config is captured but Phase 1 schema doesn't surface it via
    // a typed user_config column. Encode as JSON in user_config_values column.
    // space_id is currently ignored — workspaces are wired through AutomationHub's
    // current-workspace context. Phase 3b adds explicit space scoping.
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
```

- [ ] **Step 3: Add three new Tauri commands in tauri_commands.rs**

Find the existing `install_marketplace_human` command around line 5581 and replace it AND add the three new commands. Group them in a section:

```rust
// ─── Marketplace (Phase 3a — § 13) ────────────────────────────────────

#[tauri::command]
pub async fn query_marketplace(
    state: State<'_, AppState>,
    search: Option<String>,
    item_type: Option<String>,
    category: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<crate::automation::marketplace::MarketplaceQueryResult, Error> {
    crate::automation::marketplace::query_marketplace_cached(
        &state.runtime_service,
        search,
        item_type,
        category,
        page.unwrap_or(0),
        page_size.unwrap_or(20),
    )
    .await
    .map_err(|e| Error::Internal(format!("{:#}", e)))
}

#[tauri::command]
pub async fn get_marketplace_detail(
    state: State<'_, AppState>,
    slug: String,
) -> Result<crate::automation::marketplace::MarketplaceDetail, Error> {
    crate::automation::marketplace::get_marketplace_detail_cached(&state.runtime_service, &slug)
        .await
        .map_err(|e| Error::Internal(format!("{:#}", e)))
}

#[tauri::command]
pub async fn check_marketplace_updates(
    state: State<'_, AppState>,
) -> Result<Vec<crate::automation::marketplace::MarketplaceUpdate>, Error> {
    crate::automation::marketplace::check_updates_cached(&state.runtime_service)
        .await
        .map_err(|e| Error::Internal(format!("{:#}", e)))
}

#[tauri::command]
pub async fn install_marketplace_human(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    slug: String,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    progress_channel: Option<String>,
) -> Result<crate::automation::manager::HumaneSpecRow, Error> {
    crate::automation::marketplace::install_human(
        &state.runtime_service,
        app_handle,
        &slug,
        space_id,
        user_config,
        progress_channel,
    )
    .await
    .map_err(|e| {
        tracing::error!(slug = %slug, error = format!("{:#}", e), "install_marketplace_human failed");
        Error::Internal(format!("{:#}", e))
    })
}

#[tauri::command]
pub async fn refresh_marketplace(
    state: State<'_, AppState>,
) -> Result<u32, Error> {
    let source = crate::automation::marketplace::RegistrySource::default();
    crate::automation::marketplace::cache::sync_registry(
        &state.runtime_service.db,
        &source,
        true,
    )
    .await
    .map_err(|e| Error::Internal(format!("{:#}", e)))
}

// list_marketplace_humans kept as deprecated wrapper for backward compat — Phase 3b removes
#[tauri::command]
pub async fn list_marketplace_humans(
    state: State<'_, AppState>,
    _registry_url: Option<String>,
) -> Result<Vec<crate::automation::marketplace::MarketplaceItem>, Error> {
    let result = crate::automation::marketplace::query_marketplace_cached(
        &state.runtime_service, None, Some("automation".into()), None, 0, 200,
    )
    .await
    .map_err(|e| Error::Internal(format!("{:#}", e)))?;
    Ok(result.items)
}
```

Delete the previous `install_marketplace_human` and `list_marketplace_humans` bodies.

- [ ] **Step 4: Register new commands in main.rs invoke_handler!**

Find the `invoke_handler!` macro in `src-tauri/src/main.rs`. Find the existing `install_marketplace_human` and `list_marketplace_humans` entries. Add right after them:

```rust
            query_marketplace,
            get_marketplace_detail,
            check_marketplace_updates,
            refresh_marketplace,
```

- [ ] **Step 5: Build + smoke**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd src-tauri && cargo test --lib 2>&1 | tail -3
```

Expected: build OK, all tests still pass (no test changes; the new commands aren't tested directly — UI-level tests cover them).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(marketplace): 4 new Tauri commands + install upgrade

  query_marketplace(search?, item_type?, category?, page?, page_size?)
    → MarketplaceQueryResult (items + total + has_more)
  get_marketplace_detail(slug)
    → MarketplaceDetail (item + spec_yaml + parsed_spec + requires + installed_version)
  check_marketplace_updates()
    → Vec<MarketplaceUpdate>
  refresh_marketplace()
    → u32 (item count; bypasses TTL)

install_marketplace_human upgraded:
  before: (registry_url, slug) → HumaneSpecRow
  after:  (slug, space_id?, user_config?, progress_channel?) → HumaneSpecRow
  - Emits Tauri events on progress_channel: phase, percent, message
  - Stores user_config into user_config_values column
  - Calls runtime.activate() after install
  - Emits chat:pet-celebrate event on success (Pet awareness — § 13.3.E)

list_marketplace_humans kept as deprecated wrapper around query_marketplace
for backward compat (Phase 3b removes)."
```

---

## Task 5: Frontend atoms — marketplace state slice

**Files:**
- Modify: `ui/src/atoms/marketplace.ts` (or create if it doesn't exist as separate file)

- [ ] **Step 1: Survey the current marketplace atoms**

Read the existing file:

```bash
cat ui/src/atoms/automation.ts | head -20
```

You'll see `humaneSpecsAtom`, `pendingEscalationsAtom`, etc. Keep those. Add new atoms in a separate file.

- [ ] **Step 2: Create the marketplace atom slice**

Create `ui/src/atoms/marketplace.ts`:

```typescript
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import type { MarketplaceItem, MarketplaceUpdate, MarketplaceDetail } from '@/lib/tauri-bridge'

// Type filter — All / Digital Human / Skill / MCP
export type MarketplaceItemTypeFilter = 'all' | 'automation' | 'skill' | 'mcp'

// 3-sub-view atom for AutomationsView (我的数字人 / 我的应用 / 应用商店)
export type AutomationsSubview = 'humans' | 'apps' | 'store' | 'store-detail'

export const automationsSubviewAtom = atomWithStorage<AutomationsSubview>(
  'uclaw-automations-subview',
  'humans',
)

// Store filters — debounced search lives upstream in the StoreHeader component
export interface MarketplaceFilters {
  search: string
  itemType: MarketplaceItemTypeFilter
  category: string | null
}

export const marketplaceFiltersAtom = atom<MarketplaceFilters>({
  search: '',
  itemType: 'all',
  category: null,
})

// Paged item list (accumulator — page 0 replaces, page > 0 appends)
export const marketplaceItemsAtom = atom<MarketplaceItem[]>([])
export const marketplacePageAtom = atom<number>(0)
export const marketplaceHasMoreAtom = atom<boolean>(false)
export const marketplaceTotalAtom = atom<number>(0)
export const marketplaceLoadingAtom = atom<boolean>(false)
export const marketplaceLoadErrorAtom = atom<string | null>(null)

// Category counts for StoreHeader chips
export const marketplaceCategoryCountsAtom = atom<Record<string, number>>({})

// Current detail view target (null = grid view, slug = detail view)
export const marketplaceSelectedSlugAtom = atom<string | null>(null)
export const marketplaceDetailAtom = atom<MarketplaceDetail | null>(null)
export const marketplaceDetailLoadingAtom = atom<boolean>(false)

// Detail page sub-tab (概览 / 配置 / 依赖 / 提示词)
export type DetailSubTab = 'overview' | 'config' | 'requires' | 'prompt'
export const marketplaceDetailSubtabAtom = atom<DetailSubTab>('overview')

// Available updates badge (Updates check polling)
export const marketplaceUpdatesAtom = atom<MarketplaceUpdate[]>([])

// Install wizard state
export type InstallWizardStep = 'scope' | 'config' | 'confirm' | 'progress' | null

export interface InstallWizardState {
  step: InstallWizardStep
  slug: string | null
  spaceId: string | null
  userConfig: Record<string, unknown>
  progress: { phase: string; percent: number; message?: string } | null
  error: string | null
}

export const installWizardAtom = atom<InstallWizardState>({
  step: null,
  slug: null,
  spaceId: null,
  userConfig: {},
  progress: null,
  error: null,
})

// Sandbox try-install state
export interface SandboxState {
  active: boolean
  slug: string | null
  workspaceId: string | null   // ephemeral workspace id
  startedAt: number | null
}
export const sandboxStateAtom = atom<SandboxState>({
  active: false,
  slug: null,
  workspaceId: null,
  startedAt: null,
})
```

- [ ] **Step 3: Verify the file compiles**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
```

Expected: 0 (the types it imports come from tauri-bridge which we'll update in Task 6).

If you get errors about `MarketplaceUpdate` / `MarketplaceDetail` not exported from tauri-bridge, that's expected — Task 6 adds them.

- [ ] **Step 4: Commit**

```bash
git add ui/src/atoms/marketplace.ts
git commit -m "feat(marketplace): atom slice — filters, items, detail, wizard, sandbox state

13 atoms organised into 4 groups:

  Subview navigation:
    automationsSubviewAtom (atomWithStorage) — humans/apps/store/store-detail

  Browse/search state:
    marketplaceFiltersAtom, marketplaceItemsAtom, marketplacePageAtom,
    marketplaceHasMoreAtom, marketplaceTotalAtom, marketplaceLoadingAtom,
    marketplaceLoadErrorAtom, marketplaceCategoryCountsAtom

  Detail state:
    marketplaceSelectedSlugAtom, marketplaceDetailAtom,
    marketplaceDetailLoadingAtom, marketplaceDetailSubtabAtom

  Operations state:
    marketplaceUpdatesAtom, installWizardAtom, sandboxStateAtom

atomWithStorage on subview so user returns to where they left off."
```

---

## Task 6: Frontend bindings — tauri-bridge.ts types + wrappers

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: Locate existing marketplace section**

```bash
grep -n "MarketplaceItem\|installMarketplaceHuman\|listMarketplaceHumans" ui/src/lib/tauri-bridge.ts | head
```

You'll find the Phase 1 wrappers and types.

- [ ] **Step 2: Add the new interfaces near MarketplaceItem**

Find the `export interface MarketplaceItem` and add right after it:

```typescript
export interface MarketplaceQueryResult {
  items: MarketplaceItem[]
  total: number
  hasMore: boolean
}

export interface MarketplaceDetail {
  item: MarketplaceItem
  specYaml: string
  parsedSpecJson: unknown | null
  requiresMcps: string[]
  requiresSkills: string[]
  installedVersion: string | null
}

export interface MarketplaceUpdate {
  slug: string
  installedVersion: string
  latestVersion: string
}

export interface MarketplaceInstallProgress {
  phase: 'fetching_spec' | 'parsing' | 'installing' | 'activating' | 'complete' | string
  slug: string
  percent: number
  message: string | null
}
```

- [ ] **Step 3: Replace the existing install wrapper + add the new ones**

Find `installMarketplaceHuman` and `listMarketplaceHumans`. Replace and append:

```typescript
export const queryMarketplace = (
  search?: string,
  itemType?: string,
  category?: string,
  page?: number,
  pageSize?: number,
): Promise<MarketplaceQueryResult> =>
  invoke<MarketplaceQueryResult>('query_marketplace', {
    search, itemType, category, page, pageSize,
  })

export const getMarketplaceDetail = (slug: string): Promise<MarketplaceDetail> =>
  invoke<MarketplaceDetail>('get_marketplace_detail', { slug })

export const checkMarketplaceUpdates = (): Promise<MarketplaceUpdate[]> =>
  invoke<MarketplaceUpdate[]>('check_marketplace_updates')

export const refreshMarketplace = (): Promise<number> =>
  invoke<number>('refresh_marketplace')

export const installMarketplaceHuman = (
  slug: string,
  spaceId?: string,
  userConfig?: Record<string, unknown>,
  progressChannel?: string,
): Promise<HumaneSpecRow> =>
  invoke<HumaneSpecRow>('install_marketplace_human', {
    slug, spaceId, userConfig, progressChannel,
  })

// Deprecated — kept until Phase 3b removes; new code uses queryMarketplace + filter('automation')
export const listMarketplaceHumans = (): Promise<MarketplaceItem[]> =>
  invoke<MarketplaceItem[]>('list_marketplace_humans', { registryUrl: null })
```

- [ ] **Step 4: Verify tsc clean**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
```

Expected: 0.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts
git commit -m "feat(marketplace): TS bindings — query/detail/updates/install (Phase 3a)

  queryMarketplace(search?, itemType?, category?, page?, pageSize?)
    → MarketplaceQueryResult
  getMarketplaceDetail(slug)
    → MarketplaceDetail
  checkMarketplaceUpdates()
    → MarketplaceUpdate[]
  refreshMarketplace()
    → number (item count)
  installMarketplaceHuman(slug, spaceId?, userConfig?, progressChannel?)
    → HumaneSpecRow (upgraded signature)

4 new interfaces: MarketplaceQueryResult, MarketplaceDetail,
MarketplaceUpdate, MarketplaceInstallProgress (all camelCase via
Tauri serde rename in Rust types).

listMarketplaceHumans kept as deprecated wrapper."
```

---

## Task 7: AppTypeBadge component

**Files:**
- Create: `ui/src/components/automation/AppTypeBadge.tsx`

- [ ] **Step 1: Write the component**

```tsx
import * as React from 'react'
import { cn } from '@/lib/utils'

const TYPE_META: Record<
  string,
  { label: string; tooltip: string; cls: string }
> = {
  automation: {
    label: '数字人',
    tooltip: '自动化数字员工 — 完整的 AI 智能体，能订阅事件、执行任务、记忆历史',
    cls: 'bg-primary/10 text-primary border-primary/30',
  },
  mcp: {
    label: 'MCP',
    tooltip: 'Model Context Protocol 服务 — 给数字人提供工具能力',
    cls: 'bg-blue-500/10 text-blue-500 border-blue-500/30',
  },
  skill: {
    label: '技能',
    tooltip: '复用技能脚本 — 装到工作区供多个数字人共享',
    cls: 'bg-success/10 text-success border-success/30',
  },
  extension: {
    label: '扩展',
    tooltip: 'uClaw 应用扩展',
    cls: 'bg-warning/10 text-warning border-warning/30',
  },
}

interface Props {
  type: string
  /** Position of the hover tooltip relative to the badge. */
  tooltipDirection?: 'up' | 'down'
  className?: string
}

export function AppTypeBadge({ type, tooltipDirection = 'down', className }: Props): React.ReactElement {
  const meta = TYPE_META[type] ?? {
    label: type,
    tooltip: `Unknown type: ${type}`,
    cls: 'bg-muted text-muted-foreground border-border',
  }

  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-[1px] rounded-md border text-[10px] font-medium tabular-nums',
        meta.cls,
        className,
      )}
      title={meta.tooltip}
      aria-label={meta.tooltip}
      data-tooltip-direction={tooltipDirection}
    >
      {meta.label}
    </span>
  )
}
```

- [ ] **Step 2: Write a smoke test**

Create `ui/src/components/automation/AppTypeBadge.test.tsx`:

```tsx
import { describe, test, expect } from 'vitest'
import { render } from '@testing-library/react'
import { AppTypeBadge } from './AppTypeBadge'

describe('AppTypeBadge', () => {
  test('renders Chinese label for automation', () => {
    const { getByText } = render(<AppTypeBadge type="automation" />)
    expect(getByText('数字人')).toBeInTheDocument()
  })
  test('renders MCP label', () => {
    const { getByText } = render(<AppTypeBadge type="mcp" />)
    expect(getByText('MCP')).toBeInTheDocument()
  })
  test('falls back to raw type string for unknown type', () => {
    const { getByText } = render(<AppTypeBadge type="exotic-type" />)
    expect(getByText('exotic-type')).toBeInTheDocument()
  })
  test('exposes tooltip via title attribute', () => {
    const { container } = render(<AppTypeBadge type="automation" />)
    const el = container.querySelector('[title]')
    expect(el?.getAttribute('title')).toContain('自动化数字员工')
  })
})
```

- [ ] **Step 3: Run tests**

```bash
cd ui && npm test -- --run AppTypeBadge 2>&1 | tail -5
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/AppTypeBadge.tsx ui/src/components/automation/AppTypeBadge.test.tsx
git commit -m "feat(marketplace): AppTypeBadge — 4-color type pill + hover tooltip

Chinese labels with theme-aware tint backgrounds:
  automation → 数字人  (primary)
  mcp        → MCP     (blue-500)
  skill      → 技能    (success/emerald)
  extension  → 扩展    (warning/amber)

Used inline on StoreCard, StoreDetail header, and AutomationCard list rows.
title attribute provides the hover tooltip (browser-native, lightweight).

4 vitest tests covering rendering, fallback, tooltip wiring."
```

---

## Task 8: StoreHeader — search + type tabs + category chips with counts

**Files:**
- Create: `ui/src/components/automation/StoreHeader.tsx`

- [ ] **Step 1: Write the component**

```tsx
import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { Search, RotateCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  marketplaceFiltersAtom,
  marketplaceCategoryCountsAtom,
  marketplaceLoadingAtom,
  type MarketplaceItemTypeFilter,
} from '@/atoms/marketplace'

const TYPE_TABS: { id: MarketplaceItemTypeFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'automation', label: '数字人' },
  { id: 'skill', label: '技能' },
  { id: 'mcp', label: 'MCP' },
]

const CATEGORY_LABELS: Record<string, string> = {
  social: '社交',
  productivity: '生产力',
  content: '内容',
  news: '新闻',
  data: '数据',
  dev: '开发',
  shopping: '购物',
  other: '其他',
}

interface Props {
  onRefresh: () => void
}

export function StoreHeader({ onRefresh }: Props): React.ReactElement {
  const [filters, setFilters] = useAtom(marketplaceFiltersAtom)
  const counts = useAtomValue(marketplaceCategoryCountsAtom)
  const loading = useAtomValue(marketplaceLoadingAtom)

  // Debounce search by 300ms — Tailwind doesn't help with input debouncing,
  // so we hold a draft string in local state and push to atom on settle.
  const [draft, setDraft] = React.useState(filters.search)
  React.useEffect(() => {
    if (draft === filters.search) return
    const handle = setTimeout(() => {
      setFilters((f) => ({ ...f, search: draft }))
    }, 300)
    return () => clearTimeout(handle)
  }, [draft, filters.search, setFilters])

  // Build the category chip list — show only categories with at least one item
  const categoryChips = React.useMemo(() => {
    const entries = Object.entries(counts).sort((a, b) => b[1] - a[1])
    return entries
  }, [counts])

  return (
    <div className="border-b border-border/50">
      {/* Row 1: search + refresh */}
      <div className="flex items-center gap-2 px-6 py-3">
        <div className="relative flex-1 max-w-2xl">
          <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/60" />
          <input
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="搜索数字人 / 技能 / MCP..."
            className={cn(
              'w-full pl-8 pr-3 py-1.5 text-[13px]',
              'rounded-md border border-border/50 bg-card',
              'placeholder:text-muted-foreground/50',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
              'transition-colors',
            )}
          />
        </div>
        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className={cn(
            'p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/30 transition-colors',
            loading && 'opacity-50 cursor-wait',
          )}
          title="刷新注册表"
        >
          <RotateCw size={13} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>

      {/* Row 2: type tabs */}
      <div className="flex items-center gap-1 px-6 pb-2">
        {TYPE_TABS.map((tab) => {
          const active = filters.itemType === tab.id
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setFilters((f) => ({ ...f, itemType: tab.id, category: null }))}
              className={cn(
                'relative px-3 py-1 text-[12px] rounded-md transition-colors',
                active
                  ? 'bg-muted text-foreground font-medium'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent/30',
              )}
            >
              {active && <span className="absolute left-0 top-1.5 bottom-1.5 w-[2px] bg-primary rounded-r" />}
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Row 3: category chips (only when there are any) */}
      {categoryChips.length > 0 && (
        <div className="flex items-center gap-1.5 px-6 pb-3 overflow-x-auto">
          <button
            type="button"
            onClick={() => setFilters((f) => ({ ...f, category: null }))}
            className={cn(
              'shrink-0 px-2 py-0.5 rounded-full text-[11px] border transition-colors',
              filters.category === null
                ? 'bg-primary/10 text-primary border-primary/30'
                : 'bg-muted text-muted-foreground border-border/50 hover:bg-muted/80',
            )}
          >
            全部
          </button>
          {categoryChips.map(([cat, count]) => {
            const active = filters.category === cat
            const label = CATEGORY_LABELS[cat] ?? cat
            return (
              <button
                key={cat}
                type="button"
                onClick={() => setFilters((f) => ({ ...f, category: cat }))}
                className={cn(
                  'shrink-0 px-2 py-0.5 rounded-full text-[11px] border transition-colors tabular-nums',
                  active
                    ? 'bg-primary/10 text-primary border-primary/30'
                    : 'bg-muted text-muted-foreground border-border/50 hover:bg-muted/80',
                )}
              >
                {label} · {count}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Write a small smoke test**

Create `ui/src/components/automation/StoreHeader.test.tsx`:

```tsx
import { describe, test, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { StoreHeader } from './StoreHeader'

describe('StoreHeader', () => {
  test('renders search input and type tabs', () => {
    const { getByPlaceholderText, getByText } = renderWithProviders(
      <StoreHeader onRefresh={() => {}} />,
    )
    expect(getByPlaceholderText(/搜索数字人/)).toBeInTheDocument()
    expect(getByText('全部')).toBeInTheDocument()
    expect(getByText('数字人')).toBeInTheDocument()
    expect(getByText('技能')).toBeInTheDocument()
    expect(getByText('MCP')).toBeInTheDocument()
  })

  test('refresh button triggers callback', () => {
    const onRefresh = vi.fn()
    const { getByTitle } = renderWithProviders(<StoreHeader onRefresh={onRefresh} />)
    fireEvent.click(getByTitle('刷新注册表'))
    expect(onRefresh).toHaveBeenCalledOnce()
  })
})
```

- [ ] **Step 3: Run tests**

```bash
cd ui && npm test -- --run StoreHeader 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/StoreHeader.tsx ui/src/components/automation/StoreHeader.test.tsx
git commit -m "feat(marketplace): StoreHeader — search + type tabs + smart category chips

3 rows in a single header strip with consistent border-b/50 separator:

  Row 1: <Search icon /> 300ms-debounced input + manual refresh button
  Row 2: 4-type tab pills (全部 / 数字人 / 技能 / MCP) with SettingsNav
         2-px left-bar active indicator
  Row 3: category chips with counts (Social · 12) — primary/10 tint when
         active, muted when inactive

All theme tokens, no hardcoded colors. Reset category when switching type
(category is type-scoped). Counts come from marketplaceCategoryCountsAtom
populated by the orchestrator (StoreView). Phase 3a innovations D + the
SettingsNav left-bar active pattern from § 13.5."
```

---

## Task 9: StoreCard + StoreGrid

**Files:**
- Create: `ui/src/components/automation/StoreCard.tsx`
- Create: `ui/src/components/automation/StoreGrid.tsx`

- [ ] **Step 1: StoreCard.tsx**

```tsx
import * as React from 'react'
import { Download } from 'lucide-react'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const MAX_VISIBLE_TAGS = 3

interface Props {
  item: MarketplaceItem
  hasUpdate?: boolean
  isInstalled?: boolean
  onClick: (slug: string) => void
}

export function StoreCard({ item, hasUpdate, isInstalled, onClick }: Props): React.ReactElement {
  const displayName = item.i18nName ?? item.name
  const displayDesc = item.i18nDescription ?? item.description
  const visibleTags = item.tags.slice(0, MAX_VISIBLE_TAGS)
  const hiddenTagCount = item.tags.length - visibleTags.length

  return (
    <button
      type="button"
      onClick={() => onClick(item.slug)}
      className={cn(
        'w-full text-left p-4',
        'rounded-xl border border-border/50 bg-card',
        'hover:border-primary/40 hover:bg-secondary/50',
        'transition-colors',
      )}
    >
      {/* Row 1: icon + name + type + version */}
      <div className="flex items-start justify-between gap-2 mb-1">
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <div className="w-7 h-7 rounded-md bg-primary/10 flex items-center justify-center text-[12px] shrink-0">
            {item.icon ?? '🤖'}
          </div>
          <span className="text-[13px] font-medium truncate">{displayName}</span>
          <AppTypeBadge type={item.appType} tooltipDirection="up" />
        </div>
        <span className="text-[10px] text-muted-foreground tabular-nums shrink-0 mt-0.5">
          v{item.version}
        </span>
      </div>

      {/* Row 2: author + status indicators */}
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span>by {item.author}</span>
        {isInstalled && !hasUpdate && (
          <span className="px-1.5 py-[1px] rounded-md bg-success-bg text-success text-[10px] font-medium">
            已安装
          </span>
        )}
        {hasUpdate && (
          <span className="px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
            有更新
          </span>
        )}
      </div>

      {/* Description */}
      <p className="text-[12px] text-muted-foreground mt-2 line-clamp-2 min-h-[2.5em]">
        {displayDesc}
      </p>

      {/* Tags */}
      {visibleTags.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-3">
          {visibleTags.map((tag) => (
            <span
              key={tag}
              className="text-[10px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground"
            >
              {tag}
            </span>
          ))}
          {hiddenTagCount > 0 && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
              +{hiddenTagCount}
            </span>
          )}
        </div>
      )}

      {/* CTA hint */}
      <div className="flex items-center gap-1 mt-3 text-[10px] text-muted-foreground">
        <Download size={10} />
        <span>查看详情</span>
      </div>
    </button>
  )
}
```

- [ ] **Step 2: StoreGrid.tsx**

```tsx
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Loader2, Search as SearchIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { StoreCard } from './StoreCard'
import {
  marketplaceItemsAtom,
  marketplaceLoadingAtom,
  marketplaceLoadErrorAtom,
  marketplaceHasMoreAtom,
  marketplaceTotalAtom,
  marketplaceUpdatesAtom,
  marketplaceSelectedSlugAtom,
  automationsSubviewAtom,
} from '@/atoms/marketplace'
import { humaneSpecsAtom } from '@/atoms/automation'

interface Props {
  onLoadMore: () => void
}

export function StoreGrid({ onLoadMore }: Props): React.ReactElement {
  const items = useAtomValue(marketplaceItemsAtom)
  const loading = useAtomValue(marketplaceLoadingAtom)
  const error = useAtomValue(marketplaceLoadErrorAtom)
  const hasMore = useAtomValue(marketplaceHasMoreAtom)
  const total = useAtomValue(marketplaceTotalAtom)
  const updates = useAtomValue(marketplaceUpdatesAtom)
  const installedSpecs = useAtomValue(humaneSpecsAtom)
  const setSelectedSlug = useSetAtom(marketplaceSelectedSlugAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)

  const updateSlugs = React.useMemo(() => new Set(updates.map((u) => u.slug)), [updates])
  const installedSlugs = React.useMemo(() => {
    return new Set(
      installedSpecs
        .filter((s) => s.source === 'marketplace' && s.sourceRef)
        .map((s) => {
          // source_ref shape: 'marketplace://halo/{slug}'
          const m = /^marketplace:\/\/[^/]+\/(.+)$/.exec(s.sourceRef ?? '')
          return m?.[1] ?? null
        })
        .filter((x): x is string => x !== null),
    )
  }, [installedSpecs])

  const openDetail = (slug: string) => {
    setSelectedSlug(slug)
    setSubview('store-detail')
  }

  // Empty / loading / error states
  if (error) {
    return (
      <div className="flex flex-col items-center gap-3 py-16 text-muted-foreground">
        <span className="text-[13px]">无法加载市场</span>
        <span className="text-[11px] max-w-md text-center">{error}</span>
      </div>
    )
  }
  if (loading && items.length === 0) {
    return (
      <div className="flex items-center gap-2 justify-center py-16 text-muted-foreground">
        <Loader2 size={14} className="animate-spin" />
        <span className="text-[13px]">正在加载注册表...</span>
      </div>
    )
  }
  if (!loading && items.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 py-16 text-muted-foreground">
        <SearchIcon size={28} className="text-muted-foreground/30" />
        <p className="text-[13px]">市场里还没有匹配的数字员工</p>
        <p className="text-[11px]">试试别的关键词，或浏览全部分类</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col">
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 px-6 py-4">
        {items.map((item) => (
          <StoreCard
            key={item.slug}
            item={item}
            hasUpdate={updateSlugs.has(item.slug)}
            isInstalled={installedSlugs.has(item.slug)}
            onClick={openDetail}
          />
        ))}
      </div>
      {hasMore && (
        <div className="flex justify-center pb-6">
          <button
            type="button"
            onClick={onLoadMore}
            disabled={loading}
            className={cn(
              'px-4 py-1.5 text-[12px] rounded-md',
              'border border-border/50 bg-card hover:bg-accent/30',
              'transition-colors disabled:opacity-50 disabled:cursor-wait',
            )}
          >
            {loading ? '加载中...' : `加载更多（已显示 ${items.length} / ${total}）`}
          </button>
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 3: Smoke test for both**

Create `ui/src/components/automation/StoreCard.test.tsx`:

```tsx
import { describe, test, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { StoreCard } from './StoreCard'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const makeItem = (overrides: Partial<MarketplaceItem> = {}): MarketplaceItem => ({
  slug: 'ai-news', name: 'AI News', version: '1.0.0', author: 'a',
  description: 'desc', appType: 'automation', category: 'news',
  icon: null, tags: ['ai', 'news'], sizeBytes: null, minAppVersion: null,
  locale: null, i18nName: null, i18nDescription: null,
  ...overrides,
})

describe('StoreCard', () => {
  test('renders name, author, version, description', () => {
    const { getByText } = renderWithProviders(<StoreCard item={makeItem()} onClick={() => {}} />)
    expect(getByText('AI News')).toBeInTheDocument()
    expect(getByText('by a')).toBeInTheDocument()
    expect(getByText('v1.0.0')).toBeInTheDocument()
    expect(getByText('desc')).toBeInTheDocument()
  })
  test('shows "已安装" badge when isInstalled', () => {
    const { getByText } = renderWithProviders(
      <StoreCard item={makeItem()} isInstalled={true} onClick={() => {}} />,
    )
    expect(getByText('已安装')).toBeInTheDocument()
  })
  test('shows "有更新" badge when hasUpdate', () => {
    const { getByText } = renderWithProviders(
      <StoreCard item={makeItem()} isInstalled={true} hasUpdate={true} onClick={() => {}} />,
    )
    expect(getByText('有更新')).toBeInTheDocument()
  })
  test('calls onClick with slug on click', () => {
    const onClick = vi.fn()
    const { getByRole } = renderWithProviders(
      <StoreCard item={makeItem()} onClick={onClick} />,
    )
    fireEvent.click(getByRole('button'))
    expect(onClick).toHaveBeenCalledWith('ai-news')
  })
})
```

- [ ] **Step 4: Run tests**

```bash
cd ui && npm test -- --run StoreCard 2>&1 | tail -5
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/StoreCard.tsx ui/src/components/automation/StoreGrid.tsx ui/src/components/automation/StoreCard.test.tsx
git commit -m "feat(marketplace): StoreCard + StoreGrid — installed/update badges + WelcomeView-tone empty states

StoreCard:
  - rounded-xl bg-card border-border/50 (uClaw card pattern, not rounded-lg)
  - 7x7 primary/10 icon bubble at top-left
  - inline 已安装 / 有更新 badges using theme tokens (success-bg / warning-bg)
  - line-clamp-2 description with min-h to prevent jumpy heights
  - tag overflow indicator (+N)
  - 查看详情 hint at bottom (instead of '点击安装' — single tap goes to detail, not direct install)

StoreGrid:
  - Responsive 1/2/3 columns (mobile / md / xl)
  - Load More button when has_more (replaces infinite scroll — simpler)
  - 3 empty states matching WelcomeView tone:
    - error: '无法加载市场' + underlying error msg
    - loading: spinner + '正在加载注册表...'
    - empty result: dimmed icon + '市场里还没有匹配的数字员工 — 试试别的关键词'

Marketplace innovation § 13.3.H (WelcomeView-tone empty states)."
```

---

## Task 10: StoreFeaturedRow + StoreView orchestrator

**Files:**
- Create: `ui/src/components/automation/StoreFeaturedRow.tsx`
- Create: `ui/src/components/automation/StoreView.tsx`

- [ ] **Step 1: StoreFeaturedRow.tsx**

```tsx
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import {
  marketplaceItemsAtom,
  marketplaceSelectedSlugAtom,
  automationsSubviewAtom,
} from '@/atoms/marketplace'

// Phase 3a hardcoded featured list — Phase 4 makes this remote-driven.
const FEATURED_SLUGS = [
  'ai-daily-news',
  'github-pr-reviewer',
  'weibo-hot-tracker',
  'wechat-article-monitor',
]

export function StoreFeaturedRow(): React.ReactElement | null {
  const items = useAtomValue(marketplaceItemsAtom)
  const setSelectedSlug = useSetAtom(marketplaceSelectedSlugAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)

  const featured = React.useMemo(
    () => FEATURED_SLUGS.map((slug) => items.find((i) => i.slug === slug)).filter((x): x is NonNullable<typeof x> => x !== undefined),
    [items],
  )

  if (featured.length === 0) return null

  return (
    <div className="px-6 pt-4 pb-2">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
        <Sparkles size={11} className="text-primary" />
        <span>今日推荐</span>
      </div>
      <div className="flex gap-3 overflow-x-auto pb-1 -mx-6 px-6 scrollbar-thin">
        {featured.map((item) => (
          <button
            key={item.slug}
            type="button"
            onClick={() => {
              setSelectedSlug(item.slug)
              setSubview('store-detail')
            }}
            className={cn(
              'shrink-0 w-[320px] p-4',
              'rounded-xl border border-border/50 bg-card',
              'hover:border-primary/40 hover:bg-secondary/50',
              'transition-colors text-left',
            )}
          >
            <div className="flex items-center gap-2 mb-2">
              <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center text-[18px]">
                {item.icon ?? '🤖'}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className="text-[14px] font-semibold truncate">{item.i18nName ?? item.name}</span>
                  <AppTypeBadge type={item.appType} />
                </div>
                <span className="text-[11px] text-muted-foreground">by {item.author}</span>
              </div>
            </div>
            <p className="text-[12px] text-muted-foreground line-clamp-2">
              {item.i18nDescription ?? item.description}
            </p>
          </button>
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: StoreView.tsx (orchestrator)**

```tsx
import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { StoreHeader } from './StoreHeader'
import { StoreFeaturedRow } from './StoreFeaturedRow'
import { StoreGrid } from './StoreGrid'
import {
  marketplaceItemsAtom,
  marketplacePageAtom,
  marketplaceHasMoreAtom,
  marketplaceTotalAtom,
  marketplaceLoadingAtom,
  marketplaceLoadErrorAtom,
  marketplaceFiltersAtom,
  marketplaceCategoryCountsAtom,
  marketplaceUpdatesAtom,
} from '@/atoms/marketplace'
import {
  queryMarketplace,
  refreshMarketplace,
  checkMarketplaceUpdates,
} from '@/lib/tauri-bridge'

const PAGE_SIZE = 20

export function StoreView(): React.ReactElement {
  const [items, setItems] = useAtom(marketplaceItemsAtom)
  const [page, setPage] = useAtom(marketplacePageAtom)
  const setHasMore = useSetAtom(marketplaceHasMoreAtom)
  const setTotal = useSetAtom(marketplaceTotalAtom)
  const [loading, setLoading] = useAtom(marketplaceLoadingAtom)
  const setLoadError = useSetAtom(marketplaceLoadErrorAtom)
  const filters = useAtomValue(marketplaceFiltersAtom)
  const setCounts = useSetAtom(marketplaceCategoryCountsAtom)
  const setUpdates = useSetAtom(marketplaceUpdatesAtom)

  const loadPage = React.useCallback(
    async (pageNum: number, replace: boolean) => {
      setLoading(true)
      setLoadError(null)
      try {
        const result = await queryMarketplace(
          filters.search || undefined,
          filters.itemType === 'all' ? undefined : filters.itemType,
          filters.category ?? undefined,
          pageNum,
          PAGE_SIZE,
        )
        setItems((prev) => (replace ? result.items : [...prev, ...result.items]))
        setHasMore(result.hasMore)
        setTotal(result.total)
        setPage(pageNum)
        // Update category counts after every successful query so chips reflect the type filter
        const cats: Record<string, number> = {}
        for (const it of result.items) {
          cats[it.category] = (cats[it.category] ?? 0) + 1
        }
        // Counts here are page-local; for the overall count we'd need another query.
        // Phase 3a uses local-page counts for simplicity — close enough until
        // Phase 3b adds a real /category-counts endpoint.
        setCounts((prev) => ({ ...prev, ...cats }))
      } catch (err) {
        setLoadError(String(err))
      } finally {
        setLoading(false)
      }
    },
    [filters, setItems, setHasMore, setTotal, setPage, setLoading, setLoadError, setCounts],
  )

  // Reload when filters change (debounced search via StoreHeader)
  React.useEffect(() => {
    void loadPage(0, true)
  }, [filters.search, filters.itemType, filters.category, loadPage])

  // Initial updates check
  React.useEffect(() => {
    checkMarketplaceUpdates()
      .then(setUpdates)
      .catch((err) => console.warn('[StoreView] check updates failed:', err))
  }, [setUpdates])

  const handleLoadMore = () => {
    if (!loading) void loadPage(page + 1, false)
  }

  const handleRefresh = async () => {
    try {
      const count = await refreshMarketplace()
      toast.success(`已刷新，${count} 个项目`)
      void loadPage(0, true)
    } catch (err) {
      toast.error(`刷新失败：${String(err)}`)
    }
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <StoreHeader onRefresh={handleRefresh} />
      <div className="flex-1 overflow-y-auto">
        <StoreFeaturedRow />
        <StoreGrid onLoadMore={handleLoadMore} />
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Verify build**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
```

Expected: 0.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/StoreFeaturedRow.tsx ui/src/components/automation/StoreView.tsx
git commit -m "feat(marketplace): StoreFeaturedRow (hero band) + StoreView orchestrator

StoreFeaturedRow:
  - Hard-coded 4 slugs: ai-daily-news / github-pr-reviewer / weibo-hot-tracker /
    wechat-article-monitor (Phase 4 makes this remote-driven)
  - Horizontal scroll, 320px-wide cards
  - 'Sparkles + 今日推荐' uppercase tracking-wider header (uClaw section-label style)
  - Hidden when no featured slugs match (graceful)

StoreView:
  - Composes Header + FeaturedRow + Grid
  - Owns the load lifecycle: filters changes → loadPage(0, true); Load More → page+1 append
  - Refresh button → refreshMarketplace() + reload + toast
  - On mount checks updates via checkMarketplaceUpdates → atom for grid badges
  - 20 items per page

§ 13.3.C (Featured row above search)."
```

---

## Task 11: StoreDetail with 4 sub-tabs

**Files:**
- Create: `ui/src/components/automation/StoreDetail.tsx`

This is the heaviest UI commit. ~280 LOC.

- [ ] **Step 1: Create StoreDetail.tsx**

```tsx
import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { ArrowLeft, Download, Sparkles, Loader2, AlertTriangle } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import {
  marketplaceSelectedSlugAtom,
  marketplaceDetailAtom,
  marketplaceDetailLoadingAtom,
  marketplaceDetailSubtabAtom,
  automationsSubviewAtom,
  installWizardAtom,
  type DetailSubTab,
} from '@/atoms/marketplace'
import { getMarketplaceDetail } from '@/lib/tauri-bridge'

const TABS: { id: DetailSubTab; label: string }[] = [
  { id: 'overview', label: '概览' },
  { id: 'config', label: '配置' },
  { id: 'requires', label: '依赖' },
  { id: 'prompt', label: '提示词' },
]

export function StoreDetail(): React.ReactElement {
  const slug = useAtomValue(marketplaceSelectedSlugAtom)
  const [detail, setDetail] = useAtom(marketplaceDetailAtom)
  const [loading, setLoading] = useAtom(marketplaceDetailLoadingAtom)
  const [activeTab, setActiveTab] = useAtom(marketplaceDetailSubtabAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)
  const setWizard = useSetAtom(installWizardAtom)
  const [promptExpanded, setPromptExpanded] = React.useState(false)

  // Load detail when slug changes
  React.useEffect(() => {
    if (!slug) return
    setLoading(true)
    setActiveTab('overview')
    getMarketplaceDetail(slug)
      .then(setDetail)
      .catch((err) => {
        toast.error(`加载详情失败：${String(err)}`)
        setSubview('store')
      })
      .finally(() => setLoading(false))
  }, [slug, setDetail, setLoading, setActiveTab, setSubview])

  // Esc returns to store grid
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSubview('store')
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [setSubview])

  if (loading || !detail) {
    return (
      <div className="flex items-center gap-2 justify-center py-16 text-muted-foreground">
        <Loader2 size={14} className="animate-spin" />
        <span className="text-[13px]">正在加载详情...</span>
      </div>
    )
  }

  const { item, parsedSpecJson, requiresMcps, requiresSkills, installedVersion, specYaml } = detail
  const isInstalled = installedVersion !== null
  const hasUpdate = isInstalled && installedVersion !== item.version

  const openInstallWizard = () => {
    setWizard({
      step: 'scope',
      slug: item.slug,
      spaceId: null,
      userConfig: {},
      progress: null,
      error: null,
    })
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Sticky header */}
      <div className="sticky top-0 z-10 backdrop-blur-md bg-content-area/95 border-b border-border/50">
        <div className="flex items-center gap-3 px-6 py-3">
          <button
            type="button"
            onClick={() => setSubview('store')}
            className="text-muted-foreground hover:text-foreground transition-colors"
            title="返回市场 (Esc)"
          >
            <ArrowLeft size={16} />
          </button>
          <div className="w-9 h-9 rounded-md bg-primary/10 flex items-center justify-center text-[14px]">
            {item.icon ?? '🤖'}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-[15px] font-semibold truncate">{item.i18nName ?? item.name}</span>
              <AppTypeBadge type={item.appType} />
              <span className="text-[11px] text-muted-foreground tabular-nums">v{item.version}</span>
              {hasUpdate && (
                <span className="px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
                  当前 v{installedVersion} · 可更新
                </span>
              )}
            </div>
            <span className="text-[11px] text-muted-foreground">by {item.author} · {item.category}</span>
          </div>
          {item.appType === 'automation' ? (
            <button
              type="button"
              onClick={openInstallWizard}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium',
                'bg-primary text-primary-foreground hover:bg-primary/90 transition-colors',
              )}
            >
              <Download size={12} />
              {isInstalled && !hasUpdate ? '重新安装' : hasUpdate ? '更新到 v' + item.version : '安装'}
            </button>
          ) : (
            <span className="text-[11px] text-muted-foreground italic">
              {item.appType.toUpperCase()} 安装在 Phase 3b 开放
            </span>
          )}
        </div>
        {/* Sub-tab strip */}
        <div className="flex items-center gap-1 px-6 pb-2">
          {TABS.map((tab) => {
            const active = activeTab === tab.id
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  'relative px-3 py-1 text-[12px] rounded-md transition-colors',
                  active
                    ? 'bg-muted text-foreground font-medium'
                    : 'text-muted-foreground hover:text-foreground hover:bg-accent/30',
                )}
              >
                {active && <span className="absolute left-0 top-1.5 bottom-1.5 w-[2px] bg-primary rounded-r" />}
                {tab.label}
              </button>
            )
          })}
        </div>
      </div>

      {/* Sub-tab content (fade transitions) */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        <AnimatePresence mode="wait">
          <motion.div
            key={activeTab}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="max-w-3xl"
          >
            {activeTab === 'overview' && (
              <div className="space-y-4">
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">描述</h3>
                  <p className="text-[13px] text-foreground/90 leading-relaxed">{item.i18nDescription ?? item.description}</p>
                </section>
                {item.tags.length > 0 && (
                  <section>
                    <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">标签</h3>
                    <div className="flex flex-wrap gap-1.5">
                      {item.tags.map((tag) => (
                        <span key={tag} className="text-[11px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </section>
                )}
                <section className="grid grid-cols-2 gap-x-6 gap-y-2 text-[12px]">
                  <Row label="作者" value={item.author} />
                  <Row label="版本" value={`v${item.version}`} />
                  <Row label="分类" value={item.category} />
                  <Row label="语言" value={item.locale ?? '未指定'} />
                  {item.minAppVersion && <Row label="最低 uClaw 版本" value={item.minAppVersion} />}
                </section>
              </div>
            )}

            {activeTab === 'config' && (
              <div>
                <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">配置项预览</h3>
                <ConfigSchemaPreview parsedSpecJson={parsedSpecJson} />
              </div>
            )}

            {activeTab === 'requires' && (
              <div className="space-y-4">
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
                    MCP 服务 ({requiresMcps.length})
                  </h3>
                  {requiresMcps.length === 0 ? (
                    <p className="text-[12px] text-muted-foreground italic">无</p>
                  ) : (
                    <ul className="space-y-1 text-[12px]">
                      {requiresMcps.map((m) => (
                        <li key={m} className="px-3 py-2 rounded-md bg-card border border-border/50">
                          {m}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
                    依赖技能 ({requiresSkills.length})
                  </h3>
                  {requiresSkills.length === 0 ? (
                    <p className="text-[12px] text-muted-foreground italic">无</p>
                  ) : (
                    <ul className="space-y-1 text-[12px]">
                      {requiresSkills.map((s) => (
                        <li key={s} className="px-3 py-2 rounded-md bg-card border border-border/50">
                          {s}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              </div>
            )}

            {activeTab === 'prompt' && (
              <div>
                <div className="flex items-center justify-between mb-2">
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">系统提示词</h3>
                  <button
                    type="button"
                    onClick={() => setPromptExpanded((v) => !v)}
                    className="text-[11px] text-primary hover:underline"
                  >
                    {promptExpanded ? '折叠' : '展开'}
                  </button>
                </div>
                <pre className={cn(
                  'text-[11px] font-mono text-foreground/80 whitespace-pre-wrap',
                  'px-3 py-2 rounded-md bg-card border border-border/50',
                  !promptExpanded && 'max-h-[200px] overflow-hidden relative',
                )}>
                  {parsedSpecJson && typeof parsedSpecJson === 'object' && parsedSpecJson !== null && 'system_prompt' in parsedSpecJson
                    ? String((parsedSpecJson as Record<string, unknown>).system_prompt)
                    : specYaml.slice(0, 2000) + (specYaml.length > 2000 ? '\n...' : '')}
                </pre>
              </div>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 py-1 border-b border-border/30">
      <span className="text-muted-foreground shrink-0">{label}</span>
      <span className="text-foreground/80 truncate text-right">{value}</span>
    </div>
  )
}

function ConfigSchemaPreview({ parsedSpecJson }: { parsedSpecJson: unknown | null }) {
  if (!parsedSpecJson) {
    return (
      <div className="flex items-start gap-2 p-3 rounded-md bg-warning-bg text-warning text-[12px]">
        <AlertTriangle size={14} className="mt-0.5 shrink-0" />
        <span>spec.yaml 解析失败，配置预览不可用。安装时会回退到 raw YAML 编辑模式。</span>
      </div>
    )
  }
  const obj = parsedSpecJson as Record<string, unknown>
  const schema = obj.config_schema
  if (!Array.isArray(schema) || schema.length === 0) {
    return <p className="text-[12px] text-muted-foreground italic">此数字员工无可配置项</p>
  }
  return (
    <ul className="space-y-2">
      {(schema as Array<Record<string, unknown>>).map((item, idx) => (
        <li key={idx} className="px-3 py-2 rounded-md bg-card border border-border/50 text-[12px]">
          <div className="flex items-center gap-2 mb-1">
            <span className="font-medium">{String(item.label ?? item.key)}</span>
            <span className="text-[10px] text-muted-foreground px-1.5 py-[1px] rounded bg-muted">
              {String(item.type ?? 'unknown')}
            </span>
            {item.required ? (
              <span className="text-[10px] text-danger">必填</span>
            ) : (
              <span className="text-[10px] text-muted-foreground/70">可选</span>
            )}
          </div>
          {typeof item.description === 'string' && item.description && (
            <p className="text-[11px] text-muted-foreground">{item.description}</p>
          )}
          {item.default != null && (
            <p className="text-[11px] text-muted-foreground mt-1">
              默认: <span className="font-mono">{String(item.default)}</span>
            </p>
          )}
        </li>
      ))}
    </ul>
  )
}
```

- [ ] **Step 2: Verify build**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
```

Expected: 0.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/automation/StoreDetail.tsx
git commit -m "feat(marketplace): StoreDetail with sticky CTA + 4 sub-tabs

8-section hello-halo detail page replaced with uClaw's 4-sub-tab pattern (§ 13.3.G):

  概览 (overview)  — description + tags + metadata grid (作者/版本/分类/语言/最低版本)
  配置 (config)    — config_schema preview as 3-row cards (label/type/desc/default/required)
                     Graceful 'spec parse failed' fallback when parsed_spec_json is null.
  依赖 (requires)  — MCP services + skills lists, '无' italic placeholder
  提示词 (prompt)  — system_prompt in pre/font-mono, expand/collapse toggle

Sticky header (§ 13.3.F):
  - backdrop-blur-md bg-content-area/95
  - ← back button, 9x9 icon bubble, name + AppTypeBadge + version
  - Install CTA stays visible across all sub-tabs
  - When isInstalled: '重新安装' or '更新到 v{X}'
  - For non-automation types: italic '{TYPE} 安装在 Phase 3b 开放'

Esc closes the detail view back to the store grid.

motion/react AnimatePresence sub-tab transitions with 0.18s, ease
[0.32, 0.72, 0, 1] — uClaw signature timing."
```

---

## Task 12: InstallWizard — 3-step flow

**Files:**
- Create: `ui/src/components/automation/InstallWizard.tsx`
- Modify: `ui/src/components/automation/StoreDetail.tsx` — render the wizard
- Modify: `ui/src/components/automation/StoreView.tsx` — render the wizard

- [ ] **Step 1: Create InstallWizard.tsx**

```tsx
import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Loader2, Check, AlertCircle, X } from 'lucide-react'
import { toast } from 'sonner'
import { listen } from '@tauri-apps/api/event'
import { cn } from '@/lib/utils'
import { installWizardAtom, marketplaceDetailAtom, humaneSpecsAtom } from '@/atoms/marketplace'
import { humaneSpecsAtom as installedSpecsAtom } from '@/atoms/automation'
import { installMarketplaceHuman } from '@/lib/tauri-bridge'
import type { MarketplaceInstallProgress } from '@/lib/tauri-bridge'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'

const STEPS = ['scope', 'config', 'confirm', 'progress'] as const

export function InstallWizard(): React.ReactElement | null {
  const [state, setState] = useAtom(installWizardAtom)
  const detail = useAtomValue(marketplaceDetailAtom)
  const setSpecs = useSetAtom(installedSpecsAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)

  // Default selected space to current active workspace
  React.useEffect(() => {
    if (state.step === 'scope' && state.spaceId === null && activeWorkspaceId) {
      setState((s) => ({ ...s, spaceId: activeWorkspaceId }))
    }
  }, [state.step, state.spaceId, activeWorkspaceId, setState])

  // Subscribe to install progress events when in progress step
  React.useEffect(() => {
    if (state.step !== 'progress' || !state.slug) return
    const channel = `install_progress_${state.slug}`
    let unlisten: (() => void) | undefined
    listen<MarketplaceInstallProgress>(channel, (event) => {
      setState((s) => ({ ...s, progress: event.payload }))
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [state.step, state.slug, setState])

  if (state.step === null) return null

  const close = () => setState({ step: null, slug: null, spaceId: null, userConfig: {}, progress: null, error: null })

  const submit = async () => {
    if (!state.slug || !state.spaceId) return
    setState((s) => ({ ...s, step: 'progress', progress: { phase: 'fetching_spec', percent: 5 } }))
    try {
      const row = await installMarketplaceHuman(
        state.slug,
        state.spaceId,
        state.userConfig,
        `install_progress_${state.slug}`,
      )
      setSpecs((prev) => [row, ...prev.filter((s) => s.id !== row.id)])
      toast.success(`已安装 ${row.name}`)
      setTimeout(close, 800)
    } catch (err) {
      setState((s) => ({ ...s, error: String(err) }))
    }
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center p-6 bg-foreground/10 backdrop-blur-sm">
      <motion.div
        initial={{ opacity: 0, scale: 0.992 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.992 }}
        transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
        className="w-full max-w-xl bg-content-area rounded-xl shadow-2xl border border-border/50 overflow-hidden"
      >
        {/* Header with progress dots + close */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border/50">
          <div className="flex items-center gap-2">
            {(['scope', 'config', 'confirm'] as const).map((step, i) => {
              const stepIdx = STEPS.indexOf(state.step ?? 'scope')
              const completed = i < stepIdx
              const current = i === stepIdx
              return (
                <div key={step} className="flex items-center gap-2">
                  <div className={cn(
                    'w-2 h-2 rounded-full transition-colors',
                    current ? 'bg-primary' : completed ? 'bg-primary/40' : 'bg-muted',
                  )} />
                  {i < 2 && <div className={cn('w-4 h-px', i < stepIdx ? 'bg-primary/40' : 'bg-muted')} />}
                </div>
              )
            })}
            <span className="ml-2 text-[12px] text-muted-foreground">
              {state.step === 'scope' && '选择空间 (1/3)'}
              {state.step === 'config' && '填写配置 (2/3)'}
              {state.step === 'confirm' && '确认安装 (3/3)'}
              {state.step === 'progress' && '安装中...'}
            </span>
          </div>
          <button onClick={close} className="text-muted-foreground hover:text-foreground" title="关闭 (Esc)">
            <X size={14} />
          </button>
        </div>

        <AnimatePresence mode="wait">
          <motion.div
            key={state.step}
            initial={{ opacity: 0, x: 8 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -8 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="p-5 min-h-[200px]"
          >
            {state.step === 'scope' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">选择安装到哪个工作区</h3>
                <div className="space-y-1">
                  {workspaces.map((ws) => (
                    <button
                      key={ws.id}
                      type="button"
                      onClick={() => setState((s) => ({ ...s, spaceId: ws.id }))}
                      className={cn(
                        'w-full flex items-center justify-between px-3 py-2 rounded-md text-[13px] transition-colors',
                        state.spaceId === ws.id
                          ? 'bg-primary/10 text-foreground border border-primary/30'
                          : 'border border-border/50 hover:bg-accent/30',
                      )}
                    >
                      <span className="truncate">{ws.name}</span>
                      {state.spaceId === ws.id && <Check size={14} className="text-primary" />}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {state.step === 'config' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">填写运行参数</h3>
                <ConfigForm
                  parsedSpecJson={detail?.parsedSpecJson}
                  values={state.userConfig}
                  onChange={(v) => setState((s) => ({ ...s, userConfig: v }))}
                />
              </div>
            )}

            {state.step === 'confirm' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">确认安装</h3>
                <dl className="text-[12px] space-y-2">
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">数字员工</dt>
                    <dd>{detail?.item.name}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">版本</dt>
                    <dd>v{detail?.item.version}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">工作区</dt>
                    <dd>{workspaces.find((w) => w.id === state.spaceId)?.name ?? '?'}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">配置项</dt>
                    <dd>{Object.keys(state.userConfig).length} 项</dd>
                  </div>
                </dl>
                {state.error && (
                  <div className="flex items-start gap-2 mt-3 p-2 rounded-md bg-danger-bg text-danger text-[11px]">
                    <AlertCircle size={12} className="mt-0.5" />
                    <span>{state.error}</span>
                  </div>
                )}
              </div>
            )}

            {state.step === 'progress' && (
              <div className="flex flex-col items-center gap-3 py-6">
                <Loader2 size={20} className="animate-spin text-primary" />
                <div className="text-[13px]">{state.progress?.message ?? '处理中...'}</div>
                <div className="w-full max-w-xs bg-muted rounded-full overflow-hidden h-1">
                  <div
                    className="bg-primary h-full transition-all"
                    style={{ width: `${state.progress?.percent ?? 0}%` }}
                  />
                </div>
                <div className="text-[10px] text-muted-foreground tabular-nums">
                  {state.progress?.percent ?? 0}% · {state.progress?.phase ?? ''}
                </div>
              </div>
            )}
          </motion.div>
        </AnimatePresence>

        {/* Footer */}
        {state.step !== 'progress' && (
          <div className="flex items-center justify-between px-5 py-3 border-t border-border/50 bg-card/30">
            <button
              type="button"
              onClick={() => {
                if (state.step === 'scope') close()
                else if (state.step === 'config') setState((s) => ({ ...s, step: 'scope' }))
                else if (state.step === 'confirm') setState((s) => ({ ...s, step: 'config' }))
              }}
              className="text-[12px] text-muted-foreground hover:text-foreground transition-colors"
            >
              {state.step === 'scope' ? '取消' : '← 返回'}
            </button>
            <button
              type="button"
              onClick={() => {
                if (state.step === 'scope') setState((s) => ({ ...s, step: 'config' }))
                else if (state.step === 'config') setState((s) => ({ ...s, step: 'confirm' }))
                else if (state.step === 'confirm') void submit()
              }}
              disabled={state.step === 'scope' && !state.spaceId}
              className={cn(
                'px-3 py-1.5 text-[12px] rounded-md font-medium transition-colors',
                'bg-primary text-primary-foreground hover:bg-primary/90',
                'disabled:opacity-50 disabled:cursor-not-allowed',
              )}
            >
              {state.step === 'confirm' ? '安装' : '继续 →'}
            </button>
          </div>
        )}
      </motion.div>
    </div>
  )
}

interface ConfigFormProps {
  parsedSpecJson: unknown | null
  values: Record<string, unknown>
  onChange: (v: Record<string, unknown>) => void
}

function ConfigForm({ parsedSpecJson, values, onChange }: ConfigFormProps): React.ReactElement {
  if (!parsedSpecJson || typeof parsedSpecJson !== 'object') {
    return (
      <p className="text-[12px] text-muted-foreground italic">
        spec 解析失败，将以默认配置安装。
      </p>
    )
  }
  const schema = (parsedSpecJson as Record<string, unknown>).config_schema
  if (!Array.isArray(schema) || schema.length === 0) {
    return (
      <p className="text-[12px] text-muted-foreground italic">
        此数字员工无可配置项，可直接进入下一步。
      </p>
    )
  }

  const setField = (key: string, v: unknown) => onChange({ ...values, [key]: v })

  return (
    <div className="space-y-3 max-h-[300px] overflow-y-auto">
      {(schema as Array<Record<string, unknown>>).map((field, idx) => {
        const key = String(field.key ?? `field-${idx}`)
        const label = String(field.label ?? key)
        const type = String(field.type ?? 'text')
        const required = field.required === true
        const placeholder = typeof field.placeholder === 'string' ? field.placeholder : undefined
        const description = typeof field.description === 'string' ? field.description : undefined
        const current = values[key] ?? field.default ?? ''
        return (
          <div key={key}>
            <label className="block text-[12px] font-medium mb-1">
              {label}
              {required && <span className="text-danger ml-0.5">*</span>}
            </label>
            {description && <p className="text-[11px] text-muted-foreground mb-1">{description}</p>}
            {type === 'boolean' ? (
              <input
                type="checkbox"
                checked={!!current}
                onChange={(e) => setField(key, e.target.checked)}
              />
            ) : type === 'number' ? (
              <input
                type="number"
                value={String(current)}
                onChange={(e) => setField(key, Number(e.target.value))}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            ) : type === 'select' && Array.isArray(field.options) ? (
              <select
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              >
                {(field.options as Array<unknown>).map((opt, i) => {
                  const o = opt as Record<string, unknown>
                  return (
                    <option key={i} value={String(o.value ?? o)}>
                      {String(o.label ?? o.value ?? o)}
                    </option>
                  )
                })}
              </select>
            ) : type === 'text' ? (
              <textarea
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[60px]"
              />
            ) : (
              <input
                type="text"
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            )}
          </div>
        )
      })}
    </div>
  )
}
```

- [ ] **Step 2: Mount the wizard in StoreDetail.tsx**

In `StoreDetail.tsx`, just before the closing `</div>` of the outermost container, add:

```tsx
      <InstallWizard />
```

And add the import at the top:

```tsx
import { InstallWizard } from './InstallWizard'
```

- [ ] **Step 3: Verify build**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Address any errors (most likely missing exports from workspace atoms — verify the actual atom names with grep).

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/InstallWizard.tsx ui/src/components/automation/StoreDetail.tsx
git commit -m "feat(marketplace): 3-step InstallWizard with config form + progress channel

§ 13.3.A — three sequential steps replace hello-halo's single crammed dialog:

  1. scope    — workspace selector (defaults to active workspace)
  2. config   — dynamic form per config_schema (boolean/number/select/text/string)
  3. confirm  — review summary
  → progress  — Tauri event channel install_progress_{slug} streams phase/percent

Progress dots at top show flow position (filled / current / upcoming). Footer
buttons: 取消 ↔ ← 返回 / 继续 → / 安装. Esc closes via the same X button.

ConfigForm renders InputDef types:
  - boolean → <input type=checkbox>
  - number  → <input type=number>
  - select  → <select> with options (handles both shapes)
  - text    → <textarea>
  - string/url/email/unknown → <input type=text>
Graceful fallback: spec parse failed → '将以默认配置安装' info text.

Mounted inside StoreDetail (which itself lives inside the AutomationsView
sub-view) — uses position:absolute inset-0 within that container so it
doesn't break the MainArea panel.

Motion: 0.22s [0.32, 0.72, 0, 1] entry, 0.18s step transitions."
```

---

## Task 13: AutomationsView — 3-sub-view container

**Files:**
- Create: `ui/src/views/AutomationsView.tsx`
- Create: `ui/src/components/automation/AppsTab.tsx`
- Modify: `ui/src/components/tabs/MainArea.tsx`

- [ ] **Step 1: AppsTab.tsx (Phase 3b placeholder)**

```tsx
import * as React from 'react'
import { PackageOpen } from 'lucide-react'

export function AppsTab(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center h-full text-muted-foreground p-8">
      <PackageOpen size={32} className="text-muted-foreground/30 mb-3" />
      <p className="text-[14px] font-medium mb-1">我的应用</p>
      <p className="text-[12px] text-muted-foreground max-w-md text-center">
        MCP 服务 / 复用技能 / 扩展程序的管理界面将在 Phase 3b 开放，配合多注册表支持一起发布。
      </p>
    </div>
  )
}
```

- [ ] **Step 2: AutomationsView.tsx**

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { cn } from '@/lib/utils'
import { AutomationHub } from '@/components/automation/AutomationHub'
import { AppsTab } from '@/components/automation/AppsTab'
import { StoreView } from '@/components/automation/StoreView'
import { StoreDetail } from '@/components/automation/StoreDetail'
import { automationsSubviewAtom } from '@/atoms/marketplace'

const TABS: { id: 'humans' | 'apps' | 'store'; label: string }[] = [
  { id: 'humans', label: '我的数字人' },
  { id: 'apps', label: '我的应用' },
  { id: 'store', label: '应用商店' },
]

export function AutomationsView(): React.ReactElement {
  const [subview, setSubview] = useAtom(automationsSubviewAtom)

  // Normalise 'store-detail' to 'store' for tab highlighting
  const activeTab = subview === 'store-detail' ? 'store' : subview

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Top tab strip */}
      <div className="flex items-center gap-1 px-6 py-2 border-b border-border/50 flex-shrink-0">
        {TABS.map((tab) => {
          const active = activeTab === tab.id
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setSubview(tab.id)}
              className={cn(
                'relative px-3 py-1.5 text-[13px] rounded-md transition-colors',
                active
                  ? 'bg-muted text-foreground font-medium'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent/30',
              )}
            >
              {active && <span className="absolute left-0 top-2 bottom-2 w-[2px] bg-primary rounded-r" />}
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Sub-view body */}
      <div className="flex-1 min-h-0 relative">
        <AnimatePresence mode="wait">
          <motion.div
            key={subview}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="absolute inset-0"
          >
            {subview === 'humans' && <AutomationHub />}
            {subview === 'apps' && <AppsTab />}
            {subview === 'store' && <StoreView />}
            {subview === 'store-detail' && <StoreDetail />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Replace AutomationHub mount in MainArea.tsx**

Find:

```typescript
{automationOpen ? (
  automationBody
) : previewOpen ? (
```

Replace the entire `automationBody` block above it with a single import + use:

```typescript
import { AutomationsView } from '@/views/AutomationsView'
// ...
{automationOpen ? (
  <AutomationsView />
) : previewOpen ? (
```

Delete the old `automationBody` `<>...</>` block — it's replaced.

Also delete the old Esc handler that closed the automation panel — AutomationsView's StoreDetail has its own Esc back-to-grid; closing the entire view is now done via the LeftSidebar button (it toggles `automationPanelOpenAtom`).

- [ ] **Step 4: Build + smoke**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: 0 tsc errors, existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/views/AutomationsView.tsx ui/src/components/automation/AppsTab.tsx ui/src/components/tabs/MainArea.tsx
git commit -m "feat(marketplace): AutomationsView 3-sub-view shell — 我的数字人 / 我的应用 / 应用商店

§ 13.2 — uClaw architecture diff vs hello-halo: NOT a top-level AppsPage
with split panes. Instead, AutomationsView replaces the direct AutomationHub
render in MainArea, owning a top tab strip + 3-sub-view body:

  humans       → AutomationHub (Phase 1 component unchanged)
  apps         → AppsTab (Phase 3b placeholder — empty state explainer)
  store        → StoreView (browse grid + featured + filters)
  store-detail → StoreDetail (with InstallWizard mounted inside)

Tab strip uses SettingsNav 2-px left-bar active indicator (§ 13.5). Sub-view
transitions use AnimatePresence + motion/react opacity fade, 0.18s
[0.32, 0.72, 0, 1] — uClaw signature timing.

automationPanelOpenAtom continues to gate the whole view (LeftSidebar
button toggles it). MainArea swaps to AutomationsView when atom is true.

The old 'automationBody' inline render in MainArea is removed."
```

---

## Task 14: Pet celebration + cleanup

**Files:**
- Modify: `ui/src/hooks/usePetStateSync.ts` — listen for `chat:pet-celebrate`
- Delete: `ui/src/components/automation/MarketplaceModal.tsx`
- Delete: `ui/src/components/automation/MarketplaceCard.tsx`
- Delete: `ui/src/components/automation/MarketplaceModal.test.tsx`
- Delete: `ui/src/components/automation/MarketplaceCard.test.tsx`
- Modify: `ui/src/components/automation/AutomationHub.tsx` — remove imports of deleted MarketplaceModal

- [ ] **Step 1: Wire chat:pet-celebrate into PetStateSync**

In `ui/src/hooks/usePetStateSync.ts`, find the existing `register` calls (around line 47). Add:

```typescript
    register('chat:pet-celebrate', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('success')
      successTimer.current = setTimeout(() => {
        setPrimary('idle')
        successTimer.current = null
      }, SUCCESS_LINGER_MS)
    })
```

This piggybacks on the existing success animation — Pet shows the 4-second celebration any time the marketplace install fires the event. § 13.3.E.

- [ ] **Step 2: Remove the old marketplace components**

```bash
rm ui/src/components/automation/MarketplaceModal.tsx
rm ui/src/components/automation/MarketplaceCard.tsx
rm ui/src/components/automation/MarketplaceModal.test.tsx
rm ui/src/components/automation/MarketplaceCard.test.tsx
```

- [ ] **Step 3: Remove references in AutomationHub.tsx**

Find any `import { MarketplaceModal } from './MarketplaceModal'` lines and delete them. Also find any `MarketplaceModal` rendering blocks and delete those — AutomationHub is now just the "我的数字人" content (installed list), no longer the marketplace entry point. The "+ 浏览数字人市场" button can be repointed to set `automationsSubviewAtom = 'store'`.

Search:

```bash
grep -n "MarketplaceModal\|marketplaceOpen\|浏览数字人市场" ui/src/components/automation/AutomationHub.tsx
```

If `setMarketplaceOpen(true)` is called from a button — replace with:

```typescript
import { useSetAtom } from 'jotai'
import { automationsSubviewAtom } from '@/atoms/marketplace'

// inside the component:
const setSubview = useSetAtom(automationsSubviewAtom)

// the button's onClick:
onClick={() => setSubview('store')}
```

And remove the `<MarketplaceModal />` render entirely.

- [ ] **Step 4: Build + test**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -c "error TS"
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: 0 tsc, all tests pass (-2 deleted Marketplace tests, expected new total = previous - 2 + new from this PR).

- [ ] **Step 5: Commit**

```bash
git add ui/src/hooks/usePetStateSync.ts ui/src/components/automation/AutomationHub.tsx
git rm ui/src/components/automation/MarketplaceModal.tsx \
       ui/src/components/automation/MarketplaceCard.tsx \
       ui/src/components/automation/MarketplaceModal.test.tsx \
       ui/src/components/automation/MarketplaceCard.test.tsx
git commit -m "chore(marketplace): wire chat:pet-celebrate + delete Phase 3a-mini components

PetStateSync gains a new 'chat:pet-celebrate' event listener — fires the
4-second celebration animation whenever the backend's install_marketplace_human
succeeds. § 13.3.E (Pet awareness).

Removes the Phase 3a-mini scaffolding (now superseded by the StoreView /
StoreGrid / StoreCard set):

  - MarketplaceModal.tsx (replaced by StoreView)
  - MarketplaceCard.tsx (replaced by StoreCard)
  - both .test.tsx files (coverage moved to StoreCard.test.tsx etc.)

AutomationHub's '+ 浏览数字人市场' button now sets automationsSubviewAtom
= 'store' instead of opening the local modal. Cleaner state model:
single atom controls the user's location in the AutomationsView tree."
```

---

## Self-review checklist

Run through the following before declaring Phase 3a done:

- [ ] All 14 commits compile + green tests in main
- [ ] `cd ui && npx tsc --noEmit` = 0 errors
- [ ] `cd src-tauri && cargo build 2>&1 | grep ^error` = empty
- [ ] `cd ui && npm test -- --run` = all pass
- [ ] No hardcoded `bg-zinc-X` / `text-gray-X` / `text-green-500` in any new file (`grep -rE "bg-(zinc|gray|slate|stone|neutral|green|red|amber)-[0-9]" ui/src/components/automation/ ui/src/views/`)
- [ ] All new cards use `rounded-xl`, not `rounded-lg`
- [ ] All motion uses `motion/react` (Framer), not pure CSS transitions for state changes
- [ ] Theme switch test passes: open settings → switch to warm-paper → check marketplace renders correctly → switch to qingye → check → switch back
- [ ] Manual end-to-end: open LeftSidebar 🤖 → 应用商店 tab → see featured + grid + filters → click ai-daily-news card → see detail page with 4 sub-tabs → click 安装 → wizard opens with workspace selector → fill config → confirm → progress bar → success toast → pet celebrates → installed spec appears in 我的数字人 tab

## Spec-coverage check

Mapping each major feature to its commit:

| Feature | Commit |
|---|---|
| V23a marketplace cache schema | Task 1 |
| Cache sync + query | Task 2 |
| MarketplaceQueryResult / Detail / Update types | Task 3 |
| 4 new Tauri commands + install upgrade | Task 4 |
| Frontend atom slice | Task 5 |
| TS bindings | Task 6 |
| AppTypeBadge | Task 7 |
| StoreHeader (search + tabs + chips + counts) | Task 8 |
| StoreCard + StoreGrid (with installed/update badges, WelcomeView-tone empty states) | Task 9 |
| StoreFeaturedRow + StoreView orchestrator | Task 10 |
| StoreDetail (sticky CTA + 4 sub-tabs) | Task 11 |
| 3-step InstallWizard | Task 12 |
| AutomationsView (3-sub-view shell) | Task 13 |
| Pet celebration + delete Phase 3a-mini components | Task 14 |

Out of scope for v1 (Phase 3b / 4 per spec § 12 + § 13.4):
- Multi-registry management UI
- Try-install sandbox (§ 13.3.B) — design preserved in spec but implementation deferred; the spec doc notes the sandbox design will land in Phase 3b alongside multi-registry
- Auto-uninstall sandbox timer
- Featured row as remote config
- In-card dependency tooltip
- Compare mode
- CJK-aware FTS

The try-install sandbox (§ 13.3.B) was one of the proposed innovations but the implementation requires ephemeral workspace creation + cleanup wiring that touches Phase 1's workspace lifecycle — adding it here doubles the PR size. Deferred with a TODO in `2026-05-14-marketplace-ui-port-design.md § 13.4`.
