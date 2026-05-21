//! Bundle 26-A — Skill use telemetry.
//!
//! Tracks per-skill usage to close the auto-extraction feedback loop:
//! every `_auto_extracted/<slug>/` (and other tiers) gets a sibling
//! `meta.json` recording when/how often the skill was returned by
//! `skill_search` and how often it was followed by a successful or
//! failing next turn.
//!
//! Used by:
//! - `skill_search` tool — calls `record_returned` for each hit.
//! - dispatcher (post-turn) — calls `record_outcome` based on whether
//!   the turn that consumed the skill ended in failure or not.
//! - future `SkillDistillationScenario` (Bundle 26-B) — reads
//!   `unused_for_days` + `success_rate` to decide what to prune /
//!   merge.
//!
//! Format is JSON (one file per skill) for easy human inspection and
//! atomic write via `tempfile::NamedTempFile` + persist.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persistent usage stats for a single learned skill.
///
/// Stored as `<skill_dir>/meta.json` next to `SKILL.md`. Schema is
/// designed to be **monotonic + safe to merge** — callers only ever
/// increment counters and update timestamps, never decrement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillMeta {
    /// Slug of the skill (matches the directory name).
    pub slug: String,
    /// Unix ms — when the meta.json was first created.
    pub created_at: i64,
    /// Unix ms — when stats were last updated (any of the increments below).
    pub updated_at: i64,

    /// How many times `skill_search` returned this skill as a hit.
    /// This is the **candidate** signal — the LLM saw it; doesn't mean
    /// it was actually applied.
    #[serde(default)]
    pub returned_count: u32,
    /// Unix ms — last time `skill_search` returned this skill.
    #[serde(default)]
    pub last_returned_at: Option<i64>,

    /// How many times the agent turn that followed a `skill_search`
    /// return for this skill **completed without a failure outcome**.
    /// Increment from dispatcher post-loop when LoopOutcome::Response
    /// or LoopOutcome::ToolResult and the most-recent skill_search
    /// candidate set included this slug.
    #[serde(default)]
    pub success_count: u32,
    /// Mirror counter — turn ended in LoopOutcome::Failure /
    /// MaxIterations, or a tool error was visible in the turn.
    #[serde(default)]
    pub failure_count: u32,
    /// Unix ms — last time this skill participated in any outcome.
    #[serde(default)]
    pub last_used_at: Option<i64>,

    /// Schema version — bump if we add non-additive fields later.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    1
}

impl SkillMeta {
    /// Create a fresh meta record stamped with the real wall clock.
    /// Convenience wrapper around `new_at(slug, chrono::Utc::now().timestamp_millis())`
    /// for production callers that don't need to inject time.
    pub fn new(slug: impl Into<String>) -> Self {
        Self::new_at(slug, chrono::Utc::now().timestamp_millis())
    }

    /// Create a fresh meta record stamped with the given timestamp.
    /// Tests / time-injecting callers use this so `record_returned` and
    /// friends can keep their API fully deterministic in `now_ms`.
    pub fn new_at(slug: impl Into<String>, now_ms: i64) -> Self {
        Self {
            slug: slug.into(),
            created_at: now_ms,
            updated_at: now_ms,
            returned_count: 0,
            last_returned_at: None,
            success_count: 0,
            failure_count: 0,
            last_used_at: None,
            schema_version: default_schema_version(),
        }
    }

    /// Total observed "outcomes" — success + failure.
    pub fn observed_outcomes(&self) -> u32 {
        self.success_count + self.failure_count
    }

    /// `success_count / observed_outcomes`. Returns `None` when no
    /// outcomes have been observed (caller decides how to bias new
    /// skills — typically as "neutral").
    pub fn success_rate(&self) -> Option<f32> {
        let n = self.observed_outcomes();
        if n == 0 {
            None
        } else {
            Some(self.success_count as f32 / n as f32)
        }
    }

    /// Days since last skill_search hit. Returns `None` if never returned.
    /// Useful for the future distillation pruner.
    pub fn unused_for_days(&self, now_ms: i64) -> Option<f64> {
        let last = self.last_returned_at?;
        let delta_ms = (now_ms - last).max(0) as f64;
        Some(delta_ms / (1000.0 * 60.0 * 60.0 * 24.0))
    }
}

// ────────────────────────────────────────────────────────────────────────
// Persistence — load / store
// ────────────────────────────────────────────────────────────────────────

/// Build the canonical meta.json path for a skill directory.
pub fn meta_path_for_skill_dir(skill_dir: &Path) -> PathBuf {
    skill_dir.join("meta.json")
}

/// Load meta.json. Returns `Ok(None)` if the file doesn't exist
/// (legitimate for never-touched skills); `Err` only for IO / parse
/// errors that the caller should log + treat as "no telemetry".
pub fn load_meta(skill_dir: &Path) -> std::io::Result<Option<SkillMeta>> {
    let path = meta_path_for_skill_dir(skill_dir);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    match serde_json::from_str::<SkillMeta>(&raw) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse meta.json at {}: {}", path.display(), e),
        )),
    }
}

/// Atomic write — serialize to a sibling temp file, fsync, then rename
/// into place. Prevents half-written meta.json if the process crashes
/// (Bundle 27-C's exact concern) mid-update.
pub fn save_meta(skill_dir: &Path, meta: &SkillMeta) -> std::io::Result<()> {
    fs::create_dir_all(skill_dir)?;
    let final_path = meta_path_for_skill_dir(skill_dir);
    // Same-directory temp file → rename is atomic on the same filesystem.
    let tmp_path = final_path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp_path)?;
        let body = serde_json::to_string_pretty(meta).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("serialize meta.json: {}", e),
            )
        })?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?; // ensure bytes are on disk before rename
    }
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

/// Load-or-default — returns existing meta or a fresh meta stamped with `now_ms`.
///
/// `now_ms` is threaded through (rather than calling `chrono::Utc::now()` here)
/// so the surrounding `record_returned` / `record_outcome` API stays fully
/// deterministic in their `now_ms` argument. Tests rely on this property.
pub fn load_or_init(skill_dir: &Path, slug: &str, now_ms: i64) -> SkillMeta {
    match load_meta(skill_dir) {
        Ok(Some(meta)) => meta,
        Ok(None) => SkillMeta::new_at(slug, now_ms),
        Err(e) => {
            tracing::warn!(
                skill_dir = %skill_dir.display(),
                error = %e,
                "[skill_telemetry] load_meta failed, starting fresh meta"
            );
            SkillMeta::new_at(slug, now_ms)
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Update primitives — called from the two write sites
// ────────────────────────────────────────────────────────────────────────

/// Outcome label for `record_outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillOutcome {
    /// Agent turn that consulted this skill ended without a failure
    /// signal (Response / ToolResult).
    Success,
    /// Agent turn ended in Failure / MaxIterations / surfaced tool error.
    Failure,
}

/// Record that `skill_search` returned this skill as a hit.
///
/// Idempotent-safe: increments `returned_count` and refreshes
/// `last_returned_at` to `now_ms`. Creates meta.json if missing.
pub fn record_returned(skill_dir: &Path, slug: &str, now_ms: i64) -> std::io::Result<()> {
    let mut meta = load_or_init(skill_dir, slug, now_ms);
    meta.returned_count = meta.returned_count.saturating_add(1);
    meta.last_returned_at = Some(now_ms);
    meta.updated_at = now_ms;
    save_meta(skill_dir, &meta)
}

/// Record that an agent turn consumed this skill and produced an
/// outcome (Success or Failure). Increments the matching counter and
/// refreshes `last_used_at`.
pub fn record_outcome(
    skill_dir: &Path,
    slug: &str,
    outcome: SkillOutcome,
    now_ms: i64,
) -> std::io::Result<()> {
    let mut meta = load_or_init(skill_dir, slug, now_ms);
    match outcome {
        SkillOutcome::Success => {
            meta.success_count = meta.success_count.saturating_add(1);
        }
        SkillOutcome::Failure => {
            meta.failure_count = meta.failure_count.saturating_add(1);
        }
    }
    meta.last_used_at = Some(now_ms);
    meta.updated_at = now_ms;
    save_meta(skill_dir, &meta)
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixed_now() -> i64 {
        // 2026-05-22T00:00:00Z in ms
        1_779_488_000_000
    }

    #[test]
    fn new_meta_has_zero_counters() {
        let meta = SkillMeta::new("foo");
        assert_eq!(meta.slug, "foo");
        assert_eq!(meta.returned_count, 0);
        assert_eq!(meta.success_count, 0);
        assert_eq!(meta.failure_count, 0);
        assert!(meta.last_returned_at.is_none());
        assert!(meta.last_used_at.is_none());
        assert_eq!(meta.schema_version, 1);
        assert_eq!(meta.success_rate(), None);
        assert_eq!(meta.observed_outcomes(), 0);
    }

    #[test]
    fn success_rate_computed_correctly() {
        let mut meta = SkillMeta::new("x");
        meta.success_count = 3;
        meta.failure_count = 1;
        assert_eq!(meta.observed_outcomes(), 4);
        assert!((meta.success_rate().unwrap() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn record_returned_creates_meta_when_missing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("test-skill");
        let now = fixed_now();
        record_returned(&dir, "test-skill", now).unwrap();
        let meta = load_meta(&dir).unwrap().expect("meta should exist");
        assert_eq!(meta.slug, "test-skill");
        assert_eq!(meta.returned_count, 1);
        assert_eq!(meta.last_returned_at, Some(now));
        assert_eq!(meta.created_at, now);
        assert_eq!(meta.updated_at, now);
    }

    #[test]
    fn record_returned_is_monotonic() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s");
        let t1 = fixed_now();
        let t2 = t1 + 1_000;
        let t3 = t2 + 5_000;
        record_returned(&dir, "s", t1).unwrap();
        record_returned(&dir, "s", t2).unwrap();
        record_returned(&dir, "s", t3).unwrap();
        let meta = load_meta(&dir).unwrap().unwrap();
        assert_eq!(meta.returned_count, 3);
        assert_eq!(meta.last_returned_at, Some(t3));
        assert_eq!(meta.created_at, t1); // unchanged
    }

    #[test]
    fn record_outcome_increments_correct_counter() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s");
        let now = fixed_now();
        record_outcome(&dir, "s", SkillOutcome::Success, now).unwrap();
        record_outcome(&dir, "s", SkillOutcome::Success, now + 1).unwrap();
        record_outcome(&dir, "s", SkillOutcome::Failure, now + 2).unwrap();
        let meta = load_meta(&dir).unwrap().unwrap();
        assert_eq!(meta.success_count, 2);
        assert_eq!(meta.failure_count, 1);
        assert_eq!(meta.last_used_at, Some(now + 2));
        assert!((meta.success_rate().unwrap() - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn unused_for_days_computed_correctly() {
        let mut meta = SkillMeta::new("x");
        let now = fixed_now();
        let three_days_ago = now - 3 * 86_400_000;
        meta.last_returned_at = Some(three_days_ago);
        let days = meta.unused_for_days(now).unwrap();
        assert!((days - 3.0).abs() < 0.01, "expected ~3.0 days, got {days}");
    }

    #[test]
    fn unused_for_days_none_when_never_returned() {
        let meta = SkillMeta::new("x");
        assert!(meta.unused_for_days(fixed_now()).is_none());
    }

    #[test]
    fn load_meta_missing_file_returns_ok_none() {
        let tmp = TempDir::new().unwrap();
        assert!(load_meta(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn save_then_load_roundtrips_all_fields() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s");
        let mut orig = SkillMeta::new("rt-test");
        orig.returned_count = 7;
        orig.success_count = 4;
        orig.failure_count = 2;
        orig.last_returned_at = Some(fixed_now());
        orig.last_used_at = Some(fixed_now() + 100);
        save_meta(&dir, &orig).unwrap();
        let loaded = load_meta(&dir).unwrap().unwrap();
        assert_eq!(loaded, orig);
    }

    #[test]
    fn save_is_atomic_no_tmp_left_behind() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s");
        let meta = SkillMeta::new("atomic");
        save_meta(&dir, &meta).unwrap();
        // meta.json present, meta.json.tmp absent
        assert!(dir.join("meta.json").exists());
        assert!(!dir.join("meta.json.tmp").exists());
    }

    #[test]
    fn load_or_init_returns_fresh_when_corrupt() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("meta.json"), "{ not valid json").unwrap();
        let meta = load_or_init(&dir, "fallback-slug", fixed_now());
        assert_eq!(meta.slug, "fallback-slug");
        assert_eq!(meta.returned_count, 0);
    }
}
