# Step 1 — Skill + Tool layers UNCONDITIONAL on bucket_seal (delete memory_graph fallbacks + flags) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make bucket_seal the unconditional store for the **skill** and **tool-memory** layers — delete the gated memory_graph `else`-branches, remove the `skill_store_repoint_enabled` / `tool_memory_repoint_enabled` flags, and repoint the two still-ungated skill tools (`skill_search`, `load_skill`) — so the skill/tool layers are fully on bucket_seal with no rollback flag (the finish-line discipline from ADR `2026-06-01-memory-two-layer-terminal-state.md`).

**Architecture:** Data-safety gate is PASSED (verified against the live DB: bucket_seal already holds 248 skills + 142 tool_stats; memory_graph keeps the copies — non-destructive). So deleting the fallbacks loses nothing. `memory_graph` itself is RETAINED (the rich-structure layer) — this plan touches ONLY the skill/tool gated paths + the two skill tools; it does NOT touch reflection/personality/EntityPages/health, and does NOT touch the gbrain flags (those are Step 2).

**Tech Stack:** Rust, `memory_adapter::skills` + `tool_stats` facades, `BucketSealAdapter`, the proactive/skill/tool subsystems.

---

## Recon findings (complete — ground truth)

- **skills facade** (`memory_adapter/skills.rs`): `put_skill`, `get_skill(adapter, space, slug)`, `top_skills(adapter, space, limit)` (usage-ranked), `bump_cited`, `bump_usage`. **NO keyword search** → Task 1 adds `skills::search`.
- **`skill_search.rs`** (`agent/tools/builtin/`): `struct SkillSearchTool { store: Arc<MemoryGraphStore>, … }`, `new(...)`. Uses `store.list_top_learned_skills(space, 500)` (skill_search.rs:72), `store.search_by_keyword(space, tok)` (:211), `store.bump_skill_usage(&ids)` (:424). Ctor at `registry_build.rs:258`.
- **`load_skill.rs`**: `struct LoadSkillTool { store: Arc<MemoryGraphStore>, … }`, `new(...)`. Uses `store.find_learned_skill_by_normalized_title(space, normalized)` (:100), `store.get_active_version(&node.id)` (:111), `store.bump_skill_usage(&[id])` (:116). Ctor at `registry_build.rs:267`. **Maps cleanly** to `get_skill`/`Skill.body`/`bump_usage` — no facade gap.
- **`tool_memory.rs`** (`ToolUsageMemoryManager`): `repoint_enabled` + `repoint_adapter` fields; `record_tool_usage`/`record_co_usage`/`get_tool_stats` each have `if repoint_enabled { adapter } else { memory_graph }`. Constructed `service.rs:643` `::new(store, Some(adapter), enabled)`.
- **`service.rs`**: `MemoryOsRuntimeConfig` carries `skill_store_repoint_enabled` + `tool_memory_repoint_enabled` (struct + `from_memubot_config` + `Default` + `for_tests`); the skill-write gated `else` (store_skill_as_procedure) is in `ingest_draft_file`/the skill loop (~service.rs:2140–2235).
- **`tauri_commands.rs`**: `get_learned_skill` (~8631 adapter / ~8666 memory_graph else), `record_skill_cited` (~8770 adapter / ~8856 memory_graph else), the slash-command skill-inject memory_graph fallback (~8391). Each gated by `skill_store_repoint_enabled` (read from `state.memubot_config`).
- **`memu_tools.rs`** (`MemuMemoryTool`): `store: Option<Arc<MemoryGraphStore>>` + `with_skill_adapter`/`with_store`; `register_memu_tools` in `registry_build.rs` passes the flag + (when off) the store.
- **`MemoryOsConfig`** (`memubot_config.rs`): both flags as `#[serde(default)] bool` + `default_*` fns + `impl Default` + tests.
- Migration modules + freeze-hook allowlist: **leave** (`skill_migration`/`tool_memory_migration` read memory_graph in prod, write only in test fixtures — keep allowlisted).
- Worktree placeholders (fresh build): `mkdir -p src-tauri/{bunembed,pyembed,gbrain-source}; touch src-tauri/bunembed/bun src-tauri/pyembed/python; echo x > src-tauri/gbrain-source/placeholder.txt`.

## Worktree

`/Users/ryanliu/Documents/uclaw-worktrees/step1-skill-tool-unconditional` on `claude/step1-skill-tool-unconditional` off `origin/main`. Placeholders above; baseline `cargo build` clean before Task 1.

## File structure / tasks

| Task | Files | DONE when |
|---|---|---|
| 1 | `memory_adapter/skills.rs` | `skills::search` exists + tested |
| 2 | `load_skill.rs` + `registry_build.rs` | load_skill reads bucket_seal; no `MemoryGraphStore` in it |
| 3 | `skill_search.rs` + `registry_build.rs` | skill_search reads bucket_seal; no `MemoryGraphStore` in it |
| 4 | `tool_memory.rs` + `service.rs` + `memubot_config.rs` | `tool_memory_repoint_enabled` gone; tool_memory has no memory_graph path |
| 5 | `service.rs` + `tauri_commands.rs` + `memu_tools.rs` + `registry_build.rs` + `memubot_config.rs` | `skill_store_repoint_enabled` gone; gated skill else-branches deleted |
| 6 | — | whole-slice verify |

---

### Task 1: add `skills::search` (keyword search over the skills namespace)

**Files:** Modify `src-tauri/src/memory_adapter/skills.rs`.

- [ ] **Step 1: Write the failing test** (in the existing `#[cfg(test)] mod tests`):

```rust
#[tokio::test]
async fn search_finds_by_keyword_and_scopes_space() {
    let a = InMemoryAdapter::new();
    put_skill(&a, &Skill{slug:"rust-async".into(),space:"default".into(),name:"Rust async".into(),body:"tokio and futures".into(),usage_count:0,cited_count:0,keywords:vec!["tokio".into()],status:"draft".into()}).await.unwrap();
    put_skill(&a, &Skill{slug:"py".into(),space:"default".into(),name:"Python".into(),body:"asyncio".into(),usage_count:0,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    let hits = search(&a, "default", "tokio", 10).await.unwrap();
    assert!(hits.iter().any(|s| s.slug == "rust-async"));
    assert!(!hits.iter().any(|s| s.slug == "py"));
}
```

- [ ] **Step 2: Run → fail** — `cd src-tauri && cargo test --lib memory_adapter::skills::tests::search 2>&1 | tail -8` → `cannot find function search`.

- [ ] **Step 3: Implement** (after `top_skills`):

```rust
/// Keyword search within `space_id` over the skills namespace (name/body/keywords),
/// via the adapter's namespace-scoped recall. Returns up to `limit` matching skills.
pub async fn search(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, query: &str, limit: usize) -> anyhow::Result<Vec<Skill>> {
    let opts = crate::memory_adapter::RecallOpts { namespace: Some(SKILLS_NAMESPACE), ..Default::default() };
    let entries = adapter.recall(query, limit.saturating_mul(2), opts).await?;
    let mut out: Vec<Skill> = entries.into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok())
        .filter(|s| s.space == space_id)
        .collect();
    out.truncate(limit);
    Ok(out)
}
```

(Confirm `RecallOpts`/`SKILLS_NAMESPACE`/`adapter.recall` signatures match the file's existing usage — `recall` is the trait method bucket_seal FTS-backs. The InMemoryAdapter test double's `recall` does substring match, which satisfies the test.)

- [ ] **Step 4: Run → pass**; `cargo build` clean.
- [ ] **Step 5: Commit** — `git add src-tauri/src/memory_adapter/skills.rs && git commit -m "feat(memory_adapter): skills::search — keyword search over skills namespace (Step 1)"`.

---

### Task 2: repoint `load_skill` to bucket_seal

**Files:** `src-tauri/src/agent/tools/builtin/load_skill.rs`, `src-tauri/src/agent/tools/registry_build.rs:267`.

- [ ] **Step 1:** In `LoadSkillTool`, replace `pub store: Arc<MemoryGraphStore>` with `pub skill_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>`; update `new(...)` accordingly; drop the `use crate::memory_graph::store::MemoryGraphStore;`.
- [ ] **Step 2:** Replace the lookup body (load_skill.rs ~100–120):

```rust
    let normalized = crate::proactive::skill_parser::normalize_title_for_dedup(name);
    let skill = match crate::memory_adapter::skills::get_skill(&self.skill_adapter, &self.space_id, &normalized).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err(ToolError::Execution(format!("learned skill not found: {name}"))),
        Err(e) => return Err(ToolError::Execution(format!("get_skill failed: {e:#}"))),
    };
    let body = skill.body.clone(); // was the active version content
    // best-effort usage bump (was bump_skill_usage)
    let _ = crate::memory_adapter::skills::bump_usage(&self.skill_adapter, &self.space_id, &normalized).await;
    // … build the tool output from `skill.name` / `body` as before …
```

(Match the surrounding output-construction; preserve the not-found semantics. `normalize_title_for_dedup` is the slug, same as the migration used.)

- [ ] **Step 3:** `registry_build.rs:267` — pass `Arc::clone(&state.bucket_seal_adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>` instead of the memory_graph store.
- [ ] **Step 4:** Update load_skill's tests (the `fresh_store`/memory_graph fixtures → seed via `skills::put_skill` on an in-memory adapter). `cargo build` + `cargo test --lib load_skill 2>&1 | tail` green.
- [ ] **Step 5:** Commit — `feat(agent): load_skill reads bucket_seal skills facade (Step 1)`.

---

### Task 3: repoint `skill_search` to bucket_seal

**Files:** `src-tauri/src/agent/tools/builtin/skill_search.rs`, `registry_build.rs:258`.

- [ ] **Step 1:** Swap `store: Arc<MemoryGraphStore>` → `skill_adapter: Arc<dyn MemoryAdapter>` in `SkillSearchTool` + `new`; drop the memory_graph `use`.
- [ ] **Step 2:** Replace the three memory_graph calls:
  - candidate list (`store.list_top_learned_skills(space, 500)`) → `skills::top_skills(&self.skill_adapter, &self.space_id, 500).await`.
  - per-token keyword search (`store.search_by_keyword(space, tok)`) → `skills::search(&self.skill_adapter, &self.space_id, tok, 50).await` (Task 1). Map the returned `Skill`s into the same candidate shape the ranking code expects (use `Skill.name`/`body`/`keywords`/`slug`; where the old code used `node.id`, use `slug`).
  - usage bump (`store.bump_skill_usage(&bump_ids)`) → for each hit slug, `skills::bump_usage(&self.skill_adapter, &self.space_id, &slug).await` (best-effort, ignore errors).
  Preserve the existing ranking/scoring logic — only the data source changes.
- [ ] **Step 3:** `registry_build.rs:258` — pass the bucket_seal adapter (cast to `Arc<dyn MemoryAdapter>`).
- [ ] **Step 4:** Update skill_search's tests (memory_graph fixtures → `skills::put_skill` seeding on in-memory adapter; the `bump_skill_usage_called_on_hit` test → assert `bump_usage` incremented via `get_skill`). `cargo build` + `cargo test --lib skill_search 2>&1 | tail` green.
- [ ] **Step 5:** Commit — `feat(agent): skill_search reads bucket_seal skills facade (Step 1)`.

---

### Task 4: remove `tool_memory_repoint_enabled` + delete tool_memory's memory_graph paths

**Files:** `tool_memory.rs`, `service.rs`, `memubot_config.rs`.

- [ ] **Step 1:** In `tool_memory.rs`, make the adapter path unconditional in `record_tool_usage`/`record_co_usage`/`get_tool_stats`: delete the `if self.repoint_enabled { … } else { …memory_graph… }` wrapper, keep the adapter body; the `repoint_adapter: Option<…>` becomes a required `Arc<dyn MemoryAdapter>` field (drop the `repoint_enabled` field + the `Option`). Update `ToolUsageMemoryManager::new(store, adapter, enabled)` → `new(store, adapter)` (the `store: Arc<MemoryGraphStore>` field may now be unused → remove it + the `use`).
- [ ] **Step 2:** `service.rs:643` — update the `::new` call (drop the flag arg + the memory_graph store arg). Remove `tool_memory_repoint_enabled` from `MemoryOsRuntimeConfig` (struct + `from_memubot_config` + `Default` + `for_tests`) and its read.
- [ ] **Step 3:** `memubot_config.rs` — delete `tool_memory_repoint_enabled` field + `default_tool_memory_repoint_enabled` fn + the `impl Default` line + its two tests.
- [ ] **Step 4:** `cargo build 2>&1 | grep -E "^error" | head` empty; `cargo test --lib tool_memory 2>&1 | tail` + `--lib proactive` green; `grep -rn tool_memory_repoint_enabled src-tauri/src/` → empty.
- [ ] **Step 5:** Commit — `refactor(memory): tool_memory unconditional on bucket_seal; drop tool_memory_repoint_enabled (Step 1)`.

---

### Task 5: remove `skill_store_repoint_enabled` + delete the gated skill memory_graph paths

**Files:** `service.rs`, `tauri_commands.rs`, `memu_tools.rs`, `registry_build.rs`, `memubot_config.rs`.

- [ ] **Step 1:** `service.rs` skill-write site (~2140–2235): delete the `else { store_skill_as_procedure(memory_graph_store, …) }`; keep the adapter `put_skill` body unconditional. Remove `skill_store_repoint_enabled` from `MemoryOsRuntimeConfig`.
- [ ] **Step 2:** `tauri_commands.rs`: `get_learned_skill` — delete the memory_graph else, keep the `skills::get_skill` body. `record_skill_cited` — delete the memory_graph else, keep `skills::bump_cited` + promotion. Slash-command skill-inject (~8391) — delete the memory_graph fallback, keep the adapter path. (Each reads `state.memubot_config…skill_store_repoint_enabled` — remove the read + the `if`.)
- [ ] **Step 3:** `memu_tools.rs`: drop `store: Option<Arc<MemoryGraphStore>>` + `with_store` + the SQL fast-path (`list_top_skills_by_usage` block); keep `skill_adapter` + the `top_skills` path unconditional. `registry_build.rs` `register_memu_tools` — stop passing the flag + the store.
- [ ] **Step 4:** `memubot_config.rs` — delete `skill_store_repoint_enabled` field + `default_*` fn + `impl Default` line + its two tests.
- [ ] **Step 5:** `cargo build` empty; `cargo test --lib memu`, `--lib memory_adapter`, `--lib proactive`, `--lib tauri` (or the relevant) green; `grep -rn skill_store_repoint_enabled src-tauri/src/` → empty.
- [ ] **Step 6:** Commit — `refactor(memory): skill layer unconditional on bucket_seal; drop skill_store_repoint_enabled (Step 1)`.

---

### Task 6: whole-slice verification

- [ ] `cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] `cargo test --lib memory_adapter::skills`, `--lib skill_search`, `--lib load_skill`, `--lib tool_memory`, `--lib proactive` → green.
- [ ] `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Finish-line check:** `grep -rn "skill_store_repoint_enabled\|tool_memory_repoint_enabled" src-tauri/src/` → **empty** (flags gone). `grep -rn "MemoryGraphStore" src-tauri/src/agent/tools/builtin/skill_search.rs src-tauri/src/agent/tools/builtin/load_skill.rs` → **empty** (skill tools off memory_graph).
- [ ] `gitnexus_detect_changes()` before PR.

## Adjacent-edit checklist (PR body)

- 2 config flags removed (MemoryOsConfig + MemoryOsRuntimeConfig) → all readers updated; deserialize-without-field is still fine (serde default gone → unknown field ignored, or remove from any sample config).
- `SkillSearchTool`/`LoadSkillTool`/`ToolUsageMemoryManager`/`MemuMemoryTool` constructors changed → all call sites (registry_build, service.rs) updated.
- New `skills::search`.
- memory_graph itself untouched (retained rich layer); migration modules + freeze hook untouched. gbrain flags untouched (Step 2).

## PR shape

Branch `claude/step1-skill-tool-unconditional`, one PR, `## Commits (bisectable)` table (Tasks 1–5 = 5 commits). Title: `refactor(memory): Step 1 — skill+tool layers unconditional on bucket_seal (drop memory_graph fallbacks + flags)`. Body: data-safety gate passed (248 skills + 142 tool_stats already in bucket_seal); deletes the rollback flags + dead memory_graph else-branches + repoints skill_search/load_skill; memory_graph retained; gbrain = Step 2.

## Self-review notes

- **Spec coverage:** scope items 1–5 → Tasks: flags (4,5), gated else-branches (4,5), skill_search/load_skill repoint (2,3 + the skills::search gap in 1), memu_tools store-drop (5), keep migrations/freeze/gbrain (untouched). ✔
- **Type consistency:** `skills::{search,get_skill,top_skills,bump_usage}` signatures consistent across Tasks 1/2/3; `skill_adapter: Arc<dyn MemoryAdapter>` field name uniform; slug = `normalize_title_for_dedup`. ✔
- **Bisectability:** T1 (additive facade) compiles; T2/T3 (repoint one tool each + its ctor) compile; T4 (tool flag + its readers atomic) compiles; T5 (skill flag + its readers atomic) compiles. ✔
- **Follow-the-recon items** (flagged): exact else-branch bounds at each site (read first per task); skill_search candidate-shape mapping from `Skill` (Task 3 Step 2); the `store: Arc<MemoryGraphStore>` field on `ToolUsageMemoryManager` may be unused after T4 (remove); tests' memory_graph fixtures → adapter seeding (Tasks 2/3). Each has concrete guidance.
