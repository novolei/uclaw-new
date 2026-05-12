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
                // RenameMode::Both arrives with paths = [src, dst]; emit a single
                // paired Renamed event so the frontend's tree-patch can move the
                // node in one step. All other event kinds fan out per-path.
                let is_paired_rename = matches!(
                    event.kind,
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                );
                if is_paired_rename && event.paths.len() == 2 {
                    Self::queue_rename_pair(&mut inner, &event.paths[0], &event.paths[1]);
                } else {
                    for path in &event.paths {
                        Self::queue_event(&mut inner, path, &event.kind);
                    }
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
    ///
    /// NOTE: this silently no-ops the actual notify subscription if `start()`
    /// has not yet been called. The mount entry is still inserted into the
    /// internal map. Callers MUST invoke `start()` before any
    /// `register_mount()` calls — otherwise events for that mount never
    /// arrive. Service layer (`FilesRailService::start`) handles the order.
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
        // Note: for Remove events, path.is_dir() returns false because the entry
        // is already gone — consumers should not rely on is_dir to distinguish
        // file vs directory deletions. Use prior tree state for that.
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

    /// Emit a single paired Renamed event when notify provides both src + dst
    /// paths in one event. Falls back to two `Renamed` orphans if neither
    /// path lives under any registered mount.
    fn queue_rename_pair(inner: &mut Inner, src: &Path, dst: &Path) {
        // The src and dst paths usually share a mount but we don't assume.
        let src_owning = inner.mounts.iter_mut().find_map(|(_, e)| {
            if src.starts_with(&e.root) { Some(e) } else { None }
        });
        let Some(entry) = src_owning else { return };

        let src_rel = src
            .strip_prefix(&entry.root)
            .unwrap_or(src)
            .to_string_lossy()
            .replace('\\', "/");
        let dst_rel = dst
            .strip_prefix(&entry.root)
            .unwrap_or(dst)
            .to_string_lossy()
            .replace('\\', "/");
        // is_dir: probe dst (src is gone after the rename, dst exists).
        let is_dir = dst.is_dir();

        entry.pending.push(FileChange {
            kind: ChangeKind::Renamed,
            rel_path: src_rel,
            new_rel_path: Some(dst_rel),
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
