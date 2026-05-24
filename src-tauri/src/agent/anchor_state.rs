// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use similar::{Algorithm, DiffOp};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, info, warn};
use once_cell::sync::Lazy;

pub static GLOBAL_ANCHOR_STATE_MANAGER: Lazy<Arc<AnchorStateManager>> = Lazy::new(|| {
    Arc::new(AnchorStateManager::new())
});

pub static GLOBAL_FILE_CONTEXT_TRACKER: Lazy<Arc<FileContextTracker>> = Lazy::new(|| {
    FileContextTracker::new()
});


/// Pure FNV-1a 32-bit hashing utility
pub fn fnv1a_32(data: &[u8]) -> u32 {
    let mut hash = 2166136261u32;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// A static list of curated, easily readable words to generate readable anchors
const CURATED_WORDS: &[&str] = &[
    "Apple", "Banana", "Cherry", "Date", "Elderberry", "Fig", "Grape", "Honeydew",
    "Kiwi", "Lemon", "Mango", "Nectarine", "Orange", "Papaya", "Quince", "Raspberry",
    "Strawberry", "Tangerine", "Ugli", "Vanilla", "Watermelon", "Acorn", "Bamboo",
    "Cedar", "Dahlia", "Elm", "Fern", "Ginkgo", "Hazel", "Ivy", "Juniper", "Kelp",
    "Larch", "Maple", "Nutmeg", "Oak", "Pine", "Rose", "Spruce", "Tulip", "Walnut",
    "Yew", "Zinnia", "Badger", "Beaver", "Camel", "Dolphin", "Eagle", "Falcon",
    "Gecko", "Heron", "Jaguar", "Koala", "Lemur", "Moose", "Newt", "Ocelot", "Panda",
    "Robin", "Sable", "Tiger", "Vixen", "Walrus", "Zebra"
];

/// Generates a stateful, highly readable anchor (e.g., Apple§a1f89c) based on line content
pub fn generate_anchor(line: &str) -> String {
    let trimmed = line.trim();
    let hash = fnv1a_32(trimmed.as_bytes());
    let word_idx = (hash % CURATED_WORDS.len() as u32) as usize;
    let word = CURATED_WORDS[word_idx];
    format!("{}§{:06x}", word, hash & 0xFFFFFF)
}

/// Initializes a deterministic, unique set of anchors for a list of file lines
pub fn initialize_anchors(lines: &[String]) -> Vec<String> {
    let mut anchors = Vec::with_capacity(lines.len());
    let mut seen = HashMap::new();
    for line in lines {
        let base = generate_anchor(line);
        let count = seen.entry(base.clone()).or_insert(0);
        let anchor = if *count > 0 {
            format!("{}-{}", base, *count)
        } else {
            base
        };
        *count += 1;
        anchors.push(anchor);
    }
    anchors
}

/// Myers Diff-based stateful anchor alignment algorithm
pub fn align_anchors(
    old_lines: &[String],
    new_lines: &[String],
    old_anchors: &[String],
) -> Vec<String> {
    let ops = similar::capture_diff_slices(Algorithm::Myers, old_lines, new_lines);
    let mut new_anchors = vec![String::new(); new_lines.len()];
    let mut seen_in_new = HashMap::new();

    // Map existing aligned lines first
    for op in ops {
        match op {
            DiffOp::Equal { old_index, new_index, len } => {
                for i in 0..len {
                    let old_idx = old_index + i;
                    let new_idx = new_index + i;
                    if old_idx < old_anchors.len() {
                        let anchor = old_anchors[old_idx].clone();
                        new_anchors[new_idx] = anchor.clone();
                        *seen_in_new.entry(anchor).or_insert(0) += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // Generate new unique anchors for inserted or edited lines
    for (idx, line) in new_lines.iter().enumerate() {
        if new_anchors[idx].is_empty() {
            let base = generate_anchor(line);
            let count = seen_in_new.entry(base.clone()).or_insert(0);
            let anchor = if *count > 0 {
                format!("{}-{}", base, *count)
            } else {
                base
            };
            *count += 1;
            new_anchors[idx] = anchor;
        }
    }

    new_anchors
}

/// Stateful manager for session-level line anchor resolution
#[derive(Default)]
pub struct AnchorStateManager {
    file_anchors: Mutex<HashMap<PathBuf, Vec<String>>>,
}

impl AnchorStateManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets or initializes the anchors for a given file
    pub fn register_file_lines(&self, path: &Path, lines: &[String]) {
        let path_buf = path.to_path_buf();
        let mut anchors = self.file_anchors.lock().unwrap();
        let aligned = if let Some(old_anchors) = anchors.get(&path_buf) {
            // If we already had anchors, we try to align them via Myers Diff
            // But if it was an empty file, we just initialize them
            if old_anchors.is_empty() {
                initialize_anchors(lines)
            } else {
                // Here we don't have the old_lines directly, so we either fall back to
                // initializing or we can just initialize if we can't find old content.
                // To support true cross-write alignment, we can maintain the last-seen line content too.
                initialize_anchors(lines)
            }
        } else {
            initialize_anchors(lines)
        };
        anchors.insert(path_buf, aligned);
    }

    /// Aligns existing anchors for a file with the new lines content
    pub fn align_file_anchors(&self, path: &Path, old_lines: &[String], new_lines: &[String]) {
        let path_buf = path.to_path_buf();
        let mut anchors = self.file_anchors.lock().unwrap();
        let old_anchors = anchors.entry(path_buf.clone()).or_insert_with(|| initialize_anchors(old_lines));
        let aligned = align_anchors(old_lines, new_lines, old_anchors);
        *old_anchors = aligned;
    }

    /// Returns the anchors registered for a given file
    pub fn get_anchors(&self, path: &Path) -> Option<Vec<String>> {
        let anchors = self.file_anchors.lock().unwrap();
        anchors.get(path).cloned()
    }

    /// Removes a tracked file's anchors
    pub fn unregister_file(&self, path: &Path) {
        let mut anchors = self.file_anchors.lock().unwrap();
        anchors.remove(path);
    }
}

/// Watcher and Tracker for active files to detect external modifications
pub struct FileContextTracker {
    active_files: Arc<Mutex<HashSet<PathBuf>>>,
    expected_writes: Arc<Mutex<HashSet<PathBuf>>>,
    stale_files: Arc<Mutex<HashSet<PathBuf>>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl FileContextTracker {
    pub fn new() -> Arc<Self> {
        let active_files = Arc::new(Mutex::new(HashSet::new()));
        let expected_writes = Arc::new(Mutex::new(HashSet::new()));
        let stale_files = Arc::new(Mutex::new(HashSet::new()));

        let active_files_clone = active_files.clone();
        let expected_writes_clone = expected_writes.clone();
        let stale_files_clone = stale_files.clone();

        // Create the background filesystem watcher callback
        let watcher_callback = move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if is_modify_event(&event) {
                    for path in event.paths {
                        let path_abs = match path.canonicalize() {
                            Ok(p) => p,
                            Err(_) => path.clone(),
                        };

                        // Check if this path is active
                        let is_active = {
                            let active = active_files_clone.lock().unwrap();
                            active.contains(&path_abs)
                        };

                        if is_active {
                            // Check if this modification is from the agent itself
                            let mut expected = expected_writes_clone.lock().unwrap();
                            if expected.remove(&path_abs) {
                                debug!("Ignoring expected write from agent for {:?}", path_abs);
                            } else {
                                info!("External change detected for active file {:?}", path_abs);
                                let mut stale = stale_files_clone.lock().unwrap();
                                stale.insert(path_abs);
                            }
                        }
                    }
                }
            }
        };

        let watcher = notify::recommended_watcher(watcher_callback).ok();

        Arc::new(Self {
            active_files,
            expected_writes,
            stale_files,
            watcher: Mutex::new(watcher),
        })
    }

    /// Returns a list of all active files being tracked
    pub fn get_active_files(&self) -> Vec<PathBuf> {
        let active = self.active_files.lock().unwrap();
        active.iter().cloned().collect()
    }

    /// Starts tracking a file and registers it in the file watcher
    pub fn track_file(&self, path: &Path) {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };

        {
            let mut active = self.active_files.lock().unwrap();
            active.insert(path_abs.clone());
        }

        // Add to watcher
        if let Some(ref mut w) = *self.watcher.lock().unwrap() {
            if let Err(e) = w.watch(&path_abs, RecursiveMode::NonRecursive) {
                warn!("Failed to watch file {:?}: {}", path_abs, e);
            } else {
                debug!("Now watching {:?}", path_abs);
            }
        }
    }

    /// Unregisters and stops tracking a file
    pub fn untrack_file(&self, path: &Path) {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };

        {
            let mut active = self.active_files.lock().unwrap();
            active.remove(&path_abs);
            let mut stale = self.stale_files.lock().unwrap();
            stale.remove(&path_abs);
        }

        if let Some(ref mut w) = *self.watcher.lock().unwrap() {
            let _ = w.unwatch(&path_abs);
        }
    }

    /// Marks a file write as expected (originating from the agent itself)
    pub fn register_expected_write(&self, path: &Path) {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };
        let mut expected = self.expected_writes.lock().unwrap();
        expected.insert(path_abs);
    }

    /// Checks if a file is stale (modified externally by user)
    pub fn is_stale(&self, path: &Path) -> bool {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };
        let stale = self.stale_files.lock().unwrap();
        stale.contains(&path_abs)
    }

    /// Clears the stale status of a file (typically after it is re-read)
    pub fn clear_stale(&self, path: &Path) {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };
        let mut stale = self.stale_files.lock().unwrap();
        stale.remove(&path_abs);
    }
}

/// Heuristic helper to check if a notify event is a file content write/modification
fn is_modify_event(event: &Event) -> bool {
    match event.kind {
        EventKind::Modify(ref mod_kind) => {
            match mod_kind {
                notify::event::ModifyKind::Data(_) | notify::event::ModifyKind::Any => true,
                _ => false,
            }
        }
        EventKind::Create(_) | EventKind::Remove(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_hashing() {
        let h1 = fnv1a_32(b"hello world");
        let h2 = fnv1a_32(b"hello world ");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_generate_anchor() {
        let a1 = generate_anchor("pub fn foo() {}");
        let a2 = generate_anchor("pub fn foo() {}");
        let a3 = generate_anchor("pub fn bar() {}");
        assert_eq!(a1, a2);
        assert_ne!(a1, a3);
        assert!(a1.contains("§"));
    }

    #[test]
    fn test_initialize_anchors_unique() {
        let lines = vec![
            "pub fn foo() {}".to_string(),
            "pub fn foo() {}".to_string(),
        ];
        let anchors = initialize_anchors(&lines);
        assert_eq!(anchors.len(), 2);
        assert_ne!(anchors[0], anchors[1]);
        assert!(anchors[1].ends_with("-1"));
    }

    #[test]
    fn test_align_anchors() {
        let old_lines = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
        ];
        let old_anchors = initialize_anchors(&old_lines);

        let new_lines = vec![
            "line 1".to_string(),
            "line 2 modified".to_string(),
            "line 3".to_string(),
        ];

        let aligned = align_anchors(&old_lines, &new_lines, &old_anchors);
        assert_eq!(aligned.len(), 3);
        assert_eq!(aligned[0], old_anchors[0]);
        assert_eq!(aligned[2], old_anchors[2]);
        // line 2 modified gets a new anchor because its content changed and Myers Diff didn't align it as Equal
        assert_ne!(aligned[1], old_anchors[1]);
    }
}
