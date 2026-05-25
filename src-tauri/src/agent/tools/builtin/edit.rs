use async_trait::async_trait;
use similar::TextDiff;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

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

#[derive(serde::Deserialize, Debug, Clone)]
struct EditArg {
    #[serde(default)]
    old_text: String,
    new_text: String,
    insert_line: Option<u32>,
    anchor: Option<String>,
    end_anchor: Option<String>,
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
    Applied { path: String, diff: String, edit_count: usize },
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
        edits_val: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let edits = edits_val
            .as_array()
            .ok_or_else(|| ToolError::InvalidParams("edits must be an array".into()))?;

        if edits.is_empty() {
            return Err(ToolError::InvalidParams("edits array is empty".into()));
        }

        let full_path = self.resolve_path(&path);

        // Active File External Change Watcher check
        if crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(ToolError::Execution(
                "File has been modified externally by the user. Run read_file tool to synchronize.".into(),
            ));
        }

        info!(path = %full_path.display(), edits = edits.len(), "Applying edits");

        // Read the original content
        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        let mut content = original.clone();
        let mut resolved_edits: Vec<ResolvedEdit> = Vec::new();
        let mut current_search_pos = 0;

        for (i, edit) in edits.iter().enumerate() {
            let old_text = edit
                .get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::InvalidParams(format!(
                        "edits[{}].old_text is required and must be a string",
                        i
                    ))
                })?;
            let new_text = edit
                .get("new_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::InvalidParams(format!(
                        "edits[{}].new_text is required and must be a string",
                        i
                    ))
                })?;
            let insert_line = edit.get("insert_line").and_then(|v| v.as_u64());
            let anchor = edit.get("anchor").and_then(|v| v.as_str());
            let end_anchor = edit.get("end_anchor").and_then(|v| v.as_str());

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                    .get_anchors(&full_path)
                    .unwrap_or_else(|| {
                        let a = crate::agent::anchor_state::initialize_anchors(&lines);
                        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                            .register_file_lines(&full_path, &lines);
                        a
                    });

                let start_idx = anchors.iter().position(|r| r == anchor_str).ok_or_else(|| {
                    ToolError::Execution(format!(
                        "Start anchor '{}' not found in file. Make sure you have the correct anchor.",
                        anchor_str
                    ))
                })?;

                let end_idx = if let Some(end_anchor_str) = end_anchor {
                    anchors
                        .iter()
                        .skip(start_idx)
                        .position(|r| r == end_anchor_str)
                        .map(|p| start_idx + p)
                        .ok_or_else(|| {
                            ToolError::Execution(format!(
                                "End anchor '{}' not found after start anchor in file.",
                                end_anchor_str
                            ))
                        })?
                } else {
                    start_idx
                };

                let (start_pos, end_pos) = find_line_char_range(&content, start_idx, end_idx);
                let mut formatted_new_text = new_text.to_string();
                if !formatted_new_text.ends_with('\n')
                    && content[start_pos..end_pos].ends_with('\n')
                {
                    formatted_new_text.push('\n');
                }

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: formatted_new_text,
                });
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
                let mut pos = content[current_search_pos..]
                    .find(old_text)
                    .map(|p| p + current_search_pos);
                if pos.is_none() {
                    pos = content.find(old_text);
                }

                let start_pos = pos.ok_or_else(|| {
                    ToolError::Execution(format!(
                        "old_text '{}' not found in file. Make sure the text matches exactly including whitespace and indentation.",
                        old_text
                    ))
                })?;
                let end_pos = start_pos + old_text.len();
                current_search_pos = end_pos;

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: new_text.to_string(),
                });
            }
        }

        resolved_edits.sort_by(|a, b| {
            b.start_pos
                .cmp(&a.start_pos)
                .then_with(|| b.end_pos.cmp(&a.end_pos))
        });

        let mut applied = 0;
        for re in resolved_edits {
            let mut new_content = String::with_capacity(content.len() + re.new_text.len());
            new_content.push_str(&content[..re.start_pos]);
            new_content.push_str(&re.new_text);
            new_content.push_str(&content[re.end_pos..]);
            content = new_content;
            applied += 1;
        }

        crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER
            .register_expected_write(&full_path);

        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        let old_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
            .align_file_anchors(&full_path, &old_lines, &new_lines);

        let diff = Self::generate_diff(&original, &content, &path);
        let summary = format!(
            "Applied {} edit(s) to {}\n\n{}",
            applied,
            full_path.display(),
            diff
        );

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

        if crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(ToolError::Execution(
                "File has been modified externally by the user. Run read_file tool to synchronize.".into(),
            ));
        }

        // Read file — validates existence and readability; NO disk write
        let content = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        let mut current_search_pos = 0usize;
        for edit in edits.iter() {
            let old_text = &edit.old_text;
            let insert_line = edit.insert_line;
            let anchor = edit.anchor.as_deref();
            let end_anchor = edit.end_anchor.as_deref();

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                    .get_anchors(&full_path)
                    .unwrap_or_else(|| {
                        let a = crate::agent::anchor_state::initialize_anchors(&lines);
                        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                            .register_file_lines(&full_path, &lines);
                        a
                    });

                let start_idx = anchors.iter().position(|r| r == anchor_str).ok_or_else(|| {
                    ToolError::Execution(format!(
                        "Start anchor '{}' not found in file. Make sure you have the correct anchor.",
                        anchor_str
                    ))
                })?;

                if let Some(end_anchor_str) = end_anchor {
                    anchors
                        .iter()
                        .skip(start_idx)
                        .position(|r| r == end_anchor_str)
                        .ok_or_else(|| {
                            ToolError::Execution(format!(
                                "End anchor '{}' not found after start anchor in file.",
                                end_anchor_str
                            ))
                        })?;
                }
            } else if old_text.is_empty() {
                // Insert mode — any insert_line value is accepted (apply clamps)
            } else {
                // Search-replace: verify old_text exists
                let pos = content[current_search_pos..]
                    .find(old_text.as_str())
                    .map(|p| p + current_search_pos)
                    .or_else(|| content.find(old_text.as_str()));

                match pos {
                    Some(p) => {
                        current_search_pos = p + old_text.len();
                    }
                    None => {
                        return Err(ToolError::Execution(format!(
                            "old_text '{}' not found in file. Make sure the text matches exactly including whitespace and indentation.",
                            old_text
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Phase 2 — apply the edits to the file on disk and return `(diff, edit_count)`.
    /// Caller MUST have called `validate_single_file` successfully first.
    async fn apply_validated_single_file(
        &self,
        path: String,
        edits: Vec<EditArg>,
    ) -> Result<(String, usize), ToolError> {
        let full_path = self.resolve_path(&path);

        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        let mut content = original.clone();
        let mut resolved_edits: Vec<ResolvedEdit> = Vec::new();
        let mut current_search_pos = 0;

        for (i, edit) in edits.iter().enumerate() {
            let old_text = &edit.old_text;
            let new_text = &edit.new_text;
            let insert_line = edit.insert_line;
            let anchor = edit.anchor.as_deref();
            let end_anchor = edit.end_anchor.as_deref();

            if let Some(anchor_str) = anchor {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                    .get_anchors(&full_path)
                    .unwrap_or_else(|| {
                        let a = crate::agent::anchor_state::initialize_anchors(&lines);
                        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                            .register_file_lines(&full_path, &lines);
                        a
                    });

                let start_idx = anchors.iter().position(|r| r == anchor_str).ok_or_else(|| {
                    ToolError::Execution(format!(
                        "Start anchor '{}' not found in file. Make sure you have the correct anchor.",
                        anchor_str
                    ))
                })?;

                let end_idx = if let Some(end_anchor_str) = end_anchor {
                    anchors
                        .iter()
                        .skip(start_idx)
                        .position(|r| r == end_anchor_str)
                        .map(|p| start_idx + p)
                        .ok_or_else(|| {
                            ToolError::Execution(format!(
                                "End anchor '{}' not found after start anchor in file.",
                                end_anchor_str
                            ))
                        })?
                } else {
                    start_idx
                };

                let (start_pos, end_pos) = find_line_char_range(&content, start_idx, end_idx);
                let mut formatted_new_text = new_text.clone();
                if !formatted_new_text.ends_with('\n')
                    && content[start_pos..end_pos].ends_with('\n')
                {
                    formatted_new_text.push('\n');
                }

                resolved_edits.push(ResolvedEdit { start_pos, end_pos, new_text: formatted_new_text });
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
                let mut pos = content[current_search_pos..]
                    .find(old_text.as_str())
                    .map(|p| p + current_search_pos);
                if pos.is_none() {
                    pos = content.find(old_text.as_str());
                }

                let start_pos = pos.ok_or_else(|| {
                    ToolError::Execution(format!(
                        "old_text '{}' not found in file. Make sure the text matches exactly including whitespace and indentation.",
                        old_text
                    ))
                })?;
                let end_pos = start_pos + old_text.len();
                current_search_pos = end_pos;

                resolved_edits.push(ResolvedEdit { start_pos, end_pos, new_text: new_text.clone() });
            }
        }

        resolved_edits.sort_by(|a, b| {
            b.start_pos
                .cmp(&a.start_pos)
                .then_with(|| b.end_pos.cmp(&a.end_pos))
        });

        let mut applied = 0;
        for re in resolved_edits {
            let mut new_content = String::with_capacity(content.len() + re.new_text.len());
            new_content.push_str(&content[..re.start_pos]);
            new_content.push_str(&re.new_text);
            new_content.push_str(&content[re.end_pos..]);
            content = new_content;
            applied += 1;
        }

        crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER
            .register_expected_write(&full_path);

        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        let old_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
            .align_file_anchors(&full_path, &old_lines, &new_lines);

        let diff = Self::generate_diff(&original, &content, &path);
        Ok((diff, applied))
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
            let mut results: Vec<FileBatchResult> = Vec::with_capacity(files.len());
            for (i, (file_arg, validation)) in
                files.iter().zip(validations.into_iter()).enumerate()
            {
                if i == fail_idx {
                    results.push(FileBatchResult::ValidationFailed {
                        path: file_arg.path.clone(),
                        error: validation.unwrap_err(),
                    });
                } else {
                    // Both pre-failure and post-failure entries are skipped —
                    // two-phase means all-or-nothing at Phase 2.
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
                Ok((diff, edit_count)) => {
                    results.push(FileBatchResult::Applied {
                        path: file_arg.path,
                        diff,
                        edit_count,
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
                FileBatchResult::Applied { path, edit_count, .. } => {
                    summary.push_str(&format!("✓ {}: {} edit(s) applied\n", path, edit_count));
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
        "Edit one or more files via search-replace, line insertion, or anchor-targeted edits. \
         Use `files: [{path, edits}]` to batch edits across files in a single call — \
         reduces LLM round-trips. Legacy `{path, edits}` form for single-file edits also works."
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
                                        "anchor": { "type": "string", "description": "Optional starting anchor for stateful Myers Diff alignment." },
                                        "end_anchor": { "type": "string", "description": "Optional ending anchor for stateful Myers Diff alignment." }
                                    },
                                    "required": ["old_text", "new_text"]
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
                            "anchor": { "type": "string", "description": "Optional starting anchor for stateful Myers Diff alignment." },
                            "end_anchor": { "type": "string", "description": "Optional ending anchor for stateful Myers Diff alignment." }
                        },
                        "required": ["old_text", "new_text"]
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
            self.execute_single_file(path, params["edits"].clone()).await
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
