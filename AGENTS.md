# AGENTS.md

Top-level entry file for **Codex CLI** working in uClaw.

> **Codex reads `AGENTS.md` hierarchically** from the repo root down to the
> current working directory. This file is uClaw's canonical Codex entry.
>
> **Codex does not support `@import` syntax**, so the critical short-form
> rules are inlined below. The full multi-session behavior contract lives in
> `BEHAVIOR.md` and **you must read it** before any non-trivial work — it
> applies to every agent/IDE working in this repo, not just Codex. The
> strategic baseline is in `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`.

---

## Read these first

1. **`BEHAVIOR.md`** — the canonical multi-session behavior contract (10 practices).
2. **`CONTEXT.md`** — detailed project reference: architecture, build commands, gotchas, migration registry.
3. **`docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`** — strategic baseline (Agent OS v2). New strategic specs must answer ADR §18's 11 questions.

These three files together form the source of truth. This `AGENTS.md` only
echoes the critical rules so Codex has them in immediate context.

---

## Critical rules (inlined for Codex)

These are non-negotiable. The git pre-commit hooks in `scripts/git-hooks/`
back them up — any violation blocks the commit. To install the hooks:
`./scripts/install-git-hooks.sh`.

- **License**: Apache-2.0. Every file derived (with or without modification)
  from `openai/codex` must carry an SPDX header (`SPDX-License-Identifier: Apache-2.0`)
  and a `Derived from codex-rs/<path>` attribution in its first 10 lines,
  plus an entry in the repo-root `NOTICE`. See `docs/THIRD_PARTY.md` §3.2.
- **`memory_graph` is FROZEN** (ADR §11.2). Never write to it. New durable
  facts go to `gbrain`. The pre-commit hook blocks new
  `memory_graph::{write,insert,update,delete}*` calls. Allowlist is limited
  to `memory_graph/mod.rs` (the freeze panic guard) and
  `memory_graph/legacy_migration/`.
- **`dirs::home_dir().*".uclaw"` is banned**. Use `uclaw_utils_home::uclaw_home()`
  and directory helpers such as `uclaw_skills_dir()` and
  `uclaw_sessions_dir()`. Remaining legacy call sites must stay on explicit
  allowlists until swept.
- **Plan Mode for non-trivial work**: explore → plan → implement → commit.
  Plans live in `docs/superpowers/plans/`. One plan = one PR. PR diff must
  show only the current task's commits. Use the branch naming required by the
  active goal/tracker/plan (for example `codex/<browser-runtime-phase-name>` in
  Browser Runtime phase-pack work); otherwise use a clean `prep/<task>` or
  `codex/<task>` branch from `origin/main`.
- **Verification mandatory**: every PR's commit body must include a
  verification command (typically `cargo test -p <crate> -- <filter>` or
  `cd ui && npm test -- --run`) and the expected output.
- **Migration registry**: never reuse a V-number. Check the table in
  `CONTEXT.md` § Active migration registry before writing a schema migration.
- **High-attention policy files** (`CLAUDE.md`, `db/migrations.rs`,
  `Cargo.toml` workspace root, `BEHAVIOR.md`): these are review-sensitive
  files, not forbidden files. Touch them with an explicit plan, tight scope,
  focused verification, and a PR note explaining why the edit is necessary.
  Runtime hot-path files such as `agentic_loop.rs` and `tauri_commands.rs` are
  no longer special DMZ files; edit them under normal code discipline: plan the
  slice, run GitNexus impact for changed symbols, keep the diff narrow, run
  focused tests, and request fresh review only when the change is broad, risky,
  or paired with HIGH/CRITICAL GitNexus impact. Do not make these large files
  larger by default: put new behavior in focused modules and keep these files
  as orchestration/IPC shims whenever practical.
- **ADR §18 11 questions**: every strategic spec must answer all 11. See the
  ADR. Skipping is a behavior violation.

---

## GitNexus — Code Intelligence

The auto-managed GitNexus block below is the canonical detailed tool map. In
short: run impact before editing existing code symbols, run detect-changes
before commit, and treat HIGH/CRITICAL as a review gate unless the DRI/user has
explicitly authorized continuing. Docs-only edits that do not modify code
symbols do not require symbol impact, but they still require detect-changes.

---

## Codex-specific notes

- Codex's default model selection and reasoning effort are not pinned by this
  file — they follow your `~/.codex/config.toml`.
- For long-running work (autonomous agents, harness episodes), prefer to run
  Codex inside a sub-worktree (`git worktree add ~/Documents/uclaw-worktrees/<task>`)
  so its checkouts don't collide with the human's primary worktree at
  `~/Documents/uclaw`. See `BEHAVIOR.md` §5.
- Long-running goal-mode PR chains may fast-forward the primary `main` after a
  PR merges, but must preserve unrelated untracked files and must not edit the
  primary worktree directly.
- Codex's seatbelt sandbox and the uClaw `SafetyManager` are independent
  layers; both will be active when Codex spawns shell commands. Don't write
  code that assumes either is disabled.

---

## When this file conflicts with `BEHAVIOR.md`

`BEHAVIOR.md` wins for any rule that says "always", "must", or "never".
This file may add Codex-specific notes on top of `BEHAVIOR.md` but cannot
weaken its rules. If you find a real contradiction, open a PR titled
`docs(behavior): <what>` and resolve it in `BEHAVIOR.md`.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **uclaw-new** (37465 symbols, 61927 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/uclaw-new/context` | Codebase overview, check index freshness |
| `gitnexus://repo/uclaw-new/clusters` | All functional areas |
| `gitnexus://repo/uclaw-new/processes` | All execution flows |
| `gitnexus://repo/uclaw-new/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
