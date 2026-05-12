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
