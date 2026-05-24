// SPDX-License-Identifier: Apache-2.0

//! `cache_align` — helper to align L1 summaries and stable prefixes
//! with 1024-token prompt caching boundaries.
//!
//! Padding stable prefix blocks to 1024-token chunks isolates the
//! stable prefix from dynamic content in subsequent turns. This prevents
//! dynamic L0 suffix pollution of the final 1024-token block of the prefix,
//! maximizing Anthropic's prefix cache hit rates.

use uclaw_message_types::estimate_tokens;

/// Align a stable text block (like the L1 archive summary) to a 1024-token boundary
/// by appending a safe markdown HTML comment filled with spaces.
/// This prevents dynamic content from sharing and polluting the final 1024-token
/// cache block of the prefix, maximizing Anthropic prompt cache efficiency.
pub fn align_to_1024_tokens(text: &str) -> String {
    // Check if the input text itself is empty or whitespace-only.
    // If so, return an empty string. This ensures we don't pad empty prompts.
    if text.trim().is_empty() {
        return String::new();
    }

    let tokens = estimate_tokens(text) as usize;
    let block_size = 1024;
    let remainder = tokens % block_size;
    if remainder == 0 {
        return text.to_string();
    }

    let padding_tokens_needed = block_size - remainder;
    // Estimate overhead: "\n\n<!-- cache_align:  -->"
    let overhead_text = "\n\n<!-- cache_align:  -->";
    let overhead_tokens = estimate_tokens(overhead_text) as usize;

    let target_padding = if padding_tokens_needed > overhead_tokens {
        padding_tokens_needed - overhead_tokens
    } else {
        (padding_tokens_needed + block_size) - overhead_tokens
    };

    // Since spaces are 0.15 tokens each in estimate_tokens:
    // We floor the approximation to guarantee we undershoot, then fine-tune.
    let approx_spaces = (target_padding as f32 / 0.15).floor() as usize;

    let mut aligned = text.trim_end().to_string();
    aligned.push_str("\n\n<!-- cache_align: ");
    aligned.push_str(&" ".repeat(approx_spaces));
    aligned.push_str(" -->\n");

    // Fine-tune by adding spaces one by one. Since space weight is 0.15 (< 1.0),
    // ceil() is guaranteed to hit every integer, ensuring we land exactly on the 1024 multiple.
    let mut iterations = 0;
    while (estimate_tokens(&aligned) as usize) % block_size != 0 && iterations < 100 {
        if let Some(pos) = aligned.rfind(" -->\n") {
            aligned.insert(pos, ' ');
        } else {
            break;
        }
        iterations += 1;
    }

    aligned
}

/// Align a static block using a 5-Tier Prompt Ladder of token sizes:
/// 2048, 4096, 8192, 16384, 32768 tokens.
/// Utilizes a lightweight heuristic of 1 token ≈ 4.1 characters to pad with a
/// `<!-- pad: N bytes -->` HTML comment.
pub fn pad_to_ladder(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }

    let estimated_tokens = (text.len() as f32 / 4.1).round() as usize;
    let tiers = [2048, 4096, 8192, 16384, 32768];
    
    // Find the smallest tier >= estimated_tokens
    let target_tier = match tiers.iter().find(|&&t| t >= estimated_tokens) {
        Some(&t) => t,
        None => {
            // Fall back to aligning to nearest 1024 boundary
            let next_1024 = ((estimated_tokens + 1023) / 1024) * 1024;
            next_1024
        }
    };

    if target_tier <= estimated_tokens {
        return text.to_string();
    }

    let tokens_needed = target_tier - estimated_tokens;
    let chars_needed = (tokens_needed as f32 * 4.1).round() as usize;
    
    let base_overhead = "\n\n<!-- pad:  -->\n";
    if chars_needed <= base_overhead.len() {
        return text.to_string();
    }

    let padding_len = chars_needed - base_overhead.len();
    let padding_spaces = " ".repeat(padding_len);

    let mut padded = text.trim_end().to_string();
    padded.push_str("\n\n<!-- pad: ");
    padded.push_str(&padding_spaces);
    padded.push_str(" -->\n");
    padded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_to_1024_tokens() {
        let base_text = "Hello World! This is some base text that is stable and needs prompt caching.";
        let aligned = align_to_1024_tokens(base_text);

        let initial_tokens = estimate_tokens(base_text) as usize;
        let aligned_tokens = estimate_tokens(&aligned) as usize;

        assert!(aligned_tokens > initial_tokens);
        assert_eq!(aligned_tokens % 1024, 0);
        assert!(aligned.contains("<!-- cache_align:"));
        assert!(aligned.ends_with(" -->\n"));
    }

    #[test]
    fn test_align_empty_text() {
        let aligned = align_to_1024_tokens("");
        assert_eq!(aligned, "");

        let aligned_spaces = align_to_1024_tokens("   \n\t ");
        assert_eq!(aligned_spaces, "");
    }

    #[test]
    fn test_pad_to_ladder() {
        let text = "Short text needing padding";
        let padded = pad_to_ladder(text);
        assert!(padded.contains("<!-- pad:"));
        assert!(padded.len() > text.len());
        
        let est_tokens = (padded.len() as f32 / 4.1).round() as usize;
        // Should align to the first tier: 2048 tokens
        assert!(est_tokens >= 2040 && est_tokens <= 2056);
    }
}
