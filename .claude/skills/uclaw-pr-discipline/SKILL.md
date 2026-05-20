---
name: uclaw-pr-discipline
description: Use whenever you're about to commit, open a PR, finish a task in a plan, or are working on the codex-absorption-v2.2 integration branch. Trigger phrases include "open a PR", "create a pull request", "ready to merge", "commit", "push", "cherry-pick", "prep branch", "stack PR", "one PR per plan", "bisectable", "PR discipline", "review-ready diff". Loads the cherry-pick prep-branch pattern (clean PR diffs from an accumulating integration branch), the stacking convention, and the commit-shape rule.
---

# uClaw — PR Discipline

The codex-absorption work uses a **two-branch model** to keep PR diffs
clean while letting the integration branch accumulate experimental work:

```
                    main
                     |
                     |
   ┌─────────────────┴─────────────────┐
   |                                   |
prep/codex-absorption-pr<N>-<name>   claude/codex-absorption-v2.2
   |                                   (integration branch — work freely here)
   |                                   accumulating commits
PR to main                             ↑
(clean, single-commit                  | cherry-pick of one
 review-ready diff)                    | logical commit
```

The integration branch (`claude/codex-absorption-v2.2`) is where you do
exploratory work — commit early, commit often, sometimes squash later.
The prep branches are **clean cherry-picks of one task's commit** off
`origin/main` (or stacked on a predecessor prep branch). PRs go from prep
branches → main.

## When this skill applies

- You finished a plan task (a row in `docs/superpowers/plans/*.md` or the
  upgrade-plan) and want a PR for it.
- The work is on `claude/codex-absorption-v2.2` (or any long-running
  integration branch you're driving).
- You want a bisectable, review-friendly diff.

## The procedure — one task → one PR

```bash
# 1. On the integration branch, make focused commit(s) for THIS task.
git checkout claude/codex-absorption-v2.2
# ...do work...
git add <files>
git commit -m "<scope>: <subject> [Phase 0.5-T<N>]"
# Take note of the commit SHA: e.g. abc1234

# 2. Decide the base for the prep branch.
#    - If no predecessor PR is open: base on origin/main
#    - If a predecessor PR is open and unmerged: base on its prep branch
git fetch origin main --quiet
# OR for a stacked PR:
git fetch origin prep/codex-absorption-pr<PREV>-<name> --quiet

# 3. Create the prep branch from that base.
git checkout -b prep/codex-absorption-pr<N>-<topic> origin/main
# OR
git checkout -b prep/codex-absorption-pr<N>-<topic> origin/prep/codex-absorption-pr<PREV>-<name>

# 4. Cherry-pick the integration commit.
git cherry-pick abc1234

# 5. Push the prep branch.
git push -u origin prep/codex-absorption-pr<N>-<topic>

# 6. Open the PR. Use --body-file with a real markdown file — heredoc with
#    backticks in table cells gets mangled.
gh pr create \
  --base main \
  --head prep/codex-absorption-pr<N>-<topic> \
  --title "[Phase 0.5-T<N>] <short title>" \
  --body-file /tmp/pr<N>-body.md
```

## Stacking convention

When PR <N> logically depends on PR <N-1> being merged first:

- Base prep branch <N> on `origin/prep/codex-absorption-pr<N-1>-<name>`,
  not on `origin/main`.
- The PR `--base` is `prep/codex-absorption-pr<N-1>-<name>`, not `main`.
- GitHub will auto-retarget the PR to `main` after <N-1> merges.
- Mention the stack in the PR body: "Stacked on #XXX".

## Commit shape (per CLAUDE.md)

> Bisectability: one logical change per commit. Match the plans in
> `docs/superpowers/plans/*.md`.

> PR shape: one branch per plan, one commit per plan task, one PR with a
> `## Commits (bisectable)` table. See PRs #29, #31, #33, #35, #36.

Subject format:

```
<scope>(<subscope>): <verb-phrase> [Phase 0.5-T<N>]

<2-paragraph body>:
1. WHY this commit exists (link ADR / spec section)
2. WHAT changes mechanically (file-level summary)
```

## What NEVER to do

- ❌ **Force-push to a shared branch** — `main` is protected. Prep branches
  are throwaways: you can force-push your own prep branch before opening
  the PR, but never after a reviewer has commented.
- ❌ **PR directly from the integration branch** — diffs will include every
  experimental commit, reviewers will hate you.
- ❌ **Multi-task PRs** — split into stacked PRs instead. PRs #29, #31, #33,
  #35, #36 are the canonical examples.
- ❌ **`git commit --no-verify`** outside an emergency. If you bypassed the
  pre-commit hook, the next commit must fix the violation or document the
  allowlist exception. Mention in the PR body.

## Verification before pushing

```bash
# Backend
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -20

# Frontend
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10

# Impact analysis (per uclaw-gitnexus-workflow skill)
gitnexus detect-changes --repo uclaw-new
```

If any of these fail, fix before pushing. Don't punt to "CI will catch it".

## See also

- CLAUDE.md Part 1 *Workflow* + *Verification commands*
- `BEHAVIOR.md §"DMZ Files Need Two-Session Review"` — DMZ edits need a reviewer
- `uclaw-codex-comparison-and-design.md §25` — single source of truth rules
- Past examples: PRs #29, #31, #33, #35, #36, #289 (LICENSE), #291 (hooks),
  #292 (BEHAVIOR), #293 (DRI naming), #294 (claude hooks)
