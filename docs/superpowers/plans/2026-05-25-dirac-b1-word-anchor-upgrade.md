# Dirac-B1 — Word-Anchor Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Steps use `- [ ]` syntax.
>
> ⚠️ **C2 ORDERING**: Do NOT start until C1 (Phase A A1-A4 + M2 closeout) ships. Run `./scripts/milestone-drift-check.sh --since "1 week ago"` and confirm C1 is closed in MILESTONE_STATUS.md before touching code.

**Goal:** Make uClaw's existing word-anchor machinery (`anchor_state.rs`) load-bearing in the ReadFile/EditTool pipeline. Fix the cross-read alignment bug, pivot anchor format to `Apple§<literal content>`, expand dictionary via 2-word combos, wire ReadFileTool to emit anchors, wire EditTool to consume them with 4-step byte-equal validation + stale-file rejection.

**Architecture:** Tokens stored in `AnchorStateManager` (small, opaque), literals composed at render time via `render_anchor_line`. EditTool gains `AnchoredEdit` variant alongside existing literal/line shapes (additive, like A2's `files: [...]`).

**Tech Stack:** Rust only. Existing `similar` crate (already in `edit.rs` and `anchor_state.rs`). No new crates.

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-b1-word-anchor-upgrade-design.md`

**PR tag:** `[C2-Dirac-B1]`

**Depends on:**
- C1-Dirac-A2 merged (multi-file edit schema — provides the
  refactored `validate_single_file` / `apply_validated_single_file`
  hooks that B1 extends with the anchored path)
- C1-Dirac-A3 merged ([File Hash:] header — output composition is
  coordinated)

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/anchor_state.rs` | Major refactor — `FileAnchorState`, `record_read` API, `generate_anchor_token`, 2-word escalation, `render_anchor_line`. Deprecate `register_file_lines`. +250-300 lines. |
| `src-tauri/src/agent/tools/builtin/file.rs` | `ReadFileTool::execute` calls `record_read` + renders anchor-prefixed lines after [File Hash:]. +30-40 lines. |
| `src-tauri/src/agent/tools/builtin/edit.rs` | `AnchoredEdit` variant + `resolve_anchored_edit` 4-step validator + stale-file precondition. +180-220 lines. |
| `src-tauri/src/agent/tools/tool.rs` (or wherever `ToolErrorKind` lives) | Add `PreconditionFailed` variant if absent. ~5 lines. |
| `src-tauri/src/agent/anchor_state.rs::tests` | +12 new tests. ~250 lines. |
| `src-tauri/src/agent/tools/builtin/edit.rs::tests` | +5 anchored-edit tests. ~150 lines. |
| `src-tauri/src/agent/tools/builtin/file.rs::tests` | +2 read-emits-anchor tests. ~50 lines. |
| `docs/superpowers/MILESTONE_STATUS.md` | One-line entry under M2 / M3 wire-up |

### Possibly touched

| Path | Why |
|---|---|
| `ui/src/components/**/*read*.tsx` | If the frontend parses read_file output expecting plain content, add anchor-section strip |
| Any code that calls `register_file_lines` directly | Deprecation warning surfaces — migrate to `record_read` |

---

## Pre-flight

- [ ] **Step 0.1: Confirm C1 is closed**

```bash
cd /Users/ryanliu/Documents/uclaw
git fetch origin && git checkout main && git pull
grep -A 2 "^## C1\|## Phase A\|C1-Dirac" docs/superpowers/MILESTONE_STATUS.md | head -30
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
```

**Expected**: MILESTONE_STATUS shows C1 closed (A1-A4 all merged), drift
GREEN/YELLOW. If C1 isn't closed → STOP, raise it, do not proceed.

- [ ] **Step 0.2: Branch + baseline**

```bash
git checkout -b claude/dirac-b1-word-anchor-upgrade
cd src-tauri && cargo test --lib agent::anchor_state 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
```

Expect: existing tests pass. If any A1-A4 test regressed, that's a
merge-conflict signal — fix before continuing.

- [ ] **Step 0.3: Downstream parser audit**

The output format change (anchors per line) is a contract change.
Audit:

```bash
grep -rn "read_file\|ReadFileTool::execute" ui/src/ src-tauri/src/ \
  | grep -v "test\|spec" | head -30
grep -rn "register_file_lines\|generate_anchor\|align_file_anchors" src-tauri/src/ \
  | grep -v "anchor_state.rs"
```

Record findings:
- Frontend parsers (if any): plan to strip anchor-section before
  display rendering, OR pass through if the UI shows the same string
  the LLM sees
- Existing `register_file_lines` callsites: list them. Each gets a
  migration to `record_read` in Task 2.

- [ ] **Step 0.4: Verify ToolErrorKind shape**

```bash
grep -n "enum ToolErrorKind\|PreconditionFailed" src-tauri/src/agent/tools/tool.rs
```

Need `PreconditionFailed` variant. If absent, add as part of Task 4.

---

## Task 1: Refactor `AnchorStateManager` to `FileAnchorState` + `record_read`

**Files:**
- Modify: `src-tauri/src/agent/anchor_state.rs`

- [ ] **Step 1.1: Add `FileAnchorState` struct**

After the existing `pub static GLOBAL_*` blocks but before
`AnchorStateManager`:

```rust
/// Per-file anchor state — last-seen lines + 1:1 anchor tokens.
/// Stored together so Myers-diff alignment can run cross-read
/// without the caller needing to track old content.
#[derive(Debug, Clone)]
struct FileAnchorState {
    lines: Vec<String>,
    anchors: Vec<String>,
}
```

- [ ] **Step 1.2: Replace `AnchorStateManager.file_anchors` field**

```rust
#[derive(Default)]
pub struct AnchorStateManager {
    files: Mutex<HashMap<PathBuf, FileAnchorState>>,
}
```

Adjust `Default` impl if it explicitly initialized the old field name.

- [ ] **Step 1.3: Implement `record_read`**

```rust
impl AnchorStateManager {
    /// Idempotent: on first call initializes anchors; on subsequent
    /// calls aligns via Myers diff over (old_lines, new_lines) and
    /// returns the aligned anchor list.
    pub fn record_read(&self, path: &Path, lines: &[String]) -> Vec<String> {
        let mut files = self.files.lock().unwrap();
        let key = path.to_path_buf();
        let new_anchors = match files.get(&key) {
            None => initialize_anchors(lines),
            Some(prev) => align_anchors(&prev.lines, lines, &prev.anchors),
        };
        files.insert(key, FileAnchorState {
            lines: lines.to_vec(),
            anchors: new_anchors.clone(),
        });
        new_anchors
    }

    /// Anchor token → line index in CURRENT state. None if not tracked
    /// or token not found.
    pub fn resolve_anchor_index(&self, path: &Path, token: &str) -> Option<usize> {
        let files = self.files.lock().unwrap();
        files.get(path)?
            .anchors
            .iter()
            .position(|a| a == token)
    }

    /// (line_content, anchor_token) at idx. Used by EditTool to build
    /// "Expected: ..., Provided: ..." mismatch messages.
    pub fn snapshot_line(&self, path: &Path, idx: usize) -> Option<(String, String)> {
        let files = self.files.lock().unwrap();
        let state = files.get(path)?;
        Some((
            state.lines.get(idx)?.clone(),
            state.anchors.get(idx)?.clone(),
        ))
    }
}
```

- [ ] **Step 1.4: Deprecate old `register_file_lines` and `align_file_anchors` (kept as shims)**

```rust
impl AnchorStateManager {
    #[deprecated(note = "use record_read which handles initialize + align in one call")]
    pub fn register_file_lines(&self, path: &Path, lines: &[String]) {
        let _ = self.record_read(path, lines);
    }

    #[deprecated(note = "use record_read; old_lines tracked internally now")]
    pub fn align_file_anchors(&self, path: &Path, _old_lines: &[String], new_lines: &[String]) {
        let _ = self.record_read(path, new_lines);
    }

    pub fn get_anchors(&self, path: &Path) -> Option<Vec<String>> {
        let files = self.files.lock().unwrap();
        files.get(path).map(|s| s.anchors.clone())
    }

    pub fn unregister_file(&self, path: &Path) {
        let mut files = self.files.lock().unwrap();
        files.remove(path);
    }
}
```

- [ ] **Step 1.5: Migrate Step 0.3-discovered callsites**

For each callsite found in pre-flight that calls
`register_file_lines`/`align_file_anchors`: replace with
`record_read`. Each is a 1-line change. List the migrations in the
commit message.

- [ ] **Step 1.6: Build + existing tests still pass**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error|^warning.*deprecated" | head -20
# Expect: zero errors; deprecation warnings only on test-internal uses
cd src-tauri && cargo test --lib agent::anchor_state 2>&1 | tail -10
```

**Commit checkpoint:**
```
git add -A
git commit -m "refactor(agent/anchor_state): introduce FileAnchorState + record_read API

Replaces the previous (file_anchors HashMap) with (files HashMap of
FileAnchorState) which stores last-seen lines alongside anchors.
record_read is the new idempotent entry point that initializes on
first call and aligns via Myers diff on subsequent calls — fixes the
'we don't have old_lines directly' fall-through at the old
register_file_lines:136-140 path.

register_file_lines and align_file_anchors kept as #[deprecated]
shims forwarding to record_read.

resolve_anchor_index and snapshot_line are new APIs for the upcoming
EditTool 4-step validator (Task 4).

Pure plumbing — anchor TOKEN format unchanged in this commit (still
'Apple§a1f89c'). Format pivot lands in Commit 2.

Spec: docs/superpowers/specs/2026-05-25-dirac-b1-word-anchor-upgrade-design.md
Inspired by Dirac AnchorStateManager.reconcile()
(src/utils/AnchorStateManager.ts:119-217)."
```

---

## Task 2: Pivot anchor token + `render_anchor_line` + 2-word escalation

- [ ] **Step 2.1: Add `ANCHOR_DELIMITER` + `render_anchor_line`**

At top of `anchor_state.rs`:

```rust
/// The Unicode section sign (U+00A7) — separates anchor token from
/// literal line content in rendered output. Matches Dirac's
/// ANCHOR_DELIMITER (src/shared/utils/line-hashing.ts:6).
pub const ANCHOR_DELIMITER: char = '§';

/// Compose the LLM-visible anchored line. The token portion comes
/// from `AnchorStateManager` (stable across reads via Myers diff);
/// the literal content is the current line. EditTool's 4-step
/// validator splits on the delimiter and byte-compares.
pub fn render_anchor_line(token: &str, content: &str) -> String {
    format!("{}{}{}", token, ANCHOR_DELIMITER, content)
}
```

- [ ] **Step 2.2: Pivot `generate_anchor` → `generate_anchor_token`**

Replace the existing `generate_anchor` (which returned
`"Apple§a1f89c"`) with a token-only function:

```rust
/// Generate the anchor TOKEN portion for a line. Returns a stable
/// human-readable identifier ("Apple" for salt=0; "AppleCedar" for
/// salt=1; etc.). Caller composes the full anchor via
/// `render_anchor_line(token, line_content)`.
///
/// Salt parameter exists to escalate from 1-word → 2-word combos on
/// collision. `initialize_anchors` increments salt internally until a
/// unique token is found.
pub fn generate_anchor_token(line: &str, salt: u64) -> String {
    let trimmed = line.trim();
    let hash = u64::from(fnv1a_32(trimmed.as_bytes())) ^ salt;
    let n = CURATED_WORDS.len() as u64;
    let first = CURATED_WORDS[(hash % n) as usize];
    if salt == 0 {
        first.to_string()
    } else {
        let second = CURATED_WORDS[((hash / n) % n) as usize];
        format!("{first}{second}")
    }
}
```

Keep the old `generate_anchor` as a deprecated shim:

```rust
#[deprecated(note = "returns legacy 'Apple§<hash>' format; use generate_anchor_token + render_anchor_line for current format")]
pub fn generate_anchor(line: &str) -> String {
    let token = generate_anchor_token(line, 0);
    let hash = fnv1a_32(line.trim().as_bytes());
    format!("{}§{:06x}", token, hash & 0xFFFFFF)
}
```

- [ ] **Step 2.3: Update `initialize_anchors` to use salt escalation**

```rust
pub fn initialize_anchors(lines: &[String]) -> Vec<String> {
    let mut anchors = Vec::with_capacity(lines.len());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in lines {
        let mut salt = 0u64;
        let token = loop {
            let candidate = generate_anchor_token(line, salt);
            if seen.insert(candidate.clone()) {
                break candidate;
            }
            salt += 1;
            if salt > 10_000 {
                // Pathological collision storm — emit numbered fallback
                let fb = format!("Anchor{}", anchors.len());
                seen.insert(fb.clone());
                break fb;
            }
        };
        anchors.push(token);
    }

    anchors
}
```

- [ ] **Step 2.4: Update `align_anchors` — already correct**

`align_anchors` operates on anchor strings, not on the hash inside
the legacy format. It already does the right thing — verify by
running existing tests:

```bash
cd src-tauri && cargo test --lib agent::anchor_state 2>&1 | tail -10
# Existing tests should pass; format-pivot tests come in Task 5
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(agent/anchor_state): pivot anchor format to Apple§<literal> via render_anchor_line

Anchor TOKEN (e.g., 'Apple', 'AppleCedar', 'Apple-1') is stored in
AnchorStateManager. Rendered output to the LLM composes
'<token>§<literal line content>' via render_anchor_line — enables
byte-equal validation in EditTool's upcoming 4-step validator.

Token generation gains salt-based 2-word escalation:
  - salt 0 → 'Apple' (60 single words)
  - salt 1 → 'AppleCedar' (60×60 = 3,600 combos)
  - salt > 10,000 → 'AnchorN' fallback (pathological)

Capacity: ~3,660 unique tokens, enough for any realistic file.

Legacy generate_anchor (returning 'Apple§<hash6hex>') kept as
#[deprecated] shim for any external caller.

Inspired by Dirac AnchorStateManager + line-hashing
(src/utils/AnchorStateManager.ts + src/shared/utils/line-hashing.ts)."
```

---

## Task 3: Wire `ReadFileTool` to emit anchored output

- [ ] **Step 3.1: Update `ReadFileTool::execute`**

In `src-tauri/src/agent/tools/builtin/file.rs`, after A3's [File
Hash:] header is emitted but before returning:

```rust
async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let start = std::time::Instant::now();
    let path = params["path"].as_str()
        .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
    let full_path = self.resolve_path(path);

    let content = self.read_file_with_error_mapping(&full_path).await?;
    let current_hash = compute_file_hash(&content);  // from A3
    let header = format!("[File Hash: 0x{:08x}]", current_hash);

    // A3: short-circuit on matching assume_hash
    if let Some(provided) = parse_assume_hash_opt(&params)? {
        if provided == current_hash {
            let msg = format!(
                "{header}\nno changes have been made to the file since your last read (Hash: 0x{:08x})",
                current_hash,
            );
            return Ok(ToolOutput::success(&msg, start.elapsed().as_millis() as u64));
        }
    }

    // B1: render anchored lines
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
        .record_read(&full_path, &lines);

    let mut output = String::with_capacity(content.len() + lines.len() * 16);
    output.push_str(&header);
    output.push('\n');
    for (token, line) in anchors.iter().zip(lines.iter()) {
        output.push_str(&crate::agent::anchor_state::render_anchor_line(token, line));
        output.push('\n');
    }
    // Trim trailing newline if original didn't end with one
    if !content.ends_with('\n') {
        output.pop();
    }

    Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
}
```

- [ ] **Step 3.2: Mark file active in FileContextTracker**

After `record_read`:

```rust
crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER
    .mark_active(&full_path);
```

If `mark_active` doesn't exist on the current `FileContextTracker`,
add it as a 5-line method. Check `anchor_state.rs:170+` for the
existing API.

- [ ] **Step 3.3: Update tool description**

```rust
fn description(&self) -> &str {
    "Read a file. Returns content prefixed with [File Hash: 0x...] header \
     and each line prefixed with a stable anchor token like 'Apple§<line>'. \
     The anchor tokens stay stable across reads for unchanged lines — use them \
     in edit_file's anchor/end_anchor parameters to target edits precisely. \
     Pass the prior [File Hash:] value as assume_hash to short-circuit re-reads."
}
```

- [ ] **Step 3.4: Build + test**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
```

A3 tests will likely fail because output format changed. Update
A3-era tests inline (e.g., assertions on `[File Hash:]` line followed
by content → followed by anchored content):

```rust
// Old (A3):
assert!(out.contains("\nhello world"));
// New (B1):
assert!(out.contains("§hello world"));  // anchor token + § + content
```

Document any A3 test adjustments in the commit message.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/file): emit anchored lines from read_file

After A3's [File Hash:] header, each file line is now prefixed with
its anchor token + ANCHOR_DELIMITER:

    [File Hash: 0xab12cd34]
    Apple§    def process(data):
    Banana§        if data is None:
    Cedar§            return []
    ...

Anchor tokens stay stable across reads for unchanged lines (Myers
diff carry-forward). The LLM can reference any line by its anchor in
subsequent edit_file calls. Marks file active in
GLOBAL_FILE_CONTEXT_TRACKER for stale-file detection.

Adapts A3-era tests for the new output shape."
```

---

## Task 4: Wire `EditTool` to consume anchors (4-step validator + stale check)

- [ ] **Step 4.1: Add `PreconditionFailed` to `ToolErrorKind` if absent**

```rust
// in agent/tools/tool.rs
pub enum ToolErrorKind {
    InvalidParams,
    ResourceNotFound,
    PermissionDenied,
    PreconditionFailed,   // <-- new
    Other,
}
```

- [ ] **Step 4.2: Add `AnchoredEditType` + new `EditArg` variants**

```rust
// in edit.rs
#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum AnchoredEditType {
    Replace,
    InsertAfter,
    InsertBefore,
}
fn default_anchored_edit_type() -> AnchoredEditType { AnchoredEditType::Replace }

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(untagged)]
enum EditArg {
    Anchored {
        anchor: String,
        #[serde(default)]
        end_anchor: Option<String>,
        #[serde(default = "default_anchored_edit_type")]
        edit_type: AnchoredEditType,
        text: String,
    },
    LiteralOrLine {
        #[serde(default)]
        old_text: String,
        new_text: String,
        insert_line: Option<u32>,
    },
}
```

`#[serde(untagged)]` means serde tries variants in order — Anchored
first (more specific, has `anchor` field), then LiteralOrLine. Pin
this order with a test.

- [ ] **Step 4.3: Implement `resolve_anchored_edit`**

```rust
fn resolve_anchored_edit(
    path: &Path,
    current_lines: &[String],
    anchor: &str,
) -> Result<usize, ToolError> {
    // Step 1: format split
    let (token, provided_content) = anchor
        .split_once(crate::agent::anchor_state::ANCHOR_DELIMITER)
        .ok_or_else(|| ToolError::InvalidParams(format!(
            "anchor must contain '{}' delimiter: {:?}",
            crate::agent::anchor_state::ANCHOR_DELIMITER, anchor
        )))?;

    if !is_valid_anchor_token(token) {
        return Err(ToolError::InvalidParams(format!(
            "anchor token {:?} must match ^[A-Z][a-zA-Z]+(-\\d+)?$", token
        )));
    }

    // Step 3: no newline
    if provided_content.contains('\n') {
        return Err(ToolError::InvalidParams(
            "anchor content must be single-line (no '\\n')".into()
        ));
    }

    // Step 2: token exists in current file
    let idx = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
        .resolve_anchor_index(path, token)
        .ok_or_else(|| ToolError::InvalidParams(format!(
            "anchor token '{}' not found in {}. Re-read the file with read_file to refresh anchors.",
            token, path.display()
        )))?;

    // Step 4: byte-equal content
    let actual = current_lines.get(idx).ok_or_else(|| ToolError::InvalidParams(
        format!("anchor '{}' resolved to out-of-range index {} (file has {} lines)",
                token, idx, current_lines.len())
    ))?;
    if actual != provided_content {
        return Err(ToolError::InvalidParams(format!(
            "anchor content mismatch.\n  Expected: {:?}\n  Provided: {:?}\n  Re-read with read_file if you think the file changed.",
            actual, provided_content
        )));
    }

    Ok(idx)
}

fn is_valid_anchor_token(s: &str) -> bool {
    if s.is_empty() { return false; }
    let mut chars = s.chars();
    if !chars.next().unwrap().is_ascii_uppercase() { return false; }
    chars.all(|c| c.is_ascii_alphabetic() || c == '-' || c.is_ascii_digit())
}
```

- [ ] **Step 4.4: Add stale-file precondition + route Anchored edits**

Extend `apply_validated_single_file` (the A2-refactored body):

```rust
async fn apply_validated_single_file(
    &self,
    path: String,
    edits: Vec<EditArg>,
) -> Result<(String, usize), ToolError> {
    let full_path = self.resolve_path(&path);

    // B1: stale-file gate
    if crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
        return Err(ToolError::kinded(
            ToolErrorKind::PreconditionFailed,
            format!(
                "{} was modified externally since last read. Re-read with read_file before editing.",
                full_path.display()
            )
        ));
    }

    let original = tokio::fs::read_to_string(&full_path).await
        .map_err(/* ... */)?;
    let mut lines: Vec<String> = original.lines().map(String::from).collect();

    // Refresh anchor state with current disk content
    let _ = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
        .record_read(&full_path, &lines);

    // Sort Anchored edits by resolved index DESCENDING (matches Dirac
    // EditExecutor.applyEdits — splicing high-to-low keeps lower
    // indices valid).
    let mut applied_count = 0;
    let mut anchored_ops: Vec<(usize, &EditArg)> = Vec::new();
    let mut literal_ops: Vec<&EditArg> = Vec::new();

    for edit in &edits {
        match edit {
            EditArg::Anchored { anchor, .. } => {
                let idx = resolve_anchored_edit(&full_path, &lines, anchor)?;
                anchored_ops.push((idx, edit));
            }
            EditArg::LiteralOrLine { .. } => {
                literal_ops.push(edit);
            }
        }
    }

    // Apply anchored ops descending so splice indices stay valid
    anchored_ops.sort_by(|a, b| b.0.cmp(&a.0));
    for (idx, edit) in anchored_ops {
        if let EditArg::Anchored { end_anchor, edit_type, text, .. } = edit {
            let end_idx = match end_anchor {
                Some(ea) => resolve_anchored_edit(&full_path, &lines, ea)?,
                None => idx,
            };
            apply_anchored_op(&mut lines, idx, end_idx, edit_type, text)?;
            applied_count += 1;
        }
    }

    // Apply literal ops via existing path
    for edit in literal_ops {
        // ... existing literal/insert_line logic ...
        applied_count += 1;
    }

    let modified = lines.join("\n") + if original.ends_with('\n') { "\n" } else { "" };
    let diff = Self::generate_diff(&original, &modified, &path);

    // Mark write as expected so FileContextTracker doesn't flag it stale
    crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER
        .register_expected_write(&full_path);

    tokio::fs::write(&full_path, modified).await
        .map_err(/* ... */)?;

    Ok((diff, applied_count))
}

fn apply_anchored_op(
    lines: &mut Vec<String>,
    start: usize,
    end: usize,
    edit_type: &AnchoredEditType,
    text: &str,
) -> Result<(), ToolError> {
    let new_lines: Vec<String> = text.split('\n').map(String::from).collect();
    match edit_type {
        AnchoredEditType::Replace => {
            lines.splice(start..=end, new_lines);
        }
        AnchoredEditType::InsertAfter => {
            lines.splice(start + 1..start + 1, new_lines);
        }
        AnchoredEditType::InsertBefore => {
            lines.splice(start..start, new_lines);
        }
    }
    Ok(())
}
```

- [ ] **Step 4.5: Update tool description + schema**

```rust
fn description(&self) -> &str {
    "Edit one or more files. Supports three edit shapes per file: \
     (1) anchor-targeted (preferred): {anchor: 'Apple§<line content>', end_anchor?, edit_type, text}, \
     (2) literal search-replace: {old_text, new_text}, \
     (3) line insertion: {old_text: '', new_text, insert_line}. \
     Use the `files` array to batch across files in one call. \
     Anchors come from the read_file output — they stay stable across \
     reads for unchanged lines."
}
```

Schema: add `oneOf` for `EditArg` items (anchored vs literal/line).

- [ ] **Step 4.6: Build + run**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
```

A2 tests should pass (untagged variant fallback keeps literal/line
working). If a test fails because serde picked the wrong variant,
adjust the `untagged` ordering or use explicit tagging.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/edit): anchored edits + 4-step validator + stale-file gate

EditArg gains an Anchored variant: {anchor: 'Apple§<literal line>',
end_anchor?, edit_type, text}. Validator runs 4 checks:
  1. anchor format (token§content split, no newline in content)
  2. token format (^[A-Z][a-zA-Z]+(-\\d+)?$)
  3. token exists in current file (AnchorStateManager lookup)
  4. byte-equal: provided content == lines[idx]

Mismatch → InvalidParams with 'Expected: ..., Provided: ...' message
matching Dirac EditExecutor.resolveAnchor's error wording (LLM
learns to re-read on this error).

Before applying any edit (anchored OR literal), check
FileContextTracker.is_stale → hard-reject with PreconditionFailed if
stale. Register the write as expected so the watcher doesn't flag
our own write as stale.

Anchored ops sorted descending by line index before splicing
(matches Dirac EditExecutor.applyEdits pattern — high-to-low keeps
lower indices stable).

Adds ToolErrorKind::PreconditionFailed.

Inspired by Dirac EditExecutor + FileContextTracker
(src/core/task/tools/handlers/edit-file/EditExecutor.ts:55-169;
src/core/context/context-tracking/FileContextTracker.ts:108-231)."
```

---

## Task 5: Twelve new tests

Split across 3 test modules. List per file:

### `anchor_state::tests` — 7 tests

- [ ] **Step 5.1: `record_read_initializes_first_call`**

```rust
#[test]
fn record_read_initializes_first_call() {
    let mgr = AnchorStateManager::new();
    let lines: Vec<String> = vec!["fn foo() {".into(), "    bar();".into(), "}".into()];
    let anchors = mgr.record_read(Path::new("/tmp/test.rs"), &lines);
    assert_eq!(anchors.len(), 3);
    let unique: std::collections::HashSet<_> = anchors.iter().collect();
    assert_eq!(unique.len(), 3, "anchors must be unique");
}
```

- [ ] **Step 5.2: `record_read_preserves_anchors_on_unchanged_file`**

```rust
#[test]
fn record_read_preserves_anchors_on_unchanged_file() {
    let mgr = AnchorStateManager::new();
    let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let first = mgr.record_read(Path::new("/tmp/u.rs"), &lines);
    let second = mgr.record_read(Path::new("/tmp/u.rs"), &lines);
    assert_eq!(first, second);
}
```

- [ ] **Step 5.3: `record_read_carries_anchors_across_inserted_lines`**

```rust
#[test]
fn record_read_carries_anchors_across_inserted_lines() {
    let mgr = AnchorStateManager::new();
    let p = Path::new("/tmp/i.rs");
    let v1: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let a1 = mgr.record_read(p, &v1);

    let v2: Vec<String> = vec!["a".into(), "NEW1".into(), "NEW2".into(), "b".into(), "c".into()];
    let a2 = mgr.record_read(p, &v2);

    assert_eq!(a2[0], a1[0], "line 'a' keeps token");
    assert_eq!(a2[3], a1[1], "line 'b' keeps token");
    assert_eq!(a2[4], a1[2], "line 'c' keeps token");
    assert_ne!(a2[1], a1[0]);
    assert_ne!(a2[1], a1[1]);
    assert_ne!(a2[1], a1[2]);
}
```

- [ ] **Step 5.4: `record_read_freshens_anchors_for_changed_lines`**

```rust
#[test]
fn record_read_freshens_anchors_for_changed_lines() {
    let mgr = AnchorStateManager::new();
    let p = Path::new("/tmp/c.rs");
    let v1: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let a1 = mgr.record_read(p, &v1);

    let v2: Vec<String> = vec!["a".into(), "MODIFIED".into(), "c".into()];
    let a2 = mgr.record_read(p, &v2);

    assert_eq!(a2[0], a1[0]);
    assert_eq!(a2[2], a1[2]);
    assert_ne!(a2[1], a1[1], "modified line should get a fresh token");
}
```

- [ ] **Step 5.5: `dictionary_capacity_handles_3000_lines`**

```rust
#[test]
fn dictionary_capacity_handles_3000_lines() {
    let lines: Vec<String> = (0..3000).map(|i| format!("line_{i}")).collect();
    let anchors = initialize_anchors(&lines);
    let unique: std::collections::HashSet<_> = anchors.iter().collect();
    assert_eq!(unique.len(), 3000, "must produce 3000 distinct tokens");
}
```

- [ ] **Step 5.6: `generate_anchor_token_pivot`**

```rust
#[test]
fn generate_anchor_token_pivot() {
    // Salt 0 → single word
    let t0 = generate_anchor_token("    def foo():", 0);
    assert!(t0.chars().all(|c| c.is_ascii_alphabetic()));
    assert!(t0.chars().next().unwrap().is_ascii_uppercase());

    // Salt 1 → 2-word combo (uppercase initial of each)
    let t1 = generate_anchor_token("    def foo():", 1);
    let uppers = t1.chars().filter(|c| c.is_ascii_uppercase()).count();
    assert_eq!(uppers, 2, "2-word combo should have 2 capitals");
}
```

- [ ] **Step 5.7: `render_anchor_line_format`**

```rust
#[test]
fn render_anchor_line_format() {
    let out = render_anchor_line("Apple", "    def foo():");
    assert_eq!(out, "Apple§    def foo():");
    let (token, content) = out.split_once(ANCHOR_DELIMITER).unwrap();
    assert_eq!(token, "Apple");
    assert_eq!(content, "    def foo():");
}
```

### `edit::tests` — 3 tests

- [ ] **Step 5.8: `anchored_edit_byte_equal_pass`**

Build a tempdir with a 3-line file. Call read_file → capture anchors.
Call edit with an Anchored variant using the correct anchor.
Assert: returns Ok, file content changed.

- [ ] **Step 5.9: `anchored_edit_byte_mismatch_fails`**

Read → call edit with anchor `Apple§wrong content` when actual is
`Apple§correct content`. Assert: InvalidParams, error message
contains "Expected: " and "Provided: ".

- [ ] **Step 5.10: `edit_tool_rejects_stale_file`**

Read → mark file stale via FileContextTracker (simulate external
mod). Call edit. Assert: `PreconditionFailed` with "modified
externally" in message.

### `file::tests` — 2 tests

- [ ] **Step 5.11: `read_file_emits_anchored_lines`**

Create tempfile with 3 lines. Call read_file. Assert output:
- starts with `[File Hash: 0x`
- next 3 lines match pattern `<token>§<original line>`

- [ ] **Step 5.12: `read_file_anchor_stability_across_reads`**

Read file twice with no modification between. Extract the anchor-
section of each output. Assert byte-equal.

- [ ] **Step 5.13: Run all tests**

```bash
cd src-tauri && cargo test --lib agent::anchor_state 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
# Expect: all pre-existing + 12 new tests passing
```

**Commit checkpoint:**
```
git add -A
git commit -m "test(agent/anchor_state + tools): 12 tests for word-anchor upgrade

anchor_state (7):
  - record_read initializes on first call
  - re-read with unchanged content preserves tokens
  - insertion preserves tokens for unchanged surrounding lines
  - modification freshens token for changed line
  - dictionary capacity handles 3000 distinct lines (2-word escalation)
  - generate_anchor_token salt 0 vs 1 format
  - render_anchor_line composition

edit (3):
  - anchored edit happy path (byte-equal match)
  - anchored edit byte mismatch → InvalidParams with Expected/Provided
  - edit rejects stale file with PreconditionFailed

file (2):
  - read_file emits [File Hash:] + anchored lines
  - re-read produces byte-stable anchor section"
```

---

## Task 6: SSoT + PR

- [ ] **Step 6.1: Update MILESTONE_STATUS**

Under §M2 or §M3 (whichever is the wire-up bucket post-A2/A3
landing):

```
| C2-Dirac-B1 | Word-anchor upgrade — Read emits Apple§<line>, Edit validates byte-equal + stale gate | #<PR-number> |
```

Also update the header `**After PR**:` line.

- [ ] **Step 6.2: Drift + push + PR**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
git push -u origin claude/dirac-b1-word-anchor-upgrade

gh pr create \
  --title "[C2-Dirac-B1] feat(anchor_state + tools): word-anchor upgrade — stable anchors across reads + byte-equal edits + stale gate" \
  --body "..."
```

PR description includes:
- Summary (one paragraph)
- Why (link research doc §7.2 B1 + §2.2 word anchors)
- Commits (bisectable) — 6 commits
- Verification: cargo test tails, bench numbers if available
- Output format change (anchored read output) — flag explicitly for reviewer
- Spec link
- Closes (C2-Dirac-B1)
- **Depends on**: C1-Dirac-A2 + A3 merged (already on main)

- [ ] **Step 6.3: Self-merge gate**

- [ ] CI green
- [ ] PR tag `[C2-Dirac-B1]`
- [ ] Output-format change documented + downstream parsers audited
- [ ] All 4 validator paths covered by tests (Steps 5.8, 5.9 + 2 additional)
- [ ] Stale-file rejection test green

---

## Rollback procedure

```bash
git revert <merge-commit-sha>
git push
```

What's restored: legacy `Apple§a1f89c` anchor format (still
generated, no longer rendered into ReadFileTool output), no anchored
EditTool variant, no stale-file gate. EditTool reverts to A2 shape.
No data corruption.

---

## Closes / unblocks

- C2-Dirac-B1 ✓
- Drives M3 progress (Capability Mesh) ~+5-7%
- Unblocks Phase C2 (AST tools) — `replace_symbol` etc. can adopt
  the same byte-range + validation pattern
- Pairs with A2 / A3 — together = stable + cheap + drift-resistant
  editing across long tasks

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> Run only when invoked by the autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).
>
> ⚠️ **C1 close gate**: orchestrator MUST have verified C1 closed
> (MILESTONE_STATUS shows C1 row closed AND phase-a-closeout PR
> merged) before this task starts. If not → ESCALATE, do not start.

- [ ] **Step A.1: Stage 2 self-verify (per protocol §3.2)**

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::anchor_state 2>&1 | tail -10
cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
cargo clippy --lib -- -D warnings 2>&1 | tail -5

# Scope:
git diff --name-only main..HEAD | sort
# Expected:
#   src-tauri/src/agent/anchor_state.rs
#   src-tauri/src/agent/tools/builtin/edit.rs
#   src-tauri/src/agent/tools/builtin/file.rs
#   src-tauri/src/agent/tools/tool.rs                    (ToolErrorKind::PreconditionFailed)
#   docs/superpowers/MILESTONE_STATUS.md
#   optionally: ui/src/...                                (frontend parser adapter)
# Any extra Rust file → ESCALATE

# Format pivot is real (Tokens stored, not literal-hash strings)
grep -n "format!(.*\"{}§{:06x}\"" src-tauri/src/agent/anchor_state.rs
# Expected: ONLY inside the #[deprecated] generate_anchor shim, nowhere else

# 4-step validator present
grep -n "Expected:\|Provided:\|byte-equal\|anchor.*not found" src-tauri/src/agent/tools/builtin/edit.rs
# Expected: at least 4 hits (one per validator step error path)

# Stale gate is HARD reject
grep -n "PreconditionFailed\|modified externally" src-tauri/src/agent/tools/builtin/edit.rs
# Expected: at least 2 hits (the kinded error + the message)
```

- [ ] **Step A.2: Spawn adversarial reviewer (protocol §3.3)**

B1-specific CRITICAL focus from spec §12:
- Format pivot completeness — TOKEN-only storage in `AnchorStateManager`
- Myers diff carry-forward survived (tests 5.3 + 5.4 prove it)
- Stale-file rejection is `PreconditionFailed` kind, not soft warning
- Downstream parser audit documented in PR body

For HIGH risk PR, reviewer is encouraged to flag even cosmetic
issues as `medium` if they relate to the format pivot.

- [ ] **Step A.3: Reconcile per protocol §3.4** — note: this is a
  HIGH-risk PR, so retry budget is the same but ESCALATE threshold
  is lower (any 2nd reviewer rejection → escalate regardless of
  severity)

- [ ] **Step A.4: PR open + CI + auto-merge (protocol §3.5)**

```bash
git push -u origin claude/dirac-b1-word-anchor-upgrade
PR=$(gh pr create --title "[C2-Dirac-B1] feat(anchor_state + tools): word-anchor upgrade — stable anchors + byte-equal edits + stale gate" --body-file ./pr-body.md --json number -q .number)
gh pr checks $PR --watch --interval 30 --required
gh pr merge $PR --merge --delete-branch
git checkout main && git pull
```

- [ ] **Step A.5: Log + return outcome (protocol §7)**
