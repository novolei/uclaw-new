---
name: write-a-skill
description: Create new agent skills with proper structure, progressive disclosure, and bundled resources. Use when user wants to create, write, or build a new skill.
---

# Writing Skills

## Process

1. **Gather requirements** - ask user about:
   - What task/domain does the skill cover?
   - What specific use cases should it handle?
   - Does it need executable scripts or just instructions?
   - Any reference materials to include?

2. **Draft the skill** - create:
   - SKILL.md with concise instructions
   - Additional reference files if content exceeds 500 lines
   - Utility scripts if deterministic operations needed

3. **Review with user** - present draft and ask:
   - Does this cover your use cases?
   - Anything missing or unclear?
   - Should any section be more/less detailed?

## Skill Structure

```
skill-name/
├── SKILL.md           # Main instructions (required)
├── REFERENCE.md       # Detailed docs (if needed)
├── EXAMPLES.md        # Usage examples (if needed)
└── scripts/           # Utility scripts (if needed)
    └── helper.js
```

## SKILL.md Template

```md
---
name: skill-name
description: Brief description of capability. Use when [specific triggers].
---

# Skill Name

## Quick start

[Minimal working example]

## Workflows

[Step-by-step processes with checklists for complex tasks]

## Advanced features

[Link to separate files: See [REFERENCE.md](REFERENCE.md)]
```

## Description Requirements

The description is **the only thing your agent sees** when deciding which skill to load. It's surfaced in the system prompt alongside all other installed skills. Your agent reads these descriptions and picks the relevant skill based on the user's request.

**Goal**: Give your agent just enough info to know:

1. What capability this skill provides
2. When/why to trigger it (specific keywords, contexts, file types)

**Format**:

- Max 1024 chars
- Write in third person
- First sentence: what it does
- Second sentence: "Use when [specific triggers]"

**Good example**:

```
Extract text and tables from PDF files, fill forms, merge documents. Use when working with PDF files or when user mentions PDFs, forms, or document extraction.
```

**Bad example**:

```
Helps with documents.
```

The bad example gives your agent no way to distinguish this from other document skills.

## When to Add Scripts

Add utility scripts when:

- Operation is deterministic (validation, formatting)
- Same code would be generated repeatedly
- Errors need explicit handling

Scripts save tokens and improve reliability vs generated code.

## When to Split Files

Split into separate files when:

- SKILL.md exceeds 100 lines
- Content has distinct domains (finance vs sales schemas)
- Advanced features are rarely needed

## Review Checklist

After drafting, verify:

- [ ] Description includes triggers ("Use when...")
- [ ] SKILL.md under 100 lines
- [ ] No time-sensitive info
- [ ] Consistent terminology
- [ ] Concrete examples included
- [ ] References one level deep

## Output — How to Save the Skill (uClaw / Bundle 21-A)

**ALWAYS use the `skill_write` tool. NEVER use generic file write
or edit to create a SKILL.md.** A SKILL.md written by `edit` ends up
at a path the registry doesn't scan, so it's invisible to future
sessions.

### Scope selection

`skill_write` requires a `scope` argument. Pick by these rules:

- **`scope: "project"` (default)** — the skill is specific to this
  workspace's domain. Examples: "deploy steps for THIS repo's CI",
  "this codebase's preferred lint config", "build pipeline for the
  current monorepo". Auto-approved; lives under
  `<workspace>/.uclaw/skills/<name>/`. Removable via `rm -rf .uclaw/`
  at any time.

- **`scope: "user"`** — the skill is general-purpose and should
  follow the user across all projects. Examples: "lunar↔solar date
  conversion", "format git commit messages in conventional style",
  "draft a polite Chinese email". Requires user approval; lives
  under `~/.uclaw/skills/<name>/`. **Only ask for user scope when
  the user explicitly says** "save as my skill" / "全局技能" / "make
  it available everywhere".

Default to `project` when ambiguous. The user can always promote
later by reinstalling with `scope=user`.

### Argument format

```json
{
  "name": "kebab-case-skill-name",
  "description": "One sentence on what it does. Use when <trigger>.",
  "body": "# Skill Name\n\n## Quick start\n...",
  "scope": "project",
  "references": {
    "REFERENCE.md": "# Reference\n\n...",
    "EXAMPLES.md": "# Examples\n\n..."
  }
}
```

- `name` — kebab-case (lowercase + digits + hyphens). Becomes the
  directory name.
- `description` — ≤1024 chars; the same string you put in the
  frontmatter.
- `body` — markdown body ONLY. **Do NOT include a `---` block** —
  `skill_write` adds the frontmatter from `name` and `description`
  automatically.
- `references` — optional companion files. Filenames must not
  contain path separators.

### After write

`skill_write` automatically:
1. Writes SKILL.md (and any references) to the correct directory
2. Calls `SkillsRegistry::discover()` to rescan — the new skill is
   immediately visible to `skill_search` in the same agent loop
3. Emits an `agent:skill-created` event so the UI shows a confirmation
   chip
4. Returns `{ok, path, registryTotal}` — confirm the path to the
   user

### Finding existing skills first

Before writing a new skill, consider:
1. Search local registry with `skill_search` — there may already be
   a uClaw skill for this
2. If no local match, ask the user "should I check skills.sh for an
   existing skill?" → use `skill_marketplace_search` to query
   GitHub for SKILL.md files matching the task
3. If a good marketplace match exists, install it via
   `skill_install_from_marketplace` (requires user approval)
4. Only fall back to `skill_write` when nothing existing fits
