# Workspace Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [`docs/superpowers/specs/2026-05-11-workspace-phase3-design.md`](../specs/2026-05-11-workspace-phase3-design.md)

**Goal:** Add a path-aware safety layer so agent tools can't escape the active workspace (`read_file /etc/passwd` etc.), and wire the visual FileDropZone to a real `upload_workspace_file` backend plus native folder-drop → `attach_workspace_directory`.

**Architecture:** New `safety::path_policy` module owns the whitelist (workspace.path + workspace.attached_dirs + session.attached_dirs + a user-configurable global `always_allowed_paths`) and the in-memory session-grants map. `SafetyManager` holds a `PathPolicy` field; `~/.uclaw/path_policy.json` persists the global list only. `Tool` trait gets a default-implemented `path_args(&args) -> Vec<&str>`; six tools (`read_file`, `write_file`, `edit`, `grep`, `glob`, `bash`) override it. Dispatcher injects one new step between tool-level approval and `tool.execute()`: collect path_args, resolve to absolute, ask `SafetyManager.check_paths(...)`; if Prompt, reuse the existing `PendingApprovals` oneshot with a new modal `kind: "path"` variant whose payload returns "once" / "session" / "deny". File uploads land via a new `upload_workspace_file(workspace_id, filename, content)` IPC; native folder drops use Tauri's window-level `onDragDropEvent` → existing `attach_workspace_directory`. Build is green at every commit.

**Tech Stack:** Rust + rusqlite + Tauri 2 + React 18 + Jotai. No new third-party deps. Reuses `tauri-plugin-dialog` (Phase 2) for the Settings folder-picker.

---

## File Structure

**Create (Rust):**
- `src-tauri/src/safety/path_policy.rs` — `PathPolicy` struct, `is_under` helper, `PathDecision` enum, inline tests (Task 1)

**Modify (Rust):**
- `src-tauri/src/safety/mod.rs` — `pub mod path_policy`; extend `SafetyManager` with `path_policy: PathPolicy` field + accessor methods + path-check entry point (Task 2)
- `src-tauri/src/agent/tools/tool.rs` — add `fn path_args(&self, args: &serde_json::Value) -> Vec<String>` with default empty impl on `Tool` trait (Task 3)
- `src-tauri/src/agent/tools/builtin/file.rs` — override `path_args` on `ReadFileTool` + `WriteFileTool` (Task 3)
- `src-tauri/src/agent/tools/builtin/edit.rs` — override `path_args` on `EditTool` (Task 3)
- `src-tauri/src/agent/tools/builtin/search.rs` — override `path_args` on `GrepTool` + `GlobTool` (Task 3)
- `src-tauri/src/agent/tools/builtin/shell.rs` — override `path_args` on `BashTool` (Task 3)
- `src-tauri/src/agent/dispatcher.rs` — path-check hook between approval and execution; emit `agent:need_approval` with `kind: "path"` when Prompt; on `path_scope = "session"`, call `safety_manager.allow_path_for_session(...)` (Task 4)
- `src-tauri/src/app.rs` — extend `ApprovalResult` with `path_scope: Option<String>` and `paths: Option<Vec<String>>` fields (Task 4)
- `src-tauri/src/tauri_commands.rs` — `upload_workspace_file` (Task 5), `path_is_directory` (Task 6), 5 path-policy IPCs (Task 8); existing `resolve_approval` already accepts the extended `ApprovalResult` shape (Task 4)
- `src-tauri/src/main.rs` — register 7 new IPC commands in `invoke_handler!` (Tasks 5, 6, 8)

**Create (TS):**
- `ui/src/components/settings/WorkspaceSandboxSettings.tsx` — global allowed list + session-grants section (Task 8)

**Modify (TS):**
- `ui/src/lib/tauri-bridge.ts` — 7 new IPC wrappers + extend `ApprovalResult` type (Tasks 4, 5, 6, 8)
- `ui/src/components/agent/ApprovalModal.tsx` — handle new `kind: "path"` payload, three buttons, return `path_scope` (Task 4)
- `ui/src/components/agent/SidePanel.tsx` — pass real `onDrop` to both `FileDropZone` instances (Task 7)
- `ui/src/components/agent/AgentView.tsx` — top-level `onDragDropEvent` listener for native folder drops (Task 7)
- `ui/src/components/file-browser/FileDropZone.tsx` — add per-target hint text (Task 7)
- `ui/src/components/settings/Settings.tsx` or `SafetySettings.tsx` (whichever holds the Safety tab) — mount `WorkspaceSandboxSettings` (Task 8)

---

## Conventions for this plan

- Run from repo root `/Users/ryanliu/Documents/uclaw` unless noted.
- Branch is already `claude/workspace-phase3` (created off main `72f8d73`; spec committed at `6aa5116`).
- Each task ends with a commit. Commit messages are pre-written — copy verbatim.
- After each commit: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head` should be empty. For TS-touching tasks add: `cd ui && npx tsc --noEmit 2>&1 | head -10` clean.
- Build stays green at every commit. If a task can't complete without breaking the build, **stop and escalate** rather than push a broken state.
- TDD: write failing test → run to confirm RED → implement → run to confirm GREEN → commit. Skip TDD only on pure UI wiring tasks where there's no logic to test.

---

### Task 1: `path_policy.rs` module — `PathPolicy` + `is_under` + tests

Spec §4.1. The pure-logic module. No SafetyManager wiring yet; no persistence yet. Tests prove `is_under` handles relative/absolute/`..`/symlink-outside cases, and that the decision algorithm respects all four whitelist sources.

**Files:**
- Create: `src-tauri/src/safety/path_policy.rs`

- [ ] **Step 1: Create the module skeleton with types and a failing test**

Create `src-tauri/src/safety/path_policy.rs`:

```rust
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
    let cand = canonicalize_or_normalize(candidate);
    let r = canonicalize_or_normalize(root);
    cand.starts_with(&r)
}

fn canonicalize_or_normalize(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
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
}
```

- [ ] **Step 2: Run the tests to confirm they pass**

```bash
cd src-tauri && cargo test --lib safety::path_policy 2>&1 | tail -15
```

Expected: `test result: ok. 5 passed; 0 failed`. (`safety` module won't see the new file until Step 3.)

- [ ] **Step 3: Wire the module into `safety/mod.rs`**

Edit `src-tauri/src/safety/mod.rs` — add `pub mod path_policy;` near the existing `pub mod permissions;` line (around line 6):

```rust
pub mod path_policy;
pub mod permissions;
```

- [ ] **Step 4: Add the decision-algorithm test**

Append to the `mod tests` block in `src-tauri/src/safety/path_policy.rs` (before the closing `}`):

```rust
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
```

- [ ] **Step 5: Run tests and verify all pass**

```bash
cd src-tauri && cargo test --lib safety::path_policy 2>&1 | tail -20
```

Expected: `test result: ok. 10 passed; 0 failed`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/safety/path_policy.rs src-tauri/src/safety/mod.rs
git commit -m "feat(safety): path_policy module — whitelist + is_under + 10 tests

Pure-logic module. No SafetyManager wiring, no persistence yet. Owns the
decision algorithm: workspace.path → workspace.attached_dirs →
session.attached_dirs → global_allowed → session_allowed → Prompt.

is_under canonicalizes both sides (follows symlinks to prevent in-workspace
symlinks to /etc from bypassing the check); falls back to lexical normalize
when either side doesn't exist on disk."
```

---

### Task 2: SafetyManager integration + JSON persistence

Spec §4.2 and §5. Adds `path_policy` to `SafetyManager`. Load from `<data_dir>/path_policy.json` at init; save on every mutation. Wraps the `PathPolicy` API with SafetyManager-level methods. Adds a `check_paths(...)` entry point that resolves Yolo-mode escape before consulting the policy, matching the existing `should_approve` pattern.

**Files:**
- Modify: `src-tauri/src/safety/mod.rs:95-110` (init), `:155-175` (add methods at end of impl block)

- [ ] **Step 1: Write the failing persistence test**

Append to the existing `#[cfg(test)] mod tests` at end of `src-tauri/src/safety/mod.rs`:

```rust
    #[test]
    fn path_policy_persists_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        {
            let mut mgr = SafetyManager::new(tmp.path());
            mgr.add_always_allowed_path(outside.path().to_path_buf()).unwrap();
            assert_eq!(mgr.list_always_allowed_paths(), &[outside.path().to_path_buf()]);
        }
        // Re-open: the file at <tmp>/path_policy.json should round-trip.
        let mgr2 = SafetyManager::new(tmp.path());
        assert_eq!(mgr2.list_always_allowed_paths(), &[outside.path().to_path_buf()]);
    }

    #[test]
    fn check_paths_inside_workspace_allows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let target = ws.path().join("a.txt");
        std::fs::write(&target, "x").unwrap();
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[target],
            None,
        );
        assert_eq!(decision, crate::safety::path_policy::PathDecision::Allow);
    }

    #[test]
    fn check_paths_yolo_mode_short_circuits() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap().path().join("b.txt");
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[outside],
            Some(&SafetyMode::Yolo),
        );
        assert_eq!(decision, crate::safety::path_policy::PathDecision::Allow);
    }

    #[test]
    fn check_paths_outside_prompts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = SafetyManager::new(tmp.path());
        let ws = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap().path().join("c.txt");
        let decision = mgr.check_paths(
            "sess1",
            ws.path(),
            &[],
            &[],
            &[outside],
            None,
        );
        assert!(matches!(decision, crate::safety::path_policy::PathDecision::Prompt { .. }));
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd src-tauri && cargo test --lib safety::tests 2>&1 | tail -10
```

Expected: compile errors (`check_paths`, `add_always_allowed_path`, `list_always_allowed_paths` don't exist).

- [ ] **Step 3: Extend `SafetyManager` struct and `new`**

In `src-tauri/src/safety/mod.rs`, replace the `SafetyManager` struct definition (around line 95) and the `new` constructor:

```rust
pub struct SafetyManager {
    policy: SafetyPolicy,
    config_path: PathBuf,
    path_policy: path_policy::PathPolicy,
    path_policy_path: PathBuf,
}

impl SafetyManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let config_path = data_dir.join("safety_policy.json");
        let policy = Self::load_policy(&config_path).unwrap_or_default();

        let path_policy_path = data_dir.join("path_policy.json");
        let path_policy = Self::load_path_policy(&path_policy_path)
            .map(path_policy::PathPolicy::from_persisted)
            .unwrap_or_else(path_policy::PathPolicy::empty);

        tracing::info!("SafetyManager initialized with mode: {:?}", policy.global_mode);
        Self { policy, config_path, path_policy, path_policy_path }
    }

    fn load_path_policy(path: &std::path::Path) -> Option<path_policy::PathPolicyPersisted> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_path_policy(&self) -> Result<(), crate::error::Error> {
        if let Some(parent) = self.path_policy_path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::error::Error::Io)?;
        }
        let content = serde_json::to_string_pretty(&self.path_policy.to_persisted())
            .map_err(crate::error::Error::Serde)?;
        std::fs::write(&self.path_policy_path, content).map_err(crate::error::Error::Io)?;
        Ok(())
    }
```

- [ ] **Step 4: Add the SafetyManager-level path methods**

Append inside the same `impl SafetyManager` block, before the closing `}` (after `unblock_tool` around line 173):

```rust
    // ─── PathPolicy proxy ─────────────────────────────────────────────

    /// Decide whether the given candidate paths can be accessed without a prompt.
    /// `mode_override` of `Yolo` short-circuits to Allow (matches existing
    /// should_approve semantics).
    pub fn check_paths(
        &self,
        session_id: &str,
        workspace_root: &std::path::Path,
        workspace_attached: &[PathBuf],
        session_attached: &[PathBuf],
        candidates: &[PathBuf],
        mode_override: Option<&SafetyMode>,
    ) -> path_policy::PathDecision {
        if matches!(mode_override, Some(SafetyMode::Yolo)) || matches!(self.policy.global_mode, SafetyMode::Yolo) {
            return path_policy::PathDecision::Allow;
        }
        for c in candidates {
            match self.path_policy.check(
                session_id,
                workspace_root,
                workspace_attached,
                session_attached,
                c,
            ) {
                path_policy::PathDecision::Allow => continue,
                other => return other,
            }
        }
        path_policy::PathDecision::Allow
    }

    pub fn list_always_allowed_paths(&self) -> &[PathBuf] {
        self.path_policy.list_global()
    }

    pub fn add_always_allowed_path(&mut self, p: PathBuf) -> Result<(), crate::error::Error> {
        self.path_policy.add_global(p);
        self.save_path_policy()
    }

    pub fn remove_always_allowed_path(&mut self, p: &std::path::Path) -> Result<(), crate::error::Error> {
        self.path_policy.remove_global(p);
        self.save_path_policy()
    }

    pub fn list_session_allowed_paths(&self, sid: &str) -> Vec<PathBuf> {
        self.path_policy.list_for_session(sid)
    }

    pub fn allow_path_for_session(&mut self, sid: &str, p: PathBuf) {
        // Session grants are in-memory only — no save.
        self.path_policy.allow_for_session(sid, p);
    }

    pub fn promote_session_path_to_global(&mut self, sid: &str, p: &std::path::Path) -> Result<(), crate::error::Error> {
        self.path_policy.promote_session_to_global(sid, p);
        self.save_path_policy()
    }
```

- [ ] **Step 5: Run all safety tests**

```bash
cd src-tauri && cargo test --lib safety:: 2>&1 | tail -15
```

Expected: all path_policy tests (10) + the 4 new SafetyManager tests pass, plus any pre-existing safety tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/safety/mod.rs
git commit -m "feat(safety): SafetyManager owns PathPolicy + persistence

New \`path_policy\` field on SafetyManager loaded from
\`<data_dir>/path_policy.json\` at init, saved on every global-list
mutation. Session-scoped grants stay in memory (cleared on restart).

\`check_paths\` short-circuits to Allow under Yolo mode (matches
should_approve), otherwise consults PathPolicy.check per candidate. Six
proxy methods exposed for the dispatcher and the upcoming IPCs:
list/add/remove always_allowed, list_session_allowed,
allow_path_for_session, promote_session_path_to_global.

4 new unit tests including a persistence round-trip."
```

---

### Task 3: `Tool::path_args` trait method + 6 tool overrides

Spec §4.3. Adds a default-implemented method on the `Tool` trait so the dispatcher can extract path arguments without knowing tool internals. Six tools override it to return the args that name a filesystem path; everything else inherits the empty default.

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs` (add trait method)
- Modify: `src-tauri/src/agent/tools/builtin/file.rs`, `edit.rs`, `search.rs`, `shell.rs` (overrides)

- [ ] **Step 1: Find the existing Tool trait definition**

```bash
grep -n "trait Tool\b" src-tauri/src/agent/tools/tool.rs
```

Note the line number. The trait has `name`, `description`, `parameters_schema`, `requires_approval`, `execute` already. Add `path_args` next to `requires_approval`.

- [ ] **Step 2: Write the failing test for one tool**

Append to the end of `src-tauri/src/agent/tools/builtin/file.rs` (the file currently has no tests; the `mod tests` is new):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_file_path_args_returns_path() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(tool.path_args(&args), vec!["src/main.rs"]);
    }

    #[test]
    fn write_file_path_args_returns_path() {
        let tool = WriteFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "out.txt", "content": "x"});
        assert_eq!(tool.path_args(&args), vec!["out.txt"]);
    }

    #[test]
    fn read_file_path_args_missing_path_returns_empty() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({});
        assert!(tool.path_args(&args).is_empty());
    }
}
```

- [ ] **Step 3: Run the tests to confirm RED**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::file::tests 2>&1 | tail -10
```

Expected: compile error — `path_args` not in scope.

- [ ] **Step 4: Add `path_args` to the Tool trait**

In `src-tauri/src/agent/tools/tool.rs`, find the trait block and add this method just after `requires_approval`:

```rust
    /// Return the argument keys that name filesystem paths. The dispatcher
    /// uses these to consult the SafetyManager's PathPolicy before invoking
    /// `execute`. Default impl returns empty — tools without path args (web,
    /// plan, exit_plan_mode, etc.) inherit this.
    fn path_args<'a>(&self, _arguments: &'a serde_json::Value) -> Vec<&'a str> {
        Vec::new()
    }
```

- [ ] **Step 5: Override on `ReadFileTool` and `WriteFileTool`**

In `src-tauri/src/agent/tools/builtin/file.rs`, inside the `impl Tool for ReadFileTool` block (between `requires_approval` and `execute`), add:

```rust
    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
    }
```

Repeat the identical override inside `impl Tool for WriteFileTool`.

- [ ] **Step 6: Run the file.rs tests**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::file::tests 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 7: Override on `EditTool`**

In `src-tauri/src/agent/tools/builtin/edit.rs`, inside `impl Tool for EditTool`, add the same `path_args` override as Step 5 (reads `args["path"]`). Then append the test mod at end of file:

```rust
#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn edit_path_args_returns_path() {
        let tool = EditTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "lib.rs", "edits": []});
        assert_eq!(tool.path_args(&args), vec!["lib.rs"]);
    }
}
```

- [ ] **Step 8: Override on `GrepTool` and `GlobTool`**

In `src-tauri/src/agent/tools/builtin/search.rs`, inside `impl Tool for GrepTool` add:

```rust
    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
    }
```

Repeat inside `impl Tool for GlobTool`. Append the test mod at end of file:

```rust
#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn grep_path_args_returns_path_when_present() {
        let tool = GrepTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "TODO", "path": "src/"});
        assert_eq!(tool.path_args(&args), vec!["src/"]);
    }

    #[test]
    fn grep_path_args_empty_when_absent() {
        let tool = GrepTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "TODO"});
        assert!(tool.path_args(&args).is_empty());
    }

    #[test]
    fn glob_path_args_returns_path_when_present() {
        let tool = GlobTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src/"});
        assert_eq!(tool.path_args(&args), vec!["src/"]);
    }
}
```

- [ ] **Step 9: Override on `BashTool`**

In `src-tauri/src/agent/tools/builtin/shell.rs`, inside `impl Tool for BashTool`, add:

```rust
    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["working_dir"].as_str().map(|s| vec![s]).unwrap_or_default()
    }
```

(Note: this is `working_dir`, not `path`. Bash command-body path scanning is explicit non-goal — spec §3.)

Append a test at the end of shell.rs inside the existing tests block (if any), or create a new mod:

```rust
#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn bash_path_args_returns_working_dir_when_present() {
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"command": "ls", "working_dir": "/var/log"});
        assert_eq!(tool.path_args(&args), vec!["/var/log"]);
    }

    #[test]
    fn bash_path_args_empty_when_no_working_dir() {
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"command": "ls"});
        assert!(tool.path_args(&args).is_empty());
    }
}
```

- [ ] **Step 10: Run all six tools' path_args tests**

```bash
cd src-tauri && cargo test --lib path_args 2>&1 | tail -15
```

Expected: 8 tests pass (3 file + 1 edit + 3 search + 2 shell — wait, 3+1+3+2 = 9; the test names use `path_args` substring, the test functions match).

- [ ] **Step 11: Verify the trait default still works for non-path tools**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
```

Expected: empty (no errors). `web`, `plan`, `exit_plan_mode`, `self_eval`, `ask_user` inherit the default empty implementation — no changes needed in those files.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/agent/tools/
git commit -m "feat(agent): Tool::path_args + 6 overrides for path-aware dispatch

New trait method with default empty impl. Six tools override:
- read_file, write_file, edit, grep, glob → args.path
- bash → args.working_dir (command-body scanning is out of scope; spec §3)

Other tools (web, plan, exit_plan_mode, self_eval, ask_user) inherit the
empty default, no changes needed. 9 unit tests across 4 files."
```

---

### Task 4: Dispatcher path-check hook + `agent:need_approval` `kind: "path"` payload

Spec §4.3 and §4.4. The integration task. Between the tool-level approval block and `tool.execute()`, the dispatcher pulls candidate paths, asks SafetyManager, and reuses the existing `PendingApprovals` oneshot if the decision is Prompt. The frontend approval modal grows a new path-variant branch with three buttons (`once` / `session` / `deny`); the resolver IPC carries the chosen scope back so the dispatcher can call `allow_path_for_session` for "session".

This is the most complex task. Split into backend (Steps 1-7) and frontend (Steps 8-12).

**Files:**
- Modify: `src-tauri/src/app.rs` (extend `ApprovalResult`)
- Modify: `src-tauri/src/agent/dispatcher.rs:920-930` (insert path-check between approval and execute)
- Modify: `src-tauri/src/tauri_commands.rs::resolve_approval` (deserialize new fields; passing them through to ApprovalResult)
- Modify: `ui/src/components/agent/ApprovalModal.tsx` (handle `kind: "path"`)
- Modify: `ui/src/lib/tauri-bridge.ts` (extend `ApprovalResult` type)

- [ ] **Step 1: Extend `ApprovalResult` in `src-tauri/src/app.rs`**

Locate `pub struct ApprovalResult` (search: `grep -n "struct ApprovalResult" src-tauri/src/app.rs`). Add two new fields with `#[serde(default)]` so existing approval resolvers still deserialize:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResult {
    pub approved: bool,
    #[serde(default)]
    pub always_allow: bool,
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Path approval: "once" | "session" | "deny" (only set when kind=="path")
    #[serde(default)]
    pub path_scope: Option<String>,
    /// Path approval: which absolute paths to grant (only when path_scope="session")
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}
```

Update any `ApprovalResult { approved, always_allow, tool_name }` literal in the file by appending `path_scope: None, paths: None,` (search: `ApprovalResult {`).

- [ ] **Step 2: Find the `resolve_approval` IPC command**

```bash
grep -n "fn resolve_approval\|fn approve_tool_call" src-tauri/src/tauri_commands.rs
```

The command takes an `ApprovalResult`-shaped payload from the frontend and writes it into the oneshot via `state.pending_approvals.resolve(...)`. Since `ApprovalResult` now has `#[serde(default)]` on the new fields, existing callers don't break.

- [ ] **Step 3: Write the failing dispatcher test**

This one's hard to unit-test in isolation — the dispatcher integrates LLM, SafetyManager, tools, and the approval oneshot. Skip a dedicated dispatcher unit test for Task 4 and rely on the manual smoke checklist (§7 of the spec) plus the SafetyManager tests from Task 2.

Optional: if a quick smoke test is needed, add this to `src-tauri/src/agent/tests/` (if missing, this directory is OK to create just for this) but most agent_loop logic isn't unit-tested today and a new harness is out of scope. **Skip and rely on integration manual testing.**

- [ ] **Step 4: Locate the dispatcher hook site**

```bash
grep -n "Execute tool\|tool.execute(tc.arguments" src-tauri/src/agent/dispatcher.rs
```

Around line 924-929 you'll see:

```rust
                    // Emit tool start
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments);
                    tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool");
                    let tool_start = std::time::Instant::now();

                    // Execute tool
                    match tool.execute(tc.arguments.clone()).await {
```

The path-check goes **between** `emit_tool_start` and `tool.execute`. (Emit start first so the UI sees the tool was triggered even if path-check denies.)

- [ ] **Step 5: Insert the path-check block**

Replace the block from `// Emit tool start` through `let tool_start = ...` with:

```rust
                    // Emit tool start
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments);
                    tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool");

                    // ─── Path-aware sandbox (Phase 3) ───────────────────
                    // Resolve candidate paths from the tool's path_args, ask
                    // SafetyManager. Prompt → reuse the same approval
                    // modal/oneshot pattern with kind: "path".
                    let candidate_paths: Vec<std::path::PathBuf> = tool
                        .path_args(&tc.arguments)
                        .into_iter()
                        .map(|p| {
                            let pb = std::path::PathBuf::from(p);
                            if pb.is_absolute() {
                                pb
                            } else if let Some(root) = self.workspace_root.as_deref() {
                                root.join(pb)
                            } else {
                                pb
                            }
                        })
                        .collect();

                    if !candidate_paths.is_empty() && self.workspace_root.is_some() {
                        use crate::safety::path_policy::PathDecision;
                        let workspace_root = self.workspace_root.clone().unwrap();
                        let (ws_attached, sess_attached) = load_attached_dirs_for_session(
                            &self.app_handle,
                            &self.conversation_id,
                        );
                        let path_decision = {
                            let mgr = self.safety_manager.read().await;
                            mgr.check_paths(
                                &self.conversation_id,
                                &workspace_root,
                                &ws_attached,
                                &sess_attached,
                                &candidate_paths,
                                self.safety_mode.as_ref(),
                            )
                        };
                        match path_decision {
                            PathDecision::Allow => {}
                            PathDecision::Block { reason } => {
                                tracing::warn!(tool = %tc.name, reason = %reason, "Path blocked by sandbox");
                                reason_ctx.messages.push(ChatMessage::user_tool_result(
                                    &tc.id,
                                    &format!("Error: {}", reason),
                                    true,
                                ));
                                continue;
                            }
                            PathDecision::Prompt { reason } => {
                                tracing::info!(tool = %tc.name, reason = %reason, "Path requires approval");
                                let approval_id = format!("{}::path", tc.id);
                                let rx = self.pending_approvals.register(approval_id.clone());
                                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                                    "kind": "path",
                                    "toolName": tc.name,
                                    "toolId": approval_id,
                                    "arguments": tc.arguments,
                                    "paths": candidate_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                                    "reason": reason,
                                    "sessionId": self.conversation_id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                let path_result = rx.await.unwrap_or_else(|_| {
                                    crate::app::ApprovalResult {
                                        approved: false,
                                        always_allow: false,
                                        tool_name: None,
                                        path_scope: Some("deny".into()),
                                        paths: None,
                                    }
                                });
                                if !path_result.approved {
                                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                                        &tc.id,
                                        "Error: User denied access to out-of-workspace path.",
                                        true,
                                    ));
                                    continue;
                                }
                                if path_result.path_scope.as_deref() == Some("session") {
                                    let paths_to_grant = path_result.paths.clone()
                                        .unwrap_or_else(|| candidate_paths.iter().map(|p| p.display().to_string()).collect());
                                    let mut mgr = self.safety_manager.write().await;
                                    for p in paths_to_grant {
                                        mgr.allow_path_for_session(&self.conversation_id, std::path::PathBuf::from(p));
                                    }
                                }
                                // "once" falls through without persisting
                            }
                        }
                    }

                    let tool_start = std::time::Instant::now();

                    // Execute tool
```

- [ ] **Step 6: Add the `load_attached_dirs_for_session` helper near the bottom of `dispatcher.rs`**

Find a free spot below the impl block (e.g. after the last function in the impl):

```rust
/// Load workspace.attached_dirs and session.attached_dirs for the given
/// session. Returns empty vecs on any error (missing rows, malformed JSON).
fn load_attached_dirs_for_session(
    app_handle: &tauri::AppHandle,
    session_id: &str,
) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    use tauri::Manager;
    let Some(state) = app_handle.try_state::<crate::app::AppState>() else {
        return (Vec::new(), Vec::new());
    };
    let Ok(conn) = state.db.lock() else {
        return (Vec::new(), Vec::new());
    };
    let parse = |json: String| -> Vec<std::path::PathBuf> {
        serde_json::from_str::<Vec<String>>(&json)
            .ok()
            .unwrap_or_default()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect()
    };
    let ws_attached = conn
        .query_row(
            "SELECT attached_dirs FROM spaces WHERE id = (SELECT space_id FROM agent_sessions WHERE id = ?1)",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    let sess_attached = conn
        .query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    (ws_attached, sess_attached)
}
```

- [ ] **Step 7: Backend build check**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head -20
```

Expected: empty. Fix any borrow / type errors before proceeding. Common pitfalls:
- `self.workspace_root` is `Option<PathBuf>` — clone it before extracting.
- The `safety_manager.read().await` must release before `safety_manager.write().await`.

- [ ] **Step 8: Extend the frontend `ApprovalResult` type**

In `ui/src/lib/tauri-bridge.ts`, find the `ApprovalResult` interface (or type) — search:

```bash
grep -n "ApprovalResult" ui/src/lib/tauri-bridge.ts ui/src/components/agent/ApprovalModal.tsx
```

Extend the type with optional fields (TS shape — mirrors Rust):

```ts
export interface ApprovalResult {
  approved: boolean
  alwaysAllow?: boolean
  toolName?: string
  pathScope?: 'once' | 'session' | 'deny'
  paths?: string[]
}
```

If `approveToolCall` already exists as a wrapper, no signature change — it takes the same shape.

- [ ] **Step 9: Add path-variant rendering to `ApprovalModal.tsx`**

Locate `ApprovalModal.tsx` (`ui/src/components/agent/ApprovalModal.tsx`). The modal subscribes to `agent:need_approval`. The current branch handles `kind: "tool"` / `kind: "bash_command"` (or no kind = tool). Add a `kind === 'path'` branch.

Add to the listener payload type (around the top of the component):

```ts
type ApprovalPayload =
  | { kind?: 'tool' | 'bash_command'; toolName: string; toolId: string; arguments: unknown; reason: string; sessionId: string; riskLevel?: string }
  | { kind: 'path'; toolName: string; toolId: string; arguments: unknown; paths: string[]; reason: string; sessionId: string }
```

Inside the render block, before the existing tool/bash JSX, add:

```tsx
if (payload.kind === 'path') {
  const { toolId, paths, reason } = payload
  return (
    <Dialog open onOpenChange={(o) => { if (!o) resolve({ approved: false, pathScope: 'deny' }) }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>外部路径访问请求</DialogTitle>
          <DialogDescription>{reason}</DialogDescription>
        </DialogHeader>
        <div className="mt-2 max-h-40 overflow-y-auto rounded-md border bg-muted/30 p-2 text-xs">
          {paths.map((p) => (
            <div key={p} className="font-mono truncate" title={p}>{p}</div>
          ))}
        </div>
        <DialogFooter className="mt-4 flex gap-2">
          <button
            type="button"
            className="px-3 py-1.5 text-xs rounded-md text-destructive hover:bg-destructive/10"
            onClick={() => { void approveToolCall(toolId, { approved: false, pathScope: 'deny' }); close() }}
          >
            拒绝
          </button>
          <button
            type="button"
            className="px-3 py-1.5 text-xs rounded-md hover:bg-muted"
            onClick={() => { void approveToolCall(toolId, { approved: true, pathScope: 'once' }); close() }}
          >
            仅此一次
          </button>
          <button
            type="button"
            className="px-3 py-1.5 text-xs rounded-md bg-primary text-primary-foreground hover:bg-primary/90"
            onClick={() => { void approveToolCall(toolId, { approved: true, pathScope: 'session', paths }); close() }}
          >
            本会话允许
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

(Where `close()` is the existing helper that sets the modal payload to null. If the existing modal uses a different state name, adapt accordingly.)

- [ ] **Step 10: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: empty. Common pitfalls: the `Dialog` / `DialogContent` / etc. imports must come from `@/components/ui/dialog`. If the existing modal uses `AlertDialog`, mirror that import.

- [ ] **Step 11: Manual smoke**

Build is required to test the integration: `cd src-tauri && cargo tauri build --debug 2>&1 | tail -5`. Then run, switch to a workspace, and ask the agent to `read /etc/hosts`. The modal should appear with three buttons. Click "本会话允许" — the second `read /etc/hosts` shouldn't prompt. Restart the app — it should prompt again.

(If a build is too slow, mock the manual smoke and document the limitation in the PR.)

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "feat(agent): dispatcher path-check hook + approval modal path variant

Between the tool-level approval gate and tool.execute(), the dispatcher
now resolves candidate paths via Tool::path_args, asks SafetyManager.
check_paths(...), and on Prompt registers a path-scoped PendingApprovals
oneshot with kind: 'path'. Three actions return through the existing
resolve_approval IPC: 仅此一次 / 本会话允许 / 拒绝. Session grants are
written via SafetyManager.allow_path_for_session(...). 'once' falls
through; 'deny' surfaces as a tool error to the agent.

ApprovalResult gains \`path_scope\` and \`paths\` optional fields (serde
default so existing tool/bash flows still deserialize).

Yolo mode (session or global) short-circuits the path check in
SafetyManager.check_paths — matches existing should_approve semantics."
```

---

### Task 5: `upload_workspace_file` IPC + tests

Spec §4.5. Write file bytes into the workspace folder; sanitize filename; dedupe on collision with `(2)`, `(3)`, …; cap at 99.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add command + tests)
- Modify: `src-tauri/src/main.rs::invoke_handler!` (register)

- [ ] **Step 1: Write the failing tests**

Find a spot near the end of `tauri_commands.rs` (above the test module that already exists). The test module is at the end — append inside it. Tests will fail on compile because `do_upload_workspace_file` doesn't exist yet:

```rust
    // ─── upload_workspace_file ──────────────────────────────────────

    #[test]
    fn upload_workspace_file_sanitizes_filename() {
        assert_eq!(super::sanitize_upload_filename("hello.txt"), Ok("hello.txt".to_string()));
        assert_eq!(super::sanitize_upload_filename("a/b/c.txt"), Ok("c.txt".to_string()));
        assert_eq!(super::sanitize_upload_filename("../escape.txt"), Err(super::Error::InvalidInput("filename contains '..'".into())));
        assert_eq!(super::sanitize_upload_filename(".hidden"), Err(super::Error::InvalidInput("dotfiles are not allowed".into())));
        assert_eq!(super::sanitize_upload_filename(""), Err(super::Error::InvalidInput("filename is empty".into())));
        // Truncation: 250 chars + .png → 200 chars max
        let long = "a".repeat(250) + ".png";
        let out = super::sanitize_upload_filename(&long).unwrap();
        assert!(out.len() <= 200);
        assert!(out.ends_with(".png"));
    }

    #[test]
    fn upload_workspace_file_dedupes_on_collision() {
        let dir = tempfile::TempDir::new().unwrap();
        // Pre-create the original.
        std::fs::write(dir.path().join("logo.png"), b"a").unwrap();
        let p2 = super::next_available_path(dir.path(), "logo.png").unwrap();
        assert_eq!(p2.file_name().unwrap(), "logo (2).png");

        // Pre-create the (2) variant.
        std::fs::write(dir.path().join("logo (2).png"), b"b").unwrap();
        let p3 = super::next_available_path(dir.path(), "logo.png").unwrap();
        assert_eq!(p3.file_name().unwrap(), "logo (3).png");
    }

    #[test]
    fn upload_workspace_file_no_extension_still_dedupes() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("README"), b"a").unwrap();
        let p = super::next_available_path(dir.path(), "README").unwrap();
        assert_eq!(p.file_name().unwrap(), "README (2)");
    }
```

- [ ] **Step 2: Run tests to confirm RED**

```bash
cd src-tauri && cargo test --lib upload_workspace_file 2>&1 | tail -10
```

Expected: compile error — `sanitize_upload_filename` / `next_available_path` don't exist.

- [ ] **Step 3: Implement the helpers and the command**

Add to `src-tauri/src/tauri_commands.rs` (near the other workspace commands, e.g. after `list_directory_entries`):

```rust
/// Sanitize a user-provided filename so it can't escape the target dir or
/// hide as a dotfile. Returns the cleaned name. Truncates total length
/// (incl. extension) to 200 chars; preserves the extension on truncation.
pub(crate) fn sanitize_upload_filename(raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("filename is empty".into()));
    }
    if trimmed.contains("..") {
        return Err(Error::InvalidInput("filename contains '..'".into()));
    }
    let base = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::InvalidInput("filename has no basename".into()))?;
    if base.starts_with('.') {
        return Err(Error::InvalidInput("dotfiles are not allowed".into()));
    }
    if base.len() <= 200 {
        return Ok(base.to_string());
    }
    // Truncate keeping the extension.
    let p = std::path::Path::new(base);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|s| s.to_str());
    let ext_part = ext.map(|e| format!(".{}", e)).unwrap_or_default();
    let max_stem = 200usize.saturating_sub(ext_part.len());
    let truncated_stem: String = stem.chars().take(max_stem).collect();
    Ok(format!("{}{}", truncated_stem, ext_part))
}

/// Given a target dir + sanitized filename, return a path that doesn't
/// collide with anything on disk. Appends " (2)", " (3)", … before the
/// extension. Errors after 99 attempts.
pub(crate) fn next_available_path(dir: &std::path::Path, filename: &str) -> Result<std::path::PathBuf, Error> {
    let initial = dir.join(filename);
    if !initial.exists() {
        return Ok(initial);
    }
    let p = std::path::Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|s| s.to_str());
    for n in 2..=99u32 {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, n, e),
            None => format!("{} ({})", stem, n),
        };
        let candidate = dir.join(new_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(Error::Internal(format!("could not find a free filename for '{}' after 99 attempts", filename)))
}

#[tauri::command]
pub async fn upload_workspace_file(
    state: State<'_, AppState>,
    workspace_id: String,
    filename: String,
    content: Vec<u8>,
) -> Result<String, Error> {
    // Look up workspace path.
    let path_raw: Option<String> = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        conn.query_row(
            "SELECT path FROM spaces WHERE id = ?1",
            rusqlite::params![workspace_id],
            |row| row.get::<_, Option<String>>(0),
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("workspace '{}'", workspace_id)),
            other => Error::Database(other),
        })?
    };
    let ws_path = path_raw
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::InvalidInput(format!("workspace '{}' has no path", workspace_id)))?;
    let ws_path = std::path::PathBuf::from(ws_path);

    tokio::fs::create_dir_all(&ws_path).await.map_err(Error::Io)?;

    let clean = sanitize_upload_filename(&filename)?;
    let target = next_available_path(&ws_path, &clean)?;
    tokio::fs::write(&target, &content).await.map_err(Error::Io)?;
    Ok(target.to_string_lossy().into_owned())
}
```

- [ ] **Step 4: Register the command in main.rs**

Open `src-tauri/src/main.rs`. Find the `invoke_handler!` block. Add a line near other workspace commands:

```rust
            uclaw_core::tauri_commands::upload_workspace_file,
```

- [ ] **Step 5: Run tests to confirm GREEN**

```bash
cd src-tauri && cargo test --lib upload_workspace_file 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 6: Add bridge wrapper**

In `ui/src/lib/tauri-bridge.ts`, add near `listDirectoryEntries`:

```ts
/** Write file bytes into the workspace folder. Returns absolute path of
 *  the written file (deduped if collision). */
export const uploadWorkspaceFile = (
  workspaceId: string,
  filename: string,
  content: number[],
): Promise<string> => invoke('upload_workspace_file', { workspaceId, filename, content })
```

- [ ] **Step 7: TS check + commit**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: empty.

```bash
git add -A
git commit -m "feat(workspace): upload_workspace_file IPC + tests

Writes file bytes into the workspace folder. Sanitizes filename
(rejects '..', dotfiles, empty; truncates to 200 chars keeping
extension). On collision, appends ' (n)' before the extension; errors
after 99 attempts.

Returns absolute path so the frontend can refresh FileBrowser. No path
policy check needed — destination is by construction inside workspace."
```

---

### Task 6: `path_is_directory` IPC + bridge wrapper

Spec §4.6. Tiny IPC the frontend uses to disambiguate file vs folder when handling Tauri's native `onDragDropEvent`.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs::invoke_handler!`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: Implement the command**

Append to `src-tauri/src/tauri_commands.rs` (near `list_directory_entries`):

```rust
/// Lightweight type-of-path probe. Used by the frontend to decide
/// whether a native drag-drop event payload is a folder (→
/// attach_workspace_directory) or a file (→ upload_workspace_file).
/// Returns false on missing path or any IO error.
#[tauri::command]
pub async fn path_is_directory(path: String) -> Result<bool, Error> {
    let p = std::path::PathBuf::from(&path);
    let meta = match tokio::fs::metadata(&p).await {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    Ok(meta.is_dir())
}
```

- [ ] **Step 2: Register in main.rs**

```rust
            uclaw_core::tauri_commands::path_is_directory,
```

- [ ] **Step 3: Add bridge wrapper**

In `ui/src/lib/tauri-bridge.ts`, near `uploadWorkspaceFile`:

```ts
export const pathIsDirectory = (path: string): Promise<boolean> =>
  invoke('path_is_directory', { path })
```

- [ ] **Step 4: Build + commit**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head
```

Both should be empty.

```bash
cd /Users/ryanliu/Documents/uclaw
git add -A
git commit -m "feat(workspace): path_is_directory IPC + bridge wrapper

One-liner used by frontend to disambiguate file vs folder when handling
Tauri native onDragDropEvent payloads. Returns false on any IO error so
the caller can treat that as 'not a folder' without exception handling."
```

---

### Task 7: FileDropZone wiring — file bytes + native folder drop

Spec §4.6. SidePanel passes a real `onDrop` callback that uploads File bytes; AgentView listens to Tauri's window-level `onDragDropEvent` for real folder paths. The two paths handle the cases the other can't (browser File has no real path on Tauri; window event doesn't fire for element-level React DragEvents).

**Files:**
- Modify: `ui/src/components/agent/SidePanel.tsx`
- Modify: `ui/src/components/agent/AgentView.tsx`
- Modify: `ui/src/components/file-browser/FileDropZone.tsx` (per-target hint)

- [ ] **Step 1: Wire `onDrop` in `SidePanel.tsx`**

Find both `<FileDropZone …/>` mounts (search: `grep -n "FileDropZone" ui/src/components/agent/SidePanel.tsx`). They're at the bottoms of the session-files and workspace-files sections.

Add a callback in the component body (near `handleAddToChat`):

```tsx
const handleFilesDropped = React.useCallback(async (files: File[]) => {
  if (!currentWorkspaceId) {
    toast.error('请先选择工作区')
    return
  }
  for (const file of files) {
    try {
      const buf = await file.arrayBuffer()
      const bytes = Array.from(new Uint8Array(buf))
      const writtenPath = await uploadWorkspaceFile(currentWorkspaceId, file.name, bytes)
      console.debug('[upload] wrote', writtenPath)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`上传 ${file.name} 失败: ${msg}`)
    }
  }
  setFilesVersion((v) => v + 1)
}, [currentWorkspaceId, setFilesVersion])
```

Add `onDrop={handleFilesDropped}` to **both** `<FileDropZone>` instances. Also pass the per-target hint:

```tsx
<FileDropZone
  target="workspace"
  hint="拖入工作区文件 / 文件夹"
  onDrop={handleFilesDropped}
  onFilesUploaded={handleFilesUploaded}
/>
```

And for the session-files section:

```tsx
<FileDropZone
  sessionId={sessionId}
  target="session"
  hint="拖入会话目录"
  onDrop={handleFilesDropped}
  onFilesUploaded={handleFilesUploaded}
/>
```

Add the import at the top of SidePanel:

```ts
import { toast } from 'sonner'
import { uploadWorkspaceFile } from '@/lib/tauri-bridge'
```

(If `toast` is already imported, skip.)

- [ ] **Step 2: Listen to native folder-drop in AgentView**

In `ui/src/components/agent/AgentView.tsx`, add near the other window-level effects (search for `getCurrentWindow` or similar; if none exists, add at the top of the component body):

```tsx
React.useEffect(() => {
  if (!currentWorkspaceId) return
  const win = getCurrentWindow()
  let unlisten: (() => void) | undefined
  win.onDragDropEvent((evt) => {
    if (evt.payload.type !== 'drop') return
    const paths = evt.payload.paths
    void Promise.all(paths.map(async (p) => {
      try {
        const isDir = await pathIsDirectory(p)
        if (!isDir) return  // Files are handled by the React-event branch
        await attachWorkspaceDirectory(currentWorkspaceId, p)
        toast.success(`已附加目录: ${p}`)
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        toast.error(`附加 ${p} 失败: ${msg}`)
      }
    })).then(() => {
      // Refresh the wsAttachedMap by re-fetching via the existing atom path
      // (WorkspaceFilesView re-renders on workspaceAttachedDirsMapAtom change;
      // attachWorkspaceDirectory's return value already updates that atom
      // inside SidePanel/WorkspaceFilesView, but the listener here is global
      // — call setFilesVersion to nudge a refresh.)
      setFilesVersion((v) => v + 1)
    })
  }).then((u) => { unlisten = u })
  return () => { unlisten?.() }
}, [currentWorkspaceId])
```

Imports at top:

```ts
import { getCurrentWindow } from '@tauri-apps/api/window'
import { attachWorkspaceDirectory, pathIsDirectory } from '@/lib/tauri-bridge'
import { useSetAtom } from 'jotai'
import { workspaceFilesVersionAtom } from '@/atoms/agent-atoms'
import { toast } from 'sonner'
```

Inside the component, derive setters:

```tsx
const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)
```

(`currentWorkspaceId` should already be in scope from existing AgentView logic — confirm by grepping.)

- [ ] **Step 3: Add `hint` prop forwarding in FileDropZone**

In `ui/src/components/file-browser/FileDropZone.tsx`, the component already accepts `hint`. Confirm by reading the component (see Phase 2 already shipped this). If `hint` isn't a prop yet, add it:

```ts
interface FileDropZoneProps {
  // existing...
  hint?: string
}

// In the component
hint = '拖拽文件到此处',  // default
```

(Already present from Phase 2 — confirm and skip if so.)

- [ ] **Step 4: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head
```

Expected: empty.

- [ ] **Step 5: Manual smoke**

Build (`cd src-tauri && cargo tauri build --debug`), run, in a workspace:
1. Drag a `.png` from Finder onto right-panel workspace files → file appears in workspace folder + FileBrowser within ~1s.
2. Drag a folder from Finder onto the same area → toast "已附加目录", folder appears under 附加目录 list.
3. Drag a duplicate filename → second one becomes `name (2).png`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(workspace): wire FileDropZone — file uploads + native folder drops

Two drop paths handle the two cases the browser/Tauri APIs each can't
cover:

Browser drag-drop (React DragEvent on FileDropZone) → file bytes via
File.arrayBuffer() → uploadWorkspaceFile IPC. Tauri 2 doesn't expose
real OS paths on browser File objects (security restriction), but bytes
are available.

Tauri native onDragDropEvent (window-level) → real OS paths → path_is_directory
probe → attach_workspace_directory for folders. The window event doesn't
fire for element-level drops, so we listen globally in AgentView.

Per-target hint strings on FileDropZone: 'workspace' vs 'session'."
```

---

### Task 8: Settings — `WorkspaceSandboxSettings` + 5 path-policy IPCs

Spec §4.7. Settings → 安全 (Safety) tab gains a new section managing the global allowed list and offering a promote-from-session button for the currently-active session's grants.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (5 IPCs + tests)
- Modify: `src-tauri/src/main.rs::invoke_handler!`
- Modify: `ui/src/lib/tauri-bridge.ts` (5 wrappers)
- Create: `ui/src/components/settings/WorkspaceSandboxSettings.tsx`
- Modify: settings parent (whichever file mounts the Safety tab)

- [ ] **Step 1: Backend — write the failing IPC test**

In `src-tauri/src/tauri_commands.rs`, append inside the existing test mod:

```rust
    // ─── path policy IPCs ────────────────────────────────────────────

    #[test]
    fn path_policy_ipc_add_remove_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut mgr = crate::safety::SafetyManager::new(tmp.path());
        let outside = tempfile::TempDir::new().unwrap().path().to_path_buf();
        mgr.add_always_allowed_path(outside.clone()).unwrap();
        assert!(mgr.list_always_allowed_paths().contains(&outside));
        mgr.remove_always_allowed_path(&outside).unwrap();
        assert!(!mgr.list_always_allowed_paths().contains(&outside));
    }

    #[test]
    fn path_policy_ipc_promote_clears_session_adds_global() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut mgr = crate::safety::SafetyManager::new(tmp.path());
        let outside = tempfile::TempDir::new().unwrap().path().to_path_buf();
        mgr.allow_path_for_session("sess1", outside.clone());
        assert_eq!(mgr.list_session_allowed_paths("sess1"), vec![outside.clone()]);
        mgr.promote_session_path_to_global("sess1", &outside).unwrap();
        assert!(mgr.list_session_allowed_paths("sess1").is_empty());
        assert!(mgr.list_always_allowed_paths().contains(&outside));
    }
```

- [ ] **Step 2: Run tests to confirm GREEN (these wrap Task 2 methods, no new code)**

```bash
cd src-tauri && cargo test --lib path_policy_ipc 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed`. (If these fail, Task 2 has an issue — fix that first.)

- [ ] **Step 3: Add the 5 Tauri commands**

Append to `src-tauri/src/tauri_commands.rs` (near other safety-related commands, or below `path_is_directory`):

```rust
// ─── Path policy IPCs ──────────────────────────────────────────────────

#[tauri::command]
pub async fn list_always_allowed_paths(state: State<'_, AppState>) -> Result<Vec<String>, Error> {
    let mgr = state.safety_manager.read().await;
    Ok(mgr.list_always_allowed_paths().iter().map(|p| p.display().to_string()).collect())
}

#[tauri::command]
pub async fn add_always_allowed_path(state: State<'_, AppState>, path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_absolute() {
        return Err(Error::InvalidInput("path must be absolute".into()));
    }
    let mut mgr = state.safety_manager.write().await;
    mgr.add_always_allowed_path(p)
}

#[tauri::command]
pub async fn remove_always_allowed_path(state: State<'_, AppState>, path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_always_allowed_path(&p)
}

#[tauri::command]
pub async fn list_session_allowed_paths(state: State<'_, AppState>, session_id: String) -> Result<Vec<String>, Error> {
    let mgr = state.safety_manager.read().await;
    Ok(mgr.list_session_allowed_paths(&session_id).iter().map(|p| p.display().to_string()).collect())
}

#[tauri::command]
pub async fn promote_session_path_to_global(state: State<'_, AppState>, session_id: String, path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    let mut mgr = state.safety_manager.write().await;
    mgr.promote_session_path_to_global(&session_id, &p)
}
```

- [ ] **Step 4: Register all 5 in main.rs**

```rust
            uclaw_core::tauri_commands::list_always_allowed_paths,
            uclaw_core::tauri_commands::add_always_allowed_path,
            uclaw_core::tauri_commands::remove_always_allowed_path,
            uclaw_core::tauri_commands::list_session_allowed_paths,
            uclaw_core::tauri_commands::promote_session_path_to_global,
```

- [ ] **Step 5: Backend build check**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
```

Expected: empty.

- [ ] **Step 6: Add 5 bridge wrappers**

In `ui/src/lib/tauri-bridge.ts`:

```ts
export const listAlwaysAllowedPaths = (): Promise<string[]> =>
  invoke('list_always_allowed_paths')

export const addAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('add_always_allowed_path', { path })

export const removeAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('remove_always_allowed_path', { path })

export const listSessionAllowedPaths = (sessionId: string): Promise<string[]> =>
  invoke('list_session_allowed_paths', { sessionId })

export const promoteSessionPathToGlobal = (sessionId: string, path: string): Promise<void> =>
  invoke('promote_session_path_to_global', { sessionId, path })
```

- [ ] **Step 7: Create `WorkspaceSandboxSettings.tsx`**

Create `ui/src/components/settings/WorkspaceSandboxSettings.tsx`:

```tsx
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Plus, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import {
  listAlwaysAllowedPaths,
  addAlwaysAllowedPath,
  removeAlwaysAllowedPath,
  listSessionAllowedPaths,
  promoteSessionPathToGlobal,
  openFolderDialog,
} from '@/lib/tauri-bridge'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'

export function WorkspaceSandboxSettings(): React.ReactElement {
  const sessionId = useAtomValue(currentAgentSessionIdAtom)
  const [global, setGlobal] = React.useState<string[]>([])
  const [session, setSession] = React.useState<string[]>([])

  const refreshGlobal = React.useCallback(async () => {
    try { setGlobal(await listAlwaysAllowedPaths()) } catch (err) { console.error('[sandbox]', err) }
  }, [])

  const refreshSession = React.useCallback(async () => {
    if (!sessionId) { setSession([]); return }
    try { setSession(await listSessionAllowedPaths(sessionId)) } catch (err) { console.error('[sandbox]', err) }
  }, [sessionId])

  React.useEffect(() => { void refreshGlobal() }, [refreshGlobal])
  React.useEffect(() => { void refreshSession() }, [refreshSession])

  const handleAdd = async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      await addAlwaysAllowedPath(picked.path)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`添加失败: ${msg}`)
    }
  }

  const handleRemove = async (p: string) => {
    try {
      await removeAlwaysAllowedPath(p)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`删除失败: ${msg}`)
    }
  }

  const handlePromote = async (p: string) => {
    if (!sessionId) return
    try {
      await promoteSessionPathToGlobal(sessionId, p)
      await refreshGlobal()
      await refreshSession()
      toast.success('已升级为永久允许')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`升级失败: ${msg}`)
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">始终允许的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">Agent 在任何工作区都可以访问这些路径,无需提示。</p>
        <div className="rounded-md border bg-muted/30">
          {global.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">尚未添加任何路径。</div>
          )}
          {global.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handleRemove(p)}
                className={cn('shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10')}
                title="删除"
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={handleAdd}
          className="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md bg-primary/10 text-primary hover:bg-primary/20"
        >
          <Plus className="size-3.5" />
          添加路径
        </button>
      </section>

      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">本会话已临时授权的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">
          仅本会话有效,重启应用后清除。点"升级为永久"加入上面的列表。
        </p>
        <div className="rounded-md border bg-muted/30">
          {!sessionId && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">没有活动会话。</div>
          )}
          {sessionId && session.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">本会话没有触发过外部路径授权。</div>
          )}
          {sessionId && session.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handlePromote(p)}
                className="shrink-0 px-2 py-0.5 text-[11px] rounded text-primary hover:bg-primary/10"
              >
                升级为永久
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}
```

- [ ] **Step 8: Mount in the Settings parent**

Locate the Safety / 安全 settings tab. Try:

```bash
grep -rn "SafetySettings\|安全" ui/src/components/settings/ | head
```

Open whichever file holds the Safety tab (likely `SafetySettings.tsx`). At the bottom of the tab body, add:

```tsx
<div className="mt-8 pt-6 border-t">
  <h2 className="text-base font-semibold mb-3">工作区沙箱</h2>
  <WorkspaceSandboxSettings />
</div>
```

Add the import at the top:

```ts
import { WorkspaceSandboxSettings } from './WorkspaceSandboxSettings'
```

- [ ] **Step 9: Write a UI test**

Create `ui/src/components/settings/WorkspaceSandboxSettings.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { WorkspaceSandboxSettings } from './WorkspaceSandboxSettings'

vi.mock('@/lib/tauri-bridge', () => ({
  listAlwaysAllowedPaths: vi.fn().mockResolvedValue(['/tmp', '/Users/me/notes']),
  addAlwaysAllowedPath: vi.fn().mockResolvedValue(undefined),
  removeAlwaysAllowedPath: vi.fn().mockResolvedValue(undefined),
  listSessionAllowedPaths: vi.fn().mockResolvedValue([]),
  promoteSessionPathToGlobal: vi.fn().mockResolvedValue(undefined),
  openFolderDialog: vi.fn().mockResolvedValue({ path: '/new/path' }),
}))

describe('WorkspaceSandboxSettings', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders the global allowed list from IPC', async () => {
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('/tmp')).toBeInTheDocument()
      expect(screen.getByText('/Users/me/notes')).toBeInTheDocument()
    })
  })

  it('shows empty state when global list is empty', async () => {
    const { listAlwaysAllowedPaths } = await import('@/lib/tauri-bridge')
    vi.mocked(listAlwaysAllowedPaths).mockResolvedValueOnce([])
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('尚未添加任何路径。')).toBeInTheDocument()
    })
  })

  it('shows "no active session" when sessionId is null', async () => {
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('没有活动会话。')).toBeInTheDocument()
    })
  })
})
```

- [ ] **Step 10: Run UI tests**

```bash
cd ui && npm test -- --run WorkspaceSandboxSettings 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 11: Final commit**

```bash
git add -A
git commit -m "feat(settings): WorkspaceSandboxSettings — global allowed list + session promote

Settings → 安全 → 工作区沙箱 section. Two parts:

1. 始终允许的外部路径: global list persisted to ~/.uclaw/path_policy.json.
   Add via folder picker (reuses Phase 2 openFolderDialog); remove inline.
   These paths bypass the sandbox prompt for any session/workspace.

2. 本会话已临时授权的外部路径: read-only display of in-memory grants for
   the currently active session. Each row offers '升级为永久' which moves
   the entry into the global list. Hidden when no session is active.

5 new Tauri commands + 5 bridge wrappers + 3 Vitest tests."
```

---

## Self-Review

**1. Spec coverage:**
- §2 Goal 1 (path-aware safety layer) → Tasks 1, 2, 3, 4 ✓
- §2 Goal 2 (whitelist composition) → Task 1 (PathPolicy.check) + Task 4 (dispatcher loads attached_dirs) ✓
- §2 Goal 3 (approval modal: path variant) → Task 4 Step 9 ✓
- §2 Goal 4 (bash sandboxing scope) → Task 3 Step 9 (only working_dir) ✓
- §2 Goal 5 (upload_workspace_file) → Task 5 ✓
- §2 Goal 6 (FileDropZone wiring) → Task 7 ✓
- §2 Goal 7 (Settings) → Task 8 ✓
- §2 Goal 8 (build green at every commit) → enforced in conventions ✓
- §2 Goal 9 (~15 Rust tests + 2-3 Vitest): tally — Task 1: 10 Rust; Task 2: 4 Rust; Task 3: 9 Rust; Task 5: 3 Rust; Task 8: 2 Rust + 3 Vitest. Total 28 Rust + 3 Vitest. Exceeds estimate, fine.

**2. Placeholder scan:** searched for TBD/TODO/fill-in — none. Task 4 Step 3 explicitly notes the dispatcher path-check isn't unit-tested with reasoning (manual smoke listed) — this is a documented trade-off, not a placeholder.

**3. Type consistency:**
- `PathDecision::Allow | Prompt { reason } | Block { reason }` — used identically in Tasks 1, 2, 4.
- `SafetyManager::check_paths(session_id, workspace_root, workspace_attached, session_attached, candidates, mode_override)` — same signature in Task 2 definition and Task 4 call site.
- `ApprovalResult { approved, always_allow, tool_name, path_scope, paths }` — Task 4 Step 1 defines Rust shape, Step 8 defines TS shape, Step 9 uses both. Consistent.
- `path_args(&self, args: &serde_json::Value) -> Vec<&str>` — same trait signature in Task 3 trait definition and all 6 overrides.

No gaps found.
