# Agent Docs Skills Slimming

## Intent

Install the repo-facing mattpocock/skills configuration and reduce duplicated
agent instructions so Claude Code and Codex can load shared repo facts without
being forced through heavyweight process for every small task.

## Scope

- Add `docs/agents/` configuration consumed by mattpocock/skills.
- Add short `## Agent skills` pointers to `AGENTS.md` and `CLAUDE.md`.
- Move the long milestone closed-loop reminder out of `CLAUDE.md` and into a
  dedicated `docs/agents/` reference that points at the existing skill.
- Update `BEHAVIOR.md` to make planning and ADR §18 risk-scaled while keeping
  hard safety, migration, license, and verification guardrails intact.
- Define hard guardrails as stable safety/data/license/parallel-work
  boundaries, separate from risk-scaled workflow defaults.
- Pin GitNexus agent-doc blocks and add a wrapper/CI check so routine index
  refreshes do not rewrite `AGENTS.md` or `CLAUDE.md`.
- Do not change runtime code, hooks, migrations, or app behavior.

## ADR 18 Answers

1. Intent: reduce instruction duplication and make skill-based workflows easier
   for Claude Code and Codex to apply.
2. Autonomy: no runtime autonomy changes; only agent guidance changes.
3. Truth source: `BEHAVIOR.md` remains the canonical policy, with
   `docs/agents/` as skill configuration.
4. TaskEvent: none.
5. Context: root entry files should load less procedural detail by default.
6. Capability: mattpocock/skills can now find issue tracker, triage labels, and
   domain-doc layout.
7. Hooks: no hook behavior changes.
8. Projection: docs-only projection of agent workflow.
9. Harness: markdown diff check plus GitNexus docs-only detect.
10. Rollback: revert this docs commit.
11. Does not own: no changes to code symbols, database migrations, or CI.

## Verification

- `git diff --check -- AGENTS.md CLAUDE.md BEHAVIOR.md docs/agents docs/superpowers/plans/2026-05-25-agent-docs-skills-slimming.md`
- `scripts/verify/gitnexus-agent-docs-pinned.sh`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-docs-skills-slimming`
