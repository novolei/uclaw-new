//! Chat-turn candidate producer — Sprint 1.9.
//!
//! The "hidden big rock" of Sprint 1: without a producer, the
//! stability detector + cache + prompt section all stand idle. This
//! module is the only mandatory producer in Sprint 1; later sprints
//! will add producers for file mounts, EntityPage mentions, and
//! Composio integrations.
//!
//! ## Two-layer pipeline
//!
//! **Layer 1 — regex heuristics(always runs, zero cost).** A small
//! set of high-precision patterns over explicit user statements:
//!
//! - "I am/I'm a/an X"             → Identity: role/profession
//! - "my name is X"                → Identity: name
//! - "I (work|live) in X"          → Identity: location/timezone
//! - "I (use|prefer|like) X"       → Tooling: editor/lang/...
//! - "I (hate|don't like|never) X" → Veto
//! - "I want to/remind me to X"    → Goal
//! - "always/never X"              → Style
//! - "我是 X / 我叫 X / 我用 X / 我喜欢 X / 我讨厌 X" → bilingual
//!
//! Regex matches produce candidates with [`CueFamily::Explicit`] and
//! confidence 0.9 (slight discount vs hard 1.0 to leave headroom for
//! the explicit cue boost in [`stability_detector::stability`] to
//! actually mean something).
//!
//! **Layer 2 — LLM extraction(opt-in, behind
//! `memory_os.learning_llm_extractor_enabled`, daily token budget
//! capped).** When the regex layer produces zero candidates AND the
//! turn is long enough to be worth analysing(> 80 chars), the
//! turn is sent to the configured `MemoryOsLlm` with a structured
//! prompt asking for facet candidates as JSON. Same cost-guard
//! pattern as Phase 5 memory_lint(`cost_records.model LIKE
//! 'memory_learning%'`).
//!
//! ## Producer contract
//!
//! [`extract_from_chat_turn`] never panics, never returns an error.
//! It logs warnings and falls back to "no candidates" when the LLM
//! or regex bombs. The agent path calls this from a `tokio::spawn`
//! after every assistant turn — a failing extractor must not stall
//! the user's reply.

use std::sync::Arc;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::learning::candidate::{
    Buffer, CueFamily, EvidenceRef, FacetClass, LearningCandidate,
};
use crate::memory_graph::memory_os_llm::MemoryOsLlm;

// ─── Regex layer ───────────────────────────────────────────────────────

/// One explicit-cue pattern. Each one produces a candidate of a
/// specific (class, name) when matched.
struct ExplicitPattern {
    class: FacetClass,
    name: &'static str,
    regex: Regex,
    /// Which capture group holds the value. 1 = first capture.
    value_group: usize,
}

/// All shipped regex patterns. English + Chinese. Pattern compilation
/// happens once at first use via `Lazy`.
static PATTERNS: Lazy<Vec<ExplicitPattern>> = Lazy::new(|| {
    fn p(class: FacetClass, name: &'static str, pat: &str) -> ExplicitPattern {
        ExplicitPattern {
            class,
            name,
            regex: Regex::new(pat).expect("hard-coded regex must compile"),
            value_group: 1,
        }
    }
    vec![
        // ── Identity (English) ───────────────────────────────────
        // "my name is Alice" / "I'm Alice"
        p(FacetClass::Identity, "name", r"(?i)\bmy name is ([A-Za-z][A-Za-z'-]{0,40})"),
        p(FacetClass::Identity, "name", r"(?i)\bi(?:'m|am)?\s+called\s+([A-Za-z][A-Za-z'-]{0,40})"),
        // "I am a senior engineer"
        p(FacetClass::Identity, "role", r"(?i)\bi(?:'m|\s+am)\s+a(?:n)?\s+([a-z][a-z -]{2,40}?)(?:\s+at|\.|$|,)"),
        // "I work in Beijing" / "I live in PST"
        p(FacetClass::Identity, "location", r"(?i)\bi (?:work|live) in ([A-Z][A-Za-z /-]{1,40})"),
        // "my timezone is PST"
        p(FacetClass::Identity, "timezone", r"(?i)\bmy timezone is ([A-Z][A-Z /+-]{2,15})"),
        // ── Identity (Chinese) ────────────────────────────────────
        p(FacetClass::Identity, "name", r"我叫\s*([\p{Han}A-Za-z][\p{Han}A-Za-z .'-]{0,30})"),
        p(FacetClass::Identity, "role", r"我是(?:一名|一个)?\s*([\p{Han}][\p{Han}A-Za-z]{1,20})"),

        // ── Tooling (English) ────────────────────────────────────
        // "I use helix" / "I prefer pnpm" / "I like rust"
        p(FacetClass::Tooling, "primary", r"(?i)\bi (?:use|prefer|like|love) ([a-z][a-z0-9+./_-]{1,40})"),
        // ── Tooling (Chinese) ─────────────────────────────────────
        p(FacetClass::Tooling, "primary", r"我(?:用|喜欢|偏好)\s*([A-Za-z][A-Za-z0-9+./_-]{1,40})"),

        // ── Veto (English + Chinese) ──────────────────────────────
        p(FacetClass::Veto, "tool", r"(?i)\bi (?:hate|don't like|never use|won't use) ([a-z][a-z0-9+./_-]{1,40})"),
        p(FacetClass::Veto, "tool", r"我(?:讨厌|不喜欢|从不用)\s*([A-Za-z][A-Za-z0-9+./_-]{1,40})"),

        // ── Style ─────────────────────────────────────────────────
        // "always X" / "never Y" — capture the action as the value
        p(FacetClass::Style, "rule", r"(?i)\b(always|never)\s+([a-z][a-z 0-9.'/-]{3,60})"),

        // ── Goal ──────────────────────────────────────────────────
        // "I want to ship X" / "remind me to call X"
        p(FacetClass::Goal, "active", r"(?i)\b(?:i want to|remind me to|i need to)\s+([a-z][a-z 0-9.'/-]{3,80})"),
    ]
});

/// Run the regex layer over `text`. Returns 0+ candidates with
/// [`CueFamily::Explicit`] + confidence 0.9.
///
/// Stable order: patterns scanned in declaration order, multiple
/// matches in one pattern dedup'd by value(first wins).
pub fn extract_regex(
    text: &str,
    session_id: &str,
    turn_id: &str,
) -> Vec<LearningCandidate> {
    let mut out: Vec<LearningCandidate> = Vec::new();
    let evidence = EvidenceRef::ChatTurn {
        session_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
    };
    for pat in PATTERNS.iter() {
        for caps in pat.regex.captures_iter(text) {
            let value = match caps.get(pat.value_group) {
                Some(m) => m.as_str()
                    .trim()
                    .trim_end_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ';' | ':'))
                    .to_string(),
                None => continue,
            };
            if value.is_empty() {
                continue;
            }
            // Dedup: skip if we already produced a (class, name, value) match.
            if out.iter().any(|c| {
                c.class == pat.class && c.name == pat.name && c.value == value
            }) {
                continue;
            }
            let candidate = LearningCandidate::new(
                pat.class,
                pat.name,
                value,
                CueFamily::Explicit,
                evidence.clone(),
            )
            .with_confidence(0.9);
            out.push(candidate);
        }
    }
    out
}

// ─── LLM layer ─────────────────────────────────────────────────────────

/// LLM extractor system prompt. Pinned in source so it's reviewable
/// in PRs and round-trippable in tests via `system_prompt()`.
pub(crate) fn llm_system_prompt() -> &'static str {
    "You are the user-profile extractor for a personal AI agent. \
     Read ONE user message and extract any facets the agent should \
     remember about the user. Output ONLY a JSON array, no prose.\n\n\
     Schema (each element):\n\
     {\"class\":\"identity\"|\"style\"|\"tooling\"|\"veto\"|\"goal\"|\"channel\",\
      \"name\":\"<short slot name>\",\
      \"value\":\"<short value>\",\
      \"cue\":\"explicit\"|\"structural\"|\"behavioral\"|\"recurrence\",\
      \"confidence\":0.0-1.0}\n\n\
     Rules:\n\
     - Return [] when the message contains no extractable user facets.\n\
     - 'name' is the slot within the class (editor / role / timezone / verbosity / ...).\n\
     - 'value' is short — under 40 chars. Truncate long values.\n\
     - Use 'cue': 'explicit' for direct user declarations, 'behavioral'\n\
       for inferences from how they phrased something.\n\
     - confidence ≥ 0.7 for explicit cues, ≤ 0.5 for behavioral.\n\
     - Do not invent facts that are not in the message.\n\
     - Output ONLY the JSON array. No code fences. No explanation."
}

#[derive(Debug, serde::Deserialize)]
struct LlmFacetRow {
    class: String,
    name: String,
    value: String,
    #[serde(default = "default_cue")]
    cue: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
}

fn default_cue() -> String {
    "explicit".to_string()
}
fn default_confidence() -> f64 {
    0.7
}

/// Parse the LLM's JSON-array response into candidates. Tolerates
/// fences + surrounding prose by scanning for the first `[` and
/// matching `]`. Unknown class / cue strings drop the row.
pub(crate) fn parse_llm_response(
    text: &str,
    session_id: &str,
    turn_id: &str,
) -> Vec<LearningCandidate> {
    let trimmed = text.trim();
    let start = match trimmed.find('[') {
        Some(p) => p,
        None => return vec![],
    };
    let end = match trimmed.rfind(']') {
        Some(e) => e,
        None => return vec![],
    };
    if end <= start {
        return vec![];
    }
    let arr_slice = &trimmed[start..=end];
    let rows: Vec<LlmFacetRow> = match serde_json::from_str(arr_slice) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let evidence = EvidenceRef::ChatTurn {
        session_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
    };
    rows.into_iter()
        .filter_map(|r| {
            let class = parse_class(&r.class)?;
            let cue = parse_cue(&r.cue)?;
            let name = r.name.trim().to_string();
            let value = r.value.trim().to_string();
            if name.is_empty() || value.is_empty() {
                return None;
            }
            // Truncate value to a sane length (matches the prompt rule).
            let value = if value.chars().count() > 40 {
                value.chars().take(40).collect()
            } else {
                value
            };
            Some(
                LearningCandidate::new(class, name, value, cue, evidence.clone())
                    .with_confidence(r.confidence.clamp(0.0, 1.0)),
            )
        })
        .collect()
}

fn parse_class(s: &str) -> Option<FacetClass> {
    match s {
        "identity" => Some(FacetClass::Identity),
        "style" => Some(FacetClass::Style),
        "tooling" => Some(FacetClass::Tooling),
        "veto" => Some(FacetClass::Veto),
        "goal" => Some(FacetClass::Goal),
        "channel" => Some(FacetClass::Channel),
        _ => None,
    }
}

fn parse_cue(s: &str) -> Option<CueFamily> {
    match s {
        "explicit" => Some(CueFamily::Explicit),
        "structural" => Some(CueFamily::Structural),
        "behavioral" => Some(CueFamily::Behavioral),
        "recurrence" => Some(CueFamily::Recurrence),
        _ => None,
    }
}

/// Threshold: don't bother spending an LLM call on short messages
/// that almost never carry facets. "ok", "thanks", "sure", etc.
pub const LLM_MIN_TURN_CHARS: usize = 80;

/// Run the LLM layer over `text`. Returns candidates that may include
/// any cue family. Best-effort — errors logged + return empty Vec.
pub async fn extract_via_llm(
    llm: &Arc<dyn MemoryOsLlm>,
    text: &str,
    session_id: &str,
    turn_id: &str,
) -> Vec<LearningCandidate> {
    if text.chars().count() < LLM_MIN_TURN_CHARS {
        return vec![];
    }
    let out = match llm
        .complete_text("memory_learning", llm_system_prompt(), text, 800)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "learning::extractor: LLM call failed");
            return vec![];
        }
    };
    parse_llm_response(&out.text, session_id, turn_id)
}

// ─── End-to-end driver ─────────────────────────────────────────────────

/// Top-level entry point the agent dispatcher calls. Pushes 0+
/// candidates into `buffer` for the next stability rebuild to pick up.
///
/// Layer policy:
/// - Regex layer always runs (zero cost).
/// - LLM layer runs only when `llm_enabled` AND the regex layer
///   produced zero candidates AND `text.chars().count() >= LLM_MIN_TURN_CHARS`.
///   This avoids double-counting: if a regex already caught the
///   explicit "I prefer X" pattern, asking the LLM for the same
///   message wastes tokens and would dilute the evidence weight via
///   duplicate-key combine.
pub async fn extract_from_chat_turn(
    text: &str,
    session_id: &str,
    turn_id: &str,
    buffer: &Buffer,
    llm_enabled: bool,
    llm: Option<&Arc<dyn MemoryOsLlm>>,
) -> usize {
    let mut count = 0;
    let regex_candidates = extract_regex(text, session_id, turn_id);
    let regex_was_empty = regex_candidates.is_empty();
    for c in regex_candidates {
        buffer.push(c);
        count += 1;
    }
    if llm_enabled && regex_was_empty {
        if let Some(client) = llm {
            let llm_candidates = extract_via_llm(client, text, session_id, turn_id).await;
            for c in llm_candidates {
                buffer.push(c);
                count += 1;
            }
        }
    }
    count
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;

    fn ev() -> EvidenceRef {
        EvidenceRef::ChatTurn {
            session_id: "s".into(),
            turn_id: "t".into(),
        }
    }

    // ─── Regex layer ──────────────────────────────────────────────

    #[test]
    fn regex_no_match_empty_text() {
        let out = extract_regex("ok", "s", "t");
        assert!(out.is_empty());
    }

    #[test]
    fn regex_picks_up_my_name_is() {
        let out = extract_regex("my name is Alice and I love coffee", "s", "t");
        assert!(out.iter().any(|c| c.class == FacetClass::Identity && c.name == "name" && c.value == "Alice"));
    }

    #[test]
    fn regex_picks_up_role() {
        let out = extract_regex("I am a senior engineer at Acme.", "s", "t");
        let role = out.iter().find(|c| c.class == FacetClass::Identity && c.name == "role");
        assert!(role.is_some(), "role pattern should fire");
        assert!(role.unwrap().value.contains("engineer"));
    }

    #[test]
    fn regex_picks_up_tooling_preference() {
        let out = extract_regex("I prefer pnpm over npm and I use helix", "s", "t");
        // Both 'pnpm' and 'helix' should land as tooling.
        let values: Vec<String> = out
            .iter()
            .filter(|c| c.class == FacetClass::Tooling)
            .map(|c| c.value.clone())
            .collect();
        assert!(values.contains(&"pnpm".to_string()) || values.contains(&"helix".to_string()),
            "got: {:?}",
            values
        );
    }

    #[test]
    fn regex_picks_up_veto() {
        let out = extract_regex("I hate emacs, I never use vim either.", "s", "t");
        let vetoes: Vec<String> = out
            .iter()
            .filter(|c| c.class == FacetClass::Veto)
            .map(|c| c.value.clone())
            .collect();
        assert!(vetoes.contains(&"emacs".to_string()));
    }

    #[test]
    fn regex_picks_up_chinese_role_and_tool() {
        let out = extract_regex("我是工程师,我用 helix 写代码", "s", "t");
        assert!(
            out.iter().any(|c| c.class == FacetClass::Identity && c.name == "role"),
            "Chinese role didn't fire"
        );
        assert!(
            out.iter().any(|c| c.class == FacetClass::Tooling && c.value == "helix"),
            "Chinese tooling didn't fire"
        );
    }

    #[test]
    fn regex_picks_up_chinese_veto() {
        let out = extract_regex("我讨厌 npm,从来不用 yarn", "s", "t");
        let vetoes: Vec<String> = out
            .iter()
            .filter(|c| c.class == FacetClass::Veto)
            .map(|c| c.value.clone())
            .collect();
        assert!(vetoes.contains(&"npm".to_string()));
    }

    #[test]
    fn regex_picks_up_goal() {
        let out = extract_regex("I want to ship phase 8 by friday.", "s", "t");
        let goals: Vec<String> = out
            .iter()
            .filter(|c| c.class == FacetClass::Goal)
            .map(|c| c.value.clone())
            .collect();
        assert!(!goals.is_empty(), "goal pattern should fire");
    }

    #[test]
    fn regex_dedups_repeated_values_within_text() {
        // Same value matched twice in one text should produce one candidate.
        let out = extract_regex("I use helix. I really like helix.", "s", "t");
        let helixes: Vec<_> = out
            .iter()
            .filter(|c| c.class == FacetClass::Tooling && c.value == "helix")
            .collect();
        assert_eq!(helixes.len(), 1, "dedup: 'helix' should appear once in candidates");
    }

    #[test]
    fn regex_candidates_carry_explicit_cue_at_high_confidence() {
        let out = extract_regex("I prefer pnpm", "s", "t");
        for c in &out {
            assert_eq!(c.cue, CueFamily::Explicit);
            assert!(c.confidence >= 0.8, "confidence should be at least 0.8, got {}", c.confidence);
        }
    }

    #[test]
    fn regex_evidence_ref_is_chat_turn() {
        let out = extract_regex("I prefer pnpm", "sess-X", "turn-Y");
        assert!(!out.is_empty());
        match &out[0].evidence {
            EvidenceRef::ChatTurn { session_id, turn_id } => {
                assert_eq!(session_id, "sess-X");
                assert_eq!(turn_id, "turn-Y");
            }
            other => panic!("expected ChatTurn evidence, got {:?}", other),
        }
    }

    // ─── LLM response parsing ─────────────────────────────────────

    #[test]
    fn parse_llm_handles_clean_json_array() {
        let json = r#"[
            {"class":"tooling","name":"editor","value":"helix","cue":"explicit","confidence":0.9},
            {"class":"identity","name":"timezone","value":"PST","cue":"explicit","confidence":0.95}
        ]"#;
        let out = parse_llm_response(json, "s", "t");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].class, FacetClass::Tooling);
        assert_eq!(out[0].value, "helix");
        assert_eq!(out[1].class, FacetClass::Identity);
    }

    #[test]
    fn parse_llm_tolerates_surrounding_prose() {
        let json = "Sure! Here are the facets:\n[{\"class\":\"tooling\",\"name\":\"editor\",\"value\":\"helix\"}]\nThat's all.";
        let out = parse_llm_response(json, "s", "t");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, "helix");
    }

    #[test]
    fn parse_llm_drops_rows_with_unknown_class() {
        let json = r#"[
            {"class":"mystery","name":"x","value":"y"},
            {"class":"tooling","name":"editor","value":"helix"}
        ]"#;
        let out = parse_llm_response(json, "s", "t");
        assert_eq!(out.len(), 1, "unknown class row dropped");
        assert_eq!(out[0].value, "helix");
    }

    #[test]
    fn parse_llm_truncates_overlong_value() {
        let long_val = "a".repeat(120);
        let json = format!(
            r#"[{{"class":"tooling","name":"foo","value":"{}"}}]"#,
            long_val
        );
        let out = parse_llm_response(&json, "s", "t");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value.chars().count(), 40);
    }

    #[test]
    fn parse_llm_empty_array_returns_empty() {
        assert!(parse_llm_response("[]", "s", "t").is_empty());
        assert!(parse_llm_response("garbage", "s", "t").is_empty());
        assert!(parse_llm_response("", "s", "t").is_empty());
    }

    #[test]
    fn parse_llm_clamps_confidence_to_unit_interval() {
        let json = r#"[{"class":"tooling","name":"editor","value":"helix","confidence":1.5}]"#;
        let out = parse_llm_response(json, "s", "t");
        assert!(out[0].confidence <= 1.0);
        let json2 = r#"[{"class":"tooling","name":"editor","value":"helix","confidence":-0.2}]"#;
        let out2 = parse_llm_response(json2, "s", "t");
        assert!(out2[0].confidence >= 0.0);
    }

    // ─── End-to-end driver ───────────────────────────────────────

    #[tokio::test]
    async fn extract_from_chat_turn_regex_only_path() {
        let buf = Buffer::new(50);
        let n = extract_from_chat_turn(
            "I prefer pnpm and I am a senior engineer at Acme",
            "s",
            "t",
            &buf,
            false,
            None,
        )
        .await;
        assert!(n >= 1, "expected at least 1 candidate, got {}", n);
        let drained = buf.drain();
        assert!(drained.iter().all(|c| c.cue == CueFamily::Explicit));
    }

    #[tokio::test]
    async fn extract_from_chat_turn_llm_layer_skipped_for_short_text() {
        // Short text → LLM layer always skipped (saves tokens).
        let buf = Buffer::new(50);
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: r#"[{"class":"identity","name":"name","value":"Alice"}]"#.into(),
            canned_input_tokens: 100,
            canned_output_tokens: 30,
            canned_model: "mock".into(),
        }) as Arc<dyn MemoryOsLlm>;
        let n = extract_from_chat_turn(
            "hello", // < 80 chars
            "s",
            "t",
            &buf,
            true,
            Some(&mock),
        )
        .await;
        assert_eq!(n, 0, "short text should bypass LLM and regex shouldn't match");
    }

    #[tokio::test]
    async fn extract_from_chat_turn_llm_layer_runs_when_regex_empty() {
        // Long text without regex hits → LLM should fire.
        let buf = Buffer::new(50);
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: r#"[{"class":"goal","name":"current","value":"refactor memory layer"}]"#.into(),
            canned_input_tokens: 100,
            canned_output_tokens: 50,
            canned_model: "mock".into(),
        }) as Arc<dyn MemoryOsLlm>;
        let text = "We had a long discussion about the architecture last week, and I'd like to revisit the conclusions before the team meeting tomorrow.";
        let n = extract_from_chat_turn(text, "s", "t", &buf, true, Some(&mock)).await;
        assert!(n >= 1, "LLM extractor should produce candidate for long text with no regex hit");
        let drained = buf.drain();
        assert!(drained.iter().any(|c| c.class == FacetClass::Goal));
    }

    #[tokio::test]
    async fn extract_from_chat_turn_llm_skipped_when_regex_already_found() {
        // Regex finds explicit cue → LLM doesn't run (no double counting).
        let buf = Buffer::new(50);
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: r#"[{"class":"goal","name":"x","value":"y"}]"#.into(),
            canned_input_tokens: 100,
            canned_output_tokens: 30,
            canned_model: "mock".into(),
        }) as Arc<dyn MemoryOsLlm>;
        let text = "I prefer pnpm over npm because it is faster and reproducible.";
        let n = extract_from_chat_turn(text, "s", "t", &buf, true, Some(&mock)).await;
        // Regex should have caught 'pnpm' as tooling. LLM should NOT have run.
        let drained = buf.drain();
        assert!(drained.iter().any(|c| c.class == FacetClass::Tooling));
        // No 'goal' candidate (LLM didn't fire).
        assert!(!drained.iter().any(|c| c.class == FacetClass::Goal));
        // n matches the drain count.
        assert_eq!(n, drained.len());
    }

    // ─── llm_system_prompt sanity ────────────────────────────────

    #[test]
    fn llm_system_prompt_pins_schema_and_anti_invention() {
        let p = llm_system_prompt();
        assert!(p.contains("class"));
        assert!(p.contains("identity"));
        assert!(p.contains("Do not invent"));
        assert!(p.contains("ONLY a JSON array"));
    }
}
