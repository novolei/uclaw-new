# Slice A — Role Routing Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the dead `utility` and `summarizer` model-assignment roles to the runtime so the 模型分配 UI actually selects per-scenario models, via one unified resolution primitive with graceful fallback to the active model.

**Architecture:** Add a pure, sync resolver `ProviderConfigs::resolve_role_llm(role)` in `providers/types.rs` (trivially unit-testable, no async/locks), returning a named `ResolvedLlmConfig`. The async `ProviderService` getters become thin lock-and-delegate wrappers over it; existing tuple-returning getters keep their signatures (built from the struct via `into_tuple()`) so there is **no ripple** across the ~20 existing call sites. Then point the two canonical consumers — `/compact` fold (summarizer) and conversation title generation (utility) — at the new getters.

**Tech Stack:** Rust, Tokio (`RwLock`), Tauri. Verify with `cargo test --lib` and `cargo build`.

**Scope boundary (honest):** Slice A wires `utility` + `summarizer` to **one canonical consumer each** plus the generic primitive that makes the rest a mechanical migration. `utility_large` and `compiler` stay **defined-but-unconsumed** (no distinct real consumer found in the audit) and are documented as such — no new dead config. The full audit table of remaining migration candidates is recorded in Task 6 as backlog.

**Reference (recon, real call sites):**
- `get_chat_llm_config` / `get_ingestion_llm_config` duplicate the same role→active fallback: `src-tauri/src/providers/service.rs:165-236`.
- `get_active_llm_config` (the global): `src-tauri/src/providers/service.rs:131-144`.
- `/compact` fold summarization uses the global active: `src-tauri/src/tauri_commands.rs:9427-9428`.
- `try_generate_title` uses the global active: `src-tauri/src/tauri_commands.rs:13467-13468`.
- Types: `ProviderConfig` / `ModelSelection` / `ModelRoleConfig` / `ProviderConfigs` / `ApiType` at `src-tauri/src/providers/types.rs:76-91, 218-257, 261-321`; `find_provider` at `:369`.

---

### Task 1: Pure role resolver + `ResolvedLlmConfig` (in `types.rs`)

**Files:**
- Modify: `src-tauri/src/providers/types.rs` (add struct + `impl ProviderConfigs` method + tests)

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `src-tauri/src/providers/types.rs` (create the block if the file has none — it does have `ProviderConfigs` in scope via `super::*`):

```rust
#[cfg(test)]
mod role_resolve_tests {
    use super::*;

    fn fixture() -> ProviderConfigs {
        ProviderConfigs {
            providers: vec![
                ProviderConfig {
                    provider_id: "openai".into(),
                    display_name: "OpenAI".into(),
                    api_key: Some("sk-active".into()),
                    base_url: Some("https://api.openai.com".into()),
                    api: Some(ApiType::OpenAiCompletions),
                },
                ProviderConfig {
                    provider_id: "local".into(),
                    display_name: "Local".into(),
                    api_key: None,
                    base_url: Some("http://localhost:7337/v1".into()),
                    api: Some(ApiType::OpenAiCompletions),
                },
            ],
            active_model: Some(ModelSelection {
                provider_id: "openai".into(),
                model_id: "gpt-4o".into(),
            }),
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "utility".into(),
                model_ref: Some("local/minicpm5-1b".into()),
            }],
        }
    }

    #[test]
    fn role_hit_resolves_assigned_model() {
        let c = fixture().resolve_role_llm("utility").expect("some");
        assert_eq!(c.provider_id, "local");
        assert_eq!(c.model_id, "minicpm5-1b");
        assert_eq!(c.base_url, "http://localhost:7337/v1");
        assert_eq!(c.api_key, ""); // local has no key → empty string
    }

    #[test]
    fn role_unset_falls_back_to_active() {
        let c = fixture().resolve_role_llm("summarizer").expect("some");
        assert_eq!(c.provider_id, "openai");
        assert_eq!(c.model_id, "gpt-4o");
        assert_eq!(c.api_key, "sk-active");
    }

    #[test]
    fn role_points_at_missing_provider_falls_back_to_active() {
        let mut cfg = fixture();
        cfg.role_models = vec![ModelRoleConfig {
            role: "utility".into(),
            model_ref: Some("ghost/x".into()),
        }];
        let c = cfg.resolve_role_llm("utility").expect("some");
        assert_eq!(c.provider_id, "openai"); // ghost not found → active
    }

    #[test]
    fn no_role_no_active_returns_none() {
        let mut cfg = fixture();
        cfg.role_models.clear();
        cfg.active_model = None;
        assert!(cfg.resolve_role_llm("summarizer").is_none());
    }

    #[test]
    fn into_tuple_preserves_fields() {
        let c = fixture().resolve_role_llm("utility").unwrap();
        let (pid, mid, key, url, api) = c.into_tuple();
        assert_eq!(pid, "local");
        assert_eq!(mid, "minicpm5-1b");
        assert_eq!(key, "");
        assert_eq!(url, "http://localhost:7337/v1");
        assert_eq!(api, Some(ApiType::OpenAiCompletions));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test --lib providers::types::role_resolve_tests 2>&1 | tail -20`
Expected: FAIL — compile error `no method named resolve_role_llm` / `cannot find type ResolvedLlmConfig`.

- [ ] **Step 3: Add the struct + resolver**

In `src-tauri/src/providers/types.rs`, immediately after the `ModelRoleConfig` struct (after line 269, before `pub const MODEL_ROLES`), add:

```rust
/// A fully-resolved LLM connection target (role → concrete provider+model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLlmConfig {
    pub provider_id: String,
    pub model_id: String,
    pub api_key: String,
    pub base_url: String,
    pub api_type: Option<ApiType>,
}

impl ResolvedLlmConfig {
    fn from_provider(provider_id: &str, model_id: &str, p: &ProviderConfig) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            api_key: p.api_key.clone().unwrap_or_default(),
            base_url: p.base_url.clone().unwrap_or_default(),
            api_type: p.api.clone(),
        }
    }

    /// Legacy tuple shape used by existing getters:
    /// `(provider_id, model, api_key, base_url, api_override)`.
    #[must_use]
    pub fn into_tuple(self) -> (String, String, String, String, Option<ApiType>) {
        (self.provider_id, self.model_id, self.api_key, self.base_url, self.api_type)
    }
}
```

Then add this method inside the existing `impl ProviderConfigs { ... }` block (the one that contains `find_provider`):

```rust
    /// Resolve a model role to a concrete LLM target.
    /// Priority: `role_models[role]` (if its provider exists) → `active_model`.
    /// Returns `None` only when neither a usable role assignment nor an active
    /// model is available. This is the single source of truth for per-role
    /// model selection.
    #[must_use]
    pub fn resolve_role_llm(&self, role: &str) -> Option<ResolvedLlmConfig> {
        if let Some(rc) = self.role_models.iter().find(|r| r.role == role) {
            if let Some(model_ref) = &rc.model_ref {
                if let Some((pid, mid)) = model_ref.split_once('/') {
                    if let Some(p) = self.find_provider(pid) {
                        return Some(ResolvedLlmConfig::from_provider(pid, mid, p));
                    }
                }
            }
        }
        let active = self.active_model.as_ref()?;
        let p = self.find_provider(&active.provider_id)?;
        Some(ResolvedLlmConfig::from_provider(
            &active.provider_id,
            &active.model_id,
            p,
        ))
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --lib providers::types::role_resolve_tests 2>&1 | tail -20`
Expected: PASS — 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/providers/types.rs
git commit -m "feat(providers): pure resolve_role_llm + ResolvedLlmConfig (role→active fallback)"
```

---

### Task 2: Async wrapper + refactor chat/ingestion to delegate (`service.rs`)

**Files:**
- Modify: `src-tauri/src/providers/service.rs:168-236` (replace the two getters' bodies, add `get_role_llm_config`)

- [ ] **Step 1: Add `get_role_llm_config` and refactor the two existing getters**

In `src-tauri/src/providers/service.rs`, replace the whole body of `get_chat_llm_config` (lines 168-202) and `get_ingestion_llm_config` (lines 204-236) and insert a new method above them. The new region (replacing 165-236) reads:

```rust
    /// Resolve a model role → concrete LLM connection params, with active-model
    /// fallback. Single async entry point over `ProviderConfigs::resolve_role_llm`.
    pub async fn get_role_llm_config(
        &self,
        role: &str,
    ) -> Option<crate::providers::types::ResolvedLlmConfig> {
        self.configs.read().await.resolve_role_llm(role)
    }

    /// Resolve the chat-role model → active_model fallback chain.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_chat_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        self.get_role_llm_config("chat").await.map(|c| c.into_tuple())
    }

    /// Resolve the ingestion-role model → active_model fallback chain.
    /// Returns `(provider_id, model, api_key, base_url)` (no api override — the
    /// ingestion call path does not consume it).
    pub async fn get_ingestion_llm_config(&self) -> Option<(String, String, String, String)> {
        self.get_role_llm_config("ingestion").await.map(|c| {
            let (pid, mid, key, url, _api) = c.into_tuple();
            (pid, mid, key, url)
        })
    }
```

- [ ] **Step 2: Verify it compiles and existing tests still pass**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (no errors).

Run: `cd src-tauri && cargo test --lib providers:: 2>&1 | tail -15`
Expected: PASS — all `providers::` tests pass (no regression; behavior of chat/ingestion getters is byte-for-byte equivalent to the old inline logic).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/providers/service.rs
git commit -m "refactor(providers): chat/ingestion getters delegate to get_role_llm_config"
```

---

### Task 3: Add `utility` + `summarizer` getters (`service.rs`)

**Files:**
- Modify: `src-tauri/src/providers/service.rs` (insert after `get_ingestion_llm_config`, before the `// ── Provider configuration ──` comment at line ~238)

- [ ] **Step 1: Add the two role getters**

Insert immediately after the `get_ingestion_llm_config` method added in Task 2:

```rust
    /// Resolve the utility-role model (lightweight aux calls: titles, quick
    /// classification, translation) → active_model fallback.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_utility_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        self.get_role_llm_config("utility").await.map(|c| c.into_tuple())
    }

    /// Resolve the summarizer-role model (context compaction / fold / rollups)
    /// → active_model fallback.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_summarizer_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        self.get_role_llm_config("summarizer").await.map(|c| c.into_tuple())
    }
```

- [ ] **Step 2: Add a wrapper test proving role selection (in `service.rs` tests)**

The pure logic is already covered in Task 1. Add one async smoke test to the `#[cfg(test)] mod tests` block at the bottom of `service.rs` to prove the getters select the right role. Because `ProviderService::new` reads from disk, write a tiny providers.json to a temp dir first:

```rust
    #[tokio::test]
    async fn utility_getter_prefers_role_then_active() {
        let dir = std::env::temp_dir().join("uclaw_slice_a_utility_test");
        let _ = std::fs::create_dir_all(&dir);
        let json = r#"{
            "providers":[
              {"provider_id":"openai","display_name":"OpenAI","api_key":"sk-x","base_url":"https://api.openai.com","api":"openai-completions"},
              {"provider_id":"local","display_name":"Local","base_url":"http://localhost:7337/v1","api":"openai-completions"}
            ],
            "active_model":{"provider_id":"openai","model_id":"gpt-4o"},
            "selected_models":[],
            "role_models":[{"role":"utility","model_ref":"local/minicpm5-1b"}]
        }"#;
        std::fs::write(crate::providers::store::default_providers_path(&dir), json).unwrap();
        let svc = ProviderService::new(&dir).unwrap();

        let (pid, mid, _k, url, _api) = svc.get_utility_llm_config().await.unwrap();
        assert_eq!(pid, "local");
        assert_eq!(mid, "minicpm5-1b");
        assert_eq!(url, "http://localhost:7337/v1");

        // summarizer has no role → falls back to active
        let (pid2, mid2, _k2, _u2, _a2) = svc.get_summarizer_llm_config().await.unwrap();
        assert_eq!(pid2, "openai");
        assert_eq!(mid2, "gpt-4o");
    }
```

> The test must write `providers.json` to the exact path `ProviderService::new` reads (`service.rs:35` calls `super::store::default_providers_path(data_dir)`). The crate-absolute form `crate::providers::store::default_providers_path(&dir)` resolves correctly from inside the test module regardless of nesting. If `store::default_providers_path` is not `pub`/`pub(crate)`, make it `pub(crate)` in `providers/store.rs` (a one-line visibility change) so the test can call it.

- [ ] **Step 3: Run the new test**

Run: `cd src-tauri && cargo test --lib providers::service:: 2>&1 | tail -15`
Expected: PASS — including `utility_getter_prefers_role_then_active`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/providers/service.rs
git commit -m "feat(providers): get_utility_llm_config + get_summarizer_llm_config role getters"
```

---

### Task 4: Wire `/compact` fold → summarizer role (`tauri_commands.rs`)

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:9427-9428`

- [ ] **Step 1: Swap the config source**

Replace this exact two-line fragment (the `api_override` destructure makes it unique):

```rust
                let llm_cfg = if let Some((provider_id, model, api_key, base_url, api_override)) =
                    state.provider_service.get_active_llm_config().await
```

with:

```rust
                let llm_cfg = if let Some((provider_id, model, api_key, base_url, api_override)) =
                    state.provider_service.get_summarizer_llm_config().await
```

Everything below (the `effective_api` line, `llm_config_from_provider(..., 16384, 0.7, ...)`, the `legacy.clone()` fallback) stays unchanged — behavior is identical except the model is now chosen by the `summarizer` role (falling back to active when unset).

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(compact): /compact fold uses summarizer-role model (falls back to active)"
```

---

### Task 5: Wire conversation title generation → utility role (`tauri_commands.rs`)

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:13467-13468`

- [ ] **Step 1: Swap the config source**

Replace this exact two-line fragment (the `_api` placeholder + bare `provider_service.` — not `state.provider_service.` — makes it unique):

```rust
    let llm_cfg = if let Some((provider_id, model, api_key, base_url, _api)) =
        provider_service.get_active_llm_config().await
```

with:

```rust
    let llm_cfg = if let Some((provider_id, model, api_key, base_url, _api)) =
        provider_service.get_utility_llm_config().await
```

Leave the rest of `try_generate_title` (the `256` max_tokens / `0.3` temp / legacy fallback / the unrelated inline comment on the next line) untouched.

- [ ] **Step 2: Verify it compiles and the full lib test suite is green**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.

Run: `cd src-tauri && cargo test --lib providers:: 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(title): conversation title generation uses utility-role model (falls back to active)"
```

---

### Task 6: Record migration backlog + role status (docs)

**Files:**
- Modify: `docs/superpowers/specs/2026-06-03-local-minicpm-deskpet-design.md` (append a "Slice A — migration backlog" subsection under the Slice A section)

- [ ] **Step 1: Append the backlog note**

Add this subsection at the end of the `## Slice A — Role routing foundation` section of the spec:

```markdown
### Slice A — migration backlog (post-A, mechanical)

Slice A wires the two canonical consumers (summarizer → `/compact` fold; utility →
conversation title generation) and ships the generic `get_role_llm_config` primitive.
The remaining `get_active_llm_config()` call sites are now a mechanical migration —
each is a one-line swap to the appropriate role getter once that role getter exists:

| Call site (file:line) | What it does | Target role |
|---|---|---|
| `tauri_commands.rs` `try_generate_title` (other callers via :875, :13602 agent-session title) | title/emoji gen | utility |
| `tauri_commands.rs:8216` `call_consolidation_llm` | skill metadata consolidation | utility |
| `memory_graph/auto_classify.rs:40` | classify memory node | utility |
| `proactive/daily_summary.rs:143` | daily rollup | summarizer |
| `memory_bucket_seal/.../summariser/llm.rs:39` | bucket-seal tree fold (currently ingestion) | summarizer |
| `memorization/service.rs:469` | entity-page semantic merge | utility_large |
| `proactive/service.rs:2830`, `proactive/scenarios/entity_synthesizer.rs:203`, `memory_graph/wiki_synth.rs:269` | semantic synthesis | utility_large |

**`utility_large` and `compiler` remain defined-but-unconsumed in Slice A** — they have
candidate consumers above but no canonical wiring yet, and `compiler` has no distinct
consumer at all. They are intentionally left unrouted (no new dead config); wiring them
is follow-up work, not part of Slice A's deliverable.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-06-03-local-minicpm-deskpet-design.md
git commit -m "docs(spec): Slice A migration backlog + utility_large/compiler status"
```

---

## Verification (whole slice)

- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors
- [ ] `cd src-tauri && cargo test --lib providers:: 2>&1 | tail -15` → all green (5 pure resolver tests + 1 async getter test + pre-existing)
- [ ] Manual: in the running app, Settings → 智能 → 模型分配, set 摘要模型 and 轻工具模型 to a *different* provider than the active one; trigger `/compact` and a new-conversation title; confirm via logs/network that the assigned model is used, and that clearing the assignment falls back to the active model.

## Self-review notes

- **Spec coverage:** Slice A spec requirements map to tasks — unified primitive (T1/T2), `ResolvedLlmConfig` named struct without 20-site ripple (T1, kept tuples in T2/T3), utility live (T5), summarizer live (T4), fallback preserved (T1 logic + tests), `utility_large`/`compiler` honest-left-unconsumed (T6).
- **Type consistency:** `resolve_role_llm` / `ResolvedLlmConfig` / `into_tuple` / `get_role_llm_config` / `get_utility_llm_config` / `get_summarizer_llm_config` names are used identically across tasks.
- **No new deps, no schema/migration, no frontend change** — matches the spec's surface bound for Slice A.
