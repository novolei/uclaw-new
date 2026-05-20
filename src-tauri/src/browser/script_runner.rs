use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BrowserScriptError {
    #[error("expected_js_file")]
    ExpectedJsFile,
    #[error("path_not_allowed: {0}")]
    PathNotAllowed(String),
}

#[derive(Debug, Clone)]
pub struct ScriptPathPolicy {
    builtin_root: PathBuf,
    workspace_root: PathBuf,
    home_dir: PathBuf,
}

impl ScriptPathPolicy {
    pub fn new(builtin_root: PathBuf, workspace_root: PathBuf, home_dir: PathBuf) -> Self {
        Self {
            builtin_root: normalize_path(&builtin_root),
            workspace_root: normalize_path(&workspace_root),
            home_dir: normalize_path(&home_dir),
        }
    }

    pub fn resolve(&self, file: &str) -> Result<PathBuf, BrowserScriptError> {
        let raw = Path::new(file);
        let resolved = if raw.is_absolute() {
            normalize_path(raw)
        } else if file.starts_with("douyin/") || file.starts_with("shared/") {
            normalize_path(self.builtin_root.join(file))
        } else {
            normalize_path(self.workspace_root.join(file))
        };
        if resolved.extension().and_then(|e| e.to_str()) != Some("js") {
            return Err(BrowserScriptError::ExpectedJsFile);
        }
        if is_under(&resolved, &self.builtin_root)
            || is_under(&resolved, &self.workspace_root)
            || is_allowed_skill_path(&resolved, &self.workspace_root, &self.home_dir)
        {
            return Ok(resolved);
        }
        Err(BrowserScriptError::PathNotAllowed(
            resolved.display().to_string(),
        ))
    }
}

fn is_under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn is_allowed_skill_path(path: &Path, workspace_root: &Path, home_dir: &Path) -> bool {
    let text = path.to_string_lossy();
    let marker = "/.claude/skills/";
    let Some(idx) = text.find(marker) else {
        return false;
    };
    let root = normalize_path(Path::new(&text[..idx]));
    root == home_dir || workspace_root == root || workspace_root.starts_with(root)
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_builtin_adapter_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy.resolve("douyin/scan_comments.js").unwrap();
        assert!(resolved.ends_with("douyin/scan_comments.js"));
    }

    #[test]
    fn allows_workspace_relative_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy.resolve("./scripts/live.js").unwrap();
        assert_eq!(
            resolved,
            std::path::PathBuf::from("/workspace/scripts/live.js")
        );
    }

    #[test]
    fn rejects_absolute_path_outside_allowed_roots() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy.resolve("/tmp/steal.js").unwrap_err().to_string();
        assert!(err.contains("path_not_allowed"));
    }

    #[test]
    fn rejects_non_js_files() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy
            .resolve("./scripts/live.txt")
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected_js_file"));
    }

    #[test]
    fn rejects_parent_traversal_outside_workspace() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy.resolve("../tmp/steal.js").unwrap_err().to_string();
        assert!(err.contains("path_not_allowed"));
    }
}
