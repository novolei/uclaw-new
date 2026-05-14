export interface SkillDiff {
  /** skill_ids in the new version but not the installed one. */
  added: string[]
  /** skill_ids in the installed version but not the new one. */
  removed: string[]
  /** skill_ids present in both. */
  kept: string[]
}

/**
 * Diff two sets of bundled-skill ids. Pure — both inputs come from data the
 * frontend already has: `installedSkillIds` from
 * `InstalledAutomation.bundledSkills[].skillId`, `newSkillIds` from the new
 * spec's `requires.skills[]` (filtered to `bundled === true`).
 */
export function diffBundledSkills(
  installedSkillIds: string[],
  newSkillIds: string[],
): SkillDiff {
  const installed = new Set(installedSkillIds)
  const next = new Set(newSkillIds)
  return {
    added: newSkillIds.filter((id) => !installed.has(id)),
    removed: installedSkillIds.filter((id) => !next.has(id)),
    kept: newSkillIds.filter((id) => installed.has(id)),
  }
}
