# Follow-up PR16 — Embed Timeout + Recall Scan-Cap as Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Promote the two PR15 hot-path constants — the embedder HTTP timeout (8s) and `recall_semantic`'s scan cap (5000) — to user-tunable config, so deployments with slow/fast embedding endpoints or large memory stores can adjust without a recompile.

**Architecture:** Add `embed_timeout_secs` to `EmbeddingEndpointConfig` (timeout is an embedding-endpoint property) and `recall_semantic_max_scan` to `MemoryOsConfig` (recall is a memory-OS property). `OpenAiCompatEmbedder::new` takes the timeout; `build_embedder` reads it from config. `BucketSealAdapter` gets a `recall_max_scan` field (defaulted in `new()`, overridable via a builder) so test fixtures are unchanged and only `app.rs` overrides from config. Both fields `#[serde(default)]` so existing config files keep deserializing.

**Tech Stack:** Rust, serde, reqwest. No new deps.

---

## Source-of-truth references (verified)

- `memubot_config.rs` — `EmbeddingEndpointConfig { base_url, model, dimensions, fastembed_model }` (`#[derive(... Deserialize)] #[serde(default)]`); `MemoryOsConfig { entity_page_enabled, auto_link_enabled, wiki_view_enabled, memory_health_enabled, ... }` (line 417); `MemubotConfig { embedding_endpoint, ..., memory_os }`. Both structs have `Default` impls.
- `memory_bucket_seal/score/embed/openai_compat.rs:27-33` — `OpenAiCompatEmbedder::new(base_url, model)` builds `reqwest::Client::builder().timeout(Duration::from_secs(8)).build()...` (hardcoded 8).
- `memory_bucket_seal/score/embed/factory.rs:14` — `pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder>` (the only non-test caller of `OpenAiCompatEmbedder::new`).
- `memory_bucket_seal/adapter.rs:112` — `const MAX_SEMANTIC_SCAN: usize = 5000;` used in `recall_semantic` (line ~133). `BucketSealAdapter::new(store, content_root, embedder, summariser)`. `fresh_adapter`/`fresh_adapter_with_summariser` test fixtures call `new`.
- `app.rs:1014` — `build_embedder(&memubot_config.embedding_endpoint)`; `app.rs:1029` — `BucketSealAdapter::new(...)`.

---

## CRITICAL facts

1. **`#[serde(default)]` on both new fields** — existing persisted configs (no `embed_timeout_secs`/`recall_semantic_max_scan` keys) must still deserialize, falling back to the defaults (8s / 5000). The structs already use `#[serde(default)]` at the struct level — confirm a field-level default (`fn` returning 8 / 5000) is wired so the value is the intended default, not `0`.
2. **Zero churn to test fixtures** — `BucketSealAdapter::new` keeps its current 4-param signature; the cap is a field defaulted to 5000 in `new()`. Only `app.rs` calls `.with_recall_max_scan(cfg_value)` to override. `fresh_adapter*` are untouched.
3. **`OpenAiCompatEmbedder::new` gains one param** (`timeout_secs: u64`) — few callers (`build_embedder` + the openai_compat tests). Update them.

---

## File Structure

| File | Mod | Change |
|---|---|---|
| `memubot_config.rs` | mod | `embed_timeout_secs: u64` on `EmbeddingEndpointConfig` + `recall_semantic_max_scan: usize` on `MemoryOsConfig`, both with field defaults; update the `Default` impls + any explicit constructors |
| `score/embed/openai_compat.rs` | mod | `new(base_url, model, timeout_secs)`; use `timeout_secs` in the Client builder |
| `score/embed/factory.rs` | mod | `build_embedder` reads `cfg.embed_timeout_secs` → passes to `new` |
| `memory_bucket_seal/adapter.rs` | mod | `recall_max_scan: usize` field (default 5000 in `new`) + `with_recall_max_scan(self, n) -> Self` builder; `recall_semantic` uses `self.recall_max_scan` not the const |
| `app.rs` | mod | `.with_recall_max_scan(memubot_config.memory_os.recall_semantic_max_scan)` on the adapter; timeout flows via `build_embedder` (no app.rs change beyond the cap) |

Est. ~80 source + ~40 tests.

---

## Tasks

### Task 1: config fields

- [ ] **Step 1: Add the fields + field-default fns** in `memubot_config.rs`

```rust
fn default_embed_timeout_secs() -> u64 { 8 }
fn default_recall_semantic_max_scan() -> usize { 5000 }
```

On `EmbeddingEndpointConfig`:
```rust
    #[serde(default = "default_embed_timeout_secs")]
    pub embed_timeout_secs: u64,
```
On `MemoryOsConfig`:
```rust
    #[serde(default = "default_recall_semantic_max_scan")]
    pub recall_semantic_max_scan: usize,
```

- [ ] **Step 2: Update the `Default` impls** for both structs to set the new fields to their defaults (8 / 5000). Verify both structs have a manual `Default` impl (the file showed `MemoryOsConfig::default()` used at line 666) — add the fields there; if `EmbeddingEndpointConfig` derives `Default`, the field needs a `#[derive(Default)]`-compatible default or a manual impl (a `u64` defaults to 0 under derive — so either add a manual `Default` or ensure the serde default + a manual Default both yield 8). **Set both explicitly in the manual Default impl(s).**

- [ ] **Step 3: Tests** — add to `memubot_config.rs` tests:

```rust
    #[test]
    fn embedding_config_default_timeout_is_8() {
        assert_eq!(EmbeddingEndpointConfig::default().embed_timeout_secs, 8);
    }
    #[test]
    fn memory_os_default_scan_cap_is_5000() {
        assert_eq!(MemoryOsConfig::default().recall_semantic_max_scan, 5000);
    }
    #[test]
    fn embedding_config_deserializes_without_timeout_field() {
        // Old config files lack the key → serde default fills 8.
        let json = r#"{"base_url":"http://x/v1","model":"m","dimensions":384,"fastembed_model":"f"}"#;
        let cfg: EmbeddingEndpointConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.embed_timeout_secs, 8);
    }
```

**Adaptation:** match the exact `EmbeddingEndpointConfig`/`MemoryOsConfig` field set in the test JSON (add any required fields). Verify field-default vs struct-default interaction yields 8/5000 in BOTH the `Default::default()` and the deserialize-missing-field paths.

- [ ] **Step 4: Run + commit**

Run: `cd src-tauri && cargo test --lib memubot_config 2>&1 | tail`
```bash
git add src-tauri/src/memubot_config.rs
git commit -m "feat(config): embed_timeout_secs + recall_semantic_max_scan config fields (PR16.1)"
```

### Task 2: thread timeout into the embedder

- [ ] **Step 1:** `OpenAiCompatEmbedder::new(base_url: &str, model: &str, timeout_secs: u64)` — replace `Duration::from_secs(8)` with `Duration::from_secs(timeout_secs)`.

- [ ] **Step 2:** `build_embedder` — `OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model, cfg.embed_timeout_secs)`.

- [ ] **Step 3:** update any `OpenAiCompatEmbedder::new(...)` call in `openai_compat.rs` tests to pass a timeout (e.g. `8`).

- [ ] **Step 4: Run + commit**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail`
```bash
git add src-tauri/src/memory_bucket_seal/score/embed/
git commit -m "feat(memory_bucket_seal): embedder HTTP timeout from config (PR16.2)"
```

### Task 3: thread scan-cap into the adapter

- [ ] **Step 1:** Add `recall_max_scan: usize` to the `BucketSealAdapter` struct. In `new(...)`, set it to `5000`. Add:

```rust
/// Override the per-turn `recall_semantic` scan cap (default 5000).
/// Used at app boot to source the value from `MemoryOsConfig`.
pub fn with_recall_max_scan(mut self, n: usize) -> Self {
    self.recall_max_scan = n;
    self
}
```

- [ ] **Step 2:** In `recall_semantic`, replace `const MAX_SEMANTIC_SCAN: usize = 5000;` + its use with `self.recall_max_scan`.

- [ ] **Step 3: Test** — add to adapter tests:

```rust
    #[tokio::test]
    async fn with_recall_max_scan_overrides_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = std::sync::Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: std::sync::Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = std::sync::Arc::new(crate::memory_bucket_seal::score::embed::InertEmbedder::new());
        let summariser: std::sync::Arc<dyn crate::memory_bucket_seal::tree_source::summariser::Summariser> = std::sync::Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser)
            .with_recall_max_scan(7);
        assert_eq!(adapter.recall_max_scan, 7);
    }
```

**Adaptation:** if `recall_max_scan` is private, the test (same module) can read it; otherwise assert via behavior. Keep the field private + the test in-module.

- [ ] **Step 4: Run + commit**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter 2>&1 | tail`
```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): recall_semantic scan cap from config via builder (PR16.3)"
```

### Task 4: wire app.rs

- [ ] **Step 1:** At `app.rs:1029` (BucketSealAdapter construction), append `.with_recall_max_scan(memubot_config.memory_os.recall_semantic_max_scan)`. The timeout already flows via `build_embedder(&memubot_config.embedding_endpoint)` (no app.rs change for it).

**Adaptation:** verify `memubot_config` is the in-scope binding at line 1029 (PR12 used `memubot_config.embedding_endpoint` there) + that `.memory_os.recall_semantic_max_scan` resolves.

- [ ] **Step 2: Full build + commit**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
```bash
git add src-tauri/src/app.rs
git commit -m "feat(app): source bucket_seal recall scan cap from config (PR16.4)"
```

### Task 5: Verification

- [ ] `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -3` (≥259 + new tests).
- [ ] `cd src-tauri && cargo test --lib memubot_config 2>&1 | tail`.
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "memubot_config|openai_compat|adapter\.rs|factory\.rs|app\.rs"` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] Confirm old-config deserialization: the `embedding_config_deserializes_without_timeout_field` test passes (backward compat).

---

## Self-Review

- ✅ Spec coverage: timeout config (Tasks 1-2), scan-cap config (Tasks 1,3-4), backward-compat serde defaults (Task 1).
- ✅ No placeholders.
- ✅ Type consistency: `embed_timeout_secs: u64`, `recall_semantic_max_scan: usize`, `OpenAiCompatEmbedder::new(_, _, u64)`, `with_recall_max_scan(usize)` consistent across tasks + call sites.
- ✅ Low blast radius: `BucketSealAdapter::new` signature unchanged (builder pattern) → test fixtures untouched. Only `OpenAiCompatEmbedder::new` gains a param (few callers).
- Decision: timeout lives on `EmbeddingEndpointConfig` (endpoint property); scan-cap on `MemoryOsConfig` (memory-OS property). Both `#[serde(default)]` for backward compat.
