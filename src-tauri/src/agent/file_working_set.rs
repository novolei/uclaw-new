// SPDX-License-Identifier: Apache-2.0
//! Persisted per-session file working set (agent context continuity, phase 2).
//!
//! Every file the agent reads, writes, edits, deletes or uses as a plan is
//! appended to `agent_file_events` (V64). [`load`] folds that append-only log
//! into a [`FileWorkingSet`] — modified files first, then files that were only
//! read — and [`render`] turns it into the `<file_working_set>` block that
//! compaction carries forward so the model never "forgets" what it touched.
//!
//! Paths are normalized lexically (no filesystem access: files may have been
//! deleted) so `src/a.rs`, `./src/a.rs`, `<root>/src/a.rs` and `src/x/../a.rs`
//! collapse to one entry.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uclaw_message_types::estimate_tokens;

/// Max characters stored in `agent_file_events.detail`.
const MAX_DETAIL_CHARS: usize = 600;
/// Most recent change summaries kept per modified file.
const MAX_CHANGES_PER_FILE: usize = 5;
/// Most recent distinct read ranges kept per read file.
const MAX_RANGES_PER_FILE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOp {
    Read,
    Write,
    Edit,
    Delete,
    Plan,
}

impl FileOp {
    fn as_str(self) -> &'static str {
        match self {
            FileOp::Read => "read",
            FileOp::Write => "write",
            FileOp::Edit => "edit",
            FileOp::Delete => "delete",
            FileOp::Plan => "plan",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "read" => FileOp::Read,
            "write" => FileOp::Write,
            "edit" => FileOp::Edit,
            "delete" => FileOp::Delete,
            "plan" => FileOp::Plan,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvent {
    pub path: String,
    pub op: FileOp,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedFile {
    pub path: String,
    /// First modifying event was a `write` with no prior `read`.
    pub created: bool,
    /// Last modifying event was a `delete`.
    pub deleted: bool,
    pub edit_count: u32,
    /// Most recent change summaries (≤ 5), oldest → newest.
    pub changes: Vec<String>,
    pub last_modified_at: i64,
    /// At least one modification came from a rewound turn.
    pub rewound: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFile {
    pub path: String,
    pub ranges: Vec<String>,
    pub last_read_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWorkingSet {
    /// Ordered by `last_modified_at` descending.
    pub modified: Vec<ModifiedFile>,
    /// Only files never modified, ordered by `last_read_at` descending.
    pub read: Vec<ReadFile>,
    /// Path of the most recent non-rewound `plan` event.
    pub active_plan: Option<String>,
}

impl FileWorkingSet {
    pub fn is_empty(&self) -> bool {
        self.modified.is_empty() && self.read.is_empty()
    }

    /// Every path in the set (modified, then read-only), in set order —
    /// what `.uclawrules` glob rules are matched against (P3 Decision 9).
    pub fn paths(&self) -> Vec<std::path::PathBuf> {
        self.modified
            .iter()
            .map(|m| m.path.as_str())
            .chain(self.read.iter().map(|r| r.path.as_str()))
            .map(std::path::PathBuf::from)
            .collect()
    }
}

/// Lexically normalize `p`: drop `.`, resolve `..` against preceding normal
/// components (never above the root of an absolute path).
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(comp),
            },
            other => out.push(other),
        }
    }
    out.iter().map(|c| c.as_os_str()).collect()
}

fn to_slash_string(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
        .replacen("//", "/", 1)
}

/// Normalize a tool-supplied path into the working-set key.
///
/// Lexical only (no `canonicalize`): files may already be deleted. Paths
/// inside `workspace_root` become root-relative; paths outside it stay
/// absolute; relative paths with no root are just cleaned up.
pub fn normalize_path(raw: &str, workspace_root: Option<&Path>) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let raw_path = Path::new(raw);
    let root = workspace_root.map(lexical_normalize);
    let joined = match (&root, raw_path.is_absolute()) {
        (Some(root), false) => root.join(raw_path),
        _ => raw_path.to_path_buf(),
    };
    let norm = lexical_normalize(&joined);
    if let Some(root) = &root {
        if let Ok(rel) = norm.strip_prefix(root) {
            let s = to_slash_string(rel);
            return if s.is_empty() { ".".to_string() } else { s };
        }
    }
    let s = to_slash_string(&norm);
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

/// Append `events` to the session's file-event log. Paths are expected to be
/// normalized already ([`normalize_path`]); `detail` is capped at 600 chars.
/// Does not open its own transaction, so it is safe inside a caller's.
pub fn record(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    events: &[FileEvent],
) -> anyhow::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut stmt = conn.prepare(
        "INSERT INTO agent_file_events (session_id, turn_id, path, op, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for ev in events {
        if ev.path.is_empty() {
            continue;
        }
        stmt.execute(params![
            session_id,
            turn_id,
            ev.path,
            ev.op.as_str(),
            truncate_chars(&ev.detail, MAX_DETAIL_CHARS),
            now
        ])?;
    }
    Ok(())
}

#[derive(Default)]
struct PathAgg {
    read_before_first_mod: bool,
    first_mod_op: Option<FileOp>,
    last_mod_op: Option<FileOp>,
    mod_count: u32,
    changes: Vec<String>,
    last_modified_at: i64,
    last_mod_id: i64,
    rewound: bool,
    ranges: Vec<String>,
    last_read_at: i64,
    last_read_id: i64,
}

/// Fold the session's file-event log into a [`FileWorkingSet`].
pub fn load(conn: &Connection, session_id: &str) -> anyhow::Result<FileWorkingSet> {
    let mut stmt = conn.prepare(
        "SELECT id, path, op, detail, rewound, created_at
         FROM agent_file_events WHERE session_id = ?1 ORDER BY id",
    )?;
    let rows = stmt
        .query_map([session_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut aggs: HashMap<String, PathAgg> = HashMap::new();
    let mut active_plan: Option<(i64, String)> = None;
    for (id, path, op, detail, rewound, created_at) in rows {
        let Some(op) = FileOp::parse(&op) else { continue };
        match op {
            FileOp::Plan => {
                if !rewound {
                    active_plan = Some((id, path));
                }
            }
            FileOp::Read => {
                let agg = aggs.entry(path).or_default();
                if agg.first_mod_op.is_none() {
                    agg.read_before_first_mod = true;
                }
                if !detail.is_empty() {
                    agg.ranges.retain(|r| r != &detail);
                    agg.ranges.push(detail);
                    if agg.ranges.len() > MAX_RANGES_PER_FILE {
                        agg.ranges.remove(0);
                    }
                }
                agg.last_read_at = created_at;
                agg.last_read_id = id;
            }
            FileOp::Write | FileOp::Edit | FileOp::Delete => {
                let agg = aggs.entry(path).or_default();
                if agg.first_mod_op.is_none() {
                    agg.first_mod_op = Some(op);
                }
                agg.last_mod_op = Some(op);
                agg.mod_count += 1;
                if !detail.is_empty() {
                    agg.changes.push(detail);
                    if agg.changes.len() > MAX_CHANGES_PER_FILE {
                        agg.changes.remove(0);
                    }
                }
                agg.last_modified_at = created_at;
                agg.last_mod_id = id;
                agg.rewound |= rewound;
            }
        }
    }

    let mut modified: Vec<(i64, ModifiedFile)> = Vec::new();
    let mut read: Vec<(i64, ReadFile)> = Vec::new();
    for (path, agg) in aggs {
        if agg.mod_count > 0 {
            modified.push((
                agg.last_mod_id,
                ModifiedFile {
                    path,
                    created: agg.first_mod_op == Some(FileOp::Write) && !agg.read_before_first_mod,
                    deleted: agg.last_mod_op == Some(FileOp::Delete),
                    edit_count: agg.mod_count,
                    changes: agg.changes,
                    last_modified_at: agg.last_modified_at,
                    rewound: agg.rewound,
                },
            ));
        } else {
            read.push((
                agg.last_read_id,
                ReadFile { path, ranges: agg.ranges, last_read_at: agg.last_read_at },
            ));
        }
    }
    // Timestamps share a millisecond within one `record` call; the row id
    // breaks ties so the order is the true recency order.
    modified.sort_by(|a, b| (b.1.last_modified_at, b.0).cmp(&(a.1.last_modified_at, a.0)));
    read.sort_by(|a, b| (b.1.last_read_at, b.0).cmp(&(a.1.last_read_at, a.0)));

    Ok(FileWorkingSet {
        modified: modified.into_iter().map(|(_, m)| m).collect(),
        read: read.into_iter().map(|(_, r)| r).collect(),
        active_plan: active_plan.map(|(_, p)| p),
    })
}

/// How much of the working set a render attempt keeps.
#[derive(Clone, Copy)]
struct RenderShape {
    read_keep: usize,
    changes_keep: usize,
    modified_keep: usize,
}

const REWOUND_NOTE: &str = " [来自已回退的轮次，磁盘上仍保留]";

fn modified_label(m: &ModifiedFile) -> String {
    if m.deleted {
        "已删除".to_string()
    } else if m.created {
        "新建".to_string()
    } else {
        format!("修改 {} 次", m.edit_count)
    }
}

fn render_shape(ws: &FileWorkingSet, shape: RenderShape) -> String {
    let mut out = String::from("<file_working_set>\n");
    if !ws.modified.is_empty() {
        out.push_str("已修改的文件：\n");
        for m in ws.modified.iter().take(shape.modified_keep) {
            out.push_str(&format!("- {}（{}）", m.path, modified_label(m)));
            if m.rewound {
                out.push_str(REWOUND_NOTE);
            }
            out.push('\n');
            let skip = m.changes.len().saturating_sub(shape.changes_keep);
            for c in m.changes.iter().skip(skip) {
                out.push_str(&format!("  · {}\n", c));
            }
        }
        let omitted = ws.modified.len().saturating_sub(shape.modified_keep);
        if omitted > 0 {
            out.push_str(&format!("…另有 {} 个文件\n", omitted));
        }
    }
    if !ws.read.is_empty() {
        out.push_str("读过的文件：\n");
        for r in ws.read.iter().take(shape.read_keep) {
            if r.ranges.is_empty() {
                out.push_str(&format!("- {}\n", r.path));
            } else {
                out.push_str(&format!("- {}（{}）\n", r.path, r.ranges.join("、")));
            }
        }
        let omitted = ws.read.len().saturating_sub(shape.read_keep);
        if omitted > 0 {
            out.push_str(&format!("…另有 {} 个文件\n", omitted));
        }
    }
    out.push_str("</file_working_set>");
    out
}

/// Render the `<file_working_set>` block within `max_tokens`.
///
/// Degrades in order: drop read files from the tail → trim change summaries
/// → paths only → drop modified files from the tail. Each cut is marked
/// `…另有 N 个文件`. An empty working set renders as `""`.
pub fn render(ws: &FileWorkingSet, max_tokens: usize) -> String {
    if ws.is_empty() {
        return String::new();
    }
    let fits = |s: &str| (estimate_tokens(s) as usize) <= max_tokens;
    let n_mod = ws.modified.len();
    let n_read = ws.read.len();

    for read_keep in (0..=n_read).rev() {
        let s = render_shape(
            ws,
            RenderShape { read_keep, changes_keep: MAX_CHANGES_PER_FILE, modified_keep: n_mod },
        );
        if fits(&s) {
            return s;
        }
    }
    for changes_keep in (0..MAX_CHANGES_PER_FILE).rev() {
        let s = render_shape(ws, RenderShape { read_keep: 0, changes_keep, modified_keep: n_mod });
        if fits(&s) {
            return s;
        }
    }
    for modified_keep in (0..n_mod).rev() {
        let s = render_shape(ws, RenderShape { read_keep: 0, changes_keep: 0, modified_keep });
        if fits(&s) || modified_keep == 0 {
            return s;
        }
    }
    // n_mod == 0 and even an empty read list does not fit: smallest form.
    render_shape(ws, RenderShape { read_keep: 0, changes_keep: 0, modified_keep: 0 })
}

fn render_rewound_shape(rewound: &[&ModifiedFile], keep: usize) -> String {
    let mut out = String::from(
        "<rewound_file_changes>\n以下文件改动来自已回退的轮次，对话中已看不到这些调用，但改动仍在磁盘上：\n",
    );
    for m in rewound.iter().take(keep) {
        out.push_str(&format!("- {}（{}）{}\n", m.path, modified_label(m), REWOUND_NOTE));
        if let Some(last) = m.changes.last() {
            out.push_str(&format!("  · {}\n", last));
        }
    }
    let omitted = rewound.len().saturating_sub(keep);
    if omitted > 0 {
        out.push_str(&format!("…另有 {} 个文件\n", omitted));
    }
    out.push_str("</rewound_file_changes>");
    out
}

/// Context continuity P2 — the `<rewound_file_changes>` block surfaced in the
/// per-turn dynamic context after a rewind/fork: only the modified entries
/// with `rewound == true` (their tool calls are gone from the transcript but
/// the disk changes remain), path + label + note + the last change summary,
/// in working-set order (most recent first — deterministic, cache-stable).
/// Files are dropped from the tail to fit `max_tokens`, marked
/// `…另有 N 个文件`. No rewound entries → `""`.
pub fn render_rewound_changes(ws: &FileWorkingSet, max_tokens: usize) -> String {
    let rewound: Vec<&ModifiedFile> = ws.modified.iter().filter(|m| m.rewound).collect();
    if rewound.is_empty() {
        return String::new();
    }
    for keep in (0..=rewound.len()).rev() {
        let s = render_rewound_shape(&rewound, keep);
        if keep == 0 || (estimate_tokens(&s) as usize) <= max_tokens {
            return s;
        }
    }
    unreachable!("keep == 0 always returns")
}

/// Remove every file event of `session_id` (called from
/// [`crate::agent::transcript::delete_session`]).
pub fn delete_session(conn: &Connection, session_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM agent_file_events WHERE session_id = ?1", [session_id])?;
    Ok(())
}

/// Rewind: mark every event of `session_id` whose `turn_id` is not in
/// `kept_turn_ids` as `rewound = 1`. Events are never deleted (the disk
/// changes are still there); events with no turn id are left alone. Runs no
/// transaction of its own — call it inside the rewind transaction.
pub fn mark_rewound_after(
    tx: &Connection,
    session_id: &str,
    kept_turn_ids: &HashSet<String>,
) -> anyhow::Result<usize> {
    let mut stmt = tx.prepare(
        "SELECT id, turn_id FROM agent_file_events
         WHERE session_id = ?1 AND rewound = 0 AND turn_id IS NOT NULL",
    )?;
    let stale: Vec<i64> = stmt
        .query_map([session_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, turn)| !kept_turn_ids.contains(turn))
        .map(|(id, _)| id)
        .collect();
    let mut upd = tx.prepare("UPDATE agent_file_events SET rewound = 1 WHERE id = ?1")?;
    for id in &stale {
        upd.execute([id])?;
    }
    Ok(stale.len())
}

/// Fork: copy every event of `src` into `dst`, in order, keeping
/// `created_at`. Events whose `turn_id` is in `turn_id_map` get the mapped id
/// and keep their `rewound` flag; events of turns after the fork point (not
/// in the map) are copied too — their disk changes are still there — with
/// `rewound = 1`. Events with no turn id are copied as-is. Runs no
/// transaction of its own — call it inside the fork transaction.
pub fn clone_for_fork(
    tx: &Connection,
    src: &str,
    dst: &str,
    turn_id_map: &HashMap<String, String>,
) -> anyhow::Result<usize> {
    let mut stmt = tx.prepare(
        "SELECT turn_id, path, op, detail, rewound, created_at
         FROM agent_file_events WHERE session_id = ?1 ORDER BY id",
    )?;
    let rows = stmt
        .query_map([src], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ins = tx.prepare(
        "INSERT INTO agent_file_events (session_id, turn_id, path, op, detail, rewound, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (turn_id, path, op, detail, rewound, created_at) in &rows {
        let (turn_id, rewound) = match turn_id {
            None => (None, *rewound),
            Some(t) => match turn_id_map.get(t) {
                Some(mapped) => (Some(mapped.clone()), *rewound),
                None => (Some(t.clone()), 1),
            },
        };
        ins.execute(params![dst, turn_id, path, op, detail, rewound, created_at])?;
    }
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn ev(path: &str, op: FileOp, detail: &str) -> FileEvent {
        FileEvent { path: path.to_string(), op, detail: detail.to_string() }
    }

    fn col_count(conn: &Connection, table: &str, cols: &[&str]) -> i64 {
        let list = cols.iter().map(|c| format!("'{c}'")).collect::<Vec<_>>().join(",");
        conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name IN ({list})"),
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn v64_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).expect("first run");
        crate::db::migrations::run(&conn).expect("second run must not error (idempotent)");
        assert_eq!(
            col_count(
                &conn,
                "agent_file_events",
                &["id", "session_id", "turn_id", "path", "op", "detail", "rewound", "created_at"]
            ),
            8
        );
        assert_eq!(col_count(&conn, "agent_transcript", &["cleared_json", "cleared_est_tokens"]), 2);
        let idx: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_agent_file_events_session'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn normalizes_equivalent_paths_to_one_entry() {
        let root = Path::new("/work/proj");
        let forms = ["src/a.rs", "./src/a.rs", "/work/proj/src/a.rs", "src/x/../a.rs"];
        for f in forms {
            assert_eq!(normalize_path(f, Some(root)), "src/a.rs", "form {f}");
        }
        assert_eq!(normalize_path("/etc/hosts", Some(root)), "/etc/hosts");
        assert_eq!(normalize_path("/work/other/../proj2/b.rs", Some(root)), "/work/proj2/b.rs");
        assert_eq!(normalize_path("./src//a.rs", None), "src/a.rs");

        let conn = db();
        let events: Vec<FileEvent> =
            forms.iter().map(|f| ev(&normalize_path(f, Some(root)), FileOp::Edit, "")).collect();
        record(&conn, "s", Some("t1"), &events).unwrap();
        let ws = load(&conn, "s").unwrap();
        assert_eq!(ws.modified.len(), 1);
        assert_eq!(ws.modified[0].path, "src/a.rs");
        assert_eq!(ws.modified[0].edit_count, 4);
    }

    #[test]
    fn load_orders_modified_first_and_excludes_modified_from_read() {
        let conn = db();
        record(
            &conn,
            "s",
            Some("t1"),
            &[
                ev("a.rs", FileOp::Read, "1-20"),
                ev("b.rs", FileOp::Read, "1-50"),
                ev("a.rs", FileOp::Edit, "rename foo"),
                ev("c.rs", FileOp::Read, ""),
                ev("d.rs", FileOp::Write, "new file"),
            ],
        )
        .unwrap();
        let ws = load(&conn, "s").unwrap();
        let modified: Vec<_> = ws.modified.iter().map(|m| m.path.as_str()).collect();
        let read: Vec<_> = ws.read.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(modified, vec!["d.rs", "a.rs"]);
        assert_eq!(read, vec!["c.rs", "b.rs"]);
        assert_eq!(ws.read[1].ranges, vec!["1-50".to_string()]);
        // other sessions are invisible
        assert!(load(&conn, "other").unwrap().is_empty());
    }

    #[test]
    fn load_marks_created_and_deleted() {
        let conn = db();
        record(
            &conn,
            "s",
            None,
            &[
                ev("new.rs", FileOp::Write, "created"),
                ev("new.rs", FileOp::Edit, "tweak"),
                ev("old.rs", FileOp::Read, ""),
                ev("old.rs", FileOp::Write, "overwrite"),
                ev("gone.rs", FileOp::Edit, "x"),
                ev("gone.rs", FileOp::Delete, ""),
            ],
        )
        .unwrap();
        let ws = load(&conn, "s").unwrap();
        let get = |p: &str| ws.modified.iter().find(|m| m.path == p).unwrap().clone();
        let new = get("new.rs");
        assert!(new.created && !new.deleted);
        assert_eq!(new.edit_count, 2);
        let old = get("old.rs");
        assert!(!old.created && !old.deleted, "read before write is not a creation");
        let gone = get("gone.rs");
        assert!(gone.deleted && !gone.created);
    }

    #[test]
    fn load_keeps_last_five_changes() {
        let conn = db();
        let events: Vec<FileEvent> =
            (1..=7).map(|i| ev("a.rs", FileOp::Edit, &format!("change {i}"))).collect();
        record(&conn, "s", Some("t1"), &events).unwrap();
        let ws = load(&conn, "s").unwrap();
        assert_eq!(ws.modified[0].edit_count, 7);
        assert_eq!(
            ws.modified[0].changes,
            (3..=7).map(|i| format!("change {i}")).collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_active_plan_ignores_rewound() {
        let conn = db();
        record(&conn, "s", Some("t1"), &[ev("plan-a.md", FileOp::Plan, "")]).unwrap();
        record(&conn, "s", Some("t2"), &[ev("plan-b.md", FileOp::Plan, ""), ev("x.rs", FileOp::Edit, "e")])
            .unwrap();
        assert_eq!(load(&conn, "s").unwrap().active_plan.as_deref(), Some("plan-b.md"));

        conn.execute("UPDATE agent_file_events SET rewound = 1 WHERE turn_id = 't2'", []).unwrap();
        let ws = load(&conn, "s").unwrap();
        assert_eq!(ws.active_plan.as_deref(), Some("plan-a.md"));
        assert!(ws.modified[0].rewound, "rewound modification is kept but marked");
        assert!(ws.modified.iter().all(|m| m.path != "plan-a.md" && m.path != "plan-b.md"));
    }

    #[test]
    fn render_empty_is_empty_string() {
        assert_eq!(render(&FileWorkingSet::default(), 10_000), "");
    }

    #[test]
    fn render_truncates_read_list_first() {
        let ws = FileWorkingSet {
            modified: vec![ModifiedFile {
                path: "src/main.rs".into(),
                created: false,
                deleted: false,
                edit_count: 2,
                changes: vec!["add fn foo".into(), "fix bar".into()],
                last_modified_at: 10,
                rewound: true,
            }],
            read: (0..40)
                .map(|i| ReadFile {
                    path: format!("src/read_{i:02}.rs"),
                    ranges: vec!["1-100".into()],
                    last_read_at: 100 - i,
                })
                .collect(),
            active_plan: None,
        };
        let full = render(&ws, 1_000_000);
        assert!(full.starts_with("<file_working_set>") && full.ends_with("</file_working_set>"));
        assert!(full.contains("src/read_39.rs"));
        assert!(!full.contains("另有"));
        let full_tokens = estimate_tokens(&full) as usize;

        let cut = render(&ws, full_tokens / 2);
        assert!(estimate_tokens(&cut) as usize <= full_tokens / 2);
        // Modified section and its change summaries survive intact…
        assert!(cut.contains("src/main.rs（修改 2 次）"));
        assert!(cut.contains("来自已回退的轮次，磁盘上仍保留"));
        assert!(cut.contains("add fn foo") && cut.contains("fix bar"));
        // …while the tail of the read list is dropped and counted.
        assert!(cut.contains("src/read_00.rs"));
        assert!(!cut.contains("src/read_39.rs"));
        assert!(cut.contains("…另有 "));
        assert!(cut.find("已修改的文件").unwrap() < cut.find("读过的文件").unwrap());

        // Tighter still: summaries go before paths do.
        let paths_only = render_shape(
            &ws,
            RenderShape { read_keep: 0, changes_keep: 0, modified_keep: 1 },
        );
        let tiny = render(&ws, estimate_tokens(&paths_only) as usize);
        assert_eq!(tiny, paths_only);
        assert!(tiny.contains("src/main.rs") && tiny.contains("…另有 40 个文件"));
        assert!(!tiny.contains("add fn foo"));
    }

    #[test]
    fn delete_session_removes_file_events() {
        let conn = db();
        for s in ["s", "other"] {
            record(&conn, s, Some("t1"), &[ev("a.rs", FileOp::Edit, "x")]).unwrap();
        }
        crate::agent::transcript::delete_session(&conn, "s").unwrap();
        assert!(load(&conn, "s").unwrap().is_empty());
        assert!(!load(&conn, "other").unwrap().is_empty());
    }

    fn modified(path: &str, rewound: bool, changes: &[&str]) -> ModifiedFile {
        ModifiedFile {
            path: path.into(),
            created: false,
            deleted: false,
            edit_count: changes.len() as u32,
            changes: changes.iter().map(|c| c.to_string()).collect(),
            last_modified_at: 0,
            rewound,
        }
    }

    #[test]
    fn rewound_changes_render_only_rewound_entries() {
        let ws = FileWorkingSet {
            modified: vec![
                modified("kept.rs", false, &["edit kept"]),
                modified("src/gone.rs", true, &["first change", "last change"]),
                ModifiedFile { deleted: true, ..modified("old.rs", true, &["rm"]) },
            ],
            read: vec![ReadFile { path: "r.rs".into(), ranges: vec![], last_read_at: 0 }],
            active_plan: None,
        };
        let out = render_rewound_changes(&ws, 1_500);
        assert!(out.starts_with("<rewound_file_changes>") && out.ends_with("</rewound_file_changes>"), "{out}");
        assert!(out.contains("- src/gone.rs（修改 2 次） [来自已回退的轮次，磁盘上仍保留]\n  · last change\n"), "{out}");
        assert!(!out.contains("first change"), "only the last change summary");
        assert!(out.contains("- old.rs（已删除） [来自已回退的轮次，磁盘上仍保留]"), "{out}");
        assert!(!out.contains("kept.rs") && !out.contains("r.rs"), "only rewound modified entries");
        assert_eq!(out, render_rewound_changes(&ws, 1_500), "deterministic");
    }

    #[test]
    fn rewound_changes_empty_without_rewound_entries() {
        let ws = FileWorkingSet { modified: vec![modified("a.rs", false, &["x"])], ..Default::default() };
        assert_eq!(render_rewound_changes(&ws, 1_500), "");
        assert_eq!(render_rewound_changes(&FileWorkingSet::default(), 1_500), "");
    }

    #[test]
    fn rewound_changes_cap_counts_omitted_files() {
        let long = "x".repeat(200);
        let ws = FileWorkingSet {
            modified: (0..200).map(|i| modified(&format!("dir/file_{i}.rs"), true, &[long.as_str()])).collect(),
            ..Default::default()
        };
        let out = render_rewound_changes(&ws, 1_500);
        assert!(estimate_tokens(&out) as usize <= 1_500, "within budget");
        assert!(out.contains("dir/file_0.rs"), "most recent first");
        let shown = out.matches("[来自已回退的轮次").count();
        assert!(shown > 0 && shown < 200);
        assert!(out.contains(&format!("…另有 {} 个文件", 200 - shown)), "{out}");
    }
}
