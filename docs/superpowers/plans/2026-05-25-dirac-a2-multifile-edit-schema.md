# Dirac-A2 — Multi-File Edit Schema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Steps use `- [ ]` syntax.

**Goal:** Extend `EditTool` to accept `{files: [{path, edits}, ...]}` batch form alongside the legacy `{path, edits}` form, enabling N-file refactors in 1 LLM round-trip.

**Architecture:** Detect-and-dispatch in `execute()`. New `files` shape routes to `execute_batch`; existing `path` shape routes to a refactored-but-behaviorally-identical `execute_single_file`. Two-phase apply (validate all → apply in order; first failure halts batch).

**Tech Stack:** Rust only. No new crates. No DB. No frontend (CLI/IPC schema only).

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-a2-multifile-edit-schema-design.md`

**PR tag:** `[C1-Dirac-A2]`

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/tools/builtin/edit.rs` | Detect-and-dispatch + `execute_batch` + `path_args` extension. +200-280 lines. |
| `src-tauri/src/agent/tools/builtin/edit.rs::mod tests` | +8 new tests. ~250 lines test code. |
| `docs/superpowers/MILESTONE_STATUS.md` | One-line entry |

**No new files. No new modules.**

---

## Pre-flight

- [ ] **Step 0.1: Branch + baseline**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/dirac-a2-multifile-edit-schema
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
```

Expect: drift GREEN/YELLOW, existing edit tests passing.

- [ ] **Step 0.2: Read C1 ordering**

Read `docs/superpowers/plans/2026-05-22-pr-integration-strategy.md` §7.
Verify A2 is parallelizable with A1/A3 — they touch disjoint files.

---

## Task 1: Refactor `execute` → `execute_single_file` (no behavior change)

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/edit.rs`

- [ ] **Step 1.1: Extract existing body into private method**

```rust
impl EditTool {
    // ... existing fields, new(), resolve_path(), generate_diff() ...

    async fn execute_single_file(
        &self,
        path: String,
        edits: Vec<EditArg>,
    ) -> Result<ToolOutput, ToolError> {
        // EXACT existing body — copy out of execute()
    }
}

#[async_trait]
impl Tool for EditTool {
    // ... unchanged ...
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = params["path"].as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?
            .to_string();
        let edits: Vec<EditArg> = serde_json::from_value(params["edits"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("edits: {e}")))?;
        self.execute_single_file(path, edits).await
    }
}
```

Define `EditArg` (replacing whatever inline deserialization currently exists):

```rust
#[derive(serde::Deserialize, Debug, Clone)]
struct EditArg {
    #[serde(default)]
    old_text: String,
    new_text: String,
    insert_line: Option<u32>,
    anchor: Option<String>,
    end_anchor: Option<String>,
}
```

- [ ] **Step 1.2: Verify zero behavior change**

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
# Expect: 100% same pass/fail as pre-refactor
```

If any existing test fails, the refactor was lossy. Stop, investigate.

**Commit checkpoint:**
```
git add -A
git commit -m "refactor(tools/builtin/edit): extract execute_single_file from execute

Pure refactor — no behavior change. Prepares for batch-form dispatch
in upcoming commits. Existing test suite passes unchanged."
```

---

## Task 2: Add batch-form types

- [ ] **Step 2.1: Add `FileEditsArg`, `FileBatchResult`, batch helpers**

In `edit.rs`, after `EditArg`:

```rust
#[derive(serde::Deserialize, Debug, Clone)]
struct FileEditsArg {
    path: String,
    edits: Vec<EditArg>,
}

#[derive(Debug)]
enum FileBatchResult {
    Applied { path: String, diff: String, edit_count: usize },
    ValidationFailed { path: String, error: String },
    ApplicationFailed { path: String, error: String },
    Skipped { path: String, reason: String },
}

impl FileBatchResult {
    fn path(&self) -> &str {
        match self {
            Self::Applied { path, .. }
            | Self::ValidationFailed { path, .. }
            | Self::ApplicationFailed { path, .. }
            | Self::Skipped { path, .. } => path,
        }
    }
    fn is_success(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }
    fn short_status(&self) -> &'static str {
        match self {
            Self::Applied { .. } => "✓",
            Self::ValidationFailed { .. } | Self::ApplicationFailed { .. } => "✗",
            Self::Skipped { .. } => "-",
        }
    }
}
```

**No commit yet** — combine with Task 3 in commit 2.

---

## Task 3: Implement `execute_batch`

- [ ] **Step 3.1: Add `execute_batch` private method**

```rust
impl EditTool {
    async fn execute_batch(
        &self,
        files: Vec<FileEditsArg>,
    ) -> Result<ToolOutput, ToolError> {
        if files.is_empty() {
            return Err(ToolError::InvalidParams(
                "`files` array must contain at least one entry".into(),
            ));
        }

        let mut results: Vec<FileBatchResult> = Vec::with_capacity(files.len());
        let mut first_failure: Option<usize> = None;

        // Two-phase execution: validate each, then apply in order
        for (i, file_edits) in files.iter().enumerate() {
            if first_failure.is_some() {
                results.push(FileBatchResult::Skipped {
                    path: file_edits.path.clone(),
                    reason: "Skipped due to failure on prior file in batch".into(),
                });
                continue;
            }

            // Phase 1: validate (read file, resolve anchors, etc.) without writing
            if let Err(e) = self.validate_single_file(&file_edits.path, &file_edits.edits).await {
                first_failure = Some(i);
                results.push(FileBatchResult::ValidationFailed {
                    path: file_edits.path.clone(),
                    error: e.to_string(),
                });
                continue;
            }

            // Phase 2: apply
            match self.apply_validated_single_file(
                file_edits.path.clone(),
                file_edits.edits.clone(),
            ).await {
                Ok((diff, count)) => results.push(FileBatchResult::Applied {
                    path: file_edits.path.clone(),
                    diff,
                    edit_count: count,
                }),
                Err(e) => {
                    first_failure = Some(i);
                    results.push(FileBatchResult::ApplicationFailed {
                        path: file_edits.path.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(self.format_batch_output(&results))
    }

    /// Read file, resolve anchors, dry-run each edit. Returns Err on
    /// any validation failure. Does NOT modify the file system.
    async fn validate_single_file(
        &self,
        path: &str,
        edits: &[EditArg],
    ) -> Result<(), ToolError> {
        let full_path = self.resolve_path(path);
        let _content = fs::read_to_string(&full_path).await
            .map_err(|e| ToolError::kinded(
                /* same mapping as execute_single_file */ todo!(),
                format!("read {}: {e}", full_path.display())
            ))?;
        // For each edit, check old_text presence / anchor resolvability /
        // insert_line range. Reuse the validation predicates from
        // execute_single_file. Extract into a private helper if not
        // already factored.
        // TODO during implementation: lift inline validation out of
        // apply path so it can be called separately.
        Ok(())
    }

    async fn apply_validated_single_file(
        &self,
        path: String,
        edits: Vec<EditArg>,
    ) -> Result<(String, usize), ToolError> {
        // Use existing apply logic from execute_single_file, but
        // return (unified_diff_string, edit_count) tuple instead of
        // ToolOutput. Existing path can be refactored to call this.
        todo!("refactor")
    }

    fn format_batch_output(&self, results: &[FileBatchResult]) -> ToolOutput {
        let total = results.len();
        let applied = results.iter().filter(|r| r.is_success()).count();
        let failed = results.iter().filter(|r| matches!(r,
            FileBatchResult::ValidationFailed{..} | FileBatchResult::ApplicationFailed{..}
        )).count();
        let skipped = results.iter().filter(|r| matches!(r, FileBatchResult::Skipped{..})).count();

        let mut summary = format!(
            "Applied edits to {} file(s) ({} succeeded, {} failed, {} skipped):\n\n",
            total, applied, failed, skipped,
        );
        for r in results {
            match r {
                FileBatchResult::Applied { path, edit_count, .. } => {
                    summary.push_str(&format!("✓ {}: {} edit(s) applied\n", path, edit_count));
                }
                FileBatchResult::ValidationFailed { path, error } |
                FileBatchResult::ApplicationFailed { path, error } => {
                    summary.push_str(&format!("✗ {}: {}\n", path, error));
                }
                FileBatchResult::Skipped { path, reason } => {
                    summary.push_str(&format!("- {}: {}\n", path, reason));
                }
            }
        }
        summary.push('\n');
        for r in results {
            if let FileBatchResult::Applied { diff, .. } = r {
                summary.push_str(diff);
                summary.push('\n');
            }
        }

        ToolOutput::success(&summary, 0)
    }
}
```

> **Implementation note**: Step 3.1 leaves `todo!()` markers for the
> validation/apply refactor. During implementation, factor the existing
> body of `execute_single_file` into:
> - `validate_single_file(path, edits) -> Result<(), Err>`
> - `apply_validated_single_file(path, edits) -> Result<(diff, count), Err>`
> Then `execute_single_file` becomes `validate + apply + ToolOutput::success`.
> This refactor is necessary; it also makes the single-file path testable
> in isolation.

- [ ] **Step 3.2: Update `execute` to dispatch**

```rust
async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    if let Some(files_val) = params.get("files") {
        let files: Vec<FileEditsArg> = serde_json::from_value(files_val.clone())
            .map_err(|e| ToolError::InvalidParams(format!("`files` shape: {e}")))?;
        self.execute_batch(files).await
    } else if let Some(path_val) = params.get("path") {
        let path = path_val.as_str()
            .ok_or_else(|| ToolError::InvalidParams("`path` must be a string".into()))?
            .to_string();
        let edits: Vec<EditArg> = serde_json::from_value(params["edits"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("edits: {e}")))?;
        self.execute_single_file(path, edits).await
    } else {
        Err(ToolError::InvalidParams(
            "either `files: [{path, edits}, ...]` or `{path, edits}` required".into()
        ))
    }
}
```

- [ ] **Step 3.3: Build + run existing tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head        # empty
cd src-tauri && cargo test --lib agent::tools::builtin::edit       # all passing
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/edit): accept batch form {files: [{path, edits}]}

Adds detect-and-dispatch on the 'files' property. New shape:
{files: [{path, edits: [...]}, ...]} applies edits per file in order,
with sequential-dependency semantics — first failure halts batch and
remaining files report 'Skipped due to failure on prior file'.

Two-phase execution (validate all → apply in order) prevents partial
filesystem writes when validation can be done up front.

Legacy {path, edits} shape continues to work — no deprecation.

Inspired by Dirac BatchProcessor.executeMultiFileBatch
(src/core/task/tools/handlers/edit-file/BatchProcessor.ts:89-306).

Spec: docs/superpowers/specs/2026-05-25-dirac-a2-multifile-edit-schema-design.md"
```

---

## Task 4: Update schema + `path_args`

- [ ] **Step 4.1: Update `parameters_schema`**

Replace the existing schema with the `oneOf` form from
spec §4.1. First commit attempt — if any provider rejects `oneOf`,
fall back to the loose shape (also documented in spec).

- [ ] **Step 4.2: Extend `path_args`**

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

- [ ] **Step 4.3: Update tool description string**

```rust
fn description(&self) -> &str {
    "Edit one or more files via search-replace, line insertion, or anchor-targeted edits. \
     Use `files: [{path, edits}]` to batch edits across files in a single call — \
     reduces LLM round-trips. Legacy `{path, edits}` form for single-file edits also works."
}
```

- [ ] **Step 4.4: Cross-provider compat check (manual)**

Test against each active provider:

| Provider | `oneOf` schema accepted? | Action if no |
|---|---|---|
| Anthropic | Y/N | Note in PR description |
| OpenAI | Y/N | Use fallback (spec §4.1 fallback note) |
| Gemini | Y/N | Use fallback |

If any provider rejects, swap schema to loose form for that provider's
adapter only, or globally if cleaner. Document the choice in commit
message.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/edit): schema oneOf + path_args for batch form

- parameters_schema accepts both shapes via oneOf
- path_args collects all paths from files[] for SafetyManager
- tool description encourages batching

Provider compat: tested against [providers], all accept oneOf form. (Or:
falling back to loose schema for [X] — see spec §4.1.)"
```

---

## Task 5: Eight new tests

- [ ] **Step 5.1: Test `legacy_single_file_unchanged`**

Send `{path, edits}` shape to `EditTool::execute`. Verify output byte-
identical to the pre-PR fixture (snapshot test, or just edit_count + diff).

- [ ] **Step 5.2: Test `batch_two_files_both_succeed`**

Two files in `tempdir`, edit both via batch form. Verify both diffs
appear in output, edit_count correct per file.

- [ ] **Step 5.3: Test `batch_first_file_validation_fails`**

File a has invalid anchor / nonexistent old_text. File b is valid.
Verify: a → ValidationFailed, b → Skipped with "Skipped due to
failure on prior file" reason.

- [ ] **Step 5.4: Test `batch_first_file_apply_fails`**

File a path resolves outside workspace (forbidden). File b inside.
Verify: a → ApplicationFailed, b → Skipped.

- [ ] **Step 5.5: Test `batch_middle_file_fails`**

Files [a, b, c]. b's anchor not found. Verify: a → Applied, b →
ValidationFailed, c → Skipped.

- [ ] **Step 5.6: Test `path_args_collects_all_batch_paths`**

Call `tool.path_args(&args)` with batch-form args. Verify returns
all N paths in order.

- [ ] **Step 5.7: Test `batch_neither_files_nor_path`**

Empty params `{}`. Verify InvalidParams with the helpful error text.

- [ ] **Step 5.8: Test `batch_atomic_validate_then_apply`**

File a valid (would succeed), file b validation fails. Verify file a
on disk is UNCHANGED (no partial write). This is critical — proves
the two-phase semantics.

```bash
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail -15
# Expect: existing + 8 new tests passing
```

**Commit checkpoint:**
```
git add -A
git commit -m "test(tools/builtin/edit): 8 tests for batch form + edge cases

Covers:
- legacy shape unchanged behavior
- batch happy path (both files succeed)
- validation failure halts batch
- application failure halts batch
- middle-file failure pattern
- path_args collects batch paths
- empty params produces helpful error
- two-phase atomicity (no partial writes on validation failure)"
```

---

## Task 6: Optional bench

- [ ] **Step 6.1: (optional) Round-trip count bench**

If `examples/refactor-bench/` infra exists, run a 3-file rename fixture
before and after this PR. Record:

| | Pre-PR | Post-PR |
|---|---|---|
| LLM round-trips | ? | ? |
| Total tokens (in + out) | ? | ? |
| Wall clock | ? | ? |

Include in PR description as bench evidence. Otherwise skip — flag as
follow-up for M2 closeout bench.

---

## Task 7: SSoT + PR

- [ ] **Step 7.1: Update MILESTONE_STATUS**

```
| C1-Dirac-A2 | EditTool batch form ({files: [...]}) | #<PR-number> |
```

Under §M2 detailed status. Update header line.

- [ ] **Step 7.2: Drift check**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
# GREEN/YELLOW
```

- [ ] **Step 7.3: Push + PR**

```bash
git push -u origin claude/dirac-a2-multifile-edit-schema
gh pr create \
  --title "[C1-Dirac-A2] feat(tools/edit): batch form for multi-file edits in one tool call" \
  --body "..."
```

PR description must include:
- ## Summary (one paragraph)
- ## Why (link research doc §7.2 + §0)
- ## Commits (bisectable) table — 5 commits
- ## Verification (paste cargo test output + bench numbers if available)
- ## Provider compat (oneOf result per provider)
- ## Spec link
- ## Closes (C1-Dirac-A2)

- [ ] **Step 7.4: Self-merge gate**

- [ ] CI green
- [ ] One reviewer approved (or self per project conventions)
- [ ] PR title carries `[C1-Dirac-A2]`
- [ ] Provider compat documented
- [ ] Bench numbers (if measured) in PR

---

## Rollback procedure

```bash
git revert <merge-commit-sha>
git push
```

Both schema shapes vanish; legacy `{path, edits}` returns as the only
shape. No data corruption.

---

## Closes / unblocks

- C1-Dirac-A2 ✓
- Unblocks: Phase B1 (word-anchor upgrade) — anchors get applied
  per-file in the existing batch loop, no schema churn
- Drives M2 progress ~+5%
- Largest single-PR ROI in Phase A by token cost

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> Run only when invoked by the autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).

- [ ] **Step A.1: Stage 2 self-verify (per protocol §3.2)**

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::tools::builtin::edit 2>&1 | tail -10
cargo clippy --lib -- -D warnings 2>&1 | tail -5

# Scope: ONLY these files
git diff --name-only main..HEAD | sort
# Expected: src-tauri/src/agent/tools/builtin/edit.rs + docs/superpowers/MILESTONE_STATUS.md

# Provider-compat note: search PR body draft for "Anthropic" / "OpenAI" / "Gemini" lines
# Reviewer expects at minimum: tested-against list
```

- [ ] **Step A.2: Spawn adversarial reviewer (protocol §3.3)**

Reviewer template invocation includes A2-specific focus from spec §12:
- Sequential-dependency exact error text
- Atomicity (no partial writes on validation failure)
- Provider-compat result documented

- [ ] **Step A.3: Reconcile per protocol §3.4** (oneOf rejection by ANY provider counts as `REQUEST_CHANGES (medium)` if not documented + tested)

- [ ] **Step A.4: PR open + CI wait + auto-merge (protocol §3.5)**

```bash
git push -u origin claude/dirac-a2-multifile-edit-schema
PR=$(gh pr create --title "[C1-Dirac-A2] feat(tools/edit): batch form for multi-file edits in one tool call" --body-file ./pr-body.md --json number -q .number)
gh pr checks $PR --watch --interval 30 --required
gh pr merge $PR --merge --delete-branch
git checkout main && git pull
```

- [ ] **Step A.5: Log + return outcome to orchestrator (protocol §7)**
