use async_trait::async_trait;
use similar::TextDiff;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::info;

use super::edit_verify;
use super::fuzzy_match::fuzzy_find_and_replace;

use crate::agent::anchor_state::{ANCHOR_DELIMITER, GLOBAL_ANCHOR_STATE_MANAGER, GLOBAL_FILE_CONTEXT_TRACKER};
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

/// Edit tool — supports search-and-replace edits on files.
///
/// Each edit specifies `old_text` (the text to find) and `new_text` (the replacement).
/// If `old_text` is empty, `new_text` is inserted at the given `insert_line` (1-based),
/// or appended to the end of the file if `insert_line` is omitted.
pub struct EditTool {
    workspace_root: PathBuf,
}

// ---------------------------------------------------------------------------
// Deserialization types
// ---------------------------------------------------------------------------

/// How an anchored edit places its `new_text` relative to the resolved
/// anchor line(s). Only meaningful when `anchor` is set. (spec §3.4)
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum AnchoredEditType {
    /// Replace the anchored line (or the `anchor..=end_anchor` range).
    #[default]
    Replace,
    /// Insert `new_text` immediately after the anchored line.
    InsertAfter,
    /// Insert `new_text` immediately before the anchored line.
    InsertBefore,
}

/// A single edit operation.
///
/// Three shapes, distinguished at apply time (kept as one struct rather than
/// an `#[serde(untagged)]` enum so A2's legacy `{old_text,new_text}` and batch
/// `{files}` shapes — and their tests — keep working unchanged):
/// - **anchored** (B1, preferred): `anchor` (+ optional `end_anchor`,
///   `edit_type`) targets a line by its stable token; `new_text` is the body.
/// - **literal search-replace**: `old_text` (non-empty) + `new_text`.
/// - **line insertion**: `old_text` empty + `new_text` (+ optional `insert_line`).
#[derive(serde::Deserialize, Debug, Clone)]
struct EditArg {
    #[serde(default)]
    old_text: String,
    #[serde(default)]
    new_text: String,
    insert_line: Option<u32>,
    anchor: Option<String>,
    end_anchor: Option<String>,
    /// Placement for anchored edits; defaults to `Replace`. Ignored unless
    /// `anchor` is set.
    #[serde(default)]
    edit_type: AnchoredEditType,
}

/// Batch-form per-file entry.
#[derive(serde::Deserialize, Debug, Clone)]
struct FileEditsArg {
    path: String,
    edits: Vec<EditArg>,
}

// ---------------------------------------------------------------------------
// Batch result types
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FileBatchResult {
    Applied { path: String, diff: String, edit_count: usize, lint_warning: Option<String> },
    ValidationFailed { path: String, error: String },
    ApplicationFailed { path: String, error: String },
    Skipped { path: String, reason: String },
}

impl FileBatchResult {
    fn is_success(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }

    fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::ValidationFailed { .. } | Self::ApplicationFailed { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Resolved-edit helper (private to apply logic)
// ---------------------------------------------------------------------------

struct ResolvedEdit {
    start_pos: usize,
    end_pos: usize,
    new_text: String,
}

// ---------------------------------------------------------------------------
// EditTool implementation
// ---------------------------------------------------------------------------

impl EditTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.workspace_root.join(path)
        }
    }

    /// Generate a unified diff between two strings.
    fn generate_diff(original: &str, modified: &str, path: &str) -> String {
        let diff = TextDiff::from_lines(original, modified);
        let mut output = String::new();

        let unified = diff.unified_diff();
        for hunk in unified.iter_hunks() {
            output.push_str(&hunk.to_string());
        }

        if output.is_empty() {
            format!("No changes to {}", path)
        } else {
            format!("--- {path}\n+++ {path}\n{output}")
        }
    }

    // -----------------------------------------------------------------------
    // Single-file path (refactored from original `execute` body)
    // -----------------------------------------------------------------------

    /// Execute a single-file edit (legacy `{path, edits}` form).
    async fn execute_single_file(
        &self,
        path: String,
        edits: Vec<EditArg>,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        if edits.is_empty() {
            return Err(ToolError::InvalidParams("edits array is empty".into()));
        }

        let full_path = self.resolve_path(&path);

        // Active File External Change Watcher check — hard reject (spec §8.4).
        if GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(stale_file_error(&full_path));
        }

        info!(path = %full_path.display(), edits = edits.len(), "Applying edits");

        // Read the original content
        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        // Refresh anchor state against current disk content so the 4-step
        // validator resolves tokens against what's actually on disk.
        let initial_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
        let _ = GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &initial_lines);

        let mut content = original.clone();
        let mut resolved_edits: Vec<ResolvedEdit> = Vec::new();
        let mut fuzzy_applied = 0usize; // count of search-replace edits applied via fuzzy

        for edit in edits.iter() {
            let old_text = &edit.old_text;
            let new_text = &edit.new_text;
            let insert_line = edit.insert_line;
            let anchor = edit.anchor.as_deref();
            let end_anchor = edit.end_anchor.as_deref();

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                // B1 4-step validator (replaces A2's Apple§hash position match).
                let start_idx = resolve_anchored_edit(&full_path, &lines, anchor_str)?;
                let end_idx = match end_anchor {
                    Some(ea) => {
                        let e = resolve_anchored_edit(&full_path, &lines, ea)?;
                        if e < start_idx {
                            return Err(ToolError::InvalidParams(format!(
                                "end_anchor resolves to line {} which is before anchor line {}",
                                e, start_idx
                            )));
                        }
                        e
                    }
                    None => start_idx,
                };

                resolved_edits.push(anchored_resolved_edit(
                    &content, start_idx, end_idx, edit.edit_type, new_text,
                ));
            } else if old_text.is_empty() {
                let (start_pos, end_pos) = match insert_line {
                    Some(line_num) => {
                        let lines_count = content.lines().count();
                        let line_idx = (line_num as usize).saturating_sub(1).min(lines_count);
                        if line_idx >= lines_count {
                            (content.len(), content.len())
                        } else {
                            let (start, _) = find_line_char_range(&content, line_idx, line_idx);
                            (start, start)
                        }
                    }
                    None => (content.len(), content.len()),
                };

                let mut formatted_new_text = new_text.to_string();
                if !formatted_new_text.ends_with('\n') {
                    formatted_new_text.push('\n');
                }
                if insert_line.is_none()
                    && start_pos == content.len()
                    && !content.ends_with('\n')
                    && !content.is_empty()
                {
                    formatted_new_text = format!("\n{}", formatted_new_text);
                }

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: formatted_new_text,
                });
            } else {
                // Search-replace: route through fuzzy-match chain (SP1).
                // fuzzy_find_and_replace tries exact first (no regression), then
                // 8 fuzzy strategies. Ambiguity and escape-drift are checked inside.
                let outcome = fuzzy_find_and_replace(&content, old_text, new_text, false)
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                info!(
                    path = %full_path.display(),
                    strategy = %outcome.strategy,
                    count = outcome.match_count,
                    "edit applied via fuzzy strategy"
                );
                content = outcome.new_content;
                fuzzy_applied += 1;
                // No ResolvedEdit pushed: fuzzy already applied directly to `content`.
                // Anchored + insert edits (collected in resolved_edits) are applied below.
            }
        }

        // Apply anchored / insert resolved edits (descending by position so
        // earlier byte offsets aren't shifted by later insertions).
        resolved_edits.sort_by(|a, b| {
            b.start_pos
                .cmp(&a.start_pos)
                .then_with(|| b.end_pos.cmp(&a.end_pos))
        });

        for re in &resolved_edits {
            let mut new_content = String::with_capacity(content.len() + re.new_text.len());
            new_content.push_str(&content[..re.start_pos]);
            new_content.push_str(&re.new_text);
            new_content.push_str(&content[re.end_pos..]);
            content = new_content;
        }

        let applied = fuzzy_applied + resolved_edits.len();

        GLOBAL_FILE_CONTEXT_TRACKER.register_expected_write(&full_path);

        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        // SP2: read-back byte-compare (hard error on silent write failure).
        edit_verify::read_back_verify(&full_path, &content)
            .await
            .map_err(|e| ToolError::Execution(format!("read-back verify failed: {e}")))?;

        // SP2: incremental structured lint (advisory — attaches to result, never fails edit).
        let lint_warning = edit_verify::incremental_structured_lint(&full_path, &original, &content)
            .map(|f| format!("{}: {}", f.format, f.message));

        // Re-align anchors to the post-write content (record_read tracks the
        // old content internally, so no need to pass old_lines).
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let _ = GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &new_lines);

        let diff = Self::generate_diff(&original, &content, &path);
        let mut summary = format!(
            "Applied {} edit(s) to {}\n\n{}",
            applied,
            full_path.display(),
            diff
        );
        if let Some(ref warn) = lint_warning {
            summary.push_str(&format!("\n⚠ lint: {}", warn));
        }

        info!(path = %full_path.display(), applied, "Edits applied successfully");
        Ok(ToolOutput::success(&summary, start.elapsed().as_millis() as u64))
    }

    // -----------------------------------------------------------------------
    // Two-phase helpers: validate (no disk writes) + apply
    // -----------------------------------------------------------------------

    /// Phase 1 — read file and validate all edits can be resolved without writing.
    /// Returns `Err` on the first validation failure.
    async fn validate_single_file(
        &self,
        path: &str,
        edits: &[EditArg],
    ) -> Result<(), ToolError> {
        if edits.is_empty() {
            return Err(ToolError::InvalidParams("edits array is empty".into()));
        }

        let full_path = self.resolve_path(path);

        // Stale-file gate — hard reject (spec §8.4).
        if GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(stale_file_error(&full_path));
        }

        // Read file — validates existence and readability; NO disk write
        let content = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        // Refresh anchor state so the 4-step validator resolves against the
        // current on-disk content (no disk write here).
        let validate_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let _ = GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &validate_lines);

        // Maintain a local copy of content for sequential fuzzy validation;
        // no disk writes happen here — this tracks what the content WOULD look
        // like after each edit so subsequent edits are validated against the
        // post-edit content snapshot.
        let mut sim_content = content.clone();

        for edit in edits.iter() {
            let old_text = &edit.old_text;
            let new_text = &edit.new_text;
            let anchor = edit.anchor.as_deref();
            let end_anchor = edit.end_anchor.as_deref();

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = sim_content.lines().map(|s| s.to_string()).collect();
                // B1 4-step validator (replaces A2's Apple§hash position match).
                let start_idx = resolve_anchored_edit(&full_path, &lines, anchor_str)?;
                if let Some(end_anchor_str) = end_anchor {
                    let e = resolve_anchored_edit(&full_path, &lines, end_anchor_str)?;
                    if e < start_idx {
                        return Err(ToolError::InvalidParams(format!(
                            "end_anchor resolves to line {} which is before anchor line {}",
                            e, start_idx
                        )));
                    }
                }
            } else if old_text.is_empty() {
                // Insert mode — any insert_line value is accepted (apply clamps)
            } else {
                // Search-replace: verify old_text exists via fuzzy chain (SP1).
                // On success update sim_content so subsequent validation is correct.
                match fuzzy_find_and_replace(&sim_content, old_text, new_text, false) {
                    Ok(outcome) => {
                        sim_content = outcome.new_content;
                    }
                    Err(e) => {
                        return Err(ToolError::Execution(e.to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    /// Phase 2 — apply the edits to the file on disk and return `(diff, edit_count, lint_warning)`.
    /// Caller MUST have called `validate_single_file` successfully first.
    async fn apply_validated_single_file(
        &self,
        path: String,
        edits: Vec<EditArg>,
    ) -> Result<(String, usize, Option<String>), ToolError> {
        let full_path = self.resolve_path(&path);

        // Stale-file gate — hard reject (spec §8.4). Defensive: the batch path
        // validates first, but a direct apply must not write stale content.
        if GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(stale_file_error(&full_path));
        }

        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        // Refresh anchor state against current disk content for the validator.
        let initial_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
        let _ = GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &initial_lines);

        let mut content = original.clone();
        let mut resolved_edits: Vec<ResolvedEdit> = Vec::new();
        let mut fuzzy_applied = 0usize;

        for edit in edits.iter() {
            let old_text = &edit.old_text;
            let new_text = &edit.new_text;
            let insert_line = edit.insert_line;
            let anchor = edit.anchor.as_deref();
            let end_anchor = edit.end_anchor.as_deref();

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                // B1 4-step validator (replaces A2's Apple§hash position match).
                let start_idx = resolve_anchored_edit(&full_path, &lines, anchor_str)?;
                let end_idx = match end_anchor {
                    Some(ea) => {
                        let e = resolve_anchored_edit(&full_path, &lines, ea)?;
                        if e < start_idx {
                            return Err(ToolError::InvalidParams(format!(
                                "end_anchor resolves to line {} which is before anchor line {}",
                                e, start_idx
                            )));
                        }
                        e
                    }
                    None => start_idx,
                };

                resolved_edits.push(anchored_resolved_edit(
                    &content, start_idx, end_idx, edit.edit_type, new_text,
                ));
            } else if old_text.is_empty() {
                let (start_pos, end_pos) = match insert_line {
                    Some(line_num) => {
                        let lines_count = content.lines().count();
                        let line_idx = (line_num as usize).saturating_sub(1).min(lines_count);
                        if line_idx >= lines_count {
                            (content.len(), content.len())
                        } else {
                            let (start, _) = find_line_char_range(&content, line_idx, line_idx);
                            (start, start)
                        }
                    }
                    None => (content.len(), content.len()),
                };

                let mut formatted_new_text = new_text.clone();
                if !formatted_new_text.ends_with('\n') {
                    formatted_new_text.push('\n');
                }
                if insert_line.is_none()
                    && start_pos == content.len()
                    && !content.ends_with('\n')
                    && !content.is_empty()
                {
                    formatted_new_text = format!("\n{}", formatted_new_text);
                }

                resolved_edits.push(ResolvedEdit { start_pos, end_pos, new_text: formatted_new_text });
            } else {
                // Search-replace: route through fuzzy-match chain (SP1).
                let outcome = fuzzy_find_and_replace(&content, old_text, new_text, false)
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                info!(
                    path = %full_path.display(),
                    strategy = %outcome.strategy,
                    count = outcome.match_count,
                    "edit applied via fuzzy strategy"
                );
                content = outcome.new_content;
                fuzzy_applied += 1;
            }
        }

        // Apply anchored / insert resolved edits (descending by position).
        resolved_edits.sort_by(|a, b| {
            b.start_pos
                .cmp(&a.start_pos)
                .then_with(|| b.end_pos.cmp(&a.end_pos))
        });

        for re in &resolved_edits {
            let mut new_content = String::with_capacity(content.len() + re.new_text.len());
            new_content.push_str(&content[..re.start_pos]);
            new_content.push_str(&re.new_text);
            new_content.push_str(&content[re.end_pos..]);
            content = new_content;
        }

        let applied = fuzzy_applied + resolved_edits.len();

        GLOBAL_FILE_CONTEXT_TRACKER.register_expected_write(&full_path);

        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        // SP2: read-back byte-compare (hard error on silent write failure).
        edit_verify::read_back_verify(&full_path, &content)
            .await
            .map_err(|e| ToolError::Execution(format!("read-back verify failed: {e}")))?;

        // SP2: incremental structured lint (advisory — attaches to result, never fails edit).
        let lint_warning = edit_verify::incremental_structured_lint(&full_path, &original, &content)
            .map(|f| format!("{}: {}", f.format, f.message));

        // Re-align anchors to the post-write content (record_read tracks the
        // old content internally).
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let _ = GLOBAL_ANCHOR_STATE_MANAGER.record_read(&full_path, &new_lines);

        let diff = Self::generate_diff(&original, &content, &path);
        Ok((diff, applied, lint_warning))
    }

    // -----------------------------------------------------------------------
    // Batch execution
    // -----------------------------------------------------------------------

    /// Execute a batch of file edits with two-phase atomicity:
    ///
    /// **Phase 1**: Validate ALL files (read, resolve anchors/old_text, range-check)
    ///              with NO disk writes.
    /// **Phase 2**: Only if ALL validations passed — apply each file in order.
    ///              First application failure halts; remaining files are skipped.
    ///
    /// If any Phase-1 validation fails, NO files are written to disk.
    async fn execute_batch(
        &self,
        files: Vec<FileEditsArg>,
    ) -> Result<ToolOutput, ToolError> {
        if files.is_empty() {
            return Err(ToolError::InvalidParams(
                "`files` array must contain at least one entry".into(),
            ));
        }

        // ------------------------------------------------------------------
        // Phase 1: validate ALL files — no disk writes
        // ------------------------------------------------------------------
        let mut validations: Vec<Result<(), String>> = Vec::with_capacity(files.len());
        for file_arg in &files {
            let result = self
                .validate_single_file(&file_arg.path, &file_arg.edits)
                .await
                .map_err(|e| e.to_string());
            validations.push(result);
        }

        let first_failure_idx = validations.iter().position(|v| v.is_err());

        if let Some(fail_idx) = first_failure_idx {
            // Build result vector: failure + skips for ALL entries — no disk writes
            let failed_path = files[fail_idx].path.clone();
            let mut results: Vec<FileBatchResult> = Vec::with_capacity(files.len());
            for (i, (file_arg, validation)) in
                files.iter().zip(validations).enumerate()
            {
                if i == fail_idx {
                    results.push(FileBatchResult::ValidationFailed {
                        path: file_arg.path.clone(),
                        error: validation.unwrap_err(),
                    });
                } else if i < fail_idx {
                    // Validated OK but never applied: a LATER file failed Phase-1
                    // validation, so the whole batch aborts before any disk write.
                    // This is NOT a "prior file" skip — report it accurately so the
                    // LLM doesn't infer a false dependency on the failed file.
                    results.push(FileBatchResult::Skipped {
                        path: file_arg.path.clone(),
                        reason: format!(
                            "Not applied — batch aborted: validation failed on a later file ({failed_path})"
                        ),
                    });
                } else {
                    // Entries after the failure: genuinely skipped due to the
                    // prior-file failure (first-failure-skips-rest, spec §8.4).
                    results.push(FileBatchResult::Skipped {
                        path: file_arg.path.clone(),
                        reason: "Skipped due to failure on prior file in batch".into(),
                    });
                }
            }
            return Ok(self.format_batch_output(&results));
        }

        // ------------------------------------------------------------------
        // Phase 2: apply each file in order — first failure halts
        // ------------------------------------------------------------------
        let mut results: Vec<FileBatchResult> = Vec::with_capacity(files.len());
        let mut first_apply_failure: Option<usize> = None;

        for (i, file_arg) in files.into_iter().enumerate() {
            if first_apply_failure.is_some() {
                results.push(FileBatchResult::Skipped {
                    path: file_arg.path,
                    reason: "Skipped due to failure on prior file in batch".into(),
                });
                continue;
            }

            match self
                .apply_validated_single_file(file_arg.path.clone(), file_arg.edits)
                .await
            {
                Ok((diff, edit_count, lint_warning)) => {
                    results.push(FileBatchResult::Applied {
                        path: file_arg.path,
                        diff,
                        edit_count,
                        lint_warning,
                    });
                }
                Err(e) => {
                    first_apply_failure = Some(i);
                    results.push(FileBatchResult::ApplicationFailed {
                        path: file_arg.path,
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(self.format_batch_output(&results))
    }

    /// Format batch results into a `ToolOutput` the LLM can parse.
    fn format_batch_output(&self, results: &[FileBatchResult]) -> ToolOutput {
        let total = results.len();
        let applied = results.iter().filter(|r| r.is_success()).count();
        let failed = results.iter().filter(|r| r.is_failure()).count();
        let skipped = results
            .iter()
            .filter(|r| matches!(r, FileBatchResult::Skipped { .. }))
            .count();

        let mut summary = format!(
            "Applied edits to {} file(s) ({} succeeded, {} failed, {} skipped):\n\n",
            total, applied, failed, skipped,
        );

        for r in results {
            match r {
                FileBatchResult::Applied { path, edit_count, lint_warning, .. } => {
                    let warn = lint_warning
                        .as_deref()
                        .map(|w| format!(" ⚠ lint: {}", w))
                        .unwrap_or_default();
                    summary.push_str(&format!("✓ {}: {} edit(s) applied{}\n", path, edit_count, warn));
                }
                FileBatchResult::ValidationFailed { path, error }
                | FileBatchResult::ApplicationFailed { path, error } => {
                    summary.push_str(&format!("✗ {}: {}\n", path, error));
                }
                FileBatchResult::Skipped { path, reason } => {
                    summary.push_str(&format!("- {}: {}\n", path, reason));
                }
            }
        }
        summary.push('\n');

        for r in results {
            if let FileBatchResult::Applied { diff, path, .. } = r {
                summary.push_str(&format!("=== {} ===\n", path));
                summary.push_str(diff);
                summary.push('\n');
            }
        }

        ToolOutput::success(&summary, 0)
    }
}

/// Build the hard-reject error for a file that was modified externally since
/// last read. Uses `ToolErrorKind::PreconditionFailed` (NOT a soft warning) —
/// the LLM must re-read before editing (spec §8.4, "environment as forcing
/// function").
fn stale_file_error(full_path: &Path) -> ToolError {
    ToolError::kinded(
        ToolErrorKind::PreconditionFailed,
        format!(
            "{} was modified externally since last read. Re-read with read_file before editing.",
            full_path.display()
        ),
    )
}

/// Validate an anchor token's syntax: `^[A-Z][a-zA-Z]+(-\d+)?$`-ish.
/// First char uppercase ASCII letter; remaining chars ASCII letters, digits,
/// or `-` (to allow the legacy `Apple-1` collision suffix). (spec §3.5 step 1)
fn is_valid_anchor_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '-')
}

/// 4-step validator for an anchored edit (Dirac `EditExecutor.resolveAnchor`).
/// Returns the resolved 0-based line index on success. All failure paths
/// return `ToolError::InvalidParams` with LLM-actionable wording. (spec §3.5)
///
/// 1. format: anchor splits into `token§content`, token syntax is valid
/// 2. token exists in the file's CURRENT anchor list (AnchorStateManager)
/// 3. provided content portion is single-line (no `\n`)
/// 4. byte-equal: provided content == the current `lines[idx]`
fn resolve_anchored_edit(
    path: &Path,
    current_lines: &[String],
    anchor: &str,
) -> Result<usize, ToolError> {
    // Step 1a: split on the delimiter.
    let (token, provided_content) = anchor.split_once(ANCHOR_DELIMITER).ok_or_else(|| {
        ToolError::InvalidParams(format!(
            "anchor must contain the '{}' delimiter (format `Token{}<line content>`): {:?}",
            ANCHOR_DELIMITER, ANCHOR_DELIMITER, anchor
        ))
    })?;

    // Step 1b: token syntax.
    if !is_valid_anchor_token(token) {
        return Err(ToolError::InvalidParams(format!(
            "anchor token {:?} must match ^[A-Z][a-zA-Z]+(-\\d+)?$",
            token
        )));
    }

    // Step 3: no newline in the provided content portion.
    if provided_content.contains('\n') {
        return Err(ToolError::InvalidParams(
            "anchor content must be single-line (no '\\n')".into(),
        ));
    }

    // Step 2: token exists in the current file's anchor list.
    let idx = GLOBAL_ANCHOR_STATE_MANAGER
        .resolve_anchor_index(path, token)
        .ok_or_else(|| {
            ToolError::InvalidParams(format!(
                "anchor token '{}' not found in {}. Re-read the file with read_file to refresh anchors.",
                token,
                path.display()
            ))
        })?;

    // Step 4: byte-equal content.
    let actual = current_lines.get(idx).ok_or_else(|| {
        ToolError::InvalidParams(format!(
            "anchor '{}' resolved to out-of-range index {} (file has {} lines). Re-read with read_file.",
            token,
            idx,
            current_lines.len()
        ))
    })?;
    if actual != provided_content {
        return Err(ToolError::InvalidParams(format!(
            "anchor content mismatch.\n  Expected: {:?}\n  Provided: {:?}\n  Re-read with read_file if you think the file changed.",
            actual, provided_content
        )));
    }

    Ok(idx)
}

/// Compute the `(start_pos, end_pos, new_text)` char-range edit for a
/// validated anchored edit. `start_idx`/`end_idx` are the resolved 0-based
/// line indices (`end_idx >= start_idx`). Replace overwrites the line range;
/// InsertBefore/After splice a zero-width range with a trailing newline so the
/// inserted body becomes whole line(s). (spec §3.4 / §3.5)
fn anchored_resolved_edit(
    content: &str,
    start_idx: usize,
    end_idx: usize,
    edit_type: AnchoredEditType,
    new_text: &str,
) -> ResolvedEdit {
    match edit_type {
        AnchoredEditType::Replace => {
            let (start_pos, end_pos) = find_line_char_range(content, start_idx, end_idx);
            let mut formatted = new_text.to_string();
            // Preserve the trailing newline of the replaced range when the
            // replacement text doesn't already carry one.
            if !formatted.ends_with('\n') && content[start_pos..end_pos].ends_with('\n') {
                formatted.push('\n');
            }
            ResolvedEdit { start_pos, end_pos, new_text: formatted }
        }
        AnchoredEditType::InsertBefore => {
            let (start_pos, _) = find_line_char_range(content, start_idx, start_idx);
            let mut formatted = new_text.to_string();
            if !formatted.ends_with('\n') {
                formatted.push('\n');
            }
            ResolvedEdit { start_pos, end_pos: start_pos, new_text: formatted }
        }
        AnchoredEditType::InsertAfter => {
            // Insert at the start of the line after `end_idx`.
            let (_, after_pos) = find_line_char_range(content, end_idx, end_idx);
            let mut formatted = new_text.to_string();
            if !formatted.ends_with('\n') {
                formatted.push('\n');
            }
            // If the anchored line had no trailing newline (last line of a file
            // without a final newline), inject one before the new body so the
            // insert lands on its own line.
            if !content[..after_pos].ends_with('\n') && !content.is_empty() {
                formatted = format!("\n{}", formatted);
            }
            ResolvedEdit { start_pos: after_pos, end_pos: after_pos, new_text: formatted }
        }
    }
}

/// Find the character index range for a range of 0-based lines (inclusive)
fn find_line_char_range(content: &str, start_line_idx: usize, end_line_idx: usize) -> (usize, usize) {
    let mut start_pos = None;
    let mut end_pos = None;
    let mut current_pos = 0;

    let lines: Vec<&str> = content.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let line_len_with_nl = line.len() + 1;
        if idx == start_line_idx {
            start_pos = Some(current_pos);
        }
        if idx == end_line_idx {
            let line_end_with_nl = (current_pos + line_len_with_nl).min(content.len());
            end_pos = Some(line_end_with_nl);
            break;
        }
        current_pos += line_len_with_nl;
    }

    let start = start_pos.unwrap_or(content.len());
    let end = end_pos.unwrap_or(content.len());
    (start, end)
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit one or more files. Each edit is one of three shapes: \
         (1) anchor-targeted (preferred): {anchor: \"Apple§<exact line content>\", end_anchor?, \
         edit_type?: replace|insert_after|insert_before, new_text} — the anchor token comes from \
         read_file output and must byte-match the current line; (2) literal search-replace: \
         {old_text, new_text}; (3) line insertion: {old_text: \"\", new_text, insert_line?}. \
         Use `files: [{path, edits}]` to batch across files in one call. Editing a file that was \
         modified externally since your last read is rejected — re-read it first."
    }

    fn preview_target_path(&self, args: &serde_json::Value) -> Option<String> {
        // For batch form, return the first file's path for preview purposes
        if let Some(files) = args.get("files").and_then(|f| f.as_array()) {
            files
                .first()
                .and_then(|f| f.get("path"))
                .and_then(|p| p.as_str())
                .map(String::from)
        } else {
            args.get("path").and_then(|v| v.as_str()).map(String::from)
        }
    }

    /// Loose schema — both `files` and `path` are optional properties.
    /// Use `files: [{path, edits}]` for multi-file edits OR `{path, edits}` for a single file.
    ///
    /// Note: `oneOf` was considered (spec §8.3) but deferred — autonomous mode cannot run
    /// live provider-compat tests (Anthropic/OpenAI/Gemini). The loose schema is accepted
    /// by all providers by construction. `oneOf` can be adopted once provider testing is
    /// available (see spec §4.1 fallback note).
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "description": "Use `files: [{path, edits}]` for multi-file edits OR `{path, edits}` for a single file.",
            "properties": {
                "files": {
                    "type": "array",
                    "description": "Files to edit (batch form). Applied in order; first failure skips remaining.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "File path." },
                            "edits": {
                                "type": "array",
                                "description": "Edits applied in order.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "old_text": { "type": "string", "description": "Exact text to find; empty = insert mode." },
                                        "new_text": { "type": "string", "description": "Replacement or text to insert." },
                                        "insert_line": { "type": "integer", "description": "1-based line for insertion (only when old_text is empty)." },
                                        "anchor": { "type": "string", "description": "Anchor-targeted edit. Full anchor `Token§<exact line content>` copied from read_file output; must byte-match the current line." },
                                        "end_anchor": { "type": "string", "description": "Optional end anchor (same format) for a multi-line range; must resolve at or after `anchor`." },
                                        "edit_type": { "type": "string", "enum": ["replace", "insert_after", "insert_before"], "description": "For anchored edits: replace the line(s), or insert new_text before/after. Defaults to replace." }
                                    },
                                    "required": ["new_text"]
                                }
                            }
                        },
                        "required": ["path", "edits"]
                    }
                },
                "path": { "type": "string", "description": "File path (single-file form)." },
                "edits": {
                    "type": "array",
                    "description": "Edits applied in order (single-file form).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_text": { "type": "string", "description": "Exact text to find; empty = insert mode." },
                            "new_text": { "type": "string", "description": "Replacement or text to insert." },
                            "insert_line": { "type": "integer", "description": "1-based line for insertion (only when old_text is empty)." },
                            "anchor": { "type": "string", "description": "Anchor-targeted edit. Full anchor `Token§<exact line content>` copied from read_file output; must byte-match the current line." },
                            "end_anchor": { "type": "string", "description": "Optional end anchor (same format) for a multi-line range; must resolve at or after `anchor`." },
                            "edit_type": { "type": "string", "enum": ["replace", "insert_after", "insert_before"], "description": "For anchored edits: replace the line(s), or insert new_text before/after. Defaults to replace." }
                        },
                        "required": ["new_text"]
                    }
                }
            }
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        if let Some(files) = args.get("files").and_then(|f| f.as_array()) {
            // Batch form: collect all paths for SafetyManager
            files
                .iter()
                .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                .collect()
        } else {
            // Legacy single-file form
            args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
        }
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if let Some(files_val) = params.get("files") {
            // Batch form
            let files: Vec<FileEditsArg> = serde_json::from_value(files_val.clone())
                .map_err(|e| ToolError::InvalidParams(format!("`files` shape error: {e}")))?;
            self.execute_batch(files).await
        } else if params.get("path").is_some() {
            // Legacy single-file form
            let path = params["path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?
                .to_string();
            let edits_val = params.get("edits").ok_or_else(|| {
                ToolError::InvalidParams("`edits` array is required in single-file form".into())
            })?;
            let edits: Vec<EditArg> = serde_json::from_value(edits_val.clone())
                .map_err(|e| ToolError::InvalidParams(format!("`edits` shape error: {e}")))?;
            self.execute_single_file(path, edits).await
        } else {
            Err(ToolError::InvalidParams(
                "either `files: [{path, edits}, ...]` or `{path, edits}` required".into(),
            ))
        }
    }
}

#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn edit_path_args_returns_path() {
        let tool = EditTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "lib.rs", "edits": []});
        assert_eq!(tool.path_args(&args), vec!["lib.rs"]);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;
    use tempfile::tempdir;

    // -----------------------------------------------------------------------
    // Test 5.1 — legacy single-file form works unchanged
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn legacy_single_file_unchanged() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("foo.rs");
        tokio::fs::write(&file_path, "fn foo() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "foo.rs",
            "edits": [{"old_text": "fn foo()", "new_text": "fn bar()"}]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();
        assert!(text.contains("foo.rs"), "output should mention path: {}", text);
        assert!(text.contains("edit"), "output should mention edits: {}", text);

        let new_content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(new_content.contains("fn bar()"), "file content: {}", new_content);
        assert!(!new_content.contains("fn foo()"), "file content: {}", new_content);
    }

    // -----------------------------------------------------------------------
    // Test 5.1b — legacy single-file form with omitted/missing old_text (insert mode)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn legacy_single_file_omitted_old_text() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("foo.rs");
        tokio::fs::write(&file_path, "fn foo() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        // old_text is completely omitted here — should default to "" and run insert mode (append)
        let params = serde_json::json!({
            "path": "foo.rs",
            "edits": [{"new_text": "fn bar() {}\n"}]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();
        assert!(text.contains("foo.rs"), "output should mention path: {}", text);

        let new_content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(new_content.contains("fn foo()"), "file content: {}", new_content);
        assert!(new_content.contains("fn bar()"), "file content: {}", new_content);
    }

    // -----------------------------------------------------------------------
    // Test 5.2 — batch form: two files both succeed
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_two_files_both_succeed() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        tokio::fs::write(&file_a, "fn alpha() {}\n").await.unwrap();
        tokio::fs::write(&file_b, "fn beta() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "files": [
                {"path": "a.rs", "edits": [{"old_text": "fn alpha()", "new_text": "fn ALPHA()"}]},
                {"path": "b.rs", "edits": [{"old_text": "fn beta()", "new_text": "fn BETA()"}]}
            ]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        assert!(text.contains("✓ a.rs"), "output: {}", text);
        assert!(text.contains("✓ b.rs"), "output: {}", text);
        assert!(text.contains("2 succeeded"), "output: {}", text);

        let content_a = tokio::fs::read_to_string(&file_a).await.unwrap();
        let content_b = tokio::fs::read_to_string(&file_b).await.unwrap();
        assert!(content_a.contains("fn ALPHA()"), "a.rs: {}", content_a);
        assert!(content_b.contains("fn BETA()"), "b.rs: {}", content_b);
    }

    // -----------------------------------------------------------------------
    // Test 5.3 — first file validation fails → second file skipped with EXACT text
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_first_file_validation_fails() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        tokio::fs::write(&file_a, "fn alpha() {}\n").await.unwrap();
        tokio::fs::write(&file_b, "fn beta() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "files": [
                // old_text not present in a.rs → validation failure
                {"path": "a.rs", "edits": [{"old_text": "NONEXISTENT_TEXT", "new_text": "replaced"}]},
                {"path": "b.rs", "edits": [{"old_text": "fn beta()", "new_text": "fn BETA()"}]}
            ]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        assert!(text.contains("✗ a.rs"), "a.rs should show failure: {}", text);
        // EXACT required string per spec §3.2 and plan Task 5.3
        assert!(
            text.contains("Skipped due to failure on prior file in batch"),
            "must contain exact skip reason, got: {}",
            text
        );
        assert!(text.contains("- b.rs"), "b.rs should be skipped: {}", text);

        // Neither file changed
        let content_a = tokio::fs::read_to_string(&file_a).await.unwrap();
        let content_b = tokio::fs::read_to_string(&file_b).await.unwrap();
        assert_eq!(content_a, "fn alpha() {}\n");
        assert_eq!(content_b, "fn beta() {}\n");
    }

    // -----------------------------------------------------------------------
    // Test 5.4 — first file apply fails (nonexistent file) → second skipped
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_first_file_apply_fails() {
        let dir = tempdir().unwrap();
        let file_b = dir.path().join("b.rs");
        tokio::fs::write(&file_b, "fn beta() {}\n").await.unwrap();
        // nonexistent.rs is deliberately not created

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "files": [
                // file doesn't exist → read fails in validate_single_file
                {"path": "nonexistent.rs", "edits": [{"old_text": "anything", "new_text": "replaced"}]},
                {"path": "b.rs", "edits": [{"old_text": "fn beta()", "new_text": "fn BETA()"}]}
            ]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        assert!(text.contains("✗ nonexistent.rs"), "output: {}", text);
        assert!(
            text.contains("Skipped due to failure on prior file in batch"),
            "output: {}",
            text
        );

        // b.rs unchanged
        let content_b = tokio::fs::read_to_string(&file_b).await.unwrap();
        assert_eq!(content_b, "fn beta() {}\n");
    }

    // -----------------------------------------------------------------------
    // Test 5.5 — middle file fails: a passes validation, b fails, c skipped;
    //            two-phase means a is also NOT applied (all-or-nothing)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_middle_file_fails() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        let file_c = dir.path().join("c.rs");
        tokio::fs::write(&file_a, "fn alpha() {}\n").await.unwrap();
        tokio::fs::write(&file_b, "fn beta() {}\n").await.unwrap();
        tokio::fs::write(&file_c, "fn gamma() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "files": [
                {"path": "a.rs", "edits": [{"old_text": "fn alpha()", "new_text": "fn ALPHA()"}]},
                // b.rs: old_text missing → Phase 1 validation failure
                {"path": "b.rs", "edits": [{"old_text": "MISSING_TEXT", "new_text": "replaced"}]},
                {"path": "c.rs", "edits": [{"old_text": "fn gamma()", "new_text": "fn GAMMA()"}]}
            ]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        assert!(text.contains("✗ b.rs"), "b.rs should fail: {}", text);
        assert!(
            text.contains("Skipped due to failure on prior file in batch"),
            "output: {}",
            text
        );
        assert!(text.contains("- c.rs"), "c.rs should be skipped: {}", text);

        // Two-phase: b fails in Phase 1, so Phase 2 never runs — a.rs NOT written
        let content_a = tokio::fs::read_to_string(&file_a).await.unwrap();
        assert_eq!(
            content_a, "fn alpha() {}\n",
            "a.rs must be unchanged — b's Phase-1 failure aborts Phase 2 entirely"
        );
        let content_c = tokio::fs::read_to_string(&file_c).await.unwrap();
        assert_eq!(content_c, "fn gamma() {}\n");

        // a.rs validated OK but was NOT applied (b's Phase-1 failure aborts the
        // batch). Its output line must accurately say "batch aborted", NOT the
        // "prior file" reason (a has no prior failure) and NOT "✓ applied".
        assert!(
            text.contains("- a.rs") && text.contains("batch aborted"),
            "a.rs must report batch-abort (not applied, not a prior-file skip): {}",
            text
        );
        assert!(
            !text.contains("✓ a.rs"),
            "a.rs must NOT be reported as applied under two-phase abort: {}",
            text
        );
    }

    // -----------------------------------------------------------------------
    // Test 5.6 — path_args collects ALL batch paths in order
    // -----------------------------------------------------------------------
    #[test]
    fn path_args_collects_all_batch_paths() {
        let tool = EditTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({
            "files": [
                {"path": "src/a.rs", "edits": []},
                {"path": "src/b.rs", "edits": []},
                {"path": "src/c.rs", "edits": []}
            ]
        });
        let paths = tool.path_args(&args);
        assert_eq!(paths, vec!["src/a.rs", "src/b.rs", "src/c.rs"]);
    }

    // -----------------------------------------------------------------------
    // Test 5.7 — neither files nor path → InvalidParams with helpful message
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_neither_files_nor_path() {
        let tool = EditTool::new(std::path::PathBuf::from("/tmp"));
        let err = tool.execute(serde_json::json!({})).await.unwrap_err();
        match err {
            ToolError::InvalidParams(msg) => {
                assert!(
                    msg.contains("files") || msg.contains("path"),
                    "error should mention 'files' or 'path', got: {}",
                    msg
                );
            }
            other => panic!("expected InvalidParams, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Test 5.8 — two-phase atomicity: b fails validation → a NOT written to disk
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn batch_atomic_validate_then_apply() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");

        let original_a = "fn alpha() { /* original */ }\n";
        tokio::fs::write(&file_a, original_a).await.unwrap();
        tokio::fs::write(&file_b, "fn beta() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "files": [
                // a.rs: valid edit (would succeed alone)
                {"path": "a.rs", "edits": [{"old_text": "fn alpha()", "new_text": "fn ALPHA()"}]},
                // b.rs: old_text absent → Phase-1 validation failure
                {"path": "b.rs", "edits": [{"old_text": "DEFINITELY_NOT_IN_FILE", "new_text": "boom"}]}
            ]
        });

        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        // b.rs should be marked failed
        assert!(text.contains("✗ b.rs"), "output: {}", text);

        // CRITICAL: a.rs must NOT have been written to disk.
        // Phase 1 catches b's validation failure before Phase 2 writes anything.
        let on_disk_a = tokio::fs::read_to_string(&file_a).await.unwrap();
        assert_eq!(
            on_disk_a,
            original_a,
            "a.rs MUST be unchanged — b.rs validation failure must prevent all disk writes (two-phase atomicity)"
        );
    }
}

#[cfg(test)]
mod anchored_tests {
    use super::*;
    use crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER;
    use crate::agent::tools::builtin::file::ReadFileTool;
    use crate::agent::tools::tool::{Tool, ToolError, ToolErrorKind};
    use tempfile::tempdir;

    /// Read the file via ReadFileTool (populates the anchor state manager) and
    /// return the full anchor strings (`<token>§<line>`) for each line.
    async fn read_and_collect_anchors(dir: &std::path::Path, rel: &str) -> Vec<String> {
        let reader = ReadFileTool::new(dir.to_path_buf());
        let out = reader
            .execute(serde_json::json!({ "path": rel }))
            .await
            .unwrap();
        let content = out.result["content"].as_str().unwrap().to_string();
        // Skip the [File Hash:] header; each remaining line is `<token>§<line>`.
        content
            .split('\n')
            .skip(1)
            .filter(|l| l.contains('§'))
            .map(|l| l.to_string())
            .collect()
    }

    // ── Test 5.8 — anchored edit happy path (byte-equal match) ──
    #[tokio::test]
    async fn anchored_edit_byte_equal_pass() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("anch.rs");
        tokio::fs::write(&file_path, "fn foo() {\n    old_body();\n}\n").await.unwrap();

        let anchors = read_and_collect_anchors(dir.path(), "anch.rs").await;
        // anchors[1] corresponds to "    old_body();"
        let anchor = anchors[1].clone();
        assert!(anchor.ends_with("§    old_body();"), "anchor: {:?}", anchor);

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "anch.rs",
            "edits": [{ "anchor": anchor, "new_text": "    new_body();" }]
        });
        let result = tool.execute(params).await.unwrap();
        assert!(result.result["ok"].as_bool().unwrap_or(false), "edit should succeed: {:?}", result.result);

        let after = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(after.contains("new_body();"), "file: {}", after);
        assert!(!after.contains("old_body();"), "file: {}", after);
        assert!(after.contains("fn foo() {"), "surrounding lines intact: {}", after);
    }

    // ── Test (extra) — anchored insert_after places a new line below ──
    #[tokio::test]
    async fn anchored_edit_insert_after() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("ins.rs");
        tokio::fs::write(&file_path, "line_a\nline_b\nline_c\n").await.unwrap();

        let anchors = read_and_collect_anchors(dir.path(), "ins.rs").await;
        let anchor = anchors[1].clone(); // "line_b"

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "ins.rs",
            "edits": [{ "anchor": anchor, "edit_type": "insert_after", "new_text": "INSERTED" }]
        });
        tool.execute(params).await.unwrap();

        let after = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(after, "line_a\nline_b\nINSERTED\nline_c\n", "insert_after result: {:?}", after);
    }

    // ── Test 5.9 — anchored edit byte mismatch → InvalidParams Expected/Provided ──
    #[tokio::test]
    async fn anchored_edit_byte_mismatch_fails() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mismatch.rs");
        tokio::fs::write(&file_path, "alpha\nbeta\ngamma\n").await.unwrap();

        let anchors = read_and_collect_anchors(dir.path(), "mismatch.rs").await;
        // Take the token from anchors[1] (line "beta") but provide WRONG content.
        let token = anchors[1].split('§').next().unwrap();
        let wrong_anchor = format!("{}§wrong content", token);

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "mismatch.rs",
            "edits": [{ "anchor": wrong_anchor, "new_text": "X" }]
        });
        let err = tool.execute(params).await.unwrap_err();
        match err {
            ToolError::InvalidParams(msg) => {
                assert!(msg.contains("Expected:"), "must show Expected: got {}", msg);
                assert!(msg.contains("Provided:"), "must show Provided: got {}", msg);
            }
            other => panic!("expected InvalidParams, got {:?}", other),
        }
        // File unchanged.
        let after = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(after, "alpha\nbeta\ngamma\n");
    }

    // ── Test (extra) — token not found → InvalidParams with re-read hint ──
    #[tokio::test]
    async fn anchored_edit_token_not_found_fails() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("notfound.rs");
        tokio::fs::write(&file_path, "one\ntwo\n").await.unwrap();
        let _ = read_and_collect_anchors(dir.path(), "notfound.rs").await;

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "notfound.rs",
            // "Zzqq" is a syntactically valid token that won't be in the file.
            "edits": [{ "anchor": "Zzqq§one", "new_text": "X" }]
        });
        let err = tool.execute(params).await.unwrap_err();
        match err {
            ToolError::InvalidParams(msg) => {
                assert!(msg.contains("not found"), "msg: {}", msg);
                assert!(msg.contains("read_file"), "must hint re-read: {}", msg);
            }
            other => panic!("expected InvalidParams, got {:?}", other),
        }
    }

    // ── Test 5.10 — edit rejects stale file with PreconditionFailed KIND ──
    #[tokio::test]
    async fn edit_tool_rejects_stale_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("stale.rs");
        tokio::fs::write(&file_path, "keep\n").await.unwrap();

        // Read to register + track the file.
        let _ = read_and_collect_anchors(dir.path(), "stale.rs").await;
        // Mark stale deterministically (simulates external modification).
        GLOBAL_FILE_CONTEXT_TRACKER.mark_stale(&file_path);

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "stale.rs",
            "edits": [{ "old_text": "keep", "new_text": "changed" }]
        });
        let err = tool.execute(params).await.unwrap_err();
        match err {
            ToolError::Kinded { kind, message, .. } => {
                assert_eq!(
                    kind,
                    ToolErrorKind::PreconditionFailed,
                    "stale-file reject MUST be PreconditionFailed kind, got {:?}",
                    kind
                );
                assert!(message.contains("modified externally"), "message: {}", message);
            }
            other => panic!("expected Kinded(PreconditionFailed), got {:?}", other),
        }

        // Clean up tracker state so other tests aren't affected.
        GLOBAL_FILE_CONTEXT_TRACKER.clear_stale(&file_path);
        // File must be unchanged.
        let after = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(after, "keep\n");
    }
}

// ---------------------------------------------------------------------------
// SP1 fuzzy-match integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fuzzy_integration_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;
    use tempfile::tempdir;

    // ── SP1.T1 — exact old_text applies identically (no regression) ──────────
    #[tokio::test]
    async fn fuzzy_exact_old_text_no_regression() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("exact.rs");
        tokio::fs::write(&file_path, "fn foo() {}\nfn bar() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "exact.rs",
            "edits": [{"old_text": "fn foo()", "new_text": "fn baz()"}]
        });
        let result = tool.execute(params).await.unwrap();
        assert!(result.result["ok"].as_bool().unwrap_or(false));

        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "fn baz() {}\nfn bar() {}\n", "content: {}", content);
    }

    // ── SP1.T2 — drifted old_text (extra leading whitespace) now applies ─────
    #[tokio::test]
    async fn fuzzy_drifted_old_text_whitespace_applies() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("drift.rs");
        tokio::fs::write(&file_path, "fn alpha() {\n    return 1;\n}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        // LLM produced old_text with wrong indentation (common drift case).
        let params = serde_json::json!({
            "path": "drift.rs",
            "edits": [{
                "old_text": "  fn alpha() {\n      return 1;\n  }\n",
                "new_text": "fn alpha() {\n    return 2;\n}\n"
            }]
        });
        let result = tool.execute(params).await.unwrap();
        assert!(
            result.result["ok"].as_bool().unwrap_or(false),
            "drifted edit should succeed via fuzzy: {:?}",
            result.result
        );

        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(content.contains("return 2"), "file content: {}", content);
        assert!(!content.contains("return 1"), "old content must be gone: {}", content);
    }

    // ── SP1.T3 — escape-drift \' or \" in old+new but not in file → blocked ──
    #[tokio::test]
    async fn fuzzy_escape_drift_blocked() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("escape.py");
        // File has actual single quote in content.
        tokio::fs::write(&file_path, "x = don't panic\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        // Both old_text and new_text have \' (escape drift). The file region has '.
        // This should be blocked with a clear error (or not-found if no strategy matches).
        let params = serde_json::json!({
            "path": "escape.py",
            "edits": [{
                "old_text": "x = don\\'t panic",
                "new_text": "x = don\\'t worry"
            }]
        });
        let result = tool.execute(params).await;
        // Should either be a clear error (escape-drift or not-found),
        // NOT a silent successful write of backslash-escaped content.
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("drift") || msg.contains("not found") || msg.contains("Could not"),
                    "expected drift/not-found error, got: {}",
                    msg
                );
            }
            Ok(r) => {
                // If the edit somehow succeeded, the file must not contain \'
                let content = tokio::fs::read_to_string(&file_path).await.unwrap();
                assert!(
                    !content.contains("\\'"),
                    "escape drift must not be written to file: {}",
                    content
                );
                let _ = r; // suppress unused warning
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SP2 edit_verify integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod sp2_verify_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;
    use tempfile::tempdir;

    // ── SP2.T1 — normal edit applies + read-back passes (no regression) ──────
    #[tokio::test]
    async fn sp2_normal_edit_no_lint_warning() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        tokio::fs::write(&file_path, "fn foo() {}\n").await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        let params = serde_json::json!({
            "path": "main.rs",
            "edits": [{"old_text": "fn foo()", "new_text": "fn bar()"}]
        });
        let result = tool.execute(params).await.unwrap();
        assert!(
            result.result["ok"].as_bool().unwrap_or(false),
            "normal edit should succeed: {:?}",
            result.result
        );
        let text = result.result["content"].as_str().unwrap();
        // No lint warning for .rs files
        assert!(
            !text.contains("⚠ lint:"),
            "normal .rs edit should have no lint warning: {}",
            text
        );
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(content.contains("fn bar()"), "file content: {}", content);
    }

    // ── SP2.T2 — edit breaks .json → Applied WITH lint_warning (still succeeds) ──
    #[tokio::test]
    async fn sp2_edit_breaks_json_lint_warning_attached() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        // Start with valid JSON
        tokio::fs::write(&file_path, r#"{"key": "value"}"#).await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        // Edit to introduce a JSON syntax error (missing closing brace)
        let params = serde_json::json!({
            "path": "config.json",
            "edits": [{"old_text": r#"{"key": "value"}"#, "new_text": r#"{"key": "value""#}]
        });
        let result = tool.execute(params).await.unwrap(); // must SUCCEED (lint is advisory)
        assert!(
            result.result["ok"].as_bool().unwrap_or(false),
            "edit breaking JSON should still succeed (lint advisory): {:?}",
            result.result
        );
        let text = result.result["content"].as_str().unwrap();
        assert!(
            text.contains("⚠ lint:"),
            "broken JSON edit should have lint warning: {}",
            text
        );
        assert!(
            text.contains("json"),
            "lint warning should mention format: {}",
            text
        );
    }

    // ── SP2.T3 — edit to already-broken .json staying broken → NO lint_warning ──
    //
    // Scenario: the file was ALREADY invalid JSON before this edit. We replace
    // text with a same-length replacement so the column of the parse error stays
    // identical in pre and post. The incremental filter compares the two error
    // strings: they are equal → pre-existing breakage → suppress warning.
    //
    // "value" (5 chars) → "other" (5 chars): trailing-comma error stays at same col.
    #[tokio::test]
    async fn sp2_preexisting_broken_json_no_lint_warning() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("bad.json");
        // Broken JSON with trailing comma — invalid per JSON spec.
        // Error: "trailing comma at line 1 column 17" (position of `}` after `,`).
        let pre = "{\"key\": \"value\",}";
        tokio::fs::write(&file_path, pre).await.unwrap();

        let tool = EditTool::new(dir.path().to_path_buf());
        // Replace "value" (5 chars) with "other" (5 chars) — same-length substitution.
        // The trailing comma stays at col 17 in both pre and post → identical error string.
        let params = serde_json::json!({
            "path": "bad.json",
            "edits": [{"old_text": "value", "new_text": "other"}]
        });
        let result = tool.execute(params).await.unwrap();
        let text = result.result["content"].as_str().unwrap();

        // Verify the file was actually changed (edit landed)
        let on_disk = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(on_disk.contains("other"), "edit should have applied: {}", on_disk);

        // Verify no lint warning: pre-existing breakage (same error at same position) is suppressed
        assert!(
            !text.contains("⚠ lint:"),
            "pre-existing broken JSON (same error, same column) should produce NO lint warning: {}",
            text
        );
    }
}
