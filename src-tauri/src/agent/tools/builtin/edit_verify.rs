// SPDX-License-Identifier: Apache-2.0
//! Post-edit verification (阶段 5 SP2): read-back byte-compare (catch silent
//! write failures) + incremental structured-format lint (JSON/YAML/TOML —
//! report only NEWLY-introduced parse breakage) + project-check advisory
//! (cargo/ruff/py_compile/tsc — time-boxed, file-scoped, never blocks an edit).

use std::path::Path;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A lint finding for a structured-format file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    /// Format name: `"json"`, `"yaml"`, or `"toml"`.
    pub format: &'static str,
    /// Human-readable message with line/col when available.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Read-back verify (hard error on mismatch)
// ---------------------------------------------------------------------------

/// Re-read `path` and confirm its on-disk content equals `expected`.
///
/// Returns `Err` if the file cannot be read or if the content differs from
/// `expected` (indicating the write silently failed — encoding slip, partial
/// write, permissions change, etc.).
///
/// The error message reports expected/actual byte lengths and a divergence
/// hint (NOT the full content, which could be large).
pub async fn read_back_verify(path: &Path, expected: &str) -> anyhow::Result<()> {
    let actual = tokio::fs::read_to_string(path).await.map_err(|e| {
        anyhow::anyhow!(
            "read-back verify: could not re-read {}: {}",
            path.display(),
            e
        )
    })?;

    if actual != expected {
        let expected_len = expected.len();
        let actual_len = actual.len();

        // Build a short divergence hint: find the first byte position that
        // differs, show a small context window around it.
        let diverge_hint = first_divergence_hint(expected, &actual);

        Err(anyhow::anyhow!(
            "read-back verify failed for {}: expected {} bytes, got {} bytes; {}",
            path.display(),
            expected_len,
            actual_len,
            diverge_hint
        ))
    } else {
        Ok(())
    }
}

/// Build a short divergence hint without leaking full file content.
fn first_divergence_hint(expected: &str, actual: &str) -> String {
    let expected_bytes = expected.as_bytes();
    let actual_bytes = actual.as_bytes();
    let diverge_pos = expected_bytes
        .iter()
        .zip(actual_bytes.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected_bytes.len().min(actual_bytes.len()));

    let start = diverge_pos.saturating_sub(20);
    let end = (diverge_pos + 20).min(expected.len());
    let snippet = expected
        .get(start..end)
        .unwrap_or("")
        .replace('\n', "↵")
        .replace('\r', "↲");

    format!(
        "first divergence at byte {} (context: {:?}…)",
        diverge_pos, snippet
    )
}

// ---------------------------------------------------------------------------
// Incremental structured lint (advisory)
// ---------------------------------------------------------------------------

/// In-process incremental structured lint.
///
/// Returns a `LintFinding` ONLY when:
///  - the file extension is `.json`, `.yaml`, `.yml`, or `.toml`
///  - the post-edit content is parse-invalid for that format
///  - AND this breakage is **newly introduced**: either the pre-edit content
///    was valid, or the pre-edit content had a *different* parse error.
///
/// Returns `None` for:
///  - non-structured extensions (`.rs`, `.ts`, `.py`, etc.)
///  - valid post-edit content
///  - pre-existing breakage (pre invalid with the SAME error as post)
///  - edit that FIXED a previously-broken file (pre invalid → post valid)
pub fn incremental_structured_lint(path: &Path, pre: &str, post: &str) -> Option<LintFinding> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("json") => {
            let post_err = lint_json(post)?; // None = valid → return None
            let pre_result = lint_json(pre);
            if is_newly_broken(pre_result.as_deref(), &post_err) {
                Some(LintFinding { format: "json", message: post_err })
            } else {
                None
            }
        }
        Some("yaml") | Some("yml") => {
            let post_err = lint_yaml(post)?;
            let pre_result = lint_yaml(pre);
            if is_newly_broken(pre_result.as_deref(), &post_err) {
                Some(LintFinding { format: "yaml", message: post_err })
            } else {
                None
            }
        }
        Some("toml") => {
            let post_err = lint_toml(post)?;
            let pre_result = lint_toml(pre);
            if is_newly_broken(pre_result.as_deref(), &post_err) {
                Some(LintFinding { format: "toml", message: post_err })
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Private lint helpers — parse → Result<(), String with line/col>
// ---------------------------------------------------------------------------

/// Returns `None` if valid JSON, `Some(err_message)` if invalid.
fn lint_json(content: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => None,
        // serde_json's Display already carries "... at line N column M" —
        // use it directly to avoid double-encoding the position.
        Err(e) => Some(e.to_string()),
    }
}

/// Returns `None` if valid YAML, `Some(err_message)` if invalid.
fn lint_yaml(content: &str) -> Option<String> {
    match serde_yml::from_str::<serde_yml::Value>(content) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    }
}

/// Returns `None` if valid TOML, `Some(err_message)` if invalid.
fn lint_toml(content: &str) -> Option<String> {
    match toml::from_str::<toml::Value>(content) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    }
}

/// Strip a trailing " at line N column M" (serde_json's Display position
/// suffix) so two instances of the SAME structural error at different
/// positions compare equal. Makes the incremental filter robust to line
/// shifts from unrelated edits (e.g. inserting a valid line above a
/// pre-existing error shifts its reported position without changing its kind).
///
/// For YAML/TOML whose Display format does not embed `" at line "` in the
/// same way, this is a no-op (returns the full string) — acceptable; JSON is
/// the common case and the one where serde_json reliably appends the suffix.
fn normalize_err(e: &str) -> &str {
    e.find(" at line ").map(|i| &e[..i]).unwrap_or(e)
}

/// Returns `true` when the post-edit error is "newly introduced":
///  - pre was valid (pre_err is None) — new breakage
///  - pre had a DIFFERENT error kind — changed breakage (treat as new, not
///    pre-existing). Position shifts of the SAME error kind are suppressed
///    via `normalize_err`, so a pre-existing error that merely moves to a
///    new line/column is not falsely treated as new breakage.
///
/// Returns `false` when pre had the same error kind — pre-existing, suppress.
fn is_newly_broken(pre_err: Option<&str>, post_err: &str) -> bool {
    match pre_err {
        None => true,  // pre was valid → post broke it
        Some(e) => normalize_err(e) != normalize_err(post_err),  // suppress same kind at shifted position
    }
}

// ---------------------------------------------------------------------------
// Project-check advisory (time-boxed, per-language, never blocks an edit)
// ---------------------------------------------------------------------------

/// Config for the per-edit project check (held by `EditTool` as `Option<ProjectCheckCfg>`; `None` = disabled).
#[derive(Debug, Clone)]
pub struct ProjectCheckCfg {
    /// Maximum seconds to wait for the checker process. Clamped to ≥ 1.
    pub timeout_secs: u64,
}

/// Advisory result of a project check.
///
/// `message` contains formatted diagnostics filtered to the edited file.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckFinding {
    /// Language / runner label: `"rust"`, `"python"`, or `"typescript"`.
    pub language: &'static str,
    /// Human-readable diagnostic lines (capped, with header).
    pub message: String,
}

/// Cap applied to the number of diagnostic lines in a `CheckFinding`.
const DIAG_LINE_CAP: usize = 30;

/// Best-effort, time-boxed project check for the edited file.
///
/// Returns `Some(finding)` ONLY when:
/// - the file extension maps to a known runner,
/// - the chosen tool is present on PATH (or in the workspace),
/// - the tool completes within `cfg.timeout_secs`, AND
/// - at least one diagnostic mentions `path`.
///
/// Any of: unknown extension / tool absent / spawn error / timeout /
/// no diagnostics in this file → `None`.  NEVER blocks or errors the edit.
pub async fn project_check(
    path: &Path,
    workspace_root: &Path,
    cfg: &ProjectCheckCfg,
) -> Option<CheckFinding> {
    use std::process::Stdio;
    use tokio::process::Command;

    let timeout_dur = Duration::from_secs(cfg.timeout_secs.max(1));

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        // ── Rust: cargo check --message-format=json ──────────────────────────
        Some("rs") => {
            let mut cmd = Command::new("cargo");
            cmd.args(["check", "--message-format=json"])
                .current_dir(workspace_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true);

            let output = match tokio::time::timeout(timeout_dur, cmd.output()).await {
                Ok(Ok(o)) => o,
                _ => return None, // timeout or spawn error
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines = parse_cargo_diagnostics(&stdout, workspace_root, path);
            format_finding("rust", lines)
        }

        // ── Python: ruff (preferred) → py_compile (fallback) ─────────────────
        Some("py") => {
            // Probe ruff (time-boxed like every other spawn — never block the edit)
            let ruff_ok = matches!(
                tokio::time::timeout(
                    timeout_dur,
                    Command::new("ruff")
                        .arg("--version")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .stdin(Stdio::null())
                        .kill_on_drop(true)
                        .output(),
                )
                .await,
                Ok(Ok(_))
            );

            if ruff_ok {
                let mut cmd = Command::new("ruff");
                cmd.args(["check", "--output-format=json"])
                    .arg(path)
                    .current_dir(workspace_root)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdin(Stdio::null())
                    .kill_on_drop(true);

                match tokio::time::timeout(timeout_dur, cmd.output()).await {
                    Ok(Ok(o)) => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        let lines = parse_ruff_diagnostics(&stdout, path);
                        if lines.is_empty() {
                            // ruff produced no parseable output → fall through
                        } else {
                            return format_finding("python", lines);
                        }
                    }
                    _ => {} // timeout or error → fall through to py_compile
                }
            }

            // Fallback: python3 -m py_compile <path>
            let mut cmd = Command::new("python3");
            cmd.args(["-m", "py_compile"])
                .arg(path)
                .current_dir(workspace_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true);

            match tokio::time::timeout(timeout_dur, cmd.output()).await {
                Ok(Ok(o)) if o.status.success() => None,
                Ok(Ok(o)) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let msg = stderr.trim().to_string();
                    if msg.is_empty() {
                        None
                    } else {
                        format_finding("python", vec![msg])
                    }
                }
                _ => None, // spawn failure / timeout
            }
        }

        // ── TypeScript/JavaScript: tsc --noEmit ──────────────────────────────
        Some("ts") | Some("tsx") | Some("js") | Some("jsx") => {
            // Only run if a tsconfig.json exists in the workspace root
            if !workspace_root.join("tsconfig.json").exists() {
                return None;
            }

            let mut cmd = Command::new("npx");
            cmd.args(["tsc", "--noEmit"])
                .current_dir(workspace_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true);

            match tokio::time::timeout(timeout_dur, cmd.output()).await {
                Ok(Ok(o)) => {
                    let combined = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    );
                    let lines = parse_tsc_diagnostics(&combined, path);
                    format_finding("typescript", lines)
                }
                _ => None, // timeout (expected for large projects) or spawn error
            }
        }

        // ── Unknown extension ─────────────────────────────────────────────────
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pure, sync, I/O-free parsers (testable without spawning)
// ---------------------------------------------------------------------------

/// Parse cargo `--message-format=json` stdout.
///
/// Returns `Vec<String>` of `"{line}: {message}"` for every `compiler-message`
/// at level `"error"` whose span resolves to `edited`.
pub fn parse_cargo_diagnostics(
    stdout: &str,
    workspace_root: &Path,
    edited: &Path,
) -> Vec<String> {
    let edited_canon = edited.canonicalize().ok();

    let mut results = Vec::new();

    for raw_line in stdout.lines() {
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            continue;
        }

        // Be tolerant: skip lines that aren't valid JSON objects
        let obj: serde_json::Value = match serde_json::from_str(raw_line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only process compiler-message objects
        if obj.get("reason").and_then(|v| v.as_str()) != Some("compiler-message") {
            continue;
        }

        let msg = match obj.get("message") {
            Some(m) => m,
            None => continue,
        };

        // Only errors
        if msg.get("level").and_then(|v| v.as_str()) != Some("error") {
            continue;
        }

        let message_text = msg
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Check spans for a match against the edited file
        let spans = match msg.get("spans").and_then(|v| v.as_array()) {
            Some(s) => s,
            None => continue,
        };

        for span in spans {
            let file_name = match span.get("file_name").and_then(|v| v.as_str()) {
                Some(f) => f,
                None => continue,
            };

            let span_path = workspace_root.join(file_name);
            let matches = match (span_path.canonicalize().ok(), &edited_canon) {
                (Some(sc), Some(ec)) => &sc == ec,
                _ => {
                    // Fall back to comparing file name components (char-safe)
                    let span_fname = std::path::Path::new(file_name)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("");
                    let edited_fname = edited
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("");
                    !span_fname.is_empty() && span_fname == edited_fname
                }
            };

            if matches {
                let line_start = span
                    .get("line_start")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                // Use char-safe operations on the message text
                results.push(format!("{line_start}: {message_text}"));
                break; // one entry per diagnostic (first matching span)
            }
        }
    }

    results
}

/// Parse `ruff check --output-format=json` stdout.
///
/// Returns `Vec<String>` of `"{row}: {code} {message}"` for entries whose
/// `.filename` matches `edited`.
pub fn parse_ruff_diagnostics(stdout: &str, edited: &Path) -> Vec<String> {
    let edited_canon = edited.canonicalize().ok();
    let edited_fname = edited
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    // ruff outputs a JSON array
    let arr: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let entries = match arr.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries {
        let filename = match entry.get("filename").and_then(|v| v.as_str()) {
            Some(f) => f,
            None => continue,
        };

        let entry_path = std::path::Path::new(filename);
        let matches = match (entry_path.canonicalize().ok(), &edited_canon) {
            (Some(ec), Some(ed)) => &ec == ed,
            _ => {
                // Fall back to file name comparison
                let entry_fname = entry_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                !edited_fname.is_empty() && entry_fname == edited_fname
            }
        };

        if matches {
            let row = entry
                .get("location")
                .and_then(|l| l.get("row"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let code = entry
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let message = entry
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            results.push(format!("{row}: {code} {message}"));
        }
    }

    results
}

/// Parse `tsc --noEmit` output (stdout+stderr combined).
///
/// tsc lines look like: `path/to/file.ts(line,col): error TSxxxx: message`
/// Returns `Vec<String>` of `"{line}: {message}"` for lines matching `edited`.
pub fn parse_tsc_diagnostics(output: &str, edited: &Path) -> Vec<String> {
    let edited_str = edited.to_string_lossy();
    let edited_fname = edited
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    let mut results = Vec::new();

    for line in output.lines() {
        // tsc format: `<path>(<line>,<col>): error TS<N>: <msg>`
        // Find the `(` that starts the position tuple
        let paren_pos = match line.find('(') {
            Some(p) => p,
            None => continue,
        };

        let file_part = &line[..paren_pos];

        // Check if this line refers to the edited file
        // Compare by suffix against the edited path or by file name
        let matches = file_part.ends_with(edited_str.as_ref())
            || (!edited_fname.is_empty()
                && std::path::Path::new(file_part)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|f| f == edited_fname)
                    .unwrap_or(false));

        if !matches {
            continue;
        }

        // Extract line number from `(<line>,<col>):`
        let after_paren = &line[paren_pos + 1..];
        let line_num = after_paren
            .split(',')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Extract message after `): error TSxxxx: `
        let msg_part = line
            .find("): ")
            .map(|i| line[i + 3..].trim())
            .unwrap_or(line.trim());

        results.push(format!("{line_num}: {msg_part}"));
    }

    results
}

/// Assemble a `CheckFinding` from diagnostic lines.
///
/// Returns `None` if `lines` is empty. Applies a [`DIAG_LINE_CAP`]-line cap
/// and prepends a summary header. Uses only char-safe operations on the strings
/// so multibyte / CJK content never causes a panic.
pub fn format_finding(language: &'static str, lines: Vec<String>) -> Option<CheckFinding> {
    if lines.is_empty() {
        return None;
    }

    let total = lines.len();
    let (kept, truncated) = if total > DIAG_LINE_CAP {
        (&lines[..DIAG_LINE_CAP], total - DIAG_LINE_CAP)
    } else {
        (&lines[..], 0)
    };

    let header = format!(
        "{language} check found {total} issue(s) in this file (may include pre-existing):"
    );

    let mut body = kept.join("\n");
    if truncated > 0 {
        body.push_str(&format!("\n… ({truncated} more)"));
    }

    Some(CheckFinding {
        language,
        message: format!("{header}\n{body}"),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::NamedTempFile;
    use tokio::fs;

    // ── Helper: create a temp file with given content ────────────────────────
    async fn tmp_with(content: &str) -> NamedTempFile {
        let f = NamedTempFile::new().unwrap();
        fs::write(f.path(), content).await.unwrap();
        f
    }

    // ── read_back_verify: matching content → Ok ──────────────────────────────
    #[tokio::test]
    async fn read_back_matching_content_ok() {
        let content = "fn main() {}\n";
        let f = tmp_with(content).await;
        let result = read_back_verify(f.path(), content).await;
        assert!(result.is_ok(), "matching content should be Ok: {:?}", result);
    }

    // ── read_back_verify: tampered content → Err ────────────────────────────
    #[tokio::test]
    async fn read_back_tampered_content_err() {
        let original = "fn main() {}\n";
        let tampered = "fn main() { /* extra */ }\n";
        let f = tmp_with(tampered).await; // write tampered, but verify against original
        let result = read_back_verify(f.path(), original).await;
        assert!(result.is_err(), "tampered content should return Err");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("read-back verify failed"),
            "error should say read-back verify failed: {}",
            msg
        );
    }

    // ── read_back_verify: shorter actual → Err with length info ─────────────
    #[tokio::test]
    async fn read_back_short_content_err_with_lengths() {
        let expected = "fn main() { /* complete */ }\n";
        let short = "fn main() {\n"; // shorter
        let f = tmp_with(short).await;
        let result = read_back_verify(f.path(), expected).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        // Should report byte lengths
        assert!(
            msg.contains("bytes"),
            "error should mention bytes: {}",
            msg
        );
    }

    // ── read_back_verify: nonexistent path → Err ────────────────────────────
    #[tokio::test]
    async fn read_back_nonexistent_path_err() {
        let result = read_back_verify(
            Path::new("/nonexistent/path/file.txt"),
            "content"
        ).await;
        assert!(result.is_err());
    }

    // ── lint: valid → invalid JSON → Some(finding) ──────────────────────────
    #[test]
    fn lint_valid_to_invalid_json_some() {
        let pre = r#"{"key": "value"}"#;
        let post = r#"{"key": "value""#; // missing closing brace
        let path = Path::new("data.json");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_some(), "valid→invalid JSON should return Some");
        let finding = result.unwrap();
        assert_eq!(finding.format, "json");
        assert!(!finding.message.is_empty());
    }

    // ── lint: valid → valid JSON → None ─────────────────────────────────────
    #[test]
    fn lint_valid_to_valid_json_none() {
        let pre = r#"{"key": "value"}"#;
        let post = r#"{"key": "new_value", "extra": 42}"#;
        let path = Path::new("config.json");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_none(), "valid→valid JSON should return None");
    }

    // ── lint: invalid → invalid (same error) → None (pre-existing) ──────────
    #[test]
    fn lint_preexisting_invalid_json_none() {
        // Both pre and post have the same parse error — pre-existing breakage
        let pre = r#"{"broken":"#; // truncated
        let post = r#"{"broken":"#; // same invalid content
        let path = Path::new("bad.json");
        let result = incremental_structured_lint(path, pre, post);
        assert!(
            result.is_none(),
            "pre-existing breakage (same error) should return None, got: {:?}",
            result
        );
    }

    // ── lint: pre-invalid → post-invalid (different error) → Some ───────────
    #[test]
    fn lint_different_error_invalid_json_some() {
        // pre and post are both invalid but with DIFFERENT errors
        let pre = r#"{"key": "unclosed"#; // missing closing quote + brace
        let post = r#"[invalid json here"#; // completely different syntax error
        let path = Path::new("data.json");
        let result = incremental_structured_lint(path, pre, post);
        // Different errors → report as newly broken (changed breakage)
        assert!(
            result.is_some(),
            "different pre/post errors should return Some (changed breakage)"
        );
    }

    // ── lint: valid → invalid YAML → Some ───────────────────────────────────
    #[test]
    fn lint_valid_to_invalid_yaml_some() {
        let pre = "key: value\nother: 123\n";
        // YAML with tab indentation (invalid in YAML)
        let post = "key: value\n\t invalid_tab: here\n";
        let path = Path::new("config.yaml");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_some(), "valid→invalid YAML should return Some");
        let finding = result.unwrap();
        assert_eq!(finding.format, "yaml");
    }

    // ── lint: valid → invalid YAML (.yml extension) → Some ──────────────────
    #[test]
    fn lint_valid_to_invalid_yml_extension_some() {
        let pre = "name: test\nversion: 1\n";
        let post = "name: test\n  bad:\n   indent:\n    broken: [unclosed\n";
        let path = Path::new("manifest.yml");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_some(), "valid→invalid .yml should return Some");
        let finding = result.unwrap();
        assert_eq!(finding.format, "yaml");
    }

    // ── lint: valid → invalid TOML → Some ───────────────────────────────────
    #[test]
    fn lint_valid_to_invalid_toml_some() {
        let pre = "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\n";
        let post = "[package\nname = \"myapp\"\n"; // missing closing bracket
        let path = Path::new("Cargo.toml");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_some(), "valid→invalid TOML should return Some");
        let finding = result.unwrap();
        assert_eq!(finding.format, "toml");
    }

    // ── lint: non-structured extension (.rs) → None ──────────────────────────
    #[test]
    fn lint_non_structured_rs_none() {
        let pre = "fn main() {}\n";
        let post = "fn main() { invalid rust !@#$\n";
        let path = Path::new("main.rs");
        let result = incremental_structured_lint(path, pre, post);
        assert!(
            result.is_none(),
            "non-structured extension (.rs) should return None"
        );
    }

    // ── lint: non-structured extension (.ts) → None ──────────────────────────
    #[test]
    fn lint_non_structured_ts_none() {
        let pre = "const x = 1;";
        let post = "const x = { broken";
        let path = Path::new("app.ts");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_none(), ".ts should return None");
    }

    // ── lint: pre-invalid → post-valid (edit FIXED it) → None ───────────────
    #[test]
    fn lint_pre_invalid_post_valid_none() {
        let pre = r#"{"broken": "#; // invalid JSON
        let post = r#"{"fixed": "value"}"#; // valid JSON
        let path = Path::new("config.json");
        let result = incremental_structured_lint(path, pre, post);
        assert!(
            result.is_none(),
            "edit that FIXED broken JSON should return None"
        );
    }

    // ── lint: JSON error message contains line/col ───────────────────────────
    #[test]
    fn lint_json_error_has_line_col() {
        let pre = "{}";
        // Multi-line JSON to verify line number reporting
        let post = "{\n  \"key\": \n}"; // invalid: missing value
        let path = Path::new("data.json");
        let result = incremental_structured_lint(path, pre, post);
        assert!(result.is_some());
        let finding = result.unwrap();
        // serde_json includes "line N, col M" in our formatted message
        assert!(
            finding.message.contains("line") || finding.message.contains("col"),
            "JSON error should contain line/col info: {}",
            finding.message
        );
    }

    // ── lint: no extension → None ────────────────────────────────────────────
    #[test]
    fn lint_no_extension_none() {
        let path = Path::new("Makefile");
        let result = incremental_structured_lint(path, "x = 1", "broken {{{");
        assert!(result.is_none(), "no extension should return None");
    }

    // ── lint: pre-existing error that SHIFTS position → None (suppressed) ────
    //
    // This is the general case the SP2.T3 test in edit.rs only partially
    // covered via a same-length-replacement workaround.  Here the POST edit
    // inserts a valid line ABOVE the pre-existing error, shifting it to a
    // different line/column.  normalize_err strips the position suffix so
    // the two error KINDS compare equal → pre-existing breakage → suppress.
    #[test]
    fn lint_preexisting_error_shifted_position_suppressed() {
        let path = Path::new("data.json");

        // PRE: trailing comma — serde_json reports something like
        // "trailing comma at line 1 column 18".
        let pre = r#"{"key": "value",}"#;

        // POST: a valid line is prepended (conceptually the model inserted
        // content before this object).  We simulate the error SHIFTING by
        // embedding the same broken object at a different position inside a
        // larger string — so the raw error string (with position) differs,
        // but the error KIND (trailing comma) is the same.
        //
        // Easiest to construct: wrap in an array where the broken object is
        // now at column/line 3 rather than 1.  The structural error kind
        // ("trailing comma") stays the same; only the reported position changes.
        let post = "[\n  {\"key\": \"value\",}\n]";

        // Both pre and post produce a "trailing comma" error; only the
        // line/column in the Display string differs.  normalize_err must
        // suppress this as pre-existing breakage.
        let result = incremental_structured_lint(path, pre, post);
        assert!(
            result.is_none(),
            "pre-existing error that merely shifted position should be suppressed (None), got: {:?}",
            result
        );
    }

    // =========================================================================
    // project_check unit tests (pure parsers — no I/O, no spawning)
    // =========================================================================

    // ── T1: parse_cargo_diagnostics — matching + non-matching spans ──────────
    #[test]
    fn cargo_diag_keeps_only_matching_file() {
        // Two compiler-message/error lines: one for our edited file, one for another.
        // Plus a non-error and a non-compiler-message line.
        let workspace = Path::new("/project");
        let edited = Path::new("/project/src/main.rs");

        let stdout = r#"
{"reason":"compiler-message","message":{"level":"error","message":"use of undeclared variable `x`","spans":[{"file_name":"src/main.rs","line_start":10,"line_end":10,"column_start":5,"column_end":6}]}}
{"reason":"compiler-message","message":{"level":"error","message":"type mismatch","spans":[{"file_name":"src/lib.rs","line_start":5,"line_end":5,"column_start":1,"column_end":2}]}}
{"reason":"compiler-message","message":{"level":"warning","message":"unused import","spans":[{"file_name":"src/main.rs","line_start":1,"line_end":1,"column_start":1,"column_end":2}]}}
{"reason":"build-finished","success":false}
"#;

        let results = parse_cargo_diagnostics(stdout, workspace, edited);
        assert_eq!(results.len(), 1, "should keep exactly the main.rs error");
        assert_eq!(
            results[0], "10: use of undeclared variable `x`",
            "formatted as {{line}}: {{message}}"
        );
    }

    // ── T2: parse_ruff_diagnostics — keep edited file, drop others ───────────
    #[test]
    fn ruff_diag_keeps_only_matching_file() {
        let edited = Path::new("/project/app.py");
        let stdout = r#"[
  {"filename":"/project/app.py","location":{"row":3,"column":1},"code":"E501","message":"line too long"},
  {"filename":"/project/other.py","location":{"row":7,"column":2},"code":"F401","message":"unused import"}
]"#;

        let results = parse_ruff_diagnostics(stdout, edited);
        assert_eq!(results.len(), 1, "should keep only app.py entry");
        assert_eq!(results[0], "3: E501 line too long");
    }

    // ── T3: parse_tsc_diagnostics — keep edited file, drop others ────────────
    #[test]
    fn tsc_diag_keeps_only_matching_file() {
        let edited = Path::new("src/app.ts");
        let output = r#"src/app.ts(12,5): error TS2304: Cannot find name 'foo'.
src/other.ts(3,1): error TS2345: Argument of type 'string' is not assignable.
src/app.ts(20,10): error TS7006: Parameter 'x' implicitly has an 'any' type.
"#;

        let results = parse_tsc_diagnostics(output, edited);
        assert_eq!(results.len(), 2, "should keep both app.ts errors");
        assert_eq!(results[0], "12: error TS2304: Cannot find name 'foo'.");
        assert_eq!(results[1], "20: error TS7006: Parameter 'x' implicitly has an 'any' type.");
    }

    // ── T4a: format_finding — empty vec → None ────────────────────────────────
    #[test]
    fn format_finding_empty_is_none() {
        assert!(format_finding("rust", vec![]).is_none());
    }

    // ── T4b: format_finding — >30 lines → capped + suffix + header ───────────
    #[test]
    fn format_finding_caps_at_30_lines() {
        let lines: Vec<String> = (1..=35).map(|i| format!("{i}: error here")).collect();
        let finding = format_finding("rust", lines).expect("non-empty should produce Some");

        // Header must be present
        assert!(
            finding.message.contains("rust check found 35 issue(s)"),
            "header missing: {}",
            finding.message
        );

        // Body should be capped: 30 kept + "… (5 more)" suffix
        assert!(
            finding.message.contains("… (5 more)"),
            "truncation suffix missing: {}",
            finding.message
        );

        // Count actual diagnostic lines in message (lines after the header that
        // start with a digit pattern)
        let diag_lines: Vec<&str> = finding
            .message
            .lines()
            .skip(1) // skip header
            .filter(|l| l.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
            .collect();
        assert_eq!(diag_lines.len(), 30, "should have exactly 30 kept lines");
    }

    // ── T4c: format_finding — ≤30 lines → no truncation suffix ───────────────
    #[test]
    fn format_finding_no_truncation_when_under_cap() {
        let lines: Vec<String> = (1..=5).map(|i| format!("{i}: msg")).collect();
        let finding = format_finding("python", lines).unwrap();
        assert!(
            !finding.message.contains("more)"),
            "should not truncate when under cap: {}",
            finding.message
        );
        assert!(finding.message.contains("python check found 5 issue(s)"));
    }

    // ── T5: non-ASCII / multibyte path and message → no panic ─────────────────
    #[test]
    fn cargo_diag_multibyte_no_panic() {
        // Path with CJK characters and a message with CJK text
        let workspace = Path::new("/项目/工作区");
        let edited = Path::new("/项目/工作区/src/主程序.rs");

        // Cargo's file_name is workspace-relative; canonicalize will fail for
        // non-existent paths → must fall back to file-name comparison.
        let msg_cjk = "未声明的变量 `变量名`";
        let stdout = format!(
            r#"{{"reason":"compiler-message","message":{{"level":"error","message":"{msg_cjk}","spans":[{{"file_name":"src/主程序.rs","line_start":42,"line_end":42,"column_start":1,"column_end":2}}]}}}}"#
        );

        // Must not panic — CJK chars in file names and messages
        let results = parse_cargo_diagnostics(&stdout, workspace, edited);
        // We expect one result (matched by file name suffix since canonicalize
        // will fail for non-existent paths)
        assert_eq!(results.len(), 1, "should match via file-name fallback");
        assert!(
            results[0].contains(msg_cjk),
            "CJK message should be preserved verbatim: {}",
            results[0]
        );
        assert!(results[0].starts_with("42: "), "line number should be correct");
    }

    // ── T5b: parse_ruff_diagnostics with multibyte content → no panic ─────────
    #[test]
    fn ruff_diag_multibyte_no_panic() {
        let edited = Path::new("/项目/脚本.py");
        let stdout = format!(
            r#"[{{"filename":"/项目/脚本.py","location":{{"row":1,"column":1}},"code":"E999","message":"SyntaxError: 无效的语法"}}]"#
        );
        let results = parse_ruff_diagnostics(&stdout, edited);
        // canonicalize will fail (non-existent) → file-name fallback
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("无效的语法"));
    }

    // ── T5c: format_finding with multibyte message → no panic, correct output ─
    #[test]
    fn format_finding_multibyte_message_no_panic() {
        let lines = vec!["42: 错误：未定义的名称 'x'".to_string()];
        let finding = format_finding("rust", lines).unwrap();
        assert!(finding.message.contains("错误：未定义的名称"));
    }

    // =========================================================================
    // project_check integration tests (gated on tool availability)
    // =========================================================================

    // ── T6: python3 -m py_compile on a syntax-error file → Some ──────────────
    #[tokio::test]
    async fn project_check_py_syntax_error_some() {
        use tokio::process::Command;

        // Gate: skip if python3 is not available
        let python_ok = Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .is_ok();

        if !python_ok {
            eprintln!("SKIP project_check_py_syntax_error_some: python3 not found");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let py_file = dir.path().join("bad.py");
        tokio::fs::write(&py_file, b"def foo(\n    x\n    y\n)\n").await.unwrap();

        let cfg = ProjectCheckCfg { timeout_secs: 30 };
        let result = project_check(&py_file, dir.path(), &cfg).await;

        assert!(
            result.is_some(),
            "syntax-error .py should return Some CheckFinding"
        );
        let finding = result.unwrap();
        assert_eq!(finding.language, "python");
        assert!(
            !finding.message.is_empty(),
            "finding message should not be empty"
        );
    }

    // ── T7: python3 -m py_compile on a valid file → None ─────────────────────
    #[tokio::test]
    async fn project_check_py_valid_none() {
        use tokio::process::Command;

        // Gate: skip if python3 is not available
        let python_ok = Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .is_ok();

        if !python_ok {
            eprintln!("SKIP project_check_py_valid_none: python3 not found");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let py_file = dir.path().join("good.py");
        tokio::fs::write(&py_file, b"def foo(x):\n    return x + 1\n").await.unwrap();

        let cfg = ProjectCheckCfg { timeout_secs: 30 };
        let result = project_check(&py_file, dir.path(), &cfg).await;

        assert!(result.is_none(), "valid .py should return None, got: {:?}", result);
    }

    // ── T8: unknown extension → None WITHOUT spawning (fast) ─────────────────
    #[tokio::test]
    async fn project_check_unknown_ext_none_fast() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.unknownext");
        tokio::fs::write(&file, b"some content").await.unwrap();

        let cfg = ProjectCheckCfg { timeout_secs: 1 };

        let start = std::time::Instant::now();
        let result = project_check(&file, dir.path(), &cfg).await;
        let elapsed = start.elapsed();

        assert!(result.is_none(), "unknown extension should return None");
        // Should return essentially instantly (no spawn overhead) — well under 1s
        assert!(
            elapsed.as_millis() < 500,
            "unknown extension should return fast (<500ms), took {}ms",
            elapsed.as_millis()
        );
    }

    // ── T9: timeout path → None ───────────────────────────────────────────────
    //
    // We test the timeout wrapper directly: wrap a future that sleeps longer
    // than the budget and verify we get None (the timeout fires).
    #[tokio::test]
    async fn timeout_wrapper_returns_none_on_expiry() {
        use tokio::time::sleep;

        let budget = Duration::from_millis(50);
        let slow_future = async {
            sleep(Duration::from_secs(60)).await;
            Some(CheckFinding { language: "rust", message: "should never appear".into() })
        };

        let result = tokio::time::timeout(budget, slow_future).await;
        assert!(result.is_err(), "timeout should fire and return Err(Elapsed)");
        // project_check maps Err(Elapsed) → None; confirm the pattern
        let advisory: Option<CheckFinding> = result.ok().flatten();
        assert!(advisory.is_none());
    }
}
