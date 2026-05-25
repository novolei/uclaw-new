# uclaw git hooks

Tracked, team-shared git hooks that enforce architectural discipline at commit time.
Designed to work regardless of which agent or IDE produced the change — they fire
on the git layer, not the editor layer.

## Install

From repo root after a fresh clone:

```bash
./scripts/install-git-hooks.sh
```

This sets `git config core.hooksPath scripts/git-hooks/`. One-shot and reversible
(`git config --unset core.hooksPath` to revert).

## What's installed

### `pre-commit`

Runs every executable `check-*.sh` under `checks/`. Each check is an independent
script that exits 0 (pass) or non-zero (fail). Any failure blocks the commit.

| Check | What it blocks | Reference |
|---|---|---|
| `check-memory-graph-freeze.sh` | New `memory_graph::write*` / `insert*` / `update*` / `delete*` calls | ADR §11.2 + `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` |
| `check-dirs-home-dir-uclaw.sh` | New `dirs::home_dir().*".uclaw"` patterns | `uclaw-upgrade-implementation-plan.md` Phase 0.5-T6 sweep |
| `check-codex-derived-spdx.sh` | Missing SPDX header or "Derived from codex-rs/" attribution in `src-tauri/uclaw-{utils,async,file}-*/` | `docs/THIRD_PARTY.md` §3.2 |
| `check-gitnexus-changes.sh` | (advisory only — won't block) Surfaces HIGH/CRITICAL risk from `gitnexus detect-changes --scope staged` | `CLAUDE.md` "GitNexus — Code Intelligence" |

### `post-merge`

Auto-runs `npm install` after `git pull` / `git merge` when `ui/package.json`
or `ui/package-lock.json` changed. Previously lived in `.git/hooks/post-merge`
as a per-checkout file; now tracked so every team member gets it automatically
after running the install script.

## Bypassing

For genuine emergencies only:

```bash
git commit --no-verify
```

Use sparingly. The pre-commit failure message tells you exactly what's blocking
and where the canonical reference is.

## Adding new checks

1. Drop a new `check-*.sh` script into `checks/` (must be executable + start with `#!/usr/bin/env bash`).
2. Exit `0` to pass, non-zero to block. Print actionable messages to stderr.
3. Use `git diff --cached -U0 -- "$f" | grep -E '^\+' | grep -vE '^\+\+\+'` to inspect **only newly-added lines** in staged Rust files — don't trip on pre-existing patterns.
4. Allowlist existing call sites by file path if you need a grace period during migration.
5. Update this README's table.
6. New checks must include test cases in `scripts/git-hooks/tests/` (to be added).

## GitNexus integration

`check-gitnexus-changes.sh` is **advisory** — it surfaces risk but does not
block. It runs `gitnexus detect-changes --scope staged` and reports HIGH or
CRITICAL risk levels with the full breakdown.

If GitNexus isn't installed locally (`gitnexus` not on PATH), the check is a
silent no-op. To install:

```bash
npm install -g gitnexus
gitnexus setup       # configures MCP for your editors (Claude Code, Cursor, ...)
./scripts/gitnexus-analyze-index-only.sh
```

Use the repo wrapper for routine refreshes. A plain `gitnexus analyze` updates
the GitNexus managed blocks in `AGENTS.md` and `CLAUDE.md` by design; that is
GitNexus's AI-context generator, not a repo hook. The wrapper runs
`npx gitnexus analyze --index-only`, so it refreshes the local graph without
rewriting agent docs or skill files. If you intentionally need generated skills
or agent-doc regeneration, run GitNexus directly and review the resulting diff.

See [GitNexus README](https://github.com/abhigyanpatwari/GitNexus) for details.

## Why git hooks and not Claude Code hooks?

`.claude/` is in `.gitignore`, so Claude Code's PreToolUse hooks (in
`.claude/settings.json`) are per-user and not shareable. Git hooks fire
regardless of the agent / IDE and work on every clone after running
`install-git-hooks.sh`, so the discipline reaches everyone — human contributors,
Cowork sessions, Claude Code in any IDE, even direct `git commit` from the
terminal.

The follow-up Phase 0.5-T10 (Claude Code skills) addresses the in-session,
real-time feedback layer separately.
