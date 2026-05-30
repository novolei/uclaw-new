// SPDX-License-Identifier: AGPL-3.0-or-later
//! # Fuzzy-match chain for the `edit` tool
//!
//! Port of `hermes-agent/tools/fuzzy_match.py` (703 lines) to pure Rust.
//! Implements a 9-strategy matching chain that absorbs common LLM `old_text`
//! drift (whitespace, indentation, escape sequences, unicode) so the builtin
//! `edit` tool fails less on drifted text.
//!
//! ## Strategy order (first-match-wins)
//! 1. exact — identical bytes (fast path; behaviour is unchanged vs. today)
//! 2. line_trimmed — strip leading/trailing whitespace per line
//! 3. whitespace_normalized — collapse `[ \t]+` → single space
//! 4. indentation_flexible — lstrip every line (ignore indentation entirely)
//! 5. escape_normalized — unescape `\n`/`\t`/`\r` literals in the pattern
//! 6. trimmed_boundary — trim only the first and last lines of the pattern
//! 7. unicode_normalized — replace smart-quotes/em-dash/ellipsis/NBSP → ASCII
//! 8. block_anchor — anchor on first+last lines, similarity for middle
//! 9. context_aware — 50% line-similarity threshold (fuzzy last resort)
//!
//! ## Safety contract (CRITICAL)
//! ALL match spans returned by strategy functions are **byte** ranges into
//! `content` (not Python-style char indices). Every span must land on a UTF-8
//! char boundary. `apply_replacements` splices by byte span sorted descending
//! so earlier splices never shift later offsets.
//!
//! Debug assertions check `is_char_boundary` before every splice. Tests cover
//! multi-byte (CJK + emoji) content to gate port correctness.

use once_cell::sync::Lazy;
use regex::Regex;
use std::cmp::Reverse;

/// A matching strategy: `fn(content, pattern) -> Vec<(byte_start, byte_end)>`.
type StrategyFn = fn(&str, &str) -> Vec<(usize, usize)>;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Successful outcome from `fuzzy_find_and_replace`.
#[derive(Debug)]
pub struct FuzzyOutcome {
    /// File content after replacements.
    pub new_content: String,
    /// How many occurrences were replaced.
    pub match_count: usize,
    /// Name of the matching strategy that succeeded ("exact", "line_trimmed", …).
    pub strategy: &'static str,
}

/// Port of `hermes fuzzy_find_and_replace`.
///
/// Tries the 9 strategies in order; the first that yields matches wins.
/// Returns `Err` on: empty `old`, identical `old==new`, >1 match when
/// `replace_all=false`, escape-drift detected, or no strategy matches.
pub fn fuzzy_find_and_replace(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<FuzzyOutcome, String> {
    // Input validation (mirrors Python).
    if old.is_empty() {
        return Err("old_string cannot be empty".into());
    }
    if old == new {
        return Err("old_string and new_string are identical".into());
    }

    // Strategy table — same order as Python lines 73-83.
    let strategies: &[(&'static str, StrategyFn)] = &[
        ("exact", strategy_exact),
        ("line_trimmed", strategy_line_trimmed),
        ("whitespace_normalized", strategy_whitespace_normalized),
        ("indentation_flexible", strategy_indentation_flexible),
        ("escape_normalized", strategy_escape_normalized),
        ("trimmed_boundary", strategy_trimmed_boundary),
        ("unicode_normalized", strategy_unicode_normalized),
        ("block_anchor", strategy_block_anchor),
        ("context_aware", strategy_context_aware),
    ];

    for &(name, func) in strategies {
        let matches = func(content, old);
        if matches.is_empty() {
            continue;
        }

        // Ambiguity guard (mirrors Python).
        if matches.len() > 1 && !replace_all {
            return Err(format!(
                "Found {} matches for old_string. \
                 Provide more context to make it unique, or use replace_all=True.",
                matches.len()
            ));
        }

        // Escape-drift guard — only when not exact (mirrors Python lines 106-109).
        if name != "exact" {
            if let Some(drift_err) = detect_escape_drift(content, &matches, old, new) {
                return Err(drift_err);
            }
        }

        let new_content = apply_replacements(content, &matches, new);
        return Ok(FuzzyOutcome {
            new_content,
            match_count: matches.len(),
            strategy: name,
        });
    }

    Err("Could not find a match for old_string in the file".into())
}

// ---------------------------------------------------------------------------
// Escape-drift guard
// ---------------------------------------------------------------------------

/// Port of Python `_detect_escape_drift`.
///
/// Checks whether `new_string` contains `\'` or `\"` sequences that are present
/// in `old_string` (model "preserving context") but absent in the matched file
/// regions. That pattern is almost certainly tool-call serialisation drift —
/// writing `new` verbatim would corrupt the file.
fn detect_escape_drift(
    content: &str,
    matches: &[(usize, usize)],
    old: &str,
    new: &str,
) -> Option<String> {
    // Cheap pre-check: bail unless new contains a suspect escape.
    if !new.contains("\\'") && !new.contains("\\\"") {
        return None;
    }

    // Aggregate the matched regions of the file (what new will replace).
    let matched_regions: String = matches
        .iter()
        .map(|&(start, end)| &content[start..end])
        .collect();

    for suspect in &["\\'", "\\\""] {
        if new.contains(suspect) && old.contains(suspect) && !matched_regions.contains(suspect) {
            let plain = &suspect[1..]; // "'" or '"'
            return Some(format!(
                "Escape-drift detected: old_string and new_string contain \
                 the literal sequence {:?} but the matched region of \
                 the file does not. This is almost always a tool-call \
                 serialization artifact where an apostrophe or quote got \
                 prefixed with a spurious backslash. Re-read the file with \
                 read_file and pass old_string/new_string without \
                 backslash-escaping {:?} characters.",
                suspect, plain
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Apply replacements (byte-safe, descending order)
// ---------------------------------------------------------------------------

/// Port of Python `_apply_replacements`.
///
/// Splices `new` at each `(start, end)` byte span, working **descending** so
/// earlier replacements don't shift later byte offsets. Debug assertions verify
/// char boundaries before every slice.
fn apply_replacements(content: &str, matches: &[(usize, usize)], new: &str) -> String {
    let mut sorted: Vec<(usize, usize)> = matches.to_vec();
    sorted.sort_by_key(|b| Reverse(b.0));

    let mut result = content.to_string();
    for (start, end) in sorted {
        debug_assert!(
            result.is_char_boundary(start),
            "apply_replacements: start={} is not a char boundary",
            start
        );
        debug_assert!(
            result.is_char_boundary(end),
            "apply_replacements: end={} is not a char boundary",
            end
        );
        result = format!("{}{}{}", &result[..start], new, &result[end..]);
    }
    result
}

// ---------------------------------------------------------------------------
// Strategy 1: exact
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_exact`.
///
/// Returns byte-range matches for exact substring occurrences of `pattern` in
/// `content`. All spans are byte ranges. After each match we advance by 1 char
/// (not 1 byte) so that the next `content[start..]` slice always starts on a
/// UTF-8 char boundary, even for multi-byte content.
fn strategy_exact(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut start = 0;
    while let Some(pos) = content[start..].find(pattern) {
        let abs_pos = start + pos;
        matches.push((abs_pos, abs_pos + pattern.len()));
        // Advance by 1 char (minimum 1 byte) to stay on a char boundary.
        // This allows finding overlapping matches.
        let next_char_len = content[abs_pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        start = abs_pos + next_char_len;
    }
    matches
}

// ---------------------------------------------------------------------------
// Strategy 2: line_trimmed
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_line_trimmed`.
///
/// Strips leading/trailing whitespace from each line, then does a line-by-line
/// block match. Returns byte spans in the *original* content.
fn strategy_line_trimmed(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_norm_lines: Vec<&str> = pattern.split('\n').map(|l| l.trim()).collect();
    let content_lines: Vec<&str> = content.split('\n').collect();
    let content_norm_lines: Vec<&str> = content_lines.iter().map(|l| l.trim()).collect();

    find_normalized_matches(
        content,
        &content_lines,
        &content_norm_lines,
        &pattern_norm_lines,
    )
}

// ---------------------------------------------------------------------------
// Strategy 3: whitespace_normalized
// ---------------------------------------------------------------------------

static WS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+").unwrap());

/// Port of Python `_strategy_whitespace_normalized`.
///
/// Collapses runs of `[ \t]+` → single space in both pattern and content, then
/// matches in the normalized string and maps positions back to original byte spans.
fn strategy_whitespace_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let normalize = |s: &str| WS_RE.replace_all(s, " ").into_owned();

    let pattern_norm = normalize(pattern);
    let content_norm = normalize(content);

    let norm_matches = strategy_exact(&content_norm, &pattern_norm);
    if norm_matches.is_empty() {
        return Vec::new();
    }

    map_normalized_positions(content, &content_norm, &norm_matches)
}

// ---------------------------------------------------------------------------
// Strategy 4: indentation_flexible
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_indentation_flexible`.
///
/// Strips ALL leading whitespace from every line (lstrip), then does a
/// line-by-line block match. Returns byte spans in the original content.
fn strategy_indentation_flexible(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_stripped_lines: Vec<&str> =
        pattern.split('\n').map(|l| l.trim_start()).collect();
    let content_lines: Vec<&str> = content.split('\n').collect();
    let content_stripped_lines: Vec<&str> = content_lines.iter().map(|l| l.trim_start()).collect();

    find_normalized_matches(
        content,
        &content_lines,
        &content_stripped_lines,
        &pattern_stripped_lines,
    )
}

// ---------------------------------------------------------------------------
// Strategy 5: escape_normalized
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_escape_normalized`.
///
/// Converts `\n`/`\t`/`\r` *literal* two-character sequences in the pattern to
/// their actual control characters, then tries an exact match. Skips (returns
/// empty) when no such sequences exist (avoids redundancy with strategy 1).
fn strategy_escape_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let unescaped = pattern
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r");

    if unescaped == pattern {
        // No escape sequences in pattern; skip to avoid exact-match duplication.
        return Vec::new();
    }

    strategy_exact(content, &unescaped)
}

// ---------------------------------------------------------------------------
// Strategy 6: trimmed_boundary
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_trimmed_boundary`.
///
/// Trims whitespace from only the first and last pattern lines, then slides a
/// window of the same line-count over the content checking for a match where
/// the first and last content-window lines are also trimmed. Returns byte spans
/// via `calculate_line_byte_positions`.
fn strategy_trimmed_boundary(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_lines: Vec<&str> = pattern.split('\n').collect();
    if pattern_lines.is_empty() {
        return Vec::new();
    }

    // Trim first and last lines of the pattern (in-place equivalent).
    let first_trimmed = pattern_lines[0].trim();
    let last_trimmed = pattern_lines[pattern_lines.len() - 1].trim();
    let n = pattern_lines.len();

    // Build modified pattern as owned Strings for comparison.
    let modified_pattern_lines: Vec<String> = pattern_lines
        .iter()
        .enumerate()
        .map(|(i, &line)| {
            if i == 0 {
                first_trimmed.to_string()
            } else if i == n - 1 {
                last_trimmed.to_string()
            } else {
                line.to_string()
            }
        })
        .collect();
    let modified_pattern = modified_pattern_lines.join("\n");

    let content_lines: Vec<&str> = content.split('\n').collect();
    let content_len = content.len();
    let pattern_line_count = n;

    let mut matches = Vec::new();

    for i in 0..content_lines.len().saturating_sub(pattern_line_count - 1) {
        let block = &content_lines[i..i + pattern_line_count];

        // Build trimmed version of this block.
        let check_lines: Vec<String> = block
            .iter()
            .enumerate()
            .map(|(j, &line)| {
                if j == 0 || j == pattern_line_count - 1 {
                    line.trim().to_string()
                } else {
                    line.to_string()
                }
            })
            .collect();
        let check = check_lines.join("\n");

        if check == modified_pattern {
            let (start_pos, end_pos) =
                calculate_line_byte_positions(&content_lines, i, i + pattern_line_count, content_len);
            matches.push((start_pos, end_pos));
        }
    }

    matches
}

// ---------------------------------------------------------------------------
// Strategy 7: unicode_normalized
// ---------------------------------------------------------------------------

/// Unicode character map — mirrors Python's `UNICODE_MAP`.
/// Maps multi-byte unicode chars to their ASCII equivalents.
/// Note: em-dash (—) expands to "--" (2 bytes); ellipsis (…) to "..." (3 bytes).
const UNICODE_MAP: &[(&str, &str)] = &[
    ("\u{201C}", "\""), // left double quotation mark → "
    ("\u{201D}", "\""), // right double quotation mark → "
    ("\u{2018}", "'"),  // left single quotation mark → '
    ("\u{2019}", "'"),  // right single quotation mark → '
    ("\u{2014}", "--"), // em dash → --
    ("\u{2013}", "-"),  // en dash → -
    ("\u{2026}", "..."),// horizontal ellipsis → ...
    ("\u{00A0}", " "),  // non-breaking space → space
];

/// Apply the UNICODE_MAP substitutions to a string.
fn unicode_normalize(text: &str) -> String {
    let mut result = text.to_string();
    for &(from, to) in UNICODE_MAP {
        result = result.replace(from, to);
    }
    result
}

/// Build a mapping from each *original character index* to its *normalized byte offset*.
///
/// Because UNICODE_MAP entries may expand (e.g. em-dash 3 bytes → "--" 2 bytes,
/// or ellipsis 3 bytes → "..." 3 bytes), the normalized string can differ in
/// length. This mapping lets `map_positions_norm_to_orig` convert positions in
/// the normalized string back to original byte positions.
///
/// Returns `Vec<usize>` of length `original_char_count + 1` where entry `i` is
/// the normalized-string byte offset that character `i` of the original maps to.
/// The sentinel at `[char_count]` is the length of the normalized string.
fn build_orig_char_to_norm_byte_map(original: &str) -> Vec<usize> {
    // We use char-based iteration (mirrors Python's char-index loop).
    let mut result = Vec::new();
    let mut norm_offset = 0usize;

    for ch in original.chars() {
        result.push(norm_offset);
        // Check if this char is in the unicode map.
        let ch_str = ch.to_string();
        let repl = UNICODE_MAP.iter().find(|&&(from, _)| from == ch_str.as_str());
        if let Some(&(_, to)) = repl {
            norm_offset += to.len();
        } else {
            norm_offset += ch.len_utf8();
        }
    }
    result.push(norm_offset); // sentinel
    result
}

/// Convert `(norm_byte_start, norm_byte_end)` positions back to original byte spans.
///
/// Uses the `orig_char_to_norm_byte` map (char index → norm byte offset) to
/// find the first original char whose norm-offset >= norm_start, and similarly
/// for end. Returns byte spans into the *original* string.
fn map_positions_norm_to_orig(
    original: &str,
    orig_char_to_norm_byte: &[usize],
    norm_matches: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    // Build inverted map: norm_byte_offset → first original char_index with that offset.
    // We use a sorted vec of (norm_byte, char_index) for binary search.
    let orig_char_count = orig_char_to_norm_byte.len().saturating_sub(1);

    // inverted: norm_to_orig_start[norm_pos] = first orig char idx with that norm pos
    let mut norm_to_orig_start: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (char_idx, &norm_pos) in orig_char_to_norm_byte[..orig_char_count].iter().enumerate() {
        norm_to_orig_start.entry(norm_pos).or_insert(char_idx);
    }

    // Precompute byte offset for each original char index (for span calculation).
    let orig_char_byte_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(orig_char_count + 1);
        let mut byte_off = 0usize;
        for ch in original.chars() {
            offsets.push(byte_off);
            byte_off += ch.len_utf8();
        }
        offsets.push(byte_off); // sentinel = len of original
        offsets
    };

    let mut results = Vec::new();
    for &(norm_start, norm_end) in norm_matches {
        // Find first char whose norm_pos == norm_start.
        let orig_start_char = match norm_to_orig_start.get(&norm_start) {
            Some(&c) => c,
            None => continue, // norm_start not aligned to a char boundary
        };

        // Walk forward until orig_char_to_norm_byte[orig_end_char] >= norm_end.
        let mut orig_end_char = orig_start_char;
        while orig_end_char < orig_char_count
            && orig_char_to_norm_byte[orig_end_char] < norm_end
        {
            orig_end_char += 1;
        }

        // Convert char indices to byte offsets in the original string.
        let byte_start = orig_char_byte_offsets[orig_start_char];
        let byte_end = orig_char_byte_offsets[orig_end_char];

        // Verify char boundaries (safety assertion).
        debug_assert!(
            original.is_char_boundary(byte_start),
            "map_positions_norm_to_orig: byte_start={} not a char boundary",
            byte_start
        );
        debug_assert!(
            original.is_char_boundary(byte_end),
            "map_positions_norm_to_orig: byte_end={} not a char boundary",
            byte_end
        );

        results.push((byte_start, byte_end));
    }
    results
}

/// Port of Python `_strategy_unicode_normalized`.
///
/// Normalises smart quotes, em/en dashes, ellipsis, and NBSP to ASCII
/// equivalents in both content and pattern, then runs exact + line_trimmed
/// matching on the normalised copies, mapping positions back to original byte spans.
/// Skips when neither side changes (avoids exact-match duplication).
fn strategy_unicode_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let norm_pattern = unicode_normalize(pattern);
    let norm_content = unicode_normalize(content);

    // Skip when nothing changed in either side.
    if norm_content == content && norm_pattern == pattern {
        return Vec::new();
    }

    // Try exact then line_trimmed in the normalized space.
    let mut norm_matches = strategy_exact(&norm_content, &norm_pattern);
    if norm_matches.is_empty() {
        norm_matches = strategy_line_trimmed(&norm_content, &norm_pattern);
    }
    if norm_matches.is_empty() {
        return Vec::new();
    }

    // Map norm-byte positions back to original byte positions.
    let orig_char_to_norm_byte = build_orig_char_to_norm_byte_map(content);
    map_positions_norm_to_orig(content, &orig_char_to_norm_byte, &norm_matches)
}

// ---------------------------------------------------------------------------
// Strategy 8: block_anchor
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_block_anchor`.
///
/// Matches by anchoring on the first and last lines (after unicode normalization
/// and trim), then uses a similarity ratio for the middle. Thresholds:
/// 0.50 for a single candidate, 0.70 for multiple.
fn strategy_block_anchor(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let norm_pattern = unicode_normalize(pattern);
    let norm_content = unicode_normalize(content);

    let pattern_lines: Vec<&str> = norm_pattern.split('\n').collect();
    if pattern_lines.len() < 2 {
        return Vec::new();
    }

    let first_line = pattern_lines[0].trim();
    let last_line = pattern_lines[pattern_lines.len() - 1].trim();

    let norm_content_lines: Vec<&str> = norm_content.split('\n').collect();
    let orig_content_lines: Vec<&str> = content.split('\n').collect();
    let pattern_line_count = pattern_lines.len();
    let content_len = content.len();

    // Find candidate starting positions where first and last lines match.
    let mut potential_matches: Vec<usize> = Vec::new();
    for i in 0..norm_content_lines.len().saturating_sub(pattern_line_count - 1) {
        if norm_content_lines[i].trim() == first_line
            && norm_content_lines[i + pattern_line_count - 1].trim() == last_line
        {
            potential_matches.push(i);
        }
    }

    let threshold = if potential_matches.len() == 1 {
        0.50_f64
    } else {
        0.70_f64
    };

    let mut matches = Vec::new();
    for i in potential_matches {
        let similarity = if pattern_line_count <= 2 {
            1.0_f64
        } else {
            let content_middle = norm_content_lines[i + 1..i + pattern_line_count - 1].join("\n");
            let pattern_middle = pattern_lines[1..pattern_lines.len() - 1].join("\n");
            sequence_ratio(&content_middle, &pattern_middle)
        };

        if similarity >= threshold {
            let (start_pos, end_pos) = calculate_line_byte_positions(
                &orig_content_lines,
                i,
                i + pattern_line_count,
                content_len,
            );
            matches.push((start_pos, end_pos));
        }
    }

    matches
}

// ---------------------------------------------------------------------------
// Strategy 9: context_aware
// ---------------------------------------------------------------------------

/// Port of Python `_strategy_context_aware`.
///
/// Slides a window of `pattern_line_count` lines over `content`. A window
/// matches when at least 50% of its lines have ≥ 0.80 similarity ratio to
/// the corresponding pattern lines.
fn strategy_context_aware(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_lines: Vec<&str> = pattern.split('\n').collect();
    let content_lines: Vec<&str> = content.split('\n').collect();
    let content_len = content.len();

    if pattern_lines.is_empty() {
        return Vec::new();
    }

    let pattern_line_count = pattern_lines.len();
    let mut matches = Vec::new();

    for i in 0..content_lines.len().saturating_sub(pattern_line_count - 1) {
        let block_lines = &content_lines[i..i + pattern_line_count];
        let mut high_similarity_count = 0usize;

        for (p_line, c_line) in pattern_lines.iter().zip(block_lines.iter()) {
            let sim = sequence_ratio(p_line.trim(), c_line.trim());
            if sim >= 0.80 {
                high_similarity_count += 1;
            }
        }

        // Need at least 50% of lines with high similarity.
        if high_similarity_count as f64 >= pattern_line_count as f64 * 0.5 {
            let (start_pos, end_pos) =
                calculate_line_byte_positions(&content_lines, i, i + pattern_line_count, content_len);
            matches.push((start_pos, end_pos));
        }
    }

    matches
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Port of Python `_find_normalized_matches`.
///
/// Given the original content lines, their normalized equivalents, and the
/// normalized pattern lines, slide a window over the content looking for a
/// line-by-line block match. Returns byte spans in the *original* content.
fn find_normalized_matches(
    content: &str,
    content_lines: &[&str],
    content_normalized_lines: &[&str],
    pattern_normalized_lines: &[&str],
) -> Vec<(usize, usize)> {
    let num_pattern_lines = pattern_normalized_lines.len();
    let content_len = content.len();
    let mut matches = Vec::new();

    for i in 0..content_normalized_lines.len().saturating_sub(num_pattern_lines - 1) {
        let block = content_normalized_lines[i..i + num_pattern_lines].join("\n");
        let pattern_block = pattern_normalized_lines.join("\n");

        if block == pattern_block {
            let (start_pos, end_pos) =
                calculate_line_byte_positions(content_lines, i, i + num_pattern_lines, content_len);
            matches.push((start_pos, end_pos));
        }
    }

    matches
}

/// Port of Python `_map_normalized_positions`.
///
/// Maps `(norm_start, norm_end)` byte positions from a whitespace-normalized
/// string back to byte positions in the original string.
///
/// The normalization collapses runs of `[ \t]+` to a single space, so
/// original positions may be further along. We build an `orig_to_norm` table
/// (orig byte index → norm byte index) and invert it.
fn map_normalized_positions(
    original: &str,
    normalized: &str,
    normalized_matches: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    if normalized_matches.is_empty() {
        return Vec::new();
    }

    // Build orig_to_norm: orig byte idx → norm byte idx.
    // We use byte-level iteration matching the char-by-char Python logic.
    let orig_bytes = original.as_bytes();
    let norm_bytes = normalized.as_bytes();
    let mut orig_to_norm: Vec<usize> = Vec::with_capacity(orig_bytes.len());

    let mut orig_idx = 0usize;
    let mut norm_idx = 0usize;

    while orig_idx < orig_bytes.len() && norm_idx < norm_bytes.len() {
        if orig_bytes[orig_idx] == norm_bytes[norm_idx] {
            orig_to_norm.push(norm_idx);
            orig_idx += 1;
            norm_idx += 1;
        } else if (orig_bytes[orig_idx] == b' ' || orig_bytes[orig_idx] == b'\t')
            && norm_bytes[norm_idx] == b' '
        {
            // Original has space/tab; normalized collapsed to single space.
            orig_to_norm.push(norm_idx);
            orig_idx += 1;
            // Advance norm_idx only when we've consumed all original whitespace.
            if orig_idx < orig_bytes.len()
                && orig_bytes[orig_idx] != b' '
                && orig_bytes[orig_idx] != b'\t'
            {
                norm_idx += 1;
            }
        } else if orig_bytes[orig_idx] == b' ' || orig_bytes[orig_idx] == b'\t' {
            // Extra whitespace in original not present in normalized.
            orig_to_norm.push(norm_idx);
            orig_idx += 1;
        } else {
            // Mismatch (shouldn't happen with correct normalization).
            orig_to_norm.push(norm_idx);
            orig_idx += 1;
            norm_idx += 1;
        }
    }
    // Fill remaining original positions.
    while orig_idx < orig_bytes.len() {
        orig_to_norm.push(normalized.len());
        orig_idx += 1;
    }

    // Build reverse map: norm_pos → (first orig start, last orig end).
    let mut norm_to_orig_start: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut norm_to_orig_end: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for (orig_pos, &norm_pos) in orig_to_norm.iter().enumerate() {
        norm_to_orig_start.entry(norm_pos).or_insert(orig_pos);
        norm_to_orig_end.insert(norm_pos, orig_pos);
    }

    let mut original_matches = Vec::new();
    for &(norm_start, norm_end) in normalized_matches {
        // Find original start.
        let orig_start = if let Some(&s) = norm_to_orig_start.get(&norm_start) {
            s
        } else {
            // Find nearest: first orig idx whose norm value >= norm_start.
            match orig_to_norm.iter().position(|&n| n >= norm_start) {
                Some(p) => p,
                None => continue,
            }
        };

        // Find original end.
        let orig_end = if norm_end > 0 {
            if let Some(&e) = norm_to_orig_end.get(&(norm_end - 1)) {
                e + 1
            } else {
                orig_start + (norm_end - norm_start)
            }
        } else {
            orig_start
        };

        // Expand to include trailing whitespace that was collapsed.
        let mut orig_end = orig_end;
        while orig_end < original.len()
            && (orig_bytes[orig_end] == b' ' || orig_bytes[orig_end] == b'\t')
        {
            orig_end += 1;
        }

        // Clamp + ensure char boundary safety.
        let orig_end = orig_end.min(original.len());

        // Snap to char boundaries (byte mapping may land inside a multi-byte char).
        let orig_start = snap_to_char_boundary_start(original, orig_start);
        let orig_end = snap_to_char_boundary_end(original, orig_end);

        debug_assert!(
            original.is_char_boundary(orig_start),
            "map_normalized_positions: orig_start={} not a char boundary",
            orig_start
        );
        debug_assert!(
            original.is_char_boundary(orig_end),
            "map_normalized_positions: orig_end={} not a char boundary",
            orig_end
        );

        original_matches.push((orig_start, orig_end));
    }

    original_matches
}

/// Port of Python `_calculate_line_positions`.
///
/// Computes `(start_byte, end_byte)` for a range of lines `[start_line, end_line)`
/// in `content`. All arithmetic is byte-based via the known per-line byte lengths.
///
/// The `+1` accounts for the `\n` separator that `split('\n')` strips from each
/// line. The final `end_pos` is clamped to `content.len()` so the last line
/// (which may lack a trailing newline) is handled correctly.
fn calculate_line_byte_positions(
    content_lines: &[&str],
    start_line: usize,
    end_line: usize,
    content_len: usize,
) -> (usize, usize) {
    // start_pos = sum of (line_len + 1) for lines before start_line.
    let start_pos: usize = content_lines[..start_line]
        .iter()
        .map(|l| l.len() + 1)
        .sum();

    // end_pos = sum of (line_len + 1) for lines before end_line, minus 1
    // (because the last \n is NOT included in the matched region).
    let end_raw: usize = content_lines[..end_line].iter().map(|l| l.len() + 1).sum();
    let end_pos = end_raw.saturating_sub(1).min(content_len);

    (start_pos, end_pos)
}

/// Round `pos` down to the nearest char boundary in `s`.
fn snap_to_char_boundary_start(s: &str, pos: usize) -> usize {
    let mut p = pos.min(s.len());
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Round `pos` up to the nearest char boundary in `s`.
fn snap_to_char_boundary_end(s: &str, pos: usize) -> usize {
    let mut p = pos.min(s.len());
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}

/// Compute a similarity ratio between two strings, analogous to Python's
/// `difflib.SequenceMatcher.ratio()`.
///
/// Uses longest common subsequence length: `ratio = 2 * lcs / (len(a) + len(b))`.
/// This gives the same [0.0, 1.0] range as Python's SequenceMatcher.ratio()
/// on character sequences. We operate on chars (not bytes) for correctness.
///
/// O(m*n) but only called on single lines or small multi-line blocks.
fn sequence_ratio(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 && n == 0 {
        return 1.0;
    }
    if m == 0 || n == 0 {
        return 0.0;
    }

    // LCS via DP (two-row rolling).
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        for j in 1..=n {
            if a_chars[i - 1] == b_chars[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = curr[j - 1].max(prev[j]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|x| *x = 0);
    }
    let lcs = prev[n];
    2.0 * lcs as f64 / (m + n) as f64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Strategy 1: exact ───────────────────────────────────────────────────

    #[test]
    fn exact_single_match() {
        let r = fuzzy_find_and_replace("fn foo() {}\n", "fn foo()", "fn bar()", false).unwrap();
        assert_eq!(r.new_content, "fn bar() {}\n");
        assert_eq!(r.match_count, 1);
        assert_eq!(r.strategy, "exact");
    }

    #[test]
    fn exact_replace_all() {
        let content = "a = 1\na = 1\na = 1\n";
        let r = fuzzy_find_and_replace(content, "a = 1", "b = 2", true).unwrap();
        assert_eq!(r.new_content, "b = 2\nb = 2\nb = 2\n");
        assert_eq!(r.match_count, 3);
        assert_eq!(r.strategy, "exact");
    }

    #[test]
    fn exact_ambiguity_blocked() {
        let content = "foo\nfoo\n";
        let err = fuzzy_find_and_replace(content, "foo", "bar", false).unwrap_err();
        assert!(err.contains("2 matches"), "err: {}", err);
    }

    // ── Strategy 2: line_trimmed ─────────────────────────────────────────

    #[test]
    fn line_trimmed_leading_ws() {
        // Pattern has extra leading whitespace on each line vs. file.
        // This should match via line_trimmed (strip both ends).
        let content = "fn foo() {\n    return 1;\n}\n";
        let pattern = "  fn foo() {\n      return 1;\n  }\n";
        let r = fuzzy_find_and_replace(content, pattern, "fn bar() {}", false).unwrap();
        assert_eq!(r.strategy, "line_trimmed");
        assert!(r.new_content.contains("fn bar()"));
    }

    #[test]
    fn line_trimmed_trailing_ws() {
        // Pattern has trailing spaces on lines; file does not.
        // Match is a sub-range (not the whole content) to test correct span handling.
        let content = "preamble\nalpha\nbeta\ngamma\npostamble\n";
        let pattern = "alpha   \nbeta   \ngamma   ";
        let r = fuzzy_find_and_replace(content, pattern, "replaced", false).unwrap();
        assert_eq!(r.strategy, "line_trimmed");
        assert!(r.new_content.contains("replaced"));
        assert!(r.new_content.contains("preamble"));
        assert!(r.new_content.contains("postamble"));
    }

    // ── Strategy 3: whitespace_normalized ────────────────────────────────

    #[test]
    fn whitespace_normalized_collapsed_spaces() {
        // File has single spaces; pattern has multiple.
        let content = "x = 1 + 2\n";
        let pattern = "x  =  1  +  2";
        let r = fuzzy_find_and_replace(content, pattern, "x = 99", false).unwrap();
        assert_eq!(r.strategy, "whitespace_normalized");
        assert!(r.new_content.contains("x = 99"));
    }

    #[test]
    fn whitespace_normalized_tabs_collapsed() {
        let content = "key: value\n";
        let pattern = "key:\tvalue";
        let r = fuzzy_find_and_replace(content, pattern, "key: new", false).unwrap();
        assert_eq!(r.strategy, "whitespace_normalized");
        assert!(r.new_content.contains("key: new"));
    }

    // ── Strategy 4: indentation_flexible ─────────────────────────────────
    //
    // Note: line_trimmed (strip both ends) fires before indentation_flexible
    // (lstrip only) and catches most indentation-drift cases. The tests below
    // verify the OUTCOME is correct (the replacement happens) rather than
    // asserting the exact strategy name, since for pure leading-indent cases
    // both strategies produce the same normalized form.

    #[test]
    fn indentation_flexible_more_indent() {
        // Pattern has less indentation than file.
        // Either line_trimmed or indentation_flexible may fire — both correct.
        let content = "class Foo:\n    def bar(self):\n        pass\n";
        let pattern = "def bar(self):\n    pass\n";
        let r = fuzzy_find_and_replace(content, pattern, "def baz(self):\n    return 1", false)
            .unwrap();
        assert!(
            r.strategy == "line_trimmed" || r.strategy == "indentation_flexible",
            "unexpected strategy: {}",
            r.strategy
        );
        assert!(r.new_content.contains("baz"));
    }

    #[test]
    fn indentation_flexible_no_indent_pattern() {
        // Content has 4-space indent; pattern has none.
        // Either line_trimmed or indentation_flexible may fire — both correct.
        let content = "    x = 1\n    y = 2\n";
        let pattern = "x = 1\ny = 2\n";
        let r = fuzzy_find_and_replace(content, pattern, "x = 9\ny = 9", false).unwrap();
        assert!(
            r.strategy == "line_trimmed" || r.strategy == "indentation_flexible",
            "unexpected strategy: {}",
            r.strategy
        );
        assert!(r.new_content.contains("x = 9"));
    }

    // ── Strategy 5: escape_normalized ────────────────────────────────────

    #[test]
    fn escape_normalized_literal_newline() {
        // Pattern contains the two-character sequence \n; file has actual newline.
        let content = "line1\nline2\n";
        let pattern = "line1\\nline2";
        let r = fuzzy_find_and_replace(content, pattern, "merged", false).unwrap();
        assert_eq!(r.strategy, "escape_normalized");
        assert!(r.new_content.contains("merged"));
    }

    #[test]
    fn escape_normalized_literal_tab() {
        let content = "col1\tcol2\n";
        let pattern = "col1\\tcol2";
        let r = fuzzy_find_and_replace(content, pattern, "col1 col2", false).unwrap();
        assert_eq!(r.strategy, "escape_normalized");
        assert!(r.new_content.contains("col1 col2"));
    }

    // ── Strategy 6: trimmed_boundary ─────────────────────────────────────
    //
    // trimmed_boundary only normalises the first and last pattern lines.
    // line_trimmed (which fires earlier) normalises ALL lines, so it also
    // catches trimmed_boundary cases. The test below verifies the outcome
    // (replacement happens) — either strategy firing is correct.

    #[test]
    fn trimmed_boundary_first_last_line_ws() {
        // Pattern first/last lines have extra whitespace; middle exact.
        // line_trimmed fires first but produces the same correct result.
        let content = "start\nfoo\nend\n";
        let pattern = "  start  \nfoo\n  end  ";
        let r = fuzzy_find_and_replace(content, pattern, "REPLACED", false).unwrap();
        assert!(
            r.strategy == "line_trimmed" || r.strategy == "trimmed_boundary",
            "unexpected strategy: {}",
            r.strategy
        );
        assert!(r.new_content.contains("REPLACED"));
    }

    // ── Strategy 7: unicode_normalized ───────────────────────────────────

    #[test]
    fn unicode_normalized_smart_quotes() {
        // File has ASCII quotes; pattern has smart quotes (LLM drift).
        let content = "let x = \"hello\"\n";
        let pattern = "let x = \u{201C}hello\u{201D}";
        let r = fuzzy_find_and_replace(content, pattern, "let x = \"world\"", false).unwrap();
        assert_eq!(r.strategy, "unicode_normalized");
        assert!(r.new_content.contains("world"));
    }

    #[test]
    fn unicode_normalized_em_dash() {
        // File has "--"; pattern has em dash.
        let content = "range: 1--2\n";
        let pattern = "range: 1\u{2014}2"; // em dash
        let r = fuzzy_find_and_replace(content, pattern, "range: 3--4", false).unwrap();
        assert_eq!(r.strategy, "unicode_normalized");
        assert!(r.new_content.contains("3--4"));
    }

    #[test]
    fn unicode_normalized_nbsp_to_space() {
        // File has regular space; pattern has non-breaking space.
        let content = "hello world\n";
        let pattern = "hello\u{00A0}world"; // NBSP
        let r = fuzzy_find_and_replace(content, pattern, "hello earth", false).unwrap();
        assert_eq!(r.strategy, "unicode_normalized");
        assert!(r.new_content.contains("earth"));
    }

    // ── Multi-byte (CJK / emoji) byte-boundary safety ────────────────────

    #[test]
    fn multibyte_cjk_exact_splice_safe() {
        // CJK characters are 3 bytes each in UTF-8.
        let content = "你好世界\n再见\n";
        let r = fuzzy_find_and_replace(content, "你好世界", "再见世界", false).unwrap();
        assert_eq!(r.strategy, "exact");
        assert_eq!(r.new_content, "再见世界\n再见\n");
        // Verify result is valid UTF-8.
        assert!(std::str::from_utf8(r.new_content.as_bytes()).is_ok());
    }

    #[test]
    fn multibyte_emoji_unicode_normalized_no_panic() {
        // Emoji with smart quotes around them.
        let content = "\"🎉 done\"\n";
        let pattern = "\u{201C}🎉 done\u{201D}"; // smart quotes around emoji
        let r = fuzzy_find_and_replace(content, pattern, "\"🎉 finished\"", false).unwrap();
        assert!(r.new_content.contains("finished"));
        assert!(std::str::from_utf8(r.new_content.as_bytes()).is_ok());
    }

    // ── Strategy 8: block_anchor ──────────────────────────────────────────

    #[test]
    fn block_anchor_matches_drifted_middle() {
        // First and last lines match exactly; middle has drift.
        // (block_anchor uses similarity threshold, so slight middle drift is ok.)
        let content = "START_MARKER\n    x = 1\n    y = 2\nEND_MARKER\n";
        // Pattern with slightly different middle (same lines, extra space).
        let pattern = "START_MARKER\n    x  =  1\n    y  =  2\nEND_MARKER";
        let r = fuzzy_find_and_replace(content, pattern, "REPLACED_BLOCK", false).unwrap();
        // Should match via block_anchor or an earlier strategy.
        assert!(
            r.strategy == "block_anchor"
                || r.strategy == "whitespace_normalized"
                || r.strategy == "line_trimmed",
            "strategy: {}",
            r.strategy
        );
        assert!(r.new_content.contains("REPLACED_BLOCK"));
    }

    // ── Strategy 9: context_aware ────────────────────────────────────────

    #[test]
    fn context_aware_high_similarity() {
        // Lines are ~90% similar to pattern lines (one char differs per line).
        let content = "fn process_data(x: i32) {\n    let result = x + 1;\n    result\n}\n";
        // Pattern with minor typos (extra space removed) — should hit context_aware
        // if earlier strategies miss.
        let pattern = "fn process_data(x: i32){\n    let result = x + 1;\n    result\n}";
        let r = fuzzy_find_and_replace(content, pattern, "fn process_data(x: i32) { x }", false)
            .unwrap();
        assert!(r.new_content.contains("process_data"));
    }

    // ── Escape-drift guard ────────────────────────────────────────────────

    #[test]
    fn escape_drift_apostrophe_blocked() {
        // old and new both contain \' (drift artifact), but the file region has '
        let content = "don't stop\n";
        let old = "don\\'t stop";
        let new = "don\\'t continue";
        // Strategy: line_trimmed or whitespace_normalized may try to match —
        // but the drift guard should block before apply.
        // Note: exact won't match (old has \', content has '). A fuzzy strategy
        // will match "don't stop" region which has ' not \'.
        // If none of the fuzzy strategies match (since \' isn't in content at all),
        // we get a no-match error. Either outcome is acceptable — the key is no corruption.
        // Let's use a case where a strategy WOULD match but drift guard fires.
        // Use line_trimmed: add leading whitespace to trigger fuzzy.
        let content2 = "  don't stop  \n";
        let old2 = "  don\\'t stop  ";
        let new2 = "  don\\'t continue  ";
        let result = fuzzy_find_and_replace(content2, old2, new2, false);
        // Should either be a drift error or no-match — NOT a successful replace with \'
        match result {
            Err(e) => {
                // Either escape-drift error or no-match — both are correct.
                assert!(
                    e.contains("drift") || e.contains("not found") || e.contains("backslash"),
                    "err: {}",
                    e
                );
            }
            Ok(r) => {
                // If it somehow matched and applied, the result must NOT contain \'
                assert!(
                    !r.new_content.contains("\\'"),
                    "escape drift must not be written to file: {}",
                    r.new_content
                );
            }
        }
    }

    #[test]
    fn escape_drift_quote_blocked() {
        // new_string contains \" that's in old_string but NOT in the file region.
        // Exact match won't work since \" isn't in file. Use a case where a fuzzy
        // strategy would match but drift guard fires.
        // File has: say "hello"  (with real ASCII quotes)
        // old has:  say \"hello\"   (escaped — drift artifact; fuzzy trims and matches)
        // new has:  say \"world\"   (same drift artifact)
        let content = "  say \"hello\"  \n";
        let old = "say \\\"hello\\\"";
        let new = "say \\\"world\\\"";
        let result = fuzzy_find_and_replace(content, old, new, false);
        match result {
            Err(e) => {
                assert!(
                    e.contains("drift") || e.contains("not found") || e.contains("backslash"),
                    "err: {}",
                    e
                );
            }
            Ok(r) => {
                assert!(
                    !r.new_content.contains("\\\""),
                    "escape drift must not be written: {}",
                    r.new_content
                );
            }
        }
    }

    // ── Precedence: exact wins over fuzzy ─────────────────────────────────

    #[test]
    fn exact_wins_over_fuzzy() {
        // When exact matches, it must be used — NOT a fuzzier strategy.
        let content = "fn foo() {}\n";
        let r = fuzzy_find_and_replace(content, "fn foo()", "fn bar()", false).unwrap();
        assert_eq!(r.strategy, "exact");
    }

    // ── No-match / validation errors ─────────────────────────────────────

    #[test]
    fn no_match_returns_error() {
        let err = fuzzy_find_and_replace("hello world\n", "ZZZNOMATCH", "x", false).unwrap_err();
        assert!(err.contains("Could not find"), "err: {}", err);
    }

    #[test]
    fn empty_old_returns_error() {
        let err = fuzzy_find_and_replace("content", "", "new", false).unwrap_err();
        assert!(err.contains("empty"), "err: {}", err);
    }

    #[test]
    fn identical_old_new_returns_error() {
        let err = fuzzy_find_and_replace("content", "content", "content", false).unwrap_err();
        assert!(err.contains("identical"), "err: {}", err);
    }

    // ── apply_replacements byte safety ────────────────────────────────────

    #[test]
    fn apply_replacements_cjk_no_panic() {
        // Verify byte-level splice on CJK content doesn't panic.
        let content = "日本語テスト\n";
        // Find byte span of "テスト" (3 chars × 3 bytes = 9 bytes).
        let start = content.find("テスト").unwrap();
        let end = start + "テスト".len();
        let spans = vec![(start, end)];
        let result = apply_replacements(content, &spans, "TEST");
        assert_eq!(result, "日本語TEST\n");
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }
}
