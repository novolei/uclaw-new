# Dirac-A3 — ReadFile Hash Short-Circuit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Steps use `- [ ]` syntax.

**Goal:** Add `[File Hash: 0x<fnv1a32>]` prefix to every `read_file` output and accept optional `assume_hash` param to short-circuit re-reads of unchanged files.

**Architecture:** Pure addition. New `compute_file_hash` helper (FNV1a32). New optional schema property. Conditional output path when hash matches.

**Tech Stack:** Rust only. No new crates (FNV1a32 is a 5-line implementation; reuse `anchor_state.rs::fnv1a_32` if present).

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-a3-read-hash-shortcircuit-design.md`

**PR tag:** `[C1-Dirac-A3]`

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/tools/builtin/file.rs` | Add `compute_file_hash` + `parse_assume_hash` + extend `ReadFileTool::execute` + schema. +60-80 lines. |
| `src-tauri/src/agent/tools/builtin/file.rs::mod tests` | +6 tests. ~100 lines. |
| `docs/superpowers/MILESTONE_STATUS.md` | One-line entry |

### Possibly touched (audit)

| Path | Why |
|---|---|
| `src-tauri/src/agent/anchor_state.rs` | If `fnv1a_32` exists here, reuse it instead of duplicating. Spec §3.3 hash function. |
| `ui/src/components/**/file-preview*.tsx` (or similar) | If frontend parses tool result expecting raw file content at index 0, add `[File Hash: ...]` line strip. |

---

## Pre-flight

- [ ] **Step 0.1: Branch + baseline**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/dirac-a3-read-hash-shortcircuit
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
```

- [ ] **Step 0.2: Reuse audit**

```bash
grep -rn "fnv1a_32\|compute_file_hash\|FNV" src-tauri/src/
# If anchor_state.rs has fnv1a_32, plan to reuse it. Otherwise add fresh.
```

Decide based on result:
- **If `anchor_state::fnv1a_32` exists**: import + reuse. Commit 1 message mentions "deduplicates fnv1a_32 callsites" instead of "adds helper".
- **If not**: add fresh in file.rs. Note in spec §3.3 update.

- [ ] **Step 0.3: Frontend audit**

```bash
grep -rn "read_file" ui/src/ | grep -v test | head -20
# Look for any place that parses tool result text expecting raw file content at byte 0.
```

If parsers exist, plan to strip `[File Hash: 0x...]\n` prefix client-side
in their handler. Add as commit 2 or 3.

---

## Task 1: Add `compute_file_hash` + `parse_assume_hash`

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/file.rs`

- [ ] **Step 1.1: Add hash helper (if not already present)**

If reusing existing `anchor_state::fnv1a_32`:

```rust
use crate::agent::anchor_state::fnv1a_32;

pub fn compute_file_hash(content: &str) -> u32 {
    fnv1a_32(content.as_bytes())
}
```

Otherwise, add fresh:

```rust
/// FNV-1a 32-bit hash of file content. Matches Dirac's contentHash
/// (src/utils/line-hashing.ts) so future borrow patterns remain compatible.
pub fn compute_file_hash(content: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in content.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x01000193);
    }
    h
}
```

- [ ] **Step 1.2: Add `parse_assume_hash`**

```rust
/// Parse an optional `assume_hash` parameter. Accepts "0xab12cd34" or
/// "AB12CD34" (case-insensitive, with or without 0x prefix). Returns
/// None on malformed input — caller decides whether to reject or
/// continue without short-circuit.
fn parse_assume_hash(s: &str) -> Option<u32> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if stripped.len() != 8 || !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(stripped, 16).ok()
}
```

- [ ] **Step 1.3: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
```

**Combine with Task 2 in commit 1.**

---

## Task 2: Emit `[File Hash: ...]` header on every read

- [ ] **Step 2.1: Update `ReadFileTool::execute`**

```rust
async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let start = std::time::Instant::now();
    let path = params["path"].as_str()
        .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
    let full_path = if PathBuf::from(path).is_absolute() {
        PathBuf::from(path)
    } else {
        self.workspace_root.join(path)
    };

    let content = match fs::read_to_string(&full_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ToolError::kinded(
                ToolErrorKind::ResourceNotFound,
                format!("File not found: {}", full_path.display()),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(ToolError::kinded(
                ToolErrorKind::PermissionDenied,
                format!("Permission denied: {}", full_path.display()),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    let current_hash = compute_file_hash(&content);
    let header = format!("[File Hash: 0x{:08x}]", current_hash);

    let provided_hash = match params.get("assume_hash").and_then(|v| v.as_str()) {
        Some(s) => match parse_assume_hash(s) {
            Some(h) => Some(h),
            None => return Err(ToolError::InvalidParams(format!(
                "Invalid assume_hash format: {s:?}. Expected 0x-prefixed 8-char hex (e.g., 0xab12cd34)."
            ))),
        },
        None => None,
    };

    let output = if Some(current_hash) == provided_hash {
        format!(
            "{header}\nno changes have been made to the file since your last read (Hash: 0x{:08x})",
            current_hash,
        )
    } else {
        format!("{header}\n{content}")
    };

    Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
}
```

- [ ] **Step 2.2: Verify existing tests still pass with header prefix**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
```

If any existing test asserts `assert_eq!(output, "<file content>")` —
the test will break (header now prepended). Update the test to either:
- assert `output.ends_with(<expected>)`, or
- strip the leading `[File Hash: ...]\n` line before comparison.

Document the test change as part of commit 1 ("test: adapt existing
ReadFileTool tests to new [File Hash:] header prefix").

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/file): emit [File Hash: 0x...] header + compute_file_hash helper

Every read_file response is now prefixed with [File Hash: 0xab12cd34],
where the hex is FNV-1a 32 of the file content. Existing callers see
content shifted by one leading line; tests adapted.

This is the foundation for the assume_hash short-circuit in the next
commit. Matches Dirac contentHash (src/utils/line-hashing.ts) and is
algorithm-compatible with anchor_state.rs (if reusing fnv1a_32 there).

Spec: docs/superpowers/specs/2026-05-25-dirac-a3-read-hash-shortcircuit-design.md"
```

---

## Task 3: Schema + tool description

- [ ] **Step 3.1: Update `parameters_schema`**

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
                "description": "Optional. If you saw `[File Hash: 0x...]` on a prior read of this file, pass that value here. If the file hasn't changed since, the tool returns a short 'no changes' confirmation instead of the full content — saves significant tokens on repeated reads of large files. Format: 0x-prefixed 8-char hex, e.g. \"0xab12cd34\".",
                "pattern": "^0[xX][0-9a-fA-F]{8}$"
            }
        },
        "required": ["path"]
    })
}
```

- [ ] **Step 3.2: Update tool description**

```rust
fn description(&self) -> &str {
    "Read the contents of a file. Returns text content prefixed with [File Hash: 0x...]. \
     For repeated reads of the same file, pass the prior hash as `assume_hash` — \
     if unchanged, the tool short-circuits with a one-line confirmation."
}
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/file): accept optional assume_hash for read short-circuit

Schema gains optional assume_hash property. When the LLM passes back
a hash from a prior read, and the current file hash matches, the tool
returns:

    [File Hash: 0xab12cd34]
    no changes have been made to the file since your last read (Hash: 0xab12cd34)

instead of re-emitting the full content. Token savings approach 95%+
for large files re-read multiple times in a task.

Malformed assume_hash → InvalidParams (not silent ignore — surfaces
bugs in LLM/history corruption).

Inspired by Dirac ReadFileToolHandler.ts:289-294."
```

---

## Task 4: Six new tests

- [ ] **Step 4.1: `test_compute_file_hash_fnv1a_known_values`**

```rust
#[test]
fn test_compute_file_hash_fnv1a_known_values() {
    assert_eq!(compute_file_hash(""), 0x811c9dc5);
    assert_eq!(compute_file_hash("a"), 0xe40c292c);
    assert_eq!(compute_file_hash("foobar"), 0xbf9cf968);
}
```

These are published FNV-1a 32-bit test vectors. If they fail, the
hash implementation is wrong — fix it before proceeding.

- [ ] **Step 4.2: `test_read_emits_file_hash_header`**

```rust
#[tokio::test]
async fn test_read_emits_file_hash_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.txt");
    tokio::fs::write(&path, "hello world").await.unwrap();
    let tool = ReadFileTool::new(dir.path().to_path_buf());
    let result = tool.execute(json!({"path": "foo.txt"})).await.unwrap();
    let out = result.output_text();
    assert!(out.starts_with("[File Hash: 0x"), "got: {}", out);
    assert!(out.contains("\nhello world"), "got: {}", out);
}
```

- [ ] **Step 4.3: `test_read_short_circuits_on_matching_hash`**

```rust
#[tokio::test]
async fn test_read_short_circuits_on_matching_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.txt");
    tokio::fs::write(&path, "stable content").await.unwrap();
    let tool = ReadFileTool::new(dir.path().to_path_buf());

    let first = tool.execute(json!({"path": "foo.txt"})).await.unwrap();
    let first_out = first.output_text();
    let hash_part = first_out.split_once('\n').unwrap().0; // "[File Hash: 0x...]"
    let hash_hex = hash_part
        .trim_start_matches("[File Hash: ")
        .trim_end_matches(']');

    let second = tool.execute(json!({
        "path": "foo.txt",
        "assume_hash": hash_hex,
    })).await.unwrap();
    let second_out = second.output_text();
    assert!(second_out.contains("no changes have been made"), "got: {}", second_out);
    assert!(!second_out.contains("stable content"), "should not re-emit content");
}
```

- [ ] **Step 4.4: `test_read_returns_content_on_mismatched_hash`**

```rust
#[tokio::test]
async fn test_read_returns_content_on_mismatched_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.txt");
    tokio::fs::write(&path, "new content").await.unwrap();
    let tool = ReadFileTool::new(dir.path().to_path_buf());

    let result = tool.execute(json!({
        "path": "foo.txt",
        "assume_hash": "0x00000000",  // wrong
    })).await.unwrap();
    let out = result.output_text();
    assert!(out.contains("new content"), "got: {}", out);
    assert!(!out.contains("no changes have been made"));
}
```

- [ ] **Step 4.5: `test_read_returns_content_when_no_assume_hash`**

```rust
#[tokio::test]
async fn test_read_returns_content_when_no_assume_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.txt");
    tokio::fs::write(&path, "first read content").await.unwrap();
    let tool = ReadFileTool::new(dir.path().to_path_buf());

    let result = tool.execute(json!({"path": "foo.txt"})).await.unwrap();
    let out = result.output_text();
    assert!(out.contains("[File Hash:"));
    assert!(out.contains("first read content"));
}
```

- [ ] **Step 4.6: `test_read_rejects_malformed_assume_hash`**

```rust
#[tokio::test]
async fn test_read_rejects_malformed_assume_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.txt");
    tokio::fs::write(&path, "x").await.unwrap();
    let tool = ReadFileTool::new(dir.path().to_path_buf());

    let result = tool.execute(json!({
        "path": "foo.txt",
        "assume_hash": "not-hex",
    })).await;
    match result {
        Err(ToolError::InvalidParams(msg)) => {
            assert!(msg.contains("Invalid assume_hash"), "got: {}", msg);
        }
        other => panic!("expected InvalidParams, got: {:?}", other),
    }
}
```

- [ ] **Step 4.7: Run all tests**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -15
# Expect: existing + 6 new tests passing
```

**Commit checkpoint:**
```
git add -A
git commit -m "test(tools/builtin/file): 6 tests for File Hash header + assume_hash short-circuit

Covers:
- FNV-1a 32 hash known test vectors (empty, 'a', 'foobar')
- Read emits [File Hash: 0x...] header
- Matching assume_hash → short-circuit message (no content)
- Mismatched assume_hash → full content + new hash
- Missing assume_hash → full content (normal read)
- Malformed assume_hash → InvalidParams with helpful message"
```

---

## Task 5: Frontend parser audit + adapt (if needed)

- [ ] **Step 5.1: Locate read_file result parsers in frontend**

```bash
grep -rn "read_file\|ReadFileTool" ui/src/ | grep -v test
```

For each location, check if it expects raw file content at byte 0.
If so:
- Add a leading-line strip: `output.replace(/^\[File Hash: 0x[0-9a-f]{8}\]\n/i, '')`
- Add a comment pointing back to this PR

- [ ] **Step 5.2: Run UI tests**

```bash
cd ui && npx tsc --noEmit && echo "tsc clean"
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: no regressions.

If frontend changes were made:
```
git add ui/
git commit -m "fix(ui): strip [File Hash:] prefix from read_file results before display"
```

If no frontend changes needed: skip this commit.

---

## Task 6: SSoT + PR

- [ ] **Step 6.1: Update MILESTONE_STATUS**

```
| C1-Dirac-A3 | ReadFile [File Hash] header + assume_hash short-circuit | #<PR-number> |
```

- [ ] **Step 6.2: Drift check + push + PR**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -5
git push -u origin claude/dirac-a3-read-hash-shortcircuit

gh pr create \
  --title "[C1-Dirac-A3] feat(tools/file): [File Hash] header + assume_hash short-circuit" \
  --body "..."
```

PR description includes:
- Summary (one paragraph)
- Why (link research doc §7.2)
- Commits (bisectable) — 4-5 commits
- Verification — cargo test output, hash test vector check
- Token-savings analysis (spec §3.5 table)
- Spec link
- Closes (C1-Dirac-A3)

- [ ] **Step 6.3: Self-merge gate**

- [ ] CI green
- [ ] PR tag `[C1-Dirac-A3]`
- [ ] Frontend audit recorded (changes made OR explicit "no changes needed")
- [ ] FNV test vectors green (proves hash impl correct)

---

## Rollback procedure

```bash
git revert <merge-commit-sha>
git push
```

Header disappears, short-circuit disappears. Existing callers fully
restored to pre-PR behavior. Token-savings benefit lost; no data corruption.

---

## Closes / unblocks

- C1-Dirac-A3 ✓
- Drives M2 progress ~+3-4%
- Pairs with A4 (next PR teaches LLM to use assume_hash via tool
  description / system prompt tuning)
- Foundation for Phase C2 `[Function Hash: ...]` AST tool variant

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> Run only when invoked by the autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).

- [ ] **Step A.1: Stage 2 self-verify (per protocol §3.2)**

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
cargo clippy --lib -- -D warnings 2>&1 | tail -5

# Scope: ONLY these files + frontend audit results
git diff --name-only main..HEAD | sort
# Expected: src-tauri/src/agent/tools/builtin/file.rs
#         + docs/superpowers/MILESTONE_STATUS.md
#         + optionally ui/src/... if frontend parser audit found something

# FNV test vectors check (non-negotiable per spec §12)
cargo test --lib agent::tools::builtin::file::tests::test_compute_file_hash_fnv1a_known_values 2>&1 | tail -3
# Must show: test result: ok. 1 passed
```

- [ ] **Step A.2: Spawn adversarial reviewer (protocol §3.3)**

A3-specific focus from spec §12:
- FNV known-vector test present and passing
- `[File Hash: 0x...]` format consistency (lowercase, 8 hex, 0x prefix)
- Downstream parser audit (plan Step 0.3) documented in PR body

- [ ] **Step A.3: Reconcile per protocol §3.4**

- [ ] **Step A.4: PR open + CI + auto-merge (protocol §3.5)**

```bash
git push -u origin claude/dirac-a3-read-hash-shortcircuit
PR=$(gh pr create --title "[C1-Dirac-A3] feat(tools/file): [File Hash] header + assume_hash short-circuit" --body-file ./pr-body.md --json number -q .number)
gh pr checks $PR --watch --interval 30 --required
gh pr merge $PR --merge --delete-branch
git checkout main && git pull
```

- [ ] **Step A.5: Log + return outcome (protocol §7)**
