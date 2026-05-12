# Per-Workspace Skill Tag Scoping — Implementation Plan

**Spec:** [`docs/superpowers/specs/2026-05-13-workspace-skill-tags-design.md`](../specs/2026-05-13-workspace-skill-tags-design.md)
**Branch:** `claude/workspace-skill-tags`
**Migration:** V19
**Target:** 5 bisectable commits in one PR

## Task 1: V19 migration

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` — add `V19_SPACES_SKILL_TAGS` const + runner block

**Content:**
```sql
ALTER TABLE spaces ADD COLUMN skill_tags TEXT NOT NULL DEFAULT '[]';
```

**Runner pattern:** identical to V17/V18 — split by `;`, log skip on error (handles re-runs).

**Update CLAUDE.md migration registry table:** V19 row, `skill_tags` for per-workspace skill scoping.

**Test:** unit test that runs all migrations on an in-memory db, then queries `pragma table_info(spaces)` and asserts `skill_tags` column exists with default `'[]'`.

## Task 2: Workspace tags read/write helpers

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` — add two IPCs.

**Functions:**
```rust
#[tauri::command]
pub async fn get_workspace_skill_tags(state, space_id) -> Result<Vec<String>, Error>

#[tauri::command]
pub async fn set_workspace_skill_tags(state, space_id, tags: Vec<String>) -> Result<Vec<String>, Error>
```

`set_*` normalizes: trim, lowercase, drop empty, dedup (preserve insertion order). Returns the normalized set so the frontend can show what was stored.

**Register both in main.rs.**

**Tests:**
- `set_then_get_round_trips` — write `["Engineering", " process "]`, read back `["engineering", "process"]`.
- `set_dedups_and_drops_empty` — `["a", "A", "", "  ", "a"]` → `["a"]`.
- `get_returns_empty_for_unset_workspace` — fresh workspace with default → `[]`.

## Task 3: Manifest filter wiring

**Files:**
- Modify: `src-tauri/src/skills_manifest.rs` — extend `collect_entries` + `compute_active_manifest_entries` + `build_skills_manifest` to accept optional workspace tags.
- Modify: `src-tauri/src/tauri_commands.rs` — `send_agent_message` resolves workspace tags before calling `build_skills_manifest`; `list_active_manifest_skills` does the same.

**Filter logic** (inside `collect_entries`):
```rust
fn skill_matches_workspace(skill_tags: &[String], workspace_tags: &Option<Vec<String>>) -> bool {
    let Some(ws) = workspace_tags else { return true };
    if ws.is_empty() { return true; }
    if skill_tags.is_empty() { return true; }
    skill_tags.iter().any(|t| ws.contains(t))
}
```

Applied to both static (`SkillsRegistry::list_enabled().activation.tags`) and learned (`metadata.tags` if present, else empty). Tag comparison is case-sensitive — normalization happened at write time.

**Tests:**
- `manifest_includes_untagged_skill_in_tagged_workspace` — workspace `["engineering"]`, skill empty tags → included.
- `manifest_filters_by_tag_intersection` — workspace `["engineering"]`, skill `["engineering", "process"]` → included; skill `["research"]` → excluded.
- `manifest_no_filter_when_workspace_untagged` — workspace `[]`, all skills appear regardless of tags.

## Task 4: Frontend tag editor

**Files:**
- Modify: `ui/src/lib/types.ts` — no new type, `string[]` works.
- Modify: `ui/src/lib/tauri-bridge.ts` — add `getWorkspaceSkillTags(spaceId)` and `setWorkspaceSkillTags(spaceId, tags)`.
- Modify: `ui/src/components/settings/WorkspaceSettings.tsx` (or equivalent — find the actual file) — add a section.

**Component:** simple chip input with comma-or-Enter to add a tag, ✕ to remove. Empty state placeholder: "未设标签 = 所有 Skill 都可见（默认）". Live count of active manifest skills below.

**Test:** Vitest case for the chip input — add/remove flow.

## Task 5: Wire the filter through send_agent_message

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` — `send_agent_message` resolves the workspace's tags before `build_skills_manifest`.

This is the integration point — without this, the V19 column is set but nothing reads it.

**Test:**
- Add a smoke test that exercises `send_agent_message` (or a smaller fn it calls into) and verifies the manifest reflects workspace tags. Since `send_agent_message` is hard to unit-test as a whole, a focused test on `collect_entries(workspace_tags=Some(...))` from Task 3 covers the core logic.

## Bisectability

Each commit compiles and tests pass. Commit 1 is data-only (column add). Commit 2 adds IPCs that read/write the column but nothing uses them yet. Commit 3 adds the filter helper but doesn't wire it in. Commit 4 adds frontend UI but it talks to existing IPCs. Commit 5 wires the filter into the production path. So `git bisect` after a regression can isolate "is the filter wrong" vs "is the IPC broken" vs "is the UI broken".

## Rollback

If V19 needs reverting:
- `ALTER TABLE spaces DROP COLUMN skill_tags;` — SQLite supports this in 3.35+ (Tauri ships 3.x).
- Frontend code paths gracefully handle missing column (the IPC returns `Vec<String>` — empty on error).

But realistically a fwd-only migration is the expected mode.
