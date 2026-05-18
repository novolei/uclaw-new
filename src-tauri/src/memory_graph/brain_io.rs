//! Disk-mirror IO for EntityPages — Memory OS Foundation Phase 7.
//!
//! ## What this module does
//!
//! Reads and writes one file per EntityPage at
//! `<brain_root>/<subkind>/<slug>.md` with a YAML frontmatter block
//! followed by the markdown body (the `compiled_truth`). Tracks each
//! file's last-synced state in the `brain_sync_state` table (V37) so
//! the sync engine (Phase 7.2) can detect:
//!
//! - DB-moved-since-last-sync (active version id differs from
//!   `brain_sync_state.last_synced_version_id`)
//! - disk-moved-since-last-sync (file mtime > `file_mtime_at_last_sync_ms`
//!   AND SHA-256 differs from `last_synced_sha256` — the SHA gate
//!   filters out IDE touch noise that bumps mtime without changing
//!   bytes).
//!
//! ## Frontmatter shape
//!
//! ```yaml
//! ---
//! node_uuid: 7f30...
//! slug: alice
//! subkind: person
//! title: Alice
//! aliases:
//!   - Allie
//! enrichment_tier: 2
//! last_synthesized_at: "2026-05-15T10:00:00Z"
//! last_synced_version_id: e8c1...
//! timeline:
//!   - date: "2026-05-01"
//!     text: "joined Acme"
//!   - date: "2026-05-15"
//!     text: "promoted to staff"
//! ---
//!
//! <compiled_truth markdown body>
//! ```
//!
//! New files the user creates *without* `node_uuid` are treated as
//! "create a new EntityPage from this file" by Phase 7.2.
//!
//! ## Why YAML and not TOML/JSON
//!
//! Obsidian, Foam, Logseq, and most static-site generators expect
//! `---`-delimited YAML frontmatter. Round-tripping through that
//! convention is the whole point of Phase 7. We use `serde_yml` for
//! parse + emit so user-edited variations (key order, single vs
//! double quotes, etc.) are tolerated by the parser without our own
//! validation maze.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::memory_graph::entity_page::{EntityPageMetadata, TimelineEntry};
use crate::memory_graph::store::MemoryGraphStore;

// ─── Frontmatter struct ────────────────────────────────────────────────

/// Wire-shape for the YAML frontmatter block. Fields that are absent in
/// the parsed file deserialize to `None` / empty so round-tripping a
/// pre-Phase-7 file (no `node_uuid`) still works.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrainFrontmatter {
    /// Optional — present when the file was first written by `export`.
    /// Absent on user-created files (Phase 7.2 treats absent + valid
    /// slug as "create a new EntityPage and adopt this file").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_uuid: Option<String>,
    /// The `last_synced_version_id` written by the most recent export.
    /// Lets the sync engine notice "DB has moved past this" without
    /// hitting `brain_sync_state` for every file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synced_version_id: Option<String>,
    pub slug: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subkind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment_tier: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synthesized_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<TimelineEntry>,
}

// ─── Export config + outcome ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BrainExportConfig {
    /// Absolute path of the brain directory. Defaults to
    /// `~/Documents/workground/brain` when the IPC caller doesn't
    /// override.
    pub brain_root: PathBuf,
    pub space_id: String,
}

impl BrainExportConfig {
    /// Resolve the default brain root: `~/Documents/workground/brain/`.
    /// Returns `None` if the home directory can't be determined.
    pub fn default_brain_root() -> Option<PathBuf> {
        dirs::document_dir().map(|d| d.join("workground").join("brain"))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BrainExportOutcome {
    pub pages_written: u32,
    pub pages_unchanged: u32,
    pub overview_written: bool,
    pub index_written: bool,
    /// Per-page error strings; the outcome is still success-shape so
    /// the caller can show a partial export.
    pub errors: Vec<String>,
}

// ─── Pure render / parse helpers ───────────────────────────────────────

/// Compose the full `.md` file body (frontmatter + body) for one
/// EntityPage. The `compiled_truth` is the markdown body that follows
/// the `---` block.
pub fn render_file(frontmatter: &BrainFrontmatter, compiled_truth: &str) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    match serde_yml::to_string(frontmatter) {
        Ok(yaml) => {
            // serde_yml emits a trailing newline by default — keep it.
            out.push_str(yaml.trim_end_matches('\n'));
            out.push('\n');
        }
        Err(e) => {
            // Defensive fallback so we never write a broken file.
            tracing::warn!("brain_io: frontmatter serialize failed: {}", e);
            out.push_str(&format!("# frontmatter serialize error: {}\n", e));
        }
    }
    out.push_str("---\n\n");
    out.push_str(compiled_truth.trim_start_matches('\n'));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Parse a markdown file into (frontmatter, body). Returns `None` when
/// the file doesn't begin with `---\n` (i.e. has no frontmatter); the
/// caller decides how to treat such files (Phase 7.2 currently skips
/// them, on the grounds that they were created outside the sync flow).
pub fn parse_file(raw: &str) -> Option<(BrainFrontmatter, String)> {
    let trimmed = raw.trim_start_matches('\u{feff}'); // BOM
    let rest = trimmed.strip_prefix("---\n").or_else(|| trimmed.strip_prefix("---\r\n"))?;
    // Find the closing `---` line. We're permissive about newline
    // style: \n or \r\n, and tolerate trailing spaces on the marker.
    let end = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    let yaml = &rest[..end];
    // Skip past "\n---" or "\r\n---", then past the line break that
    // follows it (if any).
    let after_marker = &rest[end..];
    let body = after_marker
        .split_once('\n')
        .map(|(_, tail)| tail)
        .map(|tail| tail.split_once('\n').map(|(_, rest)| rest).unwrap_or(tail))
        .unwrap_or("");
    let fm: BrainFrontmatter = match serde_yml::from_str(yaml) {
        Ok(fm) => fm,
        Err(e) => {
            tracing::warn!("brain_io: frontmatter parse failed: {}", e);
            return None;
        }
    };
    Some((fm, body.to_string()))
}

/// SHA-256 the bytes as a lowercase hex string. Used to gate
/// "real edit" vs "touch with no content change" in Phase 7.2.
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

/// Build the on-disk path for an EntityPage. The layout mirrors
/// gbrain: `<brain_root>/<subkind>/<slug>.md`. When `subkind` is
/// missing we fall back to `_uncategorized/` so the user can still
/// find the file (and the sync engine can still resolve it).
pub fn file_path_for_page(
    brain_root: &Path,
    subkind: Option<&str>,
    slug: &str,
) -> PathBuf {
    let safe_subkind = subkind.unwrap_or("_uncategorized");
    let safe_slug = slug.trim();
    brain_root.join(safe_subkind).join(format!("{}.md", safe_slug))
}

/// Lossy filesystem mtime → epoch ms. Returns 0 on any failure so the
/// caller treats the file as "ancient" and a future scan reads its
/// real mtime in the next pass.
pub fn file_mtime_ms(path: &Path) -> i64 {
    match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
        Err(_) => 0,
    }
}

// ─── DB-side helpers ───────────────────────────────────────────────────

/// Project the EntityPage row + current active version into a
/// frontmatter struct. Returns `None` when the node doesn't exist or
/// isn't an entity_page.
pub fn read_page_for_export(
    store: &MemoryGraphStore,
    node_id: &str,
) -> Result<Option<(BrainFrontmatter, String, String)>, crate::error::Error> {
    // Returns (frontmatter, body, active_version_id_or_empty).
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT title, metadata_json FROM memory_nodes \
             WHERE id = ?1 AND kind = 'entity_page'",
            params![node_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let (title, metadata_raw) = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    let meta_value: serde_json::Value = metadata_raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let meta = EntityPageMetadata::from_value(&meta_value);
    let slug = meta
        .slug
        .clone()
        .unwrap_or_else(|| node_id.to_string());
    let (active_version_id, compiled_truth): (String, String) = conn
        .query_row(
            "SELECT id, content FROM memory_versions \
             WHERE node_id = ?1 AND status = 'active' \
             ORDER BY created_at DESC LIMIT 1",
            params![node_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap_or_else(|_| (String::new(), String::new()));

    let fm = BrainFrontmatter {
        node_uuid: Some(node_id.to_string()),
        last_synced_version_id: if active_version_id.is_empty() {
            None
        } else {
            Some(active_version_id.clone())
        },
        slug,
        title,
        subkind: meta.subkind.clone(),
        aliases: meta.aliases.clone(),
        enrichment_tier: meta.enrichment_tier,
        last_synthesized_at: meta.last_synthesized_at.clone(),
        timeline: meta.timeline.clone(),
    };
    Ok(Some((fm, compiled_truth, active_version_id)))
}

/// Insert or update one `brain_sync_state` row after a successful
/// export. Best-effort: errors logged, not returned, so a flaky FS
/// op doesn't fail the whole batch.
pub fn upsert_sync_state(
    store: &MemoryGraphStore,
    node_id: &str,
    space_id: &str,
    file_path: &Path,
    last_synced_version_id: Option<&str>,
    file_mtime_at_last_sync_ms: i64,
    last_synced_sha256: &str,
) -> Result<(), crate::error::Error> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    conn.execute(
        "INSERT INTO brain_sync_state \
         (node_id, space_id, file_path, last_synced_version_id, \
          last_synced_at_ms, file_mtime_at_last_sync_ms, last_synced_sha256) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(node_id) DO UPDATE SET \
           space_id = excluded.space_id, \
           file_path = excluded.file_path, \
           last_synced_version_id = excluded.last_synced_version_id, \
           last_synced_at_ms = excluded.last_synced_at_ms, \
           file_mtime_at_last_sync_ms = excluded.file_mtime_at_last_sync_ms, \
           last_synced_sha256 = excluded.last_synced_sha256",
        params![
            node_id,
            space_id,
            file_path.to_string_lossy().to_string(),
            last_synced_version_id,
            now_ms,
            file_mtime_at_last_sync_ms,
            last_synced_sha256,
        ],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(())
}

/// Look up the existing sync state for one node. Returns `None` when
/// the row hasn't been created yet (first export).
pub fn read_sync_state(
    store: &MemoryGraphStore,
    node_id: &str,
) -> Result<Option<SyncStateRow>, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let row = conn
        .query_row(
            "SELECT space_id, file_path, last_synced_version_id, \
                    last_synced_at_ms, file_mtime_at_last_sync_ms, last_synced_sha256 \
             FROM brain_sync_state WHERE node_id = ?1",
            params![node_id],
            |r| {
                Ok(SyncStateRow {
                    space_id: r.get(0)?,
                    file_path: r.get(1)?,
                    last_synced_version_id: r.get(2)?,
                    last_synced_at_ms: r.get(3)?,
                    file_mtime_at_last_sync_ms: r.get(4)?,
                    last_synced_sha256: r.get(5)?,
                })
            },
        )
        .ok();
    Ok(row)
}

#[derive(Debug, Clone)]
pub struct SyncStateRow {
    pub space_id: String,
    pub file_path: String,
    pub last_synced_version_id: Option<String>,
    pub last_synced_at_ms: i64,
    pub file_mtime_at_last_sync_ms: i64,
    pub last_synced_sha256: String,
}

// ─── End-to-end export ─────────────────────────────────────────────────

/// Export one EntityPage to its `<brain_root>/<subkind>/<slug>.md`
/// file and refresh `brain_sync_state`. Idempotent: re-running on an
/// unchanged page short-circuits when the rendered SHA matches the
/// stored one.
pub fn export_entity_page(
    store: &MemoryGraphStore,
    node_id: &str,
    cfg: &BrainExportConfig,
) -> Result<ExportPageOutcome, crate::error::Error> {
    let (fm, compiled_truth, active_version_id) = read_page_for_export(store, node_id)?
        .ok_or_else(|| {
            crate::error::Error::Internal(format!("EntityPage not found: {}", node_id))
        })?;
    let rendered = render_file(&fm, &compiled_truth);
    let new_sha = sha256_hex(&rendered);

    let existing = read_sync_state(store, node_id)?;
    let expected_path =
        file_path_for_page(&cfg.brain_root, fm.subkind.as_deref(), &fm.slug);

    // Short-circuit on a perfect match (same sha + same path + same
    // active version) so re-export-all is cheap.
    if let Some(s) = &existing {
        if s.last_synced_sha256 == new_sha
            && s.file_path == expected_path.to_string_lossy()
            && s.last_synced_version_id.as_deref()
                == (if active_version_id.is_empty() {
                    None
                } else {
                    Some(active_version_id.as_str())
                })
        {
            return Ok(ExportPageOutcome::Unchanged);
        }
    }

    // If the file is being moved (subkind / slug changed), delete the
    // old file first so we don't leave orphans behind. Cheap; bail-out
    // is fine if the unlink fails (often a stale entry).
    if let Some(s) = existing {
        let old = PathBuf::from(s.file_path);
        if old != expected_path && old.exists() {
            let _ = fs::remove_file(&old);
        }
    }

    if let Some(parent) = expected_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            crate::error::Error::Internal(format!("create_dir_all {}: {}", parent.display(), e))
        })?;
    }
    let mut f = fs::File::create(&expected_path).map_err(|e| {
        crate::error::Error::Internal(format!("create {}: {}", expected_path.display(), e))
    })?;
    f.write_all(rendered.as_bytes())
        .map_err(|e| crate::error::Error::Internal(format!("write: {}", e)))?;
    drop(f);

    let mtime_ms = file_mtime_ms(&expected_path);
    upsert_sync_state(
        store,
        node_id,
        &cfg.space_id,
        &expected_path,
        if active_version_id.is_empty() {
            None
        } else {
            Some(&active_version_id)
        },
        mtime_ms,
        &new_sha,
    )?;
    Ok(ExportPageOutcome::Written {
        path: expected_path,
        sha256: new_sha,
    })
}

#[derive(Debug)]
pub enum ExportPageOutcome {
    Written { path: PathBuf, sha256: String },
    Unchanged,
}

/// Export every EntityPage in `space_id` to the brain directory, plus
/// the current `wiki_artifacts(kind='overview')` and `('index')` rows
/// as `overview.md` / `index.md` at the brain root.
pub fn export_all(
    store: &MemoryGraphStore,
    cfg: &BrainExportConfig,
) -> Result<BrainExportOutcome, crate::error::Error> {
    let mut outcome = BrainExportOutcome::default();

    let node_ids: Vec<String> = {
        let conn = store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id FROM memory_nodes \
                 WHERE space_id = ?1 AND kind = 'entity_page' \
                 ORDER BY updated_at DESC",
            )
            .map_err(crate::error::Error::Database)?;
        let rows = stmt
            .query_map(params![cfg.space_id], |r| r.get::<_, String>(0))
            .map_err(crate::error::Error::Database)?;
        rows.flatten().collect()
    };

    for nid in &node_ids {
        match export_entity_page(store, nid, cfg) {
            Ok(ExportPageOutcome::Written { .. }) => outcome.pages_written += 1,
            Ok(ExportPageOutcome::Unchanged) => outcome.pages_unchanged += 1,
            Err(e) => outcome.errors.push(format!("{}: {}", nid, e)),
        }
    }

    // Overview + index artifacts (best-effort).
    outcome.overview_written =
        export_wiki_artifact(store, cfg, "overview", "overview.md").unwrap_or(false);
    outcome.index_written =
        export_wiki_artifact(store, cfg, "index", "index.md").unwrap_or(false);

    Ok(outcome)
}

/// Write the latest `wiki_artifacts` row of `kind` into the brain root
/// as `filename`. Returns true when a file was written (false when no
/// such artifact exists yet — empty wiki = no overview.md).
pub fn export_wiki_artifact(
    store: &MemoryGraphStore,
    cfg: &BrainExportConfig,
    kind: &str,
    filename: &str,
) -> Result<bool, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let body: Option<String> = conn
        .query_row(
            "SELECT content FROM wiki_artifacts \
             WHERE space_id = ?1 AND kind = ?2 \
             ORDER BY generated_at DESC LIMIT 1",
            params![cfg.space_id, kind],
            |r| r.get::<_, String>(0),
        )
        .ok();
    drop(conn);
    let body = match body {
        Some(b) => b,
        None => return Ok(false),
    };
    let target = cfg.brain_root.join(filename);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            crate::error::Error::Internal(format!("create_dir_all {}: {}", parent.display(), e))
        })?;
    }
    fs::write(&target, body).map_err(|e| {
        crate::error::Error::Internal(format!("write {}: {}", target.display(), e))
    })?;
    Ok(true)
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::entity_page::TimelineEntry;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch(crate::db::migrations::V37_MEMORY_OS_PHASE_7).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn insert_page(
        store: &MemoryGraphStore,
        id: &str,
        title: &str,
        slug: &str,
        subkind: &str,
        compiled_truth: &str,
        aliases: Vec<&str>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let mut meta = EntityPageMetadata::default();
        meta.slug = Some(slug.into());
        meta.subkind = Some(subkind.into());
        meta.aliases = aliases.into_iter().map(String::from).collect();
        meta.timeline = vec![TimelineEntry {
            date: "2026-05-01".into(),
            text: "first observed".into(),
            source_node_id: None,
            source_session_id: None,
        }];
        let meta_json = serde_json::to_string(&meta.to_value()).unwrap();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json, created_at, updated_at) \
             VALUES (?1, 'default', 'entity_page', ?2, ?3, ?4, ?4)",
            params![id, title, meta_json, now],
        )
        .unwrap();
        let v_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_versions (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
             VALUES (?1, ?2, NULL, 'active', ?3, NULL, NULL, ?4)",
            params![v_id, id, compiled_truth, now],
        )
        .unwrap();
    }

    // ─── Pure helpers ──────────────────────────────────────────────

    #[test]
    fn sha256_hex_is_deterministic_and_lowercase() {
        let a = sha256_hex("hello world");
        let b = sha256_hex("hello world");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "SHA-256 hex is 64 chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_lowercase())));
    }

    #[test]
    fn sha256_hex_differs_on_content_change() {
        let a = sha256_hex("hello");
        let b = sha256_hex("hello world");
        assert_ne!(a, b);
    }

    #[test]
    fn file_path_uses_subkind_then_slug() {
        let root = PathBuf::from("/tmp/brain");
        let p = file_path_for_page(&root, Some("person"), "alice");
        assert_eq!(p, PathBuf::from("/tmp/brain/person/alice.md"));
    }

    #[test]
    fn file_path_falls_back_to_uncategorized() {
        let root = PathBuf::from("/tmp/brain");
        let p = file_path_for_page(&root, None, "x");
        assert_eq!(p, PathBuf::from("/tmp/brain/_uncategorized/x.md"));
    }

    #[test]
    fn render_file_emits_yaml_frontmatter() {
        let fm = BrainFrontmatter {
            node_uuid: Some("n1".into()),
            last_synced_version_id: Some("v1".into()),
            slug: "alice".into(),
            title: "Alice".into(),
            subkind: Some("person".into()),
            aliases: vec!["Allie".into()],
            enrichment_tier: Some(2),
            last_synthesized_at: None,
            timeline: vec![],
        };
        let out = render_file(&fm, "Senior engineer at Acme.");
        assert!(out.starts_with("---\n"));
        assert!(out.contains("node_uuid: n1"));
        assert!(out.contains("slug: alice"));
        assert!(out.contains("subkind: person"));
        assert!(out.contains("- Allie"));
        assert!(out.contains("enrichment_tier: 2"));
        assert!(out.contains("---\n\nSenior engineer at Acme."));
    }

    #[test]
    fn render_then_parse_roundtrips_full_frontmatter() {
        let fm = BrainFrontmatter {
            node_uuid: Some("n1".into()),
            last_synced_version_id: Some("v1".into()),
            slug: "alice".into(),
            title: "Alice".into(),
            subkind: Some("person".into()),
            aliases: vec!["Allie".into(), "A. S.".into()],
            enrichment_tier: Some(2),
            last_synthesized_at: Some("2026-05-15T10:00:00Z".into()),
            timeline: vec![TimelineEntry {
                date: "2026-05-01".into(),
                text: "joined Acme".into(),
                source_node_id: None,
                source_session_id: None,
            }],
        };
        let body = "Senior engineer at **Acme**.\n\nWorks on RAG.";
        let rendered = render_file(&fm, body);
        let (parsed_fm, parsed_body) = parse_file(&rendered).expect("parse");
        assert_eq!(parsed_fm.node_uuid, fm.node_uuid);
        assert_eq!(parsed_fm.slug, fm.slug);
        assert_eq!(parsed_fm.aliases, fm.aliases);
        assert_eq!(parsed_fm.enrichment_tier, fm.enrichment_tier);
        assert_eq!(parsed_fm.timeline.len(), 1);
        assert_eq!(parsed_fm.timeline[0].date, "2026-05-01");
        assert_eq!(parsed_body.trim(), body.trim());
    }

    #[test]
    fn parse_file_handles_user_minimal_frontmatter() {
        // A user creating a file by hand — only slug + title, no UUID.
        let raw = "---\nslug: bob\ntitle: Bob\n---\n\nA person.\n";
        let (fm, body) = parse_file(raw).expect("minimal parses");
        assert!(fm.node_uuid.is_none(), "no UUID → Phase 7.2 'create new'");
        assert_eq!(fm.slug, "bob");
        assert_eq!(fm.title, "Bob");
        assert_eq!(body.trim(), "A person.");
    }

    #[test]
    fn parse_file_returns_none_when_no_frontmatter() {
        assert!(parse_file("just a markdown body\n").is_none());
        assert!(parse_file("# heading\n").is_none());
    }

    // ─── DB-backed export ──────────────────────────────────────────

    #[test]
    fn export_entity_page_writes_file_and_sync_state() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "Senior eng.", vec!["Allie"]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let out = export_entity_page(&store, "n1", &cfg).unwrap();
        let path = match out {
            ExportPageOutcome::Written { path, .. } => path,
            ExportPageOutcome::Unchanged => panic!("first export must write"),
        };
        assert_eq!(path, tmp.path().join("person").join("alice.md"));
        assert!(path.exists(), "file must be on disk");

        let body = fs::read_to_string(&path).unwrap();
        let (fm, md) = parse_file(&body).expect("written file must parse");
        assert_eq!(fm.node_uuid.as_deref(), Some("n1"));
        assert_eq!(fm.slug, "alice");
        assert_eq!(fm.aliases, vec!["Allie"]);
        assert!(md.contains("Senior eng."));

        // brain_sync_state row was written.
        let state = read_sync_state(&store, "n1").unwrap().unwrap();
        assert_eq!(state.file_path, path.to_string_lossy());
        assert!(state.last_synced_at_ms > 0);
        assert_eq!(state.last_synced_sha256.len(), 64);
    }

    #[test]
    fn export_entity_page_is_idempotent() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "Senior eng.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let first = export_entity_page(&store, "n1", &cfg).unwrap();
        assert!(matches!(first, ExportPageOutcome::Written { .. }));
        let second = export_entity_page(&store, "n1", &cfg).unwrap();
        assert!(
            matches!(second, ExportPageOutcome::Unchanged),
            "unchanged content → idempotent short-circuit"
        );
    }

    #[test]
    fn export_entity_page_moves_file_when_slug_changes() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "x", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let first = export_entity_page(&store, "n1", &cfg).unwrap();
        let old_path = match first {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        assert!(old_path.exists());

        // Rename: change metadata.slug from "alice" → "alice-smith".
        {
            let conn = store.conn.lock().unwrap();
            let raw: String = conn
                .query_row(
                    "SELECT metadata_json FROM memory_nodes WHERE id = 'n1'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let mut meta: EntityPageMetadata =
                serde_json::from_str(&raw).unwrap();
            meta.slug = Some("alice-smith".into());
            conn.execute(
                "UPDATE memory_nodes SET metadata_json = ?1 WHERE id = 'n1'",
                params![serde_json::to_string(&meta.to_value()).unwrap()],
            )
            .unwrap();
        }

        let second = export_entity_page(&store, "n1", &cfg).unwrap();
        let new_path = match second {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!("slug change should re-write"),
        };
        assert_eq!(new_path.file_stem().unwrap(), "alice-smith");
        assert!(new_path.exists());
        assert!(!old_path.exists(), "old file should have been removed");
    }

    #[test]
    fn export_all_writes_every_entity_page() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "...", vec![]);
        insert_page(&store, "n2", "Bob", "bob", "person", "...", vec![]);
        insert_page(&store, "n3", "RAG", "rag", "concept", "...", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let outcome = export_all(&store, &cfg).unwrap();
        assert_eq!(outcome.pages_written, 3);
        assert_eq!(outcome.pages_unchanged, 0);
        assert!(outcome.errors.is_empty());
        assert!(tmp.path().join("person/alice.md").exists());
        assert!(tmp.path().join("person/bob.md").exists());
        assert!(tmp.path().join("concept/rag.md").exists());
    }

    #[test]
    fn export_all_marks_unchanged_on_second_run() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "x", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        export_all(&store, &cfg).unwrap();
        let second = export_all(&store, &cfg).unwrap();
        assert_eq!(second.pages_written, 0);
        assert_eq!(second.pages_unchanged, 1);
    }

    #[test]
    fn export_wiki_artifact_writes_overview_when_present() {
        let store = fresh_store();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO wiki_artifacts (id, space_id, kind, content, generated_at, source_node_ids) \
                 VALUES ('w1', 'default', 'overview', '# Overview\n\nstuff', 1, '[]')",
                [],
            )
            .unwrap();
        }
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let wrote = export_wiki_artifact(&store, &cfg, "overview", "overview.md").unwrap();
        assert!(wrote);
        let body = fs::read_to_string(tmp.path().join("overview.md")).unwrap();
        assert!(body.contains("# Overview"));
    }

    #[test]
    fn export_wiki_artifact_returns_false_when_absent() {
        let store = fresh_store();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let wrote = export_wiki_artifact(&store, &cfg, "overview", "overview.md").unwrap();
        assert!(!wrote, "no artifact → no file");
        assert!(!tmp.path().join("overview.md").exists());
    }
}
