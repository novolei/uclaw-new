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
  (and `uclaw_skills_dir()`, `uclaw_sessions_dir()`, etc.) once
  `uclaw-utils-home` lands. Existing call sites in `tauri_commands.rs` and
  `memubot_config.rs` are allowlisted until Phase 0.5-T6 sweeps them.
- **Plan Mode for non-trivial work**: explore → plan → implement → commit.
  Plans live in `docs/superpowers/plans/`. One plan = one PR. PR diff must
  show only the current task's commits — cherry-pick onto a clean
  `prep/<task>` branch from `origin/main` if needed.
- **Verification mandatory**: every PR's commit body must include a
  verification command (typically `cargo test -p <crate> -- <filter>` or
  `cd ui && npm test -- --run`) and the expected output.
- **Migration registry**: never reuse a V-number. Check the table in
  `CONTEXT.md` § Active migration registry before writing a schema migration.
- **DMZ files** (`agentic_loop.rs`, `tauri_commands.rs`, `CLAUDE.md`,
  `db/migrations.rs`, `Cargo.toml` workspace root, `BEHAVIOR.md`): touching
  these requires a writer/reviewer two-session review (see `BEHAVIOR.md` §8)
  or a clear status note in the PR description.
- **ADR §18 11 questions**: every strategic spec must answer all 11. See the
  ADR. Skipping is a behavior violation.

---

## Codex-specific notes

- Codex's default model selection and reasoning effort are not pinned by this
  file — they follow your `~/.codex/config.toml`.
- For long-running work (autonomous agents, harness episodes), prefer to run
  Codex inside a sub-worktree (`git worktree add ~/Documents/uclaw-worktrees/<task>`)
  so its checkouts don't collide with the human's primary worktree at
  `~/Documents/uclaw`. See `BEHAVIOR.md` §5.
- Codex's seatbelt sandbox and the uClaw `SafetyManager` are independent
  layers; both will be active when Codex spawns shell commands. Don't write
  code that assumes either is disabled.

---

## When this file conflicts with `BEHAVIOR.md`

`BEHAVIOR.md` wins for any rule that says "always", "must", or "never".
This file may add Codex-specific notes on top of `BEHAVIOR.md` but cannot
weaken its rules. If you find a real contradiction, open a PR titled
`docs(behavior): <what>` and resolve it in `BEHAVIOR.md`.
