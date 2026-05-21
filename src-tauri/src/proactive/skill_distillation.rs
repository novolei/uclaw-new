//! Bundle 26-B — idle-time skill distillation.
//!
//! Reads the meta.json telemetry that Bundle 26-A writes next to each
//! auto-extracted SKILL.md, and uses it to prune / merge the skill
//! library when the system is idle. This is the "求精" half of the
//! skill loop — Bundle 24 made extraction more aggressive (5 tool
//! calls / 60s), which produces noise; Bundle 26-B cleans up the
//! noise so the library converges instead of growing forever.
//!
//! v1 scope (this commit):
//! - Scan `_auto_extracted/` for skills with `returned_count == 0`
//!   AND `unused_for_days > min_unused_days`.
//! - Move them to `_archive/<timestamp>/<slug>/`. Reversible — the
//!   user (or a future "promote-from-archive" command) can restore
//!   them; nothing is permanently deleted.
//!
//! v2 (Bundle 26-B2, deferred): LLM-driven merge of similar skills
//! into a canonical `_distilled/<slug>/SKILL.md`. Requires an
//! embedding-based clustering step + a merge prompt. Designed for
//! later when we have more telemetry to validate the clustering
//! heuristic.

use std::fs;
use std::path::{Path, PathBuf};

use crate::proactive::skill_telemetry::{load_meta, SkillMeta};

/// One auto-extracted skill discovered on disk.
#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    /// Filesystem slug (directory name under `_auto_extracted/`).
    pub slug: String,
    /// Full path to the skill directory.
    pub dir: PathBuf,
    /// Loaded `meta.json` — None if the file is missing/corrupt.
    pub meta: Option<SkillMeta>,
}

impl DiscoveredSkill {
    /// True iff the skill has never been returned by `skill_search`
    /// AND has been on disk long enough to confidently call it
    /// "noise" (per the caller's `min_unused_days` threshold).
    pub fn is_stale(&self, now_ms: i64, min_unused_days: f64) -> bool {
        let meta = match &self.meta {
            Some(m) => m,
            // Missing meta.json — treat as fresh (recently extracted,
            // not yet measured). The next pass will catch it.
            None => return false,
        };
        if meta.returned_count > 0 {
            return false;
        }
        // Use `created_at` as the age signal since the skill has
        // never been returned (`last_returned_at` is None).
        let age_ms = (now_ms - meta.created_at).max(0) as f64;
        let age_days = age_ms / (1000.0 * 60.0 * 60.0 * 24.0);
        age_days >= min_unused_days
    }
}

/// Scan `<data_dir>/skills/_auto_extracted/` for all skill
/// directories. Each subdir must contain `SKILL.md`; meta.json is
/// loaded best-effort.
pub fn scan_auto_extracted(data_dir: &Path) -> std::io::Result<Vec<DiscoveredSkill>> {
    let root = data_dir.join("skills").join("_auto_extracted");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").exists() {
            // Not a real skill dir — skip.
            continue;
        }
        let slug = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if slug.is_empty() {
            continue;
        }
        let meta = match load_meta(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(
                    dir = %path.display(),
                    error = %e,
                    "[skill_distillation] meta.json unreadable — continuing without"
                );
                None
            }
        };
        out.push(DiscoveredSkill {
            slug,
            dir: path,
            meta,
        });
    }
    Ok(out)
}

/// Archive a single skill: move its directory under
/// `_archive/<YYYYMMDD-HHMM>/<slug>/`. Reversible.
pub fn archive_skill(
    data_dir: &Path,
    skill: &DiscoveredSkill,
    timestamp_prefix: &str,
) -> std::io::Result<PathBuf> {
    let archive_root = data_dir
        .join("skills")
        .join("_archive")
        .join(timestamp_prefix);
    fs::create_dir_all(&archive_root)?;
    let dest = archive_root.join(&skill.slug);
    if dest.exists() {
        // Should be rare (different sessions could collide on the
        // same minute). Append a counter to break the tie.
        let mut counter = 1;
        loop {
            let alt = archive_root.join(format!("{}.{}", skill.slug, counter));
            if !alt.exists() {
                fs::rename(&skill.dir, &alt)?;
                return Ok(alt);
            }
            counter += 1;
            if counter > 1000 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("archive: cannot find a free slot for {}", skill.slug),
                ));
            }
        }
    } else {
        fs::rename(&skill.dir, &dest)?;
        Ok(dest)
    }
}

/// Result of one distillation pass — for telemetry/logging.
#[derive(Debug, Clone, Default)]
pub struct DistillationReport {
    pub scanned: usize,
    pub archived: Vec<String>, // slugs
    pub skipped_kept: usize,
    pub errors: Vec<String>,
}

/// Bundle 26-D — promotion candidate. A skill that has crossed the
/// "useful enough to become a Gene" threshold.
#[derive(Debug, Clone)]
pub struct PromotionCandidate {
    pub slug: String,
    pub dir: PathBuf,
    pub returned_count: u32,
    /// Full SKILL.md body (frontmatter + content), capped at
    /// `max_content_chars` so we don't dump 10KB into the
    /// gene_candidate_pool.
    pub skill_md_excerpt: String,
}

/// True iff a skill is ready to be pushed to the GEP
/// `gene_candidate_pool` for distillation into a Gene.
///
/// v1 criterion: `returned_count >= min_returned_count`
///                AND `promoted_at.is_none()`.
///
/// v2 (when Bundle 26-A2 wires outcome tracking) will add
/// `success_rate() >= 0.7 && observed_outcomes >= 3` as a second
/// gate. For now, "LLM repeatedly found this skill relevant" is a
/// good-enough proxy: it shows the skill matched real query patterns
/// at least N times.
pub fn is_promotion_eligible(skill: &DiscoveredSkill, min_returned_count: u32) -> bool {
    let meta = match &skill.meta {
        Some(m) => m,
        None => return false,
    };
    if meta.promoted_at.is_some() {
        return false; // already promoted; don't re-inject
    }
    meta.returned_count >= min_returned_count
}

/// Find all skills ready for promotion. Reads SKILL.md content (capped).
pub fn find_promotion_candidates(
    data_dir: &Path,
    min_returned_count: u32,
    max_content_chars: usize,
) -> std::io::Result<Vec<PromotionCandidate>> {
    let skills = scan_auto_extracted(data_dir)?;
    let mut out = Vec::new();
    for skill in skills {
        if !is_promotion_eligible(&skill, min_returned_count) {
            continue;
        }
        let returned_count = skill.meta.as_ref().map(|m| m.returned_count).unwrap_or(0);
        let md_path = skill.dir.join("SKILL.md");
        let content = match fs::read_to_string(&md_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    path = %md_path.display(),
                    error = %e,
                    "[Bundle 26-D] cannot read SKILL.md for promotion candidate"
                );
                continue;
            }
        };
        let excerpt = if content.chars().count() > max_content_chars {
            let mut head: String = content.chars().take(max_content_chars).collect();
            head.push_str("\n\n[…truncated for gene candidate pool…]");
            head
        } else {
            content
        };
        out.push(PromotionCandidate {
            slug: skill.slug,
            dir: skill.dir,
            returned_count,
            skill_md_excerpt: excerpt,
        });
    }
    Ok(out)
}

/// Stamp `promoted_at = now_ms` on the skill's meta.json so we don't
/// double-promote. Best-effort: on error, the candidate would be
/// re-promoted next tick, which is mildly wasteful but not incorrect
/// (the dedup logic in `inject_candidate` catches the duplicate).
pub fn mark_promoted(skill_dir: &Path, now_ms: i64) -> std::io::Result<()> {
    let mut meta = crate::proactive::skill_telemetry::load_or_init(
        skill_dir,
        skill_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
        now_ms,
    );
    meta.promoted_at = Some(now_ms);
    meta.updated_at = now_ms;
    crate::proactive::skill_telemetry::save_meta(skill_dir, &meta)
}

/// Run a single distillation pass. v1 = prune-only.
///
/// `now_ms` is exposed (rather than `chrono::Utc::now()` inline) so
/// tests can drive the time deterministically.
pub fn run_prune_pass(
    data_dir: &Path,
    now_ms: i64,
    min_unused_days: f64,
) -> std::io::Result<DistillationReport> {
    let skills = scan_auto_extracted(data_dir)?;
    let mut report = DistillationReport::default();
    report.scanned = skills.len();

    if skills.is_empty() {
        return Ok(report);
    }

    let timestamp_prefix = {
        let secs = (now_ms / 1000) as i64;
        // Build YYYYMMDD-HHMM from now_ms — using chrono since it's
        // already a project dep.
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
            .unwrap_or_else(chrono::Utc::now);
        dt.format("%Y%m%d-%H%M").to_string()
    };

    for skill in &skills {
        if !skill.is_stale(now_ms, min_unused_days) {
            report.skipped_kept += 1;
            continue;
        }
        match archive_skill(data_dir, skill, &timestamp_prefix) {
            Ok(_dest) => {
                tracing::info!(
                    slug = %skill.slug,
                    age_days = ?skill.meta.as_ref().map(|m| {
                        let age_ms = (now_ms - m.created_at).max(0) as f64;
                        age_ms / (1000.0 * 60.0 * 60.0 * 24.0)
                    }),
                    "[Bundle 26-B] archived stale auto-extracted skill"
                );
                report.archived.push(skill.slug.clone());
            }
            Err(e) => {
                tracing::warn!(
                    slug = %skill.slug,
                    error = %e,
                    "[Bundle 26-B] failed to archive skill"
                );
                report.errors.push(format!("{}: {}", skill.slug, e));
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proactive::skill_telemetry::{save_meta, SkillMeta};
    use tempfile::TempDir;

    fn day_ms() -> i64 {
        1000 * 60 * 60 * 24
    }

    fn make_skill(data_dir: &Path, slug: &str, meta: Option<SkillMeta>) {
        let dir = data_dir
            .join("skills")
            .join("_auto_extracted")
            .join(slug);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "# placeholder\n").unwrap();
        if let Some(m) = meta {
            save_meta(&dir, &m).unwrap();
        }
    }

    #[test]
    fn scan_empty_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let skills = scan_auto_extracted(tmp.path()).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_picks_up_dirs_with_skill_md() {
        let tmp = TempDir::new().unwrap();
        make_skill(tmp.path(), "foo", None);
        make_skill(tmp.path(), "bar", None);
        // Not a skill (no SKILL.md) — should be ignored
        fs::create_dir_all(
            tmp.path()
                .join("skills")
                .join("_auto_extracted")
                .join("baz"),
        )
        .unwrap();
        let skills = scan_auto_extracted(tmp.path()).unwrap();
        let slugs: Vec<_> = skills.iter().map(|s| s.slug.clone()).collect();
        assert_eq!(slugs.len(), 2);
        assert!(slugs.contains(&"foo".to_string()));
        assert!(slugs.contains(&"bar".to_string()));
    }

    #[test]
    fn is_stale_requires_zero_returns_and_age() {
        let now = 1_779_000_000_000_i64;
        let make = |returned: u32, created_offset_ms: i64| {
            let mut m = SkillMeta::new_at("s", now + created_offset_ms);
            m.returned_count = returned;
            DiscoveredSkill {
                slug: "s".into(),
                dir: PathBuf::from("/tmp/s"),
                meta: Some(m),
            }
        };

        // Used at least once → NOT stale.
        let used = make(1, -10 * day_ms());
        assert!(!used.is_stale(now, 7.0));

        // Never used, but only 3 days old (< 7 day threshold) → keep.
        let young = make(0, -3 * day_ms());
        assert!(!young.is_stale(now, 7.0));

        // Never used, 10 days old → stale.
        let old = make(0, -10 * day_ms());
        assert!(old.is_stale(now, 7.0));

        // No meta → not stale (don't have data to judge).
        let no_meta = DiscoveredSkill {
            slug: "s".into(),
            dir: PathBuf::from("/tmp/s"),
            meta: None,
        };
        assert!(!no_meta.is_stale(now, 7.0));
    }

    #[test]
    fn archive_moves_dir_under_archive_subdir() {
        let tmp = TempDir::new().unwrap();
        let mut m = SkillMeta::new_at("foo", 0);
        m.returned_count = 0;
        make_skill(tmp.path(), "foo", Some(m));
        let skill = scan_auto_extracted(tmp.path()).unwrap().pop().unwrap();
        let dest = archive_skill(tmp.path(), &skill, "20260522-0500").unwrap();
        assert!(dest.exists());
        assert!(!skill.dir.exists());
        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("meta.json").exists());
    }

    #[test]
    fn is_promotion_eligible_basic() {
        let now = 1_779_000_000_000_i64;
        let make = |returned: u32, promoted: Option<i64>| {
            let mut m = SkillMeta::new_at("s", now);
            m.returned_count = returned;
            m.promoted_at = promoted;
            DiscoveredSkill {
                slug: "s".into(),
                dir: PathBuf::from("/tmp/s"),
                meta: Some(m),
            }
        };
        // below threshold → not eligible
        assert!(!is_promotion_eligible(&make(2, None), 3));
        // at threshold + not promoted → eligible
        assert!(is_promotion_eligible(&make(3, None), 3));
        // above threshold + not promoted → eligible
        assert!(is_promotion_eligible(&make(7, None), 3));
        // above threshold but already promoted → not eligible (idempotent)
        assert!(!is_promotion_eligible(&make(7, Some(now - 100)), 3));
        // no meta → not eligible
        let no_meta = DiscoveredSkill {
            slug: "s".into(),
            dir: PathBuf::from("/tmp/s"),
            meta: None,
        };
        assert!(!is_promotion_eligible(&no_meta, 3));
    }

    #[test]
    fn find_promotion_candidates_returns_excerpts() {
        let tmp = TempDir::new().unwrap();
        let now = 1_779_000_000_000_i64;
        // Eligible: returned_count = 5, not promoted
        let mut m_a = SkillMeta::new_at("a", now);
        m_a.returned_count = 5;
        make_skill(tmp.path(), "alpha", Some(m_a));
        // Not eligible: only 1 returned
        let mut m_b = SkillMeta::new_at("b", now);
        m_b.returned_count = 1;
        make_skill(tmp.path(), "beta", Some(m_b));
        // Not eligible: already promoted
        let mut m_c = SkillMeta::new_at("c", now);
        m_c.returned_count = 10;
        m_c.promoted_at = Some(now - 1000);
        make_skill(tmp.path(), "gamma", Some(m_c));

        let cands = find_promotion_candidates(tmp.path(), 3, 1000).unwrap();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].slug, "alpha");
        assert_eq!(cands[0].returned_count, 5);
        assert!(cands[0].skill_md_excerpt.contains("placeholder"));
    }

    #[test]
    fn mark_promoted_sets_field_idempotent() {
        let tmp = TempDir::new().unwrap();
        let now = 1_779_000_000_000_i64;
        let mut m = SkillMeta::new_at("p", now - 1000);
        m.returned_count = 5;
        make_skill(tmp.path(), "promotable", Some(m));
        let dir = tmp
            .path()
            .join("skills")
            .join("_auto_extracted")
            .join("promotable");
        mark_promoted(&dir, now).unwrap();
        let loaded = crate::proactive::skill_telemetry::load_meta(&dir)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.promoted_at, Some(now));
        // second call overwrites the timestamp — also OK (idempotent in intent).
        mark_promoted(&dir, now + 100).unwrap();
        let again = crate::proactive::skill_telemetry::load_meta(&dir)
            .unwrap()
            .unwrap();
        assert_eq!(again.promoted_at, Some(now + 100));
    }

    #[test]
    fn run_prune_pass_archives_only_stale() {
        let tmp = TempDir::new().unwrap();
        let now = 1_779_000_000_000_i64;
        // Skill A — old + never used → archive
        let mut m_a = SkillMeta::new_at("a", now - 30 * day_ms());
        m_a.returned_count = 0;
        make_skill(tmp.path(), "a-old-noise", Some(m_a));
        // Skill B — old but used → keep
        let mut m_b = SkillMeta::new_at("b", now - 30 * day_ms());
        m_b.returned_count = 3;
        make_skill(tmp.path(), "b-old-but-used", Some(m_b));
        // Skill C — fresh → keep
        let mut m_c = SkillMeta::new_at("c", now - 1 * day_ms());
        m_c.returned_count = 0;
        make_skill(tmp.path(), "c-fresh", Some(m_c));

        let report = run_prune_pass(tmp.path(), now, 7.0).unwrap();
        assert_eq!(report.scanned, 3);
        assert_eq!(report.archived, vec!["a-old-noise".to_string()]);
        assert_eq!(report.skipped_kept, 2);
        assert!(report.errors.is_empty());

        // Verify on disk: a-old-noise gone, b/c still there.
        let auto = tmp.path().join("skills").join("_auto_extracted");
        assert!(!auto.join("a-old-noise").exists());
        assert!(auto.join("b-old-but-used").exists());
        assert!(auto.join("c-fresh").exists());
        // Archive dir exists with a-old-noise inside.
        let archive = tmp.path().join("skills").join("_archive");
        assert!(archive.exists());
    }
}
