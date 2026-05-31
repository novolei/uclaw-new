# Proactive Memory Freeze-Consistency Implementation Plan (Sub-project C)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make `memory_graph`'s freeze honest by migrating the one **lossless** production writer (task_memory's `Episode` nodes) onto the `MemoryAdapter` (`bucket_seal`), explicitly exempting + documenting the two **rich** writers (tool_memory's edge graph, skill_parser's versioned/keyworded skill store — deferred to the gbrain↔openhuman effort), and tightening the freeze hook to block NEW `create_node`/`create_edge`/etc. bypasses.

**Architecture:** `TaskMemoryManager` swaps its `Arc<MemoryGraphStore>` for an adapter handle; its `record_task`/`list_recent_tasks`/keyword-recall become `async` and serialize `Episode`↔`MemoryEntry.content` under `proactive:episode:{space}`. A one-time idempotent startup migration moves existing Episode nodes. tool_memory/skill_parser stay on memory_graph with exemption docs. The hook gains `create_*` method-call detection + a file allowlist.

**Tech Stack:** Rust, `MemoryAdapter`, `serde_json`. No new deps. Spec: `docs/superpowers/specs/2026-05-31-proactive-memory-freeze-consistency-design.md`.

---

## Source-of-truth references (verified)

- `proactive/task_memory.rs`: `TaskMemoryManager { store: Arc<MemoryGraphStore> }` (122), `new(store: Arc<MemoryGraphStore>)` (127). `record_task` (137, **sync**) → builds `MemoryNode { kind: Episode, title, metadata: Some(json!{task_type,status,files_changed,tools_used,duration_ms,error_messages,solution_summary,session_id}), ... }`, `create_node` (174), keyword rows (187). `find_similar_tasks` uses `search_by_keyword` (219). `list_recent_tasks` (285, **sync**) → `list_nodes_by_kind(space, Episode, limit)` → maps each node to `SimilarTask { node_id, title, task_type, status, solution_summary: Option<String>, files_changed: Vec<String>, score: f64, recorded_at }` (96).
- `memory_adapter/traits.rs`: `async fn store(&self, namespace: &str, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>`; `async fn recall(&self, query, limit, opts: RecallOpts) -> anyhow::Result<Vec<MemoryEntry>>`. `MemoryEntry { id, key, content, namespace, category, timestamp, session_id, score }`.
- `state.memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>`, `state.default_memory_backend` (= `"bucket_seal"`).
- `scripts/git-hooks/checks/check-memory-graph-freeze.sh:29`: `if echo "$added" | grep -qE '\bmemory_graph\s*::\s*(write|insert|update|delete)[A-Za-z_]*\s*\('; then`. Allowlist `case` at ~21-22 (`memory_graph/mod.rs`, `legacy_migration/*`).
- `memubot_config.rs`: `MemoryOsConfig` bool-field pattern (`#[serde(default="default_X")] pub X: bool` + `fn default_X()` + Default entry); mirror `unified_load_context_enabled` (added in A).
- `app.rs`: startup fire-and-forget idiom (`tauri::async_runtime::spawn`) — see the checkpoint prune block + A's load_context wiring. Where `TaskMemoryManager::new` is called (recon — likely proactive service construction).

---

## CRITICAL facts

1. **Only task_memory migrates.** tool_memory + skill_parser are exempt (rich semantics, deferred). Do NOT migrate them.
2. **Sync → async ripple.** `TaskMemoryManager`'s methods become `async` (adapter is async). Recon + update all callers to `.await`. This is the integration-risk task.
3. **Lossless.** Episode → `MemoryEntry.content` JSON must round-trip ALL `SimilarTask` fields (title, task_type, status, solution_summary, files_changed, recorded_at) + keywords (folded into content for recall match).
4. **Migration idempotent + infallible** — flag `proactive_episode_migrated_v1`; old nodes retained; never block boot.
5. **Hook must not break tests** — the allowlist covers test-only writers (skill_search.rs, skills_manifest.rs) + the exempt/migration files.
6. **Pre-commit hooks** — no `--no-verify`. Use `uclaw_home`/adapter, not raw `memory_graph` writes, in new code.

---

## File Structure

| File | Change |
|---|---|
| `memubot_config.rs` | `proactive_episode_migrated_v1` flag (default false) + tests |
| `proactive/task_memory.rs` | constructor → adapter handle; record/list/keyword-recall → async + adapter store/recall + Episode↔content serde + tests |
| `proactive/memory_migration.rs` | **new** — one-time idempotent Episode migration |
| `app.rs` (+ proactive service construction) | pass adapter to `TaskMemoryManager`; startup migration call |
| `proactive/tool_memory.rs`, `proactive/skill_parser.rs` | exemption doc notes |
| `scripts/git-hooks/checks/check-memory-graph-freeze.sh` | `create_*` detection + file allowlist |
| `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md`, `memory_adapter/mod.rs` | exemption notes |

---

## Tasks

### Task 1: config flag

- [ ] **Step 1: Failing tests** (memubot_config.rs, mirror `unified_load_context_enabled`):
```rust
#[test]
fn memory_os_default_proactive_episode_migrated_false() {
    assert!(!MemoryOsConfig::default().proactive_episode_migrated_v1);
}
#[test]
fn memory_os_deserializes_without_proactive_episode_migrated() {
    let cfg: MemubotConfig = serde_json::from_str(r#"{"memory_os":{}}"#).unwrap();
    assert!(!cfg.memory_os.proactive_episode_migrated_v1);
}
```
- [ ] **Step 2: Implement.** Field `#[serde(default = "default_proactive_episode_migrated_v1")] pub proactive_episode_migrated_v1: bool,` + `fn default_proactive_episode_migrated_v1() -> bool { false }` + Default entry `proactive_episode_migrated_v1: false,`. Doc: "Set true once the one-time migration of legacy memory_graph Episode nodes into the MemoryAdapter (proactive:episode) has run."
- [ ] **Step 3: Run + commit.** `cd src-tauri && cargo test --lib memubot_config 2>&1 | tail`; `git commit -am "feat(config): proactive_episode_migrated_v1 flag (C.1)"`

### Task 2: task_memory → adapter (async migration)

- [ ] **Step 1: RECON** `TaskMemoryManager` callers. `grep -rn "TaskMemoryManager\|record_task\|list_recent_tasks\|find_similar" src-tauri/src` — find construction site(s) + every call to the methods becoming async. Note which are already in async context (most proactive paths are).

- [ ] **Step 2: Serialization helpers + tests.** Add pure helpers to `task_memory.rs`:
```rust
/// Serialize a recorded task into the adapter content payload (lossless for SimilarTask).
fn task_to_content(title: &str, task_type: &str, status: &str, solution_summary: Option<&str>,
                   files_changed: &[String], recorded_at: &str, keywords: &[String]) -> String {
    serde_json::json!({
        "title": title, "task_type": task_type, "status": status,
        "solution_summary": solution_summary, "files_changed": files_changed,
        "recorded_at": recorded_at, "keywords": keywords,
    }).to_string()
}
/// Reconstruct a SimilarTask from a recalled MemoryEntry (id + content JSON).
fn entry_to_similar_task(id: &str, content: &str, score: f64) -> Option<SimilarTask> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    Some(SimilarTask {
        node_id: id.to_string(),
        title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        task_type: v.get("task_type").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
        status: v.get("status").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
        solution_summary: v.get("solution_summary").and_then(|x| x.as_str()).map(|s| s.to_string()),
        files_changed: v.get("files_changed").and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
        score,
        recorded_at: v.get("recorded_at").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}
```
Tests: `task_to_content` then `entry_to_similar_task` round-trips all fields (title/type/status/solution_summary/files_changed/recorded_at); malformed content → None.

- [ ] **Step 3: Swap the store handle + make methods async.** Change `TaskMemoryManager` to hold the adapter: `{ adapter: Arc<dyn MemoryAdapter>, default_space_ns: ... }` (or hold `adapters: Arc<HashMap<...>>` + resolve `bucket_seal`). Update `new(...)` to take the adapter handle. Make `record_task` / `list_recent_tasks` / `find_similar_tasks` `async`:
  - `record_task`: build keywords (existing `extract_keywords`), `content = task_to_content(...)`, `self.adapter.store(&format!("proactive:episode:{space_id}"), &node_id, &content, MemoryCategory::Core /* or the task category */, task.session_id.as_deref()).await?`. Drop `create_node` + keyword-row writes.
  - `list_recent_tasks`: `self.adapter.recall("", limit, RecallOpts { namespace: Some(&ns), ..Default::default() }).await?` (empty query = list namespace; if the adapter needs a non-empty query for listing, use `list`/namespace recall — recon `recall` semantics for empty query, else add a list call) → `entry_to_similar_task` per entry.
  - `find_similar_tasks`: `self.adapter.recall(&query_or_keywords_joined, limit, opts).await?` → reconstruct.
- [ ] **Step 4: Update callers** to `.await` (from Step 1 recon). Most are in async proactive paths.
- [ ] **Step 5: Run + commit.** `cd src-tauri && cargo test --lib proactive::task_memory 2>&1 | tail`; `cargo build 2>&1 | grep -E "^error" | head`; `git commit -am "feat(proactive): task_memory Episode write/read via MemoryAdapter (C.2)"`

### Task 3: one-time migration + startup wiring

- [ ] **Step 1: Create `proactive/memory_migration.rs`:**
```rust
//! One-time migration of legacy memory_graph Episode nodes into the
//! MemoryAdapter (proactive:episode namespace). Idempotent via the
//! `proactive_episode_migrated_v1` config flag. Infallible: logs + skips on error.
use std::sync::Arc;
use crate::memory_adapter::MemoryAdapter;
use crate::memory_graph::store::MemoryGraphStore;
use crate::memory_graph::models::MemoryNodeKind;

pub async fn migrate_episodes_if_needed(
    graph: &MemoryGraphStore,
    adapter: &Arc<dyn MemoryAdapter>,
    spaces: &[String],
) -> usize {
    let mut migrated = 0usize;
    for space_id in spaces {
        let nodes = match graph.list_nodes_by_kind(space_id, MemoryNodeKind::Episode, 100_000) {
            Ok(n) => n,
            Err(e) => { tracing::warn!(space=%space_id, error=%e, "episode migration: list failed; skip space"); continue; }
        };
        for node in nodes {
            let content = serde_json::json!({
                "title": node.title,
                "metadata": node.metadata,
                "recorded_at": node.created_at,
                "legacy_migrated": true,
            }).to_string();
            let ns = format!("proactive:episode:{space_id}");
            if let Err(e) = adapter.store(&ns, &node.id, &content,
                crate::memory_adapter::MemoryCategory::Core, None).await {
                tracing::warn!(node=%node.id, error=%e, "episode migration: store failed; skip node");
                continue;
            }
            migrated += 1;
        }
    }
    migrated
}
```
(Recon: how to enumerate `spaces` — `graph.list_spaces()` or the existing space registry; if a single/default space, migrate that. Adapt the `content` shape to match Task 2's `task_to_content` so reads reconstruct — reuse the same field names; if the legacy node metadata differs, map it.)
- [ ] **Step 2: Wire startup** in `app.rs` after the adapters + config are built (mirror the checkpoint-prune fire-and-forget):
```rust
{
    let migrated_flag = memubot_config.memory_os.proactive_episode_migrated_v1;
    if !migrated_flag {
        // clone the graph store + bucket_seal adapter + spaces; spawn best-effort
        tauri::async_runtime::spawn(async move {
            let n = crate::proactive::memory_migration::migrate_episodes_if_needed(&graph, &adapter, &spaces).await;
            tracing::info!(migrated = n, "proactive episode migration complete");
            // persist the flag (set proactive_episode_migrated_v1 = true + save config)
        });
    }
}
```
Recon the config-persist path (how A/other code flips + saves a `memory_os` flag at runtime — e.g. `memubot_config.save(...)` or a tauri command). If runtime config-save from a spawned task is awkward, use the alternative idempotency sentinel: check whether `adapter.namespace_summaries()` already contains a `proactive:episode:*` namespace with entries, and skip if so (no flag write needed). **Pick whichever is cleaner; flag your choice.**
- [ ] **Step 3: Build + commit.** `cargo build 2>&1 | grep -E "^error" | head`; `git commit -am "feat(proactive): one-time idempotent Episode migration at startup (C.3)"`

### Task 4: tool_memory + skill_parser exemption docs + ADR/roster notes

- [ ] **Step 1:** Add to the top module-doc (`//!`) of `proactive/tool_memory.rs`: `//! EXEMPT from memory_graph freeze: co-used-tools graph (edges) has no MemoryAdapter equivalent; migration deferred to the gbrain↔openhuman effort (see gbrain-primary-freeze ADR).` and to `proactive/skill_parser.rs`: `//! EXEMPT from memory_graph freeze: versioned/keyword-indexed/ranked learned-skill store has no MemoryAdapter equivalent; migration deferred to the gbrain↔openhuman effort.`
- [ ] **Step 2:** Add a short "## Freeze exemptions (2026-05-31)" note to `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` listing tool_memory (co-used graph) + skill_parser (versioned skill store) as documented exemptions pending the gbrain↔openhuman effort; add the same one-liner to the `memory_adapter/mod.rs` roster doc block.
- [ ] **Step 3: Commit.** `git commit -am "docs(memory): document tool_memory + skill_parser as freeze exemptions (C.4)"`

### Task 5: tighten the freeze hook

- [ ] **Step 1:** In `check-memory-graph-freeze.sh`, after the existing path-call check (line 29), add a method-call check:
```sh
    if echo "$added" | grep -qE '\.(create_node|create_entity_page|create_edge|create_version|create_keyword)\s*\('; then
        VIOLATIONS+=("$f")
    fi
```
- [ ] **Step 2:** Extend the allowlist `case` (~line 21) to skip the legitimate/exempt/test-only writers:
```sh
        src-tauri/src/proactive/tool_memory.rs) continue ;;
        src-tauri/src/proactive/skill_parser.rs) continue ;;
        src-tauri/src/proactive/memory_migration.rs) continue ;;
        src-tauri/src/agent/tools/builtin/skill_search.rs) continue ;;
        src-tauri/src/skills_manifest.rs) continue ;;
```
(Keep the existing `memory_graph/mod.rs` + `legacy_migration/*` entries.)
- [ ] **Step 3:** Update the hook's header comment to mention the `create_*` method-call rule + that the allowlist marks reviewed exemptions.
- [ ] **Step 4: Manual verification** (the hook runs on staged diffs): create a throwaway staged change adding `foo.create_node(&x);` in a non-allowlisted `.rs` file → run the hook → BLOCKED; the same line in `tool_memory.rs` → passes. Revert the throwaway. (Document the check in the commit body; no Rust test.)
- [ ] **Step 5: Commit.** `git add scripts/git-hooks/checks/check-memory-graph-freeze.sh && git commit -m "feat(hooks): freeze hook also blocks create_node/edge/version/keyword bypasses + exemption allowlist (C.5)"`

### Task 6: Verification

- [ ] `cd src-tauri && cargo test --lib proactive::task_memory 2>&1 | tail` (serde round-trip + adapter store/recall).
- [ ] `cargo test --lib memubot_config 2>&1 | tail` (flag).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib agent 2>&1 | tail -6` + `cargo test --lib proactive 2>&1 | tail` — net green; only the 2 known pre-existing failures.
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "task_memory|memory_migration|memubot_config" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **task_memory lossless:** record→list round-trip preserves all SimilarTask fields via the adapter; no `create_node`/`list_nodes_by_kind` remain in task_memory production paths.
- [ ] **migration idempotent:** flag (or namespace-sentinel) prevents re-migration.
- [ ] **exemptions intact:** tool_memory + skill_parser still compile + their tests pass (unchanged).
- [ ] **hook:** the Task 5 manual check confirms a new non-allowlisted `create_node` is blocked.

---

## Self-Review

- ✅ **Spec coverage:** task_memory migration (Tasks 1-2) + one-time migration (Task 3) + exemption docs (Task 4) + hook tightening (Task 5) + verification (Task 6). tool_memory/skill_parser explicitly NOT migrated (exempt). Out-of-scope (rich-store migration, node deletion) deferred.
- ✅ **Placeholder scan:** full code for config/serde-helpers/migration/hook; the sync→async caller update + config-persist are recon-and-adapt with a concrete fallback (namespace-sentinel idempotency).
- ✅ **Type consistency:** `task_to_content(...) -> String` / `entry_to_similar_task(&str,&str,f64) -> Option<SimilarTask>`; `adapter.store(ns,key,content,category,session)`; `migrate_episodes_if_needed(graph, adapter, spaces) -> usize`; config `proactive_episode_migrated_v1: bool`. `SimilarTask` fields per task_memory.rs:96.
- ✅ **Risk-scaled:** the sync→async ripple (Task 2) is the integration-risk task — isolated to TaskMemoryManager + its callers, with serde round-trip tests guarding losslessness; migration idempotent+infallible; rich writers untouched (no functionality loss); hook prevents regressions.
- Decisions: migrate only the lossless writer; exempt+document the two rich writers (ADR-deferred); idempotency via config flag OR namespace-sentinel (implementer picks); old nodes retained.
