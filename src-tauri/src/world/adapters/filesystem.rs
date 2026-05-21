//! Filesystem adapter — scan a directory, upsert `WorldEntity`s.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Options for [`scan_directory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// Recurse into sub-directories. When `false`, only the top-level
    /// entries of `root` are visited.
    pub recursive: bool,
    /// Hard cap on the number of entities the scan produces. `0` =
    /// no cap. Protects against runaway scans of very large trees.
    pub max_entities: usize,
    /// Skip entries whose name starts with `.` (dotfiles). Convenient
    /// default for source trees.
    pub skip_hidden: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            max_entities: 10_000,
            skip_hidden: true,
        }
    }
}

/// Result returned by `scan_directory` — entities found + reason
/// for stopping (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub entities: Vec<WorldEntity>,
    pub hit_max_entities: bool,
    pub entries_skipped_hidden: usize,
}

/// Walk `root` and produce one `WorldEntity` per file/directory.
///
/// Pure function — does NOT call into `ProjectionStore`. The wrapper
/// `FileSystemAdapter::scan_and_upsert` (below) handles the store
/// integration. Pure form makes the scan unit-testable without a
/// store.
///
/// Entity properties populated:
/// - `size` (number) for files
/// - `is_dir` (bool)
/// - `extension` (string, when present)
///
/// `observed_at` is the caller-supplied timestamp so tests stay
/// deterministic.
pub fn scan_directory<P: AsRef<Path>>(
    root: P,
    options: &ScanOptions,
    observed_at: &str,
) -> ScanResult {
    let mut entities = Vec::new();
    let mut entries_skipped_hidden = 0usize;
    let mut hit_max_entities = false;

    let mut stack: Vec<PathBuf> = vec![root.as_ref().to_path_buf()];

    'outer: while let Some(dir) = stack.pop() {
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue, // unreadable dir → skip silently
        };
        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if options.skip_hidden && name.starts_with('.') {
                entries_skipped_hidden += 1;
                continue;
            }
            if options.max_entities > 0 && entities.len() >= options.max_entities {
                hit_max_entities = true;
                break 'outer;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let is_dir = meta.is_dir();
            let size = meta.len();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string());
            let mut state = WorldEntityState::fresh(observed_at)
                .with_property("size", json!(size))
                .with_property("is_dir", json!(is_dir));
            if let Some(e) = ext {
                state = state.with_property("extension", json!(e));
            }
            entities.push(WorldEntity::new(
                EntityRef::new(format!("file:{}", path.display())),
                WorldEntityKind::File,
                state,
            ));
            if is_dir && options.recursive {
                stack.push(path);
            }
        }
    }

    ScanResult {
        entities,
        hit_max_entities,
        entries_skipped_hidden,
    }
}

/// Wrapper that hands the scan output to a [`ProjectionStore`].
pub struct FileSystemAdapter {
    store: ProjectionStore,
}

impl FileSystemAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    /// Scan `root` and upsert every produced entity into the store.
    /// Returns the same `ScanResult` for caller observability.
    pub async fn scan_and_upsert<P: AsRef<Path>>(
        &self,
        root: P,
        options: &ScanOptions,
        observed_at: &str,
    ) -> ScanResult {
        let result = scan_directory(root, options, observed_at);
        for entity in &result.entities {
            self.store.upsert(entity.clone()).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    // ── ScanOptions ────────────────────────────────────────────────

    #[test]
    fn default_options_match_spec() {
        let o = ScanOptions::default();
        assert!(o.recursive);
        assert_eq!(o.max_entities, 10_000);
        assert!(o.skip_hidden);
    }

    // ── empty directory ────────────────────────────────────────────

    #[test]
    fn scan_empty_dir_returns_empty() {
        let tmp = make_tempdir();
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "t0");
        assert!(r.entities.is_empty());
        assert!(!r.hit_max_entities);
        assert_eq!(r.entries_skipped_hidden, 0);
    }

    // ── flat directory ─────────────────────────────────────────────

    #[test]
    fn scan_flat_dir_produces_one_entity_per_entry() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("a.txt"), "hello");
        write_file(&tmp.path().join("b.rs"), "fn main() {}");
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "t0");
        assert_eq!(r.entities.len(), 2);
        // Each entity has kind = File.
        for e in &r.entities {
            assert_eq!(e.kind, WorldEntityKind::File);
        }
        // Extension property captured.
        let ents: std::collections::HashMap<_, _> = r
            .entities
            .iter()
            .map(|e| {
                let name = e
                    .r#ref
                    .id
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .to_string();
                (name, e)
            })
            .collect();
        let a = ents.get("a.txt").unwrap();
        assert_eq!(a.state.properties.get("extension"), Some(&json!("txt")));
        assert_eq!(a.state.properties.get("is_dir"), Some(&json!(false)));
        let b = ents.get("b.rs").unwrap();
        assert_eq!(b.state.properties.get("extension"), Some(&json!("rs")));
    }

    // ── recursive vs non-recursive ─────────────────────────────────

    #[test]
    fn non_recursive_skips_nested() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("a.txt"), "x");
        write_file(&tmp.path().join("sub/b.txt"), "y");
        let mut opts = ScanOptions::default();
        opts.recursive = false;
        let r = scan_directory(tmp.path(), &opts, "t0");
        // Top-level: a.txt + sub (the dir).
        assert_eq!(r.entities.len(), 2);
        let names: Vec<&str> = r
            .entities
            .iter()
            .map(|e| e.r#ref.id.rsplit('/').next().unwrap())
            .collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        // b.txt NOT scanned.
        assert!(!names.contains(&"b.txt"));
    }

    #[test]
    fn recursive_descends() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("a.txt"), "x");
        write_file(&tmp.path().join("sub/b.txt"), "y");
        write_file(&tmp.path().join("sub/deep/c.txt"), "z");
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "t0");
        // a.txt + sub (dir) + b.txt + deep (dir) + c.txt = 5
        assert_eq!(r.entities.len(), 5);
    }

    // ── hidden files ───────────────────────────────────────────────

    #[test]
    fn skip_hidden_default_skips_dotfiles() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("visible.txt"), "x");
        write_file(&tmp.path().join(".secret"), "y");
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "t0");
        assert_eq!(r.entities.len(), 1);
        assert_eq!(r.entries_skipped_hidden, 1);
    }

    #[test]
    fn skip_hidden_off_includes_dotfiles() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("visible.txt"), "x");
        write_file(&tmp.path().join(".secret"), "y");
        let mut opts = ScanOptions::default();
        opts.skip_hidden = false;
        let r = scan_directory(tmp.path(), &opts, "t0");
        assert_eq!(r.entities.len(), 2);
        assert_eq!(r.entries_skipped_hidden, 0);
    }

    // ── max_entities cap ───────────────────────────────────────────

    #[test]
    fn max_entities_caps_output() {
        let tmp = make_tempdir();
        for i in 0..20 {
            write_file(&tmp.path().join(format!("f{i}.txt")), "x");
        }
        let mut opts = ScanOptions::default();
        opts.max_entities = 5;
        let r = scan_directory(tmp.path(), &opts, "t0");
        assert_eq!(r.entities.len(), 5);
        assert!(r.hit_max_entities);
    }

    // ── observed_at ────────────────────────────────────────────────

    #[test]
    fn observed_at_propagates_to_entity_state() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("a.txt"), "x");
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "2026-05-21T12:00:00Z");
        assert_eq!(r.entities[0].state.observed_at, "2026-05-21T12:00:00Z");
    }

    // ── FileSystemAdapter ─────────────────────────────────────────

    #[tokio::test]
    async fn adapter_scan_and_upsert_populates_store() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("a.txt"), "x");
        write_file(&tmp.path().join("sub/b.txt"), "y");
        let store = ProjectionStore::new("t0");
        let adapter = FileSystemAdapter::new(store.clone());
        let r = adapter
            .scan_and_upsert(tmp.path(), &ScanOptions::default(), "t0")
            .await;
        // 3 entities: a.txt + sub + b.txt
        assert_eq!(r.entities.len(), 3);
        let snap = store.snapshot();
        assert_eq!(snap.entities.len(), 3);
    }

    // ── ext-less files ─────────────────────────────────────────────

    #[test]
    fn extless_file_omits_extension_property() {
        let tmp = make_tempdir();
        write_file(&tmp.path().join("Makefile"), "x");
        let r = scan_directory(tmp.path(), &ScanOptions::default(), "t0");
        assert_eq!(r.entities.len(), 1);
        assert!(r.entities[0]
            .state
            .properties
            .get("extension")
            .is_none());
    }
}
