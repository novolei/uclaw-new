// SPDX-License-Identifier: Apache-2.0
//! Post-edit verification (阶段 5 SP2): read-back byte-compare (catch silent
//! write failures) + incremental structured-format lint (JSON/YAML/TOML —
//! report only NEWLY-introduced parse breakage). Code-level / semantic lint
//! (clippy/tsc/LSP) is a deferred effort (project-scoped; no tree-sitter).

use std::path::Path;

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
}
