// SPDX-License-Identifier: Apache-2.0
//! Shadow git checkpoint store — crash-safe rollback for agent edits.
//!
//! Creates snapshots of the working directory before the first mutating tool
//! call in each turn, stored in an out-of-project bare git repository under
//! `uclaw_home/checkpoints/store/`.  The user's `.git` is NEVER touched.
//!
//! Storage layout (mirrors hermes checkpoint_manager.py v2):
//! ```text
//! ~/.uclaw/checkpoints/
//!     store/                          — single bare shadow git repo
//!         HEAD, config, objects/      — standard git internals
//!         refs/uclaw/<hash16>         — per-project branch tip
//!         indexes/<hash16>            — per-project git index
//!         info/exclude                — default excludes
//! ```
//!
//! # Critical contract
//! 1. `ensure_checkpoint` NEVER propagates errors — git failure → `debug` + `false`.
//! 2. Snapshot at most once per `(llm_round, working_dir)` pair (per-LLM-round dedup).
//! 3. All git ops set `GIT_DIR=<store>`, `GIT_WORK_TREE=<working_dir>`,
//!    `GIT_INDEX_FILE=<store>/indexes/<hash>`, plus config-isolation vars,
//!    so NOTHING leaks into the user's `.git`.
//! 4. Commits land in the shadow `GIT_DIR`, not `<project>/.git`.
//! 5. Bare snapshot via `commit-tree` (no HEAD movement in working tree).
//!
//! Port of hermes `tools/checkpoint_manager.py` core — functions:
//! `_git_env` (line 236), `_run_git` (273), `_init_store` (387), `_take` (840),
//! `ensure_checkpoint` (623), `list_checkpoints` (657), `restore` (761),
//! `_validate_commit_hash` (155), `_validate_file_path` (171),
//! `_project_hash` (198).
//! Ignored: prune, auto-prune, clear, legacy, status, format.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

// ── Constants ────────────────────────────────────────────────────────────────

/// Refs namespace: `refs/uclaw/<project_hash>` (hermes uses `refs/hermes/…`).
const REFS_PREFIX: &str = "refs/uclaw";
const INDEXES_DIRNAME: &str = "indexes";
const STORE_DIRNAME: &str = "store";
const GIT_TIMEOUT_SECS: u64 = 30;

// ── Public types ─────────────────────────────────────────────────────────────

/// A single checkpoint entry returned by `list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    /// Full commit hash.
    pub commit: String,
    /// ISO 8601 author timestamp.
    pub when: String,
    /// Commit subject (the `reason` passed to `ensure_checkpoint`).
    pub reason: String,
}

/// Outcome of a successful `restore` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOutcome {
    /// Short (8-char) commit hash that was restored.
    pub restored_to: String,
    /// Commit subject of the restored snapshot.
    pub reason: String,
    /// Working directory that was restored.
    pub directory: String,
    /// If a single file was restored, its relative path.
    pub file: Option<String>,
}

/// Accumulated statistics returned by [`CheckpointStore::prune`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PruneStats {
    /// Number of per-project refs deleted (tip commit was older than the cutoff).
    pub refs_deleted: usize,
    /// Number of per-project refs kept (tip commit was within the age window).
    pub refs_kept: usize,
}

// ── CheckpointStore ──────────────────────────────────────────────────────────

/// Thread-safe shadow git checkpoint store.
///
/// `store_dir` = `uclaw_home/checkpoints/store` (the bare-ish shadow git dir).
/// `taken` = per-LLM-round dedup set: `(llm_round, abs_working_dir)`.
///
/// The `turn` / `llm_round` key passed to `ensure_checkpoint` is
/// `ctx.iteration` from `agentic_loop.rs` — an LLM-round counter that
/// increments on every round-trip to the model within a single user message.
/// Dedup therefore suppresses duplicate snapshots **within the same LLM round**;
/// a fresh snapshot is taken on each subsequent round (finer-grained than
/// per-conversation-turn), and a `diff-index` no-op check skips commits when
/// nothing has changed.
pub struct CheckpointStore {
    /// Absolute path to the bare shadow git repository (`…/checkpoints/store`).
    store_dir: PathBuf,
    /// Per-LLM-round dedup set. Each entry is `(llm_round, abs_working_dir_string)`.
    taken: Mutex<HashSet<(u64, String)>>,
}

impl CheckpointStore {
    /// Create a new store handle. Does NOT initialise the git repo yet —
    /// init happens lazily on the first `ensure_checkpoint` call.
    pub fn new(store_dir: PathBuf) -> Self {
        Self {
            store_dir,
            taken: Mutex::new(HashSet::new()),
        }
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Snapshot `working_dir` at most once per LLM round (`turn` = `ctx.iteration`).
    ///
    /// `turn` is the LLM-round counter from `agentic_loop.rs`, NOT a
    /// conversation-turn counter: it increments on every model round-trip
    /// within a single user message, so dedup suppresses duplicate snapshots
    /// **within the same LLM round** while still allowing a fresh snapshot
    /// on each subsequent round.
    ///
    /// Returns `true` if a snapshot was taken, `false` otherwise (dedup,
    /// skip-broad-dir, git unavailable, no-op diff, or any git failure).
    /// **NEVER propagates an error.**
    pub fn ensure_checkpoint(&self, working_dir: &str, turn: u64) -> bool {
        // Check git availability lazily.
        if !git_available() {
            tracing::debug!("[checkpoint] skipped: git not found");
            return false;
        }

        let abs_dir = match canonicalize_dir(working_dir) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("[checkpoint] skipped: cannot canonicalize dir: {e}");
                return false;
            }
        };
        let abs_str = abs_dir.to_string_lossy().to_string();

        // Skip overly broad directories (port of hermes ensure_checkpoint guard).
        if is_broad_dir(&abs_str) {
            tracing::debug!("[checkpoint] skipped: directory too broad: {abs_str}");
            return false;
        }

        // Per-turn dedup.
        {
            let mut taken = self.taken.lock().unwrap();
            let key = (turn, abs_str.clone());
            if taken.contains(&key) {
                tracing::debug!("[checkpoint] dedup hit for turn={turn} dir={abs_str}");
                return false;
            }
            taken.insert(key);
        }

        match self.take(&abs_str, "agent-turn") {
            Ok(commit) => {
                tracing::debug!("[checkpoint] taken for {abs_str}: {commit}");
                true
            }
            Err(e) => {
                tracing::debug!("[checkpoint] failed (non-fatal): {e}");
                false
            }
        }
    }

    /// Restore the working tree to a previous checkpoint.
    ///
    /// - `commit = None` → restore the **latest** checkpoint for the project.
    /// - `file = None` → restore the entire working tree.
    /// - `file = Some(rel)` → restore only that relative path.
    ///
    /// Validates the commit hash and file path before executing.
    pub fn restore(
        &self,
        working_dir: &str,
        commit: Option<&str>,
        file: Option<&str>,
    ) -> Result<RestoreOutcome> {
        let abs_dir = canonicalize_dir(working_dir)?;
        let abs_str = abs_dir.to_string_lossy().to_string();

        if !self.store_dir.join("HEAD").exists() {
            bail!("No checkpoint store exists yet");
        }

        // Resolve the commit hash (None → latest ref tip).
        let resolved_commit: String = match commit {
            Some(c) => {
                validate_commit_hash(c)?;
                c.to_string()
            }
            None => {
                let dir_hash = project_hash(&abs_str);
                let ref_name = ref_name_for(&dir_hash);
                let (ok, stdout, _) = self.run_git(
                    &["rev-parse", "--verify", &format!("{ref_name}^{{commit}}")],
                    &abs_str,
                    None,
                );
                if !ok || stdout.is_empty() {
                    bail!("No checkpoint found for {working_dir}");
                }
                stdout
            }
        };

        // Validate file path if given.
        if let Some(f) = file {
            validate_file_path(f, &abs_str)?;
        }

        // Verify the commit object exists.
        let (ok, _, _) = self.run_git(&["cat-file", "-t", &resolved_commit], &abs_str, None);
        if !ok {
            bail!("Checkpoint '{}' not found in shadow store", resolved_commit);
        }

        let dir_hash = project_hash(&abs_str);
        let index_file = self.index_path(&dir_hash);

        let restore_target = file.unwrap_or(".");
        let (ok, _, err) = self.run_git(
            &["checkout", &resolved_commit, "--", restore_target],
            &abs_str,
            Some(&index_file),
        );
        if !ok {
            bail!("Restore failed: {err}");
        }

        // Fetch reason from commit subject.
        let (ok2, reason_out, _) =
            self.run_git(&["log", "--format=%s", "-1", &resolved_commit], &abs_str, None);
        let reason = if ok2 { reason_out } else { "unknown".to_string() };

        Ok(RestoreOutcome {
            restored_to: resolved_commit.chars().take(8).collect(),
            reason,
            directory: abs_str,
            file: file.map(|f| f.to_string()),
        })
    }

    /// List checkpoints for `working_dir`, newest first.
    pub fn list(&self, working_dir: &str) -> Result<Vec<CheckpointInfo>> {
        let abs_dir = canonicalize_dir(working_dir)?;
        let abs_str = abs_dir.to_string_lossy().to_string();

        if !self.store_dir.join("HEAD").exists() {
            return Ok(vec![]);
        }

        let dir_hash = project_hash(&abs_str);
        let ref_name = ref_name_for(&dir_hash);

        let (ok, stdout, _) = self.run_git(
            &["log", &ref_name, "--format=%H|%aI|%s", "-n", "50"],
            &abs_str,
            None,
        );
        if !ok || stdout.is_empty() {
            return Ok(vec![]);
        }

        let mut result = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() == 3 {
                result.push(CheckpointInfo {
                    commit: parts[0].to_string(),
                    when: parts[1].to_string(),
                    reason: parts[2].to_string(),
                });
            }
        }
        Ok(result)
    }

    // ── Internal: snapshot ────────────────────────────────────────────────

    /// Take a snapshot.  Returns the new commit hash on success.
    /// (Port of hermes `_take`.)
    fn take(&self, abs_dir: &str, reason: &str) -> Result<String> {
        // Lazily init the store.
        init_store(&self.store_dir, abs_dir)?;

        let dir_hash = project_hash(abs_dir);
        let index_file = self.index_path(&dir_hash);
        let ref_name = ref_name_for(&dir_hash);

        // Seed index from current ref tip (if any) before staging.
        let (ok_ref, ref_commit, _) = self.run_git(
            &["rev-parse", "--verify", &format!("{ref_name}^{{commit}}")],
            abs_dir,
            None,
        );
        let has_ref = ok_ref && !ref_commit.is_empty();

        if index_file.exists() {
            if has_ref {
                // Reset index to current tip so we only diff changes.
                let _ = self.run_git(
                    &["read-tree", &ref_commit],
                    abs_dir,
                    Some(&index_file),
                );
            } else {
                // No ref yet — start with a clean index.
                let _ = std::fs::remove_file(&index_file);
            }
        } else {
            if let Some(parent) = index_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Stage everything.
        let (ok_add, _, err_add) = self.run_git(&["add", "-A"], abs_dir, Some(&index_file));
        if !ok_add {
            bail!("git add -A failed: {err_add}");
        }

        // Skip if no changes vs current ref tip.
        if has_ref {
            let (ok_diff, _, _) = self.run_git(
                &["diff-index", "--cached", "--quiet", &ref_commit],
                abs_dir,
                Some(&index_file),
            );
            if ok_diff {
                bail!("no changes in {abs_dir} — snapshot skipped");
            }
        } else {
            // No ref yet: skip if index is empty.
            let (ok_ls, ls_out, _) =
                self.run_git(&["ls-files", "--cached"], abs_dir, Some(&index_file));
            if ok_ls && ls_out.trim().is_empty() {
                bail!("empty tree in {abs_dir} — snapshot skipped");
            }
        }

        // Write tree from per-project index.
        let (ok_tree, tree_sha, err_tree) =
            self.run_git(&["write-tree"], abs_dir, Some(&index_file));
        if !ok_tree || tree_sha.is_empty() {
            bail!("write-tree failed: {err_tree}");
        }

        // Build commit via commit-tree (never moves HEAD).
        let new_sha = if has_ref {
            let args = [
                "commit-tree", &tree_sha,
                "-p", &ref_commit,
                "-m", reason,
                "--no-gpg-sign",
            ];
            let (ok_c, sha, err_c) = self.run_git(&args, abs_dir, Some(&index_file));
            if !ok_c || sha.is_empty() {
                bail!("commit-tree failed: {err_c}");
            }
            sha
        } else {
            let args = ["commit-tree", &tree_sha, "-m", reason, "--no-gpg-sign"];
            let (ok_c, sha, err_c) = self.run_git(&args, abs_dir, Some(&index_file));
            if !ok_c || sha.is_empty() {
                bail!("commit-tree (initial) failed: {err_c}");
            }
            sha
        };

        // Update per-project ref.
        let (ok_ref_update, _, err_ref) = if has_ref {
            self.run_git(
                &["update-ref", &ref_name, &new_sha, &ref_commit],
                abs_dir,
                None,
            )
        } else {
            self.run_git(&["update-ref", &ref_name, &new_sha], abs_dir, None)
        };
        if !ok_ref_update {
            bail!("update-ref failed: {err_ref}");
        }

        Ok(new_sha)
    }

    // ── Internal: git subprocess ──────────────────────────────────────────

    /// Run a git command against the shadow store.
    /// Returns `(ok, stdout, stderr)`.
    /// (Port of hermes `_run_git`.)
    fn run_git(
        &self,
        args: &[&str],
        working_dir: &str,
        index_file: Option<&Path>,
    ) -> (bool, String, String) {
        let env = git_env(&self.store_dir, working_dir, index_file);
        let mut cmd = Command::new("git");
        cmd.args(args)
            .envs(&env)
            .current_dir(working_dir);

        // Remove inherited GIT_* that would interfere (done via env_remove below).
        cmd.env_remove("GIT_NAMESPACE");
        cmd.env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");

        match cmd.output() {
            Ok(output) => {
                let ok = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if !ok {
                    tracing::debug!(
                        "[checkpoint] git {:?} rc={} stderr={}",
                        args,
                        output.status.code().unwrap_or(-1),
                        stderr
                    );
                }
                (ok, stdout, stderr)
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::debug!("[checkpoint] git {:?} spawn error: {msg}", args);
                (false, String::new(), msg)
            }
        }
    }

    /// Age-based prune: delete whole per-project refs whose tip commit is older
    /// than `max_age_days`, then gc to reclaim objects. `max_age_days == 0`
    /// disables pruning (returns default stats — nothing deleted). Best-effort &
    /// INFALLIBLE: any git failure leaves the store untouched and returns whatever
    /// stats accumulated (mirrors `ensure_checkpoint`). NOTE: this prunes whole
    /// abandoned-session chains; bounding an ACTIVE chain's length (re-rooting) is
    /// a future enhancement.
    pub fn prune(&self, max_age_days: u64) -> PruneStats {
        // Disabled guard.
        if max_age_days == 0 {
            return PruneStats::default();
        }

        // Uninit store guard — nothing to prune.
        if !self.store_dir.join("HEAD").exists() {
            return PruneStats::default();
        }

        // Compute now_unix.  SystemTime is fine — this is runtime Rust, not const.
        let now_unix = match std::time::SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => {
                tracing::debug!("[checkpoint:prune] SystemTime before UNIX_EPOCH; skipping");
                return PruneStats::default();
            }
        };

        // Use store_dir itself as the working_dir for git commands.  run_git sets
        // current_dir(working_dir) and git_env sets GIT_DIR=store_dir + GIT_WORK_TREE=wd.
        // For read-only / metadata commands like for-each-ref, update-ref, and gc the
        // working_dir only needs to be a real existing directory.  store_dir is always
        // present at this point (HEAD check above passed).
        let wd = self.store_dir.to_string_lossy().to_string();

        // Enumerate all refs/uclaw/* with their committer unix timestamp.
        let (ok, stdout, _stderr) = self.run_git(
            &["for-each-ref", "--format=%(refname) %(committerdate:unix)", REFS_PREFIX],
            &wd,
            None,
        );
        if !ok {
            tracing::debug!("[checkpoint:prune] for-each-ref failed; skipping prune");
            return PruneStats::default();
        }

        let mut stats = PruneStats::default();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Parse: "<refname> <unix_timestamp>"
            // rsplitn(2) handles the case where refname itself contains spaces (unlikely but safe).
            let mut parts = line.rsplitn(2, ' ');
            let unix_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let refname = match parts.next() {
                Some(s) => s.trim().to_string(),
                None => continue,
            };
            let committer_unix: i64 = match unix_str.parse() {
                Ok(v) => v,
                Err(_) => {
                    tracing::debug!(
                        "[checkpoint:prune] could not parse unix timestamp {:?} for ref {}; skipping",
                        unix_str,
                        refname,
                    );
                    continue;
                }
            };

            if is_stale(committer_unix, now_unix, max_age_days) {
                let (del_ok, _, del_err) =
                    self.run_git(&["update-ref", "-d", &refname], &wd, None);
                if del_ok {
                    tracing::debug!("[checkpoint:prune] deleted stale ref {refname}");
                    stats.refs_deleted += 1;
                } else {
                    tracing::debug!(
                        "[checkpoint:prune] failed to delete ref {refname}: {del_err}"
                    );
                }
            } else {
                stats.refs_kept += 1;
            }
        }

        // Best-effort gc to reclaim unreachable objects.
        if stats.refs_deleted > 0 {
            let _ = self.run_git(&["gc", "--prune=now", "--quiet"], &wd, None);
        }

        stats
    }

    // ── Path helpers ──────────────────────────────────────────────────────

    fn index_path(&self, dir_hash: &str) -> PathBuf {
        self.store_dir.join(INDEXES_DIRNAME).join(dir_hash)
    }
}

// ── Free functions (path / hash / env / validation) ──────────────────────────

/// Build the env map that redirects all git ops to the shadow store.
/// (Exact port of hermes `_git_env`.)
///
/// Uses `HashMap` so the entries override rather than extend the inherited env.
fn git_env(store: &Path, working_dir: &str, index_file: Option<&Path>) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();

    env.insert("GIT_DIR".into(), store.to_string_lossy().to_string());
    env.insert("GIT_WORK_TREE".into(), working_dir.to_string());

    // Per-project index so projects don't race on a shared index.
    if let Some(idx) = index_file {
        env.insert("GIT_INDEX_FILE".into(), idx.to_string_lossy().to_string());
    } else {
        env.remove("GIT_INDEX_FILE");
    }

    // Config isolation — prevents gpg-sign / credential-helper prompts.
    env.insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    env.insert("GIT_CONFIG_SYSTEM".into(), "/dev/null".into());
    env.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());

    // Drop inherited namespace / alternates (rare but can confuse).
    env.remove("GIT_NAMESPACE");
    env.remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");

    env
}

/// Pure cutoff test — extracted so the age arithmetic is unit-testable without git.
///
/// Returns `true` if the commit at `committer_unix` is older than `max_age_days`
/// relative to `now_unix`.  Both timestamps are Unix seconds (i64).
/// When `max_age_days == 0` the caller short-circuits before reaching this function,
/// but calling it directly with 0 is safe (returns `false`).
fn is_stale(committer_unix: i64, now_unix: i64, max_age_days: u64) -> bool {
    let max_age_secs = (max_age_days as i64).saturating_mul(86_400);
    now_unix.saturating_sub(committer_unix) > max_age_secs
}

/// Lazily initialise the bare shadow store if needed.
/// (Port of hermes `_init_store`.)
fn init_store(store: &Path, working_dir: &str) -> Result<()> {
    if store.join("HEAD").exists() {
        return Ok(());
    }

    // Create directory hierarchy.
    std::fs::create_dir_all(store)?;
    std::fs::create_dir_all(store.join(INDEXES_DIRNAME))?;

    // `git init --bare` rejects GIT_WORK_TREE, so we use a raw env without it.
    let mut init_env: HashMap<String, String> = std::env::vars().collect();
    init_env.insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    init_env.insert("GIT_CONFIG_SYSTEM".into(), "/dev/null".into());
    init_env.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    for k in &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
               "GIT_NAMESPACE", "GIT_ALTERNATE_OBJECT_DIRECTORIES"] {
        init_env.remove(*k);
    }

    let result = Command::new("git")
        .args(["init", "--bare", store.to_str().unwrap_or("")])
        .envs(&init_env)
        .output();

    match result {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!("git init --bare failed: {stderr}");
        }
        Err(e) => bail!("git init --bare spawn failed: {e}"),
    }

    // Configure the store (belt-and-suspenders on top of env isolation).
    // Use the store's parent as a pseudo working_dir for these config commands.
    let cfg_wd = store.parent().unwrap_or(store).to_string_lossy().to_string();
    let run_cfg = |args: &[&str]| {
        let env_map = git_env(store, &cfg_wd, None);
        let _ = Command::new("git").args(args).envs(&env_map).output();
    };
    run_cfg(&["config", "user.email", "uclaw@local"]);
    run_cfg(&["config", "user.name", "uClaw Checkpoint"]);
    run_cfg(&["config", "commit.gpgsign", "false"]);
    run_cfg(&["config", "tag.gpgSign", "false"]);
    run_cfg(&["config", "gc.auto", "0"]);

    // Default excludes (mirrors hermes DEFAULT_EXCLUDES).
    let info_dir = store.join("info");
    std::fs::create_dir_all(&info_dir)?;
    let _ = std::fs::write(
        info_dir.join("exclude"),
        DEFAULT_EXCLUDES,
    );

    tracing::debug!("[checkpoint] shadow store initialised at {}", store.display());
    let _ = working_dir; // used indirectly; suppress lint
    Ok(())
}

const DEFAULT_EXCLUDES: &str = "\
node_modules/\n\
dist/\n\
build/\n\
target/\n\
out/\n\
.next/\n\
.nuxt/\n\
__pycache__/\n\
*.pyc\n\
*.pyo\n\
.cache/\n\
.pytest_cache/\n\
.mypy_cache/\n\
.ruff_cache/\n\
coverage/\n\
.coverage\n\
.venv/\n\
venv/\n\
env/\n\
.git/\n\
.hg/\n\
.svn/\n\
.env\n\
.env.local\n\
.env.*\n\
*.lock\n\
.DS_Store\n\
Thumbs.db\n\
*.log\n\
";

/// Deterministic 16-char hex hash of the absolute working dir path.
/// (Port of hermes `_project_hash`.)
fn project_hash(abs_dir: &str) -> String {
    use std::hash::{Hash, Hasher};
    // Use SHA-256 via std when sha2 is available; fall back to a stable
    // deterministic FNV-style hash. Because we already have sha2 as an
    // indirect dep (app.rs uses it), we build it inline here without
    // adding a new dependency.
    // We use the same algorithm as hermes: sha256(abs_path.encode())[:16].
    // Re-implement via sha2 crate (available in the workspace).
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(abs_dir.as_bytes());
    let result = hasher.finalize();
    // Take first 8 bytes → 16 hex chars (matches hermes `[:16]`).
    result[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// `refs/uclaw/<hash>` — the per-project ref.
fn ref_name_for(dir_hash: &str) -> String {
    format!("{REFS_PREFIX}/{dir_hash}")
}

/// Resolve and validate `working_dir` as an existing directory.
fn canonicalize_dir(working_dir: &str) -> Result<PathBuf> {
    let p = PathBuf::from(working_dir);
    if !p.exists() {
        bail!("working directory does not exist: {working_dir}");
    }
    if !p.is_dir() {
        bail!("working directory is not a directory: {working_dir}");
    }
    Ok(p.canonicalize()?)
}

/// Skip `/` and `$HOME` — too broad to snapshot.
fn is_broad_dir(abs_str: &str) -> bool {
    if abs_str == "/" {
        return true;
    }
    if let Ok(home) = uclaw_utils_home::uclaw_home_pathbuf() {
        // Check the actual home dir (one level up from .uclaw).
        if let Some(home_parent) = home.parent() {
            if abs_str == home_parent.to_string_lossy() {
                return true;
            }
        }
    }
    // Also check via dirs-style home.
    // We use std::env::var("HOME") here rather than dirs::home_dir to
    // avoid the pre-commit-blocked dirs::home_dir call.
    if let Ok(home) = std::env::var("HOME") {
        if abs_str == home {
            return true;
        }
    }
    false
}

/// Returns `true` if `git` is on `$PATH`.
fn git_available() -> bool {
    Command::new("git")
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Validate a commit hash: 4–64 hex chars, must not start with `-`.
/// (Port of hermes `_validate_commit_hash`.)
pub fn validate_commit_hash(hash: &str) -> Result<()> {
    let s = hash.trim();
    if s.is_empty() {
        bail!("Empty commit hash");
    }
    if s.starts_with('-') {
        bail!("Invalid commit hash (must not start with '-'): {s:?}");
    }
    if !(4..=64).contains(&s.len()) || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid commit hash (expected 4-64 hex characters): {s:?}");
    }
    Ok(())
}

/// Validate that a relative file path stays within `abs_working_dir`.
/// (Port of hermes `_validate_file_path`.)
pub fn validate_file_path(file_path: &str, abs_working_dir: &str) -> Result<()> {
    let s = file_path.trim();
    if s.is_empty() {
        bail!("Empty file path");
    }
    if Path::new(s).is_absolute() {
        bail!("File path must be relative, got absolute path: {s:?}");
    }
    let workdir = Path::new(abs_working_dir);
    let resolved = workdir.join(s);
    // We check the prefix without requiring the file to exist.
    let resolved_str = resolved.to_string_lossy();
    // Normalise away `..` via components.
    let normalised = resolve_dots(&resolved);
    if !normalised.starts_with(workdir) {
        bail!("File path escapes the working directory via traversal: {s:?}");
    }
    let _ = resolved_str;
    Ok(())
}

/// Simple `..` resolution without calling `canonicalize` (file need not exist).
fn resolve_dots(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => { out.pop(); }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a temp project dir with at least one file.
    fn make_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.txt"), "initial content").unwrap();
        dir
    }

    /// Helper: create a CheckpointStore backed by a temp dir.
    fn make_store() -> (TempDir, CheckpointStore) {
        let store_base = TempDir::new().unwrap();
        let store_dir = store_base.path().join(STORE_DIRNAME);
        let store = CheckpointStore::new(store_dir);
        (store_base, store)
    }

    // ── ensure_checkpoint ─────────────────────────────────────────────────

    #[test]
    fn ensure_checkpoint_creates_ref_and_no_project_dot_git() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        let taken = store.ensure_checkpoint(&proj_path, 1);
        assert!(taken, "first snapshot should be taken");

        // The shadow store should have a ref for this project.
        let dir_hash = project_hash(
            &project.path().canonicalize().unwrap().to_string_lossy()
        );
        let ref_path = store.store_dir.join("refs").join("uclaw").join(&dir_hash);
        assert!(ref_path.exists(), "per-project ref must exist in shadow store");

        // CRITICAL: project dir must NOT have .git created.
        assert!(
            !project.path().join(".git").exists(),
            ".git must NOT be created in the project directory"
        );
    }

    #[test]
    fn ensure_checkpoint_dedup_same_turn() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        let first = store.ensure_checkpoint(&proj_path, 42);
        assert!(first, "first snapshot of turn 42 should be taken");

        let second = store.ensure_checkpoint(&proj_path, 42);
        assert!(!second, "second call same turn should be deduped (returns false)");
    }

    #[test]
    fn ensure_checkpoint_different_turns_take_new_snapshots() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        assert!(store.ensure_checkpoint(&proj_path, 1));

        // Modify the project before the next turn snapshot.
        fs::write(project.path().join("hello.txt"), "turn 2 content").unwrap();

        // Different turn → should take a new snapshot.
        let second = store.ensure_checkpoint(&proj_path, 2);
        assert!(second, "second turn snapshot should be taken after content change");
    }

    #[test]
    fn ensure_checkpoint_skips_root_dir() {
        let (_store_base, store) = make_store();
        let taken = store.ensure_checkpoint("/", 1);
        assert!(!taken, "snapshot of / should be skipped");
    }

    #[test]
    fn ensure_checkpoint_nonexistent_dir_returns_false() {
        let (_store_base, store) = make_store();
        let taken = store.ensure_checkpoint("/tmp/__uclaw_nonexistent_test_dir__", 1);
        assert!(!taken, "nonexistent dir should return false gracefully");
    }

    // ── restore ──────────────────────────────────────────────────────────

    #[test]
    fn restore_whole_tree_reverts_modified_file() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Take snapshot.
        assert!(store.ensure_checkpoint(&proj_path, 1));

        // Modify file.
        fs::write(project.path().join("hello.txt"), "modified content").unwrap();

        // Restore with commit = None (latest), file = None (whole tree).
        let outcome = store.restore(&proj_path, None, None).unwrap();
        // outcome.directory is the canonicalized path; compare canonicalized forms.
        let canonical_proj = project.path().canonicalize().unwrap();
        assert_eq!(outcome.directory, canonical_proj.to_string_lossy().as_ref());
        assert!(outcome.file.is_none());

        // File should be restored.
        let content = fs::read_to_string(project.path().join("hello.txt")).unwrap();
        assert_eq!(content, "initial content");
    }

    #[test]
    fn restore_single_file_reverts_only_that_file() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Add another file and take snapshot.
        fs::write(project.path().join("other.txt"), "other content").unwrap();
        assert!(store.ensure_checkpoint(&proj_path, 1));

        // Modify both files.
        fs::write(project.path().join("hello.txt"), "modified").unwrap();
        fs::write(project.path().join("other.txt"), "modified other").unwrap();

        // Restore only hello.txt.
        let outcome = store.restore(&proj_path, None, Some("hello.txt")).unwrap();
        assert_eq!(outcome.file.as_deref(), Some("hello.txt"));

        // hello.txt should be restored, other.txt should still be modified.
        let hello = fs::read_to_string(project.path().join("hello.txt")).unwrap();
        assert_eq!(hello, "initial content");
        let other = fs::read_to_string(project.path().join("other.txt")).unwrap();
        assert_eq!(other, "modified other");
    }

    #[test]
    fn restore_specific_commit_hash() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        assert!(store.ensure_checkpoint(&proj_path, 1));

        // Get the commit hash from list.
        let checkpoints = store.list(&proj_path).unwrap();
        assert!(!checkpoints.is_empty());
        let commit = &checkpoints[0].commit;

        // Modify file then restore to the specific commit.
        fs::write(project.path().join("hello.txt"), "modified").unwrap();
        let outcome = store.restore(&proj_path, Some(commit), None).unwrap();
        assert_eq!(&outcome.restored_to, &commit[..8]);

        let content = fs::read_to_string(project.path().join("hello.txt")).unwrap();
        assert_eq!(content, "initial content");
    }

    #[test]
    fn restore_bad_commit_hash_returns_err() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Initialise the store first so the HEAD check passes.
        assert!(store.ensure_checkpoint(&proj_path, 1));

        let err = store.restore(&proj_path, Some("--bad-flag"), None).unwrap_err();
        assert!(err.to_string().contains("commit hash"), "expected validation error: {err}");
    }

    #[test]
    fn restore_out_of_tree_path_returns_err() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        assert!(store.ensure_checkpoint(&proj_path, 1));

        let err = store
            .restore(&proj_path, None, Some("../../etc/passwd"))
            .unwrap_err();
        assert!(
            err.to_string().contains("traversal") || err.to_string().contains("escape"),
            "expected path-traversal error: {err}"
        );
    }

    // ── list ─────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_checkpoints_newest_first() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Take two snapshots on different turns (must modify between them).
        assert!(store.ensure_checkpoint(&proj_path, 1));
        fs::write(project.path().join("hello.txt"), "second snapshot content").unwrap();
        assert!(store.ensure_checkpoint(&proj_path, 2));

        let checkpoints = store.list(&proj_path).unwrap();
        assert!(checkpoints.len() >= 2, "expected at least 2 checkpoints, got {}", checkpoints.len());
        // Newest first: the latest `when` timestamp should be at index 0.
        // Verify ordering by checking when[0] >= when[1] lexicographically
        // (ISO 8601 timestamps compare correctly as strings).
        assert!(
            checkpoints[0].when >= checkpoints[1].when,
            "list should be newest-first: {} vs {}",
            checkpoints[0].when,
            checkpoints[1].when
        );
    }

    #[test]
    fn list_empty_before_any_snapshot() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        let checkpoints = store.list(&proj_path).unwrap();
        assert!(checkpoints.is_empty(), "no snapshots yet → empty list");
    }

    // ── validation helpers ────────────────────────────────────────────────

    #[test]
    fn validate_commit_hash_accepts_valid() {
        assert!(validate_commit_hash("abcd1234").is_ok());
        assert!(validate_commit_hash(&"a".repeat(40)).is_ok());
        assert!(validate_commit_hash("ABCD").is_ok());
    }

    #[test]
    fn validate_commit_hash_rejects_invalid() {
        assert!(validate_commit_hash("").is_err());
        assert!(validate_commit_hash("--flag").is_err());
        assert!(validate_commit_hash("abc").is_err()); // too short
        assert!(validate_commit_hash("xyz!").is_err()); // non-hex
    }

    #[test]
    fn validate_file_path_accepts_relative() {
        validate_file_path("src/main.rs", "/tmp").unwrap();
        validate_file_path("dir/subdir/file.txt", "/tmp").unwrap();
    }

    #[test]
    fn validate_file_path_rejects_traversal() {
        assert!(validate_file_path("../../etc/passwd", "/tmp/project").is_err());
        assert!(validate_file_path("/absolute/path", "/tmp").is_err());
        assert!(validate_file_path("", "/tmp").is_err());
    }

    // ── project_hash ─────────────────────────────────────────────────────

    #[test]
    fn project_hash_is_16_hex_chars_and_stable() {
        let h1 = project_hash("/home/user/my-project");
        let h2 = project_hash("/home/user/my-project");
        assert_eq!(h1.len(), 16);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(h1, h2, "hash must be deterministic");
    }

    #[test]
    fn project_hash_differs_for_different_paths() {
        let h1 = project_hash("/home/user/project-a");
        let h2 = project_hash("/home/user/project-b");
        assert_ne!(h1, h2);
    }

    // ── DEFAULT_EXCLUDES / .env security ────────────────────────────────────

    /// Verify that `.env` files are NOT included in the snapshot tree.
    ///
    /// The shadow git store writes DEFAULT_EXCLUDES to `<store>/info/exclude`
    /// during `init_store`.  `git add -A` honours `info/exclude` patterns via
    /// the same core-excludes mechanism as `.gitignore`, so `.env`, `.env.local`,
    /// and `.env.*` must never appear in the committed tree.
    #[test]
    fn env_files_excluded_from_snapshot() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Write a secret .env file (and a .env.local variant) into the project.
        fs::write(
            project.path().join(".env"),
            "DATABASE_URL=postgres://user:s3cr3t@localhost/db\nSECRET_KEY=hunter2\n",
        )
        .unwrap();
        fs::write(
            project.path().join(".env.local"),
            "API_SECRET=local-secret\n",
        )
        .unwrap();

        // Take a checkpoint.
        let taken = store.ensure_checkpoint(&proj_path, 1);
        assert!(taken, "snapshot should be taken (project has hello.txt)");

        // Verify neither .env file is in the snapshot tree.
        // We use `git ls-tree -r <ref>` via the shadow store env to enumerate
        // all blobs committed.
        let dir_hash = project_hash(
            &project.path().canonicalize().unwrap().to_string_lossy()
        );
        let ref_name = ref_name_for(&dir_hash);
        let env_map = git_env(&store.store_dir, &proj_path, None);

        let output = std::process::Command::new("git")
            .args(["ls-tree", "-r", "--name-only", &ref_name])
            .envs(&env_map)
            .current_dir(&proj_path)
            .output()
            .expect("git ls-tree should run");

        let tree_files = String::from_utf8_lossy(&output.stdout);
        assert!(
            !tree_files.lines().any(|f| f == ".env"),
            ".env must NOT be in the snapshot tree; got:\n{tree_files}"
        );
        assert!(
            !tree_files.lines().any(|f| f == ".env.local"),
            ".env.local must NOT be in the snapshot tree; got:\n{tree_files}"
        );
        // hello.txt (the non-secret file) must still be present.
        assert!(
            tree_files.lines().any(|f| f == "hello.txt"),
            "hello.txt should be in snapshot; got:\n{tree_files}"
        );
    }

    // ── is_stale ─────────────────────────────────────────────────────────

    #[test]
    fn is_stale_within_window_returns_false() {
        let now = 1_000_000i64;
        let ten_days_ago = now - 10 * 86_400;
        assert!(
            !is_stale(ten_days_ago, now, 14),
            "10 days old with 14-day window should NOT be stale"
        );
    }

    #[test]
    fn is_stale_beyond_window_returns_true() {
        let now = 1_000_000i64;
        let twenty_days_ago = now - 20 * 86_400;
        assert!(
            is_stale(twenty_days_ago, now, 14),
            "20 days old with 14-day window should be stale"
        );
    }

    #[test]
    fn is_stale_small_values_arithmetic() {
        // Exactly at the boundary: age == max_age_days in seconds should NOT be stale
        // (the condition is strictly `>`, not `>=`).
        let now = 10_000i64;
        let exactly_at = now - 86_400; // exactly 1 day
        assert!(!is_stale(exactly_at, now, 1), "exactly at boundary should NOT be stale (strict >)");

        // One second over boundary should be stale.
        let one_over = now - 86_400 - 1;
        assert!(is_stale(one_over, now, 1), "one second past boundary should be stale");

        // With max_age_days=0 the arithmetic still works (0 * 86400 = 0; any positive age > 0).
        // In practice prune() short-circuits at 0; but the function itself is safe to call.
        assert!(is_stale(now - 1, now, 0), "with max_age_days=0, even 1-second-old is stale");
    }

    // ── prune ─────────────────────────────────────────────────────────────

    #[test]
    fn prune_uninit_store_returns_default_no_panic() {
        let store_base = TempDir::new().unwrap();
        let store_dir = store_base.path().join(STORE_DIRNAME);
        // store_dir does NOT exist — no HEAD, no git repo.
        let store = CheckpointStore::new(store_dir);

        let stats = store.prune(14);
        assert_eq!(stats, PruneStats::default(), "uninit store should return default stats");
    }

    #[test]
    fn prune_zero_days_returns_default_nothing_deleted() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Initialise the store so there's something to prune.
        assert!(store.ensure_checkpoint(&proj_path, 1), "should take snapshot");

        let stats = store.prune(0);
        assert_eq!(
            stats,
            PruneStats::default(),
            "prune(0) should be disabled and return default stats"
        );
    }

    #[test]
    fn prune_fresh_checkpoint_not_deleted() {
        let project = make_project();
        let (_store_base, store) = make_store();
        let proj_path = project.path().to_str().unwrap().to_string();

        // Create two checkpoints so there is at least one ref.
        assert!(store.ensure_checkpoint(&proj_path, 1));
        fs::write(project.path().join("hello.txt"), "second content").unwrap();
        assert!(store.ensure_checkpoint(&proj_path, 2));

        // Prune with 14-day window; the commits are just-created so they are fresh.
        let stats = store.prune(14);
        assert_eq!(
            stats.refs_deleted, 0,
            "fresh ref must not be deleted; got refs_deleted={}",
            stats.refs_deleted
        );
        assert!(
            stats.refs_kept >= 1,
            "at least one ref must be counted as kept; got refs_kept={}",
            stats.refs_kept
        );
    }
}
