# Borrowed Skills — Attribution

Skills in this directory are vendored from [mattpocock/skills](https://github.com/mattpocock/skills)
(MIT License) at upstream commit `f304057d` (2026-05-13).

Each `SKILL.md` keeps the upstream **prompt body** verbatim so future re-borrows
can diff cleanly.

**Frontmatter divergence (documented):** As of 2026-05-13, an `activation.tags`
block was added to the domain-specific skills (`tdd`, `diagnose`, `zoom-out`)
to enable per-workspace scoping under the V19 `spaces.skill_tags` filter
introduced in PR #126. The change is **purely additive** — the original
`name` / `description` / `disable-model-invocation` fields are untouched, and
re-vendoring only requires re-adding the same tag block per skill. The
cross-domain skills (`caveman`, `handoff`, `grill-me`, `write-a-skill`) are
intentionally left untagged so they remain globally available in every
workspace per V19's "untagged = global" rule.

Per uClaw's `CLAUDE.md`, these skills are subject to uClaw's own `SkillsRegistry`
discovery and citation tracking — same as builtin skills under `skills/writing-assistant/`.

## Skills

| Name | Source | Purpose |
|---|---|---|
| `diagnose` | engineering/diagnose | Disciplined bug diagnosis loop (reproduce → minimise → hypothesise → instrument → fix) |
| `tdd` | engineering/tdd | Red-green-refactor with vertical slices |
| `zoom-out` | engineering/zoom-out | Pull-perspective directive |
| `handoff` | productivity/handoff | Cross-session compaction via `mktemp` |
| `grill-me` | productivity/grill-me | Plan stress-test via Socratic interview |
| `caveman` | productivity/caveman | Ultra-compressed communication mode |
| `write-a-skill` | productivity/write-a-skill | Scaffold for authoring new skills |

## License

MIT — see `LICENSE` in the upstream repo.
