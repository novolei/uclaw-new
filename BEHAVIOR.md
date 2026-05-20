# uClaw Multi-Session Behavior Spec

> **Canonical behavior contract for any AI agent or session working in uClaw.**
>
> Read this once at the start of every non-trivial session. It applies regardless
> of which agent or IDE you are: Claude Cowork, Claude Code (any IDE), Codex CLI,
> Cursor, Continue.dev, GitHub Copilot, Aider, or direct shell.
>
> When an IDE-specific entry file (`CLAUDE.md`, `AGENTS.md`, `.cursorrules`,
> `.github/copilot-instructions.md`) conflicts with this file, **this file wins**
> for any rule that says "always", "must", or "never". IDE entry files may add
> IDE-specific notes on top of this spec but cannot weaken its rules.
>
> See [§ How each IDE wires up](#how-each-ide-wires-up) at the bottom for the
> per-IDE entry-file pattern. See [`docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`](docs/adr/2026-05-20-uclaw-agent-platform-north-star.md)
> for the strategic baseline this spec serves.

---

## 1. Plan Mode — Four Phases

For any non-trivial change (more than a typo / single-line fix / doc-only edit),
always run these four phases in order:

1. **Explore** — read files, ask clarifying questions, do not modify anything.
   Use subagents for broad reads to keep your main context clean.
2. **Plan** — write a plan to `docs/superpowers/plans/<M*-T*>-<topic>.md`.
   The plan must answer ADR §18's 11 questions (intent / autonomy / truth source /
   TaskEvent / context / capability / hooks / projection / harness / rollback /
   what this does not own).
3. **Implement** — small commits, each independently compilable. Use
   `verify-then-commit`: every commit's body lists the verification command and
   its expected output.
4. **Commit + PR** — push to a `prep/codex-absorption-pr<N>-<name>` branch
   cherry-picked from `origin/main` so the PR diff shows only the current task.
   Open one PR per plan.

**Skip Plan Mode** only for: typos, ≤ 1-file mechanical fixes, doc-only changes,
or hotfixes with an obvious root cause and a ≤ 1-file fix.

## 2. Context Discipline — Keep CLAUDE.md Concise

Root context files should stay near the official Claude Code best-practice
target of ~120 lines. Detailed reference material goes in `CONTEXT.md`. Per-area
conventions go in `src-tauri/src/<area>/CLAUDE.md` and are loaded on demand.

- For each line in any context file, ask: "Would removing this cause an agent
  to make a mistake?" If no, cut it.
- Migration tables, architecture diagrams, and historical decisions belong in
  `CONTEXT.md`, not in the root.
- When a rule applies only inside a single module, place it in that module's
  `CLAUDE.md`, not the root.

## 3. Progressive Disclosure — Skills, Not Bloat

Domain knowledge that is needed only sometimes belongs in `.claude/skills/`,
not in `CLAUDE.md`. Skills are loaded on demand.

uClaw skill catalog (auto-managed by `gitnexus setup` and team-curated):

- `.claude/skills/gitnexus/*` — code intelligence: exploring, impact analysis,
  debugging, refactoring, guide, CLI.
- `.claude/skills/generated/<area>/*` — per-area maps generated from the
  knowledge graph (agent, browser, automation, harness, learning, …).
- `.claude/skills/superpowers/*` — universal workflow skills (brainstorming,
  writing-plans, subagent-driven-development, …).

If an agent reads more than ~3 unrelated `.claude/skills/*/SKILL.md` files in a
single session, that is a smell — the wrong skills are being loaded, or the
work has drifted off topic.

## 4. Subagent Discipline — Read in Subagents, Edit in the Main Session

When research requires reading many files (`grep`, `find`, broad exploration),
do that work in a subagent so the file contents stay out of the main session's
context window. The subagent reports findings back to the main session, which
then edits.

Rule of thumb: **subagents for exploration; the main session for editing**.

## 5. Worktree Isolation — Parallel Sessions in Separate Physical Directories

When two sessions (e.g. Cowork + Claude Code in VS Code) work on the same
uClaw repo simultaneously, use [git worktrees](https://git-scm.com/docs/git-worktree)
to isolate them physically:

```bash
git worktree add ~/Documents/uclaw-cowork -b claude/codex-absorption-v2.2
```

Each worktree is a separate checkout, separate `target/` build cache, separate
file-system view. Sessions in different worktrees cannot stomp on each other.

A central rule: **the user-facing primary worktree (`~/Documents/uclaw`)
belongs to the human's IDE**. Cowork and other AI sessions live in
subordinate worktrees under `~/Documents/uclaw-cowork`, `~/Documents/uclaw-worktrees/<task>`,
or `.claude/worktrees/<task>` (`.claude/worktrees/` is gitignored).

## 6. Context Management — Compact Proactively, Clear Between Tasks

- Run `/compact <focus>` at roughly 60% context utilization. Manual compaction
  while the model still remembers everything produces a far better summary than
  the auto-compaction that kicks in at 85–90%.
- Run `/clear` when switching to an unrelated task. Polluting one task with
  another's context is the single most common cause of bad agent output.
- For multi-day work, use named resumable sessions (`claude --resume <name>` or
  the Cowork equivalent) so each work stream keeps its own clean context.

## 7. Verification First — Highest-Leverage Habit

Every task ends with a verification command whose output is unambiguous.
Without verification you become the agent's only feedback loop and every
mistake costs you attention.

Patterns:

- Tests: `cargo test -p <crate> -- <filter>` or `cd ui && npm test -- --run`
- Screenshots: `mcp__plugin_pdf-viewer_pdf__interact` (PDFs) or the Claude in
  Chrome extension for web UI
- Expected output: `cargo build 2>&1 | grep -E "^error" | head` to confirm no
  compiler errors
- Symbol-level: `gitnexus impact <symbol>` to confirm the blast radius matches
  what the plan said it should

Always include the verification command **and its expected output** in the
commit body.

## 8. Writer / Reviewer — Separate Sessions for Fresh-Eye Review

For P0 tasks (anything touching DMZ files: `agentic_loop.rs`, `tauri_commands.rs`,
`CLAUDE.md`, `db/migrations.rs`, `Cargo.toml` workspace root, or anything
flagged HIGH/CRITICAL by `gitnexus impact`), use two sessions:

- **Writer session** implements the change and pushes the prep branch.
- **Reviewer session** (a fresh `/clear` or a separate IDE) reviews the diff
  without reading the writer's transcript. The reviewer's prompt is
  `gh pr diff <number>` plus `gitnexus context <main_symbol>` and nothing else.

This avoids the "writer rationalizing their own bug" failure mode.

## 9. Deterministic Enforcement — Hooks, Not Vibes

Some rules must be enforced by code, not by agent self-discipline. uClaw has
two enforcement layers:

- **Git pre-commit hooks** (universal — fire on every commit regardless of
  who/what authored it): `scripts/git-hooks/` with `install-git-hooks.sh`.
  Blocks: `memory_graph::write*` (ADR §11.2 freeze),
  `dirs::home_dir().*".uclaw"` (use `uclaw_utils_home` instead), missing
  SPDX header in derived crates. Advisory: `gitnexus detect-changes` risk
  surfacing.
- **Claude Code hooks** (in-session real-time feedback): `.claude/settings.json`
  registers `PreToolUse` hooks. Same rule set; faster feedback than waiting
  until commit.

To bypass for an emergency: `git commit --no-verify`. Use sparingly. Every
bypass should be paired with a follow-up commit that fixes the violation or
adds a documented allowlist exception.

## 10. DRI / Agent Manager — One Human Owns This

> **Current DRI**: **Ryan Liu** ([@novolei](https://github.com/novolei) on GitHub, `ryanclaudemax@gmail.com`).
>
> The DRI is the single point of decision when an agent / contributor / hook disagrees with this spec. They own:

- `BEHAVIOR.md` content and revisions
- `.claude/settings.json` and the team-curated skill set
- The plugin marketplace selection and config
- The `scripts/git-hooks/` policy set
- The quarterly review cadence (every 3 months: prune CLAUDE.md, refresh
  skills, audit hooks for false positives, retire deprecated rules)

This DRI is the single point of decision when an agent / contributor / hook
disagrees with the spec. Without a DRI the spec becomes a vague suggestion.

---

## uClaw-specific rules layered on top

These are non-negotiable uClaw rules. They apply to every session regardless
of which agent or IDE is acting:

- **License**: Apache-2.0. Every new derived file from `openai/codex` needs
  `SPDX-License-Identifier: Apache-2.0` + `Derived from codex-rs/<path>`
  header + an entry in `NOTICE`. See `docs/THIRD_PARTY.md`.
- **`memory_graph` is FROZEN** (ADR §11.2). Never write to it. New durable
  facts go to `gbrain`. The pre-commit hook blocks new
  `memory_graph::{write,insert,update,delete}*` calls; the runtime panic
  guard (lands in Phase 0.5-T7) backs this up at execution time.
- **`dirs::home_dir().*".uclaw"` is banned**. Use `uclaw_utils_home::uclaw_home()`
  (and the directory helpers `uclaw_skills_dir()`, `uclaw_sessions_dir()`,
  `uclaw_plugins_dir()`, etc.) once `uclaw-utils-home` lands (Phase 0.5-T5).
  The pre-commit hook blocks the pattern; the existing call sites are
  allowlisted until Phase 0.5-T6 sweeps them.
- **ADR §18 11 questions**: every strategic spec must answer 11 questions
  (intent, autonomy, truth source, TaskEvent, context, capability, hooks,
  projection, harness, rollback, what it does not own). See
  `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` §18.
- **Active migration registry** lives in `CONTEXT.md`. Reserve your V-number
  there before writing any schema migration.
- **GitNexus discipline** — see the auto-managed `<!-- gitnexus:start -->`
  block in `CLAUDE.md`. MUST run `gitnexus impact` before editing any symbol;
  MUST run `gitnexus detect-changes` before committing.

---

## How each IDE wires up

Every IDE has its own canonical entry file. They all point here for the
behavior contract:

### Claude Code (Cowork, VS Code, JetBrains, web)

- Reads `CLAUDE.md` automatically. `CLAUDE.md` uses `@BEHAVIOR.md` import
  so the spec is inlined into the model's context at session start.
- Per-area overrides in `src-tauri/src/<area>/CLAUDE.md` are loaded on demand
  when the agent walks into that directory.

### Codex CLI

- Reads `AGENTS.md` automatically (hierarchical, concatenated from repo root
  down to cwd). `AGENTS.md` instructs Codex to read this `BEHAVIOR.md` file
  as part of session initialization.
- Codex does not support `@import` syntax — `AGENTS.md` therefore restates
  the *critical* short-form rules inline and references this file for the
  full text.

### Cursor

- Reads `.cursorrules` automatically. `.cursorrules` restates the critical
  short-form rules and instructs Cursor to read `BEHAVIOR.md` before
  non-trivial edits.

### GitHub Copilot (in VS Code / JetBrains)

- Reads `.github/copilot-instructions.md` for repo-wide custom instructions.
  Same pattern as `.cursorrules`: critical rules inline, full text via file
  reference.

### Continue.dev, Aider, Windsurf, OpenCode, others

- All of these read custom-instruction files (`.continuerules`, `.aider.conf`,
  `.windsurfrules`, etc.) or default to `AGENTS.md` / `CLAUDE.md`. The
  pattern is identical: restate critical rules inline, reference `BEHAVIOR.md`
  for the full spec.

### Direct shell / human

- A human contributor reads `CLAUDE.md` (top-level) + `BEHAVIOR.md` (this file)
  + the ADR before opening a non-trivial PR. The pre-commit hook backs up
  the discipline if the human forgets.

---

## How to update this file

1. Open a PR titled `docs(behavior): <what changed>`.
2. Update every IDE entry file's "Inline critical rules" section to mirror
   any new critical rule.
3. Update CLAUDE.md (and CONTEXT.md if relevant) with cross-references.
4. The DRI (see §10) reviews and merges. Bypassing the DRI on a behavior-spec
   change is itself a behavior violation.

**Last reviewed**: 2026-05-20
**Next scheduled review**: 2026-08-20 (quarterly cadence)
