# 阶段 4 PR6 — `memory_bucket_seal` canonicalize + chunker port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port openhuman's chunker + canonicalize/{mod, chat, document} into `memory_bucket_seal` so the ingestion pipeline becomes end-to-end usable: input payload (chat batch / document) → `CanonicalisedSource` → `Vec<Chunk>` ready for `stage_chunks` + `BucketSealStore::upsert_staged_chunks` (already shipped in PR5). Drop email canonicalizers — no uClaw caller today.

**Architecture:** Faithful port of `openhuman/src/openhuman/memory/tree/{chunker.rs, canonicalize/{mod, chat, document}.rs}` into nested `memory_bucket_seal/canonicalize/` + flat `memory_bucket_seal/chunker.rs`. Chunker keeps the full 3-way SourceKind dispatch (chat / email / document) so adding the email canonicalizer later is plug-and-play. Lift PR5's private `redact` helper from `paths.rs` to a shared `memory_bucket_seal/util.rs` (DRY — addresses PR5 review note).

**Tech Stack:** Rust, `chrono`, `serde`, `tracing`. All deps already in workspace. No new ones.

---

## Source-of-truth references

Openhuman files this PR ports from (read fully before porting each):
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/chunker.rs` (port full — 814 LoC + 17 tests). Drop nothing — the email dispatch arm is unused but kept for the verbatim-port discipline.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/mod.rs` (port full — 59 LoC). `CanonicalisedSource` + `CanonicaliseRequest<P>` + `normalize_source_ref`.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/chat.rs` (port full — 223 LoC + 6 tests).
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/document.rs` (port full — 138 LoC + 5 tests).

**DO NOT port** `canonicalize/email.rs` (257 LoC) or `canonicalize/email_clean.rs` (382 LoC). The chunker's email-dispatch arm references `split_email_messages` which IS ported (it splits markdown by `---\nFrom:` separators — that's all the chunker needs from the email subsystem).

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/util.rs` (new) | Shared `redact(s) -> String` (8-char SHA-256 prefix). Lifts the inline helper from PR5's `paths.rs`. | ~20 |
| `src-tauri/src/memory_bucket_seal/paths.rs` (modify, -7 +1 lines) | Remove inline `fn redact` + its `use sha2/hex` imports; add `use crate::memory_bucket_seal::util::redact;`. | -6 net |
| `src-tauri/src/memory_bucket_seal/canonicalize/mod.rs` (new) | `CanonicalisedSource`, `CanonicaliseRequest<P>`, `normalize_source_ref`. `pub mod chat; pub mod document;`. | ~75 |
| `src-tauri/src/memory_bucket_seal/canonicalize/chat.rs` (new) | `ChatMessage`, `ChatBatch`, `canonicalise(...)` + 6 inline tests. | ~250 |
| `src-tauri/src/memory_bucket_seal/canonicalize/document.rs` (new) | `DocumentInput`, `canonicalise(...)` + 5 inline tests. | ~160 |
| `src-tauri/src/memory_bucket_seal/chunker.rs` (new) | `DEFAULT_CHUNK_MAX_TOKENS`, `ChunkerOptions`, `ChunkerInput`, `chunk_markdown` + 4 private split helpers + 17 inline tests. | ~860 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +6 lines) | `pub mod canonicalize; pub mod chunker; pub mod util;` + re-exports for `chunk_markdown`, `ChunkerInput`, `ChunkerOptions`, `DEFAULT_CHUNK_MAX_TOKENS`, `CanonicalisedSource`, `CanonicaliseRequest`. | +6 |

**LoC budget**: ~1380 source + ~280 tests ≈ **1660 LoC total**. Matches the Option B scope envelope.

---

## Decisions Already Locked

- **Module path**: `memory_bucket_seal/canonicalize/{mod, chat, document}.rs` (nested directory because the canonicalize/ pattern has multiple files). `chunker.rs` stays flat at the module root.
- **British/American naming preserved**: directory = `canonicalize` (American), function = `canonicalise` (British). Match openhuman bit-for-bit.
- **Email scope**: NO `email.rs`, NO `email_clean.rs` files. BUT the chunker's `split_email_messages` private helper IS ported (it's a 90-LoC `---\nFrom:` splitter; pure string ops; the chunker's `match input.source_kind { SourceKind::Email => ... }` arm references it).
- **Redact helper**: lifted to `memory_bucket_seal/util.rs` once. PR5's `paths.rs` adapts to import from there. `chunker.rs` uses the same import.
- **No new deps**: `chrono::serde::ts_milliseconds` is used by `ChatMessage` + `EmailMessage` (but Email is deferred, so only chat needs it). `serde` + `tracing` already in workspace.
- **`chunk_markdown` uses `chrono::Utc::now()`** for each chunk's `created_at` — keep verbatim, don't try to inject a clock.
- **Test names**: keep openhuman's test names exactly. If a name like `pack_segments_handles_oversize_unit` exists in openhuman, port it under the same name in the corresponding file.
- **No AppState wiring**, **no IPC**, **no Tauri commands**. PR9 (BucketSealAdapter) does that. PR6 is data layer + composition.
- **Error type**: openhuman uses `Result<Option<CanonicalisedSource>, String>` for canonicalisers. Keep this verbatim — chat/document return `Ok(None)` for "nothing to ingest" and `Err(String)` for malformed input.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

For each task:

1. **Re-read the openhuman source file** being ported. The snippets in this plan are skeletons — copy from openhuman wherever the plan and openhuman disagree.
2. **`log::*` → `tracing::*`** with structured fields (`tracing::debug!(source_id = %input.source_id, ...)`) — uClaw convention as established in PR5.
3. **Import path rewrites**:
   - `use crate::openhuman::memory::tree::types::{...}` → `use crate::memory_bucket_seal::types::{...}`
   - `use crate::openhuman::memory::tree::util::redact::redact` → `use crate::memory_bucket_seal::util::redact` (after Task 1 lands)
   - `use super::{...}` inside canonicalize/* — adjust to match new mod path
   - `use crate::openhuman::memory::tree::canonicalize::email_clean` → **REMOVE** (we're not porting email)
4. **Verify openhuman's chunker doesn't reference email-only types** beyond the `split_email_messages` helper. If `chunk_markdown` imports anything from `canonicalize::email` directly, that's a signal you need to look more carefully — but openhuman's chunker should only depend on `types::*` (Chunk, Metadata, SourceKind, approx_token_count).
5. **`ChatMessage` + `EmailMessage`-shaped types**: chat.rs has its own `ChatMessage` for the canonicalize-input side. Don't confuse with future chunker types.
6. **`partial_message` field on Chunk**: PR5 set this with `#[serde(default)]`. The chunker sets `partial_message = true` when an oversize unit is hard-split. Verify the field is writeable.
7. **`chrono::Utc::now()` non-determinism in tests**: tests assert chunk fields by value but `created_at` is fresh on every call. Openhuman's tests don't compare `created_at` exactly; they just check it's set. Port that pattern — don't compare timestamps for equality.
8. **Pre-commit hooks**: same hooks as PR5. After each task commit, expect a hooks pass. If any fails, fix the underlying issue.

---

### Task 1: Lift `redact` to `util.rs` + adapt `paths.rs`

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/util.rs`
- Modify: `src-tauri/src/memory_bucket_seal/paths.rs` (remove inline `redact`)
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (add `pub mod util;`)

- [ ] **Step 1: Write `util.rs`**

```rust
//! Shared utilities for the bucket-seal module.
//!
//! `redact` returns a short, non-revealing token for log lines that would
//! otherwise leak source ids (email addresses, channel names). Used by
//! both `paths.rs` and `chunker.rs` when logging fallback paths.

use sha2::{Digest, Sha256};

/// Redact a string to a short non-revealing token for log lines.
///
/// Returns the first 8 hex characters of `SHA-256(input)`. Sufficient for
/// deduplicated logging of malformed source ids without leaking PII.
pub fn redact(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    let hex_str = hex::encode(digest);
    hex_str[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_is_deterministic_for_same_input() {
        assert_eq!(redact("alice@example.com"), redact("alice@example.com"));
    }

    #[test]
    fn redact_differs_for_different_input() {
        assert_ne!(redact("alice@example.com"), redact("bob@example.com"));
    }

    #[test]
    fn redact_is_8_chars() {
        assert_eq!(redact("anything").len(), 8);
    }
}
```

- [ ] **Step 2: Update `mod.rs`**

Add `pub mod util;` near the other `pub mod` declarations (alongside `pub mod atomic; pub mod paths; pub mod store; pub mod types;`).

- [ ] **Step 3: Remove inline `redact` from `paths.rs`**

In `src-tauri/src/memory_bucket_seal/paths.rs`:
- Find the inline `fn redact(s: &str) -> String { ... }` block (~line 22, 7 lines including the doc comment).
- Delete that function.
- Find the `use sha2::{Digest, Sha256};` import at the top of `paths.rs`. If this import is now unused (it was only used by the now-deleted `redact`), delete it. **Verify by grep**: `grep -n "Sha256\|Digest" src/memory_bucket_seal/paths.rs` should return zero hits after the deletion.
- Add `use crate::memory_bucket_seal::util::redact;` at the top of `paths.rs`.

- [ ] **Step 4: Build + run all PR5 + new util tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: previous 31 PR5 tests still pass + 3 new `util::tests::*` tests = 34 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/util.rs src-tauri/src/memory_bucket_seal/mod.rs src-tauri/src/memory_bucket_seal/paths.rs
git commit -m "refactor(memory_bucket_seal): lift redact to util.rs (PR6.1 of 阶段 4)"
```

---

### Task 2: `canonicalize/mod.rs` — foundational types

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/canonicalize/mod.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (add `pub mod canonicalize;`)

- [ ] **Step 1: Port `canonicalize/mod.rs` from openhuman**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/mod.rs` in full. Port verbatim with these adjustments:
- Top of file:
  ```rust
  //! Canonicalisers — normalise source-specific payloads into canonical
  //! Markdown with provenance metadata.
  //!
  //! Faithful port of `openhuman::memory::tree::canonicalize`. PR6 ships the
  //! `mod`, `chat`, and `document` sub-modules. `email` + `email_clean` are
  //! deferred until a real email producer lands.
  
  pub mod chat;
  pub mod document;
  
  use serde::{Deserialize, Serialize};
  
  use crate::memory_bucket_seal::types::{Metadata, SourceRef};
  ```
- Body of file: copy `CanonicalisedSource`, `CanonicaliseRequest<P>`, `normalize_source_ref` verbatim. Two structs + one function.
- DO NOT include `pub mod email;` or `pub mod email_clean;`.

- [ ] **Step 2: Update `memory_bucket_seal/mod.rs`**

Add to the `pub mod` block (after `pub mod chunker;` — note: `chunker` will be added in Task 4, but add the `canonicalize` declaration now):

```rust
pub mod canonicalize;
```

And re-export the foundational types:

```rust
pub use canonicalize::{CanonicalisedSource, CanonicaliseRequest, normalize_source_ref};
```

(Place these `pub use` lines next to the existing `pub use types::{...}` / `pub use store::BucketSealStore;`.)

- [ ] **Step 3: Compile + test scaffolding**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`

Expected: TWO errors — `cannot find unresolved import 'crate::memory_bucket_seal::canonicalize::chat'` and `cannot find unresolved import 'crate::memory_bucket_seal::canonicalize::document'`. These are EXPECTED because Task 3+4 haven't landed yet. To unblock the build, temporarily wrap the `pub mod chat;` and `pub mod document;` in `canonicalize/mod.rs` with a `#[cfg(not(any()))]` cfg-off — or simpler, just NOT add `pub mod chat;` / `pub mod document;` to `canonicalize/mod.rs` at this step. Add them in Task 3 (`pub mod chat;` lands with the chat.rs file) and Task 4 (`pub mod document;`).

**Revised step 1**: do not add `pub mod chat;` / `pub mod document;` in the `mod.rs` body during Task 2. Just leave the file at: top doc + `use` imports + `CanonicalisedSource` + `CanonicaliseRequest` + `normalize_source_ref`. The sub-mod declarations come with their files in Tasks 3 + 4.

After this revision, build is expected to succeed.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: 34 tests pass (no new tests in `canonicalize/mod.rs` — it's pure types).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/canonicalize/ src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): canonicalize mod + types (PR6.2 of 阶段 4)"
```

---

### Task 3: `canonicalize/chat.rs` — chat batches

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/canonicalize/chat.rs`
- Modify: `src-tauri/src/memory_bucket_seal/canonicalize/mod.rs` (add `pub mod chat;`)

- [ ] **Step 1: Port `chat.rs` from openhuman verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/chat.rs` in full (all 223 LoC). Port with these adjustments:
- Replace `use super::{normalize_source_ref, CanonicalisedSource};` with `use super::{normalize_source_ref, CanonicalisedSource};` (same path — `super` is now `canonicalize`).
- Replace `use crate::openhuman::memory::tree::types::{Metadata, SourceKind};` with `use crate::memory_bucket_seal::types::{Metadata, SourceKind};`.
- Convert `log::*` to `tracing::*` (with structured fields where openhuman uses `{format}` interpolation).
- Port the 6 `#[test]` blocks at the bottom of the file verbatim. If any test uses a path that no longer exists in our module, adapt the path.

The function shape:

```rust
pub fn canonicalise(
    source_id: &str,
    owner: &str,
    tags: &[String],
    batch: ChatBatch,
) -> Result<Option<CanonicalisedSource>, String>
```

Returns `Ok(None)` for empty batches; `Ok(Some(...))` otherwise; `Err(String)` for malformed input (if openhuman has any such validation — likely none).

- [ ] **Step 2: Wire `pub mod chat;` into `canonicalize/mod.rs`**

Edit `src-tauri/src/memory_bucket_seal/canonicalize/mod.rs` — add `pub mod chat;` between the doc comment and the `use` imports (matching openhuman's layout).

- [ ] **Step 3: Build + run tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::canonicalize::chat 2>&1 | tail -15`
Expected: 6 passed (matching openhuman's test count).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/canonicalize/chat.rs src-tauri/src/memory_bucket_seal/canonicalize/mod.rs
git commit -m "feat(memory_bucket_seal): chat canonicaliser (PR6.3 of 阶段 4)"
```

---

### Task 4: `canonicalize/document.rs` — single documents

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/canonicalize/document.rs`
- Modify: `src-tauri/src/memory_bucket_seal/canonicalize/mod.rs` (add `pub mod document;`)

- [ ] **Step 1: Port `document.rs` from openhuman verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/canonicalize/document.rs` in full (138 LoC). Port with the same adjustments as Task 3:
- `use crate::openhuman::memory::tree::types::{...}` → `use crate::memory_bucket_seal::types::{...}`
- `use super::{normalize_source_ref, CanonicalisedSource};` stays.
- Port the 5 `#[test]` blocks verbatim.

Function shape (single document, NOT a thread):

```rust
pub fn canonicalise(
    source_id: &str,
    owner: &str,
    tags: &[String],
    doc: DocumentInput,
) -> Result<Option<CanonicalisedSource>, String>
```

Returns `Ok(None)` when both title and body are empty.

- [ ] **Step 2: Wire `pub mod document;` into `canonicalize/mod.rs`**

Edit `canonicalize/mod.rs` — add `pub mod document;` next to the `pub mod chat;` line.

- [ ] **Step 3: Build + run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::canonicalize 2>&1 | tail -15`
Expected: 11 passed (6 chat + 5 document).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/canonicalize/document.rs src-tauri/src/memory_bucket_seal/canonicalize/mod.rs
git commit -m "feat(memory_bucket_seal): document canonicaliser (PR6.4 of 阶段 4)"
```

---

### Task 5: `chunker.rs` — markdown → chunks

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/chunker.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (add `pub mod chunker;` + re-exports)

- [ ] **Step 1: Port `chunker.rs` from openhuman verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/chunker.rs` in full (all 814 LoC including tests). Port with these adjustments:
- `use crate::openhuman::memory::tree::types::{approx_token_count, Chunk, Metadata, SourceKind};` → `use crate::memory_bucket_seal::types::{approx_token_count, Chunk, Metadata, SourceKind};`
- `use crate::openhuman::memory::tree::util::redact::redact;` → `use crate::memory_bucket_seal::util::redact;`
- Convert `log::*` to `tracing::*` with structured fields.
- Keep all 4 private helper fns: `split_chat_messages`, `split_email_messages`, `pack_segments`, `hard_split_by_chars`. The email splitter is unused-but-present (the email canonicalizer that would produce its input format doesn't exist in uClaw yet, but the helper IS in the chunker's match arm). DO NOT delete it.
- Port the 17 `#[test]` blocks verbatim.

Note on chunk_id determinism (per openhuman's types.rs comment): `chunk_id(source_kind, source_id, seq_in_source, content)` is deterministic on those 4 inputs. The chunker generates `seq_in_source` from 0..N. Two `chunk_markdown` calls with identical inputs produce identical chunk ids.

Note on `created_at`: each chunk gets `chrono::Utc::now()` so two `chunk_markdown` calls produce different `created_at` even though the chunk ids are identical. Tests must assert chunk ids by exact value but never compare `created_at` for equality.

- [ ] **Step 2: Update `memory_bucket_seal/mod.rs`**

Add module declaration + re-exports. After existing `pub mod` lines, add:

```rust
pub mod chunker;
```

After existing `pub use` lines, add:

```rust
pub use chunker::{chunk_markdown, ChunkerInput, ChunkerOptions, DEFAULT_CHUNK_MAX_TOKENS};
```

- [ ] **Step 3: Build + run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::chunker 2>&1 | tail -20`
Expected: 17 passed.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: total memory_bucket_seal tests = 31 (PR5) + 3 (util) + 6 (chat) + 5 (document) + 17 (chunker) = **62 passed**.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/chunker.rs src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): markdown chunker (PR6.5 of 阶段 4)"
```

---

### Task 6: End-to-end integration test

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (append integration test to existing `#[cfg(test)]` block at bottom)

- [ ] **Step 1: Add integration test**

Append to the `#[cfg(test)] mod tests` block at the bottom of `mod.rs`:

```rust
    #[test]
    fn end_to_end_chat_batch_to_chunks_to_disk_to_sql() {
        use crate::memory_bucket_seal::canonicalize::chat::{canonicalise, ChatBatch, ChatMessage};
        use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use chrono::{TimeZone, Utc};
        use tempfile::TempDir;

        // 1. Build a chat batch
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let batch = ChatBatch {
            platform: "slack".to_string(),
            channel_label: "eng".to_string(),
            messages: vec![
                ChatMessage {
                    author: "alice".to_string(),
                    timestamp: ts,
                    text: "first message".to_string(),
                    source_ref: None,
                },
                ChatMessage {
                    author: "bob".to_string(),
                    timestamp: ts,
                    text: "second message".to_string(),
                    source_ref: None,
                },
            ],
        };

        // 2. Canonicalise
        let canonical = canonicalise("slack:#eng", "alice", &[], batch)
            .unwrap()
            .expect("non-empty batch should produce CanonicalisedSource");
        assert!(canonical.markdown.contains("first message"));
        assert!(canonical.markdown.contains("second message"));

        // 3. Chunk
        let chunker_input = ChunkerInput {
            source_kind: canonical.metadata.source_kind,
            source_id: canonical.metadata.source_id.clone(),
            markdown: canonical.markdown.clone(),
            metadata: canonical.metadata.clone(),
        };
        let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
        assert!(!chunks.is_empty(), "should produce at least one chunk");
        // Two messages should fit in one chunk under DEFAULT_CHUNK_MAX_TOKENS = 3_000.
        assert_eq!(chunks.len(), 1);

        // 4. Stage to disk
        let dir = TempDir::new().unwrap();
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(!staged[0].content_path.is_empty(), "chat chunks must have content_path");

        // 5. Upsert to SQLite
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        let n = store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.count_chunks().unwrap(), 1);

        // 6. Round-trip via get_chunk
        let got = store.get_chunk(&chunks[0].id).unwrap()
            .expect("chunk should be retrievable by deterministic id");
        assert_eq!(got.metadata.source_id, "slack:#eng");
    }
```

This test exercises the **full** PR5+PR6 pipeline: payload → canonicalise → chunk → stage → upsert → fetch. If any link breaks, this test breaks.

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tests::end_to_end 2>&1 | tail -10`
Expected: 1 passed.

- [ ] **Step 3: Run all memory_bucket_seal tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: **63 passed** (62 + 1 e2e).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "test(memory_bucket_seal): end-to-end chat → chunks → disk → SQL (PR6.6 of 阶段 4)"
```

---

### Task 7: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: 63 passed.

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive (≥ baseline + 32 from PR6).

- [ ] **Step 3: Clippy on PR6 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep "memory_bucket_seal" | head -20`
Expected: zero hits.

- [ ] **Step 4: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nrE "TODO|FIXME|XXX" src/memory_bucket_seal/canonicalize/ src/memory_bucket_seal/chunker.rs src/memory_bucket_seal/util.rs`
Expected: zero hits (any TODO inherited from openhuman → either keep with explicit attribution comment, or remove).

- [ ] **Step 5: No new workspace deps**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr6-bucket-seal-canonicalize-chunker && git diff main -- src-tauri/Cargo.toml`
Expected: empty.

- [ ] **Step 6: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR6 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| `redact` determinism + length | 3 | `util::tests` |
| `chat::canonicalise` empty/non-empty/metadata | 6 | `canonicalize::chat::tests` |
| `document::canonicalise` empty/non-empty/metadata | 5 | `canonicalize::document::tests` |
| `chunker::chunk_markdown` 3-way dispatch + greedy-pack + oversize fallback + edge cases | 17 | `chunker::tests` |
| End-to-end chat → SQL | 1 | `mod::tests` |
| **Total new tests** | **32** | — |
| **PR5 tests preserved** | 31 | (unchanged) |
| **Module total** | **63** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B from brainstorming → chunker + canonicalize/{mod, chat, document}. Email canonicalizers DROPPED. End-to-end pipeline works for the 2 active source kinds.
- ✅ **Scope check**: NO email.rs, NO email_clean.rs, NO AppState wiring, NO IPC, NO BucketSealAdapter (PR9), NO scoring (PR7), NO summaries (PR8), NO jobs (PR10+). Pipeline ends at `upsert_staged_chunks`.
- ✅ **Faithful port**: chunker uses identical algorithms (chunk_id determinism, `## ` chat splitter, paragraph-pack for document). Canonicalize/{chat, document} ported verbatim with import rewrites only.
- ✅ **No placeholders**: every step shows actual code paths and openhuman source line ranges. Adaptation responsibilities enumerated.
- ✅ **DRY hygiene**: `redact` lifted to `util.rs` (addresses PR5 review note). PR5's `paths.rs` adapted in Task 1.
- ✅ **Bisectability**: 6 task commits (util-lift / canonicalize-mod / chat / document / chunker / e2e). Each builds standalone.
- ✅ **No new deps**: verified.
- ✅ **`tracing::*` discipline**: no `log::*` slips through.
- ✅ **Test names match openhuman**: faithful port preserves test names so a reviewer with openhuman tab open can cross-reference.
