//! Install service: write/remove a skill's files under a root, plus the V25
//! `marketplace_standalone_installs` bookkeeping (`item_type = 'skill'`).
//!
//! Roots: [`global_root`] (`<data_dir>/skills/_marketplace`) or
//! [`workspace_root`] (`<ws>/.uclaw/skills`). Workspace installs are real
//! copies (no symlinks) so the skill travels with the project.

use std::path::{Path, PathBuf};

use super::{MarketplaceError, SkillDetail};

use rusqlite::OptionalExtension as _;

/// `expo/skills/react-native` → `expo__skills__react-native`.
pub fn flatten_slug(id: &str) -> String {
    id.replace('/', "__")
}

/// `<data_dir>/skills/_marketplace`.
pub fn global_root(data_dir: &Path) -> PathBuf {
    data_dir.join("skills").join("_marketplace")
}

/// `<ws>/.uclaw/skills`.
pub fn workspace_root(ws: &Path) -> PathBuf {
    ws.join(".uclaw").join("skills")
}

/// A single safe name: non-empty, not `.`/`..`, only `[A-Za-z0-9._-]`.
/// Rules out `/`, `\`, `:` (drive letters), absolute paths and traversal.
fn is_safe_component(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

fn check_slug(slug: &str) -> Result<(), MarketplaceError> {
    if is_safe_component(slug) {
        Ok(())
    } else {
        Err(MarketplaceError::Invalid(format!("不安全的技能标识：{slug}")))
    }
}

/// A relative file path whose every `/`-separated segment is safe.
fn check_rel_path(path: &str) -> Result<(), MarketplaceError> {
    if path.is_empty() || path.split('/').any(|seg| !is_safe_component(seg)) {
        return Err(MarketplaceError::Invalid(format!("不安全的文件路径：{path}")));
    }
    Ok(())
}

fn io_err(e: std::io::Error) -> MarketplaceError {
    MarketplaceError::Install(e.to_string())
}

/// Write `detail.files` into `root/slug/`, replacing any previous install
/// there. Every path component is validated before anything is written.
pub fn write_skill_files(root: &Path, slug: &str, detail: &SkillDetail) -> Result<PathBuf, MarketplaceError> {
    check_slug(slug)?;
    for f in &detail.files {
        check_rel_path(&f.path)?;
    }
    let dir = root.join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(io_err)?;
    }
    for f in &detail.files {
        let dest = dir.join(&f.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        std::fs::write(&dest, &f.contents).map_err(io_err)?;
    }
    Ok(dir)
}

/// Remove `root/slug/`; returns `(dir, existed)`. Missing dir is a no-op.
pub fn remove_skill_dir(root: &Path, slug: &str) -> Result<(PathBuf, bool), MarketplaceError> {
    check_slug(slug)?;
    let dir = root.join(slug);
    let existed = dir.exists();
    if existed {
        std::fs::remove_dir_all(&dir).map_err(io_err)?;
    }
    Ok((dir, existed))
}

fn db_err(e: rusqlite::Error) -> MarketplaceError {
    MarketplaceError::Install(e.to_string())
}

/// Record (or refresh) an install row; `version` = SKILL.md hash.
pub fn record_install(conn: &rusqlite::Connection, slug: &str, hash: &str) -> Result<(), MarketplaceError> {
    conn.execute(
        "INSERT OR REPLACE INTO marketplace_standalone_installs (slug, item_type, version, installed_at, mcp_server_id) \
         VALUES (?1, 'skill', ?2, ?3, NULL)",
        rusqlite::params![slug, hash, chrono::Utc::now().timestamp()],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn read_install_version(conn: &rusqlite::Connection, slug: &str) -> Option<String> {
    conn.query_row(
        "SELECT version FROM marketplace_standalone_installs WHERE slug = ?1 AND item_type = 'skill'",
        rusqlite::params![slug],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn remove_install_row(conn: &rusqlite::Connection, slug: &str) -> Result<(), MarketplaceError> {
    conn.execute(
        "DELETE FROM marketplace_standalone_installs WHERE slug = ?1 AND item_type = 'skill'",
        rusqlite::params![slug],
    )
    .map_err(db_err)?;
    Ok(())
}

/// True when not installed or the stored hash differs from `latest_hash`.
pub fn needs_update(conn: &rusqlite::Connection, slug: &str, latest_hash: &str) -> bool {
    read_install_version(conn, slug).as_deref() != Some(latest_hash)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::skills_marketplace::SkillFile;
    use tempfile::TempDir;

    pub(crate) fn detail_with(files: &[(&str, &str)]) -> SkillDetail {
        SkillDetail {
            id: "o/r/s".into(),
            source: "o/r".into(),
            slug: "o__r__s".into(),
            hash: "h1".into(),
            files: files.iter().map(|(p, c)| SkillFile { path: (*p).into(), contents: (*c).into() }).collect(),
        }
    }

    fn detail() -> SkillDetail {
        detail_with(&[("SKILL.md", "---\nname: s\n---\nbody")])
    }

    pub(crate) fn v25_conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE marketplace_standalone_installs (slug TEXT PRIMARY KEY, item_type TEXT NOT NULL, version TEXT NOT NULL, installed_at INTEGER NOT NULL, mcp_server_id TEXT);",
        )
        .unwrap();
        c
    }

    #[test]
    fn slug_flattens_id() {
        assert_eq!(flatten_slug("expo/skills/react-native"), "expo__skills__react-native");
    }

    #[test]
    fn roots_are_where_the_registry_looks() {
        let d = Path::new("/data");
        assert_eq!(global_root(d), Path::new("/data/skills/_marketplace"));
        assert_eq!(workspace_root(Path::new("/ws")), Path::new("/ws/.uclaw/skills"));
    }

    #[test]
    fn global_install_writes_files_under_marketplace() {
        let tmp = TempDir::new().unwrap();
        let root = global_root(tmp.path());
        let dir = write_skill_files(&root, "o__r__s", &detail()).unwrap();
        assert_eq!(dir, root.join("o__r__s"));
        assert_eq!(std::fs::read_to_string(dir.join("SKILL.md")).unwrap(), "---\nname: s\n---\nbody");
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let d = detail_with(&[("SKILL.md", "x"), ("../evil.md", "x")]);
        assert!(write_skill_files(tmp.path(), "o__r__s", &d).is_err());
        assert!(!tmp.path().join("evil.md").exists());
        assert!(!tmp.path().join("o__r__s").exists(), "nothing written when any path is bad");
    }

    #[test]
    fn rejects_dot_slug_and_windows_abs_and_backslash() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        assert!(write_skill_files(root, ".", &detail()).is_err());
        assert!(write_skill_files(root, "..", &detail()).is_err());
        assert!(write_skill_files(root, "a/b", &detail()).is_err());
        assert!(write_skill_files(root, "x1", &detail_with(&[("C:/evil.md", "x")])).is_err());
        assert!(write_skill_files(root, "x2", &detail_with(&[("/etc/evil.md", "x")])).is_err());
        assert!(write_skill_files(root, "x3", &detail_with(&[("examples\\evil.md", "x")])).is_err());
        assert!(write_skill_files(root, "x4", &detail_with(&[("examples/app.ts", "x")])).is_ok(), "subdirs allowed");
    }

    #[test]
    fn reinstall_drops_files_removed_upstream() {
        let tmp = TempDir::new().unwrap();
        write_skill_files(tmp.path(), "s", &detail_with(&[("SKILL.md", "v1"), ("a.md", "old")])).unwrap();
        let dir = write_skill_files(tmp.path(), "s", &detail_with(&[("SKILL.md", "v2")])).unwrap();
        assert!(!dir.join("a.md").exists(), "file removed upstream must not linger");
        assert_eq!(std::fs::read_to_string(dir.join("SKILL.md")).unwrap(), "v2");
    }

    #[test]
    fn workspace_install_writes_a_real_copy() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let dir = write_skill_files(&workspace_root(&ws), "o__r__s", &detail()).unwrap();
        let md = dir.join("SKILL.md");
        assert!(md.is_file());
        assert!(!std::fs::symlink_metadata(&dir).unwrap().file_type().is_symlink());
        assert!(dir.starts_with(ws.join(".uclaw").join("skills")));
    }

    #[test]
    fn remove_skill_dir_deletes_install_and_guards_slug() {
        let tmp = TempDir::new().unwrap();
        let dir = write_skill_files(tmp.path(), "o__r__s", &detail()).unwrap();
        let (removed, existed) = remove_skill_dir(tmp.path(), "o__r__s").unwrap();
        assert_eq!(removed, dir);
        assert!(existed);
        assert!(!dir.exists());
        let (_, again) = remove_skill_dir(tmp.path(), "o__r__s").unwrap();
        assert!(!again, "idempotent");
        assert!(remove_skill_dir(tmp.path(), "..").is_err());
        assert!(remove_skill_dir(tmp.path(), "a/b").is_err());
    }

    #[test]
    fn records_and_reads_install_row() {
        let c = v25_conn();
        assert!(needs_update(&c, "s", "h1"), "not installed → needs install");
        record_install(&c, "s", "h1").unwrap();
        assert_eq!(read_install_version(&c, "s").as_deref(), Some("h1"));
        assert!(!needs_update(&c, "s", "h1"));
        assert!(needs_update(&c, "s", "h2"));
        record_install(&c, "s", "h2").unwrap();
        assert_eq!(read_install_version(&c, "s").as_deref(), Some("h2"), "re-record refreshes");
        remove_install_row(&c, "s").unwrap();
        assert_eq!(read_install_version(&c, "s"), None);
    }

    #[test]
    fn remove_install_row_leaves_non_skill_rows() {
        let c = v25_conn();
        c.execute(
            "INSERT INTO marketplace_standalone_installs VALUES ('mcp-x', 'mcp_server', '1', 0, 'srv')",
            [],
        )
        .unwrap();
        remove_install_row(&c, "mcp-x").unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM marketplace_standalone_installs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
