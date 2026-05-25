# Dirac-A2 — Multi-File Edit Schema Extension (C1)

> **Context**: Phase A item #2 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> Companion plan: [`plans/2026-05-25-dirac-a2-multifile-edit-schema.md`](../plans/2026-05-25-dirac-a2-multifile-edit-schema.md).
> **C1 slot**: M2 closeout. Highest single-PR ROI in Phase A (predicted
> 30-50% reduction in LLM round-trip count on refactor tasks).
> Independent of A1 — A2 can ship in any order relative to A1.

## 1. Background

`src-tauri/src/agent/tools/builtin/edit.rs::EditTool` ships a tool with
this schema (`parameters_schema` lines 89-111):

```json
{
  "path": "<one file>",
  "edits": [{ "old_text", "new_text", "insert_line", "anchor", "end_anchor" }, ...]
}
```

The tool already supports multiple **edits** per call, but only against
**one file**. The model must emit N tool calls to edit N files.

**Cost analysis for an 8-file refactor** (representative — Dirac eval
`refactor_DynamicCache`):

| Architecture | tool calls | LLM round-trips | prompt-cache writes |
|---|---|---|---|
| Single-file edit only (uClaw today) | 8 | 8 | 8 |
| Multi-file edit (Dirac, A2 target) | 1 | 1 | 1 |

Per Dirac's published eval (research doc §0), this single schema change
contributes the bulk of the 2.8× cost reduction over Cline (the
Cline-Dirac diff is dominated by the tool batching change).

**Dirac's schema** (reverse-engineered from
`/Users/ryanliu/Documents/dirac/src/core/prompts/system-prompt/tools/edit_file.ts`,
verified at lines 29-77):

```json
{
  "files": [
    {
      "path": "<file path>",
      "edits": [
        {
          "edit_type": "replace" | "insert_after" | "insert_before",
          "anchor": "<anchor_word>§<line content>",
          "end_anchor": "<anchor_word>§<line content>",
          "text": "<replacement>"
        }
      ]
    }
  ]
}
```

This spec borrows the **`files: [...]` outer wrapping** and the
**sequential-dependency semantics** (rejecting file N skips N+1..end).
It does NOT borrow the anchor format yet — that's A2-out-of-scope and
ships in Phase B1 (word-anchor upgrade). For A2, the existing
`old_text` / `new_text` / `anchor` (line-hash) primitives stay.

## 2. Scope

Single PR. One subsystem (`agent::tools::builtin::edit`). One Tauri
backward-compat shim.

### 2.1 In scope

1. Extend `EditTool::parameters_schema()` to accept **both**:
   - **Legacy form** (unchanged): `{path, edits: [...]}` — fully
     supported, no deprecation.
   - **Batch form** (new): `{files: [{path, edits: [...]}, ...]}` —
     applies edits per file in array order.
2. Add `EditTool::execute()` dispatch: detect which shape and route.
3. Implement **sequential dependency**: if file N fails (validation
   error, write error, user rejection in approval flow), skip files
   N+1..end with structured error `"Skipped due to failure on prior file in batch"`.
4. Implement **cross-file diff preview**: when approval is required,
   show all files' diffs in one preview pass before applying anything
   (transactional approval, atomic application). Spec accepts either
   approach if the existing approval infra makes one easier — favor
   parallel approval over per-file iteration to match Dirac's fast path.
5. Update tool description string to encourage batching:
   > "Edit one or more files. Use the `files` array to batch edits
   > across files in a single call — reduces LLM round-trips."
6. ~8 new unit tests covering both shapes + sequential-dependency.

### 2.2 Out of scope

- **Word anchors** (Dirac `AppleBanana§` style) — Phase B1.
- **AST-based replace_symbol** — Phase C2.
- **Multi-file `read_file` / `write_file`** — separate future work; A3
  is multi-file *flavor* but for read, not edit.
- Schema versioning / explicit version field — both shapes coexist via
  property detection (`files` present → batch, else legacy). Simple
  and JSON-Schema validatable.
- LLM tool description rewording beyond the one sentence above — full
  prompt-engineering pass deferred to A4 work.

## 3. Design

### 3.1 Detect-and-dispatch in `execute()`

```rust
async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    if let Some(files_val) = params.get("files") {
        // Batch form
        let files: Vec<FileEditsArg> = serde_json::from_value(files_val.clone())
            .map_err(|e| ToolError::InvalidParams(format!("`files` shape error: {e}")))?;
        self.execute_batch(files).await
    } else if params.get("path").is_some() {
        // Legacy form — delegate to existing execute_single_file
        let legacy: SingleFileEditsArg = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        self.execute_single_file(legacy).await
    } else {
        Err(ToolError::InvalidParams(
            "either `files: [{path, edits}, ...]` or `{path, edits}` required".into()
        ))
    }
}
```

`execute_single_file` is the existing body, refactored into a private
method. **Zero behavior change** for legacy callers.

### 3.2 Batch execution semantics

```rust
async fn execute_batch(&self, files: Vec<FileEditsArg>) -> Result<ToolOutput, ToolError> {
    let mut per_file_results: Vec<FileBatchResult> = Vec::with_capacity(files.len());
    let mut first_failure_idx: Option<usize> = None;

    // Phase 1: validate ALL inputs (paths exist, anchors resolve, etc.)
    //          — no file system writes yet
    let validations = self.validate_batch(&files).await;

    if validations.iter().any(|v| v.is_err()) {
        // Build per-file result vector with skip-after-failure semantics
        for (i, v) in validations.into_iter().enumerate() {
            match v {
                Err(e) if first_failure_idx.is_none() => {
                    first_failure_idx = Some(i);
                    per_file_results.push(FileBatchResult::ValidationFailed { path: files[i].path.clone(), error: e });
                }
                _ if first_failure_idx.is_some() => {
                    per_file_results.push(FileBatchResult::Skipped {
                        path: files[i].path.clone(),
                        reason: "Skipped due to failure on prior file in batch".into(),
                    });
                }
                Ok(_) => per_file_results.push(FileBatchResult::Validated { path: files[i].path.clone() }),
            }
        }
        return Ok(ToolOutput::from_batch_results(per_file_results, /* applied = */ 0));
    }

    // Phase 2: apply each file in order. First failure halts.
    for (i, file_edits) in files.into_iter().enumerate() {
        if first_failure_idx.is_some() {
            per_file_results.push(FileBatchResult::Skipped {
                path: file_edits.path,
                reason: "Skipped due to failure on prior file in batch".into(),
            });
            continue;
        }
        match self.apply_single_file(file_edits.path.clone(), file_edits.edits).await {
            Ok(diff) => per_file_results.push(FileBatchResult::Applied { path: file_edits.path, diff }),
            Err(e) => {
                first_failure_idx = Some(i);
                per_file_results.push(FileBatchResult::ApplicationFailed { path: file_edits.path, error: e });
            }
        }
    }

    Ok(ToolOutput::from_batch_results(per_file_results, first_failure_idx.map(|i| i).unwrap_or(files.len())))
}
```

**Two-phase (validate-all-then-apply)** matches Dirac
`BatchProcessor.executeMultiFileBatch` lines 156-166 (pre-flight) + 169-206
(apply). It lets the entire batch reject before any disk write.

### 3.3 Approval flow integration

Existing `SafetyManager` integration calls `requires_approval` per tool
call. With batching, one tool call covers N files. Two options:

- **Option X** (recommended): preview ALL diffs in a single approval
  prompt; one accept/reject for the whole batch. UI shows tabbed diff
  per file.
- **Option Y**: per-file approval mid-execution; rejecting file N halts
  batch (matches Dirac `BatchProcessor.executeMultiFileBatch:208-290`).

**Decision**: ship Option X for A2 — simpler UI, smaller PR. If user
feedback shows per-file approval is needed (e.g., "the first 3 files
were fine but I want to review the 4th separately"), upgrade to
Option Y in a follow-up. Decision recorded §8.2.

### 3.4 Output formatting

`ToolOutput::from_batch_results(results, applied_count)` produces:

```
Applied edits to 5 files (3 succeeded, 1 failed, 1 skipped):

✓ src/foo.rs: 12 edits applied
✓ src/bar.rs: 3 edits applied
✓ src/baz.rs: 1 edit applied
✗ src/quux.rs: anchor "L4231abcd" not found (re-read the file)
- src/zap.rs: Skipped due to failure on prior file in batch

[diff per file]
--- src/foo.rs
+++ src/foo.rs
@@ ...
...
```

LLM sees the per-file outcome + diffs in one tool result, can fix
errors and re-batch without round-tripping per file.

### 3.5 Path arguments for `SafetyManager`

Existing trait method:

```rust
fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
    args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
}
```

Extend to handle batch form:

```rust
fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
    if let Some(files) = args.get("files").and_then(|f| f.as_array()) {
        files.iter()
            .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
            .collect()
    } else {
        args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
    }
}
```

`SafetyManager::check_paths` thus sees all N paths and can enforce
path policy on the whole batch.

## 4. Interfaces

### 4.1 Schema (in `parameters_schema`)

```json
{
  "type": "object",
  "oneOf": [
    {
      "title": "Batch form (preferred for multi-file edits)",
      "properties": {
        "files": {
          "type": "array",
          "minItems": 1,
          "description": "Files to edit. Applied in order; first failure skips remaining.",
          "items": {
            "type": "object",
            "properties": {
              "path": {"type": "string"},
              "edits": { "$ref": "#/definitions/edit_array" }
            },
            "required": ["path", "edits"]
          }
        }
      },
      "required": ["files"]
    },
    {
      "title": "Legacy single-file form (backward compat)",
      "properties": {
        "path": {"type": "string"},
        "edits": { "$ref": "#/definitions/edit_array" }
      },
      "required": ["path", "edits"]
    }
  ],
  "definitions": {
    "edit_array": {
      "type": "array",
      "items": { /* unchanged edit object schema */ }
    }
  }
}
```

> **Note on oneOf**: Some LLM providers (older OpenAI tool-use) don't
> handle JSON Schema `oneOf` at the top level cleanly. **Fallback**: if
> JSON-Schema-strict providers reject the `oneOf` shape, ship a
> looser schema with both `files` and `path` as optional properties +
> a one-line description: *"Use `files` for multi-file edits OR
> `{path, edits}` for single file"*. Test against Anthropic + OpenAI
> + Gemini before final commit; record what works.

### 4.2 Rust types

```rust
#[derive(Deserialize)]
struct FileEditsArg {
    path: String,
    edits: Vec<EditArg>,
}

#[derive(Deserialize)]
struct SingleFileEditsArg {
    path: String,
    edits: Vec<EditArg>,
}

#[derive(Deserialize)]
struct EditArg {
    #[serde(default)]
    old_text: String,
    new_text: String,
    insert_line: Option<u32>,
    anchor: Option<String>,
    end_anchor: Option<String>,
}

enum FileBatchResult {
    Applied { path: String, diff: String },
    ValidationFailed { path: String, error: String },
    ApplicationFailed { path: String, error: String },
    Skipped { path: String, reason: String },
    Validated { path: String }, // intermediate
}
```

## 5. Tests

Eight new tests, in `agent::tools::builtin::edit` test module.

| # | Test | Scenario |
|---|---|---|
| 1 | `..._legacy_single_file_unchanged` | Send `{path, edits}` — verify identical output to pre-PR behavior |
| 2 | `..._batch_two_files_both_succeed` | `{files: [a, b]}`, both apply, output shows both diffs |
| 3 | `..._batch_first_file_validation_fails` | File a anchor not found → file b skipped, output shows skip reason |
| 4 | `..._batch_first_file_apply_fails` | File a path forbidden by path policy → file b skipped |
| 5 | `..._batch_middle_file_fails` | Files a,b,c — b's anchor invalid → c skipped, a applied |
| 6 | `..._path_args_collects_all_batch_paths` | `path_args(&args)` returns all N paths |
| 7 | `..._batch_neither_files_nor_path` | Empty params → InvalidParams error with helpful message |
| 8 | `..._batch_atomic_validate_then_apply` | Two-file batch where file b fails validation → file a not touched on disk (no partial write) |

Test fixtures use `tempdir()` for workspace_root and write source
files before testing.

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
# Expect: existing + 8 new tests passing

cd src-tauri && cargo build 2>&1 | grep -E "^error" | head    # empty
cd src-tauri && cargo clippy --lib -- -D warnings | tail -5    # clean
```

### 6.2 LLM integration (manual)

```bash
# Set provider to Anthropic
# Open a uClaw session in a fixture repo with 3+ files
# Prompt: "Rename function `foo` to `bar` across all files using a
#          single edit_file tool call"
# Expect: ONE tool call from the LLM with files: [a, b, c]
# Expect: ONE tool_result back with all 3 diffs
# Expect: rollout.jsonl shows N_tool_calls reduced vs pre-PR baseline
```

### 6.3 Round-trip count bench

Optional but high-value:

```
Fixture: examples/refactor-bench/3-file-rename/
Pre-PR: ~6 LLM round-trips
Post-PR: ~1-2 LLM round-trips
Track in M2 closeout report.
```

Defer to follow-up if bench harness not ready.

## 7. Migration / rollback

- **DB migration**: none.
- **Backward compat**: legacy `{path, edits}` shape fully supported. Any
  saved session / rollout / existing LLM prompt that emits the legacy
  shape continues working. **No deprecation.**
- **Rollback**: revert PR. Both shapes disappear; legacy single-file
  remains.
- **Feature flag**: not needed. Schema is additive; new shape only
  triggers when LLM emits `files: [...]`. Existing LLM behavior
  (single-file calls) unchanged.

## 8. Decisions (locked 2026-05-25)

### 8.1 Keep legacy `{path, edits}` shape forever (no deprecation)

- **Decision**: legacy form is permanent, not deprecated.
- **Why**: zero deprecation cost (the detect-and-dispatch is 5 lines).
  Removing it later would break replay of old sessions and confuse
  LLMs that learned the legacy shape from training data. The cost of
  keeping it is negligible.

### 8.2 Whole-batch approval, not per-file (Option X)

- **Decision**: approval gate is one accept/reject per batch.
- **Why**: simpler UI, smaller PR, matches Dirac's `backgroundEditEnabled`
  fast path. Per-file approval (Option Y from §3.3) ships only if
  user feedback demands it.
- **UI implication**: settings page may need a "preview all N file
  diffs before approving" affordance — out of scope for A2 backend
  PR; tracked as follow-up `[C1.X-frontend]`.

### 8.3 `oneOf` schema with fallback

- **Decision**: ship `oneOf` first; if any active provider rejects it,
  fall back to loose schema in §4.1 fallback note.
- **Why**: `oneOf` produces tighter LLM behavior (model knows the two
  shapes are mutually exclusive). Tested at PR time against current
  4 providers. Document the result in commit message.

### 8.4 First-failure-skips-rest, no try-all-then-report

- **Decision**: stop at first failure (validation or application);
  skip remaining with structured "Skipped" result.
- **Why**: matches Dirac's `BatchProcessor.executeMultiFileBatch:250-259`
  semantics. The user-mental-model is "this is one logical
  operation"; partial application is harder to reason about and
  rollback. The LLM gets enough info in the result to fix the first
  failure and re-batch.
- **Trade-off accepted**: if files a,b,c are independent edits and b
  fails for a transient reason, c is unnecessarily skipped. Mitigation:
  the result message is clear; LLM can re-batch [c] alone.

### 8.5 Tool description sentence: "use files for multi-file"

- **Decision**: update tool description to ONE sentence promoting
  batch form, no other prompt changes.
- **Why**: prompt-engineering changes are explicitly Phase A4. This is
  the minimum the LLM needs to discover the new shape. A4 will tune
  further.

## 9. Concrete commit plan

```
Commit 1: refactor(tools/builtin/edit): extract execute_single_file from execute
          (pure refactor — no behavior change, prep for dispatch)
Commit 2: feat(tools/builtin/edit): accept batch form {files: [{path, edits}]}
          with sequential-dependency semantics
Commit 3: feat(tools/builtin/edit): cross-file path_args + schema oneOf
Commit 4: test(tools/builtin/edit): 8 new tests covering both shapes + edge cases
Commit 5: docs(MILESTONE_STATUS): record C1-Dirac-A2 completion
```

Five commits, ~400-500 lines of diff total. Bisectable.

## 10. Estimated effort

- Refactor commit 1: 1 hour
- Implement commit 2-3: 3-4 hours
- Tests: 2-3 hours
- LLM integration test + provider compat verification: 1 hour
- **Total: ~1 day** (matches research doc estimate)

## 11. Closes / unblocks

- C1-Dirac-A2 ✓
- Drives M2 progress: counts as M-Wireup. ~+5% to M2 closeout.
- **High visibility**: this is the borrow with the largest single-PR
  cost-saving impact on user-visible refactor tasks. Worth flagging in
  M2 closeout report with bench numbers.
- Unblocks future B1 (word-anchor upgrade) — anchors get applied
  per-file in the same batch loop with no schema churn.

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)):

- **Pre-flight branch check** (Stage 1 prerequisite): C1-Dirac-A1
  must be MERGED before A2 starts (A2's refactor of
  `execute_single_file` is cleaner when A1's `purge_orphaned_tool_results`
  is settled). The orchestrator enforces this; the reviewer should
  double-check the branch was created from up-to-date `main`.
- **Provider-compat checkpoint** (Stage 2 + Stage 3): spec §4.1
  `oneOf` schema fallback path is a known risk. The implementer
  must include the per-provider test result (Anthropic / OpenAI /
  Gemini) in the PR description. Reviewer must verify the fallback
  is exercised by at least one test if oneOf is rejected by any
  active provider.
- **Sequential-dependency semantics** (Stage 3 review focus): test
  `batch_first_file_validation_fails` (plan Task 5.3) must assert
  the EXACT error message text `"Skipped due to failure on prior
  file in batch"` (matches spec §3.2 line 257 reference). Reviewer
  enforces.
- **Atomicity** (Stage 3 review focus): test `batch_atomic_validate_then_apply`
  (plan Task 5.8) must verify NO disk write when validation fails
  on a later file in the batch. Reviewer must read this test
  carefully — it's the critical safety property.
- **Bench (optional but recommended)**: spec §6.3 declares optional
  bench. If implementer ran it, results in PR description; if not,
  flagged as M2 closeout follow-up. Either is acceptable; reviewer
  notes which.
- **Risk class**: MEDIUM — touches a critical agent tool;
  detect-and-dispatch logic adds surface area; bench claim is
  load-bearing for B-phase ROI.
