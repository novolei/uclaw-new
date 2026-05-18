//! Detect agent loops in the browser tool sequence.
//!
//! A loop is declared when the same `(url, action_fingerprint)` appears
//! >= 3 times in the last 20 tool calls. On detection the agent loop
//! should surface an error so the LLM is forced to re-plan.

use std::collections::VecDeque;

/// Fingerprint: URL + tool name + first 32 chars of serialised input.
pub fn make_fingerprint(url: &str, tool_name: &str, input_prefix: &str) -> String {
    let prefix = if input_prefix.len() > 32 { &input_prefix[..32] } else { input_prefix };
    format!("{url}|{tool_name}|{prefix}")
}

pub struct LoopDetector {
    window_size: usize,
    threshold: usize,
    history: VecDeque<String>,
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new(20, 3)
    }
}

impl LoopDetector {
    pub fn new(window_size: usize, threshold: usize) -> Self {
        Self { window_size, threshold, history: VecDeque::new() }
    }

    /// Record a fingerprint. Returns `true` if a loop is detected.
    pub fn record(&mut self, fingerprint: &str) -> bool {
        self.history.push_back(fingerprint.to_string());
        if self.history.len() > self.window_size {
            self.history.pop_front();
        }
        let count = self.history.iter().filter(|h| h.as_str() == fingerprint).count();
        count >= self.threshold
    }

    /// Reset the history (e.g. on session close).
    pub fn reset(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_loop_on_distinct_actions() {
        let mut det = LoopDetector::new(20, 3);
        for i in 0..10 {
            assert!(!det.record(&format!("url|click|btn{i}")));
        }
    }

    #[test]
    fn loop_detected_at_threshold() {
        let mut det = LoopDetector::new(20, 3);
        det.record("url|click|btn1");
        det.record("url|click|btn1");
        let detected = det.record("url|click|btn1");
        assert!(detected, "should detect loop at 3rd occurrence");
    }

    #[test]
    fn window_evicts_old_entries() {
        let mut det = LoopDetector::new(5, 3);
        // Fill window with other fingerprints.
        for i in 0..5 {
            det.record(&format!("other|act|{i}"));
        }
        // Now add the loop fingerprint — the two old occurrences were evicted.
        det.record("url|click|btn1");
        det.record("url|click|btn1");
        let detected = det.record("url|click|btn1");
        assert!(detected);
    }

    #[test]
    fn reset_clears_history() {
        let mut det = LoopDetector::new(20, 3);
        det.record("url|click|btn1");
        det.record("url|click|btn1");
        det.reset();
        det.record("url|click|btn1");
        det.record("url|click|btn1");
        let detected = det.record("url|click|btn1");
        assert!(detected); // 3 after reset — fresh loop
    }

    #[test]
    fn make_fingerprint_truncates_long_input() {
        let long = "a".repeat(100);
        let fp = make_fingerprint("http://x.com", "click", &long);
        let expected_len = "http://x.com|click|".len() + 32;
        assert_eq!(fp.len(), expected_len);
    }
}
