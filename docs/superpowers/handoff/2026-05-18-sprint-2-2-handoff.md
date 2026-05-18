# Sprint 2.2 — Profile UI tab (Mac-side commit hand-off)

Branch: `claude/sprint-2-2-profile-ui` (off `main` @ `85486b4`).

The sandbox keeps re-creating `.git/index.lock` so the commit needs to
land Mac-side. All 8 files are already staged.

## Pre-flight

```bash
cd ~/Documents/uclaw
git checkout claude/sprint-2-2-profile-ui     # branch should already exist
rm -f .git/index.lock                          # clear sandbox-leftover lock
git status                                     # 8 files staged + 1 doc untracked
```

## Commit

```bash
git commit -F docs/superpowers/handoff/COMMIT_SPRINT_2_2.txt
```

(commit message file is in the same handoff folder)

## Verify

```bash
cd ui
npx tsc --noEmit
npm test -- --run src/components/settings/LearnedProfileTab.test.tsx \
                  src/components/settings/SettingsNav.test.tsx
```

Sandbox couldn't run Vitest (missing platform-specific rollup binary)
but `npx tsc --noEmit` was green in the sandbox before commit. The
component follows the same shape as MemoryHealthPanel (Phase 4) which
has been green in CI since it landed.

## What's in the commit

```
ui/src/atoms/settings-tab.ts                          # +'learnedProfile' to enum
ui/src/components/settings/LearnedProfileTab.tsx      # new — 424 lines
ui/src/components/settings/LearnedProfileTab.test.tsx # new — 5 vitest cases
ui/src/components/settings/SettingsNav.tsx            # +nav entry, UserCircle2 icon
ui/src/components/settings/SettingsNav.test.tsx       # +'学到的偏好' spot-check
ui/src/components/settings/SettingsPanel.tsx          # switch + label
ui/src/lib/tauri-bridge.ts                            # +3 IPC wrappers
ui/src/lib/types.ts                                   # +FacetDto + 4 inputs
```

## What this unlocks

The whole Sprint 1+2 pipeline is now end-to-end visible:

1. Producer (Sprint 2.0) — chat-turn extractor pushes candidates
2. Stability rebuild (Sprint 1.4 + ProactiveService tick) — every 30 min
3. FacetCache refresh (Sprint 1.5)
4. PROFILE.md disk write (Sprint 2.1a)
5. System prompt injection (Sprint 1.8 + Sprint 2.0 set_learned_profile_block)
6. **Sprint 2.2** — Settings tab so user can see + dismiss

Without #6 the user could only inspect the cache via PROFILE.md on
disk. Now they can see / dismiss facets from the UI.
