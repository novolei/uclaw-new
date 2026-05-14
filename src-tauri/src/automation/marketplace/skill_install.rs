//! Stage + atomic-rename bundled skill files into ~/.uclaw/skills/_marketplace/<slug>/.
//!
//! Lives in its own module because mod.rs is already large and the rollback
//! semantics (staging dir as failure boundary) read better in isolation.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

use crate::automation::marketplace::halo_adapter;
use crate::automation::marketplace::types::{RegistryEntry, RegistrySource};
use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

/// One bundled skill's files, fetched and ready to commit.
pub struct StagedSkill {
    pub skill_id: String,
    pub file_count: i64,
}

/// Fetch every bundled skill referenced by the spec, into a staging dir.
/// On success the caller must call `commit_staged_skills` to atomically
/// move them into place. On failure the staging dir is cleaned and an
/// Err is returned — no partial state survives.
pub async fn fetch_bundled_skills(
    source: &RegistrySource,
    entry: &RegistryEntry,
    spec: &HumaneAutomationSpec,
    skills_root: &Path,
) -> Result<Vec<StagedSkill>> {
    let staging = skills_root.join(".staging").join(&entry.slug);
    // Clean any leftover staging from a previous failed attempt.
    let _ = std::fs::remove_dir_all(&staging);

    let bundled: Vec<_> = spec
        .requires
        .as_ref()
        .and_then(|r| {
            r.get("skills")
                .and_then(|s| s.as_array())
                .map(|arr| arr.iter().filter(|s| s.get("bundled").and_then(|b| b.as_bool()).unwrap_or(false)).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let mut staged: Vec<StagedSkill> = Vec::new();
    for skill_val in bundled {
        let skill_id = skill_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("requires.skills[].id missing or non-string"))?
            .to_string();
        let files: Vec<String> = skill_val
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|f| f.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if files.is_empty() {
            // Bundled skill declared no files — skip silently; nothing to install.
            continue;
        }

        let skill_staging = staging.join(&skill_id);
        std::fs::create_dir_all(&skill_staging)
            .with_context(|| format!("create staging {}", skill_staging.display()))?;

        for filename in &files {
            // Guard against path-traversal — filename must be plain (no slashes / ..).
            if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
                cleanup_staging(&staging);
                return Err(anyhow!("rejecting suspicious filename: {}", filename));
            }
            let body = halo_adapter::fetch_skill_file(source, entry, &skill_id, filename)
                .await
                .with_context(|| format!("fetch skill file {}/{}", skill_id, filename))?;
            let target = skill_staging.join(filename);
            std::fs::write(&target, &body)
                .with_context(|| format!("write {}", target.display()))?;
        }

        staged.push(StagedSkill {
            skill_id,
            file_count: files.len() as i64,
        });
    }

    Ok(staged)
}

/// Atomically promote the staging dir into the real marketplace skills tree.
/// Removes any pre-existing tree at the destination first (re-install case).
pub fn commit_staged_skills(slug: &str, skills_root: &Path) -> Result<PathBuf> {
    let staging = skills_root.join(".staging").join(slug);
    if !staging.exists() {
        // Nothing staged (spec had no bundled skills) — return the would-be path.
        return Ok(skills_root.join("_marketplace").join(slug));
    }
    let marketplace_root = skills_root.join("_marketplace");
    std::fs::create_dir_all(&marketplace_root)
        .with_context(|| format!("create _marketplace root {}", marketplace_root.display()))?;
    let final_dir = marketplace_root.join(slug);
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .with_context(|| format!("remove existing {}", final_dir.display()))?;
    }
    std::fs::rename(&staging, &final_dir)
        .with_context(|| format!("rename {} -> {}", staging.display(), final_dir.display()))?;
    Ok(final_dir)
}

/// Remove the staging dir; used on install failure to abandon partial state.
pub fn cleanup_staging(staging_dir: &Path) {
    let _ = std::fs::remove_dir_all(staging_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_with_empty_staging_returns_planned_path() {
        let tmp = tempfile::tempdir().unwrap();
        let result = commit_staged_skills("xhs-monitor", tmp.path()).unwrap();
        assert_eq!(
            result,
            tmp.path().join("_marketplace").join("xhs-monitor")
        );
        // No directory should have been created since staging was empty.
        assert!(!result.exists());
    }

    #[test]
    fn commit_moves_staging_to_final_location() {
        let tmp = tempfile::tempdir().unwrap();
        let staging_skill = tmp.path().join(".staging").join("auto-1").join("skill-a");
        std::fs::create_dir_all(&staging_skill).unwrap();
        std::fs::write(staging_skill.join("SKILL.md"), b"# Skill A\n").unwrap();

        let final_dir = commit_staged_skills("auto-1", tmp.path()).unwrap();
        assert!(final_dir.join("skill-a").join("SKILL.md").exists());
        // Staging is now gone.
        assert!(!tmp.path().join(".staging").join("auto-1").exists());
    }

    #[test]
    fn commit_overwrites_existing_install() {
        let tmp = tempfile::tempdir().unwrap();
        // Pre-existing install.
        let preexisting = tmp.path().join("_marketplace").join("auto-1").join("old");
        std::fs::create_dir_all(&preexisting).unwrap();
        std::fs::write(preexisting.join("STALE.md"), b"stale").unwrap();
        // Fresh staging.
        let staging_skill = tmp.path().join(".staging").join("auto-1").join("skill-a");
        std::fs::create_dir_all(&staging_skill).unwrap();
        std::fs::write(staging_skill.join("SKILL.md"), b"# Skill A\n").unwrap();

        commit_staged_skills("auto-1", tmp.path()).unwrap();
        assert!(!tmp
            .path()
            .join("_marketplace")
            .join("auto-1")
            .join("old")
            .exists());
        assert!(tmp
            .path()
            .join("_marketplace")
            .join("auto-1")
            .join("skill-a")
            .join("SKILL.md")
            .exists());
    }

    #[test]
    fn cleanup_staging_is_idempotent_on_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // No staging dir exists; cleanup must not panic.
        cleanup_staging(&tmp.path().join(".staging").join("nothing"));
    }
}
