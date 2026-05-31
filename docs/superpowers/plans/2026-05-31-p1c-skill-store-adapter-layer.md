# P1c — Skill-Store Adapter Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a thin, additive skill-store facade (`Skill` + `put_skill`/`get_skill`/`top_skills`/`bump_cited`) over the existing `MemoryAdapter` methods, under a `"skills"` namespace — a latest-wins ranked keyed store unblocking P3's skill_parser migration. No version history, no trait change, no live wiring, no skill_parser change. Completes P1's three slices (page/graph/skill).

**Architecture:** A new `memory_adapter/skills.rs`: free functions over `Arc<dyn MemoryAdapter>` that store a `Skill` as a `MemoryEntry` (JSON content, keyed by normalized `slug` under `"skills"`) via `store`/`get`, with `top_skills` = `list` + sort by `cited_count` desc and `bump_cited` = read-modify-write. Pure capability + unit tests; nothing calls it yet (P3 will). Mirrors P1a `pages.rs` / P1b `edges.rs`.

**Tech Stack:** Rust, `serde_json`, existing `MemoryAdapter` trait. No new deps. Spec: `docs/superpowers/specs/2026-05-31-p1c-skill-store-adapter-layer-design.md`.

---

## Source-of-truth references (verified)

- `memory_adapter/traits.rs`: `async fn store(&self, namespace, key, content, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>`; `async fn get(&self, namespace, key) -> anyhow::Result<Option<MemoryEntry>>`; `async fn list(&self, namespace: Option<&str>, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>`. `MemoryCategory::Core` valid.
- `memory_adapter/pages.rs` (P1a) + `edges.rs` (P1b): the established thin-facade pattern + an in-memory `MemoryAdapter` test stub in their `#[cfg(test)]` modules — **use as the template** for the skills test stub. `list(Some(ns), None, None)` returns all entries in `ns`. Async test attribute `#[tokio::test]`.
- `memory_adapter/mod.rs`: `pub mod pages; pub mod edges;` + `pub use ...` — mirror for `skills`.
- Consumer shape (for P3, not this slice): `proactive/skill_parser.rs` — `store_skill_as_procedure` (dedup-upsert by normalized title), `list_top_learned_skills` (top-N by cited_count), `cited_count` increment.

---

## CRITICAL facts

1. **Facade, not trait** — free `async fn`s over `&Arc<dyn MemoryAdapter>`; no trait change.
2. **No live wiring** — nothing in production calls these in P1c (P3 repoints skill_parser). Purely additive.
3. **Latest-wins, no version history** — `put_skill` keys on `slug`; same slug overwrites. Version supersedes-chains are NOT modeled (approved).
4. **Dedup by slug** — caller passes a normalized `slug`; same slug = same key = overwrite.
5. **Robust** — malformed `"skills"` content → `get_skill` None / skipped in `top_skills`; never panics. `bump_cited` on absent slug → `Ok(false)`.
6. **`top_skills` list-scan** is O(skills) — fine for the learned-skill volume; an index is a later optimization (documented, not a silent cap).
7. **Pre-commit hooks** — no `--no-verify`.

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `memory_adapter/skills.rs` | **new** — `Skill` + `put_skill`/`get_skill`/`top_skills`/`bump_cited` + in-memory test stub + tests | ~55 src + ~100 test |
| `memory_adapter/mod.rs` | `pub mod skills;` + `pub use skills::{Skill, put_skill, get_skill, top_skills, bump_cited};` | +2 |

---

## Tasks

### Task 1: `skills.rs` facade + tests (TDD)

**Files:** Create `src-tauri/src/memory_adapter/skills.rs`; modify `src-tauri/src/memory_adapter/mod.rs`.

- [ ] **Step 1: Declare the module.** In `memory_adapter/mod.rs`, add `pub mod skills;` (next to `pub mod edges;`) + `pub use skills::{Skill, put_skill, get_skill, top_skills, bump_cited};`.

- [ ] **Step 2: Write `skills.rs` with the facade + a `#[cfg(test)]` in-memory stub + failing tests.** Source (above the test module):
```rust
//! Thin skill-store facade over `MemoryAdapter` (convergence ADR P1c).
//! Free functions — NOT trait methods — over store/get/list. A latest-wins ranked
//! keyed store (no version history). No live wiring yet; P3 repoints skill_parser here.
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const SKILLS_NAMESPACE: &str = "skills";

/// A learned skill — the ranked-keyed-store subset of skill_parser's record.
/// `slug` is the normalized-title key (write-time dedup = same slug overwrites).
/// Version history is NOT modeled (latest-wins); see the convergence ADR P1c.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub slug: String,
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub cited_count: u64,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub status: String,
}

/// Upsert a skill (dedup by `slug`, latest-wins overwrite). key = slug, content = JSON(Skill).
pub async fn put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill) -> anyhow::Result<()> {
    let content = serde_json::to_string(skill)?;
    adapter.store(SKILLS_NAMESPACE, &skill.slug, &content, MemoryCategory::Core, None).await
}

/// Fetch a skill by slug. None if absent or content isn't a valid `Skill`.
pub async fn get_skill(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<Option<Skill>> {
    match adapter.get(SKILLS_NAMESPACE, slug).await? {
        Some(entry) => Ok(serde_json::from_str::<Skill>(&entry.content).ok()),
        None => Ok(None),
    }
}

/// Top-N skills by `cited_count` descending. List-scans the namespace (fine for the
/// learned-skill volume; an index is later). Unparseable entries skipped.
pub async fn top_skills(adapter: &Arc<dyn MemoryAdapter>, limit: usize) -> anyhow::Result<Vec<Skill>> {
    let entries = adapter.list(Some(SKILLS_NAMESPACE), None, None).await?;
    let mut skills: Vec<Skill> = entries
        .into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok())
        .collect();
    skills.sort_by(|a, b| b.cited_count.cmp(&a.cited_count));
    skills.truncate(limit);
    Ok(skills)
}

/// Increment a skill's `cited_count` by 1 (read-modify-write). `false` if absent (no-op).
pub async fn bump_cited(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<bool> {
    match get_skill(adapter, slug).await? {
        Some(mut skill) => {
            skill.cited_count = skill.cited_count.saturating_add(1);
            put_skill(adapter, &skill).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}
```
Tests (in `#[cfg(test)] mod tests`): copy the in-memory `MemoryAdapter` stub from `memory_adapter/edges.rs`/`pages.rs` (HashMap<(namespace,key),MemoryEntry>; `store` inserts, `get` looks up, `list(Some(ns),..)` returns namespace entries; others minimal). Then:
```rust
fn skill(slug: &str, name: &str, cited: u64) -> Skill {
    Skill { slug: slug.into(), name: name.into(), body: "b".into(), cited_count: cited, keywords: vec!["k".into()], status: "draft".into() }
}
#[tokio::test]
async fn put_then_get_round_trips_all_fields() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    let s = skill("intro", "Intro", 3);
    put_skill(&a, &s).await.unwrap();
    assert_eq!(get_skill(&a, "intro").await.unwrap(), Some(s));
}
#[tokio::test]
async fn put_same_slug_dedups_latest_wins() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    put_skill(&a, &skill("s", "S", 1)).await.unwrap();
    put_skill(&a, &skill("s", "S", 9)).await.unwrap(); // same slug → overwrite
    assert_eq!(a.list(Some("skills"), None, None).await.unwrap().len(), 1);
    assert_eq!(get_skill(&a, "s").await.unwrap().unwrap().cited_count, 9);
}
#[tokio::test]
async fn top_skills_sorts_desc_and_truncates() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    put_skill(&a, &skill("a", "A", 1)).await.unwrap();
    put_skill(&a, &skill("b", "B", 9)).await.unwrap();
    put_skill(&a, &skill("c", "C", 5)).await.unwrap();
    let top = top_skills(&a, 2).await.unwrap();
    assert_eq!(top.iter().map(|s| s.slug.clone()).collect::<Vec<_>>(), vec!["b".to_string(), "c".to_string()]);
}
#[tokio::test]
async fn bump_cited_increments_and_reports_absent() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    put_skill(&a, &skill("s", "S", 2)).await.unwrap();
    assert!(bump_cited(&a, "s").await.unwrap());
    assert_eq!(get_skill(&a, "s").await.unwrap().unwrap().cited_count, 3);
    assert!(!bump_cited(&a, "absent").await.unwrap());
    assert!(get_skill(&a, "absent").await.unwrap().is_none()); // no entry created
}
#[tokio::test]
async fn get_and_top_skip_malformed() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    a.store("skills", "bad", "not json", MemoryCategory::Core, None).await.unwrap();
    put_skill(&a, &skill("ok", "OK", 1)).await.unwrap();
    assert_eq!(get_skill(&a, "bad").await.unwrap(), None);
    assert_eq!(top_skills(&a, 10).await.unwrap().len(), 1); // only "ok"
}
#[test]
fn skill_serde_defaults() {
    let s: Skill = serde_json::from_str(r#"{"slug":"s","name":"N","body":"b"}"#).unwrap();
    assert_eq!(s.cited_count, 0);
    assert!(s.keywords.is_empty());
    assert_eq!(s.status, "");
}
```
(Match the crate's async-test attribute `#[tokio::test]`; confirm the stub `list` honors the namespace filter.)

- [ ] **Step 3: Run → red→green.** `cd src-tauri && cargo test --lib memory_adapter::skills 2>&1 | tail`.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/memory_adapter/skills.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory): skill-store adapter facade (Skill + put/get/top/bump_cited, latest-wins) — convergence P1c"
```

### Task 2: Verification

- [ ] `cd src-tauri && cargo test --lib memory_adapter::skills 2>&1 | tail` (6 tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib memory_adapter 2>&1 | tail -3` (broader memory_adapter green — no regression to adapter/router/bucket_seal/pages/edges tests).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter/skills" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Additive-only confirm:** `grep -rn "skills::put_skill\|skills::top_skills\|skills::bump_cited" src-tauri/src | grep -v "memory_adapter/skills.rs\|memory_adapter/mod.rs"` → empty (nothing wires it yet; P3's job).

---

## Self-Review

- ✅ **Spec coverage:** `Skill` + `put_skill`/`get_skill`/`top_skills`/`bump_cited` facade (Task 1) + verification incl. additive-only confirm (Task 2). Version history / bigram-dedup / keyword-index / decay / index / P3 wiring explicitly out of scope.
- ✅ **Placeholder scan:** full facade + full test code; the in-memory stub is a copy-from-edges/pages instruction with a concrete behavior contract.
- ✅ **Type consistency:** `Skill { slug, name, body, cited_count: u64, keywords, status }`; `put_skill(&Arc<dyn MemoryAdapter>, &Skill) -> Result<()>`; `get_skill(...) -> Result<Option<Skill>>`; `top_skills(..., usize) -> Result<Vec<Skill>>`; `bump_cited(..., &str) -> Result<bool>`; matches `store`/`get`/`list` signatures.
- ✅ **Risk-scaled:** lowest — pure additive facade, no trait change, no live wiring, no skill_parser change; one module + tests. `top_skills` list-scan documented as a later optimization (not silent). Completes P1's three slices.
- Decisions: facade over trait; `"skills"` namespace; latest-wins (no version history); slug-key dedup; cited_count read-modify-write; keywords in content; robust-skip malformed; no live wiring (P3).
