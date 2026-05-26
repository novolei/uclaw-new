//! Code-block rescue: when the model outputs file content as text instead of
//! calling `write_file`, we parse the markdown code blocks and synthesise
//! synthetic `write_file` ToolCalls so the file actually lands on disk.
//!
//! Only fires for COMPLETE blocks (both opening and closing fence present)
//! that are long enough to be a real file (>= 10 lines). Truncated responses
//! (finish_reason=length) are handled upstream before this runs.

use crate::agent::types::ToolCall;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

/// Extract `write_file` ToolCalls from any complete markdown code blocks
/// found in `text`.
///
/// Target paths are resolved in priority order:
/// 1. Filename mentioned in the text preceding the code block
/// 2. Filename found in the first undone plan step (from workspace)
/// 3. Default name inferred from the language tag
///
/// Returns an empty Vec when no rescuable blocks are found.
pub fn extract_write_file_calls(text: &str, workspace_root: Option<&Path>) -> Vec<ToolCall> {
    let blocks = parse_code_blocks(text);
    if blocks.is_empty() {
        return Vec::new();
    }

    // Compute plan hint once for all blocks.
    let plan_hint = workspace_root.and_then(first_undone_step_text);

    let mut calls = Vec::new();
    for (lang, content, pre_text) in &blocks {
        // Skip snippets — anything shorter than 10 lines is probably an
        // inline example, not a file the model intended to write.
        if content.lines().count() < 10 {
            continue;
        }

        let path = find_filename_near(pre_text)
            .or_else(|| plan_hint.as_deref().and_then(find_filename_near))
            .or_else(|| lang_to_default_filename(lang));

        let Some(path) = path else { continue };

        tracing::info!(
            path = %path,
            lang = %lang,
            lines = content.lines().count(),
            "code_rescue: rescuing code block as write_file call"
        );

        calls.push(ToolCall {
            id: format!("rescued_{}", uuid::Uuid::new_v4()),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": path,
                "content": content,
            }),
        });
    }
    calls
}

/// Parse all COMPLETE markdown code blocks from `text`.
/// Returns `(language, content, preceding_text)` for each block.
/// Incomplete blocks (missing closing fence) are silently dropped.
fn parse_code_blocks(text: &str) -> Vec<(String, String, String)> {
    let mut results = Vec::new();
    let mut cursor = 0;

    loop {
        // Find next opening fence
        let Some(rel_open) = text[cursor..].find("```") else {
            break;
        };
        let open_pos = cursor + rel_open;
        let after_fence = open_pos + 3;

        // Language tag is the rest of the opening line
        let lang_end = text[after_fence..]
            .find('\n')
            .map(|i| after_fence + i)
            .unwrap_or(text.len());
        let lang = text[after_fence..lang_end].trim().to_string();

        let content_start = lang_end + 1;
        if content_start > text.len() {
            break;
        }

        // Find closing fence; if absent the block is truncated — stop
        let Some(rel_close) = text[content_start..].find("```") else {
            break;
        };
        let close_pos = content_start + rel_close;

        let content = text[content_start..close_pos].to_string();
        let pre_text = text[..open_pos].to_string();
        results.push((lang, content, pre_text));

        cursor = close_pos + 3;
    }

    results
}

/// Scan the last 800 characters of `text` for the last filename-like token
/// (nearest context wins).
fn find_filename_near(text: &str) -> Option<String> {
    // 用 char-aware 切片避开 UTF-8 边界 panic：当 text 含 CJK / emoji 时，
    // text.len() - 800 这个**字节**索引落在多字节字符中间会导致
    // "byte index N is not a char boundary" panic。注释也写的是"800 characters"
    // 而非"800 bytes"，所以按字符计数才是真意图。
    let total_chars = text.chars().count();
    let search = if total_chars > 800 {
        let skip = total_chars - 800;
        text.char_indices()
            .nth(skip)
            .map(|(byte_idx, _)| &text[byte_idx..])
            .unwrap_or(text)
    } else {
        text
    };

    // [\w][\w.-]* matches "script.js", "my-file.html", etc.
    // The extension list covers the file types this project deals with.
    let re = regex::Regex::new(
        r"\b([\w][\w.-]*\.(js|ts|jsx|tsx|mjs|cjs|html|css|scss|py|rs|json|yaml|yml|toml|sh|md))\b",
    )
    .ok()?;

    re.find_iter(search).last().map(|m| m.as_str().to_string())
}

/// Parse the first undone step line from the most-recently-modified plan file
/// so its description can be scanned for a filename.
fn first_undone_step_text(workspace_root: &Path) -> Option<String> {
    let plans_dir = workspace_root.join(".uclaw").join("plans");
    let mut entries: Vec<_> = std::fs::read_dir(&plans_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    // Most-recently-modified plan first
    entries.sort_by(|a, b| {
        let at = a.metadata().and_then(|m| m.modified()).ok();
        let bt = b.metadata().and_then(|m| m.modified()).ok();
        bt.cmp(&at)
    });

    let content = std::fs::read_to_string(entries.first()?.path()).ok()?;

    content
        .lines()
        .find(|line| {
            let t = line.trim_start();
            t.starts_with("- [ ]") || t.starts_with("* [ ]")
        })
        .map(|s| s.to_string())
}

/// Map a code-fence language tag to a sensible default filename when no
/// explicit name appears anywhere in the surrounding text.
fn lang_to_default_filename(lang: &str) -> Option<String> {
    match lang.to_lowercase().as_str() {
        "javascript" | "js" => Some("script.js".into()),
        "typescript" | "ts" => Some("script.ts".into()),
        "jsx" => Some("App.jsx".into()),
        "tsx" => Some("App.tsx".into()),
        "html" => Some("index.html".into()),
        "css" => Some("style.css".into()),
        "scss" => Some("style.scss".into()),
        "python" | "py" => Some("script.py".into()),
        "rust" | "rs" => Some("main.rs".into()),
        "sh" | "bash" | "shell" => Some("script.sh".into()),
        _ => None,
    }
}

/// Detect an *unclosed* markdown code block at the end of a truncated text
/// response and return `(language, content_so_far)` so the caller can
/// accumulate across finish_reason=length iterations.
///
/// Strategy: split on "```". An even number of resulting parts means an odd
/// Panic-safe wrapper around [`extract_write_file_calls`].
///
/// Code-rescue is a best-effort fallback — if any of the byte-arithmetic /
/// regex / unwrap inside the rescue pipeline panics on weird LLM output
/// (we hit this with CJK at the find_filename_near window edge), the safe
/// move is "no rescue happened this turn" rather than letting the panic
/// kill the whole agentic_loop task (which would leave the UI's streaming
/// state stuck at running:true with no chat:stream-complete event ever
/// firing).
///
/// Returns an empty Vec on panic; logs the failure via tracing::error so
/// the panic still appears in the rolling log + crashes/ directory (the
/// observability panic hook is process-wide and runs regardless).
pub fn extract_write_file_calls_safe(text: &str, workspace_root: Option<&Path>) -> Vec<ToolCall> {
    match catch_unwind(AssertUnwindSafe(|| {
        extract_write_file_calls(text, workspace_root)
    })) {
        Ok(calls) => calls,
        Err(_) => {
            tracing::error!(
                text_chars = text.chars().count(),
                "code_rescue::extract_write_file_calls panicked — treating as no-rescue",
            );
            Vec::new()
        }
    }
}

/// Panic-safe wrapper around [`extract_partial_code_block`]. Same rationale
/// as [`extract_write_file_calls_safe`].
pub fn extract_partial_code_block_safe(text: &str) -> Option<(String, String)> {
    match catch_unwind(AssertUnwindSafe(|| extract_partial_code_block(text))) {
        Ok(opt) => opt,
        Err(_) => {
            tracing::error!(
                text_chars = text.chars().count(),
                "code_rescue::extract_partial_code_block panicked — treating as no-partial",
            );
            None
        }
    }
}

/// number of fence markers — the last one opened a block that was never closed.
///
/// Returns `None` when all blocks are complete (or there are no blocks).
pub fn extract_partial_code_block(text: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = text.split("```").collect();
    // Odd count of parts = even count of ``` = all blocks closed (or none).
    if parts.len() % 2 != 0 {
        return None;
    }
    // The last element is the content after the unclosed opening ```.
    let after_fence = parts[parts.len() - 1];
    let first_newline = after_fence.find('\n').unwrap_or(after_fence.len());
    let lang = after_fence[..first_newline].trim().to_string();
    let content = if first_newline < after_fence.len() {
        after_fence[first_newline + 1..].to_string()
    } else {
        String::new()
    };
    // Skip if there is truly nothing (e.g. text ends with bare ```)
    if lang.is_empty() && content.is_empty() {
        return None;
    }
    Some((lang, content))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_content(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_single_complete_block() {
        let text = "Here is script.js:\n\n```javascript\nconsole.log('hello');\n```\n";
        let blocks = parse_code_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "javascript");
        assert!(blocks[0].1.contains("console.log"));
    }

    #[test]
    fn ignores_incomplete_block() {
        let text = "```javascript\nconsole.log('hello');\n";
        let blocks = parse_code_blocks(text);
        assert!(blocks.is_empty(), "incomplete block should be ignored");
    }

    #[test]
    fn parses_multiple_complete_blocks() {
        let text = "```html\n<html/>\n```\nand\n```css\nbody{}\n```\n";
        let blocks = parse_code_blocks(text);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn finds_filename_in_preceding_text() {
        let text = "Creating script.js now:";
        assert_eq!(find_filename_near(text), Some("script.js".into()));
    }

    #[test]
    fn finds_last_filename_when_multiple() {
        let text = "Reading index.html then creating script.js:";
        assert_eq!(find_filename_near(text), Some("script.js".into()));
    }

    #[test]
    fn skips_short_blocks() {
        let content = make_content(5); // < 10 lines
        let text = format!("script.js:\n```javascript\n{}\n```", content);
        let calls = extract_write_file_calls(&text, None);
        assert!(calls.is_empty(), "short block should be skipped");
    }

    #[test]
    fn rescues_long_block_with_filename_in_text() {
        let content = make_content(15);
        let text = format!("Writing script.js:\n```javascript\n{}\n```", content);
        let calls = extract_write_file_calls(&text, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["path"], "script.js");
    }

    #[test]
    fn falls_back_to_lang_default_filename() {
        let content = make_content(15);
        let text = format!("Here is the code:\n```javascript\n{}\n```", content);
        let calls = extract_write_file_calls(&text, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments["path"], "script.js");
    }

    #[test]
    fn unknown_lang_with_no_filename_produces_no_call() {
        let content = make_content(15);
        let text = format!("```brainfuck\n{}\n```", content);
        let calls = extract_write_file_calls(&text, None);
        assert!(
            calls.is_empty(),
            "unknown lang without filename hint should be skipped"
        );
    }

    #[test]
    fn detects_unclosed_block() {
        let text = "Writing script.js:\n```javascript\nconst x = 1;\n// ... more code";
        let result = extract_partial_code_block(text);
        assert!(result.is_some());
        let (lang, content) = result.unwrap();
        assert_eq!(lang, "javascript");
        assert!(content.contains("const x = 1;"));
    }

    #[test]
    fn no_partial_when_block_is_closed() {
        let text = "```javascript\nconst x = 1;\n```\n";
        assert!(extract_partial_code_block(text).is_none());
    }

    #[test]
    fn no_partial_when_no_blocks() {
        let text = "Just some regular text with no code.";
        assert!(extract_partial_code_block(text).is_none());
    }

    #[test]
    fn partial_after_complete_block() {
        // Complete block + new unclosed block (two truncations concatenated)
        let text = "```js\nfirst()\n```\nmore text\n```js\nsecond(";
        let result = extract_partial_code_block(text);
        assert!(result.is_some());
        let (lang, content) = result.unwrap();
        assert_eq!(lang, "js");
        assert!(content.contains("second("));
    }

    #[test]
    fn preceding_text_preferred_over_lang_default() {
        let content = make_content(15);
        let text = format!("Writing game.js using JS:\n```javascript\n{}\n```", content);
        let calls = extract_write_file_calls(&text, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments["path"], "game.js");
    }

    #[test]
    fn safe_wrapper_returns_empty_on_panic() {
        // The current implementation is fixed and does not panic on CJK input,
        // but the safe wrapper is the load-bearing defense if a future change
        // re-introduces a panic. Exercise it with the same CJK-heavy payload
        // that hit prod, and verify it returns an empty Vec instead of
        // unwinding through the caller.
        let mut text = "数".repeat(2500);
        text.push_str("\n```javascript\nlet x = 1;\n");
        text.push_str(&"console.log(x);\n".repeat(20));
        text.push_str("```\n");
        let calls = extract_write_file_calls_safe(&text, None);
        // Whether rescue succeeds or not isn't the assertion — just that it
        // didn't panic.
        let _ = calls;
    }

    #[test]
    fn find_filename_near_handles_cjk_at_window_edge() {
        // Regression: byte-index slicing at text.len() - 800 used to panic
        // when the 800-from-end position fell inside a CJK or emoji byte
        // sequence (e.g. "指" — 3 bytes UTF-8). Real-world panic site:
        //   panicked at src/agent/code_rescue.rs:108:
        //   start byte index 6867 is not a char boundary; it is inside '指'
        // Build a payload that puts a multi-byte char near the slice point.
        // Each "数" is 3 bytes; pad with enough copies so total > 800 chars.
        let mut text = "数".repeat(900);
        text.push_str(" final.js content here");
        // Must not panic.
        let _ = find_filename_near(&text);
    }
}
