# 阶段 4 PR5 — `memory_bucket_seal` content_store port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the openhuman bucket-seal content_store layer (atomic on-disk `.md` writes + SQLite-indexed chunks) into a new `src-tauri/src/memory_bucket_seal/` module. No AppState wiring, no IPC, no adapter — just the standalone storage foundation.

**Architecture:** Faithful port of `openhuman/src/openhuman/memory/tree/{types.rs, store.rs subset, content_store/{atomic.rs, paths.rs, mod.rs}}` into a single flat module `memory_bucket_seal`. Bodies on disk under `<content_root>/{chat,email,document}/<slug>/<chunk_id>.md` with YAML front-matter (compose deferred to PR6). SQLite at `<bucket_seal_dir>/chunks.db` holds the index: id, source metadata, content preview, `content_path`, `content_sha256`. Schema is applied lazily via `BucketSealStore::ensure_schema()` on first use — separate file, no V-number coordination with uClaw's `migrations.rs`.

**Tech Stack:** Rust, `rusqlite` (already in workspace), `sha2`, `hex`, `uuid`, `chrono`, `tempfile` (tests only), `tracing` for logging. No new deps.

---

## Source-of-truth references

Openhuman files this PR ports from (read once before writing each file):
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/types.rs` (full — slice out `DataSource` enum + summary-related items)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/content_store/atomic.rs` (port `write_if_new`, `sha256_hex`, `uuid_v4_hex` — DROP `stage_summary`, `StagedSummary`, `read_body_sha256`)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/content_store/paths.rs` (port `chunk_rel_path`, `chunk_abs_path`, `slugify_source_id`, `sanitize_filename` — DROP `summary_rel_path`, `summary_abs_path`, `SummaryTreeKind`)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/content_store/mod.rs` (port `StagedChunk` + `stage_chunks` — DROP summary references, `obsidian` mod, `tags` mod)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/store.rs` (port: `SCHEMA` constant slimmed to `mem_tree_chunks` only, `upsert_staged_chunks_tx`, `get_chunk`, `list_chunks` query subset, `count_chunks`, `with_connection` pattern — DROP everything tied to summaries, score, entity index, hotness, jobs, ingested_sources, lifecycle status, embeddings, raw_refs)

DO NOT port openhuman's `Config` pattern — uClaw threads paths explicitly. Each store function takes a `&Path` content root + `&BucketSealStore` instead of `&Config`.

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/mod.rs` (new) | Module declarations, `StagedChunk`, `stage_chunks` (chunks-only — no summary), re-exports. | ~130 |
| `src-tauri/src/memory_bucket_seal/types.rs` (new) | `SourceKind`, `SourceRef`, `Metadata`, `Chunk`, `chunk_id()`, `approx_token_count()`, `time_range_serde` mod. **Drops**: `DataSource` enum (8-variant; defer to later port). | ~320 |
| `src-tauri/src/memory_bucket_seal/atomic.rs` (new) | `write_if_new`, `sha256_hex`, `uuid_v4_hex`. **Drops**: `stage_summary`, `StagedSummary`, `read_body_sha256`. | ~180 |
| `src-tauri/src/memory_bucket_seal/paths.rs` (new) | `chunk_rel_path`, `chunk_abs_path`, `slugify_source_id`, `sanitize_filename`, `truncate_at_char` helper. **Drops**: `summary_rel_path`, `summary_abs_path`, `SummaryTreeKind`, `redact` (use a tiny inline `fn redact` for log lines instead). | ~320 |
| `src-tauri/src/memory_bucket_seal/store.rs` (new) | `BucketSealStore` struct holding `Arc<Mutex<Connection>>`, `new(db_path)`, `ensure_schema()`, `upsert_staged_chunks()`, `get_chunk()`, `list_chunks_by_source()`, `count_chunks()`. **Drops**: lifecycle status, embeddings, raw refs, source ingest gate, all summary/score/job tables. | ~430 |
| `src-tauri/src/lib.rs` (modify, +1 line) | `pub mod memory_bucket_seal;` declaration. | +1 |

**LoC budget**: ~1380 source + ~250 tests ≈ **1630 LoC total**. Falls within the PR5-B scope ceiling.

---

## Decisions Already Locked

- **Module name**: `memory_bucket_seal` (singular path component, matching uClaw's flat-modular convention). NOT `memory_bucket_seal/tree/` — flatten openhuman's `tree/` indirection.
- **Type subset**: port `SourceKind { Chat, Email, Document }`, `SourceRef`, `Metadata`, `Chunk`, `chunk_id`, `approx_token_count`. **Drop** `DataSource` (8-variant provider enum — not needed until PR6's canonicalize wants to dispatch by provider).
- **`SCHEMA` subset**: only `mem_tree_chunks` table + its 4 indexes, with `content_path` + `content_sha256` columns included from the start (greenfield, no ALTER needed). Drop all other tables.
- **DB location**: `BucketSealStore::new` takes an explicit `db_path: PathBuf`. Tests use `tempfile::tempdir() + dir.join("chunks.db")`. Production wiring (path resolution from `app_data_dir`) lands in PR9 with the adapter.
- **Content root**: similarly explicit `&Path` argument to `stage_chunks`. No global config singleton.
- **No AppState integration**: this PR does NOT add `bucket_seal_store: Arc<BucketSealStore>` to `AppState`. The store stands alone. PR9 (`BucketSealAdapter`) does the wiring.
- **Logging**: use `tracing::debug!` / `tracing::warn!` / `tracing::error!` (NOT `log::*` — uClaw uses tracing).
- **Error handling**: `anyhow::Result<T>` internally (matches memory_adapter pattern; PR9 adapter layer translates to trait's `anyhow::Result`).
- **Forward-slash relative paths**: keep openhuman's "store relative paths with forward-slash in SQL, resolve to OS-native via `chunk_abs_path`" convention verbatim. Critical for cross-platform vault portability.
- **Email layout**: keep openhuman's `gmail:participants` parsing + flat fallback. uClaw doesn't ship email today but the path generator should match openhuman bit-for-bit so PR6+ canonicalize can be a verbatim port.
- **Test crate**: `tempfile` is already in `[dev-dependencies]` — verify before each test write.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

This plan was written off a single read of openhuman. Before each task's implementation:

1. **Re-read the openhuman source file you're porting** in the same step. Don't paraphrase from memory.
2. **Verify dependency availability**: `rusqlite = 0.31`, `sha2 = 0.10`, `hex = 0.4`, `uuid` (feature `v4` confirmed by PR3), `chrono`, `tempfile` (dev-dep). If anything is missing, **stop and report — do NOT add new workspace deps mid-PR**.
3. **Verify the openhuman `read_body_sha256` import in `atomic.rs`**: it's referenced by `stage_summary` which we're DROPPING, so the import must also be dropped — but the function definition still exists in openhuman's `atomic.rs`. **Drop both the definition and import** in your port; we don't need it without summaries.
4. **Verify the openhuman `redact` import in `paths.rs`**: this is `crate::openhuman::memory::tree::util::redact::redact`. Replace with a tiny inline `fn redact(s: &str) -> String` that returns a short SHA-256 prefix of `s` (8 hex chars). The log line that uses `redact` only needs the value to be non-revealing; it doesn't need to match openhuman's exact algorithm.
5. **Verify `MemoryNodeKind` / openhuman type collisions**: there are none — `memory_bucket_seal::types::Chunk` does not clash with anything in `crate::memory_graph::models`. But the implementer should grep `Chunk` workspace-wide once before adding the type to make sure.
6. **Verify the `truncate_at_char` helper**: openhuman has this in `paths.rs` as a private fn. Read the body and port verbatim — it's a UTF-8-safe truncation that doesn't cut mid-codepoint.
7. **Verify `chrono::Utc::now()` returns `DateTime<Utc>`**: PR3 used this. Should be the same here.
8. **Verify `serde_json` is available**: used by `tags_json` serialization in `upsert_staged_chunks`. Check workspace.
9. **Implementer adapts if reality differs.** Don't fabricate workarounds — if a function signature in openhuman has shifted, copy what's there.

---

### Task 1: Module scaffold + types

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/mod.rs` (skeleton only at this step)
- Create: `src-tauri/src/memory_bucket_seal/types.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Skeleton `mod.rs`**

Write:

```rust
//! Bucket-seal memory backend (openhuman port).
//!
//! Standalone storage layer for chunks: atomic file writes under
//! `<content_root>/{chat,email,document}/<slug>/<chunk_id>.md` indexed by a
//! SQLite catalog at `<bucket_seal_dir>/chunks.db`. Build target for the
//! BucketSealAdapter in PR9; no AppState wiring or IPC at this stage.
//!
//! Faithful port of `openhuman::memory::tree` (atomic + paths + chunks-only
//! SQLite). Summaries, scoring, entity index, jobs, and the topic/global
//! trees follow in later PRs.

pub mod atomic;
pub mod paths;
pub mod store;
pub mod types;

pub use store::BucketSealStore;
pub use types::{approx_token_count, chunk_id, Chunk, Metadata, SourceKind, SourceRef};
```

(Leave `StagedChunk` + `stage_chunks` for Task 4. The skeleton just establishes mod structure so the rest compiles independently.)

- [ ] **Step 2: Register module in `lib.rs`**

Edit `src-tauri/src/lib.rs:42` — add the new module after the existing `pub mod memory_graph;` line:

```rust
pub mod memory;
pub mod memory_adapter;
pub mod memory_bucket_seal;
pub mod memory_graph;
```

- [ ] **Step 3: Port `types.rs`**

Open `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/types.rs` and copy:
- The `SourceKind` enum + its `as_str` + `parse` impls (lines ~20-50)
- The `SourceRef` struct + `new` ctor (lines ~146-160)
- The `Metadata` struct + `point_in_time` ctor (lines ~162-214)
- The `Chunk` struct (lines ~216-242)
- The `chunk_id` free function (lines ~256-277)
- The `approx_token_count` free function (lines ~283-287)
- The `time_range_serde` private mod (lines ~289-324)

**DO NOT** port the `DataSource` enum (lines ~65-145) — out of PR5 scope.

Top of file:

```rust
//! Core types for the bucket-seal ingestion layer (openhuman port — Phase 1
//! equivalent of issue #707). Defines [`Chunk`] + provenance [`Metadata`] +
//! deterministic chunk-id hashing.
//!
//! Faithful port of `openhuman/src/openhuman/memory/tree/types.rs` with the
//! `DataSource` enum (provider-level discriminator) dropped — that lands
//! with the canonicalize port in PR6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ... rest of file: paste openhuman content verbatim, skipping DataSource block ...
```

**At the bottom of the file**, also port openhuman's `#[cfg(test)] mod tests` block (lines ~327-end) but only the tests that exercise `SourceKind`, `Metadata`, `Chunk`, `chunk_id`, `approx_token_count`, and `time_range_serde`. Drop any tests that reference `DataSource`. Expected test count: ~6.

- [ ] **Step 4: Build + run types tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::types::tests 2>&1 | tail -10`
Expected: ~6 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/memory_bucket_seal/
git commit -m "feat(memory_bucket_seal): module skeleton + types (PR5.1 of 阶段 4)"
```

---

### Task 2: `atomic.rs` — atomic file writes

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/atomic.rs`

- [ ] **Step 1: Port `atomic.rs` chunks-only**

Open `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/content_store/atomic.rs`. Port the following:
- File-level doc comment (adapt the wording for uClaw context)
- `write_if_new(abs_path: &Path, bytes: &[u8]) -> anyhow::Result<bool>` (the full function, including `#[cfg(unix)]` parent-dir fsync block)
- `sha256_hex(bytes: &[u8]) -> String`
- `uuid_v4_hex() -> String` (the lock-free temp-file name generator)

**DO NOT** port:
- `StagedSummary` struct
- `stage_summary` function
- `read_body_sha256` function (used only by `stage_summary`)
- The `use super::compose::*` + `use super::paths::*` imports — replace with the imports only needed for `write_if_new`

**Convert all `log::debug!` / `log::warn!` / `log::error!` to `tracing::debug!` / `tracing::warn!` / `tracing::error!`** — uClaw uses tracing.

Top of file:

```rust
//! Atomic content-file writes via tempfile + fsync + rename.
//!
//! Each chunk body is written to `<parent>/.tmp_<hex>.md`, then renamed to
//! its final path. The rename is atomic on any POSIX filesystem and behaves
//! correctly on NTFS.
//!
//! **Immutability contract**: once a file exists at `abs_path`, it is never
//! overwritten by `write_if_new`. Callers must detect "already exists" and
//! handle accordingly. (Stale-body re-write logic lives at the
//! `stage_chunks` layer in `mod.rs` for the chunks-only PR5 surface.)

use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

// (write_if_new, sha256_hex, uuid_v4_hex follow — port verbatim with
// log → tracing rename)
```

- [ ] **Step 2: Port tests**

Append at the bottom — port these 3 tests from openhuman:
- `write_creates_file_and_returns_true`
- `write_is_idempotent_returns_false_on_second_call`
- `sha256_hex_is_stable`

**DO NOT** port any `stage_summary_*` tests.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::atomic::tests 2>&1 | tail -10`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/atomic.rs
git commit -m "feat(memory_bucket_seal): atomic file writes (PR5.2 of 阶段 4)"
```

---

### Task 3: `paths.rs` — path generation + slugify

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/paths.rs`

- [ ] **Step 1: Port `paths.rs` chunks-only**

Open `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/content_store/paths.rs`. Port:
- File-level doc comment (adapt)
- `chunk_rel_path(source_kind: &str, source_id: &str, chunk_id: &str) -> String` (full)
- `chunk_abs_path(content_root: &Path, source_kind: &str, source_id: &str, chunk_id: &str) -> PathBuf`
- `slugify_source_id(source_id: &str) -> String` (full)
- `sanitize_filename(input: &str) -> String` (the private helper used by `chunk_rel_path`)
- `truncate_at_char(s: &str, max: usize) -> &str` (the UTF-8-safe truncator used by `slugify_source_id`)

**DO NOT** port:
- `SummaryTreeKind` enum
- `summary_rel_path` / `summary_abs_path`
- Any `summary_*` helpers

**Replace** `use crate::openhuman::memory::tree::util::redact::redact;` with a local inline helper at the top of `paths.rs`:

```rust
/// Redact a string to a short, non-revealing token for log lines.
///
/// Returns the first 8 hex characters of `SHA-256(input)`. Sufficient for
/// deduplicated logging of malformed `source_id` values without leaking the
/// underlying email or thread identifier.
fn redact(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    let hex_str = hex::encode(digest);
    hex_str[..8].to_string()
}
```

(The implementer must verify openhuman's `redact` does something similar; if it differs significantly, document the divergence — but the log line that uses `redact` doesn't need exact parity. The point is to avoid PII in logs.)

**`log::*` → `tracing::*`** as in Task 2.

Top of file:

```rust
//! Content-file path generation.
//!
//! Each chunk body is stored as a `.md` file under `<content_root>/`. The path
//! structure depends on the source kind:
//!
//! ```text
//! Email:    <content_root>/email/<participants_slug>/<chunk_id>.md
//! Chat:     <content_root>/chat/<source_slug>/<chunk_id>.md
//! Document: <content_root>/document/<source_slug>/<chunk_id>.md
//! ```
//!
//! Faithful port of `openhuman::memory::tree::content_store::paths` minus
//! summary-tree path helpers (deferred to PR8).

use std::path::{Path, PathBuf};

// ... (redact + functions follow) ...
```

- [ ] **Step 2: Port tests**

Append at the bottom — port these tests from openhuman:
- `slugify_slack_channel`
- `slugify_gmail_thread`
- `slugify_collapses_consecutive_separators`
- `slugify_uppercase_lowercased`
- `slugify_empty_falls_back_to_unknown`
- `chunk_rel_path_chat`
- `chunk_rel_path_email_well_formed`
- `chunk_rel_path_email_malformed_fallback`
- `chunk_rel_path_document`
- `chunk_abs_path_resolves_under_root`

If any of these test names don't exist in openhuman, write equivalents — the cases ARE in openhuman's test block, even if names differ. **Read openhuman's `paths.rs` test block first.**

**DO NOT** port any `summary_*_path` tests.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::paths::tests 2>&1 | tail -15`
Expected: ~10 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/paths.rs
git commit -m "feat(memory_bucket_seal): path generation + slugify (PR5.3 of 阶段 4)"
```

---

### Task 4: `mod.rs` — `StagedChunk` + `stage_chunks`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs`

- [ ] **Step 1: Add `StagedChunk` struct and `stage_chunks` function**

Append to `mod.rs` after the `pub use` re-exports:

```rust
use std::path::Path;

/// A chunk that has been written to disk and is ready for SQLite upsert.
///
/// Callers build a `Vec<StagedChunk>` from `stage_chunks`, then pass it to
/// `BucketSealStore::upsert_staged_chunks` in a single transaction.
#[derive(Debug, Clone)]
pub struct StagedChunk {
    /// The original chunk (metadata + content).
    pub chunk: Chunk,
    /// Relative content path (forward-slash, e.g. `"chat/slack-eng/0.md"`).
    pub content_path: String,
    /// SHA-256 hex digest over the body bytes only.
    pub content_sha256: String,
}

/// Write all chunks in `chunks` to disk and return `StagedChunk` records
/// ready for SQLite upsert.
///
/// Each chunk file is written atomically via a sibling temp-file + rename.
/// Already-existing files are skipped (immutable-body contract). Parent
/// directories are created on demand.
///
/// **Email chunks skip the disk write** — their body lives in the raw archive
/// (deferred to PR8+); we still emit a `StagedChunk` row with an empty
/// `content_path` so the SQLite upsert proceeds.
///
/// **Note**: at PR5 the chunk body is written as plain bytes (`chunk.content`
/// as-is), no YAML front-matter envelope. PR6 (`canonicalize + chunker`)
/// brings in the `compose_chunk_file` step that wraps the body with front-matter.
/// Until then, the SHA-256 is computed over the raw chunk content bytes.
pub fn stage_chunks(
    content_root: &Path,
    chunks: &[Chunk],
) -> anyhow::Result<Vec<StagedChunk>> {
    let mut staged = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        if chunk.metadata.source_kind == SourceKind::Email {
            // Body lives in raw/<source>/<ts>_<id>.md — no chunk file at PR5.
            staged.push(StagedChunk {
                chunk: chunk.clone(),
                content_path: String::new(),
                content_sha256: String::new(),
            });
            continue;
        }

        let source_kind = chunk.metadata.source_kind.as_str();
        let source_id = &chunk.metadata.source_id;

        let rel_path = paths::chunk_rel_path(source_kind, source_id, &chunk.id);
        let abs_path = paths::chunk_abs_path(content_root, source_kind, source_id, &chunk.id);

        let body_bytes = chunk.content.as_bytes();
        let sha256 = atomic::sha256_hex(body_bytes);

        match atomic::write_if_new(&abs_path, body_bytes) {
            Ok(true) => {
                tracing::debug!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    "memory_bucket_seal: wrote chunk"
                );
            }
            Ok(false) => {
                tracing::debug!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    "memory_bucket_seal: chunk already on disk"
                );
            }
            Err(e) => {
                tracing::error!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    error = %e,
                    "memory_bucket_seal: failed to write chunk"
                );
                return Err(e);
            }
        }

        staged.push(StagedChunk {
            chunk: chunk.clone(),
            content_path: rel_path,
            content_sha256: sha256,
        });
    }

    Ok(staged)
}
```

Add to the `pub use` block:

```rust
pub use store::BucketSealStore;
pub use types::{approx_token_count, chunk_id, Chunk, Metadata, SourceKind, SourceRef};
```

(`StagedChunk` and `stage_chunks` are at the module root, no re-export needed for module-private callers; external callers will `use crate::memory_bucket_seal::{StagedChunk, stage_chunks};`)

- [ ] **Step 2: Add tests**

Append at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn sample_chunk(seq: u32) -> Chunk {
        let ts = chrono::Utc.timestamp_millis_opt(1_700_000_000_000 + seq as i64).unwrap();
        Chunk {
            id: format!("chunk_{seq:02}"),
            content: format!("## ts — alice\nMessage {seq}"),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec![],
                source_ref: None,
            },
            token_count: 5,
            seq_in_source: seq,
            created_at: ts,
            partial_message: false,
        }
    }

    #[test]
    fn stage_chunks_writes_files_and_returns_staged() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();

        assert_eq!(staged.len(), 2);
        for s in &staged {
            let abs = paths::chunk_abs_path(
                dir.path(),
                s.chunk.metadata.source_kind.as_str(),
                &s.chunk.metadata.source_id,
                &s.chunk.id,
            );
            assert!(abs.exists(), "file must exist: {}", abs.display());
            assert!(!s.content_path.is_empty());
            assert_eq!(s.content_sha256.len(), 64);
            assert!(!s.content_path.starts_with('/'));
            assert!(s.content_path.contains('/'));
        }
    }

    #[test]
    fn stage_chunks_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![sample_chunk(0)];
        let first = stage_chunks(dir.path(), &chunks).unwrap();
        let second = stage_chunks(dir.path(), &chunks).unwrap();
        assert_eq!(first[0].content_sha256, second[0].content_sha256);
        assert_eq!(first[0].content_path, second[0].content_path);
    }

    #[test]
    fn stage_chunks_email_skips_disk_write() {
        let dir = TempDir::new().unwrap();
        let mut chunk = sample_chunk(0);
        chunk.metadata.source_kind = SourceKind::Email;
        chunk.metadata.source_id = "gmail:alice@x.com|bob@y.com".into();
        let staged = stage_chunks(dir.path(), &[chunk]).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(staged[0].content_path.is_empty());
        assert!(staged[0].content_sha256.is_empty());
        // No file was written for email
        let email_dir = dir.path().join("email");
        assert!(!email_dir.exists(), "no email/ tree should be created at PR5");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal:: 2>&1 | tail -15`
Expected: all module tests pass (types + atomic + paths + new mod = ~22 passed).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): StagedChunk + stage_chunks (PR5.4 of 阶段 4)"
```

---

### Task 5: `store.rs` — SQLite catalog

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/store.rs`

- [ ] **Step 1: Write the `BucketSealStore` struct + `ensure_schema`**

Open openhuman's `store.rs` and extract:
- The `SCHEMA` constant (lines ~46-245) — **slim to only the `mem_tree_chunks` block + its 4 indexes**. Drop everything from "Phase 2" onward (score, entity index, summaries, buffers, hotness, jobs, ingested_sources).
- The `with_connection` pattern (open file, set busy_timeout, set foreign_keys=on, apply SCHEMA on each open).

Write the file:

```rust
//! SQLite catalog for bucket-seal chunks.
//!
//! Faithful port of `openhuman::memory::tree::store` slimmed to chunks-only:
//! drops summary trees, score, entity index, jobs, raw refs, embeddings,
//! lifecycle status, ingest-source gate. PR5 builds the foundation; PR6-12
//! restore the deferred surface in their own slices.
//!
//! Schema is applied lazily on `ensure_schema()`. The DB lives at a path
//! given by the caller (typically `<app_data_dir>/bucket_seal/chunks.db`).
//! The store wraps `Arc<Mutex<Connection>>` so multiple async tasks can
//! call into it.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::memory_bucket_seal::types::{Chunk, Metadata, SourceKind, SourceRef};
use crate::memory_bucket_seal::StagedChunk;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(15);

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS mem_tree_chunks (
    id                     TEXT PRIMARY KEY,
    source_kind            TEXT NOT NULL,
    source_id              TEXT NOT NULL,
    source_ref             TEXT,
    owner                  TEXT NOT NULL,
    timestamp_ms           INTEGER NOT NULL,
    time_range_start_ms    INTEGER NOT NULL,
    time_range_end_ms      INTEGER NOT NULL,
    tags_json              TEXT NOT NULL DEFAULT '[]',
    content                TEXT NOT NULL,
    token_count            INTEGER NOT NULL,
    seq_in_source          INTEGER NOT NULL,
    created_at_ms          INTEGER NOT NULL,
    content_path           TEXT NOT NULL DEFAULT '',
    content_sha256         TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_source
    ON mem_tree_chunks(source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_timestamp
    ON mem_tree_chunks(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_owner
    ON mem_tree_chunks(owner);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_source_seq
    ON mem_tree_chunks(source_kind, source_id, seq_in_source);
";

const DEFAULT_LIST_LIMIT: usize = 100;
const MAX_LIST_LIMIT: usize = 10_000;

#[derive(Clone)]
pub struct BucketSealStore {
    conn: Arc<Mutex<Connection>>,
}

impl BucketSealStore {
    /// Open (or create) the chunks.db at `db_path`. Sets busy_timeout and
    /// returns the store. Schema is NOT applied — call `ensure_schema()`
    /// before the first write.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create_dir_all {:?}", parent))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("open {:?}", db_path))?;
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)
            .context("set busy_timeout")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create the schema if it doesn't exist. Safe to call repeatedly.
    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA).context("apply SCHEMA")?;
        Ok(())
    }

    // Upsert / get / list / count follow in Steps 2-3.
}
```

- [ ] **Step 2: Write `upsert_staged_chunks` + `get_chunk`**

Add these methods inside the `impl BucketSealStore` block:

```rust
    /// Upsert a batch of staged chunks atomically.
    ///
    /// Returns the number of rows inserted or replaced. Re-running with the
    /// same `chunk.id` is idempotent (UPSERT on PK). The SQL `content`
    /// column stores a ≤500-char plain-text preview; the full body lives at
    /// `content_path` on disk.
    pub fn upsert_staged_chunks(&self, staged: &[StagedChunk]) -> Result<usize> {
        if staged.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().context("begin transaction")?;
        let inserted = {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO mem_tree_chunks (
                        id, source_kind, source_id, source_ref, owner,
                        timestamp_ms, time_range_start_ms, time_range_end_ms,
                        tags_json, content, token_count, seq_in_source, created_at_ms,
                        content_path, content_sha256
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                    ON CONFLICT(id) DO UPDATE SET
                        source_kind = excluded.source_kind,
                        source_id = excluded.source_id,
                        source_ref = excluded.source_ref,
                        owner = excluded.owner,
                        timestamp_ms = excluded.timestamp_ms,
                        time_range_start_ms = excluded.time_range_start_ms,
                        time_range_end_ms = excluded.time_range_end_ms,
                        tags_json = excluded.tags_json,
                        content = excluded.content,
                        token_count = excluded.token_count,
                        seq_in_source = excluded.seq_in_source,
                        created_at_ms = excluded.created_at_ms,
                        content_path = excluded.content_path,
                        content_sha256 = excluded.content_sha256",
                )
                .context("prepare upsert")?;

            for s in staged {
                let chunk = &s.chunk;
                let preview: String = chunk.content.chars().take(500).collect();
                stmt.execute(params![
                    chunk.id,
                    chunk.metadata.source_kind.as_str(),
                    chunk.metadata.source_id,
                    chunk.metadata.source_ref.as_ref().map(|r| r.value.as_str()),
                    chunk.metadata.owner,
                    chunk.metadata.timestamp.timestamp_millis(),
                    chunk.metadata.time_range.0.timestamp_millis(),
                    chunk.metadata.time_range.1.timestamp_millis(),
                    serde_json::to_string(&chunk.metadata.tags)?,
                    preview,
                    chunk.token_count,
                    chunk.seq_in_source,
                    chunk.created_at.timestamp_millis(),
                    s.content_path,
                    s.content_sha256,
                ])
                .context("execute upsert")?;
            }
            staged.len()
        };
        tx.commit().context("commit transaction")?;
        Ok(inserted)
    }

    /// Fetch one chunk by its id. Returns `None` if no row matches.
    ///
    /// Note: the returned `Chunk.content` is the SQL-stored preview (≤500
    /// chars). To read the full body, resolve `content_path` against the
    /// content root (PR6+ via the BucketSealAdapter).
    pub fn get_chunk(&self, id: &str) -> Result<Option<Chunk>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT source_kind, source_id, source_ref, owner,
                        timestamp_ms, time_range_start_ms, time_range_end_ms,
                        tags_json, content, token_count, seq_in_source, created_at_ms
                   FROM mem_tree_chunks WHERE id = ?1",
                params![id],
                |row| {
                    let source_kind_str: String = row.get(0)?;
                    let source_id: String = row.get(1)?;
                    let source_ref_str: Option<String> = row.get(2)?;
                    let owner: String = row.get(3)?;
                    let timestamp_ms: i64 = row.get(4)?;
                    let tr_start_ms: i64 = row.get(5)?;
                    let tr_end_ms: i64 = row.get(6)?;
                    let tags_json: String = row.get(7)?;
                    let content: String = row.get(8)?;
                    let token_count: u32 = row.get(9)?;
                    let seq_in_source: u32 = row.get(10)?;
                    let created_at_ms: i64 = row.get(11)?;
                    Ok((
                        source_kind_str, source_id, source_ref_str, owner,
                        timestamp_ms, tr_start_ms, tr_end_ms, tags_json,
                        content, token_count, seq_in_source, created_at_ms,
                    ))
                },
            )
            .optional()
            .context("query chunk")?;

        let Some(tup) = row else { return Ok(None) };
        let source_kind = SourceKind::parse(&tup.0).map_err(|e| anyhow::anyhow!(e))?;
        let timestamp = Utc.timestamp_millis_opt(tup.4).single()
            .ok_or_else(|| anyhow::anyhow!("invalid timestamp_ms"))?;
        let tr_start = Utc.timestamp_millis_opt(tup.5).single()
            .ok_or_else(|| anyhow::anyhow!("invalid time_range_start_ms"))?;
        let tr_end = Utc.timestamp_millis_opt(tup.6).single()
            .ok_or_else(|| anyhow::anyhow!("invalid time_range_end_ms"))?;
        let created_at = Utc.timestamp_millis_opt(tup.11).single()
            .ok_or_else(|| anyhow::anyhow!("invalid created_at_ms"))?;
        let tags: Vec<String> = serde_json::from_str(&tup.7).context("parse tags_json")?;
        let source_ref = tup.2.map(SourceRef::new);

        Ok(Some(Chunk {
            id: id.to_string(),
            content: tup.8,
            metadata: Metadata {
                source_kind,
                source_id: tup.1,
                owner: tup.3,
                timestamp,
                time_range: (tr_start, tr_end),
                tags,
                source_ref,
            },
            token_count: tup.9,
            seq_in_source: tup.10,
            created_at,
            partial_message: false,
        }))
    }
```

- [ ] **Step 3: Write `list_chunks_by_source` + `count_chunks`**

Add inside `impl BucketSealStore`:

```rust
    /// List chunks scoped to a specific source, ordered by `seq_in_source`
    /// ascending. `limit` clamps to `MAX_LIST_LIMIT` defensively.
    pub fn list_chunks_by_source(
        &self,
        source_kind: SourceKind,
        source_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Chunk>> {
        let effective_limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_kind, source_id, source_ref, owner,
                    timestamp_ms, time_range_start_ms, time_range_end_ms,
                    tags_json, content, token_count, seq_in_source, created_at_ms
               FROM mem_tree_chunks
              WHERE source_kind = ?1 AND source_id = ?2
              ORDER BY seq_in_source ASC
              LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                params![source_kind.as_str(), source_id, effective_limit as i64],
                row_to_chunk,
            )?
            .collect::<rusqlite::Result<Vec<Chunk>>>()
            .context("collect chunks")?;
        Ok(rows)
    }

    /// Total chunk count across all sources.
    pub fn count_chunks(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mem_tree_chunks",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }
}

// row_to_chunk lives at module level so list_chunks_by_source can pass it as
// query_map's row mapper without runtime closure indirection.
fn row_to_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chunk> {
    let id: String = row.get(0)?;
    let source_kind_str: String = row.get(1)?;
    let source_id: String = row.get(2)?;
    let source_ref_str: Option<String> = row.get(3)?;
    let owner: String = row.get(4)?;
    let timestamp_ms: i64 = row.get(5)?;
    let tr_start_ms: i64 = row.get(6)?;
    let tr_end_ms: i64 = row.get(7)?;
    let tags_json: String = row.get(8)?;
    let content: String = row.get(9)?;
    let token_count: u32 = row.get(10)?;
    let seq_in_source: u32 = row.get(11)?;
    let created_at_ms: i64 = row.get(12)?;

    let source_kind = SourceKind::parse(&source_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let timestamp = Utc.timestamp_millis_opt(timestamp_ms).single()
        .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
            5, rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid timestamp_ms")),
        ))?;
    let tr_start = Utc.timestamp_millis_opt(tr_start_ms).single()
        .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
            6, rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid time_range_start_ms")),
        ))?;
    let tr_end = Utc.timestamp_millis_opt(tr_end_ms).single()
        .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
            7, rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid time_range_end_ms")),
        ))?;
    let created_at = Utc.timestamp_millis_opt(created_at_ms).single()
        .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
            12, rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid created_at_ms")),
        ))?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            8, rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    let source_ref = source_ref_str.map(SourceRef::new);

    Ok(Chunk {
        id,
        content,
        metadata: Metadata {
            source_kind,
            source_id,
            owner,
            timestamp,
            time_range: (tr_start, tr_end),
            tags,
            source_ref,
        },
        token_count,
        seq_in_source,
        created_at,
        partial_message: false,
    })
}
```

- [ ] **Step 4: Add tests**

Append at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::{stage_chunks, StagedChunk};
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn sample_chunk(seq: u32) -> Chunk {
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000 + seq as i64).unwrap();
        Chunk {
            id: format!("chunk_{seq:02}"),
            content: format!("Message {seq} body"),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec!["foo".into()],
                source_ref: None,
            },
            token_count: 4,
            seq_in_source: seq,
            created_at: ts,
            partial_message: false,
        }
    }

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn ensure_schema_is_idempotent() {
        let (store, _dir) = fresh_store();
        store.ensure_schema().unwrap();
        store.ensure_schema().unwrap();
        assert_eq!(store.count_chunks().unwrap(), 0);
    }

    #[test]
    fn upsert_then_get_round_trip() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        let n = store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(n, 2);

        let got = store.get_chunk("chunk_00").unwrap().unwrap();
        assert_eq!(got.id, "chunk_00");
        assert_eq!(got.metadata.source_id, "slack:#eng");
        assert_eq!(got.token_count, 4);
        assert_eq!(got.metadata.tags, vec!["foo".to_string()]);
    }

    #[test]
    fn upsert_is_idempotent_on_chunk_id() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(store.count_chunks().unwrap(), 1);
    }

    #[test]
    fn list_chunks_by_source_orders_by_seq() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(2), sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let listed = store
            .list_chunks_by_source(SourceKind::Chat, "slack:#eng", None)
            .unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].seq_in_source, 0);
        assert_eq!(listed[1].seq_in_source, 1);
        assert_eq!(listed[2].seq_in_source, 2);
    }

    #[test]
    fn list_chunks_respects_limit() {
        let (store, dir) = fresh_store();
        let chunks: Vec<_> = (0..5).map(sample_chunk).collect();
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        let listed = store
            .list_chunks_by_source(SourceKind::Chat, "slack:#eng", Some(2))
            .unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn get_chunk_returns_none_when_missing() {
        let (store, _dir) = fresh_store();
        assert!(store.get_chunk("missing").unwrap().is_none());
    }

    #[test]
    fn count_chunks_reflects_writes() {
        let (store, dir) = fresh_store();
        assert_eq!(store.count_chunks().unwrap(), 0);
        let chunks = vec![sample_chunk(0), sample_chunk(1), sample_chunk(2)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(store.count_chunks().unwrap(), 3);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: total memory_bucket_seal tests pass — types (~6) + atomic (3) + paths (~10) + mod (3) + store (7) ≈ **29 passed**.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/store.rs
git commit -m "feat(memory_bucket_seal): BucketSealStore SQLite catalog (PR5.5 of 阶段 4)"
```

---

### Task 6: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: 29 passed, 0 failed.

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive (existing tests unchanged, +29 from memory_bucket_seal).

- [ ] **Step 3: Clippy on PR5 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "(warning|error)" | grep "memory_bucket_seal" | head -20`
Expected: zero hits. (Pre-existing repo-wide warnings outside `memory_bucket_seal` are not in scope.)

- [ ] **Step 4: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nE "TODO|FIXME|XXX" src/memory_bucket_seal/*.rs`
Expected: zero hits. If any landed, justify or remove before merge.

- [ ] **Step 5: Confirm `lib.rs` declaration is present**

Run: `cd src-tauri && grep "pub mod memory_bucket_seal" src/lib.rs`
Expected: one line.

- [ ] **Step 6: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR5 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| `SourceKind` parse + display + Serde | ~3 | `types::tests` |
| `chunk_id` determinism | 1 | `types::tests` |
| `approx_token_count` | 1 | `types::tests` |
| `time_range_serde` round-trip | 1 | `types::tests` |
| `write_if_new` happy/idempotent + sha256 | 3 | `atomic::tests` |
| `slugify_source_id` cases | 5 | `paths::tests` |
| `chunk_rel_path` chat/email/document | 4 | `paths::tests` |
| `chunk_abs_path` resolves under root | 1 | `paths::tests` |
| `stage_chunks` write + idempotency + email-skip | 3 | `mod::tests` |
| `ensure_schema` idempotency | 1 | `store::tests` |
| `upsert + get` round-trip | 1 | `store::tests` |
| `upsert` idempotency | 1 | `store::tests` |
| `list_chunks_by_source` order + limit | 2 | `store::tests` |
| `get_chunk` missing | 1 | `store::tests` |
| `count_chunks` | 1 | `store::tests` |
| **Total new tests** | **~29** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B scope from brainstorming → types subset + atomic + paths subset + SQLite store with chunks-only schema + StagedChunk + stage_chunks. **All present**.
- ✅ **Scope check**: NO summaries, NO score, NO entity index, NO embeddings, NO raw refs, NO lifecycle status, NO source ingest gate, NO jobs table, NO YAML front-matter compose (deferred to PR6), NO `read_chunk_body` (deferred to PR6+), NO AppState wiring (deferred to PR9). Lossy port is **explicitly bounded**.
- ✅ **Type consistency**: `Chunk`, `Metadata`, `SourceKind`, `SourceRef` mirror openhuman verbatim (drop `DataSource`, `partial_message` defaults false). `StagedChunk` exposes only `chunk + content_path + content_sha256`.
- ✅ **No placeholders**: every step shows actual code. Adaptation responsibility blocks explicitly enumerate verifications the implementer must do against the openhuman source.
- ✅ **Bisectability**: 5 task commits (skeleton+types / atomic / paths / mod / store) + optional cleanup. Each compiles standalone (types is dependency-free; atomic depends on types only via no imports; paths is standalone; mod depends on atomic+paths+types; store depends on mod for `StagedChunk`).
- ✅ **No new deps**: confirmed rusqlite, sha2, hex, uuid, chrono, tempfile all already in workspace Cargo.toml.
- ✅ **Logging discipline**: `tracing::*` everywhere; no `log::*` slips through.
- ✅ **DB isolation**: separate `chunks.db` file; no V-number coordination with uClaw's `migrations.rs`.
