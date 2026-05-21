# Borrowed Skills — Attribution

Skills in this directory are vendored from multiple upstream sources
(see per-skill rows below). The original prompt bodies are kept
verbatim where possible to ease re-vendoring; uClaw-specific
divergences are documented in `## Frontmatter / body divergence`
near the bottom of this file.

## Upstream sources

- [mattpocock/skills](https://github.com/mattpocock/skills) — MIT
  License, upstream commit `f304057d` (2026-05-13).
- [anthropics/skills](https://github.com/anthropics/skills) — Apache
  License 2.0, vendored 2026-05-21 (Bundle 21-C).
- [vercel-labs/skills](https://github.com/vercel-labs/skills) —
  permissive (see upstream `ThirdPartyNoticeText.txt`), vendored
  2026-05-21 (Bundle 21-C).

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
| `write-a-skill` | productivity/write-a-skill (mattpocock) | Scaffold for authoring new skills |
| `skill-creator` | anthropics/skills/skill-creator | Full skill development lifecycle: draft → test → benchmark → iterate. Includes eval-viewer scripts (Python; optional and currently not wired into uClaw runtime). |
| `find-skills` | vercel-labs/skills/find-skills | Discover and install skills from skills.sh / GitHub. **uClaw fork**: original SKILL.md drives the standalone `npx skills` CLI; the bundled copy points at uClaw's built-in `skill_marketplace_search` and `skill_install_from_marketplace` tools instead. |

## Licenses

- mattpocock/skills — MIT (see upstream `LICENSE`)
- anthropics/skills — Apache 2.0 (see `skill-creator/LICENSE.txt`)
- vercel-labs/skills — permissive, see upstream
  `ThirdPartyNoticeText.txt`

## Frontmatter / body divergence

| Skill | Upstream-modified? | Why |
| --- | --- | --- |
| `tdd`, `diagnose`, `zoom-out` | added `activation.tags` block | per-workspace scoping under V19 `spaces.skill_tags` (PR #126) |
| `find-skills` | full body rewrite | upstream targets `npx skills` CLI; uClaw uses built-in tools instead. See file header. |
| `write-a-skill` | added `## Output` section | tells the agent to use `skill_write` instead of generic file write, and explains user vs project scope (Bundle 21-B) |
| `skill-creator` | unchanged | upstream prompt body works as-is; the `eval-viewer/generate_review.py` step it references requires Python + a browser, both optional in uClaw |

Re-vendoring procedure: for each upstream-unmodified skill, `git
diff` the vendored copy against the upstream HEAD and resolve any
real conflicts. For the three uClaw-modified skills, apply the
listed delta on top of a fresh upstream snapshot.
