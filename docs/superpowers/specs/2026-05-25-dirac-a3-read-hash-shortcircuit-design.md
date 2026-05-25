# Dirac-A3 — ReadFile Hash Short-Circuit (C1)

> **Context**: Phase A item #3 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> Companion plan: [`plans/2026-05-25-dirac-a3-read-hash-shortcircuit.md`](../plans/2026-05-25-dirac-a3-read-hash-shortcircuit.md).
> **C1 slot**: M2 closeout. Independent of A1/A2.

## 1. Background

`src-tauri/src/agent/tools/builtin/file.rs::ReadFileTool` (line 6) is
the canonical read tool. Today's flow:

1. LLM emits `{"path": "src/foo.rs"}`
2. Tool reads full content via `fs::read_to_string`
3. Returns raw content as `ToolOutput::success(&content, ...)`

Re-reading the same unchanged file repeats steps 2-3 verbatim — sending
the full file body back through the LLM **on every read**. For a
50 KB file re-read 5 times in a task, that's 250 KB of redundant
tokens through the model.

**Dirac's pattern** (research doc §1.1 #3, verified at
`/Users/ryanliu/Documents/dirac/src/core/task/tools/handlers/ReadFileToolHandler.ts:280-303`):

1. Tool output is prefixed with `[File Hash: <FNV1a32 hex>]\n` (or
   embedded as a header alongside other metadata).
2. Tool accepts optional `assume_hash` (or `last_known_hash`) parameter.
3. If `assume_hash == current_hash` AND no `start_line`/`end_line`
   range → return one line:
   `"no changes have been made to the file since your last read (Hash: <hash>)"`
4. LLM, having previously seen `[File Hash: 0xab12cd34]` in earlier
   tool output, can pass it back on the next read and get a zero-token
   confirmation instead of a re-emission.

**Verified**: Dirac source line 293-294:

```ts
if (providedHash === currentHash && !startLineNum && !endLineNum) {
    results.push(`${header}no changes have been made to the file since your last read (Hash: ${providedHash})`)
}
```

This is **content-addressed memoization**: no cache layer, no eviction
policy. The hash itself serves as the cache key, and the LLM holds
the cache.

## 2. Scope

Single PR. One subsystem (`agent::tools::builtin::file`). Pure addition
— no behavior change for callers that don't pass `assume_hash`.

### 2.1 In scope

1. Add `compute_file_hash(content: &str) -> u32` using **FNV-1a 32-bit**
   (same hash family Dirac uses; matches existing
   `anchor_state.rs::fnv1a_32` if present — verify and reuse).
2. Extend `ReadFileTool::parameters_schema()`:
   - Add optional `assume_hash` (string, hex-formatted u32).
3. Prefix tool output with `[File Hash: 0x<hex>]\n` header, always.
4. On `assume_hash == current_hash` and no line range → return
   short-circuit message instead of file body.
5. Update tool description to teach the LLM about the optional param.
6. ~5 new unit tests.

### 2.2 Out of scope

- **Multi-file `read_file`** (Dirac `paths: string[]`) — defer to a
  follow-up; A3 is single-file only for ROI clarity. Multi-file is a
  larger change with its own approval-flow + diagnostic-flow effects.
- **Hash-based range fetching** ("re-read but only since this hash") —
  not in Dirac either; deferred.
- **Function-level hashes** (Dirac `[Function Hash: ...]`) — needs
  AST tooling first; deferred to Phase C2.
- **Hash short-circuit for `search_files` / `list_files`** —
  separate concern; defer.

## 3. Design

### 3.1 Hash function choice

**FNV-1a 32-bit**, matches Dirac's `contentHash` in
`src/utils/line-hashing.ts:13-19`. Already used elsewhere in uClaw
for anchor states (`anchor_state.rs`), so the algorithm is familiar.

Why not BLAKE3 / SHA-256? Cryptographic strength not needed — we just
need collision-low identity for short-circuit. FNV1a32 collides at
~1 in 4 billion; tasks read at most O(100) distinct files. Acceptable.
And it's ~3× faster than blake3 on small inputs, which matters for
the un-cached read path that hashes every read.

**Display**: `[File Hash: 0xab12cd34]` (hex, lowercase, prefix `0x`).
8-char hex + 4-char wrapper + literal = ~20 tokens overhead per read.
Negligible vs the file body it replaces on hits.

### 3.2 Tool schema change

```rust
fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Relative or absolute path to the file to read"
            },
            "assume_hash": {
                "type": "string",
                "description": "Optional. The `[File Hash: 0x...]` value from a prior read of this file. \
                                If the file hasn't changed since, the tool returns a short confirmation \
                                instead of the full content — saves tokens on repeated reads. \
                                Format: 8-char hex with `0x` prefix (e.g., \"0xab12cd34\").",
                "pattern": "^0x[0-9a-fA-F]{8}$"
            }
        },
        "required": ["path"]
    })
}
```

Optional param, no breaking change.

### 3.3 Execute flow

```rust
async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let start = std::time::Instant::now();
    let path = params["path"].as_str()
        .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
    let full_path = self.resolve_path(path);

    let content = self.read_file_with_error_mapping(&full_path).await?;
    let current_hash = compute_file_hash(&content);
    let hash_header = format!("[File Hash: 0x{:08x}]", current_hash);

    let provided_hash = params.get("assume_hash")
        .and_then(|v| v.as_str())
        .and_then(parse_assume_hash);

    if Some(current_hash) == provided_hash {
        let msg = format!(
            "{}\nno changes have been made to the file since your last read (Hash: 0x{:08x})",
            hash_header, current_hash,
        );
        return Ok(ToolOutput::success(&msg, start.elapsed().as_millis() as u64));
    }

    let output = format!("{}\n{}", hash_header, content);
    Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
}

fn parse_assume_hash(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

pub fn compute_file_hash(content: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in content.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x01000193);
    }
    h
}
```

**Hash header always present**. The short-circuit only fires when the
LLM provides a matching `assume_hash`.

### 3.4 What the LLM sees over a task lifecycle

Turn 1 — first read:
```
LLM emits: {"path": "src/foo.rs"}
Tool returns:
  [File Hash: 0xab12cd34]
  <full content of src/foo.rs>
```

Turn 5 — LLM wants to confirm nothing changed before editing:
```
LLM emits: {"path": "src/foo.rs", "assume_hash": "0xab12cd34"}
Tool returns:
  [File Hash: 0xab12cd34]
  no changes have been made to the file since your last read (Hash: 0xab12cd34)
```

Turn 8 — user externally edited the file:
```
LLM emits: {"path": "src/foo.rs", "assume_hash": "0xab12cd34"}
Tool returns:
  [File Hash: 0x12345678]
  <full content of src/foo.rs — different from last time>
```

LLM observes hash changed and knows to redo any anchor-based reasoning.

### 3.5 Token-savings model

For a 200-line Rust file (~6 KB, ~1500 tokens), re-read 5 times in a
task:

| Strategy | Total tokens out from tool |
|---|---|
| No short-circuit | 5 × 1500 = 7500 |
| With short-circuit (4 hits) | 1500 + 4 × ~20 = 1580 |
| Savings | **~79%** |

Bigger files → bigger savings. For the 50 KB code files that show up
in refactor tasks, savings approach 95-99% on each re-read.

## 4. Interfaces

No public Rust API change to existing callers. New:

- `pub fn compute_file_hash(&str) -> u32` exposed for future reuse
  (`anchor_state.rs` may want to share — if it already has fnv1a_32,
  pull that one in instead of duplicating; cite the unification in
  commit message).
- `ReadFileTool::parameters_schema` adds one optional property.

## 5. Tests

In `agent::tools::builtin::file` test module. Six tests (one helper
deduped):

| # | Test | Scenario |
|---|---|---|
| 1 | `test_read_emits_file_hash_header` | Read any file → output starts with `[File Hash: 0x...]` |
| 2 | `test_read_short_circuits_on_matching_hash` | Read → capture hash → re-read with assume_hash → short-circuit message |
| 3 | `test_read_returns_content_on_mismatched_hash` | Read with stale assume_hash → returns full content + new hash |
| 4 | `test_read_returns_content_when_no_assume_hash` | No `assume_hash` param → full content + hash header |
| 5 | `test_read_rejects_malformed_assume_hash` | `assume_hash: "not-hex"` → InvalidParams |
| 6 | `test_compute_file_hash_fnv1a_known_values` | Hash of empty string → 0x811c9dc5 (FNV offset); hash of "abc" → known FNV1a32 value |

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
# Expect: existing + 6 new tests passing

cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo clippy --lib -- -D warnings | tail -5
```

### 6.2 Cross-check FNV constants

In `test_compute_file_hash_fnv1a_known_values`, assert against known
test vectors:

```rust
assert_eq!(compute_file_hash(""), 0x811c9dc5);
assert_eq!(compute_file_hash("a"), 0xe40c292c);
assert_eq!(compute_file_hash("foobar"), 0xbf9cf968);
```

These are published FNV-1a 32-bit test vectors and lock the algorithm
in. If `anchor_state.rs::fnv1a_32` exists and is reused, run the same
tests against it to confirm identity.

### 6.3 Integration smoke (manual)

Run a session that reads the same file 3 times. Tail rollout.jsonl and
verify:
- Read 1: full content in tool result
- Read 2/3 (if LLM passes assume_hash): short-circuit message

LLM behavior depends on prompt — A4 (next PR) will tune the system
prompt to teach `assume_hash` usage. For A3 alone, manual integration
will only show short-circuit if the LLM organically discovers it from
the tool description. Acceptable — A3 ships the *capability*, A4 ships
the *training*.

## 7. Migration / rollback

- **Migration**: none.
- **Backward compat**: `assume_hash` is optional. Callers / rollouts
  that don't pass it see no behavior change — file content prefixed
  with `[File Hash: ...]` is a strict prefix addition.
- **The header IS a behavior change for downstream parsers**. Verify:
  - Rollout JSONL replay: header is a leading line in the content
    string; replay tools that grep for "fn " etc. still find them.
  - Frontend file preview: if a UI parses the tool result expecting
    raw file content at the start, it sees `[File Hash: ...]\n` first.
    Inspect `ui/src/components/...` for any such parser; if found,
    strip the leading line before render.
- **Rollback**: revert PR. Header disappears, short-circuit disappears,
  parsers (if any depended on header) return to prior state.
- **Feature flag**: not needed.

## 8. Decisions (locked 2026-05-25)

### 8.1 FNV-1a 32-bit, not 64-bit or BLAKE3

- **Decision**: FNV1a32.
- **Why**: matches Dirac (their `contentHash` is also FNV1a32);
  matches likely-existing `anchor_state.rs::fnv1a_32`. 32-bit
  collision risk (~1 in 4B) is acceptable for the use case (LLM
  short-circuit, not cryptographic identity). 8-hex-char display
  is more LLM-token-friendly than 16-hex (64-bit) or 64-hex (256-bit).

### 8.2 Hash header always emitted, short-circuit conditional

- **Decision**: prefix output with `[File Hash: 0x...]` on every read,
  whether or not short-circuit fires.
- **Why**: the LLM needs to **observe** the hash to know what to
  pass next time. If we only emitted on short-circuit, the LLM has
  no way to learn the value for a re-read. Inspired by Dirac's
  `ReadFileToolHandler.ts:303` which emits the header on the apply
  path.

### 8.3 Hex format with `0x` prefix

- **Decision**: `0xab12cd34` not `ab12cd34` and not raw decimal.
- **Why**: `0x` prefix is unambiguous → LLM less likely to mangle
  with a leading zero strip. Hex is conventional for "this is an
  opaque identifier, not a number".

### 8.4 Reject malformed `assume_hash` rather than ignore

- **Decision**: malformed `assume_hash` → `InvalidParams`, not "treat
  as no assume_hash and continue".
- **Why**: silently dropping a malformed param hides bugs. If the LLM
  emits `assume_hash: "stale"` from a corrupt history, we want a
  clear error message back, not a silent re-read. Mitigation: error
  message says exactly what valid format looks like ("expected
  0x-prefixed 8-char hex").

### 8.5 No range-aware short-circuit yet

- **Decision**: if `start_line` / `end_line` (when added in a future
  multi-file PR) are present alongside `assume_hash`, no
  short-circuit. Returns full content even on hash match.
- **Why**: matches Dirac line 293 — `!startLineNum && !endLineNum`
  guard. The hash is whole-file; line ranges may select a region
  that's stable while the whole file isn't, or vice versa. Out of
  scope to reason about; safest is fall through.

## 9. Concrete commit plan

```
Commit 1: feat(tools/builtin/file): emit [File Hash: 0x...] header on every read
          + compute_file_hash (FNV1a32) helper
Commit 2: feat(tools/builtin/file): accept optional assume_hash for read short-circuit
          + parameters_schema update + tool description update
Commit 3: test(tools/builtin/file): 6 new tests covering header, short-circuit, malformed input
Commit 4: docs(MILESTONE_STATUS): record C1-Dirac-A3 completion
```

Four commits, ~200-250 lines of diff. Bisectable.

## 10. Estimated effort

- Coding: 2-3 hours
- Tests: 1-2 hours
- Downstream parser audit (frontend / rollout replay): 1 hour
- **Total: ~1 day** (matches research doc estimate)

## 11. Closes / unblocks

- C1-Dirac-A3 ✓
- Drives M2 progress ~+3-4%
- Pairs naturally with A4 (next PR teaches LLM to use `assume_hash`
  via tool description tuning)
- Long-tail: foundation for `[Function Hash: ...]` in C2 AST tools

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)):

- **Downstream parser audit** (Stage 1 pre-flight): plan Step 0.3
  audits frontend parsers for the new `[File Hash:]` prefix. Audit
  MUST be performed and result logged. If parsers found and modified,
  the modification is in-scope.
- **FNV test vectors** (Stage 2 #2 + Stage 3 critical): the test
  `test_compute_file_hash_fnv1a_known_values` (plan Step 4.1) is
  non-negotiable. The 3 known vectors (empty, "a", "foobar") with
  exact expected values MUST be present. Reviewer rejects if
  missing — it's the only way to lock the hash algorithm in.
- **Hash format consistency** (Stage 3): `0x`-prefixed 8-char hex,
  lowercase. Reviewer verifies both in the output and in
  `parse_assume_hash` regex/parser. Format drift here breaks the
  short-circuit silently.
- **Output-format contract change** (Stage 3): the
  `[File Hash: 0x...]` header is the second observable behavior
  change to ReadFileTool output (after B1's anchored lines lands
  later). Reviewer checks A3-era tests are properly updated, not
  silenced.
- **No-bench**: §6 declares no required bench.
- **Risk class**: LOW-MEDIUM — additive header is a contract change
  for downstream consumers; otherwise contained.
