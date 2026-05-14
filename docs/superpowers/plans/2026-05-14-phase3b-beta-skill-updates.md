# Phase 3b-β — Skill Bundle Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give marketplace automations a working in-place upgrade path with a pre-upgrade change preview, fix the stale-row bug that an upgrade-that-drops-a-skill would leave behind, and populate bundled-skill descriptions in AppsTab.

**Architecture:** Upgrade *is* a destructive reinstall — the existing `install_human` already does fetch → stage → DELETE old spec → `rm -rf` old files → commit → re-insert in a rollback-safe order. Backend only needs a one-line stale-row `DELETE` fix plus a lazy `SKILL.md` description read. Frontend gets a pure skill-diff function, an `UpgradeModal` that previews the version bump + skill diff, and "升级" triggers in AppsTab + StoreDetail gated on the existing `marketplaceUpdatesAtom` drift signal. No schema change, no migration.

**Tech Stack:** Rust (rusqlite, serde_yml), React 18 + TypeScript + Jotai, Vitest.

**Spec:** `docs/superpowers/specs/2026-05-14-phase3b-beta-skill-updates-design.md`

**Pre-flight state confirmed against the live tree (2026-05-14, worktree `worktree-phase3b-beta-skill-updates`):**
- `registering_skills` phase: `src-tauri/src/automation/marketplace/mod.rs` ~lines 580-614. The `INSERT OR REPLACE` loop is at ~line 600, inside a `{ let conn = runtime.db.lock().unwrap(); ... }` scope.
- `list_installed_inner` builds `InstalledSkillBrief` with `description: None` at `src-tauri/src/automation/marketplace/mod.rs:309`.
- `parse_skill_md` in `src-tauri/src/skills.rs:263` is **too strict** for our use (validates skill name, enforces activation limits, requires a full `SkillManifest`). We write a minimal forgiving local `read_skill_description` instead.
- `MarketplaceUpdate` = `{ slug, installedVersion, latestVersion }` (`ui/src/lib/tauri-bridge.ts:1364`). `checkMarketplaceUpdates()` → `Promise<MarketplaceUpdate[]>`, stored in `marketplaceUpdatesAtom` (`ui/src/atoms/marketplace.ts:65`).
- Modal pattern: `Dialog / DialogContent / DialogHeader / DialogTitle / DialogDescription` from `@/components/ui/dialog` + `Button` from `@/components/ui/button` (see `ui/src/components/automation/EscalationModal.tsx` for the in-repo usage).
- `InstallWizard.tsx` is the existing install-with-progress-channel reference — `UpgradeModal` reuses the same `installMarketplaceHuman` bridge + progress-channel pattern.
- AppsTab uninstall button is at `ui/src/components/automation/AppsTab.tsx` ~line 99, a sibling of the expand button.

---

### Task 1: Backend — stale-row `DELETE` fix in `registering_skills`

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (the `registering_skills` phase, ~line 596-614)
- Test: `src-tauri/src/automation/marketplace/mod.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `mod.rs`. The test drives the DELETE-then-INSERT logic directly against an in-memory DB (no HTTP server needed):

```rust
#[test]
fn registering_skills_clears_stale_rows() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();

    // Simulate a prior install with skills {A, B}.
    conn.execute(
        "INSERT INTO automation_installed_skills VALUES ('auto-x', 'skill-a', 0, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO automation_installed_skills VALUES ('auto-x', 'skill-b', 0, 1)",
        [],
    ).unwrap();

    // Simulate an upgrade whose staged set is only {A}: the registering_skills
    // logic must DELETE all prior rows for the slug, then re-insert just {A}.
    super::write_installed_skill_rows(&conn, "auto-x", &[("skill-a".to_string(), 2_i64)], 1715000000);

    let rows: Vec<(String, i64)> = conn
        .prepare("SELECT skill_id, file_count FROM automation_installed_skills WHERE automation_slug = 'auto-x' ORDER BY skill_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(rows, vec![("skill-a".to_string(), 2_i64)], "stale skill-b row must be gone, skill-a refreshed");
}
```

This requires extracting the DELETE+INSERT block into a testable helper `write_installed_skill_rows`. That extraction is Step 3.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib registering_skills_clears_stale_rows`
Expected: FAIL — `write_installed_skill_rows` does not exist.

- [ ] **Step 3: Extract the helper + add the DELETE**

In `mod.rs`, add this free function near `list_installed_inner` (it takes a `&Connection` so the test can drive it without an `AppRuntimeService`):

```rust
/// Replace all `automation_installed_skills` rows for a slug with the given set.
/// DELETE-then-INSERT inside one connection is effectively atomic for our
/// single-writer SQLite. Best-effort: the V22 table is diagnostic-only, so a
/// failure here is logged but never rolls back the already-committed install.
fn write_installed_skill_rows(
    conn: &rusqlite::Connection,
    slug: &str,
    staged: &[(String, i64)], // (skill_id, file_count)
    now_secs: i64,
) {
    if let Err(e) = conn.execute(
        "DELETE FROM automation_installed_skills WHERE automation_slug = ?1",
        rusqlite::params![slug],
    ) {
        tracing::error!(slug = %slug, error = %e, "failed to clear prior installed-skill rows");
    }
    for (skill_id, file_count) in staged {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params![slug, skill_id, now_secs, file_count],
        ) {
            tracing::error!(
                slug = %slug,
                skill_id = %skill_id,
                error = %e,
                "failed to record installed skill — install continues, AppsTab may show stale state until reinstall"
            );
        }
    }
}
```

Then replace the existing inline block in the `registering_skills` phase (~line 596-614) — the `{ let conn = runtime.db.lock().unwrap(); for s in &staged { ... INSERT OR REPLACE ... } }` — with a call to the helper:

```rust
    {
        let conn = runtime.db.lock().unwrap();
        let rows: Vec<(String, i64)> = staged
            .iter()
            .map(|s| (s.skill_id.clone(), s.file_count))
            .collect();
        write_installed_skill_rows(&conn, slug, &rows, now_secs);
    }
```

(Keep the `now_secs` binding that already exists earlier in the phase. `staged` is the `Vec<StagedSkill>` already in scope; `StagedSkill` has `skill_id: String` and `file_count: i64` — confirm by reading `skill_install.rs`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib registering_skills_clears_stale_rows`
Expected: PASS.

Run the broader marketplace suite to confirm no regression:
`cd src-tauri && cargo test --lib marketplace 2>&1 | tail -10`
Expected: all green (the existing install/uninstall tests still pass).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs
git commit -m "fix(marketplace): clear stale installed-skill rows on (re)install

registering_skills used INSERT OR REPLACE, which only touches matching-PK
rows — an upgrade that drops a bundled skill left an orphan row pointing
at a skill that commit_staged_skills had already deleted from disk.
Extract write_installed_skill_rows: DELETE-by-slug then re-INSERT, so the
V22 table always mirrors what's on disk after any install or upgrade."
```

---

### Task 2: Backend — `read_skill_description` + populate `InstalledSkillBrief.description`

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (new helper + `list_installed_inner` at line ~309)
- Test: `src-tauri/src/automation/marketplace/mod.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn read_skill_description_parses_frontmatter() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skill-a");
    std::fs::create_dir_all(&skill_dir).unwrap();

    // Happy path: frontmatter with a description.
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: skill-a\ndescription: Collects search data\n---\n\nBody.\n",
    ).unwrap();
    assert_eq!(
        super::read_skill_description(&skill_dir),
        Some("Collects search data".to_string()),
    );

    // Frontmatter without a description field → None.
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: skill-a\n---\n\nBody.\n",
    ).unwrap();
    assert_eq!(super::read_skill_description(&skill_dir), None);

    // Malformed frontmatter (no closing fence) → None, never panics.
    std::fs::write(skill_dir.join("SKILL.md"), "---\nname: skill-a\nBody with no close").unwrap();
    assert_eq!(super::read_skill_description(&skill_dir), None);

    // Missing SKILL.md entirely → None.
    let empty_dir = tmp.path().join("skill-empty");
    std::fs::create_dir_all(&empty_dir).unwrap();
    assert_eq!(super::read_skill_description(&empty_dir), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib read_skill_description_parses_frontmatter`
Expected: FAIL — `read_skill_description` does not exist.

- [ ] **Step 3: Implement `read_skill_description`**

`parse_skill_md` in `skills.rs` is too strict (validates the skill name, enforces activation limits — a marketplace SKILL.md with an unusual name would error). Write a minimal forgiving local helper in `mod.rs`. Add it near `list_installed_inner`:

```rust
/// Best-effort read of a bundled skill's `description` from its SKILL.md
/// YAML frontmatter. Returns None on any problem — a bad SKILL.md must never
/// break the AppsTab list. Deliberately does NOT reuse skills::parse_skill_md,
/// which validates the skill name + enforces activation limits and would
/// reject otherwise-fine marketplace skills.
fn read_skill_description(skill_dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).ok()?;
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let trimmed = content.trim_start_matches(['\n', '\r']);
    let after_open = trimmed.strip_prefix("---")?;
    // Skip to the end of the opening fence line.
    let after_open_line = &after_open[after_open.find('\n')? + 1..];
    // Find the closing fence: a line that is exactly "---".
    let close = after_open_line
        .lines()
        .scan(0usize, |offset, line| {
            let here = *offset;
            *offset += line.len() + 1; // +1 for the '\n'
            Some((here, line))
        })
        .find(|(_, line)| line.trim() == "---")
        .map(|(here, _)| here)?;
    let yaml_str = &after_open_line[..close];

    #[derive(serde::Deserialize)]
    struct Frontmatter {
        description: Option<String>,
    }
    let fm: Frontmatter = serde_yml::from_str(yaml_str).ok()?;
    fm.description.filter(|d| !d.trim().is_empty())
}
```

- [ ] **Step 4: Wire it into `list_installed_inner`**

At `mod.rs:298-312` the loop builds each `InstalledSkillBrief`. It currently constructs `install_path` as a `String`. Restructure so the `PathBuf` is available for the description read:

```rust
        let mut bundled_skills: Vec<InstalledSkillBrief> = Vec::new();
        for s in skill_rows {
            let (skill_id, file_count) = s?;
            let install_dir = skills_root.join("_marketplace").join(&slug).join(&skill_id);
            let description = read_skill_description(&install_dir);
            bundled_skills.push(InstalledSkillBrief {
                skill_id,
                description,
                install_path: install_dir.to_string_lossy().to_string(),
                file_count,
            });
        }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib read_skill_description_parses_frontmatter`
Expected: PASS.

Run: `cd src-tauri && cargo test --lib marketplace 2>&1 | tail -10`
Expected: all green — the existing `list_installed_*` tests still pass (they use a `/tmp/uclaw-test` skills_root with no SKILL.md files, so `description` resolves to `None` for them, which matches their existing assertions).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): populate InstalledSkillBrief.description from SKILL.md

list_installed_inner hardcoded description: None with a // Phase 3b-beta
marker. Add a minimal forgiving read_skill_description helper — lazy YAML
frontmatter read, None on any error — and wire it in. Deliberately does
not reuse skills::parse_skill_md, which is too strict for marketplace
SKILL.md files."
```

---

### Task 3: Frontend — `skill-diff.ts` pure function

**Files:**
- Create: `ui/src/lib/skill-diff.ts`
- Create: `ui/src/lib/skill-diff.test.ts`

- [ ] **Step 1: Write the failing test**

Create `ui/src/lib/skill-diff.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { diffBundledSkills } from './skill-diff'

describe('diffBundledSkills', () => {
  it('classifies added / removed / kept', () => {
    expect(diffBundledSkills(['a', 'b'], ['b', 'c'])).toEqual({
      added: ['c'],
      removed: ['a'],
      kept: ['b'],
    })
  })

  it('all added when nothing installed', () => {
    expect(diffBundledSkills([], ['a', 'b'])).toEqual({
      added: ['a', 'b'],
      removed: [],
      kept: [],
    })
  })

  it('all removed when new version bundles nothing', () => {
    expect(diffBundledSkills(['a', 'b'], [])).toEqual({
      added: [],
      removed: ['a', 'b'],
      kept: [],
    })
  })

  it('all kept when identical', () => {
    expect(diffBundledSkills(['a', 'b'], ['a', 'b'])).toEqual({
      added: [],
      removed: [],
      kept: ['a', 'b'],
    })
  })

  it('empty both', () => {
    expect(diffBundledSkills([], [])).toEqual({ added: [], removed: [], kept: [] })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run skill-diff 2>&1 | tail -8`
Expected: FAIL — `./skill-diff` module not found.

- [ ] **Step 3: Implement**

Create `ui/src/lib/skill-diff.ts`:

```ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run skill-diff 2>&1 | tail -8`
Expected: 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/skill-diff.ts ui/src/lib/skill-diff.test.ts
git commit -m "feat(marketplace): diffBundledSkills pure function

Classifies bundled-skill ids into added / removed / kept. Both inputs
come from data the frontend already has (installed list +
getMarketplaceDetail), so no backend command is needed. Drives the
UpgradeModal change preview."
```

---

### Task 4: Frontend — `UpgradeModal` + "升级" triggers

**Files:**
- Create: `ui/src/components/automation/UpgradeModal.tsx`
- Create: `ui/src/components/automation/UpgradeModal.test.tsx`
- Modify: `ui/src/components/automation/AppsTab.tsx` (add 升级 button + modal state)
- Modify: `ui/src/components/automation/StoreDetail.tsx` (add 升级到 vX action)

- [ ] **Step 1: Write the failing component test**

Create `ui/src/components/automation/UpgradeModal.test.tsx`:

```tsx
import { describe, test, expect, vi } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  getMarketplaceDetail: vi.fn(),
  installMarketplaceHuman: vi.fn(),
}))

import { UpgradeModal } from './UpgradeModal'
import { getMarketplaceDetail, installMarketplaceHuman } from '@/lib/tauri-bridge'

const detail = {
  item: { slug: 'xhs', name: '小红书监控', version: '2.0.0', appType: 'automation' },
  specYaml: '',
  parsedSpecJson: {
    requires: {
      skills: [
        { id: 'xhs-search', bundled: true },
        { id: 'xhs-report', bundled: true },
      ],
    },
  },
  requiresMcps: [],
  requiresSkills: [],
  installedVersion: '1.0.0',
}

describe('UpgradeModal', () => {
  test('renders version bump and skill diff', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={() => {}}
        onUpgraded={() => {}}
      />,
    )
    expect(await findByText(/1\.0\.0/)).toBeInTheDocument()
    expect(await findByText(/2\.0\.0/)).toBeInTheDocument()
    // xhs-report is newly added, xhs-search is kept
    expect(await findByText('xhs-report')).toBeInTheDocument()
  })

  test('confirm calls installMarketplaceHuman with slug', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    ;(installMarketplaceHuman as ReturnType<typeof vi.fn>).mockResolvedValueOnce({})
    const onUpgraded = vi.fn()
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={() => {}}
        onUpgraded={onUpgraded}
      />,
    )
    const confirmBtn = await findByText(/升级到 v2\.0\.0/)
    fireEvent.click(confirmBtn)
    await waitFor(() => expect(installMarketplaceHuman).toHaveBeenCalledWith('xhs'))
  })

  test('cancel closes without calling the bridge', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    const onClose = vi.fn()
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={onClose}
        onUpgraded={() => {}}
      />,
    )
    fireEvent.click(await findByText('取消'))
    expect(onClose).toHaveBeenCalled()
    expect(installMarketplaceHuman).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run UpgradeModal 2>&1 | tail -8`
Expected: FAIL — `./UpgradeModal` module not found.

- [ ] **Step 3: Implement `UpgradeModal`**

First **read** `ui/src/components/automation/InstallWizard.tsx` to see the exact `installMarketplaceHuman` + progress-channel call pattern, and `ui/src/components/automation/EscalationModal.tsx` for the `Dialog` shell usage. Then create `ui/src/components/automation/UpgradeModal.tsx`:

```tsx
import * as React from 'react'
import { motion } from 'motion/react'
import { ArrowRight, Plus, Minus, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { getMarketplaceDetail, installMarketplaceHuman } from '@/lib/tauri-bridge'
import { diffBundledSkills, type SkillDiff } from '@/lib/skill-diff'

interface Props {
  slug: string
  name: string
  currentVersion: string
  installedSkillIds: string[]
  onClose: () => void
  onUpgraded: () => void
}

/** Extract bundled-skill ids from a parsed Humane spec's requires.skills[]. */
function bundledSkillIds(parsedSpecJson: unknown): string[] {
  const requires = (parsedSpecJson as { requires?: { skills?: unknown } } | null)?.requires
  const skills = Array.isArray(requires?.skills) ? requires.skills : []
  return skills
    .filter((s): s is { id: string; bundled?: boolean } =>
      typeof s === 'object' && s !== null && typeof (s as { id?: unknown }).id === 'string',
    )
    .filter((s) => s.bundled === true)
    .map((s) => s.id)
}

export function UpgradeModal({
  slug,
  name,
  currentVersion,
  installedSkillIds,
  onClose,
  onUpgraded,
}: Props): React.ReactElement {
  const [newVersion, setNewVersion] = React.useState<string | null>(null)
  const [diff, setDiff] = React.useState<SkillDiff | null>(null)
  const [loadError, setLoadError] = React.useState<string | null>(null)
  const [upgrading, setUpgrading] = React.useState(false)

  React.useEffect(() => {
    let cancelled = false
    getMarketplaceDetail(slug)
      .then((detail) => {
        if (cancelled) return
        setNewVersion(detail.item.version)
        setDiff(diffBundledSkills(installedSkillIds, bundledSkillIds(detail.parsedSpecJson)))
      })
      .catch((err) => {
        if (!cancelled) setLoadError(String(err))
      })
    return () => {
      cancelled = true
    }
  }, [slug, installedSkillIds])

  const handleUpgrade = async () => {
    setUpgrading(true)
    try {
      await installMarketplaceHuman(slug)
      toast.success(`已升级 ${name} 到 v${newVersion}`)
      onUpgraded()
      onClose()
    } catch (err) {
      toast.error(`升级失败：${String(err)}`)
      setUpgrading(false)
    }
  }

  return (
    <Dialog open onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="text-[15px]">升级 {name}</DialogTitle>
          <DialogDescription className="flex items-center gap-1.5 text-[12px] tabular-nums">
            <span className="text-muted-foreground">v{currentVersion}</span>
            <ArrowRight size={12} className="text-muted-foreground" />
            <span className="text-foreground font-medium">v{newVersion ?? '…'}</span>
          </DialogDescription>
        </DialogHeader>

        {loadError && (
          <div className="text-[12px] text-danger py-2">加载新版本信息失败：{loadError}</div>
        )}

        {diff && (
          <div className="flex flex-col gap-2 py-1">
            {diff.added.length === 0 && diff.removed.length === 0 ? (
              <div className="text-[12px] text-muted-foreground">本次升级不改变 bundled skill 集合。</div>
            ) : (
              <ul className="flex flex-col gap-1 text-[12px]">
                {diff.added.map((id) => (
                  <li key={`a-${id}`} className="flex items-center gap-1.5 text-success">
                    <Plus size={12} />
                    {id}
                  </li>
                ))}
                {diff.removed.map((id) => (
                  <li key={`r-${id}`} className="flex items-center gap-1.5 text-muted-foreground line-through">
                    <Minus size={12} />
                    {id}
                  </li>
                ))}
              </ul>
            )}
            {diff.kept.length > 0 && (
              <div className="text-[11px] text-muted-foreground/70">
                保留 {diff.kept.length} 个 skill：{diff.kept.join('、')}
              </div>
            )}
          </div>
        )}

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onClose} disabled={upgrading}>
            取消
          </Button>
          <Button onClick={handleUpgrade} disabled={upgrading || newVersion === null || loadError !== null}>
            {upgrading && <Loader2 size={13} className="animate-spin mr-1" />}
            升级到 v{newVersion ?? '…'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
```

Note on `motion` import: it's imported above to match the in-repo convention even though this modal leans on `Dialog`'s built-in transition — if `tsc`/lint flags `motion` as unused, drop the import. The `Dialog` shell handles enter/exit animation.

- [ ] **Step 4: Run UpgradeModal tests**

Run: `cd ui && npm test -- --run UpgradeModal 2>&1 | tail -10`
Expected: 3 PASS.

- [ ] **Step 5: Wire the 升级 button into AppsTab**

In `ui/src/components/automation/AppsTab.tsx`:
- Import `UpgradeModal`, `useAtomValue`, `marketplaceUpdatesAtom`, `checkMarketplaceUpdates`.
- On mount (alongside the existing installed-list load), call `checkMarketplaceUpdates()` and store the result, OR read `marketplaceUpdatesAtom` if it's already populated by the store views. Use whichever the existing code makes available — read the file first. If neither, call `checkMarketplaceUpdates()` in the same effect that loads the installed list and keep it in local state.
- Add local state `const [upgradeTarget, setUpgradeTarget] = React.useState<InstalledAutomation | null>(null)`.
- In each card header, next to the existing `卸载` button (~line 99), add — only when the automation's slug is in the updates set:

```tsx
{updateSlugs.has(item.slug) && (
  <button
    type="button"
    onClick={(e) => { e.stopPropagation(); setUpgradeTarget(item) }}
    className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-primary hover:bg-primary/10 transition-colors"
  >
    升级
  </button>
)}
```

- At the end of the component render, mount the modal when a target is set:

```tsx
{upgradeTarget && (
  <UpgradeModal
    slug={upgradeTarget.slug}
    name={upgradeTarget.name}
    currentVersion={upgradeTarget.version}
    installedSkillIds={upgradeTarget.bundledSkills.map((s) => s.skillId)}
    onClose={() => setUpgradeTarget(null)}
    onUpgraded={() => { void reload() }}
  />
)}
```

(`reload` is the existing installed-list refresh function in AppsTab — confirm its name by reading the file.)

- [ ] **Step 6: Wire the 升级到 vX action into StoreDetail**

In `ui/src/components/automation/StoreDetail.tsx`:
- Read the file to find where it decides the primary action button (the "安装" / "已安装" branch). It already knows `installedVersion` (from `MarketplaceDetail.installedVersion`) and `item.version`.
- When `installedVersion !== null && installedVersion !== item.version` (an update is available), render a `升级到 v{item.version}` button instead of the `已安装` label. Clicking it opens `UpgradeModal` with the same props shape. `installedSkillIds` for StoreDetail: StoreDetail doesn't have the installed skill list directly — pass `[]` (the diff will then show every new bundled skill as "added", which is acceptable for the store-side preview; the AppsTab path has the accurate installed list). Document this with a one-line comment.
- Add the same `upgradeTarget` local state + modal mount pattern.

- [ ] **Step 7: tsc + full vitest**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: zero TS errors. Vitest: prior baseline 521 passing → now 521 + 5 (skill-diff) + 3 (UpgradeModal) = 529, minus the pre-existing ConnectivityTab flake which sometimes shows. No NEW failures.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/automation/UpgradeModal.tsx ui/src/components/automation/UpgradeModal.test.tsx ui/src/components/automation/AppsTab.tsx ui/src/components/automation/StoreDetail.tsx
git commit -m "feat(marketplace): UpgradeModal + 升级 triggers in AppsTab & StoreDetail

UpgradeModal previews the version bump + bundled-skill diff (added /
removed / kept) before confirming. Upgrade reuses the existing
installMarketplaceHuman bridge — destructive reinstall is already a
correct, rollback-safe in-place upgrade. 升级 buttons are gated on the
existing marketplaceUpdatesAtom drift signal; no update → no button."
```

---

### Task 5: CLAUDE.md migration-registry correction + PR

**Files:**
- Modify: `CLAUDE.md` (the *Active migration registry* table — drive-by correction)

- [ ] **Step 1: Correct the V22 status**

Open `CLAUDE.md`, find the *Active migration registry* table. The V22 row currently reads `**this PR** (Phase 3b-α)` — Phase 3b-α merged as PR #160. Change it to:

```markdown
| V22 | automation_installed_skills + idx_aut_inst_skills_slug | merged (PR #160) |
```

Leave the rest of the table untouched. 3b-β adds **no migration** — there is no new row to add.

- [ ] **Step 2: Verify the full build one more time**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd ui && npx tsc --noEmit 2>&1 | head -3
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: Rust all green, zero TS errors, Vitest all green (no new failures vs baseline).

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): mark V22 migration as merged (PR #160)

Drive-by correction — the registry row still said 'this PR (Phase 3b-α)'.
3b-β itself adds no migration."
```

- [ ] **Step 4: Push and open PR**

```bash
git push -u origin worktree-phase3b-beta-skill-updates

gh pr create --title "feat(marketplace): Phase 3b-β — skill bundle updates" --body "$(cat <<'EOF'
## Summary

Phase 3b-α made automation install deliver working bundled skills but left no upgrade path. This slice adds one — plus fixes a stale-row bug and populates skill descriptions.

- **Upgrade path.** Reuses the existing `install_human` (destructive reinstall is already a correct, rollback-safe in-place upgrade). New `UpgradeModal` previews the version bump + bundled-skill diff (added / removed / kept) before confirming. 升级 triggers in AppsTab + StoreDetail, gated on the existing `marketplaceUpdatesAtom` drift signal.
- **Stale-row fix.** `registering_skills` used `INSERT OR REPLACE`, which only touches matching-PK rows — an upgrade that drops a bundled skill left an orphan `automation_installed_skills` row. Extracted `write_installed_skill_rows`: DELETE-by-slug then re-INSERT.
- **Skill descriptions.** `InstalledSkillBrief.description` was hardcoded `None`. New forgiving `read_skill_description` lazily reads SKILL.md frontmatter; AppsTab now shows descriptions.

No schema change — **no migration**.

## Commits (bisectable)

| # | Commit | Scope |
|---|--------|-------|
| 1 | fix(marketplace): clear stale installed-skill rows on (re)install | backend |
| 2 | feat(marketplace): populate InstalledSkillBrief.description from SKILL.md | backend |
| 3 | feat(marketplace): diffBundledSkills pure function | frontend |
| 4 | feat(marketplace): UpgradeModal + 升级 triggers in AppsTab & StoreDetail | frontend |
| 5 | docs(claude): mark V22 migration as merged (PR #160) | docs |

Spec: docs/superpowers/specs/2026-05-14-phase3b-beta-skill-updates-design.md
Plan: docs/superpowers/plans/2026-05-14-phase3b-beta-skill-updates.md

## Test plan

- [ ] `cargo test --lib` — all green (2 new: registering_skills_clears_stale_rows, read_skill_description_parses_frontmatter)
- [ ] `npm test -- --run` — all green (8 new: 5 skill-diff + 3 UpgradeModal)
- [ ] `npx tsc --noEmit` — zero errors
- [ ] Manual: install an automation, simulate a registry version bump, verify the 升级 button appears in AppsTab
- [ ] Manual: click 升级 — modal shows vX→vY + skill diff; confirm runs the upgrade; a failed upgrade leaves the old version working
- [ ] Manual: AppsTab shows bundled-skill descriptions from SKILL.md

## Follow-ups (Phase 3b-γ / δ / ε / ζ)

- 3b-γ — standalone skill / MCP marketplace entries (non-bundled install branch)
- 3b-δ — multi-registry + capability map → DB table
- 3b-ε — proxy adapters (Smithery / official MCP Registry / SkillHub)
- 3b-ζ — local hello-halo workspace as a registry source
EOF
)"
```

---

## Self-review

### Spec coverage

| Spec § | Where in plan |
|---|---|
| § 4.1 stale-row DELETE fix | Task 1 |
| § 4.2 `read_skill_description` + `list_installed_inner` | Task 2 |
| § 4.3 `diffBundledSkills` pure function | Task 3 |
| § 4.4 `UpgradeModal` | Task 4 (Steps 1-4) |
| § 4.5 升级 button placement (AppsTab + StoreDetail) | Task 4 (Steps 5-6) |
| § 5 error handling | Distributed: Task 1 (best-effort DELETE), Task 2 (None on any error), Task 4 (loadError state, toast on upgrade failure) |
| § 6.1 Rust tests | Task 1 Step 1, Task 2 Step 1 |
| § 6.2 Vitest tests | Task 3 Step 1, Task 4 Step 1 |
| § 7 no migration | Task 5 (only a drive-by CLAUDE.md correction, no new row) |
| § 9 done criteria | Task 5 Step 4 PR test plan mirrors each item |

No gaps.

### Placeholder scan

No "TODO" / "TBD" / "fill in later". Task 4 Steps 5-6 say "read the file first" to confirm exact local names (`reload`, the updates-atom availability, StoreDetail's action-button branch) — that's directed investigation with the surrounding code shown, not a placeholder. Every code step ships complete code.

### Type consistency

- `SkillDiff` / `diffBundledSkills` — defined Task 3, consumed Task 4.
- `write_installed_skill_rows(conn, slug, &[(String, i64)], i64)` — defined + tested Task 1, same signature both places.
- `read_skill_description(&Path) -> Option<String>` — defined + tested + wired Task 2, consistent.
- `UpgradeModal` props `{ slug, name, currentVersion, installedSkillIds, onClose, onUpgraded }` — identical across the test (Task 4 Step 1), the impl (Step 3), and both call sites (Steps 5-6).
- `InstalledSkillBrief` fields `{ skill_id/skillId, description, install_path/installPath, file_count/fileCount }` — Rust snake_case, TS camelCase via serde rename, consistent with the 3b-α DTO.
