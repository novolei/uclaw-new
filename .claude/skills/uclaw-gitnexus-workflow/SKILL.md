---
name: uclaw-gitnexus-workflow
description: Use before editing any function, method, or class — especially in files referenced by multiple call sites, or anywhere in src-tauri/src/agent/, src-tauri/src/llm/, ui/src/components/agent/, ui/src/atoms/. Trigger phrases include "before I change X", "what calls this", "blast radius", "impact analysis", "rename", "refactor", "where is X used", "what breaks if", "GitNexus". Loads the mandatory impact-analysis ritual (CLAUDE.md "Always Do") and the gitnexus CLI commands that ground decisions in the actual call graph rather than guesses.
---

# uClaw — GitNexus Workflow

uClaw is indexed by GitNexus as **`uclaw-new`** (28393 symbols / 47562
relationships / 300 execution flows). CLAUDE.md has a hard rule:

> **MUST run impact analysis before editing any symbol.**

This skill is the operational form of that rule. GitNexus is currently
only available via the **CLI** in this Cowork (no MCP), so commands run
through the shell tool.

## The mandatory ritual

Before any non-trivial Edit/MultiEdit on a function or class:

```bash
gitnexus impact --target "<symbol_name>" --direction upstream --repo uclaw-new
```

Read the output:

- **Risk: LOW** — proceed, normal review
- **Risk: MEDIUM** — proceed, but mention affected callers in the commit
- **Risk: HIGH** or **CRITICAL** — *stop*. Surface the blast radius to the
  user, ask before proceeding. Per CLAUDE.md: "MUST warn the user".

## Commands you'll actually use

| Need | Command |
|---|---|
| "What breaks if I change `run_agentic_loop`?" | `gitnexus impact --target run_agentic_loop --direction upstream --repo uclaw-new` |
| "What does `compress_context` call?" | `gitnexus impact --target compress_context --direction downstream --repo uclaw-new` |
| "Find the execution flow for streaming responses" | `gitnexus query --query "streaming response" --repo uclaw-new` |
| "Show me full context on a symbol" | `gitnexus context --name <symbol> --repo uclaw-new` |
| "Verify my changes only affect expected scope" | `gitnexus detect-changes --repo uclaw-new` (run before commit) |
| "Check the index is fresh" | `gitnexus status --repo uclaw-new` |

If status says the index is stale: `gitnexus analyze --repo uclaw-new`.

## Pre-commit advisory

`scripts/git-hooks/checks/check-gitnexus-changes.sh` runs
`gitnexus detect-changes` as a **non-blocking** advisory on commit. It
surfaces the symbols / processes touched so the human can sanity-check
before pushing. The advisory does NOT block — but if you see surprising
output ("touched 12 processes when I expected 1"), pause and re-read your
diff.

## Where this matters most

These areas have dense call graphs — impact analysis is required:

| Area | Why |
|---|---|
| `src-tauri/src/agent/` (384 symbols) | Agent loop is the central pipeline; everything routes through it |
| `src-tauri/src/llm/` + `providers/` | Provider trait changes ripple to every model |
| `src-tauri/src/db/migrations.rs` | DMZ — schema changes touch the whole storage layer |
| `ui/src/components/agent/AgentView.tsx` | The most-used UI surface |
| `ui/src/atoms/` (75 symbols, 27+ files) | Jotai atoms are shared state — changes cascade |

For greenfield / utility code with no callers yet, impact analysis is
optional (it'll return empty). Still cheap to run.

## What NEVER to do (per CLAUDE.md)

- ❌ Edit a function/class/method without first running `gitnexus impact`
- ❌ Ignore HIGH or CRITICAL risk warnings
- ❌ Rename via find-and-replace — use `gitnexus rename` (call-graph aware)
- ❌ Commit without `gitnexus detect-changes` (the pre-commit hook will
  surface it, but better to check before you've already typed `git commit`)

## Skill drilldowns

CLAUDE.md lists per-task skill files for deeper guidance:

| Task | Read this |
|---|---|
| "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / safe refactor | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |

(These ship with the gitnexus CLI install; `.claude/` un-ignore landed in
PR-5 so they're now visible to skill auto-discovery.)

## See also

- CLAUDE.md *GitNexus — Code Intelligence* section (the hard rule)
- `gitnexus://repo/uclaw-new/context` resource (overview, freshness)
- `gitnexus://repo/uclaw-new/clusters` (functional areas)
- `gitnexus://repo/uclaw-new/processes` (execution flows)
