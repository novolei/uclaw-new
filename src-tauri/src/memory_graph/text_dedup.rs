//! openhuman-G — shared text-dedup primitives (normalize, bigram-Jaccard,
//! CJK-aware tokenization). Used by skill_parser (skill dedup) AND reflection
//! (fact dedup). Pure functions, no DB.

/// Threshold for fuzzy (bigram-Jaccard) dedup. ≥ this similarity →
/// fold into existing skill instead of creating a new node. Tuned
/// conservatively: catches "+1 word" near-dups but rejects
/// concept-level overlap (which is D3's territory, not D2's).
pub const FUZZY_DEDUP_THRESHOLD: f32 = 0.75;

/// Character bigrams of a string. Language-agnostic — works for CJK
/// without a tokenizer, and for ASCII without a stemmer.
///
/// Empty / 1-char strings produce empty sets; 2-char strings produce
/// a single bigram. Both are correctly handled by `jaccard_similarity`.
pub fn title_bigrams(s: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = std::collections::HashSet::new();
    if chars.len() < 2 {
        return set;
    }
    for w in chars.windows(2) {
        set.insert(w.iter().collect::<String>());
    }
    set
}

/// Jaccard similarity (|A ∩ B| / |A ∪ B|) between two bigram sets.
/// Returns 0.0 for empty sets to avoid 0/0 NaN.
pub fn jaccard_similarity(
    a: &std::collections::HashSet<String>,
    b: &std::collections::HashSet<String>,
) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        return 0.0;
    }
    inter as f32 / union as f32
}

/// Estimate the proportion of CJK characters in a string.
///
/// Counts characters in the Unicode ranges:
///   - CJK Unified Ideographs (U+4E00–U+9FFF)
///   - CJK Extension A (U+3400–U+4DBF)
///   - CJK Compatibility Ideographs (U+F900–U+FAFF)
///   - Hiragana (U+3040–U+309F), Katakana (U+30A0–U+30FF)
///   - Hangul (U+AC00–U+D7AF)
///
/// Returns 0.0 for empty strings and strings without CJK characters.
pub fn cjk_char_ratio(s: &str) -> f32 {
    let total = s.chars().count();
    if total == 0 {
        return 0.0;
    }
    let cjk_count = s.chars().filter(|c| is_cjk_char(*c)).count();
    cjk_count as f32 / total as f32
}

/// Check if a single character falls within CJK Unicode ranges.
fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'  // CJK Extension A
        | '\u{F900}'..='\u{FAFF}'  // CJK Compatibility Ideographs
        | '\u{3040}'..='\u{309F}'  // Hiragana
        | '\u{30A0}'..='\u{30FF}'  // Katakana
        | '\u{AC00}'..='\u{D7AF}'  // Hangul Syllables
    )
}

/// Word-level bigrams for mixed CJK/ASCII text.
///
/// Splitting strategy:
///   - ASCII text: split on whitespace/punctuation, each word becomes a token
///   - CJK text (Han/Kana/Hangul): each CJK character is its own token
///     (no dictionary needed; CJK characters carry meaning individually)
///   - Punctuation and whitespace are discarded as tokens, but act as boundary
///     markers between ASCII words.
///
/// This produces better semantic bigrams than pure character bigrams because
/// "游戏开发" tokenizes to ["游","戏","开","发"] → bigrams ["游戏","戏开","开发"]
/// while "开发游戏" tokenizes to ["开","发","游","戏"] → ["开发","发游","游戏"] —
/// both share "开发" and "游戏" bigrams.
pub fn word_bigrams(s: &str) -> std::collections::HashSet<String> {
    let tokens = tokenize_mixed(s);
    if tokens.len() < 2 {
        return std::collections::HashSet::new();
    }
    let mut set = std::collections::HashSet::new();
    for w in tokens.windows(2) {
        set.insert(format!("{}{}", w[0], w[1]));
    }
    set
}

/// Split mixed CJK/ASCII text into word-like tokens.
///
/// Heuristic:
///   - Run of CJK characters: each char is a separate token
///   - Run of ASCII alphanumeric: accumulated then emitted as one token
///   - Whitespace and punctuation: discarded (act as token boundaries)
fn tokenize_mixed(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();

    let flush_ascii = |buf: &mut String, out: &mut Vec<String>| {
        if !buf.is_empty() {
            out.push(buf.clone());
            buf.clear();
        }
    };

    for c in s.chars() {
        if c.is_whitespace() || c.is_ascii_punctuation() {
            flush_ascii(&mut ascii_buf, &mut tokens);
            // whitespace/punctuation discarded
        } else if is_cjk_char(c) {
            flush_ascii(&mut ascii_buf, &mut tokens);
            tokens.push(c.to_string());
        } else if c.is_alphanumeric() {
            ascii_buf.push(c);
        }
        // Other Unicode (emoji, symbols, etc.) — discard
    }
    flush_ascii(&mut ascii_buf, &mut tokens);
    tokens
}

/// Normalize a skill title for dedup comparison.
///
/// Strategy: trim + lowercase + collapse whitespace + drop trailing
/// punctuation. Conservative — we only want to catch obvious duplicates
/// like "前端游戏开发项目工作流" appearing twice with different
/// casing or trailing colon. Fuzzy concept-level dedup is D2's job.
pub fn normalize_title_for_dedup(title: &str) -> String {
    let mut s = title.trim().to_lowercase();
    // Collapse runs of whitespace into single space.
    let collapsed: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    s = collapsed;
    // Drop trailing punctuation (Chinese + ASCII).
    s.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '。' | '，' | '；' | '：' | '！' | '？'
        )
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercase_whitespace_punct() {
        // Case collapse
        assert_eq!(
            normalize_title_for_dedup("Edit Tool Tips"),
            normalize_title_for_dedup("edit tool tips"),
        );
        // Trailing whitespace strip
        assert_eq!(
            normalize_title_for_dedup("前端游戏开发项目工作流"),
            normalize_title_for_dedup("  前端游戏开发项目工作流  "),
        );
        // Trailing Chinese punct strip
        assert_eq!(
            normalize_title_for_dedup("使用 edit 工具"),
            normalize_title_for_dedup("使用 edit 工具："),
        );
        // Multiple spaces collapse
        assert_eq!(normalize_title_for_dedup("a  b   c"), "a b c");
        // Different titles remain different
        assert_ne!(
            normalize_title_for_dedup("edit 工具技巧"),
            normalize_title_for_dedup("edit 工具陷阱"),
        );
    }

    #[test]
    fn title_bigrams_empty_and_short() {
        assert!(title_bigrams("").is_empty());
        assert!(title_bigrams("a").is_empty());
        // 2-char string → exactly 1 bigram
        assert_eq!(title_bigrams("ab").len(), 1);
    }

    #[test]
    fn jaccard_identical_disjoint_empty() {
        let a = title_bigrams("hello");
        // identical → 1.0
        assert_eq!(jaccard_similarity(&a, &a), 1.0);

        let b = title_bigrams("zzzzz");
        // disjoint → 0.0
        assert_eq!(jaccard_similarity(&a, &b), 0.0);

        let empty = std::collections::HashSet::new();
        // both empty → 0.0
        assert_eq!(jaccard_similarity(&empty, &empty), 0.0);
    }

    #[test]
    fn cjk_char_ratio_pure_cjk_and_ascii() {
        // Pure CJK
        let ratio = cjk_char_ratio("汉字");
        assert!((ratio - 1.0).abs() < f32::EPSILON);
        // Pure ASCII
        let ratio_ascii = cjk_char_ratio("hello");
        assert!((ratio_ascii - 0.0).abs() < f32::EPSILON);
        // Empty
        assert_eq!(cjk_char_ratio(""), 0.0);
        // Mixed: "a汉" — 1 CJK out of 2 chars = 0.5
        let ratio_mixed = cjk_char_ratio("a汉");
        assert!((ratio_mixed - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn jaccard_fuzzy_threshold_hit_and_miss() {
        // Near-dup: single extra prefix word — should exceed threshold.
        let c = title_bigrams("基于计划的增量式游戏前端开发工作流");
        let d = title_bigrams("基于计划的增量式游戏开发工作流");
        let sim = jaccard_similarity(&c, &d);
        assert!(
            sim >= FUZZY_DEDUP_THRESHOLD,
            "expected fuzzy hit for inserted word, got {}",
            sim
        );

        // Concept overlap but different titles — should miss threshold.
        let e = title_bigrams("前端游戏开发项目工作流");
        let f = title_bigrams("基于计划的增量式游戏前端开发工作流");
        let sim2 = jaccard_similarity(&e, &f);
        assert!(
            sim2 < FUZZY_DEDUP_THRESHOLD,
            "expected fuzzy miss for concept overlap, got {}",
            sim2
        );
    }
}
