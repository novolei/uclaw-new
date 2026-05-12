# Skill Recall Closed-Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the loop so extracted skills (both builtin from `*/SKILL.md` and learned from memory_graph) actually reach the LLM via a manifest in the system prompt + two new tools (`skill_search`, `load_skill`), with UI chips showing recall events.

**Architecture:** Free function `build_skills_manifest` unifies builtin (`SkillsRegistry.list()`) and learned (`MemoryGraphStore.list_top_learned_skills()`) into a Markdown block appended to `effective_system_prompt`. Two new built-in tools `skill_search` and `load_skill` query the same two sources and emit `agent:skill-recalled` events. Frontend `SkillRecallChips` component renders below assistant messages, dedup by `toolCallId`. No new schema — reuses existing `bump_skill_usage` / `record_skill_cited` for counters.

**Tech Stack:** Rust (`tokio`, `serde_json`, `async_trait`), React (Jotai atoms, Tauri event listen, sonner), SQLite via existing memory_graph_store.

**Spec:** [docs/superpowers/specs/2026-05-12-skill-recall-design.md](../specs/2026-05-12-skill-recall-design.md)

---

## File Structure

**New files:**
- `src-tauri/src/skills_manifest.rs` — free function `build_skills_manifest` + ranking helpers + tests
- `src-tauri/src/agent/tools/builtin/skill_search.rs` — `SkillSearchTool` impl + tests
- `src-tauri/src/agent/tools/builtin/load_skill.rs` — `LoadSkillTool` impl + tests
- `ui/src/components/agent/SkillRecallChips.tsx` — UI component
- `ui/src/components/agent/SkillRecallChips.test.tsx` — Vitest cases

**Modified files:**
- `src-tauri/src/lib.rs` — declare `skills_manifest` module
- `src-tauri/src/agent/tools/builtin/mod.rs` — export new tool modules
- `src-tauri/src/agent/dispatcher.rs` — manifest injection in `effective_system_prompt`; new `ChatDelegate` fields for registry + store + space_id
- `src-tauri/src/tauri_commands.rs:4252` (send_agent_message tool registration block) — register `SkillSearchTool` and `LoadSkillTool`; pass registry + store into ChatDelegate
- `ui/src/atoms/agent-atoms.ts` — `SkillRecall` type + `skillRecallsMapAtom`
- `ui/src/hooks/useGlobalAgentListeners.ts` — listen `agent:skill-recalled`
- `ui/src/components/agent/AgentMessages.tsx` — mount `SkillRecallChips` below assistant messages

**Total estimate:** ~930 LOC across 10 files, 5 commits.

---

## Task 1: Manifest builder

**Files:**
- Create: `src-tauri/src/skills_manifest.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod skills_manifest;`)

### - [ ] Step 1.1: Add module declaration in lib.rs

Open `src-tauri/src/lib.rs` and find the line `pub mod skills;`. Add the new module right after it:

```rust
pub mod skills;
pub mod skills_manifest;
```

### - [ ] Step 1.2: Create skills_manifest.rs with the public function signature + first failing test

Create `src-tauri/src/skills_manifest.rs`:

```rust
//! Build a Markdown manifest block describing the agent's available skills,
//! unioning builtin skills from `SkillsRegistry` and learned skills from
//! `MemoryGraphStore`. Injected into the system prompt by
//! `ChatDelegate::effective_system_prompt`.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §1.

use crate::memory_graph::MemoryGraphStore;
use crate::skills::SkillsRegistry;

const DEFAULT_MAX_ENTRIES: usize = 30;
const DEFAULT_MAX_TOKENS: usize = 1500;
const APPROX_TOKENS_PER_CHAR: f64 = 0.25;

/// Build the manifest block to append to the system prompt.
///
/// Returns an empty String when no skills exist in either source — caller
/// is responsible for not appending separator markers around an empty body.
pub fn build_skills_manifest(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    max_tokens: usize,
) -> String {
    let entries = collect_entries(registry, store, space_id, max_entries);
    if entries.is_empty() {
        return String::new();
    }
    format_manifest(&entries, max_tokens)
}

#[derive(Debug)]
struct ManifestEntry {
    name: String,
    summary: String,
    provenance: &'static str, // "builtin" or "learned"
    cited_count: u64,         // 0 for builtin
}

fn collect_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // Builtin: alphabetical, all enabled
    let mut builtin: Vec<_> = registry
        .list()
        .into_iter()
        .filter(|info| info.enabled)
        .collect();
    builtin.sort_by(|a, b| a.name.cmp(&b.name));
    for info in builtin {
        entries.push(ManifestEntry {
            name: info.name.clone(),
            summary: truncate_summary(&info.description, 100),
            provenance: "builtin",
            cited_count: 0,
        });
        if entries.len() >= max_entries {
            return entries;
        }
    }

    // Learned: E3 ranking from list_top_learned_skills (cited DESC, usage DESC, updated DESC)
    let learned_limit = max_entries.saturating_sub(entries.len());
    if learned_limit == 0 {
        return entries;
    }
    let learned = store
        .list_top_learned_skills(space_id, learned_limit)
        .unwrap_or_default();
    for detail in learned {
        let summary = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| detail.node.title.clone());
        let cited_count = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("cited_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entries.push(ManifestEntry {
            name: detail.node.title.clone(),
            summary: truncate_summary(&summary, 100),
            provenance: "learned",
            cited_count,
        });
        if entries.len() >= max_entries {
            break;
        }
    }

    entries
}

fn format_manifest(entries: &[ManifestEntry], max_tokens: usize) -> String {
    let header = "\n\n---\n\n## 你已学习到的技能 (Learned Skills)\n\n下面是你过往会话中沉淀的技能清单。当遇到相关问题时，先用 `skill_search` 查询，再用 `load_skill` 加载完整内容。**不强制使用**——只有当技能确实匹配当前任务时再调用。\n\n";
    let footer = "\n\n**使用流程**：\n1. `skill_search(query: \"...\", top_k: 3)` → 看摘要决定\n2. `load_skill(name: \"...\", reason: \"...\")` → 拿完整指引\n3. 应用后，在回复末尾用 `> 应用技能：name — 简短原因` 标注（供未来检索）\n\n---\n";

    let mut body = String::new();
    let budget_chars = (max_tokens as f64 / APPROX_TOKENS_PER_CHAR) as usize;
    let header_footer_chars = header.len() + footer.len();
    let mut remaining = budget_chars.saturating_sub(header_footer_chars);

    for entry in entries {
        let cite = if entry.cited_count > 0 {
            format!(" · cited {}", entry.cited_count)
        } else {
            String::new()
        };
        let line = format!(
            "- **{}** [{}{}] — {}\n",
            entry.name, entry.provenance, cite, entry.summary
        );
        if line.len() > remaining {
            break;
        }
        remaining -= line.len();
        body.push_str(&line);
    }

    if body.is_empty() {
        return String::new();
    }

    format!("{}{}{}", header, body, footer)
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        // Trim at last word boundary to avoid mid-word cut
        if let Some(last_space) = out.rfind(char::is_whitespace) {
            out.truncate(last_space);
        }
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_and_store_returns_empty_string() {
        // Will need a way to construct an empty MemoryGraphStore — use the
        // in-memory SQLite helper from memory_graph tests.
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        // Run minimal V4 migration so memory_nodes table exists.
        let _ = conn.lock().unwrap().execute_batch(
            crate::db::migrations::V4_MEMORY_GRAPH,
        );
        let store = MemoryGraphStore::new(conn);
        let registry = SkillsRegistry::new();

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500);
        assert!(manifest.is_empty(), "expected empty manifest, got: {}", manifest);
    }
}
```

### - [ ] Step 1.3: Run test, verify it fails to compile (function defined, V4_MEMORY_GRAPH may not exist)

Run:
```bash
cd src-tauri && cargo test --lib skills_manifest::tests::empty_registry_and_store_returns_empty_string 2>&1 | tail -20
```

Expected: PASS (this case actually compiles cleanly because we wrote the impl in 1.2). If `V4_MEMORY_GRAPH` constant doesn't exist as a `pub const &str` in migrations.rs, the build fails — check `grep -n "V4_MEMORY_GRAPH\|pub const V4" src-tauri/src/db/migrations.rs`. If it's named differently or private, mirror what `ensure_tables` in store.rs:21 already calls.

### - [ ] Step 1.4: Add the remaining test cases

Append to `mod tests` block in `skills_manifest.rs`:

```rust
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
    use chrono::Utc;
    use serde_json::json;

    fn fresh_store() -> std::sync::Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        std::sync::Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned_node(
        store: &MemoryGraphStore,
        title: &str,
        summary: &str,
        cited_count: u64,
        usage_count: u64,
    ) {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": summary,
                "cited_count": cited_count,
                "usage_count": usage_count,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("# {}\n\n{}", title, summary),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).unwrap();
    }

    #[test]
    fn learned_only_renders_block_with_cited_count() {
        let store = fresh_store();
        make_learned_node(&store, "stock-research", "Cross-validate stock financials", 7, 12);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500);

        assert!(manifest.contains("## 你已学习到的技能"), "missing header");
        assert!(manifest.contains("**stock-research**"), "missing skill name");
        assert!(manifest.contains("[learned · cited 7]"), "expected cited 7 segment; got:\n{}", manifest);
        assert!(manifest.contains("Cross-validate stock financials"), "missing summary");
    }

    #[test]
    fn zero_cited_omits_cite_segment() {
        let store = fresh_store();
        make_learned_node(&store, "newly-extracted", "Just extracted, never cited yet", 0, 1);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500);

        assert!(manifest.contains("[learned]"), "expected bare [learned] when cited=0; got:\n{}", manifest);
        assert!(!manifest.contains("cited 0"), "must not say 'cited 0'");
    }

    #[test]
    fn max_entries_caps_count() {
        let store = fresh_store();
        for i in 0..5 {
            make_learned_node(&store, &format!("skill-{}", i), "blah", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 2, 1500);

        let lines = manifest.matches("\n- **").count();
        assert_eq!(lines, 2, "expected exactly 2 entries; got:\n{}", manifest);
    }

    #[test]
    fn long_summary_truncates_at_word_boundary() {
        let summary = "Cross-validate stock financials across Yahoo Finance Macrotrends and StockAnalysis with HTTP 403 fallback handling and many more details that should be cut off";
        let store = fresh_store();
        make_learned_node(&store, "long-skill", summary, 0, 0);
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500);

        // Should be truncated with "…"
        assert!(manifest.contains("…"), "expected truncation marker; got:\n{}", manifest);
        // Should not contain the tail of the original summary
        assert!(!manifest.contains("cut off"));
    }

    #[test]
    fn token_budget_can_truncate_entries() {
        let store = fresh_store();
        for i in 0..30 {
            make_learned_node(&store, &format!("skill-{}", i), "some summary text", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 100);

        let lines = manifest.matches("\n- **").count();
        assert!(lines < 30, "expected token budget to drop entries; got {} lines", lines);
    }
```

### - [ ] Step 1.5: Run tests; expect mostly passing, fix issues

Run:
```bash
cd src-tauri && cargo test --lib skills_manifest 2>&1 | tail -40
```

Expected: 5 tests pass. If `V4_MEMORY_GRAPH` is named differently in `db/migrations.rs` (e.g. private), make it `pub const` in that file (one-line change). Likely fixups:
- `MemoryVersionStatus::Active` may serialize/deserialize as `"active"` — match how `create_version` stores it.
- If `MemoryGraphStore::create_version` rejects when `status != Active`, that's fine.
- If `learned_only_renders_block_with_cited_count` reports `cited 0` in spurious places, check the `cite_segment` conditional.

### - [ ] Step 1.6: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/skills_manifest.rs src-tauri/src/lib.rs src-tauri/src/db/migrations.rs
git commit -m "feat(skills): manifest builder unifies builtin + learned

New free function build_skills_manifest produces a Markdown block
suitable for appending to the agent system prompt. Sources:

- Builtin: SkillsRegistry.list() filtered by enabled, sorted alpha
- Learned: MemoryGraphStore.list_top_learned_skills (existing E3 ranking)

Entry format: '- **{name}** [{provenance}{cite_segment}] — {summary}'
where cite_segment is omitted when cited_count is 0.

Caps: top max_entries (default 30) OR max_tokens (default 1500) — whichever
first. Returns empty String when no skills exist (caller skips append).

Five unit tests cover: empty source, learned with cited count, zero-cited
omits segment, max_entries cap, summary truncation, token budget cap.

No new schema; reads memory_graph through existing public methods."
```

---

## Task 2: skill_search built-in tool

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/skill_search.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (add `pub mod skill_search;`)

### - [ ] Step 2.1: Add module declaration

Open `src-tauri/src/agent/tools/builtin/mod.rs`. Find the existing `pub mod` lines (e.g. `pub mod shell;`). Add:

```rust
pub mod skill_search;
```

### - [ ] Step 2.2: Create the tool struct + Tool trait impl skeleton

Create `src-tauri/src/agent/tools/builtin/skill_search.rs`:

```rust
//! skill_search tool — agent invokes to find learned/builtin skills
//! matching a query. Returns a list of {name, summary, score, provenance,
//! cited_count, node_id} structs. Does NOT load full content (see
//! load_skill for that).
//!
//! Side effects: emits `agent:skill-recalled` event for UI; bumps
//! usage_count on each learned-skill hit via existing bump_skill_usage.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §3.

use std::sync::Arc;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::memory_graph::MemoryGraphStore;
use crate::skills::SkillsRegistry;

pub struct SkillSearchTool {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub store: Arc<MemoryGraphStore>,
    pub app_handle: tauri::AppHandle,
    pub conversation_id: String,
    pub space_id: String,
}

impl SkillSearchTool {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        store: Arc<MemoryGraphStore>,
        app_handle: tauri::AppHandle,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, store, app_handle, conversation_id, space_id }
    }
}

#[derive(Debug, Serialize)]
struct SearchHit {
    name: String,
    summary: String,
    score: f64,
    provenance: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cited_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_id: Option<String>,
}

#[async_trait]
impl Tool for SkillSearchTool {
    fn name(&self) -> &str {
        "skill_search"
    }

    fn description(&self) -> &str {
        "Search learned skills by keywords. Returns top-N matches with one-line summaries. Use this when facing a problem similar to one you've solved before — load the full skill content via load_skill afterward if a match looks promising."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords describing the current task / problem (English works better than Chinese)."
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of skills to return (default 3, max 10).",
                    "default": 3
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("query is required".into()))?
            .trim();
        if query.is_empty() {
            return Ok(ToolOutput::new(json!([]), start.elapsed().as_millis() as u64));
        }
        let top_k = params["top_k"].as_u64().unwrap_or(3).clamp(1, 10) as usize;

        let mut hits: Vec<SearchHit> = Vec::new();

        // Builtin pass — registry.match_skills returns scored skills already
        let registry = self.registry.read().await;
        for skill in registry.match_skills(query) {
            hits.push(SearchHit {
                name: skill.manifest.name.clone(),
                summary: truncate_summary(&skill.manifest.description, 200),
                score: crate::skills::score_skill(skill, query) as f64,
                provenance: "builtin",
                cited_count: None,
                node_id: None,
            });
        }
        drop(registry);

        // Learned pass — tokenize query, search keywords, score by hit count + priors
        let tokens: Vec<&str> = query
            .split_whitespace()
            .filter(|t| t.len() >= 2)
            .collect();
        let mut node_score: std::collections::HashMap<String, (i64, u64, u64)> =
            std::collections::HashMap::new();
        for tok in &tokens {
            if let Ok(nodes) = self.store.search_by_keyword(&self.space_id, tok) {
                for node in nodes {
                    // Filter to learned procedures only
                    let is_learned = node
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("skill_type"))
                        .and_then(|v| v.as_str())
                        == Some("learned");
                    if !is_learned {
                        continue;
                    }
                    let cited = node
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("cited_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let usage = node
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("usage_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let entry = node_score.entry(node.id.clone()).or_insert((0, cited, usage));
                    entry.0 += 1;
                }
            }
        }

        // Hydrate learned hits + compute final score
        let bump_ids: Vec<String> = node_score.keys().cloned().collect();
        for (node_id, (kw_hits, cited, usage)) in node_score {
            if let Ok(Some(node)) = self.store.get_node(&node_id) {
                let summary = node
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("summary"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| node.title.clone());
                let score = (kw_hits as f64 * 1.0)
                    + (cited as f64 * 0.5)
                    + (usage as f64 * 0.2);
                hits.push(SearchHit {
                    name: node.title,
                    summary: truncate_summary(&summary, 200),
                    score,
                    provenance: "learned",
                    cited_count: Some(cited),
                    node_id: Some(node_id),
                });
            }
        }

        // Bump usage_count for learned hits (fire-and-forget; counter is soft)
        if !bump_ids.is_empty() {
            let id_refs: Vec<&str> = bump_ids.iter().map(|s| s.as_str()).collect();
            if let Err(e) = self.store.bump_skill_usage(&id_refs) {
                tracing::warn!("bump_skill_usage failed: {}", e);
            }
        }

        // Sort by score desc and trim
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);

        // Emit agent:skill-recalled event
        let tool_call_id = params["_tool_call_id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let _ = self.app_handle.emit("agent:skill-recalled", json!({
            "conversationId": self.conversation_id,
            "toolCallId": tool_call_id,
            "kind": "search",
            "query": query,
            "results": &hits,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        Ok(ToolOutput::new(
            serde_json::to_value(&hits).unwrap_or(json!([])),
            start.elapsed().as_millis() as u64,
        ))
    }
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    if let Some(last_space) = out.rfind(char::is_whitespace) {
        out.truncate(last_space);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryKeyword};
    use chrono::Utc;
    use serde_json::json;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned_node_with_keywords(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        cited: u64,
    ) -> String {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": cited,
                "usage_count": 0,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    #[tokio::test]
    async fn empty_query_returns_empty_array() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        // We need a tauri::AppHandle for the test — most tools in the codebase
        // construct one with tauri::test::mock_app(). Use that or skip the
        // emit path in tests via a feature gate. For now, use mock_app:
        let app = tauri::test::mock_app();

        let tool = SkillSearchTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }

    #[tokio::test]
    async fn learned_keyword_hit_returns_skill() {
        let store = fresh_store();
        let _id = make_learned_node_with_keywords(&store, "stock-research", &["stock", "financials"], 5);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "stock financials" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["name"], "stock-research");
        assert_eq!(hits[0]["provenance"], "learned");
        assert_eq!(hits[0]["cited_count"], 5);
    }

    #[tokio::test]
    async fn bump_skill_usage_called_on_hit() {
        let store = fresh_store();
        let id = make_learned_node_with_keywords(&store, "stock-research", &["stock"], 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let _ = tool.execute(json!({ "query": "stock" })).await.unwrap();

        let node = store.get_node(&id).unwrap().unwrap();
        let usage = node.metadata.unwrap()
            .get("usage_count").unwrap().as_u64().unwrap();
        assert_eq!(usage, 1);
    }

    #[tokio::test]
    async fn no_matches_returns_empty_array() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "nonexistent_xyz_query" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }
}
```

### - [ ] Step 2.3: Run tests; fix any compilation issues

Run:
```bash
cd src-tauri && cargo test --lib skill_search 2>&1 | tail -30
```

Expected: 4 tests pass. Likely fixups:
- `MemoryKeyword` may not be `pub` — check `memory_graph/models.rs`. If it isn't, add `pub use models::MemoryKeyword;` or use the field-by-field construction via a public helper that already exists. Inspect with `grep -n "pub.*MemoryKeyword\|fn create_keyword" src-tauri/src/memory_graph/`.
- `store.create_keyword` may not exist as named — check `grep -n "fn create_keyword\|fn add_keyword" src-tauri/src/memory_graph/store.rs` and use the actual method name (likely `add_keywords_for_node` or similar; mirror the proactive scenario's pattern).
- `tauri::test::mock_app` requires the `test` feature on tauri crate. If unavailable, use `cfg(test)` to gate the emit lines or stub the AppHandle. Alternative: skip the emit call in tests by passing an option/flag, but cleanest is `mock_app`. Check `Cargo.toml` — if `test` feature missing, add it under `[dev-dependencies]`.

### - [ ] Step 2.4: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/builtin/mod.rs src-tauri/src/agent/tools/builtin/skill_search.rs
git commit -m "feat(agent): skill_search built-in tool

New tool queries both SkillsRegistry (builtin) and MemoryGraphStore
(learned, via search_by_keyword on memory_keywords table). Merges +
scores results, returns top-k with {name, summary, score, provenance,
cited_count?, node_id?}.

Scoring for learned hits:
  score = (keyword_hits × 1.0) + (cited × 0.5) + (usage × 0.2)

Side effects:
- Bumps usage_count on each learned-skill hit via existing
  bump_skill_usage (fire-and-forget; failures logged not raised)
- Emits agent:skill-recalled event with kind=search for UI chip

4 unit tests: empty query, learned keyword hit, bump_skill_usage
called, no-match case."
```

---

## Task 3: load_skill built-in tool

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/load_skill.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (add `pub mod load_skill;`)

### - [ ] Step 3.1: Add module declaration

Open `src-tauri/src/agent/tools/builtin/mod.rs`. Add:

```rust
pub mod load_skill;
```

### - [ ] Step 3.2: Create the tool

Create `src-tauri/src/agent/tools/builtin/load_skill.rs`:

```rust
//! load_skill tool — agent fetches the full body of a skill by name.
//! Returns { name, version, content, parameters, provenance }.
//!
//! Resolution order: SkillsRegistry (builtin) first, then
//! MemoryGraphStore.find_learned_skill_by_normalized_title for learned.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §4.

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::memory_graph::MemoryGraphStore;
use crate::skills::SkillsRegistry;

pub struct LoadSkillTool {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub store: Arc<MemoryGraphStore>,
    pub app_handle: tauri::AppHandle,
    pub conversation_id: String,
    pub space_id: String,
}

impl LoadSkillTool {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        store: Arc<MemoryGraphStore>,
        app_handle: tauri::AppHandle,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, store, app_handle, conversation_id, space_id }
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load the full content of a skill. Use after skill_search identifies a promising match. The returned content is the skill's full prompt body — read it, then apply it to the current task."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Exact skill name." },
                "reason": {
                    "type": "string",
                    "description": "One sentence: why you're loading this skill in the current context. Surfaces as a chip in the UI; helps the user audit your reasoning."
                }
            },
            "required": ["name", "reason"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("name is required".into()))?
            .trim()
            .to_string();
        let reason = params["reason"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("reason is required".into()))?
            .trim()
            .to_string();

        // Builtin first
        {
            let registry = self.registry.read().await;
            if let Some(loaded) = registry.get_loaded(&name) {
                let result = json!({
                    "name": loaded.manifest.name,
                    "version": loaded.manifest.version,
                    "content": loaded.prompt_content,
                    "parameters": loaded.manifest.parameters.iter().map(|p| json!({
                        "name": p.name,
                        "type": p.r#type,
                        "required": p.required,
                        "description": p.description,
                    })).collect::<Vec<_>>(),
                    "provenance": "builtin",
                });
                self.emit_recalled(&params, "builtin", &name, &reason);
                return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
            }
        }

        // Learned — normalize title same way record_skill_cited does
        let normalized = name
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end_matches(|c: char| {
                matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '。' | '，' | '；' | '：' | '！' | '？')
            })
            .to_string();

        let node = self.store
            .find_learned_skill_by_normalized_title(&self.space_id, &normalized)
            .map_err(|e| ToolError::Execution(format!("lookup failed: {}", e)))?;

        let node = match node {
            Some(n) => n,
            None => {
                return Err(ToolError::Execution(format!("Skill '{}' not found", name)));
            }
        };

        let version = self.store
            .get_active_version(&node.id)
            .map_err(|e| ToolError::Execution(format!("get_active_version failed: {}", e)))?
            .ok_or_else(|| ToolError::Execution(format!("Skill '{}' has no active version", name)))?;

        // Bump usage_count for the load action (same counter as search; soft signal)
        let id_str = node.id.clone();
        if let Err(e) = self.store.bump_skill_usage(&[id_str.as_str()]) {
            tracing::warn!("bump_skill_usage failed: {}", e);
        }

        self.emit_recalled(&params, "learned", &node.title, &reason);

        let result = json!({
            "name": node.title,
            "version": version.id,
            "content": version.content,
            "parameters": [],
            "provenance": "learned",
        });
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}

impl LoadSkillTool {
    fn emit_recalled(&self, params: &serde_json::Value, provenance: &str, name: &str, reason: &str) {
        let tool_call_id = params["_tool_call_id"].as_str().unwrap_or("").to_string();
        let _ = self.app_handle.emit("agent:skill-recalled", json!({
            "conversationId": self.conversation_id,
            "toolCallId": tool_call_id,
            "kind": "load",
            "name": name,
            "reason": reason,
            "provenance": provenance,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
    use chrono::Utc;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned(store: &MemoryGraphStore, title: &str, body: &str) -> String {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        store.create_node(&MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": 0,
                "usage_count": 0,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        }).unwrap();
        store.create_version(&MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: body.into(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        }).unwrap();
        id
    }

    #[tokio::test]
    async fn learned_skill_loads_active_version_content() {
        let store = fresh_store();
        make_learned(&store, "stock-research", "# Stock Research SOP\n\nStep 1: ...");
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({
            "name": "stock-research",
            "reason": "User asked about Apple financials"
        })).await.unwrap();

        assert_eq!(out.result["provenance"], "learned");
        assert_eq!(out.result["name"], "stock-research");
        assert!(out.result["content"].as_str().unwrap().contains("Step 1"));
    }

    #[tokio::test]
    async fn unknown_skill_returns_tool_error() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let err = tool.execute(json!({
            "name": "does-not-exist",
            "reason": "trying"
        })).await.unwrap_err();

        match err {
            ToolError::Execution(msg) => assert!(msg.contains("not found")),
            other => panic!("expected Execution error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn load_bumps_usage_count() {
        let store = fresh_store();
        let id = make_learned(&store, "stock-research", "body");
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let _ = tool.execute(json!({
            "name": "stock-research",
            "reason": "trying"
        })).await.unwrap();

        let node = store.get_node(&id).unwrap().unwrap();
        let usage = node.metadata.unwrap().get("usage_count").unwrap().as_u64().unwrap();
        assert_eq!(usage, 1);
    }
}
```

### - [ ] Step 3.3: Run tests; fix issues

Run:
```bash
cd src-tauri && cargo test --lib load_skill 2>&1 | tail -30
```

Expected: 3 tests pass.

### - [ ] Step 3.4: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/builtin/mod.rs src-tauri/src/agent/tools/builtin/load_skill.rs
git commit -m "feat(agent): load_skill built-in tool

New tool fetches the full body of a skill by name. Resolution:
1. SkillsRegistry.get_loaded(name) — builtin hit, returns prompt_content
   + structured parameters from manifest
2. MemoryGraphStore.find_learned_skill_by_normalized_title — learned
   hit, returns memory_versions.content of the active version

Returns ToolError::Execution(\"Skill 'X' not found\") on miss.

Side effects:
- bump_skill_usage on learned hits (same counter as search; soft signal)
- emit agent:skill-recalled with kind=load + reason for UI chip

3 unit tests: learned success, unknown name → error, usage bump."
```

---

## Task 4: Frontend SkillRecallChips + atom + listener

**Files:**
- Create: `ui/src/components/agent/SkillRecallChips.tsx`
- Create: `ui/src/components/agent/SkillRecallChips.test.tsx`
- Modify: `ui/src/atoms/agent-atoms.ts` (add `SkillRecall` type + `skillRecallsMapAtom`)
- Modify: `ui/src/hooks/useGlobalAgentListeners.ts` (listen `agent:skill-recalled`)
- Modify: `ui/src/components/agent/AgentMessages.tsx` (mount chip below assistant message)

### - [ ] Step 4.1: Add type + atom

Open `ui/src/atoms/agent-atoms.ts`. Find the line that exports `liveMessagesMapAtom`. Right above or below it, add:

```typescript
export interface SkillRecall {
  toolCallId: string
  kind: 'search' | 'load'
  timestamp: string
  query?: string
  results?: Array<{
    name: string
    summary: string
    score: number
    provenance: 'learned' | 'builtin'
    cited_count?: number
  }>
  name?: string
  reason?: string
  provenance?: 'learned' | 'builtin'
}

export const skillRecallsMapAtom = atom<Map<string, SkillRecall[]>>(new Map())
```

### - [ ] Step 4.2: Add listener

Open `ui/src/hooks/useGlobalAgentListeners.ts`. Add `skillRecallsMapAtom` to the imports from `@/atoms/agent-atoms`. After the `chat:stream-tool-activity` listener block (around line 290), add:

```typescript
  // agent:skill-recalled → push into skillRecallsMapAtom (dedup by toolCallId)
  reg(
    listen<{
      conversationId: string
      toolCallId: string
      kind: 'search' | 'load'
      timestamp: string
      query?: string
      results?: Array<{ name: string; summary: string; score: number; provenance: 'learned' | 'builtin'; cited_count?: number }>
      name?: string
      reason?: string
      provenance?: 'learned' | 'builtin'
    }>('agent:skill-recalled', ({ payload }) => {
      const sid = payload.conversationId
      store.set(skillRecallsMapAtom, (prev) => {
        const next = new Map(prev)
        const current = next.get(sid) ?? []
        // Dedup by toolCallId — same call must not produce two chips
        if (current.some((r) => r.toolCallId === payload.toolCallId)) {
          return prev
        }
        next.set(sid, [...current, {
          toolCallId: payload.toolCallId,
          kind: payload.kind,
          timestamp: payload.timestamp,
          query: payload.query,
          results: payload.results,
          name: payload.name,
          reason: payload.reason,
          provenance: payload.provenance,
        }])
        return next
      })
    })
  )
```

### - [ ] Step 4.3: Create SkillRecallChips component

Create `ui/src/components/agent/SkillRecallChips.tsx`:

```tsx
/**
 * SkillRecallChips — renders chips for each skill_search / load_skill
 * invocation in the current session. Distinct from SkillCitationChips
 * (which renders post-application "applied skill X" pills).
 *
 * See docs/superpowers/specs/2026-05-12-skill-recall-design.md §6.
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Search, BookOpen } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { skillRecallsMapAtom, type SkillRecall } from '@/atoms/agent-atoms'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'

interface SkillRecallChipsProps {
  sessionId: string
  className?: string
}

export function SkillRecallChips({ sessionId, className }: SkillRecallChipsProps): React.ReactElement | null {
  const recallsMap = useAtomValue(skillRecallsMapAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  const recalls = recallsMap.get(sessionId) ?? []
  if (recalls.length === 0) return null

  const handleClick = (): void => {
    setSettingsTab('skills')
    setSettingsOpen(true)
  }

  return (
    <div className={cn('flex flex-wrap gap-1.5 mt-2 pl-[46px]', className)}>
      <TooltipProvider delayDuration={200}>
        {recalls.map((r) => (
          <ChipFor key={r.toolCallId} recall={r} onClick={handleClick} />
        ))}
      </TooltipProvider>
    </div>
  )
}

function ChipFor({ recall, onClick }: { recall: SkillRecall; onClick: () => void }): React.ReactElement {
  const isSearch = recall.kind === 'search'
  const Icon = isSearch ? Search : BookOpen
  const label = isSearch
    ? `搜索"${recall.query ?? ''}" → ${recall.results?.length ?? 0} 命中`
    : `加载"${recall.name ?? ''}"`
  const tooltipText = isSearch
    ? (recall.results && recall.results.length > 0
        ? recall.results.map((r) => `• ${r.name} (${r.provenance})`).join('\n')
        : '0 命中')
    : (recall.reason ?? '')

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
            'text-[11px] leading-tight',
            'bg-secondary/15 text-secondary-foreground border border-secondary/30',
            'hover:bg-secondary/25 hover:border-secondary/50',
            'transition-colors'
          )}
        >
          <Icon className="size-3" />
          <span className="truncate max-w-[280px]">{label}</span>
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText || '—'}
      </TooltipContent>
    </Tooltip>
  )
}
```

### - [ ] Step 4.4: Create Vitest cases

Create `ui/src/components/agent/SkillRecallChips.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { render, screen } from '@testing-library/react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { SkillRecallChips } from './SkillRecallChips'
import { skillRecallsMapAtom } from '@/atoms/agent-atoms'

function renderWith(sessionId: string, recalls: any[]) {
  const store = createStore()
  store.set(skillRecallsMapAtom, new Map([[sessionId, recalls]]))
  return render(
    <JotaiProvider store={store}>
      <TooltipProvider>
        <SkillRecallChips sessionId={sessionId} />
      </TooltipProvider>
    </JotaiProvider>
  )
}

describe('SkillRecallChips', () => {
  it('renders nothing when no recalls', () => {
    const { container } = renderWith('s1', [])
    expect(container.firstChild).toBeNull()
  })

  it('renders search chip with query and count', () => {
    renderWith('s1', [{
      toolCallId: 't1',
      kind: 'search',
      timestamp: '2026-05-12T00:00:00Z',
      query: 'stock financials',
      results: [
        { name: 'stock-research', summary: '...', score: 0.8, provenance: 'learned' },
        { name: 'api-blacklist', summary: '...', score: 0.5, provenance: 'learned' },
      ],
    }])
    expect(screen.getByText(/搜索"stock financials"/)).toBeInTheDocument()
    expect(screen.getByText(/2 命中/)).toBeInTheDocument()
  })

  it('renders load chip with skill name', () => {
    renderWith('s1', [{
      toolCallId: 't2',
      kind: 'load',
      timestamp: '2026-05-12T00:00:00Z',
      name: 'stock-research',
      reason: 'User asked about Apple',
      provenance: 'learned',
    }])
    expect(screen.getByText(/加载"stock-research"/)).toBeInTheDocument()
  })

  it('renders multiple chips for multiple recalls', () => {
    renderWith('s1', [
      { toolCallId: 't1', kind: 'search', timestamp: 'x', query: 'a', results: [] },
      { toolCallId: 't2', kind: 'load', timestamp: 'y', name: 'b', reason: 'r', provenance: 'learned' },
    ])
    expect(screen.getByText(/搜索"a"/)).toBeInTheDocument()
    expect(screen.getByText(/加载"b"/)).toBeInTheDocument()
  })
})
```

### - [ ] Step 4.5: Mount in AgentMessages

Open `ui/src/components/agent/AgentMessages.tsx`. Find the line with `<SkillCitationChips ... />` (around line 676). Add the import at the top of the file:

```typescript
import { SkillRecallChips } from './SkillRecallChips'
```

Then add the chip rendering BELOW the citation chip block. Find the location where `</Conversation>` or `</ConversationContent>` closes the assistant-message section, and add the recall chip there. The cleanest placement is right after the existing live-messages tail loop (around line 1024-1040 based on previous context). Add:

```tsx
            {/* Skill recall chips — separate visual lane from citation chips */}
            <SkillRecallChips sessionId={sessionId} />
```

### - [ ] Step 4.6: Run tests + tsc

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run SkillRecallChips 2>&1 | tail -15
```

Expected: `tsc` clean. 4 SkillRecallChips tests pass.

### - [ ] Step 4.7: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/agent/SkillRecallChips.tsx ui/src/components/agent/SkillRecallChips.test.tsx ui/src/atoms/agent-atoms.ts ui/src/hooks/useGlobalAgentListeners.ts ui/src/components/agent/AgentMessages.tsx
git commit -m "feat(ui): SkillRecallChips component + atom + listener

New chip surface for skill_search / load_skill tool invocations.
Distinct from existing SkillCitationChips (which fires post-application):

- skill_search: \"🔍 搜索 'X' → N 命中\" (tooltip lists matches)
- load_skill: \"📚 加载 'X'\" (tooltip shows agent's reason)

Lower-saturation color (bg-secondary/15) to distinguish from the
heavier citation chips. Same click target → Settings → 已学技能.

skillRecallsMapAtom holds per-session recalls (in-memory only,
session-switch keeps state, refresh clears). Dedup by toolCallId so
tool retry doesn't multi-render.

agent:skill-recalled listener in useGlobalAgentListeners pushes
into the map. 4 Vitest cases."
```

---

## Task 5: Wire it all together — manifest injection + tool registration

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs` (ChatDelegate fields + effective_system_prompt manifest append + skills_tokens accounting)
- Modify: `src-tauri/src/tauri_commands.rs:4252` (tool registration in `send_agent_message`)

### - [ ] Step 5.1: Add ChatDelegate field + setter (follows existing pattern)

Open `src-tauri/src/agent/dispatcher.rs`. ChatDelegate already uses setter-injection for optional state (see `set_memory_context`, `set_infra_service`, `set_trajectory_store`). Follow the same shape.

Find the `pub struct ChatDelegate` definition (lines 16-61). At the end of the struct (after `workspace_root`), add:

```rust
    /// Pre-built skill manifest block, set via set_skills_manifest_block
    /// before run_loop starts. Empty when no skills exist (no append).
    skills_manifest_block: String,
```

Find `ChatDelegate::new` (around line 64). In the `Self { ... }` block at line 76+, add `skills_manifest_block: String::new(),` next to the other defaulted fields.

Then add a setter at the bottom of the `impl ChatDelegate` block (right next to `set_memory_context`, around line 114):

```rust
    /// Set the skill manifest block to append to the system prompt.
    /// Caller is responsible for building this via skills_manifest::build_skills_manifest.
    pub fn set_skills_manifest_block(&mut self, block: String) {
        self.skills_manifest_block = block;
    }
```

### - [ ] Step 5.2: (merged into 5.1 — no separate task needed)

Skip — manifest is now built at the call site in tauri_commands.rs (Task 5.5), passed via setter.

### - [ ] Step 5.3: Append manifest in effective_system_prompt

Find `fn effective_system_prompt` (around line 128). Modify to append the manifest block:

```rust
fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String {
    let memory_block = self.memory_context.as_deref().filter(|s| !s.is_empty());
    let base_with_memory = match memory_block {
        Some(ctx) => format!("{}\n\n{}", self.system_prompt, ctx),
        None => self.system_prompt.clone(),
    };
    let composed = crate::agent::mode_prompts::compose_system_prompt(
        &base_with_memory,
        self.workspace_root.as_deref(),
        effective_mode,
    );
    // Append the skill manifest block (empty string when no skills exist).
    if self.skills_manifest_block.is_empty() {
        composed
    } else {
        format!("{}{}", composed, self.skills_manifest_block)
    }
}
```

### - [ ] Step 5.4: Update skills_tokens accounting

Find where `skills_tokens: 0` appears (around line 367). Replace with:

```rust
skills_tokens: estimate_tokens(&self.skills_manifest_block),
```

### - [ ] Step 5.5: Register tools + build manifest + call setter in send_agent_message

Open `src-tauri/src/tauri_commands.rs`. Find the tool registration block in `send_agent_message` (around line 4252). After existing registrations like `tools.register(builtin::shell::BashTool::new(...))`, add the two new tools (passing the same `Arc` clones already used elsewhere):

```rust
tools.register(builtin::skill_search::SkillSearchTool::new(
    Arc::clone(&state.skills_registry),
    Arc::clone(&state.memory_graph_store),
    app_handle.clone(),
    input.session_id.clone(),
    "default".into(),  // memory_graph already uses "default" space_id throughout
));
tools.register(builtin::load_skill::LoadSkillTool::new(
    Arc::clone(&state.skills_registry),
    Arc::clone(&state.memory_graph_store),
    app_handle.clone(),
    input.session_id.clone(),
    "default".into(),
));
```

Then find the `ChatDelegate::new(` call site below (use `grep -n "ChatDelegate::new" src-tauri/src/tauri_commands.rs` to locate it; likely around line 4290-4310). The call signature is **unchanged** (we use a setter, not a new arg). After the `let mut delegate = ChatDelegate::new(...)` line, build the manifest and call the setter:

```rust
let mut delegate = crate::agent::dispatcher::ChatDelegate::new(
    Arc::clone(&llm),
    Arc::clone(&tools),
    app_handle.clone(),
    model.clone(),
    AGENT_SYSTEM_PROMPT.to_string(),
    Arc::clone(&state.safety_manager),
    None,  // safety_mode override if any
    Arc::clone(&state.pending_approvals),
    input.session_id.clone(),
    Some(workspace.clone()),
);

// Build skill manifest in the async context (registry.read() is async).
{
    let registry = state.skills_registry.read().await;
    let manifest = crate::skills_manifest::build_skills_manifest(
        &registry,
        &state.memory_graph_store,
        "default",
        30,
        1500,
    );
    delegate.set_skills_manifest_block(manifest);
}
```

The exact existing args inside the `ChatDelegate::new(...)` call are already in place; do **not** rewrite that call — just add the manifest-build block right after it. If there are other `ChatDelegate::new` call sites (search: `grep -n "ChatDelegate::new" src-tauri/src/tauri_commands.rs`), add the same manifest-build block after each one. **Skip** call sites in unrelated paths (chat / queue / migrate) that don't need the manifest — those keep the empty default.

### - [ ] Step 5.6: Build + integration verify

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: 0 errors. Fix any borrow / move issues with `Arc::clone` calls.

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass (existing + the new manifest/search/load ones).

### - [ ] Step 5.7: Smoke test (manual, not automated)

Start `cargo tauri dev`. In an agent session, the LLM should now receive the system prompt with the manifest block appended. To validate:

1. Open DevTools → Network or backend log → look for the `system_prompt: Some(...)` in `Calling LLM` tracing line, OR add a temporary `tracing::debug!("system_prompt: {}", effective_prompt);` in `call_llm` and grep the log.
2. If there are zero skills, the block is omitted (no `## 你已学习到的技能` in the prompt).
3. Send a message designed to trigger search: "我之前怎么处理 API key 失败的来着？". Watch the activity panel — expect `skill_search` tool call to fire if there are matching learned skills.
4. Verify chip appears below assistant turn after the tool fires.

### - [ ] Step 5.8: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/dispatcher.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(agent): inject skill manifest + register skill tools

Wires Task 1-4 into the runtime:

1. ChatDelegate gains a `skills_manifest_block: String` field + a
   `set_skills_manifest_block` setter (following the existing
   set_memory_context / set_infra_service pattern).
2. effective_system_prompt appends manifest after mode_prompts; empty
   string short-circuits (no header / footer with no body).
3. skills_tokens accounting (previously hardcoded 0) now reflects
   real token cost via estimate_tokens.
4. send_agent_message registers SkillSearchTool and LoadSkillTool
   alongside file/web/shell tools. Manifest is built in the async
   call site (registry.read().await) and pushed via the setter.

This is the closing commit of the skill-recall closed loop:
  proactive extraction → memory_graph store → manifest in system
  prompt → agent decides to use → skill_search/load_skill tools →
  bump_skill_usage / record_skill_cited counters → next session's
  manifest re-ranks.

Spec: docs/superpowers/specs/2026-05-12-skill-recall-design.md"
```

---

## Self-Review Checklist

After all 5 tasks complete:

- [ ] Run full Rust test suite: `cd src-tauri && cargo test --lib 2>&1 | tail -5` — expect all green
- [ ] Run full TS check: `cd ui && npx tsc --noEmit` — expect clean
- [ ] Run full Vitest: `cd ui && npm test -- --run 2>&1 | tail -5` — expect all green
- [ ] `git log --oneline main..HEAD` shows exactly 5 commits with the planned prefixes
- [ ] Each commit independently compiles (bisectable)
- [ ] No PRs use `// TODO` placeholders in production code
- [ ] Verify the dead `SDKMessageRenderer` code is NOT reactivated — we deliberately don't render `compact_boundary` from `liveMessages` here (that was a separate hotfix in PR #99)

## Out-of-scope for this PR

Per spec §"Out-of-scope, noted for later":

- Semantic / embedding search (keyword-only for v1)
- Cross-skill conflict resolution
- Counter decay (cited_count accumulates forever today)
- Builtin skill usage counters
- "Skill activity" Settings panel showing recall history

## Done definition

This plan is "done" when:
1. `git log` shows 5 commits matching the planned subjects
2. CI / `cargo test` / `vitest` / `tsc` all green
3. Manual smoke (Task 5 step 7) confirms manifest reaches LLM + chip renders
4. PR opened with body referencing spec doc + commit table
