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

// ─── Sync from disk (Phase 7.2) ────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct BrainSyncOutcome {
    /// Total .md files visited under brain_root.
    pub files_scanned: u32,
    /// Files without frontmatter (skipped — not part of the sync flow).
    pub files_skipped_no_frontmatter: u32,
    /// Files unchanged since the last sync (mtime + sha both match).
    pub files_unchanged: u32,
    /// Files that prompted creating a new EntityPage (no node_uuid in
    /// frontmatter and slug was available).
    pub new_pages_created: u32,
    /// Files that updated an existing EntityPage (new version inserted).
    pub pages_updated: u32,
    /// Conflicts detected — `Phase 7.3` actually writes the finding;
    /// Phase 7.2 counts them and continues with "disk wins" semantics.
    pub conflicts: u32,
    /// Per-file errors (the outcome shape stays success so the UI can
    /// surface a partial result).
    pub errors: Vec<String>,
}

/// Walk `brain_root` recursively and reconcile each `.md` file with
/// the EntityPage row referenced by its frontmatter. See module-level
/// docs for the per-file algorithm.
pub fn sync_from_disk(
    store: &MemoryGraphStore,
    cfg: &BrainExportConfig,
) -> Result<BrainSyncOutcome, crate::error::Error> {
    let mut outcome = BrainSyncOutcome::default();
    if !cfg.brain_root.exists() {
        return Ok(outcome);
    }
    let files = collect_md_files(&cfg.brain_root);
    for path in files {
        outcome.files_scanned += 1;
        match sync_one_file(store, cfg, &path) {
            Ok(SyncFileOutcome::Unchanged) => outcome.files_unchanged += 1,
            Ok(SyncFileOutcome::NoFrontmatter) => outcome.files_skipped_no_frontmatter += 1,
            Ok(SyncFileOutcome::Created { .. }) => outcome.new_pages_created += 1,
            Ok(SyncFileOutcome::Updated { conflict }) => {
                outcome.pages_updated += 1;
                if conflict {
                    outcome.conflicts += 1;
                }
            }
            Err(e) => outcome.errors.push(format!("{}: {}", path.display(), e)),
        }
    }
    Ok(outcome)
}

#[derive(Debug)]
pub enum SyncFileOutcome {
    Unchanged,
    NoFrontmatter,
    Created { node_id: String },
    Updated { conflict: bool },
}

/// Recursive descent for `.md` files. Skips `overview.md` and
/// `index.md` at the brain root — those are export-only artifacts.
fn collect_md_files(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                // Skip the two "artifact" files at brain root.
                let is_at_root = p.parent() == Some(root);
                let stem = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if is_at_root && (stem == "overview.md" || stem == "index.md") {
                    continue;
                }
                out.push(p);
            }
        }
    }
    // Deterministic order helps tests + makes the conflict-counter
    // stable across runs.
    out.sort();
    out
}

/// Single-file sync. The conflict flag in the `Updated` branch is
/// pulled up so the caller can write a `memory_health_findings` row
/// (Phase 7.3 does the actual write; Phase 7.2 just counts it).
pub fn sync_one_file(
    store: &MemoryGraphStore,
    cfg: &BrainExportConfig,
    path: &Path,
) -> Result<SyncFileOutcome, crate::error::Error> {
    let raw = fs::read_to_string(path).map_err(|e| {
        crate::error::Error::Internal(format!("read {}: {}", path.display(), e))
    })?;
    let parsed = match parse_file(&raw) {
        Some(p) => p,
        None => return Ok(SyncFileOutcome::NoFrontmatter),
    };
    let (fm, body) = parsed;
    let current_sha = sha256_hex(&raw);
    let current_mtime_ms = file_mtime_ms(path);

    // Branch on whether frontmatter carries a node_uuid.
    if let Some(node_id) = fm.node_uuid.as_deref() {
        // Existing-page path: figure out if disk advanced, DB advanced,
        // or both (conflict).
        let state = read_sync_state(store, node_id)?;
        if let Some(s) = &state {
            if s.last_synced_sha256 == current_sha
                && s.file_mtime_at_last_sync_ms == current_mtime_ms
            {
                return Ok(SyncFileOutcome::Unchanged);
            }
            if s.last_synced_sha256 == current_sha {
                // mtime changed but bytes didn't (touch / IDE save churn).
                // Update the recorded mtime so the next pass short-circuits.
                upsert_sync_state(
                    store,
                    node_id,
                    &cfg.space_id,
                    path,
                    s.last_synced_version_id.as_deref(),
                    current_mtime_ms,
                    &current_sha,
                )?;
                return Ok(SyncFileOutcome::Unchanged);
            }
        }
        // Real disk-side change. Check for concurrent DB change.
        let conflict = detect_concurrent_db_change(store, node_id, state.as_ref())?;

        // Snapshot the colliding DB version + last-synced version id
        // BEFORE we overwrite, so the health finding payload tells the
        // user exactly which two versions diverged.
        let pre_disk_db_version = current_active_version_id(store, node_id)?;
        let prior_sync_version_id = state.as_ref().and_then(|s| s.last_synced_version_id.clone());

        // Apply disk-wins: write new version + update metadata.
        write_new_version_from_disk(store, node_id, &fm, &body)?;
        // Update sync state to reflect what's now on disk + which DB
        // version we just produced.
        let new_active_version = current_active_version_id(store, node_id)?;
        upsert_sync_state(
            store,
            node_id,
            &cfg.space_id,
            path,
            new_active_version.as_deref(),
            current_mtime_ms,
            &current_sha,
        )?;

        if conflict {
            // Phase 7.3 — write a memory_health_findings row so the user
            // sees the conflict in the Health tab. Disk-wins already
            // applied; the finding is informational so the user can
            // review what got overwritten.
            let _ = write_sync_conflict_finding(
                store,
                &cfg.space_id,
                node_id,
                path,
                prior_sync_version_id.as_deref(),
                pre_disk_db_version.as_deref(),
                new_active_version.as_deref(),
            );
        }
        Ok(SyncFileOutcome::Updated { conflict })
    } else if !fm.slug.trim().is_empty() {
        // No node_uuid → user created this file by hand. Create a new
        // EntityPage if the slug doesn't already exist.
        let new_node_id = create_page_from_disk(store, cfg, &fm, &body)?;
        let new_active_version = current_active_version_id(store, &new_node_id)?;
        upsert_sync_state(
            store,
            &new_node_id,
            &cfg.space_id,
            path,
            new_active_version.as_deref(),
            current_mtime_ms,
            &current_sha,
        )?;
        Ok(SyncFileOutcome::Created { node_id: new_node_id })
    } else {
        // Frontmatter exists but is unusable (no UUID + no slug). Skip.
        Ok(SyncFileOutcome::NoFrontmatter)
    }
}

/// Write a `memory_health_findings` row with `check_kind='sync_conflict'`
/// when both disk and DB advanced since the last sync. Best-effort:
/// errors logged + swallowed so a flaky finding write doesn't block
/// the sync itself.
///
/// Payload includes the file path, the prior synced version id, the
/// DB version id that got overwritten by disk-wins, and the new active
/// version id produced from disk. That's enough for the Health UI to
/// later show a diff between the lost DB version and the kept disk
/// version (Phase 9 / 10 will add the actual diff view).
fn write_sync_conflict_finding(
    store: &MemoryGraphStore,
    space_id: &str,
    node_id: &str,
    file_path: &Path,
    prior_synced_version_id: Option<&str>,
    overwritten_db_version_id: Option<&str>,
    new_active_version_id: Option<&str>,
) -> Result<(), crate::error::Error> {
    let payload = serde_json::json!({
        "file_path": file_path.to_string_lossy(),
        "prior_synced_version_id": prior_synced_version_id,
        "overwritten_db_version_id": overwritten_db_version_id,
        "new_active_version_id": new_active_version_id,
        "resolution": "disk_wins",
    });
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    // Reuse the same dedup contract as the Phase 4 health checks via
    // memory_health::upsert_finding — same `(space_id, subject,
    // check_kind)` key, with `subject = node_id` so each page gets at
    // most one open conflict at a time.
    let inserted = crate::proactive::scenarios::memory_health::upsert_finding(
        &conn,
        space_id,
        "error",
        "sync_conflict",
        node_id,
        Some(&payload),
        now_ms,
    )?;
    if inserted > 0 {
        tracing::warn!(
            node_id,
            file = %file_path.display(),
            "brain_io: sync conflict — disk won, finding recorded"
        );
    }
    Ok(())
}

/// Did the DB's active version advance since the last sync? Returns
/// true when the page has a newer active version than the
/// `brain_sync_state.last_synced_version_id` we recorded — meaning
/// both sides moved and Phase 7.3 should record a conflict.
fn detect_concurrent_db_change(
    store: &MemoryGraphStore,
    node_id: &str,
    state: Option<&SyncStateRow>,
) -> Result<bool, crate::error::Error> {
    let recorded = match state {
        Some(s) => s.last_synced_version_id.clone(),
        // No prior sync state for this node → can't detect a conflict
        // (we don't know what we had before). Treat as no-conflict.
        None => return Ok(false),
    };
    let current = current_active_version_id(store, node_id)?;
    Ok(current != recorded)
}

fn current_active_version_id(
    store: &MemoryGraphStore,
    node_id: &str,
) -> Result<Option<String>, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_versions \
             WHERE node_id = ?1 AND status = 'active' \
             ORDER BY created_at DESC LIMIT 1",
            params![node_id],
            |r| r.get(0),
        )
        .ok();
    Ok(id)
}

/// Deprecate the prior active version + insert a new active one with
/// `content = body`. The Phase 2 auto-link hook runs as a side effect.
/// Frontmatter metadata (aliases, timeline, subkind, etc.) is merged
/// into `memory_nodes.metadata_json` while we hold the lock so reads
/// after this function see a consistent shape.
fn write_new_version_from_disk(
    store: &MemoryGraphStore,
    node_id: &str,
    fm: &BrainFrontmatter,
    body: &str,
) -> Result<(), crate::error::Error> {
    // Deprecate the previous active version (if any) under one short lock.
    let prev_id: Option<String> = {
        let conn = store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM memory_versions \
                 WHERE node_id = ?1 AND status = 'active' \
                 ORDER BY created_at DESC LIMIT 1",
                params![node_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(pid) = &id {
            conn.execute(
                "UPDATE memory_versions SET status = 'deprecated' WHERE id = ?1",
                params![pid],
            )
            .map_err(crate::error::Error::Database)?;
        }
        id
    };

    // Insert the new active version via store.create_version (this is
    // what runs the auto-link side-effect).
    let new_version_id = uuid::Uuid::new_v4().to_string();
    let now_iso = chrono::Utc::now().to_rfc3339();
    let new_version = crate::memory_graph::models::MemoryVersion {
        id: new_version_id,
        node_id: node_id.to_string(),
        supersedes_version_id: prev_id,
        status: crate::memory_graph::models::MemoryVersionStatus::Active,
        content: body.to_string(),
        metadata: None,
        embedding_json: None,
        created_at: now_iso.clone(),
    };
    store
        .create_version(&new_version)
        .map_err(|e| crate::error::Error::Internal(format!("create_version: {}", e)))?;

    // Merge frontmatter fields into memory_nodes.metadata_json.
    merge_frontmatter_into_metadata(store, node_id, fm, &now_iso)?;
    Ok(())
}

fn merge_frontmatter_into_metadata(
    store: &MemoryGraphStore,
    node_id: &str,
    fm: &BrainFrontmatter,
    now_iso: &str,
) -> Result<(), crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM memory_nodes WHERE id = ?1",
            params![node_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let value: serde_json::Value = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut meta = EntityPageMetadata::from_value(&value);
    // Disk wins on every field the user can edit in markdown.
    meta.slug = Some(fm.slug.clone());
    meta.subkind = fm.subkind.clone();
    meta.aliases = fm.aliases.clone();
    if fm.enrichment_tier.is_some() {
        meta.enrichment_tier = fm.enrichment_tier;
    }
    if !fm.timeline.is_empty() {
        meta.timeline = fm.timeline.clone();
    }
    if fm.last_synthesized_at.is_some() {
        meta.last_synthesized_at = fm.last_synthesized_at.clone();
    }
    let new_json = serde_json::to_string(&meta.to_value())
        .map_err(crate::error::Error::Serde)?;
    conn.execute(
        "UPDATE memory_nodes \
         SET metadata_json = ?1, title = ?2, updated_at = ?3 \
         WHERE id = ?4",
        params![new_json, fm.title, now_iso, node_id],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(())
}

/// Brand-new EntityPage from a user-created file. Returns the new
/// node_id. If a page with the same slug already exists in the
/// space, returns its id and writes a new version against it (treats
/// the file as an "adopt this slug" gesture).
fn create_page_from_disk(
    store: &MemoryGraphStore,
    cfg: &BrainExportConfig,
    fm: &BrainFrontmatter,
    body: &str,
) -> Result<String, crate::error::Error> {
    // Look for an existing page with the same slug in this space.
    let existing_id: Option<String> = {
        let conn = store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.query_row(
            "SELECT id FROM memory_nodes \
             WHERE space_id = ?1 AND kind = 'entity_page' \
               AND COALESCE(json_extract(metadata_json, '$.slug'), '') = ?2 \
             LIMIT 1",
            params![cfg.space_id, fm.slug],
            |r| r.get::<_, String>(0),
        )
        .ok()
    };

    let node_id = match existing_id {
        Some(id) => {
            // Adopt the existing page; merge frontmatter + write new version.
            write_new_version_from_disk(store, &id, fm, body)?;
            id
        }
        None => {
            // Insert a brand-new node + version under one lock.
            let new_id = uuid::Uuid::new_v4().to_string();
            let now_iso = chrono::Utc::now().to_rfc3339();
            let mut meta = EntityPageMetadata::default();
            meta.slug = Some(fm.slug.clone());
            meta.subkind = fm.subkind.clone();
            meta.aliases = fm.aliases.clone();
            meta.enrichment_tier = fm.enrichment_tier;
            meta.last_synthesized_at = fm.last_synthesized_at.clone();
            meta.timeline = fm.timeline.clone();
            let meta_json = serde_json::to_string(&meta.to_value())
                .map_err(crate::error::Error::Serde)?;
            let conn = store
                .conn
                .lock()
                .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
            conn.execute(
                "INSERT INTO memory_nodes \
                 (id, space_id, kind, title, metadata_json, created_at, updated_at) \
                 VALUES (?1, ?2, 'entity_page', ?3, ?4, ?5, ?5)",
                params![new_id, cfg.space_id, fm.title, meta_json, now_iso],
            )
            .map_err(crate::error::Error::Database)?;
            drop(conn);
            // Insert the active version via create_version so auto-link runs.
            let new_version = crate::memory_graph::models::MemoryVersion {
                id: uuid::Uuid::new_v4().to_string(),
                node_id: new_id.clone(),
                supersedes_version_id: None,
                status: crate::memory_graph::models::MemoryVersionStatus::Active,
                content: body.to_string(),
                metadata: None,
                embedding_json: None,
                created_at: now_iso,
            };
            store.create_version(&new_version).map_err(|e| {
                crate::error::Error::Internal(format!("create_version: {}", e))
            })?;
            new_id
        }
    };
    Ok(node_id)
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

    // ─── Sync (Phase 7.2) ──────────────────────────────────────────

    #[test]
    fn sync_from_disk_handles_empty_brain_root() {
        let store = fresh_store();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.files_scanned, 0);
        assert!(outcome.errors.is_empty());
    }

    #[test]
    fn sync_from_disk_skips_files_without_frontmatter() {
        let store = fresh_store();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(
            tmp.path().join("notes/random.md"),
            "Just a random note, no frontmatter here.\n",
        )
        .unwrap();
        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.files_scanned, 1);
        assert_eq!(outcome.files_skipped_no_frontmatter, 1);
        assert_eq!(outcome.pages_updated, 0);
    }

    #[test]
    fn sync_from_disk_ignores_overview_and_index_at_root() {
        let store = fresh_store();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        fs::write(tmp.path().join("overview.md"), "# overview").unwrap();
        fs::write(tmp.path().join("index.md"), "# index").unwrap();
        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.files_scanned, 0, "artifact files must be skipped");
    }

    #[test]
    fn sync_from_disk_marks_unchanged_when_neither_side_moved() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "x", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        // First export creates the file + brain_sync_state row.
        let _ = export_entity_page(&store, "n1", &cfg).unwrap();
        // Sync immediately — nothing changed on either side.
        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.files_unchanged, 1);
        assert_eq!(outcome.pages_updated, 0);
        assert_eq!(outcome.conflicts, 0);
    }

    #[test]
    fn sync_from_disk_writes_new_version_when_disk_changes() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "old.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let path = match export_entity_page(&store, "n1", &cfg).unwrap() {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        // Simulate the user editing the file in Obsidian: rewrite the
        // body (preserve the frontmatter).
        let raw = fs::read_to_string(&path).unwrap();
        let (fm, _) = parse_file(&raw).unwrap();
        let edited = render_file(&fm, "Alice is the new Acme staff engineer.");
        // Sleep briefly so mtime ticks. Some filesystems have 1-second
        // resolution; this is the safe lower bound.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, edited).unwrap();

        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.pages_updated, 1);
        assert_eq!(outcome.files_unchanged, 0);

        // DB now has a NEW active version with the edited body.
        let active = store.get_active_version("n1").unwrap().unwrap();
        assert!(active.content.contains("new Acme staff engineer"));

        // The previous active version was deprecated.
        let conn = store.conn.lock().unwrap();
        let deprecated: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_versions \
                 WHERE node_id = 'n1' AND status = 'deprecated'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deprecated, 1);

        // brain_sync_state was advanced to the new sha.
        drop(conn);
        let st = read_sync_state(&store, "n1").unwrap().unwrap();
        assert_ne!(st.last_synced_sha256, sha256_hex(&raw));
    }

    #[test]
    fn sync_from_disk_ignores_touch_without_content_change() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "x", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let path = match export_entity_page(&store, "n1", &cfg).unwrap() {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        // Simulate IDE 'touch' — overwrite with the same bytes, mtime
        // moves forward.
        let raw = fs::read_to_string(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, &raw).unwrap();
        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(
            outcome.pages_updated, 0,
            "touch with identical content must not create a new version"
        );
        assert_eq!(outcome.files_unchanged, 1);
    }

    #[test]
    fn sync_from_disk_creates_new_entity_page_for_user_authored_file() {
        let store = fresh_store();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        // User creates a file by hand — no node_uuid in frontmatter.
        fs::create_dir_all(tmp.path().join("person")).unwrap();
        let body = "---\nslug: charlie\ntitle: Charlie\nsubkind: person\n---\n\nCharlie is a friend.\n";
        fs::write(tmp.path().join("person/charlie.md"), body).unwrap();

        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.new_pages_created, 1);

        // Verify the page exists in DB.
        let conn = store.conn.lock().unwrap();
        let row: (String, String) = conn
            .query_row(
                "SELECT id, title FROM memory_nodes \
                 WHERE kind = 'entity_page' \
                   AND COALESCE(json_extract(metadata_json, '$.slug'), '') = 'charlie'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let (new_id, title) = row;
        assert_eq!(title, "Charlie");
        // And a brain_sync_state row was inserted for it.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM brain_sync_state WHERE node_id = ?1",
                params![new_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn sync_from_disk_detects_conflict_when_both_sides_moved() {
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "original.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let path = match export_entity_page(&store, "n1", &cfg).unwrap() {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        // DB-side update: insert a new active version (deprecating the
        // old one). This simulates the user using the Agent to update
        // the page while their Obsidian was also editing the file.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_versions SET status = 'deprecated' \
                 WHERE node_id = 'n1' AND status = 'active'",
                [],
            )
            .unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO memory_versions \
                 (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
                 VALUES (?1, 'n1', NULL, 'active', 'db-side update', NULL, NULL, ?2)",
                params![uuid::Uuid::new_v4().to_string(), now],
            )
            .unwrap();
        }
        // Disk-side update: edit the body.
        let raw = fs::read_to_string(&path).unwrap();
        let (fm, _) = parse_file(&raw).unwrap();
        let edited = render_file(&fm, "disk-side update");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, edited).unwrap();

        let outcome = sync_from_disk(&store, &cfg).unwrap();
        assert_eq!(outcome.pages_updated, 1);
        assert_eq!(outcome.conflicts, 1, "both sides moved → conflict counted");
        // Disk wins: active version content matches the disk edit.
        let active = store.get_active_version("n1").unwrap().unwrap();
        assert_eq!(active.content, "disk-side update");
    }

    #[test]
    fn sync_from_disk_writes_sync_conflict_finding_when_conflict() {
        // Same setup as the conflict test above, but assert that a
        // memory_health_findings row was written.
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "original.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let path = match export_entity_page(&store, "n1", &cfg).unwrap() {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        // DB-side change.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_versions SET status = 'deprecated' \
                 WHERE node_id = 'n1' AND status = 'active'",
                [],
            )
            .unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO memory_versions \
                 (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
                 VALUES (?1, 'n1', NULL, 'active', 'db-side', NULL, NULL, ?2)",
                params![uuid::Uuid::new_v4().to_string(), now],
            )
            .unwrap();
        }
        // Disk-side change.
        let raw = fs::read_to_string(&path).unwrap();
        let (fm, _) = parse_file(&raw).unwrap();
        let edited = render_file(&fm, "disk-side update");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, edited).unwrap();

        sync_from_disk(&store, &cfg).unwrap();

        // Health finding row exists, severity=error, kind=sync_conflict.
        let conn = store.conn.lock().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE check_kind = 'sync_conflict' AND subject = 'n1' \
                   AND severity = 'error' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "exactly one open sync_conflict finding for n1");
        // Payload mentions the resolution.
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM memory_health_findings \
                 WHERE check_kind = 'sync_conflict' AND subject = 'n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(payload.contains("disk_wins"));
        assert!(payload.contains("file_path"));
    }

    #[test]
    fn sync_from_disk_does_not_duplicate_open_conflict_findings() {
        // Two syncs in a row both detect the same conflict: the second
        // should NOT insert a duplicate row (Phase 4 dedup contract).
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "original.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        let path = match export_entity_page(&store, "n1", &cfg).unwrap() {
            ExportPageOutcome::Written { path, .. } => path,
            _ => panic!(),
        };
        // DB-side change.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_versions SET status = 'deprecated' \
                 WHERE node_id = 'n1' AND status = 'active'",
                [],
            )
            .unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO memory_versions \
                 (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
                 VALUES (?1, 'n1', NULL, 'active', 'db-side', NULL, NULL, ?2)",
                params![uuid::Uuid::new_v4().to_string(), now],
            )
            .unwrap();
        }
        // Disk-side change.
        let raw = fs::read_to_string(&path).unwrap();
        let (fm, _) = parse_file(&raw).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, render_file(&fm, "disk-1")).unwrap();
        sync_from_disk(&store, &cfg).unwrap();

        // Second disk edit + DB also moves again.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_versions SET status = 'deprecated' \
                 WHERE node_id = 'n1' AND status = 'active'",
                [],
            )
            .unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO memory_versions \
                 (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
                 VALUES (?1, 'n1', NULL, 'active', 'db-2', NULL, NULL, ?2)",
                params![uuid::Uuid::new_v4().to_string(), now],
            )
            .unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&path, render_file(&fm, "disk-2")).unwrap();
        sync_from_disk(&store, &cfg).unwrap();

        let conn = store.conn.lock().unwrap();
        let open_findings: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE check_kind = 'sync_conflict' AND subject = 'n1' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            open_findings, 1,
            "dedup contract: only one OPEN sync_conflict per (node, kind)"
        );
    }

    #[test]
    fn sync_from_disk_adopts_existing_slug_when_user_writes_duplicate() {
        // User-authored file uses an existing slug → adopt that page
        // rather than creating a duplicate.
        let store = fresh_store();
        insert_page(&store, "n1", "Alice", "alice", "person", "original.", vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = BrainExportConfig {
            brain_root: tmp.path().to_path_buf(),
            space_id: "default".into(),
        };
        // The user writes a file with slug "alice" but no node_uuid.
        fs::create_dir_all(tmp.path().join("person")).unwrap();
        let body = "---\nslug: alice\ntitle: Alice\nsubkind: person\n---\n\nNew prose body.\n";
        fs::write(tmp.path().join("person/alice.md"), body).unwrap();

        let outcome = sync_from_disk(&store, &cfg).unwrap();
        // Either accounting is correct ("new" or "updated") as long as
        // the count of entity_page nodes hasn't doubled.
        assert!(outcome.new_pages_created + outcome.pages_updated >= 1);
        let conn = store.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_nodes \
                 WHERE kind = 'entity_page' \
                   AND COALESCE(json_extract(metadata_json, '$.slug'), '') = 'alice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "must not create a duplicate page for the same slug");
    }
}
