# Milestone Closed Loop

Use this when the user mentions "推进主线", "continue main line", M2/M3/M4/M5+
work, C1/C2/C3, Bundle wire-up, milestone closeout, next slice, or queue-next
work.

## Required Loop

1. Read `docs/superpowers/MILESTONE_STATUS.md` before any code edits.
2. Run `./scripts/milestone-drift-check.sh --since "1 week ago"` and surface
   any RED/YELLOW alarm in the first status update.
3. Load `.claude/skills/uclaw-milestone-closed-loop/SKILL.md`.
4. Tag every PR with one of `[M<N>-T<X>]`, `[M<N>-T<X> wire-up]`,
   `[Bundle <N>]`, `[Phase 0.5-T<X>]`, or `[Backlog]`.
5. Update `docs/superpowers/MILESTONE_STATUS.md` as part of the PR that moves
   milestone state.

Spec-first for wire-up: look in `docs/superpowers/specs/` for an existing
spec; if absent, write one before opening a `prep/` branch.

Strategy reference:
`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`.
