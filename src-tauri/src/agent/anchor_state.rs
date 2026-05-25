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

/// The Unicode section sign (U+00A7) — separates anchor token from
/// literal line content in rendered output. Matches Dirac's
/// ANCHOR_DELIMITER (src/shared/utils/line-hashing.ts).
pub const ANCHOR_DELIMITER: char = '§';

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

/// Compose the LLM-visible anchored line: `<token>§<literal content>`.
///
/// The token portion comes from `AnchorStateManager` (stable across reads
/// via Myers diff carry-forward); the literal content is the current line.
/// EditTool's 4-step validator splits on [`ANCHOR_DELIMITER`] and
/// byte-compares the literal portion. (spec §3.2 / §8.2)
pub fn render_anchor_line(token: &str, content: &str) -> String {
    format!("{}{}{}", token, ANCHOR_DELIMITER, content)
}

/// Generate the anchor TOKEN portion for a line — a stable, human-readable
/// identifier with no embedded content hash.
///
/// - `salt == 0` → single curated word (`"Apple"`) — 60 distinct values.
/// - `salt > 0`  → 2-word combo (`"AppleCedar"`) — 60×60 = 3,600 distinct
///   values, used to escalate past single-word collisions.
///
/// The caller composes the full anchor via [`render_anchor_line`]. The salt
/// parameter lets [`initialize_anchors`] escalate from 1-word → 2-word combos
/// deterministically on collision. (spec §3.3 / §8.3)
pub fn generate_anchor_token(line: &str, salt: u64) -> String {
    let trimmed = line.trim();
    let hash = u64::from(fnv1a_32(trimmed.as_bytes())) ^ salt;
    let n = CURATED_WORDS.len() as u64;
    let first = CURATED_WORDS[(hash % n) as usize];
    if salt == 0 {
        first.to_string()
    } else {
        let second = CURATED_WORDS[((hash / n) % n) as usize];
        format!("{first}{second}")
    }
}

/// Legacy anchor generator returning the opaque `Apple§a1f89c` format
/// (word + `§` + 6-hex content hash).
///
/// Kept as a backward-compat shim for existing callers (e.g.
/// `skeleton::generate_skeleton`). New code should use
/// [`generate_anchor_token`] + [`render_anchor_line`], which separate the
/// stable token identity from the literal content used for byte-equal
/// validation. (spec §8.6)
pub fn generate_anchor(line: &str) -> String {
    let token = generate_anchor_token(line, 0);
    let hash = fnv1a_32(line.trim().as_bytes());
    format!("{}{}{:06x}", token, ANCHOR_DELIMITER, hash & 0xFFFFFF)
}

/// Initializes a deterministic, unique set of anchor TOKENS for a list of
/// file lines.
///
/// Uniqueness within a file is guaranteed by escalating the salt: identical
/// or colliding tokens trigger a retry with `salt += 1`, which switches from
/// a single word to a 2-word combo. A pathological collision storm
/// (>10,000 retries on one line) falls back to a numbered `AnchorN` token.
/// (spec §3.3)
pub fn initialize_anchors(lines: &[String]) -> Vec<String> {
    let mut anchors = Vec::with_capacity(lines.len());
    let mut seen: HashSet<String> = HashSet::new();
    for line in lines {
        let mut salt = 0u64;
        let token = loop {
            let candidate = generate_anchor_token(line, salt);
            if seen.insert(candidate.clone()) {
                break candidate;
            }
            salt += 1;
            if salt > 10_000 {
                // Pathological collision storm — emit a numbered fallback.
                let fb = format!("Anchor{}", anchors.len());
                seen.insert(fb.clone());
                break fb;
            }
        };
        anchors.push(token);
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
    let mut seen_in_new: HashSet<String> = HashSet::new();

    // Carry forward TOKENS for lines the Myers diff considers Equal — this is
    // the stability property: an unchanged line keeps its anchor token even
    // when surrounding lines are inserted/deleted/edited (spec §8.2 / §12).
    for op in ops {
        if let DiffOp::Equal { old_index, new_index, len } = op {
            for i in 0..len {
                let old_idx = old_index + i;
                let new_idx = new_index + i;
                if old_idx < old_anchors.len() {
                    let anchor = old_anchors[old_idx].clone();
                    seen_in_new.insert(anchor.clone());
                    new_anchors[new_idx] = anchor;
                }
            }
        }
    }

    // Generate fresh unique TOKENS for inserted or edited lines, escalating
    // the salt (1-word → 2-word combo) until the token is unique within the
    // file — never colliding with a carried-forward token.
    for (idx, line) in new_lines.iter().enumerate() {
        if new_anchors[idx].is_empty() {
            let mut salt = 0u64;
            let token = loop {
                let candidate = generate_anchor_token(line, salt);
                if seen_in_new.insert(candidate.clone()) {
                    break candidate;
                }
                salt += 1;
                if salt > 10_000 {
                    let fb = format!("Anchor{}", idx);
                    seen_in_new.insert(fb.clone());
                    break fb;
                }
            };
            new_anchors[idx] = token;
        }
    }

    new_anchors
}

/// Per-file anchor state — last-seen line content + 1:1 anchor TOKENS.
///
/// Stored together so Myers-diff alignment can run cross-read without the
/// caller needing to track old content (fixes the old `register_file_lines`
/// "we don't have the old_lines directly" fall-through). Anchors here are
/// TOKENS only (`Apple`, `AppleCedar`, `Apple-1`) — the `§<literal>` portion
/// is composed at render time. (spec §3.1 / §8.2)
#[derive(Debug, Clone)]
struct FileAnchorState {
    lines: Vec<String>,
    anchors: Vec<String>,
}

/// Stateful manager for session-level line anchor resolution
#[derive(Default)]
pub struct AnchorStateManager {
    files: Mutex<HashMap<PathBuf, FileAnchorState>>,
}

impl AnchorStateManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Idempotent entry point: record the current line content of a file and
    /// return its anchor TOKENS.
    ///
    /// - First call → initializes anchors from scratch.
    /// - Subsequent calls → aligns via Myers diff over (last-seen lines, new
    ///   lines) so unchanged lines keep their tokens and only changed/inserted
    ///   lines get fresh ones.
    ///
    /// This replaces the previous `register_file_lines` fall-through that
    /// re-initialized on every read and lost cross-read stability. (spec §3.1)
    pub fn record_read(&self, path: &Path, lines: &[String]) -> Vec<String> {
        let mut files = self.files.lock().unwrap();
        let key = path.to_path_buf();
        let new_anchors = match files.get(&key) {
            None => initialize_anchors(lines),
            Some(prev) => align_anchors(&prev.lines, lines, &prev.anchors),
        };
        files.insert(
            key,
            FileAnchorState { lines: lines.to_vec(), anchors: new_anchors.clone() },
        );
        new_anchors
    }

    /// Anchor token → line index in the file's CURRENT state. Returns `None`
    /// if the path isn't tracked or the token isn't present. Used by
    /// EditTool's 4-step validator. (spec §3.5 step 2)
    pub fn resolve_anchor_index(&self, path: &Path, token: &str) -> Option<usize> {
        let files = self.files.lock().unwrap();
        files.get(path)?.anchors.iter().position(|a| a == token)
    }

    /// `(line_content, anchor_token)` snapshot at `idx`. Used to build
    /// "Expected: ..., Provided: ..." mismatch messages. (spec §3.1)
    pub fn snapshot_line(&self, path: &Path, idx: usize) -> Option<(String, String)> {
        let files = self.files.lock().unwrap();
        let state = files.get(path)?;
        Some((state.lines.get(idx)?.clone(), state.anchors.get(idx)?.clone()))
    }

    /// Sets or initializes the anchors for a given file.
    ///
    /// Backward-compat shim forwarding to [`record_read`]. New code should
    /// call `record_read` directly (it returns the anchors). Kept un-deprecated
    /// because out-of-scope callers (`get_file_skeleton`) still use it.
    /// (spec §4.2 / §8.6)
    pub fn register_file_lines(&self, path: &Path, lines: &[String]) {
        let _ = self.record_read(path, lines);
    }

    /// Aligns existing anchors for a file with the new lines content.
    ///
    /// Deprecated: `record_read` tracks old line content internally and does
    /// the align in one call. This shim ignores `old_lines` (the manager's
    /// last-seen content is authoritative) and forwards `new_lines`. (spec §4.2)
    #[deprecated(note = "use record_read; old_lines is now tracked internally")]
    pub fn align_file_anchors(&self, path: &Path, _old_lines: &[String], new_lines: &[String]) {
        let _ = self.record_read(path, new_lines);
    }

    /// Returns the anchor TOKENS registered for a given file.
    ///
    /// NOTE (B1 downstream consequence — audit per spec §4.3): post-B1 this
    /// returns bare tokens (`Apple`, `AppleCedar`) rather than the pre-B1
    /// `Apple§<hash6hex>` strings. The out-of-scope `get_file_skeleton` tool
    /// consumes this, so its skeleton display changed from `# ... §Apple§a1f89c ...`
    /// to `# ... §Apple ...` — benign and more consistent (the token now
    /// round-trips to an anchored `edit`); no test regression. See
    /// escalation/C2-Dirac-B1-side-finding.md.
    pub fn get_anchors(&self, path: &Path) -> Option<Vec<String>> {
        let files = self.files.lock().unwrap();
        files.get(path).map(|s| s.anchors.clone())
    }

    /// Removes a tracked file's anchor state.
    pub fn unregister_file(&self, path: &Path) {
        let mut files = self.files.lock().unwrap();
        files.remove(path);
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

    /// Marks a file stale (modified externally). Normally the background
    /// watcher sets this; exposed publicly so callers that detect an
    /// out-of-band change (and deterministic tests) can flag a file without
    /// depending on filesystem-event timing.
    pub fn mark_stale(&self, path: &Path) {
        let path_abs = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };
        let mut stale = self.stale_files.lock().unwrap();
        stale.insert(path_abs);
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
        // Two identical lines must still get distinct anchors. After the B1
        // pivot, collision escalation switches from a `-N` suffix to a 2-word
        // combo (salt escalation), so we assert uniqueness + combo shape
        // rather than the legacy `-1` suffix.
        let lines = vec![
            "pub fn foo() {}".to_string(),
            "pub fn foo() {}".to_string(),
        ];
        let anchors = initialize_anchors(&lines);
        assert_eq!(anchors.len(), 2);
        assert_ne!(anchors[0], anchors[1]);
        // First occurrence is a single word; the collision escalates to a
        // 2-word combo (two capitals).
        assert_eq!(
            anchors[0].chars().filter(|c| c.is_ascii_uppercase()).count(),
            1,
            "first occurrence is a single curated word: {:?}",
            anchors[0]
        );
        assert_eq!(
            anchors[1].chars().filter(|c| c.is_ascii_uppercase()).count(),
            2,
            "collision escalates to a 2-word combo: {:?}",
            anchors[1]
        );
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

    // ── Dirac-B1: record_read alignment + format-pivot tests (spec §5) ──

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Test #1 — first record_read initializes N unique anchors.
    #[test]
    fn record_read_initializes_first_call() {
        let mgr = AnchorStateManager::new();
        let lines = v(&["fn foo() {", "    bar();", "}"]);
        let anchors = mgr.record_read(Path::new("/tmp/b1_test_init.rs"), &lines);
        assert_eq!(anchors.len(), 3);
        let unique: HashSet<_> = anchors.iter().collect();
        assert_eq!(unique.len(), 3, "anchors must be unique: {:?}", anchors);
    }

    /// Test #2 — re-reading unchanged content preserves all anchors.
    #[test]
    fn record_read_preserves_anchors_on_unchanged_file() {
        let mgr = AnchorStateManager::new();
        let lines = v(&["a", "b", "c"]);
        let p = Path::new("/tmp/b1_test_unchanged.rs");
        let first = mgr.record_read(p, &lines);
        let second = mgr.record_read(p, &lines);
        assert_eq!(first, second);
    }

    /// Test #3 — inserted lines: unchanged surrounding lines keep their tokens
    /// (proves Myers carry-forward survived the refactor).
    #[test]
    fn record_read_carries_anchors_across_inserted_lines() {
        let mgr = AnchorStateManager::new();
        let p = Path::new("/tmp/b1_test_insert.rs");
        let v1 = v(&["a", "b", "c"]);
        let a1 = mgr.record_read(p, &v1);

        let v2 = v(&["a", "NEW1", "NEW2", "b", "c"]);
        let a2 = mgr.record_read(p, &v2);

        assert_eq!(a2[0], a1[0], "line 'a' keeps token");
        assert_eq!(a2[3], a1[1], "line 'b' keeps token");
        assert_eq!(a2[4], a1[2], "line 'c' keeps token");
        // Inserted lines get fresh tokens, distinct from carried ones.
        assert_ne!(a2[1], a1[0]);
        assert_ne!(a2[1], a1[1]);
        assert_ne!(a2[1], a1[2]);
        assert_ne!(a2[1], a2[2]);
    }

    /// Test #4 — a changed line gets a fresh token; neighbors keep theirs
    /// (proves freshen-on-change survived the refactor).
    #[test]
    fn record_read_freshens_anchors_for_changed_lines() {
        let mgr = AnchorStateManager::new();
        let p = Path::new("/tmp/b1_test_change.rs");
        let v1 = v(&["a", "b", "c"]);
        let a1 = mgr.record_read(p, &v1);

        let v2 = v(&["a", "MODIFIED", "c"]);
        let a2 = mgr.record_read(p, &v2);

        assert_eq!(a2[0], a1[0]);
        assert_eq!(a2[2], a1[2]);
        assert_ne!(a2[1], a1[1], "modified line should get a fresh token");
    }

    /// Test #5 — dictionary capacity: 3,000 distinct lines → 3,000 distinct
    /// tokens (2-word combo escalation, no exhaustion).
    #[test]
    fn dictionary_capacity_handles_3000_lines() {
        let lines: Vec<String> = (0..3000).map(|i| format!("line_{i}")).collect();
        let anchors = initialize_anchors(&lines);
        let unique: HashSet<_> = anchors.iter().collect();
        assert_eq!(unique.len(), 3000, "must produce 3000 distinct tokens");
    }

    /// Test #6 — generate_anchor_token: salt 0 → single word, salt 1 → 2-word combo.
    #[test]
    fn generate_anchor_token_pivot() {
        let t0 = generate_anchor_token("    def foo():", 0);
        assert!(t0.chars().all(|c| c.is_ascii_alphabetic()), "single word: {:?}", t0);
        assert!(t0.chars().next().unwrap().is_ascii_uppercase());
        assert_eq!(
            t0.chars().filter(|c| c.is_ascii_uppercase()).count(),
            1,
            "salt 0 yields a single curated word: {:?}",
            t0
        );

        let t1 = generate_anchor_token("    def foo():", 1);
        assert_eq!(
            t1.chars().filter(|c| c.is_ascii_uppercase()).count(),
            2,
            "salt 1 yields a 2-word combo: {:?}",
            t1
        );
    }

    /// Test #7 — render_anchor_line composition + round-trip split.
    #[test]
    fn render_anchor_line_format() {
        let out = render_anchor_line("Apple", "    def foo():");
        assert_eq!(out, "Apple§    def foo():");
        let (token, content) = out.split_once(ANCHOR_DELIMITER).unwrap();
        assert_eq!(token, "Apple");
        assert_eq!(content, "    def foo():");
    }

    /// Format pivot guard — generate_anchor_token returns a TOKEN only (no '§',
    /// no hex hash). The legacy 'Apple§<hash>' format lives ONLY in the
    /// generate_anchor shim.
    #[test]
    fn token_has_no_embedded_hash() {
        let token = generate_anchor_token("pub fn foo() {}", 0);
        assert!(!token.contains('§'), "token must not contain the delimiter: {:?}", token);
        assert!(
            token.chars().all(|c| c.is_ascii_alphabetic()),
            "token must be letters only (no hex hash): {:?}",
            token
        );
        // The legacy shim still emits the old opaque format.
        let legacy = generate_anchor("pub fn foo() {}");
        assert!(legacy.contains('§') && legacy.len() > token.len());
    }
}
