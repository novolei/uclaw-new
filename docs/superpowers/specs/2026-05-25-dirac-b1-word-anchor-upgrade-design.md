# Dirac-B1 — Word-Anchor Upgrade for ReadFile/EditTool Pairing (C2)

> **Context**: Phase B item #1 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> Companion plan: [`plans/2026-05-25-dirac-b1-word-anchor-upgrade.md`](../plans/2026-05-25-dirac-b1-word-anchor-upgrade.md).
>
> **C2 SLOT — DO NOT START UNTIL C1 CLOSES**. Per
> [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](2026-05-22-pr-integration-strategy.md) §7,
> C1 (M2 closeout including Phase A A1-A4) must close before any C2 work begins.
> This spec/plan is **prep-only** until C1 closeout report ships.

## 1. Background

### 1.1 What uClaw already has (important — narrower than research doc v1.1 assumed)

`src-tauri/src/agent/anchor_state.rs` already implements most of the
word-anchor machinery. Reading verbatim:

- **Curated word dictionary** (lines 31-40): 60+ words like "Apple",
  "Banana", "Cedar"
- **FNV-1a 32 hash function** (`fnv1a_32`, line 21)
- **Per-line anchor generation** (`generate_anchor`, line 43):
  ```rust
  fn generate_anchor(line: &str) -> String {
      let hash = fnv1a_32(line.trim().as_bytes());
      let word = CURATED_WORDS[(hash % CURATED_WORDS.len() as u32) as usize];
      format!("{}§{:06x}", word, hash & 0xFFFFFF)
  }
  ```
  Format today: **`Apple§a1f89c`** — word + `§` + 6-hex-char content hash.
- **Collision handling** (`initialize_anchors`, lines 52-67): appends
  `-N` suffix on collisions within one file.
- **Myers-diff anchor alignment** (`align_anchors`, lines 70-113):
  uses `similar::capture_diff_slices(Algorithm::Myers, ...)`.
  Unchanged lines carry forward their anchor word; inserted lines
  get fresh anchors.
- **`AnchorStateManager`** (lines 117-167): per-path `Vec<String>`
  anchor storage with `register_file_lines`, `align_file_anchors`,
  `get_anchors`.
- **`FileContextTracker`** (lines 170+): chokidar-equivalent watcher
  via `notify` crate with active/expected/stale set discipline.

### 1.2 The 4 actual gaps

Despite all that machinery, the anchors **are not load-bearing in the
current ReadFile/EditTool pipeline**:

| Gap | Evidence |
|---|---|
| `register_file_lines` can't do true cross-read alignment | `anchor_state.rs:136-140` comment: *"we don't have the old_lines directly, so we either fall back to initializing"* — every read re-initializes from scratch, losing stability |
| `ReadFileTool` (`file.rs:6-58`) emits raw content, no anchors | No call to `AnchorStateManager`; output is plain `fs::read_to_string` |
| `EditTool` (`edit.rs:14`) accepts `anchor` / `end_anchor` params but **doesn't use them** | Schema says "Optional starting anchor for stateful Myers Diff alignment" but `execute_single_file` matches on `old_text` only; anchor fields are dead schema |
| Anchor format is opaque (`Apple§a1f89c`) — LLM can't byte-validate | Dirac format is `Apple§<literal line content>` so `EditExecutor.resolveAnchor` can do 4-step validation including byte-equal match |

### 1.3 Why the gaps weren't filled earlier

`anchor_state.rs` looks like it was authored as a Phase 0.5 / M1 pilot
without being wired through. The comments at lines 136-140 acknowledge
the gap. Phase A's A3 ([File Hash:] short-circuit) addresses
re-read efficiency at the whole-file level; B1 addresses per-line edit
precision.

## 2. Scope

Single PR. Three files heavily touched, two lightly.

### 2.1 In scope

1. **Fix `register_file_lines` to support true cross-read alignment**:
   - Store `last_seen_lines: HashMap<PathBuf, Vec<String>>` alongside
     `file_anchors`.
   - On re-registration, call `align_anchors(old_lines, new_lines,
     old_anchors)` (which already exists at line 70) instead of
     re-initializing.
   - Replace the lines 136-140 fall-through with the proper alignment
     path.

2. **Pivot anchor format from `Apple§a1f89c` to `Apple§<literal line content>`**:
   - Existing format encodes the hash *into* the anchor; the LLM
     can't visually verify "is this anchor pointing at the line I
     think it is".
   - New format: anchor token (`Apple`) carries identity; line
     content (`    def process(data):`) carries verification.
   - The diff-carry-forward property (Myers diff on `old_lines` vs
     `new_lines`) is unchanged — unchanged lines keep their `Apple`
     token, changed lines get fresh tokens.
   - **Backward-compat detail**: `Apple-1`, `Apple-2` suffixes for
     collisions stay (no semantic change there).

3. **Wire `ReadFileTool` to emit anchors in output**:
   - After `fs::read_to_string`, call
     `GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &lines)`
     (new API — composes registering + aligning) → returns
     `Vec<String>` anchors.
   - Output format:
     ```
     [File Hash: 0x<fnv1a32>]    ← A3 already adds this
     Apple§<line 1 content>
     Banana§<line 2 content>
     Cedar-1§<line 3 content>
     ...
     ```
   - The `§` is `ANCHOR_DELIMITER` const (define alongside word list
     or `pub use` from `anchor_state`).

4. **Wire `EditTool` to consume anchors for targeting**:
   - New optional edit shape:
     ```rust
     pub struct AnchoredEdit {
         pub anchor: String,           // "Apple§    def process(data):"
         pub end_anchor: Option<String>, // for ranges
         pub edit_type: AnchoredEditType, // Replace | InsertAfter | InsertBefore
         pub text: String,
     }
     ```
   - Add a 4-step validator matching Dirac
     `EditExecutor.resolveAnchor` (research doc §2.2 step 4):
     1. Anchor format regex: `^[A-Z][a-zA-Z]+(-\d+)?§[^\r\n]*$`
     2. Anchor token (`Apple`) exists in current file's anchor list
     3. No newline in provided line content portion
     4. **Byte-equal**: content after `§` matches `lines[index]` exactly
   - Mismatch → `ToolError::InvalidParams` with `Expected: "<actual>",
     Provided: "<wrong>"` (matches Dirac error wording for LLM
     learnability).
   - Existing `old_text` / `new_text` / `insert_line` path **stays**
     (backward compat, like A2's legacy form).

5. **Expand dictionary capacity**:
   - 60 single words supports ~60 lines (with `-N` collision suffixes
     extending to a few hundred). Real refactor files can have 500+
     lines.
   - Solution: **add 2-word combinations** (`AppleBanana`, `CedarDahlia`)
     as fallback when single-word collision count exceeds threshold.
     60 × 60 = 3,600 unique combos. Matches Dirac's strategy (research
     doc §2.2 layer 1).
   - Don't expand the dictionary file itself yet — 60 words is enough
     to seed 3,600 combos.

6. **Stale-file enforcement on EditTool**:
   - Before applying any anchored edit, check
     `GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&path)`.
   - If stale → hard reject with
     `ToolError::PreconditionFailed("file modified externally since last read; re-read with read_file before editing")`.
   - This is the "**environment as forcing function**" pattern from
     the research doc — Dirac
     `FileContextTracker.detectFilesEditedAfterMessage` is the
     equivalent.

7. **~10 new tests** covering the format pivot, alignment correctness,
   anchor validator, stale-file rejection, and dictionary capacity.

### 2.2 Out of scope

- **AST-aware editing** (`replace_symbol`, `rename_symbol`,
  `get_function`) — Phase C2.
- **Persisting anchor state to disk** — anchors are session-local;
  task restart re-initializes from current file content. Persisting
  is a future M4 World Projection / checkpoint concern.
- **Multi-file anchor batching as ONE operation** — A2 already gave
  us `files: [...]` batching; B1 makes the per-file edit anchored.
  No additional batching changes.
- **Word dictionary file/asset** — keep `CURATED_WORDS` inline.
  Externalizing the wordlist is a future task if larger dictionaries
  become useful.

## 3. Design

### 3.1 New `AnchorStateManager` shape

```rust
#[derive(Default)]
pub struct AnchorStateManager {
    /// Per-file: (last_seen_lines, anchors). Aligned together via
    /// Myers diff on every `record_read`.
    files: Mutex<HashMap<PathBuf, FileAnchorState>>,
}

struct FileAnchorState {
    lines: Vec<String>,        // last-seen content (used for next align)
    anchors: Vec<String>,      // 1:1 with lines
}

impl AnchorStateManager {
    /// Idempotent: record current state of a file's lines and return
    /// the anchors. First call initializes; subsequent calls align
    /// via Myers diff (`align_anchors(old_lines, new_lines, old_anchors)`).
    pub fn record_read(&self, path: &Path, lines: &[String]) -> Vec<String> {
        let mut files = self.files.lock().unwrap();
        let entry = files.entry(path.to_path_buf());
        let anchors = match entry {
            Entry::Vacant(slot) => {
                let init = initialize_anchors(lines);
                slot.insert(FileAnchorState {
                    lines: lines.to_vec(),
                    anchors: init.clone(),
                });
                init
            }
            Entry::Occupied(mut slot) => {
                let prev = slot.get();
                let aligned = align_anchors(&prev.lines, lines, &prev.anchors);
                slot.insert(FileAnchorState {
                    lines: lines.to_vec(),
                    anchors: aligned.clone(),
                });
                aligned
            }
        };
        anchors
    }

    /// Look up the line index of an anchor in a file's CURRENT anchor list.
    /// Returns None if not found or path not tracked. Used by EditTool's
    /// 4-step validator.
    pub fn resolve_anchor_index(&self, path: &Path, anchor_token: &str) -> Option<usize> {
        let files = self.files.lock().unwrap();
        let state = files.get(path)?;
        state.anchors.iter().position(|a| a == anchor_token)
    }

    /// Get a (line, anchor) snapshot at a given index — used by the
    /// EditTool validator's "Expected: ..., Provided: ..." error path.
    pub fn snapshot_line(&self, path: &Path, idx: usize) -> Option<(String, String)> {
        let files = self.files.lock().unwrap();
        let state = files.get(path)?;
        Some((state.lines.get(idx)?.clone(), state.anchors.get(idx)?.clone()))
    }

    // Existing register_file_lines / align_file_anchors kept as deprecated
    // shims that forward to record_read. Mark with #[deprecated(note=...)]
    // so any existing caller gets a compiler nudge to migrate.
}
```

### 3.2 The new format: `Apple§<literal line content>`

Today (`anchor_state.rs:48`):
```rust
format!("{}§{:06x}", word, hash & 0xFFFFFF)
// → "Apple§a1f89c"
```

After B1:
```rust
// generate_anchor still returns just the TOKEN portion ("Apple",
// "Banana", etc.) — no literal content embedded yet.
fn generate_anchor_token(line: &str) -> String {
    let hash = fnv1a_32(line.trim().as_bytes());
    let word = CURATED_WORDS[(hash % CURATED_WORDS.len() as u32) as usize];
    word.to_string() // → "Apple"
}

// The full anchor (token + § + literal) is composed at render time,
// not stored. AnchorStateManager stores tokens only.
fn render_anchor_line(token: &str, content: &str) -> String {
    format!("{}{}{}", token, ANCHOR_DELIMITER, content)
}

pub const ANCHOR_DELIMITER: char = '§';
```

This way:
- The **stored anchor** in `AnchorStateManager` is just `Apple` (or
  `Apple-1`)
- The **rendered output** to the LLM is `Apple§    def process(data):`
- The **diff property** (Myers carry-forward) operates on the *token*,
  so a function whose content didn't change keeps `Apple` even after
  surrounding edits
- The **byte-equal validation** in `EditTool` checks the *literal
  content portion* against the current `lines[idx]`

### 3.3 Dictionary capacity (single → 2-word combos)

```rust
const CURATED_WORDS: &[&str] = &[ /* 60+ existing words */ ];

/// Generate anchor token. For files with <= 60 distinct lines, returns
/// a single word like "Apple". For larger files (or after collisions),
/// returns a 2-word combo like "AppleCedar".
fn generate_anchor_token(line: &str, salt: u64) -> String {
    let hash = fnv1a_32(line.trim().as_bytes()) as u64 ^ salt;
    let n = CURATED_WORDS.len() as u64;
    let first = CURATED_WORDS[(hash % n) as usize];

    // Salt > 0 means we're handling a collision; emit 2-word combo
    if salt > 0 {
        let second = CURATED_WORDS[((hash / n) % n) as usize];
        format!("{first}{second}")  // "AppleCedar"
    } else {
        first.to_string()  // "Apple"
    }
}
```

`initialize_anchors` then escalates from single-word to 2-word on
collision:

```rust
pub fn initialize_anchors(lines: &[String]) -> Vec<String> {
    let mut anchors = Vec::with_capacity(lines.len());
    let mut seen: HashSet<String> = HashSet::new();

    for line in lines {
        let mut salt = 0u64;
        loop {
            let candidate = generate_anchor_token(line, salt);
            if !seen.contains(&candidate) {
                seen.insert(candidate.clone());
                anchors.push(candidate);
                break;
            }
            salt += 1;
            if salt > 10_000 {
                // Pathological — fall back to numbered suffix
                anchors.push(format!("Anchor{}", anchors.len()));
                break;
            }
        }
    }
    anchors
}
```

Capacity: 60 single + 60×60 combos = **3,660 unique tokens**, enough
for any realistic uClaw edit target.

### 3.4 New EditTool shape (additive to A2)

A2 left us with two shapes (legacy `{path, edits}` + batch `{files}`).
B1 adds **anchored edits** as a new edit-object variant:

```rust
#[derive(serde::Deserialize, Debug, Clone)]
#[serde(untagged)]
enum EditArg {
    /// Existing — search/replace by literal text or line number insert
    LiteralOrLine {
        #[serde(default)]
        old_text: String,
        new_text: String,
        insert_line: Option<u32>,
    },
    /// NEW — anchor-targeted edit
    Anchored {
        anchor: String,           // "Apple§    def process(data):"
        end_anchor: Option<String>,
        #[serde(default = "default_edit_type")]
        edit_type: AnchoredEditType,
        text: String,
    },
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum AnchoredEditType {
    Replace,
    InsertAfter,
    InsertBefore,
}
fn default_edit_type() -> AnchoredEditType { AnchoredEditType::Replace }
```

Routing in `apply_validated_single_file`:
- LiteralOrLine → existing path (no change to A2 implementation)
- Anchored → new path: 4-step validator → byte-range computation →
  splice

### 3.5 The 4-step Anchored validator

```rust
fn resolve_anchored_edit(
    path: &Path,
    current_lines: &[String],
    edit: &AnchoredEditArg,
) -> Result<usize, ToolError> {
    // Step 1: format
    let (token, provided_content) = edit.anchor.split_once(ANCHOR_DELIMITER)
        .ok_or_else(|| ToolError::InvalidParams(format!(
            "anchor must contain '{}' delimiter: {:?}", ANCHOR_DELIMITER, edit.anchor
        )))?;
    if !is_valid_anchor_token(token) {
        return Err(ToolError::InvalidParams(format!(
            "anchor token {:?} must match ^[A-Z][a-zA-Z]+(-\\d+)?$", token
        )));
    }

    // Step 2: anchor exists in current file
    let idx = GLOBAL_ANCHOR_STATE_MANAGER
        .resolve_anchor_index(path, token)
        .ok_or_else(|| ToolError::InvalidParams(format!(
            "anchor '{}' not found in {}. Re-read the file with read_file to get the latest anchors.",
            token, path.display()
        )))?;

    // Step 3: no newline in provided content
    if provided_content.contains('\n') {
        return Err(ToolError::InvalidParams(
            "anchor content must be single-line (no \\n)".into()
        ));
    }

    // Step 4: byte-equal
    let actual = &current_lines[idx];
    if actual != provided_content {
        return Err(ToolError::InvalidParams(format!(
            "anchor content mismatch.\n  Expected: {:?}\n  Provided: {:?}",
            actual, provided_content
        )));
    }

    Ok(idx)
}
```

All 4 failure paths return `InvalidParams` with structured, LLM-
actionable messages.

### 3.6 Stale-file enforcement

In `apply_validated_single_file` (the A2-refactored body), before any
file mutation:

```rust
if GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
    return Err(ToolError::kinded(
        ToolErrorKind::PreconditionFailed,  // add this variant if absent
        format!(
            "{} was modified externally since last read. Re-read with read_file before editing.",
            full_path.display()
        )
    ));
}
```

After successful apply, register the write as expected so the watcher
doesn't flag it stale (uClaw's `FileContextTracker` already has
`expected_writes` set — line 172).

## 4. Interfaces

### 4.1 Public additions

```rust
// anchor_state.rs
pub const ANCHOR_DELIMITER: char = '§';
pub fn render_anchor_line(token: &str, content: &str) -> String;
impl AnchorStateManager {
    pub fn record_read(&self, path: &Path, lines: &[String]) -> Vec<String>;
    pub fn resolve_anchor_index(&self, path: &Path, token: &str) -> Option<usize>;
    pub fn snapshot_line(&self, path: &Path, idx: usize) -> Option<(String, String)>;
}
```

### 4.2 Deprecated (but kept) APIs

- `register_file_lines` → shim that forwards to `record_read`,
  `#[deprecated]` annotation
- `align_file_anchors` → kept (already correct), but real callers
  should migrate to `record_read`
- `generate_anchor` (returning `Apple§a1f89c`) → kept for any external
  caller, but internal code uses `generate_anchor_token`

### 4.3 ReadFileTool output format change

This **IS a behavior change** for any downstream parser of read_file
results. Combined with A3's `[File Hash:]` prefix, the new output is:

```
[File Hash: 0xab12cd34]
Apple§    def process(data):
Banana§        if data is None:
Cedar§            return []
DahliaElm§        result = []
Fern§        for item in data:
Ginkgo§            result.append(item * 2)
Hazel§        return result
```

Compared to today's plain file content. Audit downstream parsers
during pre-flight (see plan Step 0.3).

### 4.4 ToolErrorKind extension

Add `PreconditionFailed` variant if not already present. Used for
stale-file rejection. Coordinate with `safety::SafetyManager` error
taxonomy if it exists.

## 5. Tests

Inline in `agent::anchor_state::tests` + `agent::tools::builtin::edit::tests`
+ `agent::tools::builtin::file::tests`.

| # | Test | Scenario |
|---|---|---|
| 1 | `record_read_initializes_first_call` | First `record_read` → returns N anchors, all unique |
| 2 | `record_read_preserves_anchors_on_unchanged_file` | Read same file twice → anchors identical |
| 3 | `record_read_carries_anchors_across_inserted_lines` | Read file → insert 5 lines in middle → re-read → unchanged surrounding lines keep their anchor tokens |
| 4 | `record_read_freshens_anchors_for_changed_lines` | Read → modify line 10 → re-read → line 10 gets new token, others unchanged |
| 5 | `dictionary_capacity_3600_lines` | 3,000 distinct lines → 3,000 distinct anchors (no exhaustion) |
| 6 | `anchor_validator_byte_equal_pass` | Edit with correct anchor → returns Ok(idx) |
| 7 | `anchor_validator_token_not_found` | Edit with token "Foo" not in file → InvalidParams with "re-read" hint |
| 8 | `anchor_validator_byte_mismatch` | Edit anchor `Apple§wrong content` when line is `Apple§correct` → InvalidParams with "Expected: ..., Provided: ..." |
| 9 | `anchor_validator_no_newline_in_anchor` | Edit anchor containing `\n` → InvalidParams |
| 10 | `edit_tool_rejects_stale_file` | Mark file stale via FileContextTracker → edit → PreconditionFailed |
| 11 | `read_file_emits_anchor_per_line` | Read file → output has N anchored lines after [File Hash:] header |
| 12 | `read_file_anchor_stability_across_reads` | Read → re-read → byte-identical anchor section (line-by-line equality) |

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::anchor_state 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo clippy --lib -- -D warnings | tail -5
```

### 6.2 Integration bench (50-turn refactor task)

Run a fixture session where the LLM edits 8 files across 20+ turns.
Compare pre-B1 (literal `old_text` matching) vs post-B1 (anchor-targeted):

| Metric | Pre-B1 | Post-B1 (expected) |
|---|---|---|
| `old_text` mismatch errors per task | ~10-20 | ~0 (anchor token can't drift) |
| Total tokens / task | baseline | -15% to -25% (anchors stable across reads → less re-read churn) |
| Edit-then-rewrite cycles (LLM re-reads to recompute line nums) | 3-6 | 0-1 |

The 4 bench metrics above are the spec-promised B1 outcomes.

### 6.3 Format-pivot regression

```bash
cd src-tauri && cargo test --lib agent::anchor_state::tests::format_pivot
# Expect tests asserting:
#   generate_anchor_token("    def foo():") -> "Apple" (or whatever the FNV picks; pin in test)
#   render_anchor_line("Apple", "    def foo():") -> "Apple§    def foo():"
```

## 7. Migration / rollback

- **DB migration**: none. Anchors are session-local (in-memory).
- **Backward compat for callers of `register_file_lines` /
  `generate_anchor`**: kept via deprecation shims; existing callsites
  continue to compile and behave identically (legacy returns the old
  opaque `Apple§a1f89c` format because nothing downstream consumed
  it).
- **ReadFileTool output change**: **IS a contract change**. Audit
  during pre-flight (plan Step 0.3) and update any parser found.
- **EditTool legacy `old_text`/`new_text` shape**: unchanged.
  Anchored shape is additive.
- **Rollback**: revert PR. Anchor state still tracks; just no longer
  emitted from ReadFileTool and not validated by EditTool. Same as
  pre-B1 steady state. No data corruption.

## 8. Decisions (locked 2026-05-25)

### 8.1 Format pivot: `Apple§<literal>` not `Apple§<hash6hex>`

- **Why**: literal content portion enables byte-equal validation
  (the 4-step `EditExecutor.resolveAnchor` pattern from Dirac). With
  opaque hash, the LLM can't visually verify it's pointing at the
  right line.
- **Cost**: line content adds tokens per anchor line. 10-char anchor
  + 80-char content = 90 char/line vs 16 char/line today. **But**:
  the read tool was always emitting the full line content anyway —
  before B1 it was just `<line>`; after B1 it's `Apple§<line>`. So
  only ~10 extra tokens per line, not "doubled".

### 8.2 Tokens stored, literals composed at render time

- `AnchorStateManager` stores `Vec<String>` of TOKENS only (`Apple`,
  `Apple-1`, `BananaCedar`). The `§<literal>` part is composed by
  `render_anchor_line` at the point of emission.
- **Why**: keeps manager memory small; lets `align_anchors` operate
  on opaque tokens without recomputing on long line content.

### 8.3 2-word combo escalation, not bigger flat dictionary

- **Why**: easier to reason about (60 base words × 2 stages); easy
  to extend to 3-word later (3,600 × 60 = 216K) if needed without
  re-shuffling allocation.
- Same strategy as Dirac (research doc §2.2).

### 8.4 Stale-file edit rejection is hard, not soft warning

- **Why**: matches the "environment as forcing function" thesis. A
  soft warning would still let the LLM apply edits to stale content
  and corrupt state. Hard reject + structured error message =
  forced re-read → guaranteed correct edit.
- **Trade-off accepted**: extra round-trip for the re-read. Worth it
  for the 0% silent-corruption guarantee.

### 8.5 No persistent anchor storage

- **Why**: anchors are derived from current file content; persisting
  them adds a stale-data failure mode without saving meaningful
  work (re-initialization is cheap — FNV1a32 is fast). Future
  checkpointing could persist if needed.

### 8.6 Keep legacy `generate_anchor` and `register_file_lines`

- **Why**: deprecation, not removal. External callers (or future
  rollout-replay tools) may reference them. Compiler nudge via
  `#[deprecated]` is enough; clean break is unnecessary risk.

## 9. Concrete commit plan

```
Commit 1: refactor(anchor_state): introduce FileAnchorState + record_read API
          (+ deprecate register_file_lines as shim; behavior change isolated)
Commit 2: feat(anchor_state): generate_anchor_token + 2-word combo escalation
          + render_anchor_line helper + ANCHOR_DELIMITER const
Commit 3: feat(tools/builtin/file): emit anchored lines from read_file
          (output: [File Hash:] header from A3 + anchor-prefixed lines)
Commit 4: feat(tools/builtin/edit): accept AnchoredEdit variant + 4-step validator
          + stale-file precondition check
Commit 5: test(anchor_state + tools): 12 new tests covering record_read alignment,
          dictionary capacity, validator paths, stale rejection, format pivot
Commit 6: docs(MILESTONE_STATUS): record C2-Dirac-B1 completion
```

Six commits, ~700-900 lines of diff. Bisectable.

## 10. Estimated effort

- AnchorStateManager refactor: 0.5 day
- generate_anchor_token + 2-word escalation: 0.5 day
- ReadFileTool wiring + downstream parser audit: 0.5 day
- EditTool AnchoredEdit + validator + stale check: 0.75 day
- Tests: 1 day
- Bench (optional, recommended): 0.5 day
- **Total: 3 days** (matches research doc estimate)

## 11. Closes / unblocks

- C2-Dirac-B1 ✓
- Drives M3 progress (Capability Mesh) ~+5-7% (Edit/Read tools gain
  "capability card" reliability metadata when anchored)
- Unblocks Phase C2 (AST tools) — `replace_symbol` can layer on top
  of anchored edits since AST symbols also use the byte-range +
  validation pattern
- Pairs strongly with A2 (multi-file batch) and A3 ([File Hash:]
  short-circuit) — together these three give an LLM a stable, cheap,
  drift-resistant editing surface across long tasks

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)):

- **C1→C2 boundary check** (Stage 1 pre-flight): orchestrator MUST
  verify MILESTONE_STATUS.md shows C1 closed (A1-A4 merged + C1
  closeout PR merged). If C1 not closed → escalate, do not start.
- **Format pivot is load-bearing** (Stage 3 critical focus): the
  anchor format change (`Apple§<hash>` → `Apple§<literal content>`)
  is the WHOLE POINT of B1. Reviewer must verify:
  - `AnchorStateManager` stores TOKENS only (`Apple`, `AppleCedar`,
    `Apple-1`), NOT the legacy `Apple§<hash>` strings
  - `render_anchor_line(token, content)` is the composition point
  - ReadFileTool emits `<token>§<literal>` per line
  - EditTool's 4-step validator splits on `ANCHOR_DELIMITER` and
    byte-compares the literal portion
- **Diff carry-forward is the safety property** (Stage 3): tests
  5.3 (carries across inserted lines) and 5.4 (freshens for
  changed lines) prove the Myers diff layer survived the refactor.
  Reviewer reads both carefully — these are the regression checks
  for the algorithm.
- **Stale-file gate is HARD reject** (Stage 3): spec §8.4 locked
  decision. Reviewer verifies `is_stale` check → `PreconditionFailed`,
  NOT a soft warning. Test 5.10 enforces; reviewer confirms it
  asserts the kind, not just any error.
- **Downstream parser audit done** (Stage 1 pre-flight): plan Step
  0.3 audits any code that parses ReadFileTool output for raw
  content. Audit result logged in PR description.
- **Risk class**: HIGH — largest PR in Phase A/B (3 days, 6
  commits), format pivot affects every downstream consumer, EditTool
  semantics change. The reviewer should default to extra scrutiny
  on this one; allow `medium` REQUEST_CHANGES even on cosmetic
  drift.
