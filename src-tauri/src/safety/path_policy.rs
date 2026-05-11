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
///
/// Strategy: canonicalize `root`; canonicalize the deepest existing
/// ancestor of `candidate` and re-attach the non-existent suffix. This
/// (a) handles macOS `/var` → `/private/var` symlink prefix when the leaf
/// doesn't exist yet, and (b) blocks bypass via in-workspace symlinks
/// (e.g. agent creates `<root>/escape → /etc` then writes
/// `<root>/escape/passwd`): the existing `escape` ancestor canonicalizes
/// out to `/etc`, the synthetic candidate `/etc/passwd` no longer starts
/// with the canonical root.
pub(crate) fn is_under(candidate: &Path, root: &Path) -> bool {
    let r = canonicalize_existing_prefix(root);
    let c = canonicalize_existing_prefix(candidate);
    c.starts_with(&r)
}

/// Canonicalize the deepest existing ancestor of `p` and re-attach the
/// non-existent leaf segments (in their original order). If no ancestor
/// exists, falls back to pure lexical normalization (resolves `.` / `..`).
fn canonicalize_existing_prefix(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    let mut current = p.to_path_buf();
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if current.exists() {
            break;
        }
        match current.file_name() {
            Some(n) => {
                suffix.push(n.to_os_string());
                if !current.pop() {
                    break;
                }
            }
            None => break,
        }
    }
    let mut base = if current.as_os_str().is_empty() {
        // No existing prefix at all — fall back to lexical normalize.
        return normalize_lexical(p);
    } else {
        current.canonicalize().unwrap_or_else(|_| normalize_lexical(&current))
    };
    for n in suffix.into_iter().rev() {
        base.push(n);
    }
    base
}

/// Pure lexical `.`/`..` resolution. Used only when the path has no
/// existing ancestor at all (e.g. a totally synthetic test path).
fn normalize_lexical(p: &Path) -> PathBuf {
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

    #[cfg(unix)]
    #[test]
    fn is_under_inworkspace_symlink_escape_blocks_nonexistent_leaf() {
        // Agent creates an in-workspace symlink that points outside, then
        // tries to write through it via a non-existent leaf. Even though
        // the candidate's leaf doesn't exist (so naive lexical check would
        // pass), canonicalizing the *existing* parent reveals the escape.
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let escape_link = workspace.path().join("escape");
        std::os::unix::fs::symlink(outside.path(), &escape_link).unwrap();
        let candidate = escape_link.join("passwd");
        // candidate's leaf "passwd" doesn't exist, but "escape" (its parent)
        // does and resolves outside the workspace.
        assert!(!is_under(&candidate, workspace.path()));
    }

    #[test]
    fn is_under_nonexistent_leaf_under_real_root_allows() {
        // The macOS /var → /private/var quirk case: workspace is real but
        // candidate doesn't exist yet. Should still match by canonicalizing
        // the existing parent.
        let workspace = TempDir::new().unwrap();
        let candidate = workspace.path().join("newfile.txt");
        assert!(is_under(&candidate, workspace.path()));
    }
}
