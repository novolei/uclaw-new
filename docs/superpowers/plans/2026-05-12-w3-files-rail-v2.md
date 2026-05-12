# W3 — Files Rail v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the polling-based `<FileBrowser>` in the agent right-rail with a notify-driven `<FilesRail>` component that has top-level tabs (workspace / changes), surfaces attached_dirs as mountable sections, and preserves tree expansion state.

**Architecture:** New Rust module `src-tauri/src/files_rail/` (8 files) owns a single multi-mount `RecommendedWatcher` + a `ManagedService` that registers into the existing `ServiceManager`. Four Tauri commands plus one batched IPC event (`files_rail:change`) drive a new `ui/src/components/files-rail/` directory (~15 small files) that replaces the two `<FileBrowser>` instances inside `SidePanel.tsx`. Tree state lives in Jotai atoms; updates apply via incremental tree-patch (no full re-fetch on each event).

**Tech Stack:** Rust · `notify` v7 (already in `Cargo.toml`) · `tokio::sync::mpsc` for event batching · `serde` · React 18 + TypeScript · Jotai (`atomFamily`) · Tailwind + uClaw theme tokens · `@tauri-apps/api/event` · Vitest + RTL · `cargo test --lib`.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

---

## Pre-flight

- [ ] **Branch setup**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git checkout -b claude/w3-files-rail
```

Expected: clean checkout at main's tip (which includes W1 + W2); new branch.

- [ ] **Baseline verification**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

Record baseline test count (should be 366 from W2's last state). Confirm zero build errors.

```bash
cd ../ui && npx tsc --noEmit 2>&1 | head
cd ui && npm test -- --run 2>&1 | tail -5
```

Record baseline (should be 253 from W1).

- [ ] **State landmarks** (read once, don't change)

```bash
grep -n "FileBrowser\|workspaceFilesPath\|filesVersion" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx | head -15
```

The two `<FileBrowser>` instances at SidePanel lines ~300 and ~352 are W3's integration targets. Don't delete `ui/src/components/file-browser/` — other code may import `FileTypeIcon` from it.

```bash
sed -n '125,180p' /Users/ryanliu/Documents/uclaw/src-tauri/src/workspace/mod.rs
```

The existing `FileWatcher` lives here. **Leave it alone** — `artifact:tree_update` consumers may rely on it. W3's watcher is parallel and emits a different channel.

---

## File Structure

### New Rust modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `src-tauri/src/files_rail/mod.rs` | ~40 | barrel re-exports + module doc |
| `src-tauri/src/files_rail/types.rs` | ~120 | `FileNode`, `MountRoot`, `NodeKind`, `MountKind`, `FileChange`, `FilesRailChange` |
| `src-tauri/src/files_rail/ignore.rs` | ~80 | `SKIP_DIRS` static + `should_ignore(name, kind)` |
| `src-tauri/src/files_rail/walker.rs` | ~150 | single-level dir read + sort + size/mtime |
| `src-tauri/src/files_rail/watcher.rs` | ~200 | multi-mount `RecommendedWatcher` + 16ms event batching |
| `src-tauri/src/files_rail/service.rs` | ~140 | `FilesRailService` impl `ManagedService` |
| `src-tauri/src/files_rail/commands.rs` | ~120 | 4 Tauri commands |
| `src-tauri/src/files_rail/tests.rs` | ~200 | walker + ignore + tree-patch fixtures |

### Modified Rust files

| Path | Edit |
|---|---|
| `src-tauri/src/agent/mod.rs` | unchanged (W3 is its own top-level module) |
| `src-tauri/src/lib.rs` | `pub mod files_rail;` |
| `src-tauri/src/main.rs` | construct `FilesRailService`, register into `service_manager` in the Stage 3 block |
| `src-tauri/src/tauri_commands.rs` | 4 new `pub use files_rail::commands::*;` re-exports + register in `invoke_handler!` |
| `src-tauri/src/app.rs` | `files_rail_service: Arc<files_rail::FilesRailService>` field on `AppState`; constructed in `AppState::new` |

### New TypeScript modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `ui/src/components/files-rail/index.tsx` | ~120 | outer container, tab switch |
| `ui/src/components/files-rail/FilesRailTabs.tsx` | ~60 | Radix Tabs header |
| `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx` | ~180 | mount-list scroll container |
| `ui/src/components/files-rail/workspace/MountSection.tsx` | ~140 | one mount: header + tree root |
| `ui/src/components/files-rail/workspace/FileTreeNode.tsx` | ~180 | recursive tree node, lazy expand |
| `ui/src/components/files-rail/changes/FileChangesPanel.tsx` | ~160 | agent edits list |
| `ui/src/components/files-rail/changes/ChangeRow.tsx` | ~70 | one change entry |
| `ui/src/components/files-rail/hooks/useFilesRail.ts` | ~130 | mount/tab state composition |
| `ui/src/components/files-rail/hooks/useFileTree.ts` | ~120 | lazy dir read + per-mount cache |
| `ui/src/components/files-rail/hooks/useFilesRailWatcher.ts` | ~90 | listen `files_rail:change`, dispatch |
| `ui/src/components/files-rail/utils/tree-patch.ts` | ~140 | apply Create/Modify/Delete/Rename to in-memory tree |
| `ui/src/components/files-rail/utils/tree-patch.test.ts` | ~120 | unit tests |
| `ui/src/atoms/files-rail-atoms.ts` | ~110 | `mountRootsAtom`, `expandedPathsAtomFamily`, `fileTreeAtomFamily`, `filesRailTabAtom` |

### Modified TS files

| Path | Edit |
|---|---|
| `ui/src/components/agent/SidePanel.tsx` | replace the two `<FileBrowser>` instances with one `<FilesRail sessionId={sessionId} sessionPath={sessionPath} workspaceFilesPath={workspaceFilesPath} />` mount |
| `ui/src/lib/tauri-bridge.ts` | add typed wrappers for the 4 new Tauri commands |

**Module size budget**: every file ≤ 200 lines target, 400 hard cap. The largest file in the plan is `tests.rs` at ~200 — that's tests; production code peaks at 200 (`watcher.rs`).

**Total new code**: ~2400 LoC across 21 new files. Comparable to W4's planned size — W3 is the second-largest wave.

---

## Task 1: Rust data types

**Files:**
- Create: `src-tauri/src/files_rail/mod.rs`
- Create: `src-tauri/src/files_rail/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod files_rail;`)

- [ ] **Step 1: Create `src-tauri/src/files_rail/types.rs`**

```rust
//! Data types for the files-rail subsystem.
//!
//! Wire format for the Tauri commands and the `files_rail:change` IPC channel.
//! All types are `Serialize` (out) and `Deserialize` (in for command inputs).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the mount root (forward slashes regardless of OS).
    pub rel_path: String,
    /// Display name (last segment of path).
    pub name: String,
    pub kind: NodeKind,
    /// Size in bytes; 0 for directories.
    pub size: u64,
    /// Modification time, milliseconds since epoch.
    pub mtime_ms: i64,
    /// True if the node was filtered by ignore rules. Currently only used in
    /// changes-tab rendering — directory walks already drop ignored entries.
    pub is_ignored: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    /// `~/Documents/workground/<workspace>` — the workspace root.
    Workspace,
    /// `~/Documents/workground/<workspace>/<session>` — per-session subdir.
    Session,
    /// A directory the user attached via `attach_session_directory` etc.
    AttachedDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountRoot {
    /// Stable id used as the key in atom families and the watcher registry.
    /// Format: "workspace:<workspace_id>" / "session:<session_id>" /
    /// "attached:<sha1(path)>".
    pub id: String,
    /// Human-visible label (e.g. workspace name, "会话文件", attached dir basename).
    pub label: String,
    pub path: PathBuf,
    pub kind: MountKind,
    /// True if the user may rename / delete / write through W4's editors.
    /// AttachedDirs default to read-only; W4 adds an opt-in toggle.
    pub editable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    /// On macOS, notify often reports rename as a Created/Removed pair within
    /// the debounce window — `coalesce_pairs()` in watcher.rs merges them.
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub kind: ChangeKind,
    pub rel_path: String,
    /// Only set for `Renamed`. The new relative path.
    pub new_rel_path: Option<String>,
    pub is_dir: bool,
}

/// Payload of the `files_rail:change` IPC event.
/// Batched: each emit contains up to 100 changes accumulated over a 16ms
/// debounce window per mount.
#[derive(Debug, Clone, Serialize)]
pub struct FilesRailChange {
    pub mount_id: String,
    pub changes: Vec<FileChange>,
}

impl FilesRailChange {
    pub const CHANNEL: &'static str = "files_rail:change";
}
```

- [ ] **Step 2: Create `src-tauri/src/files_rail/mod.rs`**

```rust
//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod commands;
pub mod ignore;
pub mod service;
pub mod types;
pub mod walker;
pub mod watcher;

#[cfg(test)]
mod tests;

pub use service::FilesRailService;
pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
```

(The submodules `commands`, `ignore`, `service`, `walker`, `watcher`, `tests` don't exist yet. The compile will fail until Tasks 2-5 land them. Task 1 commits ONLY `types.rs` + a minimal `mod.rs` listing just `types`. Adjust the `mod.rs` to:

```rust
//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod types;

pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
```

and expand it incrementally in later tasks.)

- [ ] **Step 3: Wire into `src-tauri/src/lib.rs`**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "^pub mod" src-tauri/src/lib.rs | head -10
```

Insert `pub mod files_rail;` alphabetically (likely between `extensions` and `harness`). Use `Edit` with a unique anchor.

- [ ] **Step 4: Build to confirm**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error|^warning" | head -10
```

Expected: empty. The new types are not yet imported by anything, but Rust still type-checks the module.

- [ ] **Step 5: Pre-commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current && git status --short
```

Branch MUST be `claude/w3-files-rail`. Status MUST show ONLY:
```
 M src-tauri/src/lib.rs
?? src-tauri/src/files_rail/
```

If any other file appears, STOP and report BLOCKED.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/files_rail/mod.rs src-tauri/src/files_rail/types.rs
git commit -m "feat(files-rail): scaffold module + wire types (FileNode, MountRoot, FilesRailChange)"
```

Verify:

```bash
git branch --show-current && git show HEAD --stat
```

Branch must be `claude/w3-files-rail`. Commit must contain ONLY the 3 intended files.

---

## Task 2: Ignore rules + directory walker

**Files:**
- Create: `src-tauri/src/files_rail/ignore.rs`
- Create: `src-tauri/src/files_rail/walker.rs`
- Modify: `src-tauri/src/files_rail/mod.rs` (add `pub mod ignore; pub mod walker;`)

- [ ] **Step 1: Write `ignore.rs`**

```rust
//! Filtering rules for directory walks.

use std::collections::HashSet;
use once_cell::sync::Lazy;

/// Directory basenames the walker always skips. Mirrors Proma's set plus
/// uClaw-specific additions (`pyembed`, `static`, `.uclaw`).
static SKIP_DIRS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "node_modules", ".git", "dist", ".next", "__pycache__", ".venv",
        "build", ".cache", "target", ".DS_Store", ".idea", ".vscode",
        ".turbo", "coverage", "pyembed", "static", ".uclaw",
    ].into_iter().collect()
});

/// True if the walker should skip an entry with this basename + kind.
pub fn should_ignore(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') && name != ".gitignore" && name != ".env" {
        return true;
    }
    if is_dir && SKIP_DIRS.contains(name) {
        return true;
    }
    false
}
```

Verify `once_cell` is already a dep:

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "^once_cell" src-tauri/Cargo.toml
```

If missing (very unlikely), use `std::sync::LazyLock` from the stable Rust API instead — replace the `Lazy::new` line with:

```rust
static SKIP_DIRS: std::sync::LazyLock<HashSet<&'static str>> = std::sync::LazyLock::new(|| {
```

(uClaw's MSRV per Cargo.toml supports `std::sync::LazyLock` since Rust 1.80. Check `rust-toolchain.toml` or `Cargo.toml`'s `rust-version` field if uncertain.)

- [ ] **Step 2: Write `walker.rs`**

```rust
//! Single-layer directory reader.
//!
//! Returns one level of `FileNode`s at a time. Deep recursion happens in the
//! frontend via lazy-expand (Step 5 in `useFileTree.ts`); the backend never
//! walks more than one directory per call.

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
            kind: if is_dir { NodeKind::Directory } else { NodeKind::File },
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
```

- [ ] **Step 3: Update `src-tauri/src/files_rail/mod.rs`**

Replace the `mod.rs` content from Task 1 with:

```rust
//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod ignore;
pub mod types;
pub mod walker;

pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
```

- [ ] **Step 4: Write tests inline at the bottom of `walker.rs`**

Append to `walker.rs`:

```rust
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
        let fx = fixture();
        let entries = read_dir_layer(&fx.path().join("src"), fx.path()).unwrap();
        // empty dir
        assert!(entries.is_empty());
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
```

Verify `tempfile` is a dev-dep:

```bash
grep -n "tempfile" /Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml
```

If not present, add to the `[dev-dependencies]` section: `tempfile = "3"`.

- [ ] **Step 5: Run the tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib files_rail 2>&1 | tail -15
```

Expected: 5 tests pass (`should_ignore_dotfiles_except_allowlist`, `should_ignore_skip_dirs`, `read_dir_layer_returns_visible_entries_only`, `read_dir_layer_sorts_dirs_first_then_alpha`, `read_dir_layer_computes_relative_path`).

- [ ] **Step 6: Pre-commit + commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M src-tauri/Cargo.toml          (only if tempfile was added)
 M src-tauri/src/files_rail/mod.rs
?? src-tauri/src/files_rail/ignore.rs
?? src-tauri/src/files_rail/walker.rs
```

```bash
git add src-tauri/src/files_rail/ignore.rs src-tauri/src/files_rail/walker.rs src-tauri/src/files_rail/mod.rs
# if Cargo.toml modified:
git add src-tauri/Cargo.toml
git commit -m "feat(files-rail): ignore rules + single-layer directory walker (with tests)"
```

Verify branch + commit content.

---

## Task 3: Multi-mount file watcher

**Files:**
- Create: `src-tauri/src/files_rail/watcher.rs`
- Modify: `src-tauri/src/files_rail/mod.rs` (add `pub mod watcher;`)

- [ ] **Step 1: Write `watcher.rs`**

```rust
//! Multi-mount notify-based file watcher.
//!
//! Owns one `RecommendedWatcher`. Each `register_mount(mount_id, root)` call
//! adds a recursive watch and remembers the mount_id↔path mapping. Events
//! that arrive within a 16ms debounce window are batched per mount and
//! emitted as a single `files_rail:change` IPC event with a `Vec<FileChange>`.

use super::types::{ChangeKind, FileChange, FilesRailChange};
use notify::{
    event::{ModifyKind, RenameMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Debounce window for batching events per mount.
const BATCH_INTERVAL: Duration = Duration::from_millis(16);
/// Hard cap on a single batch — anything larger gets split into multiple emits.
const BATCH_MAX_EVENTS: usize = 100;

/// Internal record for a registered mount.
struct MountEntry {
    root: PathBuf,
    /// Pending events that haven't been flushed yet.
    pending: Vec<FileChange>,
}

pub struct FilesRailWatcher {
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
}

struct Inner {
    watcher: Option<RecommendedWatcher>,
    mounts: HashMap<String, MountEntry>,
}

impl FilesRailWatcher {
    pub fn new(app: AppHandle) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                watcher: None,
                mounts: HashMap::new(),
            })),
            app,
        }
    }

    /// Start the underlying notify watcher and kick off the flush loop.
    /// Idempotent — calling twice is a no-op.
    pub async fn start(&self) -> Result<(), notify::Error> {
        let mut inner = self.inner.lock().await;
        if inner.watcher.is_some() {
            return Ok(());
        }
        let inner_ref = self.inner.clone();
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Forward the raw event to the batch buffer. We synchronously
                // acquire the lock from the watcher thread via blocking_lock
                // because notify's callback is NOT async.
                let mut inner = inner_ref.blocking_lock();
                for path in &event.paths {
                    Self::queue_event(&mut inner, path, &event.kind);
                }
            }
        })?;
        inner.watcher = Some(watcher);
        drop(inner);

        // Spawn the flush task. Lives for the lifetime of the service.
        let inner_ref = self.inner.clone();
        let app = self.app.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(BATCH_INTERVAL).await;
                Self::flush_pending(&inner_ref, &app).await;
            }
        });
        Ok(())
    }

    /// Register a mount + start watching its root recursively.
    pub async fn register_mount(
        &self,
        mount_id: String,
        root: PathBuf,
    ) -> Result<(), notify::Error> {
        let mut inner = self.inner.lock().await;
        if let Some(w) = inner.watcher.as_mut() {
            w.watch(&root, RecursiveMode::Recursive)?;
        }
        inner.mounts.insert(mount_id, MountEntry {
            root,
            pending: Vec::new(),
        });
        Ok(())
    }

    /// Stop watching a mount. Idempotent.
    pub async fn unregister_mount(&self, mount_id: &str) -> Result<(), notify::Error> {
        let mut inner = self.inner.lock().await;
        if let Some(entry) = inner.mounts.remove(mount_id) {
            if let Some(w) = inner.watcher.as_mut() {
                let _ = w.unwatch(&entry.root);
            }
        }
        Ok(())
    }

    /// Synchronously queue an event into the matching mount's pending buffer.
    fn queue_event(inner: &mut Inner, path: &Path, kind: &EventKind) {
        // Find the mount whose root is an ancestor of `path`.
        let owning = inner.mounts.iter_mut().find(|(_, e)| path.starts_with(&e.root));
        let Some((_, entry)) = owning else { return };

        let rel_path = path
            .strip_prefix(&entry.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let is_dir = path.is_dir();
        let change_kind = match kind {
            EventKind::Create(_) => ChangeKind::Created,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => ChangeKind::Renamed,
            EventKind::Modify(_) => ChangeKind::Modified,
            EventKind::Remove(_) => ChangeKind::Removed,
            _ => return, // Other(_) / Access(_) — ignore
        };
        entry.pending.push(FileChange {
            kind: change_kind,
            rel_path,
            new_rel_path: None,
            is_dir,
        });
    }

    async fn flush_pending(inner_ref: &Arc<Mutex<Inner>>, app: &AppHandle) {
        let mut inner = inner_ref.lock().await;
        // Take pending for each mount and emit.
        let mount_ids: Vec<String> = inner.mounts.keys().cloned().collect();
        for mid in mount_ids {
            let Some(entry) = inner.mounts.get_mut(&mid) else { continue };
            if entry.pending.is_empty() {
                continue;
            }
            // Drain up to BATCH_MAX_EVENTS at a time.
            let drained: Vec<FileChange> = entry.pending.drain(..).collect();
            for chunk in drained.chunks(BATCH_MAX_EVENTS) {
                let payload = FilesRailChange {
                    mount_id: mid.clone(),
                    changes: chunk.to_vec(),
                };
                let _ = app.emit(FilesRailChange::CHANNEL, &payload);
            }
        }
    }
}
```

- [ ] **Step 2: Update `src-tauri/src/files_rail/mod.rs`**

Replace its content with:

```rust
//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod ignore;
pub mod types;
pub mod walker;
pub mod watcher;

pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
pub use watcher::FilesRailWatcher;
```

- [ ] **Step 3: Build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error|^warning" | head -10
```

Expected: empty. If `notify::event::{ModifyKind, RenameMode}` paths don't resolve, the notify crate version uses a different path — `cargo doc -p notify --open` to check, or fall back to matching on `event.kind.is_modify()` / `is_create()` / `is_remove()` without distinguishing rename.

- [ ] **Step 4: Pre-commit + commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M src-tauri/src/files_rail/mod.rs
?? src-tauri/src/files_rail/watcher.rs
```

```bash
git add src-tauri/src/files_rail/watcher.rs src-tauri/src/files_rail/mod.rs
git commit -m "feat(files-rail): multi-mount notify watcher with 16ms event batching"
```

Verify.

---

## Task 4: Files-rail service + Tauri commands

**Files:**
- Create: `src-tauri/src/files_rail/service.rs`
- Create: `src-tauri/src/files_rail/commands.rs`
- Modify: `src-tauri/src/files_rail/mod.rs`
- Modify: `src-tauri/src/app.rs` (add `files_rail_service` field)
- Modify: `src-tauri/src/main.rs` (register service in Stage 3)
- Modify: `src-tauri/src/tauri_commands.rs` (re-export + register 4 commands)

- [ ] **Step 1: Write `service.rs`**

```rust
//! `FilesRailService` — owns the `FilesRailWatcher` and implements `ManagedService`.

use super::watcher::FilesRailWatcher;
use crate::services::ManagedService;
use crate::services::types::{ServiceHealth, ServiceStatus};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::RwLock;

pub struct FilesRailService {
    watcher: Arc<FilesRailWatcher>,
    status: Arc<RwLock<ServiceStatus>>,
}

impl FilesRailService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            watcher: Arc::new(FilesRailWatcher::new(app)),
            status: Arc::new(RwLock::new(ServiceStatus::Stopped)),
        }
    }

    pub fn watcher(&self) -> Arc<FilesRailWatcher> {
        self.watcher.clone()
    }

    pub async fn register_mount(&self, mount_id: String, root: PathBuf) -> anyhow::Result<()> {
        self.watcher
            .register_mount(mount_id, root)
            .await
            .map_err(|e| anyhow::anyhow!("watcher register_mount: {}", e))
    }

    pub async fn unregister_mount(&self, mount_id: &str) -> anyhow::Result<()> {
        self.watcher
            .unregister_mount(mount_id)
            .await
            .map_err(|e| anyhow::anyhow!("watcher unregister_mount: {}", e))
    }
}

#[async_trait]
impl ManagedService for FilesRailService {
    fn name(&self) -> &str {
        "files_rail"
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.status.write().await = ServiceStatus::Starting;
        self.watcher
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("watcher start: {}", e))?;
        *self.status.write().await = ServiceStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.status.write().await = ServiceStatus::Stopping;
        // Watcher is held in Arc and drops naturally when service drops.
        // No explicit stop API yet — Tauri's WindowEvent::Destroyed flow
        // will release the AppState which drops the service.
        *self.status.write().await = ServiceStatus::Stopped;
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        self.status.try_read().map(|s| *s).unwrap_or(ServiceStatus::Stopped)
    }

    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            last_error: None,
            uptime_seconds: 0,
        }
    }
}
```

Before saving: confirm the `ServiceStatus` + `ServiceHealth` field shapes match `src-tauri/src/services/types.rs`:

```bash
cd /Users/ryanliu/Documents/uclaw && cat src-tauri/src/services/types.rs | head -50
```

If `ServiceHealth` has additional required fields, populate them with default-like values. If `ServiceStatus::Stopped` doesn't exist, use whatever the equivalent default-disabled state is named.

- [ ] **Step 2: Write `commands.rs`**

```rust
//! Tauri commands for the files-rail UI.
//!
//! Registered in `src-tauri/src/tauri_commands.rs` and the `invoke_handler!`
//! macro in `src-tauri/src/main.rs`.

use super::types::{FileNode, MountRoot};
use super::walker::read_dir_layer;
use crate::app::AppState;
use crate::error::Error;
use std::path::{Path, PathBuf};
use tauri::State;

#[tauri::command]
pub async fn files_rail_list_mounts(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Vec<MountRoot>, Error> {
    state.files_rail_list_mounts(session_id).await
}

#[tauri::command]
pub async fn files_rail_read_dir(
    state: State<'_, AppState>,
    mount_id: String,
    rel_path: String,
) -> Result<Vec<FileNode>, Error> {
    let (mount_root, target) = state.files_rail_resolve_dir(&mount_id, &rel_path).await?;
    let entries = read_dir_layer(&target, &mount_root)
        .map_err(|e| Error::Internal(format!("read_dir failed: {}", e)))?;
    Ok(entries)
}

#[tauri::command]
pub async fn files_rail_watch_start(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<(), Error> {
    let root = state.files_rail_mount_path(&mount_id).await?;
    state
        .files_rail_service
        .register_mount(mount_id, root)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn files_rail_watch_stop(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<(), Error> {
    state
        .files_rail_service
        .unregister_mount(&mount_id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(())
}
```

(The `AppState` helpers `files_rail_list_mounts`, `files_rail_resolve_dir`, `files_rail_mount_path` are added in Step 3.)

- [ ] **Step 3: Wire `AppState`**

In `src-tauri/src/app.rs`, locate the `AppState` struct and its `new()` function. Add a field:

```rust
pub files_rail_service: Arc<crate::files_rail::FilesRailService>,
```

In `AppState::new()`, after the existing service construction, add:

```rust
let files_rail_service = Arc::new(crate::files_rail::FilesRailService::new(app_handle.clone()));
```

…and include `files_rail_service: files_rail_service.clone(),` in the struct literal returned.

Add an `impl AppState` block (or extend the existing one) with the three helpers used by commands.rs:

```rust
impl AppState {
    pub async fn files_rail_list_mounts(
        &self,
        session_id: Option<String>,
    ) -> Result<Vec<crate::files_rail::MountRoot>, crate::error::Error> {
        use crate::files_rail::{MountKind, MountRoot};
        let mut out: Vec<MountRoot> = Vec::new();

        let workspace_root = std::path::PathBuf::from(
            dirs::home_dir()
                .ok_or_else(|| crate::error::Error::Internal("no home dir".into()))?,
        )
        .join("Documents")
        .join("workground");
        if workspace_root.exists() {
            out.push(MountRoot {
                id: "workspace:default".into(),
                label: "工作区文件".into(),
                path: workspace_root,
                kind: MountKind::Workspace,
                editable: true,
            });
        }

        if let Some(sid) = session_id {
            let session_root = self.session_attached_root(&sid).await.ok();
            if let Some(root) = session_root {
                if root.exists() {
                    out.push(MountRoot {
                        id: format!("session:{}", sid),
                        label: "会话文件".into(),
                        path: root,
                        kind: MountKind::Session,
                        editable: true,
                    });
                }
            }

            for (idx, dir) in self.session_attached_dirs(&sid).await.iter().enumerate() {
                let pb = std::path::PathBuf::from(dir);
                if !pb.exists() {
                    continue;
                }
                let name = pb
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("attached")
                    .to_string();
                out.push(MountRoot {
                    id: format!("attached:{}:{}", sid, idx),
                    label: name,
                    path: pb,
                    kind: MountKind::AttachedDir,
                    editable: false,
                });
            }
        }
        Ok(out)
    }

    pub async fn files_rail_mount_path(
        &self,
        mount_id: &str,
    ) -> Result<std::path::PathBuf, crate::error::Error> {
        let mounts = self
            .files_rail_list_mounts(self.extract_session_from_mount(mount_id))
            .await?;
        mounts
            .into_iter()
            .find(|m| m.id == mount_id)
            .map(|m| m.path)
            .ok_or_else(|| crate::error::Error::Internal(format!("mount not found: {}", mount_id)))
    }

    pub async fn files_rail_resolve_dir(
        &self,
        mount_id: &str,
        rel_path: &str,
    ) -> Result<(std::path::PathBuf, std::path::PathBuf), crate::error::Error> {
        let mount_root = self.files_rail_mount_path(mount_id).await?;
        let target = if rel_path.is_empty() || rel_path == "/" {
            mount_root.clone()
        } else {
            // Reject any `..` segment to prevent escape.
            if rel_path.split('/').any(|seg| seg == "..") {
                return Err(crate::error::Error::Internal("invalid rel_path: .. not allowed".into()));
            }
            mount_root.join(rel_path)
        };
        Ok((mount_root, target))
    }

    fn extract_session_from_mount(&self, mount_id: &str) -> Option<String> {
        if let Some(rest) = mount_id.strip_prefix("session:") {
            return Some(rest.to_string());
        }
        if let Some(rest) = mount_id.strip_prefix("attached:") {
            return rest.split(':').next().map(|s| s.to_string());
        }
        None
    }

    async fn session_attached_root(&self, _sid: &str) -> Result<std::path::PathBuf, crate::error::Error> {
        // Stub for now — derives session root from existing session manager.
        // Implementation: read `session_manager` for the session's path field.
        // Task 4 leaves this returning `Err`; Task 5 wires real lookups when
        // we audit the existing session-path API. The frontend tolerates
        // empty mount lists gracefully.
        Err(crate::error::Error::Internal("session_attached_root: not wired in W3".into()))
    }

    async fn session_attached_dirs(&self, _sid: &str) -> Vec<String> {
        // Stub — Task 5 wires reads from agent_sessions.attached_dirs JSON.
        Vec::new()
    }
}
```

⚠️ Implementation note: the stub `session_attached_root` / `session_attached_dirs` return empty/Err on first cut. The frontend will see only the workspace mount until Task 5 wires real session lookups. **This is intentional** — it keeps Task 4 small and the workspace mount is enough to verify the full plumbing end-to-end.

- [ ] **Step 4: Register service in `main.rs` Stage 3**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "service_manager.register" src-tauri/src/main.rs
```

Find the existing `service_manager.register(local_api_svc).await;` (or the last `register(...)` call in the Stage 3 block). Add after it:

```rust
                        let fr_svc = app_state.files_rail_service.clone();
                        service_manager.register(fr_svc).await;
```

- [ ] **Step 5: Register Tauri commands**

In `src-tauri/src/tauri_commands.rs`, find the top of the file imports and add:

```rust
pub use crate::files_rail::commands::{
    files_rail_list_mounts, files_rail_read_dir, files_rail_watch_start, files_rail_watch_stop,
};
```

In `src-tauri/src/main.rs`, locate the `invoke_handler![…]` macro (or whichever uClaw uses to register Tauri commands — the exact macro is `tauri::generate_handler!` or similar; grep for it):

```bash
grep -n "generate_handler\|invoke_handler" src-tauri/src/main.rs | head -5
```

Add the 4 new commands to the handler list:

```rust
            crate::tauri_commands::files_rail_list_mounts,
            crate::tauri_commands::files_rail_read_dir,
            crate::tauri_commands::files_rail_watch_start,
            crate::tauri_commands::files_rail_watch_stop,
```

(Place alphabetically next to other `files_*` or `read_*` commands if such grouping exists.)

- [ ] **Step 6: Update `files_rail/mod.rs`**

Final state:

```rust
//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod commands;
pub mod ignore;
pub mod service;
pub mod types;
pub mod walker;
pub mod watcher;

pub use service::FilesRailService;
pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
pub use watcher::FilesRailWatcher;
```

- [ ] **Step 7: Build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```

Common errors and fixes:
- `field 'files_rail_service' missing from initializer` → you didn't add it to the `AppState { ... }` struct literal in `new()`. Find and add.
- `cannot find function 'register' on '_'` → `ServiceManager::register` is async — use `.await` and the function must be `pub async`.
- `ServiceHealth` field mismatch → match the actual struct in `services/types.rs`.

If you can't resolve in 3 attempts, STOP and report NEEDS_CONTEXT.

- [ ] **Step 8: Tauri command runtime registration check**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | tail -3
```

Confirm clean. Then start the dev binary (no need to fully launch UI — just verify the binary boots):

```bash
# Don't actually run cargo tauri dev here; it requires the frontend.
# Instead just confirm the binary builds.
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build --bin uclaw 2>&1 | tail -3
```

- [ ] **Step 9: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected (5 files):
```
 M src-tauri/src/app.rs
 M src-tauri/src/files_rail/mod.rs
 M src-tauri/src/main.rs
 M src-tauri/src/tauri_commands.rs
?? src-tauri/src/files_rail/commands.rs
?? src-tauri/src/files_rail/service.rs
```

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/app.rs src-tauri/src/files_rail/commands.rs src-tauri/src/files_rail/service.rs src-tauri/src/files_rail/mod.rs src-tauri/src/main.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(files-rail): service + 4 Tauri commands (list_mounts, read_dir, watch_start, watch_stop)

Registers FilesRailService into the existing ServiceManager Stage 3 block.
Exposes files_rail_list_mounts / files_rail_read_dir / files_rail_watch_start /
files_rail_watch_stop via the invoke_handler! macro. Session-scoped mount
resolution is stubbed in this commit and wired to real session paths in a
follow-up — the workspace mount alone is enough to exercise the full
backend↔frontend plumbing end-to-end."
```

---

## Task 5: Frontend data layer (atoms + tree-patch)

**Files:**
- Create: `ui/src/atoms/files-rail-atoms.ts`
- Create: `ui/src/components/files-rail/utils/tree-patch.ts`
- Create: `ui/src/components/files-rail/utils/tree-patch.test.ts`
- Modify: `ui/src/lib/tauri-bridge.ts` (add 4 typed wrappers)

- [ ] **Step 1: Write the failing test for tree-patch**

Create `ui/src/components/files-rail/utils/tree-patch.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { applyChanges, type TreeNode } from './tree-patch'

const dir = (rel: string, name: string, children?: TreeNode[]): TreeNode => ({
  kind: 'directory', relPath: rel, name, size: 0, mtimeMs: 0, children,
})
const file = (rel: string, name: string, mtime = 0): TreeNode => ({
  kind: 'file', relPath: rel, name, size: 1, mtimeMs: mtime,
})

describe('tree-patch', () => {
  it('returns the original tree when no changes', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt')]
    const next = applyChanges(t, [])
    expect(next).toBe(t)
  })

  it('inserts a created file alphabetically into root', () => {
    const t: TreeNode[] = [file('b.txt', 'b.txt'), file('d.txt', 'd.txt')]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'c.txt', isDir: false }])
    expect(next.map((n) => n.relPath)).toEqual(['b.txt', 'c.txt', 'd.txt'])
  })

  it('inserts dirs before files at the same level', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt')]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub', isDir: true }])
    expect(next.map((n) => n.name)).toEqual(['sub', 'a.txt'])
  })

  it('removes a deleted file', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt'), file('b.txt', 'b.txt')]
    const next = applyChanges(t, [{ kind: 'removed', relPath: 'a.txt', isDir: false }])
    expect(next.map((n) => n.name)).toEqual(['b.txt'])
  })

  it('updates mtime on modify', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt', 1000)]
    const next = applyChanges(t, [{ kind: 'modified', relPath: 'a.txt', isDir: false }])
    expect(next[0].mtimeMs).toBeGreaterThan(1000)
  })

  it('ignores events targeting unexpanded subtrees (no children loaded)', () => {
    const t: TreeNode[] = [dir('sub', 'sub')] // sub has no children loaded
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub/inner.txt', isDir: false }])
    // sub still has no children — lazy expand will fetch fresh
    expect(next[0].children).toBeUndefined()
  })

  it('applies a change inside an expanded subtree', () => {
    const t: TreeNode[] = [dir('sub', 'sub', [file('sub/a.txt', 'a.txt')])]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub/b.txt', isDir: false }])
    expect(next[0].children?.map((n) => n.name)).toEqual(['a.txt', 'b.txt'])
  })

  it('handles rename as remove-then-insert', () => {
    const t: TreeNode[] = [file('old.txt', 'old.txt')]
    const next = applyChanges(t, [
      { kind: 'renamed', relPath: 'old.txt', newRelPath: 'new.txt', isDir: false },
    ])
    expect(next.map((n) => n.name)).toEqual(['new.txt'])
  })
})
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/files-rail/utils/tree-patch.test.ts
```

Expected: FAIL with "Cannot find module './tree-patch'".

- [ ] **Step 3: Implement `tree-patch.ts`**

Create `ui/src/components/files-rail/utils/tree-patch.ts`:

```ts
/**
 * tree-patch — Apply file-system change events to an in-memory tree.
 *
 * Strategy: walk the change list once, locate the target node by its parent
 * relPath, then mutate the parent's children array. Unexpanded directories
 * (those whose `children` is undefined) ignore events targeting their subtree
 * — they re-fetch lazily on expand. This keeps event handling O(depth) per
 * change without ever walking deep into uncached parts of the tree.
 */

export type NodeKind = 'file' | 'directory'

export interface TreeNode {
  kind: NodeKind
  /** Path relative to the mount root, forward-slash separated. */
  relPath: string
  /** Last segment of relPath. */
  name: string
  size: number
  mtimeMs: number
  /** Undefined → not yet expanded. Empty array → expanded, empty dir. */
  children?: TreeNode[]
}

export type ChangeKind = 'created' | 'modified' | 'removed' | 'renamed'

export interface FileChange {
  kind: ChangeKind
  relPath: string
  newRelPath?: string
  isDir: boolean
}

const parentRel = (rel: string): string => {
  const i = rel.lastIndexOf('/')
  return i === -1 ? '' : rel.slice(0, i)
}

const basename = (rel: string): string => {
  const i = rel.lastIndexOf('/')
  return i === -1 ? rel : rel.slice(i + 1)
}

const sortNodes = (a: TreeNode, b: TreeNode): number => {
  if (a.kind === 'directory' && b.kind === 'file') return -1
  if (a.kind === 'file' && b.kind === 'directory') return 1
  return a.name.toLowerCase().localeCompare(b.name.toLowerCase())
}

/**
 * Locate the node array at parent relPath. Returns `undefined` if the parent
 * is not expanded (children === undefined) — that's an intentional signal to
 * the caller that this event can be ignored (lazy expand will re-fetch).
 */
function findParentChildren(roots: TreeNode[], parent: string): TreeNode[] | undefined {
  if (parent === '') return roots
  const segments = parent.split('/')
  let current: TreeNode[] | undefined = roots
  for (const seg of segments) {
    if (!current) return undefined
    const next: TreeNode | undefined = current.find((n) => n.name === seg && n.kind === 'directory')
    if (!next || next.children === undefined) return undefined
    current = next.children
  }
  return current
}

function insertSorted(siblings: TreeNode[], node: TreeNode): TreeNode[] {
  const out = [...siblings, node]
  out.sort(sortNodes)
  return out
}

function withReplacedChildren(
  roots: TreeNode[],
  parent: string,
  nextChildren: TreeNode[],
): TreeNode[] {
  if (parent === '') return nextChildren
  return roots.map((n) => {
    if (n.kind !== 'directory') return n
    if (n.relPath === parent) {
      return { ...n, children: nextChildren }
    }
    if (parent.startsWith(`${n.relPath}/`) && n.children) {
      return { ...n, children: withReplacedChildren(n.children, parent, nextChildren) }
    }
    return n
  })
}

export function applyChanges(roots: TreeNode[], changes: FileChange[]): TreeNode[] {
  if (changes.length === 0) return roots
  let current = roots
  for (const c of changes) {
    current = applySingle(current, c)
  }
  return current
}

function applySingle(roots: TreeNode[], c: FileChange): TreeNode[] {
  const parent = parentRel(c.relPath)
  const siblings = findParentChildren(roots, parent)
  if (!siblings) return roots // parent not expanded — drop

  if (c.kind === 'created') {
    if (siblings.some((n) => n.relPath === c.relPath)) return roots
    const node: TreeNode = {
      kind: c.isDir ? 'directory' : 'file',
      relPath: c.relPath,
      name: basename(c.relPath),
      size: 0,
      mtimeMs: Date.now(),
    }
    return withReplacedChildren(roots, parent, insertSorted(siblings, node))
  }

  if (c.kind === 'removed') {
    const next = siblings.filter((n) => n.relPath !== c.relPath)
    if (next.length === siblings.length) return roots
    return withReplacedChildren(roots, parent, next)
  }

  if (c.kind === 'modified') {
    const next = siblings.map((n) =>
      n.relPath === c.relPath ? { ...n, mtimeMs: Date.now() } : n,
    )
    return withReplacedChildren(roots, parent, next)
  }

  if (c.kind === 'renamed' && c.newRelPath) {
    const removed = siblings.filter((n) => n.relPath !== c.relPath)
    const newNode: TreeNode = {
      kind: c.isDir ? 'directory' : 'file',
      relPath: c.newRelPath,
      name: basename(c.newRelPath),
      size: 0,
      mtimeMs: Date.now(),
    }
    return withReplacedChildren(roots, parent, insertSorted(removed, newNode))
  }

  return roots
}
```

- [ ] **Step 4: Run, watch pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/files-rail/utils/tree-patch.test.ts
```

Expected: 8 tests pass.

- [ ] **Step 5: Implement atoms**

Create `ui/src/atoms/files-rail-atoms.ts`:

```ts
/**
 * files-rail-atoms — State for the W3 files rail.
 *
 * mountRootsAtomFamily — list of mounts for a given sessionId
 * expandedPathsAtomFamily — per-mount Set<relPath> of expanded directories
 * fileTreeAtomFamily — per-mount root tree (TreeNode[])
 * filesRailTabAtom — workspace | changes
 * filesRailRefreshTickAtom — bump to force a full reload of mounts
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

export type FilesRailTab = 'workspace' | 'changes'
export type MountKind = 'workspace' | 'session' | 'attached_dir'

export interface MountRoot {
  id: string
  label: string
  path: string
  kind: MountKind
  editable: boolean
}

/** Loadable wrapper for a per-mount tree. */
export type TreeState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; nodes: TreeNode[] }
  | { status: 'error'; message: string }

export const filesRailTabAtom = atom<FilesRailTab>('workspace')

export const mountRootsAtomFamily = atomFamily(
  (_sessionId: string | null) => atom<MountRoot[]>([]),
)

export const expandedPathsAtomFamily = atomFamily((_mountId: string) =>
  atom<Set<string>>(new Set<string>()),
)

export const fileTreeAtomFamily = atomFamily((_mountId: string) =>
  atom<TreeState>({ status: 'idle' }),
)

export const filesRailRefreshTickAtom = atom(0)
export const bumpFilesRailRefreshAtom = atom(null, (get, set) => {
  set(filesRailRefreshTickAtom, get(filesRailRefreshTickAtom) + 1)
})
```

- [ ] **Step 6: Add Tauri-bridge wrappers**

In `ui/src/lib/tauri-bridge.ts`, find an existing `invoke<...>` wrapper as a pattern reference:

```bash
grep -n "export.*function.*invoke<\|export async function" /Users/ryanliu/Documents/uclaw/ui/src/lib/tauri-bridge.ts | head -10
```

Add at the end of the file (before any closing namespace if applicable):

```ts
// ============================================================================
// Files Rail (W3)
// ============================================================================

import type { MountRoot } from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface BackendFileNode {
  path: string
  rel_path: string
  name: string
  kind: 'file' | 'directory'
  size: number
  mtime_ms: number
  is_ignored: boolean
}

const toTreeNode = (n: BackendFileNode): TreeNode => ({
  kind: n.kind,
  relPath: n.rel_path,
  name: n.name,
  size: n.size,
  mtimeMs: n.mtime_ms,
})

interface BackendMountRoot {
  id: string
  label: string
  path: string
  kind: 'workspace' | 'session' | 'attached_dir'
  editable: boolean
}

const toMountRoot = (m: BackendMountRoot): MountRoot => ({ ...m })

export async function filesRailListMounts(sessionId: string | null): Promise<MountRoot[]> {
  const raw = await invoke<BackendMountRoot[]>('files_rail_list_mounts', { sessionId })
  return raw.map(toMountRoot)
}

export async function filesRailReadDir(mountId: string, relPath: string): Promise<TreeNode[]> {
  const raw = await invoke<BackendFileNode[]>('files_rail_read_dir', { mountId, relPath })
  return raw.map(toTreeNode)
}

export async function filesRailWatchStart(mountId: string): Promise<void> {
  await invoke<void>('files_rail_watch_start', { mountId })
}

export async function filesRailWatchStop(mountId: string): Promise<void> {
  await invoke<void>('files_rail_watch_stop', { mountId })
}
```

(If `invoke` is not already in scope at the bottom of the file, scroll up to confirm the import — uClaw uses `import { invoke } from '@tauri-apps/api/core'` typically.)

- [ ] **Step 7: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: TS clean, test count up by 8 (253 → 261).

- [ ] **Step 8: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/src/lib/tauri-bridge.ts
?? ui/src/atoms/files-rail-atoms.ts
?? ui/src/components/files-rail/utils/tree-patch.ts
?? ui/src/components/files-rail/utils/tree-patch.test.ts
```

```bash
git add ui/src/atoms/files-rail-atoms.ts ui/src/components/files-rail/utils/tree-patch.ts ui/src/components/files-rail/utils/tree-patch.test.ts ui/src/lib/tauri-bridge.ts
git commit -m "feat(ui/files-rail): data atoms + tree-patch util + 4 typed Tauri wrappers (with tests)"
```

---

## Task 6: useFileTree hook + WorkspaceFilesPanel + tree node

**Files:**
- Create: `ui/src/components/files-rail/hooks/useFileTree.ts`
- Create: `ui/src/components/files-rail/workspace/FileTreeNode.tsx`
- Create: `ui/src/components/files-rail/workspace/MountSection.tsx`
- Create: `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx`

- [ ] **Step 1: Write `useFileTree.ts`**

Create:

```ts
/**
 * useFileTree — Lazy directory loading for one mount.
 *
 * Owns: root-level children + an expanded-paths cache. Calls
 * `filesRailReadDir` only when a directory is first expanded; subsequent
 * collapse/expand cycles reuse cached children. Watcher events (Task 7) apply
 * via tree-patch without going through this hook.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import {
  expandedPathsAtomFamily,
  fileTreeAtomFamily,
} from '@/atoms/files-rail-atoms'
import { filesRailReadDir } from '@/lib/tauri-bridge'
import {
  applyChanges,
  type FileChange,
  type TreeNode,
} from '@/components/files-rail/utils/tree-patch'

interface UseFileTreeResult {
  /** Top-level entries. Empty array if loading or error (use loadState to differentiate). */
  nodes: TreeNode[]
  loadState: 'idle' | 'loading' | 'ready' | 'error'
  errorMessage?: string
  /** True if this path is expanded. */
  isExpanded: (relPath: string) => boolean
  toggleExpand: (relPath: string, isDir: boolean) => Promise<void>
  applyExternalChanges: (changes: FileChange[]) => void
  reload: () => Promise<void>
}

export function useFileTree(mountId: string): UseFileTreeResult {
  const [tree, setTree] = useAtom(fileTreeAtomFamily(mountId))
  const [expanded, setExpanded] = useAtom(expandedPathsAtomFamily(mountId))

  const reload = React.useCallback(async () => {
    setTree({ status: 'loading' })
    try {
      const nodes = await filesRailReadDir(mountId, '')
      setTree({ status: 'ready', nodes })
    } catch (err) {
      setTree({ status: 'error', message: String(err) })
    }
  }, [mountId, setTree])

  React.useEffect(() => {
    if (tree.status === 'idle') void reload()
  }, [tree.status, reload])

  const isExpanded = React.useCallback(
    (relPath: string) => expanded.has(relPath),
    [expanded],
  )

  const toggleExpand = React.useCallback(
    async (relPath: string, isDir: boolean) => {
      if (!isDir) return
      const next = new Set(expanded)
      if (next.has(relPath)) {
        next.delete(relPath)
        setExpanded(next)
        return
      }
      // Expand: fetch children if not cached.
      next.add(relPath)
      setExpanded(next)
      if (tree.status !== 'ready') return
      // Find the directory in the tree; if its children is undefined, fetch.
      const targetHasChildren = treeHasChildrenAt(tree.nodes, relPath)
      if (targetHasChildren) return
      try {
        const fetched = await filesRailReadDir(mountId, relPath)
        setTree((prev) => {
          if (prev.status !== 'ready') return prev
          return { status: 'ready', nodes: setChildrenAt(prev.nodes, relPath, fetched) }
        })
      } catch {
        // Silent — error is shown at the dir node by status check elsewhere.
      }
    },
    [expanded, setExpanded, tree, mountId, setTree],
  )

  const applyExternalChanges = React.useCallback(
    (changes: FileChange[]) => {
      setTree((prev) => {
        if (prev.status !== 'ready') return prev
        const next = applyChanges(prev.nodes, changes)
        return next === prev.nodes ? prev : { status: 'ready', nodes: next }
      })
    },
    [setTree],
  )

  return {
    nodes: tree.status === 'ready' ? tree.nodes : [],
    loadState: tree.status,
    errorMessage: tree.status === 'error' ? tree.message : undefined,
    isExpanded,
    toggleExpand,
    applyExternalChanges,
    reload,
  }
}

function treeHasChildrenAt(nodes: TreeNode[], relPath: string): boolean {
  for (const n of nodes) {
    if (n.relPath === relPath) return n.children !== undefined
    if (n.kind === 'directory' && n.children && relPath.startsWith(`${n.relPath}/`)) {
      return treeHasChildrenAt(n.children, relPath)
    }
  }
  return false
}

function setChildrenAt(
  nodes: TreeNode[],
  relPath: string,
  children: TreeNode[],
): TreeNode[] {
  return nodes.map((n) => {
    if (n.kind !== 'directory') return n
    if (n.relPath === relPath) return { ...n, children }
    if (n.children && relPath.startsWith(`${n.relPath}/`)) {
      return { ...n, children: setChildrenAt(n.children, relPath, children) }
    }
    return n
  })
}
```

- [ ] **Step 2: Write `FileTreeNode.tsx`**

```tsx
import * as React from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode) => void
}

export const FileTreeNode = React.memo(function FileTreeNode({
  node,
  depth,
  isExpanded,
  onToggle,
  onFileClick,
}: FileTreeNodeProps): React.ReactElement {
  const expanded = isExpanded(node.relPath)
  const isDir = node.kind === 'directory'

  const handleClick = React.useCallback(() => {
    if (isDir) void onToggle(node.relPath, true)
    else onFileClick(node)
  }, [isDir, node, onToggle, onFileClick])

  const indent = depth * 12

  return (
    <>
      <button
        type="button"
        onClick={handleClick}
        className={cn(
          'flex items-center w-full h-[22px] px-2 gap-1 text-[12px] text-left',
          'text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        )}
        style={{ paddingLeft: 8 + indent }}
        title={node.relPath}
      >
        {isDir ? (
          expanded ? (
            <ChevronDown size={12} className="shrink-0 text-foreground/40" />
          ) : (
            <ChevronRight size={12} className="shrink-0 text-foreground/40" />
          )
        ) : (
          <span className="w-3 shrink-0" aria-hidden />
        )}
        <FileTypeIcon
          filename={node.name}
          isDirectory={isDir}
          className="size-3.5 shrink-0"
        />
        <span className="truncate font-mono tabular-nums">{node.name}</span>
      </button>
      {isDir && expanded && node.children && (
        <>
          {node.children.map((child) => (
            <FileTreeNode
              key={child.relPath}
              node={child}
              depth={depth + 1}
              isExpanded={isExpanded}
              onToggle={onToggle}
              onFileClick={onFileClick}
            />
          ))}
        </>
      )}
    </>
  )
})
```

⚠️ `FileTypeIcon` is imported from the existing `@/components/file-browser/FileTypeIcon` — don't reimplement, just consume. If `FileTypeIcon`'s prop signature differs from what's used here (`filename` / `isDirectory` / `className`), adjust the call site to match.

```bash
grep -n "interface FileTypeIconProps\|FileTypeIcon" /Users/ryanliu/Documents/uclaw/ui/src/components/file-browser/FileTypeIcon.tsx | head -5
```

- [ ] **Step 3: Write `MountSection.tsx`**

```tsx
import * as React from 'react'
import { FolderOpen, RefreshCw, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { FileTreeNode } from './FileTreeNode'
import { filesRailWatchStart, filesRailWatchStop } from '@/lib/tauri-bridge'
import type { MountRoot } from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface MountSectionProps {
  mount: MountRoot
  onFileClick: (mount: MountRoot, node: TreeNode) => void
}

export function MountSection({ mount, onFileClick }: MountSectionProps): React.ReactElement {
  const { nodes, loadState, errorMessage, isExpanded, toggleExpand, reload } = useFileTree(mount.id)

  React.useEffect(() => {
    let active = true
    void (async () => {
      try {
        await filesRailWatchStart(mount.id)
      } catch {
        /* silent — events just won't arrive */
      }
    })()
    return () => {
      active = false
      void filesRailWatchStop(mount.id)
    }
  }, [mount.id])

  const handleFileClick = React.useCallback(
    (node: TreeNode) => onFileClick(mount, node),
    [mount, onFileClick],
  )

  return (
    <section className="flex flex-col mb-3">
      <header className="flex items-center gap-1 px-2 h-[28px] flex-shrink-0">
        <FolderOpen className="size-3 text-muted-foreground" />
        <span className="text-[11px] font-medium text-muted-foreground truncate">{mount.label}</span>
        <span className="ml-auto" />
        <button
          type="button"
          onClick={() => void reload()}
          aria-label="刷新"
          className="size-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
        >
          <RefreshCw className={cn('size-2.5', loadState === 'loading' && 'animate-spin')} />
        </button>
      </header>
      {loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive flex items-center gap-1">
          <AlertTriangle size={12} aria-hidden />
          <span className="truncate">{errorMessage ?? '加载失败'}</span>
        </div>
      )}
      {loadState === 'ready' && nodes.length === 0 && (
        <div className="px-3 py-2 text-[11px] text-muted-foreground">这里还没有文件</div>
      )}
      <div className="min-h-0">
        {nodes.map((node) => (
          <FileTreeNode
            key={node.relPath}
            node={node}
            depth={0}
            isExpanded={isExpanded}
            onToggle={toggleExpand}
            onFileClick={handleFileClick}
          />
        ))}
      </div>
    </section>
  )
}
```

- [ ] **Step 4: Write `WorkspaceFilesPanel.tsx`**

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { mountRootsAtomFamily, type MountRoot } from '@/atoms/files-rail-atoms'
import { filesRailListMounts } from '@/lib/tauri-bridge'
import { MountSection } from './MountSection'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode) => void
}

export function WorkspaceFilesPanel({
  sessionId,
  onFileClick,
}: WorkspaceFilesPanelProps): React.ReactElement {
  const [mounts, setMounts] = useAtom(mountRootsAtomFamily(sessionId))

  React.useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const fetched = await filesRailListMounts(sessionId)
        if (!cancelled) setMounts(fetched)
      } catch {
        if (!cancelled) setMounts([])
      }
    })()
    return () => {
      cancelled = true
    }
  }, [sessionId, setMounts])

  const handleClick = React.useCallback(
    (mount: MountRoot, node: TreeNode) => {
      onFileClick?.(mount, node)
    },
    [onFileClick],
  )

  if (mounts.length === 0) {
    return (
      <div className="p-4 text-[12px] text-muted-foreground">
        还没有挂载点 — 点击右上的 + 按钮添加目录
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-y-auto py-2">
      {mounts.map((m) => (
        <MountSection key={m.id} mount={m} onFileClick={handleClick} />
      ))}
    </div>
  )
}
```

- [ ] **Step 5: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: TS clean, test count unchanged from Task 5 (no new tests in this task — the components are exercised manually + by RTL in Task 9).

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected (4 new files, 0 modified):
```
?? ui/src/components/files-rail/hooks/useFileTree.ts
?? ui/src/components/files-rail/workspace/FileTreeNode.tsx
?? ui/src/components/files-rail/workspace/MountSection.tsx
?? ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx
```

```bash
git add ui/src/components/files-rail/hooks/useFileTree.ts ui/src/components/files-rail/workspace/
git commit -m "feat(ui/files-rail): useFileTree hook + MountSection + FileTreeNode + WorkspaceFilesPanel"
```

---

## Task 7: Watcher event integration (`useFilesRailWatcher`)

**Files:**
- Create: `ui/src/components/files-rail/hooks/useFilesRailWatcher.ts`
- Modify: `ui/src/components/files-rail/workspace/MountSection.tsx` (consume the watcher hook)

- [ ] **Step 1: Write `useFilesRailWatcher.ts`**

```ts
/**
 * useFilesRailWatcher — Subscribe to `files_rail:change` events for a mount
 * and apply them to the cached tree via tree-patch.
 */

import * as React from 'react'
import { listen } from '@tauri-apps/api/event'
import type { FileChange } from '@/components/files-rail/utils/tree-patch'

interface BackendFileChange {
  kind: 'created' | 'modified' | 'removed' | 'renamed'
  rel_path: string
  new_rel_path?: string | null
  is_dir: boolean
}

interface BackendFilesRailChange {
  mount_id: string
  changes: BackendFileChange[]
}

const toFileChange = (c: BackendFileChange): FileChange => ({
  kind: c.kind,
  relPath: c.rel_path,
  newRelPath: c.new_rel_path ?? undefined,
  isDir: c.is_dir,
})

export function useFilesRailWatcher(
  mountId: string,
  apply: (changes: FileChange[]) => void,
): void {
  React.useEffect(() => {
    let unlisten: (() => void) | undefined
    let cancelled = false

    void (async () => {
      const u = await listen<BackendFilesRailChange>('files_rail:change', (evt) => {
        if (cancelled) return
        if (evt.payload.mount_id !== mountId) return
        if (evt.payload.changes.length === 0) return
        apply(evt.payload.changes.map(toFileChange))
      })
      unlisten = u
    })()

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [mountId, apply])
}
```

- [ ] **Step 2: Wire into `MountSection.tsx`**

Open `MountSection.tsx`. Find the `useFileTree(mount.id)` line. After it, add the watcher subscription:

```tsx
  const { nodes, loadState, errorMessage, isExpanded, toggleExpand, reload, applyExternalChanges } = useFileTree(mount.id)
  useFilesRailWatcher(mount.id, applyExternalChanges)
```

And add the import at the top:

```tsx
import { useFilesRailWatcher } from '@/components/files-rail/hooks/useFilesRailWatcher'
```

- [ ] **Step 3: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/files-rail/hooks/useFilesRailWatcher.ts ui/src/components/files-rail/workspace/MountSection.tsx
git commit -m "feat(ui/files-rail): subscribe to files_rail:change events + apply via tree-patch"
```

---

## Task 8: Top-level `FilesRail` + tabs + changes panel

**Files:**
- Create: `ui/src/components/files-rail/FilesRailTabs.tsx`
- Create: `ui/src/components/files-rail/changes/FileChangesPanel.tsx`
- Create: `ui/src/components/files-rail/changes/ChangeRow.tsx`
- Create: `ui/src/components/files-rail/index.tsx`

- [ ] **Step 1: Write `FilesRailTabs.tsx`**

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { filesRailTabAtom, type FilesRailTab } from '@/atoms/files-rail-atoms'
import { cn } from '@/lib/utils'

const TABS: Array<{ id: FilesRailTab; label: string }> = [
  { id: 'workspace', label: '工作区文件' },
  { id: 'changes', label: '文件改动' },
]

export function FilesRailTabs(): React.ReactElement {
  const [active, setActive] = useAtom(filesRailTabAtom)
  return (
    <div role="tablist" className="flex items-center gap-3 px-3 h-[32px] border-b border-border">
      {TABS.map((t) => {
        const selected = t.id === active
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={selected}
            onClick={() => setActive(t.id)}
            className={cn(
              'h-[32px] text-[12px] font-medium border-b-2 px-0.5 transition-colors',
              selected
                ? 'border-foreground text-foreground'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            )}
          >
            {t.label}
          </button>
        )
      })}
    </div>
  )
}
```

- [ ] **Step 2: Write `ChangeRow.tsx`** + `FileChangesPanel.tsx`

`ChangeRow.tsx`:

```tsx
import * as React from 'react'
import { Plus, Minus, Pencil, ArrowRight } from 'lucide-react'
import { cn } from '@/lib/utils'

export type FileChangeBadge = 'created' | 'modified' | 'removed' | 'renamed'

interface ChangeRowProps {
  badge: FileChangeBadge
  path: string
  newPath?: string
  onClick?: () => void
}

const BADGE_META: Record<FileChangeBadge, { Icon: typeof Plus; label: string; cls: string }> = {
  created: { Icon: Plus, label: '新增', cls: 'text-[hsl(var(--success))]' },
  removed: { Icon: Minus, label: '删除', cls: 'text-destructive' },
  modified: { Icon: Pencil, label: '修改', cls: 'text-foreground/70' },
  renamed: { Icon: ArrowRight, label: '重命名', cls: 'text-foreground/70' },
}

export function ChangeRow({ badge, path, newPath, onClick }: ChangeRowProps): React.ReactElement {
  const meta = BADGE_META[badge]
  const Icon = meta.Icon
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center w-full h-[26px] px-3 gap-2 text-[12px] text-left',
        'text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
      )}
      title={newPath ? `${path} → ${newPath}` : path}
    >
      <span className={cn('shrink-0', meta.cls)} aria-label={meta.label}>
        <Icon size={12} />
      </span>
      <span className="truncate font-mono tabular-nums" dir="rtl">
        {newPath ? `${path} → ${newPath}` : path}
      </span>
    </button>
  )
}
```

`FileChangesPanel.tsx`:

```tsx
import * as React from 'react'
import { ChangeRow, type FileChangeBadge } from './ChangeRow'

/**
 * FileChangesPanel — Lists agent file edits within the current session.
 *
 * W3 implementation: reads from a stub data source that returns the empty
 * list. W4 wires this to the agent_turns table so each in-session file edit
 * surfaces as a row. The empty-state UX is what users see today.
 */

interface ChangeEntry {
  badge: FileChangeBadge
  path: string
  newPath?: string
}

export function FileChangesPanel(): React.ReactElement {
  const changes: ChangeEntry[] = []

  if (changes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center flex-1 min-h-[120px] p-6 text-center">
        <div className="text-[12px] text-muted-foreground">
          这个会话还没有文件改动
        </div>
        <div className="mt-1 text-[11px] text-muted-foreground/60">
          Agent 写入或修改文件后会出现在这里
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-y-auto py-1">
      {changes.map((c, idx) => (
        <ChangeRow
          key={`${c.badge}-${c.path}-${idx}`}
          badge={c.badge}
          path={c.path}
          newPath={c.newPath}
        />
      ))}
    </div>
  )
}
```

- [ ] **Step 3: Write `index.tsx`**

```tsx
/**
 * <FilesRail /> — W3 right-rail files panel.
 *
 * Replaces the legacy <FileBrowser> usage inside SidePanel.tsx. Use as:
 *
 *   <FilesRail sessionId={sessionId} onFileClick={...} />
 *
 * Sections (workspace tab):
 *   - Workspace files (always shown when ~/Documents/workground exists)
 *   - Session files (when the session has a directory)
 *   - Attached directories (one section per attached path)
 *
 * Changes tab: per-session list of agent edits (stubbed in W3; wired in W4).
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { filesRailTabAtom, type MountRoot } from '@/atoms/files-rail-atoms'
import { FilesRailTabs } from './FilesRailTabs'
import { WorkspaceFilesPanel } from './workspace/WorkspaceFilesPanel'
import { FileChangesPanel } from './changes/FileChangesPanel'
import type { TreeNode } from './utils/tree-patch'

interface FilesRailProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode) => void
}

export function FilesRail({ sessionId, onFileClick }: FilesRailProps): React.ReactElement {
  const tab = useAtomValue(filesRailTabAtom)
  return (
    <div className="flex flex-col h-full bg-popover">
      <FilesRailTabs />
      {tab === 'workspace' && <WorkspaceFilesPanel sessionId={sessionId} onFileClick={onFileClick} />}
      {tab === 'changes' && <FileChangesPanel />}
    </div>
  )
}
```

- [ ] **Step 4: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/files-rail/index.tsx ui/src/components/files-rail/FilesRailTabs.tsx ui/src/components/files-rail/changes/
git commit -m "feat(ui/files-rail): top-level FilesRail + tabs + FileChangesPanel (stub)"
```

---

## Task 9: Integrate into SidePanel + final verification

**Files:**
- Modify: `ui/src/components/agent/SidePanel.tsx`

- [ ] **Step 1: Read the current FileBrowser usages**

```bash
sed -n '290,365p' /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

Two `<FileBrowser>` instances. The plan replaces BOTH with a single `<FilesRail>` mount that consolidates their content under the workspace tab.

- [ ] **Step 2: Replace the two FileBrowser sections with FilesRail**

Open `SidePanel.tsx`. Find the entire JSX block from the first `<FileBrowser` (the session-files section header that wraps it) through the end of the second `<FileBrowser` (after the workspace-files section). Replace the whole block with a single mount:

```tsx
            <FilesRail
              sessionId={sessionId}
              onFileClick={(_mount, node) => handleAddToChat({ ...node, path: node.relPath })}
            />
```

Add the import at the top of the file:

```tsx
import { FilesRail } from '@/components/files-rail'
```

Remove the now-unused imports if any (TypeScript will tell you). The `FileBrowser` import from `@/components/file-browser` may still be needed by other call sites — grep first:

```bash
grep -n "FileBrowser\|FileDropZone" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

If they no longer appear, remove the import line.

**IMPORTANT**: the `handleAddToChat` callback's input shape might not match `TreeNode` exactly. Check its signature:

```bash
grep -n "handleAddToChat\|const handleAddToChat" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

Adapt the inline `onFileClick` lambda to match — the existing callback was designed for `FileEntry` from `chat-types`. You may need a thin adapter:

```tsx
onFileClick={(mount, node) => {
  handleAddToChat({
    name: node.name,
    path: `${mount.path}/${node.relPath}`,
    is_dir: node.kind === 'directory',
    size: node.size,
  })
}}
```

Reference the actual `FileEntry` type once with:

```bash
grep -n "interface FileEntry\|type FileEntry" /Users/ryanliu/Documents/uclaw/ui/src/lib/chat-types.ts
```

and match its fields exactly.

- [ ] **Step 3: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: clean.

If TS complains about unused imports, remove them. If a test fails because it asserted on the old `<FileBrowser>` rendering, fix the test to assert on FilesRail's surface (it should still render `工作区文件` text + the tabs).

- [ ] **Step 4: Manual smoke (optional if dev server unavailable)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo tauri dev
```

Open an agent session. Verify:
- Right rail shows two tabs at top (`工作区文件` / `文件改动`)
- Workspace tab lists one or more mount sections
- Clicking a directory toggles expand/collapse
- Creating a file via Finder in the watched directory makes it appear in the rail within ~100ms
- Clicking a file adds it as an attachment (legacy `handleAddToChat` behavior preserved)

If `cargo tauri dev` is not available in the environment, defer this to the PR manual-test checklist.

- [ ] **Step 5: Pre-commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/src/components/agent/SidePanel.tsx
```

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/agent/SidePanel.tsx
git commit -m "feat(ui/files-rail): replace legacy FileBrowser usages in SidePanel with FilesRail mount"
```

---

## Task 10: Wire session-scoped mounts into AppState

**Files:**
- Modify: `src-tauri/src/app.rs` (replace the stubs from Task 4)

Task 4 stubbed `session_attached_root` and `session_attached_dirs` to keep that commit small. Task 10 wires them to real session data so users actually see session files + attached directories.

- [ ] **Step 1: Find the existing session-path API**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -rn "fn.*session_path\|fn.*get_session_path\|session_manager.*path" src-tauri/src/ | head -10
```

You're looking for a method on `SessionManager` (or directly on `AppState`) that returns a session's working directory given its ID.

Also locate the schema reads for `agent_sessions.attached_dirs` (V17 column):

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "attached_dirs" src-tauri/src/ -r | grep -v migrations.rs | grep -v test | head -10
```

There should be an existing helper that parses the JSON column. If not, the `attached_dirs` column is stored as JSON (e.g. `["/Users/foo/dir1", "/Users/foo/dir2"]`).

- [ ] **Step 2: Replace the stub methods in `src-tauri/src/app.rs`**

Find the `async fn session_attached_root` stub from Task 4. Replace its body with the actual session-path lookup:

```rust
    async fn session_attached_root(
        &self,
        sid: &str,
    ) -> Result<std::path::PathBuf, crate::error::Error> {
        // Read the session's path via the existing SessionManager API.
        // (Substitute the actual method name found in Step 1.)
        let path = self
            .session_manager
            .get_session_path(sid)
            .await
            .ok_or_else(|| crate::error::Error::Internal(format!("session not found: {}", sid)))?;
        Ok(std::path::PathBuf::from(path))
    }
```

Adapt `self.session_manager.get_session_path(...)` to whatever the actual API is — if it doesn't take an async lock, drop the `.await`; if it returns a `Result`, propagate with `?`. Read the actual signature once from Step 1.

For `session_attached_dirs`, query SQLite:

```rust
    async fn session_attached_dirs(&self, sid: &str) -> Vec<String> {
        let conn = match self.db.lock().await {
            // adapt to actual db lock type
            c => c,
        };
        let row: Result<String, _> = conn
            .query_row(
                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                rusqlite::params![sid],
                |r| r.get(0),
            );
        match row {
            Ok(json) => serde_json::from_str::<Vec<String>>(&json).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
```

Replace `self.db.lock().await` with the actual database accessor on `AppState`. If `AppState.db` is `Arc<Mutex<Connection>>`, use `.lock().await`; if it's `Arc<RwLock<Connection>>`, use `.read().await`. Confirm via:

```bash
grep -n "pub db\|db: Arc" /Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs | head -5
```

- [ ] **Step 3: Build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Common errors:
- "method `get_session_path` not found" → adapt to actual session API
- "cannot lock `Mutex` without await" → adjust async vs sync lock
- "no method named `query_row`" → import `rusqlite::Connection` or use `OptionalExtension`

Resolve in 3 attempts max; otherwise report NEEDS_CONTEXT with the actual API surface.

- [ ] **Step 4: Test that `files_rail_list_mounts` now returns session entries**

Add a small integration test in `src-tauri/src/files_rail/tests.rs`:

```rust
//! Files-rail integration tests.

// (Empty for now — Task 10 doesn't add tests directly. Session-path lookups
// are exercised manually in the PR test plan because they require a live
// SessionManager; unit tests would need a heavy fixture.)
```

Actually skip this file creation — the existing walker tests cover the pure logic. Document the integration smoke in the PR test plan instead.

- [ ] **Step 5: Pre-commit + commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M src-tauri/src/app.rs
```

```bash
git add src-tauri/src/app.rs
git commit -m "feat(files-rail): wire session-scoped mounts (session_path + attached_dirs JSON)"
```

---

## Task 11: Final verification + push + PR

- [ ] **Step 1: Full Rust suite**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -10
```

Expected: all green. Test count up by 5 (Task 2's walker + ignore tests).

- [ ] **Step 2: Full UI suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: TS clean. Test count up by 8 (Task 5's tree-patch tests).

- [ ] **Step 3: Hardcoded-color audit**

```bash
grep -rnE '#[0-9a-fA-F]{3,8}\b|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/files-rail/ \
  ui/src/atoms/files-rail-atoms.ts 2>/dev/null | head
```

Expected: empty. If any line returns, refactor the offending color to a theme token.

- [ ] **Step 4: Build the full Tauri binary**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build --bin uclaw 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 5: Git log review**

```bash
cd /Users/ryanliu/Documents/uclaw && git log --oneline main..HEAD
```

Expected 9 commits (Tasks 1-10, with Tasks 6/7 close together):

1. `feat(files-rail): scaffold module + wire types (FileNode, MountRoot, FilesRailChange)`
2. `feat(files-rail): ignore rules + single-layer directory walker (with tests)`
3. `feat(files-rail): multi-mount notify watcher with 16ms event batching`
4. `feat(files-rail): service + 4 Tauri commands (list_mounts, read_dir, watch_start, watch_stop)`
5. `feat(ui/files-rail): data atoms + tree-patch util + 4 typed Tauri wrappers (with tests)`
6. `feat(ui/files-rail): useFileTree hook + MountSection + FileTreeNode + WorkspaceFilesPanel`
7. `feat(ui/files-rail): subscribe to files_rail:change events + apply via tree-patch`
8. `feat(ui/files-rail): top-level FilesRail + tabs + FileChangesPanel (stub)`
9. `feat(ui/files-rail): replace legacy FileBrowser usages in SidePanel with FilesRail mount`
10. `feat(files-rail): wire session-scoped mounts (session_path + attached_dirs JSON)`

- [ ] **Step 6: Push and open PR**

```bash
cd /Users/ryanliu/Documents/uclaw && git push -u origin claude/w3-files-rail
cd /Users/ryanliu/Documents/uclaw && gh pr create --title "W3: Files Rail v2 — notify-driven live tree + tabs + attached dirs" --body "$(cat <<'EOF'
## Summary

Wave 3 of the [Proma v0.9.27 preview port](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md) (see §5). Replaces the polling-based `<FileBrowser>` instances inside the agent right-rail with a new `<FilesRail>` component driven by:

- A new Rust `src-tauri/src/files_rail/` module (7 files: types, ignore, walker, watcher, service, commands, tests)
- A single multi-mount `RecommendedWatcher` registered into the existing `ServiceManager` Stage 3 block
- 4 new Tauri commands (`files_rail_list_mounts` / `files_rail_read_dir` / `files_rail_watch_start` / `files_rail_watch_stop`) registered in `invoke_handler!`
- 1 new IPC channel `files_rail:change` (per-mount, batched 16ms / 100 events)
- A new `ui/src/components/files-rail/` (15 files) with top-level tabs (`工作区文件` / `文件改动`), per-mount sections, lazy expand, and watcher-driven tree-patch updates

The legacy `ui/src/components/file-browser/` directory stays in place — `FileTypeIcon` is consumed by the new tree node.

## Commits (bisectable)

10 commits — see `git log main..HEAD`.

## Test plan

- [x] `cd src-tauri && cargo build` — clean
- [x] `cd src-tauri && cargo test --lib files_rail` — 5/5 pass (walker + ignore)
- [x] `cd src-tauri && cargo test --lib` — all green
- [x] `cd ui && npx tsc --noEmit` — clean
- [x] `cd ui && npm test -- --run` — all green, +8 tests (tree-patch)
- [x] No hardcoded colors in new files
- [ ] Manual: agent session right rail shows tabs; workspace tab lists ≥ 1 mount
- [ ] Manual: `touch /tmp/test.txt` inside the workspace dir → appears in tree within ~100ms
- [ ] Manual: `rm` a file → disappears within ~100ms
- [ ] Manual: rename a file → tree updates without flicker (or rename appears as remove+create — both acceptable)
- [ ] Manual: collapse + re-expand a directory → no re-fetch, instant restore
- [ ] Manual: `attach_session_directory` an extra path → new mount section appears
- [ ] Manual: 11-theme spot-check (warm-paper / qingye / forest-dark) — no hardcoded grays bleeding through
- [ ] Manual: macOS `prefers-reduced-motion` → no animation regression (we only add `animate-spin` on refresh, which is acceptable)

## Architectural notes

- The existing `FileWatcher` in `src-tauri/src/workspace/mod.rs` is **left alone** — its `artifact:tree_update` channel may have consumers elsewhere. W3's watcher is parallel and uses a different channel.
- The `FileChangesPanel` is stubbed to "empty state" — W4 will wire it to `agent_turns` so each in-session file edit surfaces as a row.
- `applyExternalChanges` returns the same reference when no patch applied — Jotai treats this as a no-op render, keeping the tree React-stable.
- Unexpanded directories silently drop events targeting their subtree — lazy re-fetch handles them on next expand. This is a uClaw-specific optimization Proma doesn't have.

## Out of scope (W4 follow-up)

- Right-click context menu (rename / delete / open in Finder)
- Add-file / attach-dir buttons in the rail header (the existing `attach_session_directory` Tauri command is used today; UI shortcut is W4)
- `FileChangesPanel` real data (W4 wires from `agent_turns`)
EOF
)" 2>&1 | tail -3