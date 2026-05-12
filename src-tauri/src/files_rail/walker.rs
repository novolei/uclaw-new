//! Single-layer directory reader.
//!
//! Returns one level of `FileNode`s at a time. Deep recursion happens in the
//! frontend via lazy-expand; the backend never walks more than one directory
//! per call.

use super::ignore::should_ignore;
use super::types::{FileNode, NodeKind};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Read one level of entries under `dir`, returning a sorted (dirs first,
/// then files, both alpha) list of non-ignored entries.
///
/// `mount_root` is used to compute `rel_path` for each entry.
pub fn read_dir_layer(dir: &Path, mount_root: &Path) -> Result<Vec<FileNode>, std::io::Error> {
    let mut entries: Vec<FileNode> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue, // permission denied etc — skip silently
        };
        let name = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue, // non-UTF-8 name; skip
        };
        let is_dir = metadata.is_dir();
        if should_ignore(&name, is_dir) {
            continue;
        }
        let rel_path = path
            .strip_prefix(mount_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        entries.push(FileNode {
            path: path.clone(),
            rel_path,
            name,
            kind: if is_dir {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            size: if is_dir { 0 } else { metadata.len() },
            mtime_ms,
            is_ignored: false,
        });
    }
    entries.sort_by(|a, b| match (a.kind, b.kind) {
        (NodeKind::Directory, NodeKind::File) => std::cmp::Ordering::Less,
        (NodeKind::File, NodeKind::Directory) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir, write};
    use tempfile::TempDir;

    fn fixture() -> TempDir {
        let dir = TempDir::new().unwrap();
        create_dir(dir.path().join("src")).unwrap();
        create_dir(dir.path().join("node_modules")).unwrap();
        create_dir(dir.path().join(".git")).unwrap();
        create_dir(dir.path().join(".hidden")).unwrap();
        write(dir.path().join("README.md"), b"hi").unwrap();
        write(dir.path().join("a.txt"), b"a").unwrap();
        write(dir.path().join(".env"), b"FOO=1").unwrap();
        dir
    }

    #[test]
    fn read_dir_layer_returns_visible_entries_only() {
        let fx = fixture();
        let entries = read_dir_layer(fx.path(), fx.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"src"));
        assert!(names.contains(&"README.md"));
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&".env"), "explicit .env allowlist");
        assert!(!names.contains(&"node_modules"));
        assert!(!names.contains(&".git"));
        assert!(!names.contains(&".hidden"));
    }

    #[test]
    fn read_dir_layer_sorts_dirs_first_then_alpha() {
        let fx = fixture();
        let entries = read_dir_layer(fx.path(), fx.path()).unwrap();
        // First entry must be the only surviving directory.
        assert_eq!(entries[0].name, "src");
        assert_eq!(entries[0].kind, NodeKind::Directory);
        // Files after it, alphabetically.
        let file_names: Vec<&str> = entries[1..].iter().map(|e| e.name.as_str()).collect();
        let mut sorted = file_names.clone();
        sorted.sort_by_key(|s| s.to_lowercase());
        assert_eq!(file_names, sorted);
    }

    #[test]
    fn read_dir_layer_computes_relative_path() {
        // Add a file to src/ so we can assert its rel_path is forward-slashed
        // and prefixed by the subdir name.
        let fx = fixture();
        std::fs::write(fx.path().join("src").join("main.rs"), b"fn main() {}").unwrap();

        let entries = read_dir_layer(&fx.path().join("src"), fx.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let node = &entries[0];
        assert_eq!(node.name, "main.rs");
        assert_eq!(node.rel_path, "src/main.rs", "rel_path must be forward-slashed and prefixed by 'src/'");
    }

    #[test]
    fn should_ignore_dotfiles_except_allowlist() {
        assert!(should_ignore(".cache", true));
        assert!(should_ignore(".hidden", true));
        assert!(should_ignore(".something", false));
        assert!(!should_ignore(".env", false));
        assert!(!should_ignore(".gitignore", false));
    }

    #[test]
    fn should_ignore_skip_dirs() {
        assert!(should_ignore("node_modules", true));
        assert!(should_ignore("target", true));
        // Same name as a SKIP_DIR but as a file → allowed
        assert!(!should_ignore("node_modules", false));
    }
}
