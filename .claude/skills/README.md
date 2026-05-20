# `.claude/skills/` — uClaw Skill Catalog

Skills are **progressive-disclosure context blocks** that Claude Code
auto-loads when their description matches user intent. The format is one
directory per skill, with a `SKILL.md` containing YAML frontmatter
(`name`, `description`) followed by the actual instruction body.

Claude only reads a skill's body when the description triggers a match —
this keeps the default context tight (CLAUDE.md ≤120 lines goal).

## uClaw-specific skills

Authored for this codebase by Cowork ([Phase 0.5-T10]). Triggered by the
keyword phrases in each `description:` frontmatter line.

| Skill | Trigger | What it loads |
|---|---|---|
| [`uclaw-migrations`](./uclaw-migrations/SKILL.md) | "migration", "V-number", "ALTER TABLE", "FTS index" | V-number registry, idempotency rules, FTS backfill, the two-table-per-domain trap |
| [`uclaw-tauri-commands`](./uclaw-tauri-commands/SKILL.md) | "Tauri command", "invoke", "invoke_handler!", "#[tauri::command]" | Two-edit rule (commands + macro), DMZ warning, command-shape template |
| [`uclaw-composers`](./uclaw-composers/SKILL.md) | "ChatInput", "AgentView", "composer", "paste files", "drag drop" | Two parallel composers, apply-to-both rule, prop wiring |
| [`uclaw-codex-derived`](./uclaw-codex-derived/SKILL.md) | "from codex", "uclaw-utils-*", "SPDX header", "NOTICE" | SPDX header template, NOTICE update procedure, batch-of-3-5 porting |
| [`uclaw-memory-graph-freeze`](./uclaw-memory-graph-freeze/SKILL.md) | "memory_graph", "knowledge graph", "store fact", "gbrain" | Freeze rationale, gbrain redirect, exempt-path list |
| [`uclaw-gitnexus-workflow`](./uclaw-gitnexus-workflow/SKILL.md) | "before I change X", "blast radius", "impact analysis", "rename" | Mandatory `gitnexus impact` ritual, risk-level decision tree |
| [`uclaw-pr-discipline`](./uclaw-pr-discipline/SKILL.md) | "open a PR", "cherry-pick", "prep branch", "stack PR", "bisectable" | Cherry-pick prep-branch pattern, stacking convention, commit shape |

## How to author a new skill

1. Create `.claude/skills/<topic>/SKILL.md`.
2. Frontmatter must include:
   ```yaml
   ---
   name: <topic>
   description: Use whenever <trigger>. Triggers include "<phrase1>", "<phrase2>". Loads <what>.
   ---
   ```
   The `description` field is what Claude's matcher reads. Be specific —
   listing trigger phrases verbatim helps the match.
3. Keep the body focused on **one decision context**. If it sprawls into
   2–3 unrelated topics, split it.
4. Reference the actual policy source (ADR, CLAUDE.md, BEHAVIOR.md). The
   skill is just a more-accessible re-statement of an existing rule.
5. End with **"See also"** linking to the canonical source.

## Conventions

- Naming: `uclaw-<topic>` for uClaw-specific skills. Other prefixes
  (`gitnexus-*`, `superpowers:*`) are external skill packs.
- Length: aim for 80–150 lines. Longer = skill is doing too much.
- Code examples > prose explanation, when both convey the same thing.
- Reference real file paths (`src-tauri/src/db/migrations.rs`), not
  abstractions ("the migrations file").

## External skills (also live here)

`.claude/skills/gitnexus/` ships with the GitNexus CLI install and covers
the deeper code-intelligence drilldowns (exploring, impact-analysis,
debugging, refactoring, guide, cli). See those skills' own READMEs.

The `superpowers:*` family of skills (brainstorming, writing-plans, etc.)
loads from the central Cowork plugin install, not from this directory.

## See also

- `BEHAVIOR.md §3` — Skills as Discrete Context Blocks (Progressive Disclosure)
- `CLAUDE.md` (this repo's root) — references the GitNexus skill list
- `uclaw-upgrade-implementation-plan.md` §23 — 10 core practices
