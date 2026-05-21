---
name: find-skills
description: Helps users discover and install agent skills when they ask questions like "how do I do X", "find a skill for X", "is there a skill that can...", or express interest in extending capabilities. This skill should be used when the user is looking for functionality that might exist as an installable skill.
---

# Find Skills

This skill helps you discover and install skills from the open agent
skills ecosystem (skills.sh / GitHub) into uClaw.

> This is uClaw's adaptation of [vercel-labs/skills/find-skills](https://www.skills.sh/vercel-labs/skills/find-skills).
> The upstream version drives the standalone `npx skills` CLI. uClaw
> wires the same workflow through two built-in tools:
> `skill_marketplace_search` (discovery) and
> `skill_install_from_marketplace` (install). See `## uClaw Tools`
> below for the mapping.

## When to Use This Skill

Use this skill when the user:

- Asks "how do I do X" where X might be a common task with an existing skill
- Says "find a skill for X" or "is there a skill for X"
- Asks "can you do X" where X is a specialized capability
- Expresses interest in extending agent capabilities
- Wants to search for tools, templates, or workflows
- Mentions they wish they had help with a specific domain (design, testing, deployment, etc.)

**Always check the local registry FIRST** with `skill_search` before
searching the marketplace — a uClaw user often already has the right
skill installed.

## uClaw Tools

| Upstream `npx skills` command | uClaw built-in tool |
| --- | --- |
| `npx skills find <query>` | `skill_marketplace_search` |
| `npx skills add <owner/repo@skill>` | `skill_install_from_marketplace` |

These two tools do exactly what the upstream CLI does — query
GitHub for SKILL.md files, fetch + install a specific skill — but
don't need a separate `npx` process on the user's machine.

## How to Help Users Find Skills

### Step 1: Understand What They Need

When a user asks for help with something, identify:

1. The domain (e.g., React, testing, design, deployment)
2. The specific task (e.g., writing tests, creating animations, reviewing PRs)
3. Whether this is a common enough task that a skill likely exists

### Step 2: Check Local Skills First

Before going to the network, run `skill_search` against the local
SkillsRegistry. uClaw ships with `~/.uclaw/skills/`, bundled skills,
and any previously-installed marketplace skills. A hit here is
zero-cost and usually higher quality than a fresh marketplace pick.

### Step 3: Search the Marketplace

If local doesn't cover the need, search the marketplace:

```
Call: skill_marketplace_search
Args:
  query: "<keywords describing what the user wants>"
  limit: 8   # default; max 20
```

Examples:

- User asks "how do I make my React app faster?"
  → `skill_marketplace_search query="react performance"`
- User asks "can you help me with PR reviews?"
  → `skill_marketplace_search query="pr review code"`
- User asks "lunar to solar date conversion"
  → `skill_marketplace_search query="lunar calendar conversion"`

The result includes `slug`, `repo`, `path`, `stars`, `htmlUrl`, and an
`installSource` field that you can pass directly to step 5.

### Step 4: Verify Quality Before Recommending

**Do not recommend a skill based solely on search results.** Always check:

1. **Stars** — Prefer repos with 100+ stars. Treat anything under 10 with caution.
2. **Source reputation** — Official sources (`anthropics/`, `vercel-labs/`, well-known organizations) are more trustworthy than personal accounts.
3. **Path sanity** — `installSource` should point at a directory containing SKILL.md, not at random files.

### Step 5: Present Options to the User

When you find relevant skills, summarize them as:

```
I found a few candidate skills:

1. **anthropics/skills/frontend-design** (132K ★)
   Build production-grade frontend UI with high design quality.

2. **vercel-labs/skills/react-best-practices** (19K ★)
   React and Next.js performance optimization guidelines.

Would you like me to install #1?
```

### Step 6: Install (Requires User Approval)

If the user confirms, call:

```
Call: skill_install_from_marketplace
Args:
  source: "<installSource from the chosen result>"
  ref:    "main"            # optional, default "main"
  force:  false             # default; pass true to overwrite existing
```

**This tool always requires user approval** — it fetches third-party
code from GitHub and persists it under
`~/.uclaw/skills/_marketplace/<owner>__<repo>__<slug>/`. The uClaw
SafetyManager surfaces an approval dialog automatically.

After install:
- The skill is auto-registered in `SkillsRegistry`
- It's immediately discoverable by `skill_search` in the same session
- A `agent:skill-installed` event fires for the UI

## Common Skill Categories

When searching, consider these common categories:

| Category        | Example Queries                          |
| --------------- | ---------------------------------------- |
| Web Development | react, nextjs, typescript, css, tailwind |
| Testing         | testing, jest, playwright, e2e           |
| DevOps          | deploy, docker, kubernetes, ci-cd        |
| Documentation   | docs, readme, changelog, api-docs        |
| Code Quality    | review, lint, refactor, best-practices   |
| Design          | ui, ux, design-system, accessibility     |
| Productivity    | workflow, automation, git                |
| Calendar / Locale | lunar, solar, timezone, i18n           |

## Tips for Effective Searches

1. **Use specific keywords**: "react testing" is better than just "testing"
2. **Try alternative terms**: If "deploy" doesn't work, try "deployment" or "ci-cd"
3. **Check popular sources**: Many skills come from `anthropics/skills`,
   `vercel-labs/skills`, or `obra/superpowers`
4. **Browse skills.sh in a browser** for richer listings + install counts.
   You can paste a skills.sh URL like
   `https://skills.sh/anthropics/skills/skill-creator` and uClaw will
   parse the `owner/repo/path` from it for install.

## When No Skills Are Found

If no relevant skills exist:

1. Acknowledge that no existing skill was found
2. Offer to help with the task directly using your general capabilities
3. Suggest the user could **create their own skill** — point them at the
   `write-a-skill` skill (uClaw built-in) which guides authoring +
   uses `skill_write` to save the new SKILL.md to the right place
4. Optionally walk them through the `skill-creator` flow if they
   want a more rigorous iteration/evaluation loop (uClaw bundled)

Example:

```
I searched skills.sh for "<query>" but didn't find a strong match.

I can help you with this task directly using my general capabilities.

If this is something you'll do often, I can:
- Author a fresh skill with you using write-a-skill (quick start)
- Or run the full skill-creator workflow (iterative draft → test →
  benchmark) for a more polished skill
```
