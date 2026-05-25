# Domain Docs

uClaw is a single-context repo for mattpocock/skills.

## Before Exploring

Read these when the task needs product language, architecture context, or prior
decisions:

- `CONTEXT.md` for repo architecture, build commands, gotchas, and the active
  migration registry.
- `docs/adr/` for relevant architectural decisions.
- `BEHAVIOR.md` for cross-agent workflow rules.

If a task is narrow and the relevant files are obvious, read only the sections
needed for that task. Prefer progressive disclosure over loading every policy
file into context.

## Vocabulary

Use the domain terms already present in `CONTEXT.md` and the relevant ADRs.
If a new term is needed, add or propose it in the smallest relevant context
document instead of inventing parallel vocabulary.

## ADR Conflicts

If a proposed change contradicts an ADR, surface the conflict explicitly and
ask whether the ADR should be superseded.
