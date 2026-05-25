//! Real-time folder watching daemon for `~/.uclaw/inbox/gbrain_drafts/` — Scheme A.
//!
//! Starts a `notify`-backed watcher that monitors the drafts folder for
//! `.md` file changes. Events are debounced (default 500ms) and unique
//! paths of updated draft files are sent to an async channel for processing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Watcher lifetime handle. Drop to stop watching cleanly.
pub struct DraftsWatcherHandle {
    _watcher: RecommendedWatcher,
    _worker: std::thread::JoinHandle<()>,
    shutdown: Arc<Mutex<bool>>,
}

impl DraftsWatcherHandle {
    /// Stop watching cleanly.
    pub fn stop(self) {
        if let Ok(mut b) = self.shutdown.lock() {
            *b = true;
        }
        let _ = self._worker.join();
    }
}

/// Filter events to only care about Create or Modify events for `.md` files
/// located inside the drafts directory.
pub fn event_is_relevant(event: &notify::Event, drafts_dir: &Path) -> bool {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_)
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        p.extension().and_then(|e| e.to_str()) == Some("md")
            && p.starts_with(drafts_dir)
    })
}

/// Start watching `drafts_dir` for changes. Debounces events and forwards unique
/// file paths to the provided tokio channel sender.
pub fn start_drafts_watcher(
    drafts_dir: PathBuf,
    debounce_ms: u64,
    tx_drafts: tokio::sync::mpsc::UnboundedSender<PathBuf>,
) -> Result<DraftsWatcherHandle, crate::error::Error> {
    if !drafts_dir.exists() {
        std::fs::create_dir_all(&drafts_dir).map_err(|e| {
            crate::error::Error::Internal(format!(
                "drafts_watcher: cannot create {}: {}",
                drafts_dir.display(),
                e
            ))
        })?;
    }

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)
        .map_err(|e| crate::error::Error::Internal(format!("notify::watcher: {}", e)))?;
    
    watcher
        .watch(&drafts_dir, RecursiveMode::NonRecursive)
        .map_err(|e| crate::error::Error::Internal(format!("watcher.watch: {}", e)))?;

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_clone = shutdown.clone();
    let drafts_dir_clone = drafts_dir.clone();

    let worker = std::thread::Builder::new()
        .name("drafts-watcher-debouncer".into())
        .spawn(move || {
            let mut pending_since: Option<Instant> = None;
            let mut pending_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
            let debounce = Duration::from_millis(debounce_ms);

            loop {
                if let Ok(b) = shutdown_clone.lock() {
                    if *b {
                        return;
                    }
                }

                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(event)) => {
                        if event_is_relevant(&event, &drafts_dir_clone) {
                            pending_since = Some(Instant::now());
                            for p in event.paths {
                                if p.extension().and_then(|e| e.to_str()) == Some("md")
                                    && p.starts_with(&drafts_dir_clone)
                                {
                                    pending_paths.insert(p);
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("drafts_watcher event error: {}", e);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Poll timeout, continue
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::info!("drafts_watcher channel closed, exiting worker");
                        return;
                    }
                }

                // If debounce quiet period is reached, dispatch the gathered paths.
                if let Some(t) = pending_since {
                    if t.elapsed() >= debounce {
                        pending_since = None;
                        for path in pending_paths.drain() {
                            if path.exists() {
                                tracing::info!("[DraftsWatcher] Dispatching draft for ingestion: {}", path.display());
                                if let Err(e) = tx_drafts.send(path) {
                                    tracing::warn!("drafts_watcher failed to send path to channel: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        })
        .map_err(|e| crate::error::Error::Internal(format!("spawn drafts watcher: {}", e)))?;

    Ok(DraftsWatcherHandle {
        _watcher: watcher,
        _worker: worker,
        shutdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{Event, EventKind, CreateKind, ModifyKind};

    #[test]
    fn test_event_is_relevant() {
        let drafts_dir = Path::new("/tmp/gbrain_drafts");

        // 1. Relevant create event for markdown file
        let mut ev1 = Event::new(EventKind::Create(CreateKind::File));
        ev1.paths.push(drafts_dir.join("test-draft.md"));
        assert!(event_is_relevant(&ev1, drafts_dir));

        // 2. Non-relevant extension (e.g., txt)
        let mut ev2 = Event::new(EventKind::Create(CreateKind::File));
        ev2.paths.push(drafts_dir.join("test-draft.txt"));
        assert!(!event_is_relevant(&ev2, drafts_dir));

        // 3. Relevant modify event for markdown file
        let mut ev3 = Event::new(EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)));
        ev3.paths.push(drafts_dir.join("subfolder/test-draft.md")); // Nested path or similar
        assert!(event_is_relevant(&ev3, drafts_dir));

        // 4. Non-relevant EventKind (e.g., Access)
        let mut ev4 = Event::new(EventKind::Access(notify::event::AccessKind::Read));
        ev4.paths.push(drafts_dir.join("test-draft.md"));
        assert!(!event_is_relevant(&ev4, drafts_dir));

        // 5. File outside of drafts_dir
        let mut ev5 = Event::new(EventKind::Create(CreateKind::File));
        ev5.paths.push(PathBuf::from("/other/path/test-draft.md"));
        assert!(!event_is_relevant(&ev5, drafts_dir));
    }
}

