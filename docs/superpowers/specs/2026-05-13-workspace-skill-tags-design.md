# Per-Workspace Skill Tag Scoping Design

**Date:** 2026-05-13
**Status:** Approved (Architecture brief Model C, action tier #3)
**Implements:** Item #3 from the skill bundling / scoping architecture brief

## Motivation

After items #1 (3-tier bundling) and #2 (active manifest panel) land, users can see what's loaded and where it came from, but they still can't say *which* skills should be loaded for *which* workspace. As skill counts grow (~50 → ~200 over time per heavy user), workspaces start to bleed into each other:

- A `tdd` skill cited in the engineering workspace surfaces in a research workspace
- A `stock-source-fallback` learned skill blocks a more relevant general skill from the top-30

The manifest top-30 ranks globally by E3 (cited × recency + usage). Cross-workspace noise increases as users accumulate skills.

## Approved Approach: Tag Intersection

From the architecture brief, **Model C** was chosen over alternatives:

- ❌ Model A (no filtering) — works for ~50 skills but doesn't scale
- ❌ Model B (explicit allowlist) — burdens users with maintenance every time a new skill extracts
- ✅ **Model C (tag scoping)** — opt-in, backwards-compatible, recommendation-friendly
- ❌ Model D (exclusion list) — doesn't solve "what's loaded" question

### Semantic Rules

1. **Skills carry tags** in their existing `activation.tags` field (already populated for `borrowed/*` and built-in skills; defaults to `[]` for learned skills today).
2. **Workspaces carry tags** in a new `spaces.skill_tags` JSON column (new in V19; defaults to `[]`).
3. **Filter rule** (applied at manifest-build time only — not at `skill_search` or slash command):
   - If `workspace.skill_tags` is empty → no filter applied (current behavior, default for all existing workspaces).
   - If `workspace.skill_tags` is non-empty:
     - Skill with empty `tags` → **included** (untagged = global, like a default builtin).
     - Skill with non-empty `tags` → included **iff** `tags ∩ workspace.skill_tags ≠ ∅`.

### Why this rule

- **Backwards-compat by default**: existing workspaces have empty `skill_tags` → manifest behaves exactly as before V19. No surprises.
- **Untagged skills are global**: a freshly-extracted learned skill (no tags yet) appears in every workspace until the user decides to tag it. This is the right cold-start behavior — you don't want learned skills hidden because the workspace happens to have tags.
- **Skills with tags get scoped**: bundled skills come pre-tagged (`tdd` → `["process", "engineering"]`); they only surface in workspaces that share at least one tag.
- **No migration scary-ness**: V19 adds a single nullable column with a default value. Existing rows get `[]` automatically.

### What scoping does NOT change

- **`skill_search` tool**: still returns hits across all stages + tags. The agent can find any skill via search regardless of workspace tags. Filtering is only for *auto-injection*.
- **`/skill-name` slash commands**: still work regardless of workspace tags — explicit user invocation always wins.
- **Learned-skill citation counting**: cited_count + auto-promotion logic untouched.

## Implementation surface

### Database

V19 migration:

```sql
ALTER TABLE spaces ADD COLUMN skill_tags TEXT NOT NULL DEFAULT '[]';
```

`skill_tags` stores a JSON array of lowercased strings, e.g. `["engineering", "process"]`. Conform to existing pattern (V17 added `spaces.attached_dirs` as JSON text).

### Backend

**Two new IPCs:**

```rust
get_workspace_skill_tags(space_id: String) -> Vec<String>
set_workspace_skill_tags(space_id: String, tags: Vec<String>) -> ()
```

`set_*` normalizes (lowercase + trim + dedup) before writing.

**Manifest filter** (in `skills_manifest::collect_entries`):
- Read `spaces.skill_tags` for the active workspace.
- If empty, pass everything through (current behavior).
- If non-empty, apply intersection filter to both static (`SkillsRegistry::list_enabled`) and learned (`list_promoted_learned_skills`) entries.
- Skills with empty `activation.tags` (static) or empty `metadata.tags` (learned) bypass the filter — they're global.

**Helper signature change:** `collect_entries` currently doesn't accept workspace tags. Either:
- (a) Add a `workspace_tags: Option<&[String]>` parameter to `collect_entries`, OR
- (b) Look up workspace tags inside `collect_entries` via a new helper.

Going with (a): keeps `skills_manifest` decoupled from `MemoryGraphStore` schema knowledge; the caller (`tauri_commands::send_agent_message`) resolves the workspace's tags and passes them in.

### Frontend

Settings → Workspace gets a new section "技能标签 (可选)":
- Multi-select chip input — existing UI primitive if one exists, otherwise a textarea with comma-separated tags.
- Empty state explains the default-global behavior.
- Live preview: "当前激活 N 个技能" recomputed via the active-manifest IPC after each save.

Backend round-trip: `getWorkspaceSkillTags(spaceId)` on workspace settings load; `setWorkspaceSkillTags(spaceId, tags)` on save.

## Migration registry update

V18 = agent_sessions.pinned_at (already merged).
**V19** = spaces.skill_tags (this PR).
V20 reserved for next claimant.

## Out of scope (deferred)

- **Automatic tag suggestion** for extracted learned skills — could LLM-classify on extraction, but adds latency + complexity. Today the user tags them via the Settings learned-skill row.
- **Tag taxonomy / hierarchy** — flat string set for now. If "engineering" + "engineering-frontend" feel needed, that's a future refactor.
- **Per-skill toggle in workspace UI** — Model B style. Tag intersection covers 90% of the use case; explicit allow/deny is a future tier if users ask.

## Success criteria

- Existing workspaces (V19+ migration runs cleanly) keep showing the same manifest.
- A workspace tagged `["engineering"]` with the `borrowed/tdd` skill tagged `["process", "engineering"]` includes it.
- A workspace tagged `["research"]` with `tdd` tagged `["process", "engineering"]` excludes it.
- A workspace tagged anything with an untagged skill (empty `activation.tags`) still includes it.
- Active manifest panel reflects the filtered set in real time.
