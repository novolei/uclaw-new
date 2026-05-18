//! Opt-in filesystem watcher over `<brain_root>/` — Memory OS Phase 7.4.
//!
//! When the user enables `memory_os.brain_watcher_enabled`, this module
//! starts a `notify`-backed recursive watcher that fires
//! [`crate::memory_graph::brain_io::sync_from_disk`] each time a
//! `.md` file under the brain root changes. Events are debounced
//! (default 500ms quiet period) so an Obsidian save burst doesn't
//! produce one sync per character.
//!
//! ## Why this is opt-in
//!
//! - Filesystem events are notoriously noisy on macOS (Spotlight
//!   indexer triggers extra Modify events, .DS_Store churn, etc.) and
//!   on networked drives (rsync etc. can fire dozens of stat-only
//!   events). Forcing this on for all users would create surprise
//!   sync runs.
//! - Even with the SHA-256 short-circuit (Phase 7.2), sync_from_disk
//!   still locks the rusqlite mutex briefly per file scanned. On a
//!   busy editor pass that lock can contend with other Memory OS
//!   work.
//! - The manual Sync button (Phase 7.2 / 7.3) covers most users. The
//!   watcher is for the smaller cohort who really want live sync.
//!
//! ## Failure modes
//!
//! - Brain root doesn't exist → log warning, do not start watcher.
//! - notify::recommended_watcher fails → log error, return Err.
//! - Sync errors during a watcher-triggered run → log warning, watcher
//!   stays alive.
//!
//! The watcher handle returned by [`start_brain_watcher`] keeps the
//! watcher alive. Drop it to stop watching cleanly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::memory_graph::brain_io::{sync_from_disk, BrainExportConfig};
use crate::memory_graph::store::MemoryGraphStore;

/// Default debounce window. Tested empirically with VS Code + Obsidian:
/// 500ms catches editor save bursts without making the user wait for
/// edits to "land".
pub const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Watcher lifetime handle. Owns the `notify` watcher + the debounce
/// worker. Drop to stop watching (both background bits go away).
pub struct BrainWatcherHandle {
    _watcher: RecommendedWatcher,
    _worker: std::thread::JoinHandle<()>,
    shutdown: Arc<Mutex<bool>>,
}

impl BrainWatcherHandle {
    /// Signal the worker to exit and wait briefly for it. Mostly for
    /// tests; production drops the handle on app exit and that's fine.
    pub fn stop(self) {
        if let Ok(mut b) = self.shutdown.lock() {
            *b = true;
        }
        // The worker checks shutdown on each loop iteration.
        let _ = self._worker.join();
    }
}

/// Decide whether one `notify::Event` is worth waking the debouncer for.
/// We only care about content-affecting events on `.md` files under the
/// brain root. Filtering here keeps the worker thread idle when, e.g.,
/// Spotlight is reindexing the whole user home.
pub fn event_is_relevant(event: &notify::Event, brain_root: &Path) -> bool {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        // Only `.md` files inside brain_root (recursive).
        p.extension().and_then(|e| e.to_str()) == Some("md")
            && p.starts_with(brain_root)
    })
}

/// Start a watcher over `brain_root`. Returns a handle whose drop
/// stops both the watcher and the debounce worker. `space_id` is
/// used when invoking `sync_from_disk` on each batch.
///
/// `debounce_ms = 0` is treated as "fire on every event" (used by
/// tests). Production should pass `DEFAULT_DEBOUNCE_MS`.
pub fn start_brain_watcher(
    store: Arc<MemoryGraphStore>,
    brain_root: PathBuf,
    space_id: String,
    debounce_ms: u64,
) -> Result<BrainWatcherHandle, crate::error::Error> {
    if !brain_root.exists() {
        std::fs::create_dir_all(&brain_root).map_err(|e| {
            crate::error::Error::Internal(format!(
                "brain_watcher: cannot create {}: {}",
                brain_root.display(),
                e
            ))
        })?;
    }

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)
        .map_err(|e| crate::error::Error::Internal(format!("notify::watcher: {}", e)))?;
    watcher
        .watch(&brain_root, RecursiveMode::Recursive)
        .map_err(|e| crate::error::Error::Internal(format!("watcher.watch: {}", e)))?;

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_clone = shutdown.clone();
    let brain_root_clone = brain_root.clone();
    let worker = std::thread::Builder::new()
        .name("brain-watcher-debouncer".into())
        .spawn(move || {
            let mut pending_since: Option<Instant> = None;
            let debounce = Duration::from_millis(debounce_ms);
            // Drain the channel with a short poll so we can also check
            // shutdown periodically.
            loop {
                if let Ok(b) = shutdown_clone.lock() {
                    if *b {
                        return;
                    }
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(event)) => {
                        if event_is_relevant(&event, &brain_root_clone) {
                            pending_since = Some(Instant::now());
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("brain_watcher event error: {}", e);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // poll
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::info!("brain_watcher channel closed, exiting worker");
                        return;
                    }
                }
                // Quiet period reached → run the sync.
                if let Some(t) = pending_since {
                    if t.elapsed() >= debounce {
                        pending_since = None;
                        let cfg = BrainExportConfig {
                            brain_root: brain_root_clone.clone(),
                            space_id: space_id.clone(),
                        };
                        match sync_from_disk(&store, &cfg) {
                            Ok(o) => {
                                if o.pages_updated > 0
                                    || o.new_pages_created > 0
                                    || o.conflicts > 0
                                {
                                    tracing::info!(
                                        updated = o.pages_updated,
                                        created = o.new_pages_created,
                                        conflicts = o.conflicts,
                                        "brain_watcher: live sync produced changes"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!("brain_watcher sync failed: {}", e);
                            }
                        }
                    }
                }
            }
        })
        .map_err(|e| crate::error::Error::Internal(format!("spawn watcher: {}", e)))?;

    Ok(BrainWatcherHandle {
        _watcher: watcher,
        _worker: worker,
        shutdown,
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_event(kind: EventKind, paths: Vec<PathBuf>) -> notify::Event {
        notify::Event {
            kind,
            paths,
            attrs: Default::default(),
        }
    }

    #[test]
    fn event_is_relevant_accepts_md_inside_brain_root() {
        let root = PathBuf::from("/tmp/brain");
        let event = make_event(
            EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            vec![PathBuf::from("/tmp/brain/person/alice.md")],
        );
        assert!(event_is_relevant(&event, &root));
    }

    #[test]
    fn event_is_relevant_rejects_non_md_files() {
        let root = PathBuf::from("/tmp/brain");
        let event = make_event(
            EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            vec![PathBuf::from("/tmp/brain/person/.DS_Store")],
        );
        assert!(!event_is_relevant(&event, &root));
    }

    #[test]
    fn event_is_relevant_rejects_files_outside_brain_root() {
        let root = PathBuf::from("/tmp/brain");
        let event = make_event(
            EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            vec![PathBuf::from("/tmp/other/alice.md")],
        );
        assert!(!event_is_relevant(&event, &root));
    }

    #[test]
    fn event_is_relevant_rejects_irrelevant_event_kinds() {
        let root = PathBuf::from("/tmp/brain");
        // Access events are noise — shouldn't trigger sync.
        let event = make_event(
            EventKind::Access(notify::event::AccessKind::Read),
            vec![PathBuf::from("/tmp/brain/person/alice.md")],
        );
        assert!(!event_is_relevant(&event, &root));
    }

    #[test]
    fn event_is_relevant_accepts_create_remove_modify() {
        let root = PathBuf::from("/tmp/brain");
        let p = vec![PathBuf::from("/tmp/brain/person/alice.md")];
        for kind in [
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            EventKind::Remove(notify::event::RemoveKind::File),
        ] {
            assert!(event_is_relevant(&make_event(kind, p.clone()), &root));
        }
    }
}
