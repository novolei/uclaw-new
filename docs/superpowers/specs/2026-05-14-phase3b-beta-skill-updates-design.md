# Phase 3b-β — Skill Bundle Updates — Design

**Status:** Draft → ready to plan
**Author:** Claude (brainstormed with Ryan, 2026-05-14)
**Date:** 2026-05-14
**Scope:** Phase 3b second slice. Depends on 3b-α (merged, PR #160). γ / δ stay deferred — see § 8.

---

## 1. Problem

3b-α made automation install actually deliver working bundled skills, but it left **no upgrade path**:

- `check_updates_cached` ([marketplace/mod.rs:168](../../../src-tauri/src/automation/marketplace/mod.rs)) detects version drift between an installed automation and the registry, but the UI has no "upgrade" affordance — the drift just surfaces as a passive "有更新" badge.
- The only way to get the new version today is to uninstall + reinstall, which is clumsy and loses the "what changed" context.
- **Stale-row bug:** `install_human`'s `registering_skills` phase writes `automation_installed_skills` rows with `INSERT OR REPLACE`. If a new automation version *drops* a bundled skill, the old row for that skill is never deleted — `INSERT OR REPLACE` only touches rows with a matching `(automation_slug, skill_id)` PK. The skill *files* are correctly pruned (`commit_staged_skills` does `rm -rf _marketplace/<slug>/` before the atomic rename), but the DB row lingers, so AppsTab would show a phantom skill that no longer exists on disk.
- **Missing descriptions:** `InstalledSkillBrief.description` is hardcoded `None` at [marketplace/mod.rs:309](../../../src-tauri/src/automation/marketplace/mod.rs) with the comment `// Phase 3b-β reads SKILL.md`. AppsTab shows skill IDs with no human-readable description.

## 2. Goals

1. **Working upgrade path.** A user with an out-of-date automation can upgrade it in-place, atomically, with rollback safety (a failed upgrade leaves the old version intact).
2. **Change preview.** Before committing to an upgrade, the user sees what will change — version bump + which bundled skills get added / removed / kept.
3. **No stale state.** After any upgrade, `automation_installed_skills` exactly reflects what's on disk.
4. **Readable skill list.** AppsTab shows each bundled skill's description, pulled from its `SKILL.md`.

## 3. Non-goals (stay deferred)

- **Smart partial-file diff.** Bundled skills are KB-scale text files (`SKILL.md` + a small `index.js`). Re-downloading the whole set on upgrade costs nothing meaningful. A diff-and-fetch-only-changed-files strategy would fight the staging + atomic-rename pattern (which is all-or-nothing by design) for no real benefit. YAGNI.
- **User-modified-file preservation.** `~/.uclaw/skills/_marketplace/<slug>/` is a *managed* namespace. A user who wants to customise a marketplace skill forks it into the flat `~/.uclaw/skills/<name>/` tree (`SkillProvenance::User`), which upgrades never touch. Detecting + preserving in-place edits to `_marketplace/` files would need a checksum table and a conflict-resolution dialog — out of proportion to how rarely users edit managed files.
- **Per-skill version numbers.** DHP specs don't carry them. `requires.skills[].files` is just a filename list; the automation's `version` is the only version signal. So "upgrade" is always whole-automation, never per-skill.
- **A dedicated `upgrade_marketplace_human` Tauri command.** Upgrade *is* a destructive reinstall — `install_human` already does fetch → stage → `DELETE` old spec row → `rm -rf` old files → commit → re-insert, in that order, which is exactly a correct in-place upgrade. The only backend gap is the stale-row bug (§ 4.1). The UI calls the existing `install_marketplace_human` bridge; only the button label differs.

## 4. Design

### 4.1 Backend — fix the stale-row bug

**File:** `src-tauri/src/automation/marketplace/mod.rs`, the `registering_skills` phase inside `install_human`.

Today the phase does:

```rust
{
    let conn = runtime.db.lock().unwrap();
    for s in &staged {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO automation_installed_skills (...) VALUES (...)",
            ...,
        ) { tracing::error!(...) }
    }
}
```

Add a single `DELETE` immediately before the loop, inside the same lock scope:

```rust
{
    let conn = runtime.db.lock().unwrap();
    // Clear prior rows first — an upgrade that drops a bundled skill must not
    // leave an orphan row (INSERT OR REPLACE only touches matching-PK rows).
    if let Err(e) = conn.execute(
        "DELETE FROM automation_installed_skills WHERE automation_slug = ?1",
        rusqlite::params![slug],
    ) {
        tracing::error!(slug = %slug, error = %e, "failed to clear prior installed-skill rows");
    }
    for s in &staged {
        // ... unchanged INSERT loop
    }
}
```

`DELETE`-then-`INSERT` inside one lock is effectively atomic for our single-writer SQLite connection. Best-effort error handling matches the existing rows (V22 table is diagnostic-only — § 4.1 of the 3b-α design established this).

### 4.2 Backend — populate `InstalledSkillBrief.description`

**File:** `src-tauri/src/automation/marketplace/mod.rs`, `list_installed_inner`.

Today each `InstalledSkillBrief` is built with `description: None`. Change it to a lazy read of the skill's `SKILL.md` frontmatter:

```rust
let install_path = skills_root.join("_marketplace").join(&slug).join(&skill_id);
let description = read_skill_description(&install_path);
bundled_skills.push(InstalledSkillBrief {
    skill_id,
    description,
    install_path: install_path.to_string_lossy().to_string(),
    file_count,
});
```

`read_skill_description(dir: &Path) -> Option<String>`:
- Reads `<dir>/SKILL.md`. If missing → `None`.
- Parses the YAML frontmatter, returns the `description` field.
- Reuse the existing frontmatter parser from `skills.rs`. The research found `parse_skill_md` there; if its signature is heavyweight (returns a full `LoadedSkill`), prefer its lighter internal frontmatter helper, or a minimal local parse: split on the leading `---` fence, `serde_yml::from_str` into a `{ description: Option<String> }` struct. Whatever is least invasive — the plan task will confirm the exact reuse path by reading `skills.rs`.
- Any parse error → `None` (a bad SKILL.md must not break the whole AppsTab list).

This is a few small file reads per AppsTab load — cheap, no caching needed, no schema column, no migration.

### 4.3 Frontend — skill-diff pure function

**File:** `ui/src/lib/skill-diff.ts` (new).

```ts
export interface SkillDiff {
  added: string[]    // skill_ids in the new version but not the installed one
  removed: string[]  // skill_ids in the installed version but not the new one
  kept: string[]     // skill_ids in both
}

/** Diff two sets of bundled-skill ids. Pure — both inputs come from data
 *  the frontend already has (installed list + getMarketplaceDetail). */
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
```

The `newSkillIds` come from the new spec's `requires.skills[]` (filter `bundled === true`, map `.id`), which is in `MarketplaceDetail.parsedSpecJson`. The `installedSkillIds` come from `InstalledAutomation.bundledSkills[].skillId`. **No new backend command** — both data sources already exist.

### 4.4 Frontend — `UpgradeModal`

**File:** `ui/src/components/automation/UpgradeModal.tsx` (new).

Props: `{ slug: string; currentVersion: string; onClose: () => void; onUpgraded: () => void }`.

On mount: `getMarketplaceDetail(slug)` to fetch the new spec + version. Compute the skill diff against the installed automation (passed in or re-fetched via `listInstalledMarketplaceAutomations`).

Renders:
- Header: automation name + `currentVersion → newVersion`.
- **Skills section:**
  - `added` — green rows, `+ <skill_id>`
  - `removed` — muted + strikethrough, `− <skill_id>`
  - `kept` — neutral rows, `<skill_id>` (collapsed/dimmed; the diff is the point, not the unchanged set)
- **Capabilities section** (if `requires.mcps` differs) — same add/remove treatment, routed through the existing capability-status display.
- Footer: `取消` + `升级到 vX` (primary). Confirm → `installMarketplaceHuman(slug)` with a progress channel, show progress, on `complete` call `onUpgraded()` (which re-fetches the installed list) + close.

Visual: matches uClaw Design DNA — `rounded-xl`, theme tokens only, `motion/react` `duration: 0.22`, the same modal pattern `InstallWizard` uses (reuse its shell if there's a shared dialog component).

### 4.5 Frontend — "升级" button placement

Two surfaces, both gated on update detection (reuse the existing `marketplaceUpdatesAtom` / `check_updates` drift signal — the same signal that drives the "有更新" badge):

1. **AppsTab** card header — a `升级` button next to `卸载`, shown only when that automation's slug is in the updates set.
2. **StoreDetail** header — when viewing an item that is installed *and* has an update, the primary action becomes `升级到 vX` instead of `已安装`.

Both buttons open the same `UpgradeModal`.

## 5. Error handling

| Failure | Behaviour |
|---|---|
| Upgrade fetch fails (network / 404) | `install_human` already rolls back staging; the old version stays fully intact (old spec row + old files untouched until after a successful fetch). `UpgradeModal` surfaces the error toast, stays open. |
| `SKILL.md` missing or malformed during `list_installed_inner` | `read_skill_description` returns `None`; the skill row renders without a description. Never breaks the list. |
| Stale-row `DELETE` fails | Logged best-effort (matches the existing INSERT handling); worst case a phantom row lingers — same severity as the bug we're fixing, not worse. |
| `getMarketplaceDetail` fails in `UpgradeModal` | Modal shows an error state with a retry; no upgrade is attempted. |

## 6. Tests

### 6.1 Rust (inline `#[cfg(test)]`)

- `registering_skills_clears_stale_rows` — seed `automation_installed_skills` with skills `{A, B}` for a slug; run the registering_skills logic with a `staged` list of only `{A}`; assert `B`'s row is gone and `A`'s remains. (Drive the DELETE+INSERT block directly or via a small extracted helper — avoid needing a live HTTP server.)
- `read_skill_description_parses_frontmatter` — write a temp `SKILL.md` with `description: foo` frontmatter; assert `read_skill_description` returns `Some("foo")`. Plus: missing file → `None`; frontmatter without `description` → `None`; malformed frontmatter → `None`.

### 6.2 Vitest

- `skill-diff.test.ts` — table-driven `diffBundledSkills`: all-added, all-removed, all-kept, mixed, empty-both.
- `UpgradeModal.test.tsx` — renders version bump + skill diff from mocked `getMarketplaceDetail`; confirm button calls `installMarketplaceHuman` with the slug; cancel closes without calling the bridge.

## 7. Migration impact

**None.** 3b-β touches no schema. The migration-number budget (next free is V24 per the CLAUDE.md registry, with the V23/V23a split noted) is preserved entirely for γ / δ.

## 8. Phase 3b roadmap (context, not in scope here)

| Sub-slice | Status |
|---|---|
| 3b-α | merged (PR #160) — bundled skill install + capability map + AppsTab |
| **3b-β (THIS PR)** | skill bundle updates — upgrade path + change preview + stale-row fix + descriptions |
| 3b-γ | standalone skill / MCP marketplace entries — non-bundled install branch |
| 3b-δ | multi-registry + capability map → DB table (depends on γ) |
| 3b-ε | proxy adapters (Smithery / official MCP Registry / SkillHub) |
| 3b-ζ | local hello-halo workspace as a registry source |

## 9. Done criteria

- [ ] Installing automation with skills `{A,B}` then "upgrading" to a version with only `{A}` leaves exactly one `automation_installed_skills` row (`A`), and `_marketplace/<slug>/B/` is gone.
- [ ] AppsTab shows each bundled skill's description (from `SKILL.md`), or nothing if the skill has no description — never an error.
- [ ] An automation with a detected update shows a `升级` button in AppsTab and a `升级到 vX` action in StoreDetail.
- [ ] Clicking it opens `UpgradeModal` showing `vX → vY` + the skill diff; confirming runs the upgrade with a progress bar; a failed upgrade leaves the old version working.
- [ ] All Rust + Vitest tests pass.
- [ ] No migration added; CLAUDE.md migration registry untouched (except flipping V22's status to `merged` if it still says "this PR" — a drive-by correction).
