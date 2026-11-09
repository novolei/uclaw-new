//! LLM prompts for memory growth — system prompts ported verbatim from
//! uclaw-pi `reflection_service.rs` (incl. the anti-invention sentence added
//! in 04ed4e2c).

use super::store::ReflectionRow;

pub const REFLECTION_SYSTEM: &str = "You are a reflection agent. Read the recent conversation between a user and an AI assistant and distill ONE durable, high-value insight about the user, their goals, working style, or the project — something worth remembering across future sessions. Ignore one-off chit-chat. Respond with ONLY a JSON object, no prose, no markdown fences:\n{\"insight\": \"<one concise sentence>\", \"confidence\": <float 0.0-1.0>}";

pub const PROMOTION_SYSTEM: &str = "You are a user-modeling agent. You are given a list of learned facts, rules, and preferences about a single user (their identity, tooling, working style, vetoes, and goals). Synthesize them into ONE compact, durable user model: a few sentences capturing who this user is and how they prefer to work, so a future assistant can serve them well without re-learning. Write plain prose (no JSON, no markdown fences, no preamble). Prefer concrete, high-signal traits over hedging. CRITICAL: state ONLY what the provided facts support — do NOT infer or invent occupation, age, identity, or preferences that are not explicitly present in the input. Keep it under ~120 words.";

pub const CONSOLIDATION_SYSTEM: &str = "You are curating an AI agent's long-term memory. You are given (1) a list of distilled reflections about one user, (2) the current user_model summary, and (3) the grounded facts the user_model must rest on. Do two things. (a) MERGE only near-identical or strongly-overlapping reflections into a single deduplicated reflection, keeping the highest confidence of each merged group; leave genuinely distinct insights untouched and NEVER invent new ones — the output set must be the same size or smaller. (b) Rewrite the user_model so every claim is STRICTLY supported by the provided facts: remove any occupation, age, identity, or preference not present in the facts; do not extrapolate. Respond with ONLY a JSON object, no prose, no markdown fences:\n{\"reflections\":[{\"insight\":\"<sentence>\",\"confidence\":<float 0.0-1.0>}],\"user_model\":\"<prose>\"}";

pub fn reflection_user(transcript: &str) -> String {
    format!("Recent conversation (oldest first):\n\n{transcript}\n\nDistill one insight as JSON.")
}

pub fn promotion_user(digest: &str) -> String {
    format!("Learned facts, rules, and preferences about the user:\n\n{digest}\n\nSynthesize the user model now.")
}

pub fn consolidation_user(reflections: &[ReflectionRow], user_model: Option<&str>, digest: Option<&str>) -> String {
    let block: String = reflections
        .iter()
        .map(|r| format!("- ({:.2}) {}\n", r.confidence, r.insight))
        .collect();
    format!(
        "Reflections (one per line, with confidence):\n{block}\nCurrent user_model:\n{}\n\nGrounded facts:\n{}\n\nCurate now.",
        user_model.unwrap_or("(none yet)"),
        digest.unwrap_or("(no facts yet)"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consolidation_user_lists_reflections_and_placeholders() {
        let rows = vec![ReflectionRow { id: "a".into(), insight: "likes TDD".into(), confidence: 0.8, created_at: 0, archived_at: None }];
        let p = consolidation_user(&rows, None, None);
        assert!(p.contains("- (0.80) likes TDD\n"));
        assert!(p.contains("Current user_model:\n(none yet)"));
        assert!(p.contains("Grounded facts:\n(no facts yet)"));
    }
}
