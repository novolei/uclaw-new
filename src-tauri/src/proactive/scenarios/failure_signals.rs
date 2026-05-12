//! Failure-signal taxonomy. Classifies free-text tool errors into a small
//! fixed set of high-level signals that can be matched against future
//! queries. Used by skill_extraction (to tag each new learned skill with
//! `signals_seen`) and by skill_search (to boost skills whose seen-signals
//! overlap with the current session's failures).

/// Fixed taxonomy. Keep this list small and stable — adding more requires
/// dogfood evidence that the new category is materially better at boosting
/// recall than just appending to an existing one.
pub const SIGNAL_TAXONOMY: &[&str] = &[
    "http_4xx",
    "http_5xx",
    "timeout",
    "permission_denied",
    "parse_error",
    "rate_limited",
    "not_found",
    "network_error",
];

/// Classify an error message into zero or more signals. An empty result
/// means "no recognised pattern" — the skill still extracts, just without
/// signals_seen entries.
pub fn classify_error(message: &str) -> Vec<&'static str> {
    let lower = message.to_lowercase();
    let mut sigs = Vec::new();
    if lower.contains("4xx") || lower.contains(" 401") || lower.contains(" 403")
        || lower.contains(" 404") || lower.contains(" 429") || lower.contains("client error")
    {
        sigs.push("http_4xx");
    }
    if lower.contains("5xx") || lower.contains(" 500") || lower.contains(" 502")
        || lower.contains(" 503") || lower.contains(" 504") || lower.contains("server error")
    {
        sigs.push("http_5xx");
    }
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline") {
        sigs.push("timeout");
    }
    if lower.contains("permission denied") || lower.contains("eacces")
        || lower.contains("forbidden") || lower.contains("unauthorized")
    {
        sigs.push("permission_denied");
    }
    if lower.contains("json") && (lower.contains("parse") || lower.contains("decode")
        || lower.contains("invalid"))
    {
        sigs.push("parse_error");
    }
    if lower.contains("rate limit") || lower.contains("too many requests")
        || lower.contains(" 429")
    {
        sigs.push("rate_limited");
    }
    if lower.contains("not found") || lower.contains("enoent") || lower.contains(" 404") {
        sigs.push("not_found");
    }
    if lower.contains("connection refused") || lower.contains("dns") || lower.contains("network")
        || lower.contains("connect failed") || lower.contains("ssl error")
    {
        sigs.push("network_error");
    }
    sigs.sort_unstable();
    sigs.dedup();
    sigs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_403_as_4xx() {
        let sigs = classify_error("Error: HTTP 403 Forbidden from yahoo.com");
        assert!(sigs.contains(&"http_4xx"));
        assert!(sigs.contains(&"permission_denied"));
    }

    #[test]
    fn classifies_timeout() {
        let sigs = classify_error("request timed out after 30s");
        assert_eq!(sigs, vec!["timeout"]);
    }

    #[test]
    fn empty_for_unrecognised() {
        let sigs = classify_error("Compilation failed: type mismatch in main.rs");
        assert!(sigs.is_empty(), "unrecognized error should not synthesize signals; got {:?}", sigs);
    }

    #[test]
    fn no_duplicates_in_result() {
        // both "permission denied" and "unauthorized" appear → still single permission_denied
        let sigs = classify_error("permission denied AND unauthorized");
        let pd_count = sigs.iter().filter(|s| **s == "permission_denied").count();
        assert_eq!(pd_count, 1);
    }

    #[test]
    fn classifies_429_as_both_4xx_and_rate_limited() {
        let sigs = classify_error("HTTP 429 Too Many Requests");
        assert!(sigs.contains(&"http_4xx"), "429 must be http_4xx; got {:?}", sigs);
        assert!(sigs.contains(&"rate_limited"), "429 must be rate_limited; got {:?}", sigs);
    }

    #[test]
    fn classifies_404_as_both_4xx_and_not_found() {
        let sigs = classify_error("HTTP 404 Not Found for url xyz");
        assert!(sigs.contains(&"http_4xx"), "404 must be http_4xx; got {:?}", sigs);
        assert!(sigs.contains(&"not_found"), "404 must be not_found; got {:?}", sigs);
    }
}
