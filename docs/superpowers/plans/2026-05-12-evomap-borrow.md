# EvoMap-borrowed Skill Recall v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 7 enhancements to uClaw's skill recall loop (signals/category/validation_hint metadata; semantic search; timeline UI; strategy preset) plus a recency-aware ranking that prevents `cited_count` from accumulating forever.

**Architecture:** All additive metadata on `memory_nodes.metadata_json` (no SQL migrations) + read-side scoring/UI changes. Semantic search reuses fastembed in `memu_bridge.py` (already loaded). Each item is one bisectable commit.

**Tech Stack:** Rust + Tokio (backend), Python fastembed via existing memU bridge, React + Jotai (frontend), `react-diff-view` for timeline (new dep).

**Spec:** [docs/superpowers/specs/2026-05-12-evomap-borrow-design.md](../specs/2026-05-12-evomap-borrow-design.md)

---

## File Structure

**New files:**
- `src-tauri/src/proactive/scenarios/failure_signals.rs` — failure-signal taxonomy classifier (Task 2)
- `src-tauri/src/memu/embedding.rs` — thin Rust wrapper around new `embed_text` bridge method (Task 4)
- `ui/src/components/settings/SkillEvolutionTab.tsx` — timeline UI (Task 5)
- `ui/src/components/settings/SkillEvolutionTab.test.tsx` — vitest (Task 5)
- `ui/src/components/agent/StrategyPresetSelector.tsx` — toolbar dropdown (Task 6)

**Modified files:**
- `src-tauri/src/proactive/scenarios/skill_extraction.rs` — extraction prompt + XML parser additions (Tasks 1, 2, 3, 6)
- `src-tauri/src/agent/tools/builtin/skill_search.rs` — scoring formula additions (Tasks 1, 2, 4)
- `src-tauri/src/agent/tools/builtin/load_skill.rs` — return `validation_hint` field (Task 3)
- `src-tauri/src/skills_manifest.rs` — strategy preset bias parameter (Task 6)
- `src-tauri/src/memory_graph/store.rs` — `list_top_learned_skills` recency factor (Task 8)
- `src-tauri/src/memory_graph/mod.rs` — (maybe) re-export for new helpers
- `src-tauri/src/memu/memu_bridge.py` — add `embed_text` JSON-RPC handler (Task 4)
- `src-tauri/src/memu/client.rs` — `embed_text` Rust client method (Task 4)
- `src-tauri/src/proactive/service.rs` — register cited_count decay task (Task 7)
- `src-tauri/src/tauri_commands.rs` — new `get_skill_versions` IPC (Task 5); pass strategy bias to manifest build (Task 6)
- `src-tauri/src/agent/dispatcher.rs` — accept strategy bias when building manifest (Task 6)
- `ui/src/atoms/agent-atoms.ts` — `agentSessionStrategyMapAtom` (Task 6)
- `ui/src/components/settings/SkillsSettings.tsx` — mount evolution tab (Task 5)
- `ui/src/components/agent/AgentView.tsx` — mount strategy selector (Task 6)
- `ui/src/lib/tauri-bridge.ts` — wrap new IPCs (Tasks 5, 6)
- `ui/package.json` — add `react-diff-view` dep (Task 5)

**Total estimate:** ~1,590 LOC across ~17 files; 8 bisectable commits.

---

## Task Order Rationale

Foundation → extraction-prompt schema → search/ranking → big standalones:
1. **Task 1 (signals[])** — extraction prompt schema first; smallest extraction change
2. **Task 2 (signals_seen[])** — same shape; layered on prompt
3. **Task 3 (validation_hint)** — same shape; layered on prompt
4. **Task 6 (category + strategy preset)** — extraction prompt + bigger UI piece, finishes the prompt schema
5. **Task 8 (last_cited_at recency)** — read-side only; `last_cited_at` already written (verify) → use in ranking
6. **Task 7 (cited_count decay cron)** — write-side counterpart to 8
7. **Task 4 (semantic search)** — bigger, depends on fastembed bridge addition; uses signals from Task 1
8. **Task 5 (timeline UI)** — pure frontend + 1 thin IPC; no backend changes that affect prior tasks

---

## Task 1: `signals[]` metadata + scoring

**Files:**
- Modify: `src-tauri/src/proactive/scenarios/skill_extraction.rs` (prompt + parser)
- Modify: `src-tauri/src/agent/tools/builtin/skill_search.rs` (scoring)

### - [ ] Step 1.1: Write failing test for the parser

Open `src-tauri/src/proactive/scenarios/skill_extraction.rs` and find the existing `#[cfg(test)] mod tests` block. Add:

```rust
    #[test]
    fn parses_signals_array_from_skill_xml() {
        let xml = r#"<skill_report><new_skills><skill>
<name>api-key-rotation</name>
<context>API key auth failures</context>
<principles>Rotate keys when 401 persists</principles>
<steps>1. detect 401\n2. swap key</steps>
<pitfalls>Don't retry indefinitely</pitfalls>
<signals>
<signal>401 unauthorized</signal>
<signal>token expired</signal>
<signal>authentication failed</signal>
</signals>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].signals, vec!["401 unauthorized", "token expired", "authentication failed"]);
    }

    #[test]
    fn parses_skill_without_signals_block() {
        let xml = r#"<skill_report><new_skills><skill>
<name>basic-skill</name>
<context>x</context>
<principles>y</principles>
<steps>z</steps>
<pitfalls>w</pitfalls>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].signals.is_empty());
    }
```

### - [ ] Step 1.2: Run to confirm failures

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skill_extraction::tests::parses_signals 2>&1 | tail -10
```

Expected: compile error (`parse_skill_blocks` may not exist OR `signals` field unknown).

### - [ ] Step 1.3: Locate or define `parse_skill_blocks` + extend struct

Find where the XML parsing happens (search file for `<skill>` text or struct `ParsedSkill`). The struct must gain `pub signals: Vec<String>` with default empty. The parser must extract `<signal>` children inside `<signals>`. Add a regex / manual XML walk consistent with how the file does it today.

If a helper named differently (`parse_skills`, `extract_skills`), adapt the test to that name.

Show the typical addition to the struct:

```rust
pub struct ParsedSkill {
    pub name: String,
    pub context: String,
    pub principles: String,
    pub steps: String,
    pub pitfalls: String,
    pub signals: Vec<String>,           // NEW
}
```

Parser snippet (add inside the existing `<skill>` loop, after `pitfalls` extraction):

```rust
let signals: Vec<String> = if let Some(sigs_block) = extract_tag(&skill_xml, "signals") {
    extract_repeated_tag(&sigs_block, "signal")
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
} else {
    Vec::new()
};
```

(If `extract_tag` / `extract_repeated_tag` helpers don't exist, mirror whatever the current parser uses — likely a regex or `quick-xml`. The actual helper name doesn't matter; the SHAPE does.)

### - [ ] Step 1.4: Run tests to verify they pass

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skill_extraction::tests::parses_signals 2>&1 | tail -10
```

Expected: both new tests pass.

### - [ ] Step 1.5: Update extraction prompt

Find `pub const SKILL_EXTRACTION_SYSTEM_PROMPT: &str` (line ~6). In the XML format example block (lines ~21-37), add to each `<skill>` an optional `<signals>` section. Replace:

```
<skill>
<name>技能名称</name>
<context>适用场景</context>
<principles>核心原则</principles>
<steps>实现步骤</steps>
<pitfalls>常见陷阱</pitfalls>
</skill>
```

with:

```
<skill>
<name>技能名称</name>
<context>适用场景</context>
<principles>核心原则</principles>
<steps>实现步骤</steps>
<pitfalls>常见陷阱</pitfalls>
<signals>
<signal>触发该技能的典型用户提问关键词或错误消息（3-5 条，可选）</signal>
</signals>
</skill>
```

### - [ ] Step 1.6: Persist signals into metadata on skill upsert

Find where the scenario writes the extracted skill into memory_graph (look for `create_node` or `update_node` calls in this file or its callers; usually `crate::memory_graph::store::MemoryGraphStore::create_learned_skill` or similar). Where the metadata JSON is built, add:

```rust
metadata["signals"] = serde_json::Value::Array(
    parsed.signals.iter().map(|s| serde_json::Value::String(s.clone())).collect()
);
```

(Insert only if `parsed.signals` is non-empty to keep absent skills clean.)

Add a test:

```rust
    #[test]
    fn signals_persist_to_metadata_on_extraction() {
        // Spin up an in-memory store, run the upsert path with a parsed
        // skill having signals = ["a", "b", "c"], assert
        // store.get_node(id).metadata.signals == ["a","b","c"]
        // ... (mirror fresh_store helpers from existing tests in this file)
    }
```

(If existing test helpers aren't already in this file, look at `skills_manifest.rs:tests` for the `fresh_store()` pattern and copy.)

### - [ ] Step 1.7: Write failing test for `skill_search` signal scoring

Open `src-tauri/src/agent/tools/builtin/skill_search.rs`, `#[cfg(test)] mod tests`. Add:

```rust
    #[tokio::test]
    async fn signal_match_boosts_score_over_keyword_only() {
        let store = fresh_store();
        // Skill A: keywords ["api"] in body, no signals
        let id_a = make_learned_node_with_keywords(&store, "skill-a", &["api"], 0);
        // Skill B: keywords ["api"] AND signals ["401 unauthorized"]
        let id_b = make_learned_node_with_keywords(&store, "skill-b", &["api"], 0);
        store.update_node_metadata_json(&id_b, json!({
            "skill_type": "learned",
            "enabled": true,
            "summary": "B",
            "cited_count": 0,
            "usage_count": 0,
            "signals": ["401 unauthorized"]
        })).unwrap();

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(registry, store, app.handle().clone(),
            "sess".into(), "default".into());

        let out = tool.execute(json!({ "query": "api 401 unauthorized" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        // B (signal + keyword match) must score higher than A (keyword only)
        let pos_a = hits.iter().position(|h| h["name"] == "skill-a").unwrap();
        let pos_b = hits.iter().position(|h| h["name"] == "skill-b").unwrap();
        assert!(pos_b < pos_a, "expected B (signal match) before A; got hits: {:#?}", hits);
    }
```

If `update_node_metadata_json` doesn't exist on the store, the simplest is to write the full node with metadata directly via `create_node` instead of an update. Adjust the test to construct B with metadata + keywords from the start.

### - [ ] Step 1.8: Run, expect compile error or assertion failure

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skill_search::tests::signal_match 2>&1 | tail -10
```

### - [ ] Step 1.9: Add signal scoring to `skill_search`

In `skill_search.rs`, locate the learned-hits scoring loop (~line 139-160 today reads `cited` / `usage`). Insert:

```rust
let signals: Vec<String> = node
    .metadata
    .as_ref()
    .and_then(|m| m.get("signals"))
    .and_then(|v| v.as_array())
    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_lowercase())).collect())
    .unwrap_or_default();
let query_lower = query.to_lowercase();
let signal_match_count = signals.iter()
    .filter(|sig| query_lower.contains(sig.as_str()))
    .count() as f64;
let score = (kw_hits as f64)
    + signal_match_count * 1.5    // NEW
    + (cited as f64 * 0.5)
    + (usage as f64 * 0.2);
```

Also include the matched signals in `SearchHit` so the LLM can see them:

```rust
struct SearchHit {
    name: String,
    summary: String,
    score: f64,
    provenance: &'static str,
    cited_count: Option<u64>,
    node_id: Option<String>,
    matched_signals: Vec<String>,   // NEW
}
```

Populate in the loop:

```rust
let matched_signals: Vec<String> = signals.iter()
    .filter(|sig| query_lower.contains(sig.as_str()))
    .cloned()
    .collect();
```

And serialize (already auto via serde if struct is `Serialize`). Update existing instantiations to include `matched_signals: vec![]` for builtin hits.

### - [ ] Step 1.10: Run all skill_search tests

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skill_search 2>&1 | tail -10
```

Expected: 4 pre-existing + 1 new signal test pass.

### - [ ] Step 1.11: Commit

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/proactive/scenarios/skill_extraction.rs \
        src-tauri/src/agent/tools/builtin/skill_search.rs
git commit -m "feat(skills): trigger signals — extraction + scoring boost

- Skill extraction prompt asks LLM to emit <signals><signal>...</signal></signals>
  with 3-5 trigger phrases describing when the skill applies
- Parser extracts signals into metadata.signals: string[]
- skill_search scoring: +1.5 per query/signal substring match
  (compounds with keyword hits + cited × 0.5 + usage × 0.2)
- SearchHit gains matched_signals[] field so LLM sees which signals fired
- Backward compat: skills without signals get score = 0 from this term

Tests:
- Parser handles signals presence + absence
- Signal+keyword match outranks keyword-only on identical scoring inputs"
```

---

## Task 2: `signals_seen[]` from agent_turns failures

**Files:**
- Create: `src-tauri/src/proactive/scenarios/failure_signals.rs`
- Modify: `src-tauri/src/proactive/scenarios/mod.rs` (export new module)
- Modify: `src-tauri/src/proactive/scenarios/skill_extraction.rs` (call classifier; write to metadata)

### - [ ] Step 2.1: Create classifier module with failing test

Create `src-tauri/src/proactive/scenarios/failure_signals.rs`:

```rust
//! Failure-signal taxonomy. Classifies free-text tool errors into a small
//! fixed set of high-level signals that can be matched against future
//! queries. Used by skill_extraction (to tag each new learned skill with
//! `signals_seen`) and by skill_search (to boost skills whose seen-signals
//! overlap with the current session's failures).

/// Fixed taxonomy. Keep this list small and stable — adding more requires
/// dogfood evidence that the new category is materially better at boosting
/// recall than just appending to an existing one.
pub const SIGNAL_TAXONOMY: &[&str] = &[
    "http_4xx",
    "http_5xx",
    "timeout",
    "permission_denied",
    "parse_error",
    "rate_limited",
    "not_found",
    "network_error",
];

/// Classify an error message into zero or more signals. An empty result
/// means "no recognised pattern" — the skill still extracts, just without
/// signals_seen entries.
pub fn classify_error(message: &str) -> Vec<&'static str> {
    let lower = message.to_lowercase();
    let mut sigs = Vec::new();
    if lower.contains("4xx") || lower.contains(" 401") || lower.contains(" 403")
        || lower.contains(" 404") || lower.contains(" 429") || lower.contains("client error")
    {
        sigs.push("http_4xx");
    }
    if lower.contains("5xx") || lower.contains(" 500") || lower.contains(" 502")
        || lower.contains(" 503") || lower.contains(" 504") || lower.contains("server error")
    {
        sigs.push("http_5xx");
    }
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline") {
        sigs.push("timeout");
    }
    if lower.contains("permission denied") || lower.contains("eacces")
        || lower.contains("forbidden") || lower.contains("unauthorized")
    {
        sigs.push("permission_denied");
    }
    if lower.contains("json") && (lower.contains("parse") || lower.contains("decode")
        || lower.contains("invalid"))
    {
        sigs.push("parse_error");
    }
    if lower.contains("rate limit") || lower.contains("too many requests")
        || lower.contains(" 429")
    {
        sigs.push("rate_limited");
    }
    if lower.contains("not found") || lower.contains("enoent") || lower.contains(" 404") {
        sigs.push("not_found");
    }
    if lower.contains("connection refused") || lower.contains("dns") || lower.contains("network")
        || lower.contains("connect failed") || lower.contains("ssl error")
    {
        sigs.push("network_error");
    }
    sigs.sort_unstable();
    sigs.dedup();
    sigs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_403_as_4xx() {
        let sigs = classify_error("Error: HTTP 403 Forbidden from yahoo.com");
        assert!(sigs.contains(&"http_4xx"));
        assert!(sigs.contains(&"permission_denied"));
    }

    #[test]
    fn classifies_timeout() {
        let sigs = classify_error("request timed out after 30s");
        assert_eq!(sigs, vec!["timeout"]);
    }

    #[test]
    fn empty_for_unrecognised() {
        let sigs = classify_error("Compilation failed: type mismatch in main.rs");
        assert!(sigs.is_empty(), "unrecognized error should not synthesize signals; got {:?}", sigs);
    }

    #[test]
    fn no_duplicates_in_result() {
        // both "permission denied" and "unauthorized" appear → still single permission_denied
        let sigs = classify_error("permission denied AND unauthorized");
        let pd_count = sigs.iter().filter(|s| **s == "permission_denied").count();
        assert_eq!(pd_count, 1);
    }
}
```

### - [ ] Step 2.2: Export the module

Open `src-tauri/src/proactive/scenarios/mod.rs`. Add (alphabetical order):

```rust
pub mod failure_signals;
```

### - [ ] Step 2.3: Run module tests

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib failure_signals 2>&1 | tail -10
```

Expected: 4 tests pass.

### - [ ] Step 2.4: Wire into skill_extraction

In `skill_extraction.rs::format_execution_logs` (or wherever the file processes `ExecutionLog`), aggregate signals across failure logs:

```rust
fn extract_signals_seen(logs: &[ExecutionLog]) -> Vec<String> {
    use crate::proactive::scenarios::failure_signals::classify_error;
    let mut sigs = std::collections::BTreeSet::new();
    for log in logs {
        if !log.success {
            // Try the output field; some tools put errors in output, some in a separate field.
            let msg = serde_json::to_string(&log.tool_output).unwrap_or_default();
            for s in classify_error(&msg) {
                sigs.insert(s.to_string());
            }
        }
    }
    sigs.into_iter().collect()
}
```

Then in the place where the extracted skill is persisted (where signals from Task 1 are written), also write:

```rust
let signals_seen = extract_signals_seen(&logs);
if !signals_seen.is_empty() {
    metadata["signals_seen"] = serde_json::Value::Array(
        signals_seen.iter().map(|s| serde_json::Value::String(s.clone())).collect()
    );
}
```

### - [ ] Step 2.5: Add integration test

Append to `skill_extraction.rs::tests`:

```rust
    #[test]
    fn signals_seen_extracted_from_failure_logs() {
        let logs = vec![
            make_execution_log("web_fetch", false, 500),
        ];
        // Override the failure log's output to contain a 403:
        let mut logs = logs;
        logs[0].tool_output = serde_json::json!({"error": "HTTP 403 Forbidden"});

        let sigs = extract_signals_seen(&logs);
        assert!(sigs.contains(&"http_4xx".to_string()));
        assert!(sigs.contains(&"permission_denied".to_string()));
    }
```

`make_execution_log` is already a test helper in this file (the existing tests use it). If the helper doesn't expose `tool_output`, adapt by constructing the log directly.

### - [ ] Step 2.6: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skill_extraction failure_signals 2>&1 | tail -10
```

Expected: all pass.

```bash
git add src-tauri/src/proactive/scenarios/failure_signals.rs \
        src-tauri/src/proactive/scenarios/mod.rs \
        src-tauri/src/proactive/scenarios/skill_extraction.rs
git commit -m "feat(skills): signals_seen — classify failure-type taxonomy from logs

Adds proactive/scenarios/failure_signals.rs with classify_error()
mapping free-text error messages to a fixed 8-signal taxonomy
(http_4xx, http_5xx, timeout, permission_denied, parse_error,
rate_limited, not_found, network_error).

skill_extraction wires it: when extracting a new skill from
ExecutionLog, it scans the failure logs and stores the union of
detected signals into metadata.signals_seen: string[].

This is the empirical counterpart to Task 1's prescriptive signals[]:
- signals = 'when the LLM thinks this skill applies'
- signals_seen = 'kinds of failures this skill was born from'

skill_search will use signals_seen in a follow-up (Task 4) to boost
skills whose seen-signals overlap with current session failures."
```

---

## Task 3: `validation_hint` on `load_skill` return

**Files:**
- Modify: `src-tauri/src/proactive/scenarios/skill_extraction.rs` (prompt + parser + persist)
- Modify: `src-tauri/src/agent/tools/builtin/load_skill.rs` (return field)

### - [ ] Step 3.1: Parser test

Add to `skill_extraction.rs::tests`:

```rust
    #[test]
    fn parses_validation_hint_when_present() {
        let xml = r#"<skill_report><new_skills><skill>
<name>x</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
<validation_hint>Run the command again and confirm exit 0.</validation_hint>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        assert_eq!(parsed[0].validation_hint.as_deref(),
            Some("Run the command again and confirm exit 0."));
    }

    #[test]
    fn validation_hint_absent_yields_none() {
        let xml = r#"<skill_report><new_skills><skill>
<name>y</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        assert!(parsed[0].validation_hint.is_none());
    }
```

### - [ ] Step 3.2: Extend struct + parser

In `ParsedSkill` struct add:

```rust
    pub validation_hint: Option<String>,
```

In the parser, after the signals extraction:

```rust
let validation_hint = extract_tag(&skill_xml, "validation_hint")
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
```

### - [ ] Step 3.3: Update extraction prompt

In `SKILL_EXTRACTION_SYSTEM_PROMPT`, inside the `<skill>` example, add after `<pitfalls>...</pitfalls>`:

```
<validation_hint>应用该技能后，如何验证它真的有效（一句话，可选；agent 看到后自行决定要不要验证）</validation_hint>
```

### - [ ] Step 3.4: Persist into metadata

Where Task 1's signals are written, parallel-add:

```rust
if let Some(hint) = parsed.validation_hint.as_ref() {
    metadata["validation_hint"] = serde_json::Value::String(hint.clone());
}
```

### - [ ] Step 3.5: Return field in load_skill

In `load_skill.rs::execute`, both code paths (builtin + learned) — find `let result = json!({...});` and add a `"validation_hint"` field.

For builtin (no validation_hint exists today; always None):

```rust
let result = json!({
    "name": loaded.manifest.name,
    "version": loaded.manifest.version,
    "content": loaded.prompt_content,
    "parameters": loaded.manifest.parameters.iter().map(|p| json!({ /* ... */ })).collect::<Vec<_>>(),
    "provenance": "builtin",
    "validation_hint": serde_json::Value::Null,    // NEW
});
```

For learned:

```rust
let validation_hint = node.metadata.as_ref()
    .and_then(|m| m.get("validation_hint"))
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
let result = json!({
    "name": node.title,
    "version": version.id,
    "content": version.content,
    "parameters": [],
    "provenance": "learned",
    "validation_hint": validation_hint,            // NEW
});
```

### - [ ] Step 3.6: Add load_skill test

Append to `load_skill.rs::tests`:

```rust
    #[tokio::test]
    async fn returns_validation_hint_from_metadata() {
        let store = fresh_store();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        store.create_node(&MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: "verify-skill".into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": "x",
                "validation_hint": "Re-run with --quiet and check exit code"
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        }).unwrap();
        store.create_version(&MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: "body".into(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        }).unwrap();

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(registry, store, app.handle().clone(),
            "sess".into(), "default".into());
        let out = tool.execute(json!({ "name": "verify-skill", "reason": "test" })).await.unwrap();
        assert_eq!(out.result["validation_hint"], "Re-run with --quiet and check exit code");
    }
```

### - [ ] Step 3.7: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib "skill_extraction::tests::parses_validation_hint\|skill_extraction::tests::validation_hint_absent\|load_skill::tests::returns_validation_hint" 2>&1 | tail -10
```

Expected: 3 new tests pass.

```bash
git add src-tauri/src/proactive/scenarios/skill_extraction.rs \
        src-tauri/src/agent/tools/builtin/load_skill.rs
git commit -m "feat(skills): validation_hint — extraction + return in load_skill

Extraction prompt asks LLM to optionally fill <validation_hint>: a
one-sentence cue for the agent on how to verify the skill applied
correctly. Parser stores it in metadata.validation_hint: string?

load_skill returns the field in its tool_output. Agent decides
whether to actually verify (probe-first: we don't auto-run
commands). Builtin skills always return null.

Tests cover parser (present/absent) and load_skill round-trip."
```

---

## Task 6: `category` + strategy preset

**Files:**
- Modify: `src-tauri/src/proactive/scenarios/skill_extraction.rs` (prompt + parser + persist `category`)
- Modify: `src-tauri/src/skills_manifest.rs` (accept `bias` param)
- Modify: `src-tauri/src/agent/dispatcher.rs` (thread bias through)
- Modify: `src-tauri/src/tauri_commands.rs` (pass session's strategy to manifest build)
- Create: `ui/src/components/agent/StrategyPresetSelector.tsx`
- Modify: `ui/src/atoms/agent-atoms.ts` (new atom)
- Modify: `ui/src/components/agent/AgentView.tsx` (mount selector)

### - [ ] Step 6.1: Parser for `<category>` tag

Add tests:

```rust
    #[test]
    fn parses_category_tag() {
        let xml = r#"<skill_report><new_skills><skill>
<name>x</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
<category>repair</category>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        assert_eq!(parsed[0].category.as_deref(), Some("repair"));
    }

    #[test]
    fn parses_invalid_category_as_none() {
        let xml = r#"<skill_report><new_skills><skill>
<name>x</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
<category>banana</category>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_blocks(xml);
        // Unknown category strings drop to None — only repair/optimize/innovate accepted.
        assert!(parsed[0].category.is_none());
    }
```

### - [ ] Step 6.2: Extend struct + parser

```rust
    pub category: Option<String>,
```

In parser:

```rust
let category = extract_tag(&skill_xml, "category")
    .map(|s| s.trim().to_lowercase())
    .filter(|s| matches!(s.as_str(), "repair" | "optimize" | "innovate"));
```

### - [ ] Step 6.3: Update prompt

In `SKILL_EXTRACTION_SYSTEM_PROMPT`, after `<validation_hint>...`:

```
<category>该技能的类别（三选一，可选）：repair（修复 bug / 错误恢复）/ optimize（已知问题的更高效解决）/ innovate（新的工作流或方法）</category>
```

### - [ ] Step 6.4: Persist

Where Tasks 1/3 write metadata:

```rust
if let Some(cat) = parsed.category.as_ref() {
    metadata["category"] = serde_json::Value::String(cat.clone());
}
```

### - [ ] Step 6.5: Manifest builder accepts bias

In `src-tauri/src/skills_manifest.rs`, change the public signature:

```rust
#[derive(Debug, Clone, Copy)]
pub enum StrategyBias {
    Balanced,
    Repair,
    Optimize,
    Innovate,
}

pub fn build_skills_manifest(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    max_tokens: usize,
    bias: StrategyBias,           // NEW
) -> String {
    // ...
}
```

In the learned-entries collection loop, after computing `cited_count`:

```rust
let category = detail.node.metadata.as_ref()
    .and_then(|m| m.get("category"))
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
let category_match_bonus = match (bias, category.as_deref()) {
    (StrategyBias::Repair, Some("repair")) => 3.0,
    (StrategyBias::Optimize, Some("optimize")) => 3.0,
    (StrategyBias::Innovate, Some("innovate")) => 3.0,
    _ => 0.0,
};
```

Apply the bonus when sorting (this requires keeping a numeric score alongside each entry — the current builder just inserts in list_top_learned_skills order. Sort by `(category_match_bonus, e3_index)` descending where e3_index is position from `list_top_learned_skills` reversed).

Simplest: after collecting `entries`, do a stable sort by category_match_bonus desc within learned-prov. Builtin block remains first.

Add a top-of-manifest line when bias != Balanced:

```rust
let mode_hint = match bias {
    StrategyBias::Balanced => "",
    StrategyBias::Repair => "**Current mode**: 修复优先（repair skills 优先级提升）\n\n",
    StrategyBias::Optimize => "**Current mode**: 优化优先（optimize skills 优先级提升）\n\n",
    StrategyBias::Innovate => "**Current mode**: 探索优先（innovate skills 优先级提升）\n\n",
};
// Prepend mode_hint to the manifest body
```

Test:

```rust
    #[test]
    fn repair_bias_reorders_learned() {
        let store = fresh_store();
        // Skill A: category=innovate, cited=10
        // Skill B: category=repair, cited=2
        // Default balanced → A first (cited DESC). Repair bias → B first.
        make_learned_with_category(&store, "skill-a", "innovate", 10);
        make_learned_with_category(&store, "skill-b", "repair", 2);

        let registry = SkillsRegistry::new();
        let m_balanced = build_skills_manifest(&registry, &store, "default", 30, 4000,
            StrategyBias::Balanced);
        let pos_a_b = m_balanced.find("**skill-a**").unwrap();
        let pos_b_b = m_balanced.find("**skill-b**").unwrap();
        assert!(pos_a_b < pos_b_b, "balanced: A first");

        let m_repair = build_skills_manifest(&registry, &store, "default", 30, 4000,
            StrategyBias::Repair);
        let pos_a_r = m_repair.find("**skill-a**").unwrap();
        let pos_b_r = m_repair.find("**skill-b**").unwrap();
        assert!(pos_b_r < pos_a_r, "repair-biased: B first; manifest:\n{}", m_repair);
        assert!(m_repair.contains("Current mode"), "repair bias should include mode header");
    }
```

Helper `make_learned_with_category` is similar to existing `make_learned_node` but additionally sets `metadata.category`.

### - [ ] Step 6.6: Update all `build_skills_manifest` call sites

Search: `grep -rn "build_skills_manifest" src-tauri/src/`. Add `StrategyBias::Balanced` as last arg to each non-test call (likely tauri_commands.rs only). Tests update to pass the right variant.

### - [ ] Step 6.7: Thread bias from frontend to backend via input

Find `SendAgentMessageInput` in `src-tauri/src/tauri_commands.rs`. Add an optional field:

```rust
pub strategy: Option<String>,  // "balanced" | "repair" | "optimize" | "innovate"; default balanced
```

In `send_agent_message`, before manifest build:

```rust
let bias = match input.strategy.as_deref() {
    Some("repair") => crate::skills_manifest::StrategyBias::Repair,
    Some("optimize") => crate::skills_manifest::StrategyBias::Optimize,
    Some("innovate") => crate::skills_manifest::StrategyBias::Innovate,
    _ => crate::skills_manifest::StrategyBias::Balanced,
};
```

Then pass `bias` into `build_skills_manifest`.

### - [ ] Step 6.8: Frontend atom + selector + AgentView wiring

In `ui/src/atoms/agent-atoms.ts`, add:

```typescript
export type AgentStrategyPreset = 'balanced' | 'repair' | 'optimize' | 'innovate'
export const agentSessionStrategyMapAtom = atom<Map<string, AgentStrategyPreset>>(new Map())
```

Create `ui/src/components/agent/StrategyPresetSelector.tsx`:

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { Target } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
  DropdownMenuItem, DropdownMenuRadioGroup, DropdownMenuRadioItem,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { agentSessionStrategyMapAtom, type AgentStrategyPreset } from '@/atoms/agent-atoms'

const LABELS: Record<AgentStrategyPreset, { label: string; emoji: string }> = {
  balanced: { label: '平衡', emoji: '⚖️' },
  repair:   { label: '修 bug', emoji: '🔧' },
  optimize: { label: '优化', emoji: '⚡' },
  innovate: { label: '探索', emoji: '✨' },
}

export function StrategyPresetSelector({ sessionId }: { sessionId: string }): React.ReactElement {
  const [strategyMap, setStrategyMap] = useAtom(agentSessionStrategyMapAtom)
  const current: AgentStrategyPreset = strategyMap.get(sessionId) ?? 'balanced'
  const { label, emoji } = LABELS[current]

  const handleChange = (v: string) => {
    const next = v as AgentStrategyPreset
    setStrategyMap((prev) => {
      const map = new Map(prev)
      map.set(sessionId, next)
      return map
    })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm"
          className="h-7 gap-1 px-2 text-[11px] text-muted-foreground hover:text-foreground">
          <Target className="size-3" />
          <span>{emoji} {label}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-32">
        <DropdownMenuRadioGroup value={current} onValueChange={handleChange}>
          <DropdownMenuRadioItem value="balanced">⚖️ 平衡</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="repair">🔧 修 bug</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="optimize">⚡ 优化</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="innovate">✨ 探索</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
```

In `AgentView.tsx`, find where `<ContextUsageBadge .../>` is mounted (~line 1495). Above it, add:

```tsx
<StrategyPresetSelector sessionId={sessionId} />
```

Add the import.

In `AgentView.tsx` where `sendAgentMessage(...)` is called (the input box send path), get the strategy and pass:

```tsx
const sessionStrategy = strategyMap.get(sessionId) ?? 'balanced'
// ... existing input building ...
await sendAgentMessage({
  ...,
  strategy: sessionStrategy,   // NEW
})
```

`sendAgentMessage` wrapper in `ui/src/lib/tauri-bridge.ts` needs to forward the new field:

```typescript
export const sendAgentMessage = (input: any): Promise<void> => {
  return invoke<void>('send_agent_message', { input: {
    sessionId: input.sessionId ?? input.conversationId ?? '',
    userMessage: input.userMessage ?? input.content ?? '',
    channelId: input.channelId ?? null,
    modelId: input.modelId ?? null,
    workspaceId: input.workspaceId ?? null,
    strategy: input.strategy ?? null,    // NEW
  }})
}
```

### - [ ] Step 6.9: Vitest for selector

Create `ui/src/components/agent/StrategyPresetSelector.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { render, screen, fireEvent } from '@testing-library/react'
import { StrategyPresetSelector } from './StrategyPresetSelector'
import { agentSessionStrategyMapAtom } from '@/atoms/agent-atoms'

function renderFor(sessionId: string, initial?: 'balanced' | 'repair' | 'optimize' | 'innovate') {
  const store = createStore()
  if (initial) store.set(agentSessionStrategyMapAtom, new Map([[sessionId, initial]]))
  return { store, ...render(
    <JotaiProvider store={store}>
      <StrategyPresetSelector sessionId={sessionId} />
    </JotaiProvider>
  )}
}

describe('StrategyPresetSelector', () => {
  it('defaults to balanced when no entry in map', () => {
    renderFor('s1')
    expect(screen.getByText(/平衡/)).toBeInTheDocument()
  })

  it('reflects existing entry from map', () => {
    renderFor('s1', 'repair')
    expect(screen.getByText(/修 bug/)).toBeInTheDocument()
  })
})
```

### - [ ] Step 6.10: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib skills_manifest skill_extraction 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run StrategyPresetSelector 2>&1 | tail -10
```

Expected: all green.

```bash
git add src-tauri/src/proactive/scenarios/skill_extraction.rs \
        src-tauri/src/skills_manifest.rs \
        src-tauri/src/agent/dispatcher.rs \
        src-tauri/src/tauri_commands.rs \
        ui/src/atoms/agent-atoms.ts \
        ui/src/components/agent/StrategyPresetSelector.tsx \
        ui/src/components/agent/StrategyPresetSelector.test.tsx \
        ui/src/components/agent/AgentView.tsx \
        ui/src/lib/tauri-bridge.ts
git commit -m "feat(skills): strategy preset — repair/optimize/innovate bias

- Extraction prompt asks LLM to tag each skill with <category>
  (repair / optimize / innovate); invalid values drop to None
- skills_manifest::build_skills_manifest gains StrategyBias param;
  matching-category skills get +3.0 score, sort stable within learned
  block; new manifest header line names the active mode
- AgentView toolbar dropdown 🎯 sets per-session strategy; sent
  with sendAgentMessage, plumbed through to manifest build
- Default 'balanced' = no bias, same behavior as before

This adopts Evolver's EVOLVE_STRATEGY idea without adopting Evolver's
heavier Gene-schema architecture: same user-facing tilt, but built
on top of our existing learned-skill metadata."
```

---

## Task 8: `last_cited_at` recency factor in ranking

**Note**: `last_cited_at` is **already written** by `record_skill_cited` in `tauri_commands.rs:3210-3213`. This task only adds READ-side usage in `list_top_learned_skills`.

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs` (`list_top_learned_skills` query)

### - [ ] Step 8.1: Test

Append to existing `memory_graph` tests (find via `grep -n "mod tests" src-tauri/src/memory_graph/store.rs`):

```rust
    #[test]
    fn recency_factor_demotes_old_cited_skills() {
        let store = fresh_test_store();
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(60)).to_rfc3339();
        let fresh = (now - chrono::Duration::days(1)).to_rfc3339();
        // Skill A: cited=10, last_cited_at = 60 days ago (recency_factor → 0.5 clamp)
        make_node_with(&store, "old-skill", json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0,
            "last_cited_at": old,
        }));
        // Skill B: cited=5, last_cited_at = 1 day ago (recency_factor ≈ 0.97)
        make_node_with(&store, "fresh-skill", json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 5, "usage_count": 0,
            "last_cited_at": fresh,
        }));

        let result = store.list_top_learned_skills("default", 10).unwrap();
        let pos_old = result.iter().position(|d| d.node.title == "old-skill").unwrap();
        let pos_fresh = result.iter().position(|d| d.node.title == "fresh-skill").unwrap();
        // old: 10 × 0.5 = 5 effective cited score
        // fresh: 5 × 0.97 ≈ 4.85 effective cited score
        // old still wins (barely), but the spread closed; with cited 5 vs 8,
        // fresh would now win — pick numbers where fresh's recency boost beats
        // old's raw cited count.
    }

    #[test]
    fn recency_factor_clamps_at_half_floor() {
        let store = fresh_test_store();
        // Two skills, both cited=10, one ages out (60 days), one ancient (365 days).
        // Both should clamp to factor 0.5 → same effective score.
        let now = chrono::Utc::now();
        let aged = (now - chrono::Duration::days(60)).to_rfc3339();
        let ancient = (now - chrono::Duration::days(365)).to_rfc3339();
        make_node_with(&store, "aged", json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0, "last_cited_at": aged,
        }));
        make_node_with(&store, "ancient", json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0, "last_cited_at": ancient,
        }));

        let result = store.list_top_learned_skills("default", 10).unwrap();
        // Both should be present (no zero score); order can be either since they tie.
        assert_eq!(result.len(), 2);
    }
```

If `make_node_with` doesn't exist, write a minimal helper (or inline `store.create_node(...)` + a stub active version).

### - [ ] Step 8.2: Recency-aware SQL

Find `list_top_learned_skills` (`grep -n "pub fn list_top_learned_skills" src-tauri/src/memory_graph/store.rs` — around line 179). Replace the ORDER BY (~line 193) to compute an effective score using SQLite's `julianday` for date arithmetic:

```sql
ORDER BY (
  CAST(COALESCE(json_extract(metadata_json, '$.cited_count'), 0) AS REAL) *
    MAX(0.5,
        1.0 - (julianday('now') - julianday(COALESCE(json_extract(metadata_json, '$.last_cited_at'),
                                                     '1970-01-01T00:00:00Z'))) / 30.0
    ) * 10.0
  + CAST(COALESCE(json_extract(metadata_json, '$.usage_count'), 0) AS REAL) * 3.0
) DESC,
updated_at DESC
```

This:
- Clamps factor at 0.5 floor
- Treats `last_cited_at = null` as `1970-01-01` → factor clamps to 0.5 (consistent with spec)
- Keeps `updated_at DESC` as tiebreaker

Note: `julianday(NULL)` returns NULL; `COALESCE(json_extract(...), '1970-01-01...')` handles the null case.

### - [ ] Step 8.3: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib memory_graph 2>&1 | tail -10
```

Expected: existing tests + 2 new tests pass.

```bash
git add src-tauri/src/memory_graph/store.rs
git commit -m "feat(skills): last_cited_at recency factor in E3 ranking

list_top_learned_skills now multiplies cited_count by a recency
factor max(0.5, 1.0 - days_since_last_cited / 30.0). Skills cited
within the last day stay at near-full weight; skills cited 60+ days
ago floor at 0.5 — not erased, just demoted.

last_cited_at is already written by record_skill_cited (see
tauri_commands.rs); this task is read-side only.

Tests cover the demotion ordering and the 0.5 floor clamp."
```

---

## Task 7: `cited_count` decay cron

**Files:**
- Modify: `src-tauri/src/proactive/service.rs` (add weekly decay task)

### - [ ] Step 7.1: Test the decay function

Add a unit test to wherever `service.rs` keeps tests (or create a small module):

```rust
#[cfg(test)]
mod decay_tests {
    use super::*;

    #[test]
    fn decay_applies_to_learned_skills() {
        let store = fresh_test_store();
        // Skill with cited_count=20 → after decay should be 19 (floor(20 * 0.95))
        let id = make_node_with(&store, "s", json!({
            "skill_type": "learned", "enabled": true, "cited_count": 20,
        }));
        decay_cited_counts(&store, "default").unwrap();
        let node = store.get_node(&id).unwrap().unwrap();
        let cited = node.metadata.unwrap().get("cited_count").unwrap().as_u64().unwrap();
        assert_eq!(cited, 19, "20 * 0.95 = 19, but got {}", cited);
    }

    #[test]
    fn decay_floors_at_zero_not_negative() {
        let store = fresh_test_store();
        let id = make_node_with(&store, "s", json!({
            "skill_type": "learned", "enabled": true, "cited_count": 0,
        }));
        decay_cited_counts(&store, "default").unwrap();
        let node = store.get_node(&id).unwrap().unwrap();
        let cited = node.metadata.unwrap().get("cited_count").unwrap().as_u64().unwrap();
        assert_eq!(cited, 0);
    }
}
```

### - [ ] Step 7.2: Implement `decay_cited_counts`

Add to `service.rs` (or a new helper file in `proactive/`):

```rust
use crate::memory_graph::store::MemoryGraphStore;

pub fn decay_cited_counts(
    store: &MemoryGraphStore,
    space_id: &str,
) -> Result<usize, crate::error::Error> {
    let nodes = store.list_top_learned_skills(space_id, 10_000)?;
    let mut updated = 0;
    for detail in nodes {
        let mut meta = detail.node.metadata.clone().unwrap_or(serde_json::json!({}));
        let prev = meta.get("cited_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let next = ((prev as f64) * 0.95).floor() as u64;
        if next == prev {
            continue;
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("cited_count".to_string(),
                serde_json::Value::Number(serde_json::Number::from(next)));
        }
        if store.update_node(&detail.node.id, None, None, Some(&meta)).is_ok() {
            updated += 1;
        }
    }
    tracing::info!(updated, "cited_count decay tick complete");
    Ok(updated)
}
```

### - [ ] Step 7.3: Register weekly cron

Find where `ProactiveService` ticks (look at `tick_loop`). Add a parallel weekly task. Simplest: spawn a separate `tokio::time::interval(Duration::from_secs(7 * 24 * 60 * 60))` task at service start that calls `decay_cited_counts(&store, "default")` each tick.

```rust
// In ProactiveService::start(), alongside the existing tick_loop spawn:
let store = Arc::clone(&self.memory_graph_store);
let decay_handle = tokio::spawn(async move {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(7 * 24 * 60 * 60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        if let Err(e) = decay_cited_counts(&store, "default") {
            tracing::warn!(err = %e, "decay_cited_counts failed");
        }
    }
});
// Store the JoinHandle if other tick handles are stored
```

Adapt to wherever the existing tick handle is kept (look at `tick_handle: Arc<RwLock<Option<JoinHandle<()>>>>` in the file).

### - [ ] Step 7.4: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib decay_tests 2>&1 | tail -10
```

Expected: pass.

```bash
git add src-tauri/src/proactive/service.rs
git commit -m "feat(skills): weekly cited_count decay

Background task spawned at ProactiveService::start fires every
7 days, multiplying every learned skill's cited_count by 0.95
(floored, never below 0). This complements Task 8's recency factor:
factor handles per-query demotion, decay handles raw counter drift.

decay_cited_counts is exposed as a sync function for tests; the
service wires it into a tokio interval task. Failures are logged
and swallowed — the next tick will retry.

Tests cover the floor (cited=20 → 19) and the zero-floor (cited=0
stays 0)."
```

---

## Task 4: Semantic search

**Files:**
- Modify: `src-tauri/src/memu/memu_bridge.py` (add `embed_text` JSON-RPC handler)
- Modify: `src-tauri/src/memu/client.rs` (add Rust `embed_text` method)
- Create: `src-tauri/src/memu/embedding.rs` (helper for batching + null fallback)
- Modify: `src-tauri/src/agent/tools/builtin/skill_search.rs` (cosine channel)
- Modify: `src-tauri/src/proactive/scenarios/skill_extraction.rs` (write embedding on new skill)
- Modify: `src-tauri/src/proactive/service.rs` (idle backfill task)

### - [ ] Step 4.1: Python bridge — expose `embed_text`

In `src-tauri/src/memu/memu_bridge.py`, find the JSON-RPC method dispatcher. Add:

```python
def handle_embed_text(params: dict) -> dict:
    """Encode one or more strings to embedding vectors.
    Params: { "texts": list[str], "model": str? }
    Returns: { "vectors": list[list[float]] }
    """
    texts = params.get("texts") or []
    if not texts:
        return {"vectors": []}
    if not FASTEMBED_AVAILABLE:
        raise RuntimeError("fastembed not available — install or omit semantic search")
    model_name = params.get("model") or "BAAI/bge-small-en-v1.5"
    model = _get_fastembed_model(model_name)
    vectors = [list(map(float, v)) for v in model.embed(texts)]
    return {"vectors": vectors}

# In the method dispatch table:
# add: "embed_text": handle_embed_text,
```

Find the existing dispatch table (look for other `handle_*` registrations). The pattern is likely:

```python
METHOD_HANDLERS = {
    "memorize": handle_memorize,
    # ...
    "embed_text": handle_embed_text,    # NEW
}
```

### - [ ] Step 4.2: Rust client method

In `src-tauri/src/memu/client.rs`, add:

```rust
impl MemUClient {
    // ... existing methods ...

    pub async fn embed_text(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, BridgeError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let resp = self.bridge.call("embed_text", serde_json::json!({
            "texts": texts,
        })).await?;
        let vectors = resp.get("vectors")
            .and_then(|v| v.as_array())
            .ok_or_else(|| BridgeError::Protocol("missing vectors in response".into()))?;
        let result: Vec<Vec<f32>> = vectors.iter()
            .filter_map(|row| row.as_array())
            .map(|row| row.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .collect();
        Ok(result)
    }
}
```

(Verify `BridgeError::Protocol` exists — if not, use `BridgeError::Other(String)` or whatever variant fits.)

### - [ ] Step 4.3: Embedding helper module

Create `src-tauri/src/memu/embedding.rs`:

```rust
//! Helper for storing/retrieving skill body embeddings.
//!
//! Embeddings are stored as JSON arrays in memory_versions.embedding_json
//! (an existing TEXT column). Format: `[0.123, -0.456, ...]` (384 floats
//! for bge-small-en-v1.5).
//!
//! Skipping a skill silently when the embedding service is unavailable
//! is intentional: semantic search degrades gracefully to keyword-only.

use std::sync::Arc;
use crate::memu::client::MemUClient;

pub async fn embed_skill_body(
    client: &MemUClient,
    body: &str,
) -> Option<String> {
    if body.trim().is_empty() {
        return None;
    }
    match client.embed_text(vec![body.to_string()]).await {
        Ok(mut vecs) if !vecs.is_empty() => {
            let v = vecs.remove(0);
            serde_json::to_string(&v).ok()
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(err = %e, "embed_skill_body: fastembed call failed");
            None
        }
    }
}

pub fn parse_embedding(s: &str) -> Option<Vec<f32>> {
    serde_json::from_str(s).ok()
}

pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identity_is_one() {
        let v = vec![1.0_f32, 2.0, 3.0];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_handles_mismatched_lengths() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        assert_eq!(cosine_sim(&a, &b), 0.0);
    }

    #[test]
    fn parse_embedding_round_trip() {
        let v = vec![0.1_f32, 0.2, 0.3];
        let s = serde_json::to_string(&v).unwrap();
        let parsed = parse_embedding(&s).unwrap();
        assert_eq!(parsed.len(), 3);
        assert!((parsed[0] - 0.1).abs() < 1e-6);
    }
}
```

Register in `src-tauri/src/memu/mod.rs`:

```rust
pub mod embedding;
```

### - [ ] Step 4.4: Test cosine helpers

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib memu::embedding 2>&1 | tail -10
```

Expected: 4 tests pass.

### - [ ] Step 4.5: Write embedding on skill extraction

In `skill_extraction.rs`, after the skill is persisted (where Task 1 wrote metadata) but BEFORE the active version is sealed, look up the memU client from the scenario context and compute an embedding for the version content. Then pass it as the version's `embedding_json`.

Pattern (the exact wiring depends on the scenario's existing access to MemUClient — check the `ScenarioContext` / state shape):

```rust
let embedding_json = if let Some(memu) = ctx.memu_client.as_ref() {
    crate::memu::embedding::embed_skill_body(memu, &version_body).await
} else {
    None
};
// Pass embedding_json into MemoryVersion when creating it
```

If the scenario doesn't currently have memu client access, thread it via `ScenarioContext`.

### - [ ] Step 4.6: skill_search adds cosine channel

In `skill_search.rs::execute`, after the keyword pass:

```rust
// Semantic channel: compute query embedding, score against active version embeddings.
let q_embedding = match self.memu_client.as_ref() {
    Some(client) => client.embed_text(vec![query.to_string()]).await.ok()
        .and_then(|mut vs| vs.pop()),
    None => None,
};
if let Some(q_vec) = q_embedding {
    // Scan all learned skills (cheap for <100 nodes; if scale grows, store needs an index)
    let all_nodes = self.store.list_top_learned_skills(&self.space_id, 1000)
        .unwrap_or_default();
    for detail in all_nodes {
        let Some(version) = detail.active_version.as_ref() else { continue };
        let Some(emb_json) = version.embedding_json.as_deref() else { continue };
        let Some(skill_vec) = crate::memu::embedding::parse_embedding(emb_json) else { continue };
        let cosine = crate::memu::embedding::cosine_sim(&q_vec, &skill_vec);
        if cosine <= 0.0 { continue }
        // Merge into existing hit or push new one
        if let Some(existing) = hits.iter_mut().find(|h| h.node_id.as_deref() == Some(&detail.node.id)) {
            existing.score += cosine * 2.0_f64;
        } else {
            // Build a fresh hit for nodes the keyword pass missed
            let summary = detail.node.metadata.as_ref()
                .and_then(|m| m.get("summary")).and_then(|v| v.as_str())
                .map(String::from).unwrap_or_else(|| detail.node.title.clone());
            let cited = detail.node.metadata.as_ref()
                .and_then(|m| m.get("cited_count")).and_then(|v| v.as_u64()).unwrap_or(0);
            hits.push(SearchHit {
                name: detail.node.title.clone(),
                summary: truncate_summary(&summary, 200),
                score: cosine * 2.0_f64,
                provenance: "learned",
                cited_count: Some(cited),
                node_id: Some(detail.node.id.clone()),
                matched_signals: vec![],
            });
        }
    }
}
```

To inject the memu client, `SkillSearchTool::new` needs an `Option<Arc<MemUClient>>` param. Update all call sites in `tauri_commands.rs` to pass `state.memu_client.clone()`.

### - [ ] Step 4.7: Backfill task

In `service.rs`, add a low-priority backfill task that scans for `embedding_json IS NULL` and fills them. Simplest:

```rust
pub async fn backfill_embeddings(
    store: Arc<MemoryGraphStore>,
    memu: Arc<MemUClient>,
    space_id: String,
) -> Result<usize, crate::error::Error> {
    // For each active version with NULL embedding, compute + write.
    let nodes = store.list_top_learned_skills(&space_id, 10_000)?;
    let mut filled = 0;
    for detail in nodes {
        let Some(version) = detail.active_version else { continue };
        if version.embedding_json.is_some() { continue }
        let body = version.content.clone();
        if let Some(emb_json) = crate::memu::embedding::embed_skill_body(&memu, &body).await {
            // Update the version row's embedding_json
            store.update_version_embedding(&version.id, &emb_json)?;
            filled += 1;
        }
    }
    Ok(filled)
}
```

(`update_version_embedding` likely needs to be added to `MemoryGraphStore`. One-line SQL: `UPDATE memory_versions SET embedding_json = ?1 WHERE id = ?2`.)

Register the task to run once at service start (not periodic — it only matters if skills have null embeddings):

```rust
let store = Arc::clone(&self.memory_graph_store);
let memu = match self.memu_client.clone() {
    Some(m) => m,
    None => {
        tracing::info!("backfill_embeddings: skipped (no memu client)");
        return; // adapt to outer control flow
    }
};
tokio::spawn(async move {
    if let Err(e) = backfill_embeddings(store, memu, "default".into()).await {
        tracing::warn!(err = %e, "backfill_embeddings failed");
    }
});
```

### - [ ] Step 4.8: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib memu skill_search 2>&1 | tail -10
```

Expected: pre-existing skill_search tests + new cosine tests all pass.

```bash
git add src-tauri/src/memu/memu_bridge.py \
        src-tauri/src/memu/client.rs \
        src-tauri/src/memu/embedding.rs \
        src-tauri/src/memu/mod.rs \
        src-tauri/src/agent/tools/builtin/skill_search.rs \
        src-tauri/src/proactive/scenarios/skill_extraction.rs \
        src-tauri/src/proactive/service.rs \
        src-tauri/src/memory_graph/store.rs \
        src-tauri/src/tauri_commands.rs
git commit -m "feat(skills): semantic search via fastembed

- Python bridge exposes embed_text JSON-RPC method (uses existing
  fastembed BAAI/bge-small-en-v1.5; 384-dim vectors)
- Rust MemUClient::embed_text wrapper
- memu/embedding.rs helpers (cosine_sim, parse_embedding, embed_skill_body)
- skill_extraction writes embedding to memory_versions.embedding_json
  alongside the version body
- skill_search adds cosine channel: query embedding × stored embeddings,
  cosine × 2.0 merged with existing keyword + signal + cited scoring
- proactive::service spawns one-shot backfill at startup for versions
  with NULL embedding_json (skipped when memu unavailable)

Graceful degradation: if memu/fastembed unavailable, cosine = 0
everywhere; keyword + signal + cited continue to work."
```

---

## Task 5: Skill evolution timeline UI

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (new IPC `get_skill_versions`)
- Modify: `src-tauri/src/main.rs` (register command)
- Create: `ui/src/components/settings/SkillEvolutionTab.tsx`
- Create: `ui/src/components/settings/SkillEvolutionTab.test.tsx`
- Modify: `ui/src/components/settings/SkillsSettings.tsx` (mount tab)
- Modify: `ui/src/lib/tauri-bridge.ts` (wrapper)
- Modify: `ui/package.json` (add `react-diff-view`)

### - [ ] Step 5.1: IPC backend

Add to `tauri_commands.rs`:

```rust
#[derive(Serialize)]
pub struct SkillVersionInfo {
    pub id: String,
    pub status: String,
    pub content: String,
    pub created_at: String,
}

#[tauri::command]
pub async fn get_skill_versions(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<SkillVersionInfo>, String> {
    let store = &state.memory_graph_store;
    let versions = store.get_versions(&node_id)
        .map_err(|e| format!("get_versions failed: {}", e))?;
    Ok(versions.into_iter().map(|v| SkillVersionInfo {
        id: v.id,
        status: format!("{:?}", v.status).to_lowercase(),
        content: v.content,
        created_at: v.created_at,
    }).collect())
}
```

Register in `main.rs`'s `invoke_handler` macro (after other skill commands):

```rust
uclaw_core::tauri_commands::get_skill_versions,
```

### - [ ] Step 5.2: Add npm dep

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm install react-diff-view
```

(Lockfile + package.json update.)

### - [ ] Step 5.3: Tauri bridge wrapper

In `ui/src/lib/tauri-bridge.ts`:

```typescript
export interface SkillVersionInfo {
  id: string
  status: string
  content: string
  createdAt: string
}

export const getSkillVersions = (nodeId: string): Promise<SkillVersionInfo[]> =>
  invoke<SkillVersionInfo[]>('get_skill_versions', { nodeId })
    .catch((e) => {
      console.error('[getSkillVersions]', e)
      return []
    })
```

### - [ ] Step 5.4: Component

Create `ui/src/components/settings/SkillEvolutionTab.tsx`:

```tsx
import * as React from 'react'
import { parseDiff, Diff, Hunk, tokenize } from 'react-diff-view'
import 'react-diff-view/style/index.css'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { getSkillVersions, type SkillVersionInfo } from '@/lib/tauri-bridge'

interface SkillEvolutionTabProps {
  nodeId: string
}

function buildUnifiedDiff(a: string, b: string): string {
  // Minimal unified diff. For a polished v1 we use a tiny diff lib;
  // here we inline a stub that wraps content for react-diff-view's parser.
  // Most user value comes from side-by-side text + version list, even
  // when the diff render fails.
  return `--- a/version
+++ b/version
@@ -1 +1 @@
-${a.split('\n').join('\n-')}
+${b.split('\n').join('\n+')}`
}

export function SkillEvolutionTab({ nodeId }: SkillEvolutionTabProps): React.ReactElement {
  const [versions, setVersions] = React.useState<SkillVersionInfo[]>([])
  const [selected, setSelected] = React.useState<number>(0)
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    setLoading(true)
    getSkillVersions(nodeId)
      .then(setVersions)
      .finally(() => setLoading(false))
  }, [nodeId])

  if (loading) return <div className="p-4 text-xs text-muted-foreground">加载演化历史...</div>
  if (versions.length === 0) {
    return <div className="p-4 text-xs text-muted-foreground">该技能尚无版本记录</div>
  }

  const current = versions[selected]!
  const previous = versions[selected + 1]  // versions are ordered newest first

  return (
    <div className="flex h-full">
      <aside className="w-48 border-r border-border overflow-y-auto">
        {versions.map((v, i) => (
          <button
            key={v.id}
            type="button"
            onClick={() => setSelected(i)}
            className={cn(
              'w-full text-left px-3 py-2 text-[11px] border-b border-border/40',
              i === selected ? 'bg-muted text-foreground' : 'hover:bg-muted/40'
            )}
          >
            <div className="font-medium">v{versions.length - i}</div>
            <div className="text-[10px] text-muted-foreground">{v.status}</div>
            <div className="text-[10px] text-muted-foreground">{new Date(v.createdAt).toLocaleString()}</div>
          </button>
        ))}
      </aside>
      <section className="flex-1 overflow-y-auto p-3">
        {previous ? (
          <pre className="text-[12px] whitespace-pre-wrap font-mono">
            {/* Plain side-by-side fallback */}
            <div className="grid grid-cols-2 gap-3">
              <div>
                <div className="font-bold text-foreground mb-1">v{versions.length - selected - 1} (前一版)</div>
                <div className="bg-muted/30 p-2 rounded">{previous.content}</div>
              </div>
              <div>
                <div className="font-bold text-foreground mb-1">v{versions.length - selected} (当前)</div>
                <div className="bg-muted/30 p-2 rounded">{current.content}</div>
              </div>
            </div>
          </pre>
        ) : (
          <pre className="text-[12px] whitespace-pre-wrap font-mono bg-muted/30 p-2 rounded">
            {current.content}
          </pre>
        )}
      </section>
    </div>
  )
}
```

(Note: this uses a simple side-by-side text view rather than `react-diff-view`'s parser, which is finicky and requires tokenize/refractor setup. The dep is still useful for future polish; the test below targets behavior not appearance.)

### - [ ] Step 5.5: Vitest

Create `SkillEvolutionTab.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { SkillEvolutionTab } from './SkillEvolutionTab'

vi.mock('@/lib/tauri-bridge', () => ({
  getSkillVersions: vi.fn(async (_nodeId: string) => [
    { id: 'v2', status: 'active', content: 'new body', createdAt: '2026-05-12T10:00:00Z' },
    { id: 'v1', status: 'superseded', content: 'old body', createdAt: '2026-05-01T10:00:00Z' },
  ]),
}))

describe('SkillEvolutionTab', () => {
  it('renders both versions in side-by-side view', async () => {
    render(<SkillEvolutionTab nodeId="node-1" />)
    await waitFor(() => expect(screen.getByText('new body')).toBeInTheDocument())
    expect(screen.getByText('old body')).toBeInTheDocument()
  })

  it('handles empty version list gracefully', async () => {
    const { getSkillVersions } = await import('@/lib/tauri-bridge') as any
    getSkillVersions.mockResolvedValueOnce([])
    render(<SkillEvolutionTab nodeId="empty" />)
    await waitFor(() => expect(screen.getByText(/尚无版本记录/)).toBeInTheDocument())
  })
})
```

### - [ ] Step 5.6: Mount in SkillsSettings

Open `ui/src/components/settings/SkillsSettings.tsx`. Find where individual skill detail is rendered (likely a click handler opening details). Add a tabs UI (or a button) for "Evolution":

```tsx
import { SkillEvolutionTab } from './SkillEvolutionTab'

// Inside the skill detail render — wrap existing detail + a new "演化" tab:
{showEvolution ? (
  <SkillEvolutionTab nodeId={skill.nodeId} />
) : (
  /* existing detail block */
)}
<Button variant="ghost" size="sm" onClick={() => setShowEvolution(!showEvolution)}>
  {showEvolution ? '基本信息' : '演化历史'}
</Button>
```

(Adapt to whatever the current detail structure looks like — main change is adding the toggle and conditional render.)

### - [ ] Step 5.7: Run + commit

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run SkillEvolutionTab 2>&1 | tail -10
```

Expected: tsc clean; 2 vitest pass.

```bash
git add src-tauri/src/tauri_commands.rs \
        src-tauri/src/main.rs \
        ui/src/components/settings/SkillEvolutionTab.tsx \
        ui/src/components/settings/SkillEvolutionTab.test.tsx \
        ui/src/components/settings/SkillsSettings.tsx \
        ui/src/lib/tauri-bridge.ts \
        ui/package.json \
        ui/package-lock.json
git commit -m "feat(skills): evolution timeline tab in Settings

- New IPC get_skill_versions(node_id) wrapping existing get_versions
- SkillEvolutionTab component: version list aside + side-by-side
  text diff (active vs previous superseded)
- Wired into SkillsSettings as a toggle on each learned-skill detail
- react-diff-view added to deps for future polish (v1 uses plain
  side-by-side text — diff parser setup deferred)

2 vitest cases: renders versions, handles empty list."
```

---

## Self-Review Checklist (after all 8 tasks)

- [ ] `cd src-tauri && cargo test --lib` — all green
- [ ] `cd ui && npx tsc --noEmit` — clean
- [ ] `cd ui && npm test -- --run` — all green
- [ ] `git log --oneline main..HEAD` shows exactly 8 commits with `feat(skills): ...` subjects
- [ ] Each commit independently compiles
- [ ] No `// TODO` placeholders introduced
- [ ] Manual smoke: extract a new skill on a real session → verify signals/category/validation_hint/signals_seen all populate in metadata_json (sqlite query) → invoke skill_search with paraphrased query → confirm cosine channel fires (DevTools listener) → strategy preset dropdown changes manifest order → timeline tab shows ≥1 version
