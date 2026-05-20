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
}

impl ScriptPathPolicy {
    pub fn new(builtin_root: PathBuf, workspace_root: PathBuf, _home_dir: PathBuf) -> Self {
        Self {
            builtin_root: canonical_or_normalized(&builtin_root),
            workspace_root: canonical_or_normalized(&workspace_root),
        }
    }

    pub fn resolve(&self, file: &str) -> Result<PathBuf, BrowserScriptError> {
        let raw = Path::new(file);
        let resolved = if raw.is_absolute() {
            canonical_or_normalized(raw)
        } else if file.starts_with("douyin/") || file.starts_with("shared/") {
            canonical_or_normalized(self.builtin_root.join(file))
        } else {
            canonical_or_normalized(self.workspace_root.join(file))
        };
        if resolved.extension().and_then(|e| e.to_str()) != Some("js") {
            return Err(BrowserScriptError::ExpectedJsFile);
        }
        if self.allowed_roots().iter().any(|root| is_under(&resolved, root)) {
            return Ok(resolved);
        }
        Err(BrowserScriptError::PathNotAllowed(
            resolved.display().to_string(),
        ))
    }

    fn allowed_roots(&self) -> Vec<PathBuf> {
        vec![
            self.builtin_root.clone(),
            canonical_or_normalized(self.workspace_root.join(".claude/skills")),
            canonical_or_normalized(self.workspace_root.join(".uclaw/skills")),
        ]
    }
}

fn is_under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
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

fn canonical_or_normalized(path: impl AsRef<Path>) -> PathBuf {
    std::fs::canonicalize(path.as_ref()).unwrap_or_else(|_| normalize_path(path))
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
    fn rejects_workspace_relative_script_outside_allowed_roots() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy.resolve("./scripts/live.js").unwrap_err().to_string();
        assert!(err.contains("path_not_allowed"));
    }

    #[test]
    fn allows_workspace_claude_skill_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy
            .resolve(".claude/skills/bili-get-messages/index.js")
            .unwrap();
        assert_eq!(
            resolved,
            std::path::PathBuf::from("/workspace/.claude/skills/bili-get-messages/index.js")
        );
    }

    #[test]
    fn allows_workspace_uclaw_skill_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy
            .resolve(".uclaw/skills/bili-get-messages/index.js")
            .unwrap();
        assert_eq!(
            resolved,
            std::path::PathBuf::from("/workspace/.uclaw/skills/bili-get-messages/index.js")
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

    #[test]
    fn rejects_skill_path_traversal_escape() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy
            .resolve(".claude/skills/../../secret.js")
            .unwrap_err()
            .to_string();
        assert!(err.contains("path_not_allowed"));
    }
}
