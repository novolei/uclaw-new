# mattpocock-borrowed Skills Authoring + Slash Command Invocation

**Date:** 2026-05-13
**Status:** Draft (pending writing-plans)
**Builds on:** PR #103 (skill-recall closed loop), PR #112 (EvoMap enhancements), PR #114 (tri-tier scoring)
**Inspired by:** [mattpocock/skills](https://github.com/mattpocock/skills) (architectural study, see chat thread)

## Background

PRs #103/#112/#114 built uClaw's skill-recall infrastructure (extract → store → manifest → search → cite → rerank). The infrastructure works, but **content quality** is the next bottleneck: extracted skills are middling because the extraction prompt produces 5-dimension bullet lists rather than opinionated single-purpose procedures.

mattpocock/skills (75k stars) is the highest-rated public skill collection. Its core insight: **good prompts are opinionated authored content**. Each skill targets one specific agent failure mode, leads with anti-patterns, uses imperative voice, ends with a checklist.

This spec adopts that authoring philosophy without forcing uClaw onto Claude Code's invocation model.

## Goals

1. **Improve extraction quality**: rewrite `skill_extraction.rs` system prompt to bias toward single-purpose skills with anti-patterns, hard description constraints, and prioritized output.
2. **Seed catalog**: ship 7 hand-curated borrowed skills from mattpocock that fit uClaw's agent loop.
3. **User invocation**: enable `/skill-name` slash commands in chat input — type `/diagnose` → that skill's body gets injected into the next LLM call.
4. **Lifecycle gate**: prevent untested LLM-extracted skills from drowning out curated builtins in the manifest top-30.
5. **Authoring guide**: document the new "good skill" shape for both LLM extractor and human contributors.

## Non-goals

- Adopting Claude Code's plugin manifest protocol (uClaw frontmatter has its own program-driven fields)
- Forking or git-submoduling mattpocock/skills (we want a curated subset, not an upstream mirror)
- Adopting collaboration-heavy skills (grill-with-docs, to-prd, to-issues, triage — assume team/issue-tracker context uClaw doesn't have)
- Multi-user skill sharing / Hub

## The 5 deliverables

### 1. Layer A — Extraction prompt rewrite

Modify `src-tauri/src/proactive/scenarios/skill_extraction.rs::SKILL_EXTRACTION_SYSTEM_PROMPT`:

**Current (5-dimension flat list)**:
> "## 分析维度: 1. 成功模式 2. 失败教训 3. 新技能指南 4. 优化建议 5. 工具使用模式"

**New (prioritized + anti-patterns + description constraint)**:
- Lead: "**核心任务是抽取新 skill**，其他四个维度是辅料；没有就跳过"
- Hard `description` constraint: 一句话、第三人称、"Use when..." 风格、≤120 字符
- New `## 反模式` section listing concrete noise patterns to skip:
  - 重复修复同一个 bug 仍当作新技能
  - 通用建议 ("多用 try-catch" / "仔细阅读文档")
  - description 写成 "Helps with X" / 含义模糊
  - 过度泛化的成功模式（"勤总结"）
- New `<anti_patterns>` XML element on each skill, mandatory if the skill makes claims about what NOT to do (LLM fills it; empty is OK)
- "Good skill checklist" the LLM self-audits against before output

Existing fields (signals, signals_seen, validation_hint, category) preserved unchanged.

### 2. Layer B-1 — Borrowed skills

Add `skills/borrowed/<name>/SKILL.md` for 7 hand-picked skills from mattpocock/skills (MIT-licensed with attribution):

| Name | Why it fits uClaw |
|---|---|
| `diagnose` | uClaw lacks standardized bug diagnosis flow; Phase 1 "build a feedback loop" fits shell+file+web tools |
| `tdd` | `cargo test` / `npm test` are high-frequency uClaw actions; vertical slice discipline |
| `zoom-out` | uClaw has no "agent went too deep, pull view" command; small body, high leverage |
| `handoff` | Complements existing `/compact`; mktemp file-based cross-session continuity |
| `grill-me` | 4-line minimalist plan-stress-test; high-frequency user intent |
| `caveman` | Token-saving mode entry point; pairs with 1M context badge |
| `write-a-skill` | Scaffold for uClaw users authoring their own builtin skills |

Each `skills/borrowed/<name>/SKILL.md` keeps mattpocock's body verbatim (faithful borrow), prepended with a 3-line frontmatter:

```yaml
---
name: <name>
description: <one-line from upstream + Use when... clause>
source: mattpocock/skills @ <commit-sha>
license: MIT
---
```

uClaw's existing `SkillsRegistry::discover` already scans `*/SKILL.md` — zero code changes for the registry.

### 3. Layer B-2 — Lifecycle field + auto-promotion

Add `metadata.lifecycle: "draft" | "promoted" | "deprecated"` to `memory_nodes`:

- Newly extracted skills default to `draft`
- Auto-promote when `cited_count >= 3` (criterion: agent has applied it 3+ times)
- Manual demote available via Settings → 已学技能 → ... menu
- `list_top_learned_skills` (manifest source) filters to `promoted` only — draft skills NEVER enter the top-30 system prompt
- `skill_search` tool **does** see drafts (so agent can discover them) but the LLM sees a `[draft]` tag in summary to know they're unvalidated

Migration: existing learned skills (pre-lifecycle) are treated as `promoted` (grandfathered). New skills go `draft → promoted` per the rule above.

### 4. Layer NEW — User-invoked slash commands

User types `/skill-name` in agent input → that skill's full body gets injected. Two implementation pieces:

**Backend** (`src-tauri/src/tauri_commands.rs::send_agent_message`):

Add `/skill-name` intercept similar to existing `/compact` intercept. When `input.user_message` starts with `/<name>`:

1. Look up via `SkillsRegistry::match_slash_command(name)` for builtin/borrowed
2. Fall back to learned skill lookup by normalized title
3. If not found: pass through as regular message (graceful — user might have a typo or be discussing slash syntax)
4. If found: inject the skill's full prompt body as a `<skill_invocation>` block prepended to the user's actual content (the part after the slash command). The skill is bumped via `bump_skill_usage`.

**Frontend** (`ui/src/components/agent/AgentInput.tsx` or wherever the input lives):

Slash command autocomplete dropdown:
- Trigger on leading `/` in empty/start-of-line position
- Lists matching skills (name + 60-char description)
- Sources: builtin + borrowed + promoted learned (not drafts — same gate as manifest)
- Keyboard nav (arrow + Tab/Enter to select)
- Selection inserts `/<name> ` (with trailing space, ready for user to type the argument)
- New IPC `list_invocable_skills()` returning `[{name, description, provenance}]` — wraps existing SkillsRegistry::list + memory_graph learned skills filtered by lifecycle

### 5. Layer B-3 — Authoring guide

Create `docs/skill-authoring-guide.md`. Single document, ~200 lines, derived from mattpocock's `write-a-skill/SKILL.md` checklist + uClaw-specific frontmatter requirements (keywords, parameters, etc.).

Audience: human contributors + new builtin skill authors. Sections:
- Philosophy ("opinionated content beats neutral templates")
- Frontmatter cheat sheet (uClaw's program-driven fields)
- Body structure template
- Anti-patterns (concrete examples from uClaw history)
- Self-audit checklist

Not user-facing — internal `docs/` doc.

## Architecture summary

| Item | Where | New code? |
|---|---|---|
| Layer A: prompt rewrite | `proactive/scenarios/skill_extraction.rs` (prompt text only) | minimal |
| Layer B-1: borrowed skills | `skills/borrowed/*/SKILL.md` | 0 code (registry already scans) |
| Layer B-2: lifecycle + auto-promote | `memory_graph/store.rs` + `proactive/scenarios/skill_extraction.rs` + `tauri_commands.rs::record_skill_cited` | moderate |
| Layer NEW: slash command | `tauri_commands.rs::send_agent_message` + frontend `AgentInput` slash-autocomplete + new `list_invocable_skills` IPC | larger |
| Layer B-3: authoring guide | `docs/skill-authoring-guide.md` | docs only |

**No SQL migrations.** lifecycle goes in `metadata_json` like other PR #112 additions.

## Implementation as 5 sequential PRs

| # | Scope | Est. LOC | Files |
|---|---|---|---|
| 1 | Layer A: prompt rewrite | ~150 | 1 (+ tests) |
| 2 | Layer B-1: 7 borrowed skills | ~600 | 7 new files (content) |
| 3 | Layer B-2: lifecycle + auto-promotion | ~300 | 3-4 files |
| 4 | Layer NEW: slash command | ~450 | 5-6 files (frontend + backend) |
| 5 | Layer B-3: authoring guide | ~200 | 1 doc |

Total ~1,700 LOC; 5 PRs. Sequential because:
- PR 2 quality is improved by PR 1's prompt being deployed first (so users can verify the new shape with real extractions)
- PR 4 (slash command) reads lifecycle to filter dropdown (depends on PR 3)
- Other dependencies are loose

## Failure modes

| Scenario | Behavior |
|---|---|
| Extraction LLM produces fewer skills under stricter prompt | OK — quality > quantity is the explicit goal |
| User types `/typo` that doesn't match any skill | Pass through as regular message; agent treats it as ordinary text |
| User types `/diagnose my actual bug` | Skill body injected, "my actual bug" treated as the actual user content |
| Skill body contains `/` itself | Argument parsing must only split on first whitespace, not first `/` |
| Learned skill at `lifecycle: draft` cited 3 times → auto-promote | `cited_count` write triggers a promotion check; updates lifecycle to "promoted" atomically with the cited bump |
| User manually demotes a promoted skill | Sets lifecycle: "deprecated"; manifest immediately excludes; skill_search still returns with `[deprecated]` warning |
| Two skills with same name (builtin + learned) | Builtin takes precedence in slash dispatch (consistent with `SkillsRegistry::match_slash_command` semantics) |
| Slash autocomplete dropdown listing 100+ skills | Cap displayed at 10; filter by user's typed prefix; show "+N more" hint |

## Philosophy check

| uClaw principle | Compatibility |
|---|---|
| probe-first | Slash command is **user-explicit** invocation, not auto-injection — fully aligned |
| local-first | All borrowed content is MIT, copied into repo; zero network deps |
| no SQL migration | lifecycle is metadata JSON — same pattern as PR #112 |
| manifest compactness | lifecycle filter REDUCES manifest noise (drafts excluded) — improves alignment |
| no Claude Code plugin protocol | We re-implement slash dispatch in our agent dispatcher — `match_slash_command` was already there as orphan code, we wire it up |
| MIT attribution honored | Each borrowed `SKILL.md` keeps its `source:` frontmatter pointing to upstream |

## Test coverage

- PR 1: extraction LLM output schema unchanged (existing tests pass); add 1-2 prompt-shape assertions
- PR 2: registry discovers all 7 borrowed skills (new helper test); plus content-format lint (frontmatter present)
- PR 3: lifecycle write on extraction; auto-promotion at cited_count=3; manifest filter excludes draft; backfill grandfathers existing rows
- PR 4: slash intercept in send_agent_message (test with mock skill); list_invocable_skills returns correct set; autocomplete component (vitest cases)
- PR 5: docs only — no automated test

## Out-of-scope, deferred

- **Sidecar refs (Matt's tdd/tests.md style)**: skill body has tightly-coupled supporting docs. Could add via `parent_skill_id` in memory_graph later. Not in this PR series.
- **User-authored learned skills**: today learned skills come only from proactive extraction. Letting a user manually author one (via Settings UI) is a separate UX concern.
- **Slash command argument parsing**: v1 just splits on first whitespace. More sophisticated argument routing (`/diagnose --verbose` etc.) deferred.
- **Skill marketplace / sharing**: out of scope per privacy/local-first stance.
- **mattpocock skill auto-updates**: borrowed skills are vendored at a specific commit; no upstream sync mechanism. If upstream changes substantially, manually re-borrow.

## Done definition

This spec is "done" when:
1. All 5 PRs merged to main
2. New `skill_extraction.rs` prompt deployed and produces visibly-different-shape output on a real session
3. 7 borrowed skills available via `skill_search` and slash autocomplete
4. lifecycle field auto-promotes a draft → promoted after 3 cites in dogfood
5. Typing `/diagnose` in agent input invokes that skill end-to-end
6. `docs/skill-authoring-guide.md` checked in
