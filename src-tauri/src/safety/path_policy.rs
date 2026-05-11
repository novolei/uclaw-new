//! Path-aware sandboxing for agent tool calls.
//!
//! Decides whether a tool argument that names a filesystem path may be
//! accessed without a user prompt. Whitelist sources, in priority order:
//!
//! 1. Active workspace's `path` (e.g. `~/Documents/workground/2222`)
//! 2. Workspace-level `attached_dirs` (Phase 2 `spaces.attached_dirs`)
//! 3. Session-level `attached_dirs` (Phase 2 `agent_sessions.attached_dirs`)
//! 4. Global `always_allowed` (persisted in `~/.uclaw/path_policy.json`)
//! 5. Session-scoped grants from the approval modal (in-memory only)
//!
//! Anything else → `PathDecision::Prompt`. Decision is centralized in
//! `PathPolicy::check`; the SafetyManager wraps this and the dispatcher
//! calls SafetyManager. Tools themselves don't change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PathDecision {
    Allow,
    Prompt { reason: String },
    Block { reason: String },
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPolicyPersisted {
    #[serde(default)]
    pub global_allowed: Vec<PathBuf>,
}

pub struct PathPolicy {
    global_allowed: Vec<PathBuf>,
    /// Keyed by session_id. Cleared on process restart.
    session_allowed: HashMap<String, Vec<PathBuf>>,
}

impl PathPolicy {
    pub fn empty() -> Self {
        Self {
            global_allowed: Vec::new(),
            session_allowed: HashMap::new(),
        }
    }

    pub fn from_persisted(p: PathPolicyPersisted) -> Self {
        Self {
            global_allowed: p.global_allowed,
            session_allowed: HashMap::new(),
        }
    }

    pub fn to_persisted(&self) -> PathPolicyPersisted {
        PathPolicyPersisted { global_allowed: self.global_allowed.clone() }
    }

    pub fn list_global(&self) -> &[PathBuf] { &self.global_allowed }

    pub fn add_global(&mut self, p: PathBuf) {
        if !self.global_allowed.iter().any(|x| x == &p) {
            self.global_allowed.push(p);
        }
    }

    pub fn remove_global(&mut self, p: &Path) {
        self.global_allowed.retain(|x| x.as_path() != p);
    }

    pub fn list_for_session(&self, sid: &str) -> Vec<PathBuf> {
        self.session_allowed.get(sid).cloned().unwrap_or_default()
    }

    pub fn allow_for_session(&mut self, sid: &str, p: PathBuf) {
        let entry = self.session_allowed.entry(sid.to_string()).or_default();
        if !entry.iter().any(|x| x == &p) {
            entry.push(p);
        }
    }

    pub fn promote_session_to_global(&mut self, sid: &str, p: &Path) {
        if let Some(entry) = self.session_allowed.get_mut(sid) {
            entry.retain(|x| x.as_path() != p);
            if entry.is_empty() {
                self.session_allowed.remove(sid);
            }
        }
        self.add_global(p.to_path_buf());
    }

    /// Return Allow if `candidate` lies inside any whitelist entry, else Prompt.
    pub fn check(
        &self,
        session_id: &str,
        workspace_root: &Path,
        workspace_attached: &[PathBuf],
        session_attached: &[PathBuf],
        candidate: &Path,
    ) -> PathDecision {
        if is_under(candidate, workspace_root) {
            return PathDecision::Allow;
        }
        for dir in workspace_attached.iter().chain(session_attached.iter()) {
            if is_under(candidate, dir) {
                return PathDecision::Allow;
            }
        }
        for dir in &self.global_allowed {
            if is_under(candidate, dir) {
                return PathDecision::Allow;
            }
        }
        if let Some(sess) = self.session_allowed.get(session_id) {
            for dir in sess {
                if is_under(candidate, dir) {
                    return PathDecision::Allow;
                }
            }
        }
        PathDecision::Prompt {
            reason: format!(
                "Path '{}' is outside the active workspace and not in any allowed directory",
                candidate.display()
            ),
        }
    }
}

/// Return true if `candidate` equals `root` or is contained inside it.
/// Canonicalize both sides when they exist on disk (this follows symlinks,
/// preventing in-workspace symlinks to /etc from bypassing the check).
/// Fall back to lexical normalization that resolves `..` segments when
/// either side doesn't exist yet (e.g. `write_file` to a new path).
pub(crate) fn is_under(candidate: &Path, root: &Path) -> bool {
    let r_canonical = root.canonicalize();
    let cand_canonical = candidate.canonicalize();

    // If both exist and are canonicalizable, use canonical forms
    if let (Ok(r_c), Ok(c_c)) = (&r_canonical, &cand_canonical) {
        return c_c.starts_with(r_c);
    }

    // If root exists but candidate doesn't, check the normalized versions
    if let Ok(_) = r_canonical {
        let cand_norm = normalize(candidate);
        let r_norm = normalize(root);

        // Check if the normalized paths match
        if cand_norm.starts_with(&r_norm) {
            return true;
        }

        // If not, they don't match
        return false;
    }

    // If root doesn't exist, just lexically normalize both
    let r_norm = normalize(root);
    let cand_norm = normalize(candidate);
    cand_norm.starts_with(&r_norm)
}

fn normalize(p: &Path) -> PathBuf {
    // Lexical normalize: resolve `.` and `..` without touching disk.
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            Prefix(p) => out.push(p.as_os_str()),
            RootDir => out.push("/"),
            CurDir => {}
            ParentDir => { out.pop(); }
            Normal(n) => out.push(n),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn is_under_same_path_is_true() {
        let dir = TempDir::new().unwrap();
        assert!(is_under(dir.path(), dir.path()));
    }

    #[test]
    fn is_under_child_path_is_true() {
        let dir = TempDir::new().unwrap();
        let child = dir.path().join("sub").join("file.txt");
        std::fs::create_dir_all(child.parent().unwrap()).unwrap();
        std::fs::write(&child, "x").unwrap();
        assert!(is_under(&child, dir.path()));
    }

    #[test]
    fn is_under_dotdot_escape_is_false() {
        let dir = TempDir::new().unwrap();
        let escape = dir.path().join("..").join("escaped.txt");
        // Doesn't need to exist; lexical normalize resolves the `..`.
        assert!(!is_under(&escape, dir.path()));
    }

    #[test]
    fn is_under_sibling_dir_is_false() {
        let dir = TempDir::new().unwrap();
        let sibling = dir.path().parent().unwrap().join("not-the-workspace");
        assert!(!is_under(&sibling, dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn is_under_symlink_to_outside_is_false() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let link = dir.path().join("escape-link");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        // Symlink itself sits inside the workspace, but the target it
        // resolves to does not — canonicalize should follow it.
        assert!(!is_under(&link, dir.path()));
    }

    #[test]
    fn check_inside_workspace_allows() {
        let p = PathPolicy::empty();
        let ws = TempDir::new().unwrap();
        let inside = ws.path().join("a.txt");
        std::fs::write(&inside, "x").unwrap();
        assert_eq!(
            p.check("sess1", ws.path(), &[], &[], &inside),
            PathDecision::Allow,
        );
    }

    #[test]
    fn check_inside_attached_dir_allows() {
        let p = PathPolicy::empty();
        let ws = TempDir::new().unwrap();
        let attached = TempDir::new().unwrap();
        let target = attached.path().join("b.txt");
        std::fs::write(&target, "x").unwrap();
        assert_eq!(
            p.check("sess1", ws.path(), &[attached.path().to_path_buf()], &[], &target),
            PathDecision::Allow,
        );
    }

    #[test]
    fn check_outside_all_prompts() {
        let p = PathPolicy::empty();
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap().path().join("c.txt");
        match p.check("sess1", ws.path(), &[], &[], &outside) {
            PathDecision::Prompt { reason } => {
                assert!(reason.contains("outside the active workspace"));
            }
            other => panic!("expected Prompt, got {:?}", other),
        }
    }

    #[test]
    fn check_after_session_grant_allows() {
        let mut p = PathPolicy::empty();
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        p.allow_for_session("sess1", outside.path().to_path_buf());
        let candidate = outside.path().join("d.txt");
        assert_eq!(
            p.check("sess1", ws.path(), &[], &[], &candidate),
            PathDecision::Allow,
        );
        // Other session is unaffected.
        assert!(matches!(
            p.check("sess2", ws.path(), &[], &[], &candidate),
            PathDecision::Prompt { .. },
        ));
    }

    #[test]
    fn promote_session_to_global_clears_session_and_adds_global() {
        let mut p = PathPolicy::empty();
        let outside = TempDir::new().unwrap();
        p.allow_for_session("sess1", outside.path().to_path_buf());
        p.promote_session_to_global("sess1", outside.path());
        assert!(p.list_for_session("sess1").is_empty());
        assert_eq!(p.list_global(), &[outside.path().to_path_buf()]);
        // Any session now sees it.
        let ws = TempDir::new().unwrap();
        let candidate = outside.path().join("e.txt");
        assert_eq!(
            p.check("sess2", ws.path(), &[], &[], &candidate),
            PathDecision::Allow,
        );
    }
}
