//! Render growth data: the transcript for the LLM and the per-turn prompt
//! block (`<user_model>` + `<recent_reflections>`).

use super::store::{ReflectionRow, TranscriptMessage};

/// Per-message cap in the transcript sent to the reflection pass.
const TRANSCRIPT_MESSAGE_MAX_CHARS: usize = 2000;
/// Prompt caps (characters).
pub const USER_MODEL_MAX_CHARS: usize = 1200;
pub const REFLECTION_MAX_CHARS: usize = 300;
pub const REFLECTIONS_IN_PROMPT: usize = 3;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// `{role}: {content}\n` per message, each capped.
pub fn render_transcript(msgs: &[TranscriptMessage]) -> String {
    msgs.iter()
        .map(|m| format!("{}: {}\n", m.role, truncate_chars(m.content.trim(), TRANSCRIPT_MESSAGE_MAX_CHARS)))
        .collect()
}

/// The per-turn block; `None` when there is neither a user model nor a
/// live reflection.
pub fn render_growth_block(user_model: Option<&str>, reflections: &[ReflectionRow]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(um) = user_model.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("<user_model>\n{}\n</user_model>", truncate_chars(um, USER_MODEL_MAX_CHARS)));
    }
    let lines: String = reflections
        .iter()
        .take(REFLECTIONS_IN_PROMPT)
        .map(|r| format!("- {}\n", truncate_chars(r.insight.trim(), REFLECTION_MAX_CHARS)))
        .collect();
    if !lines.is_empty() {
        parts.push(format!("<recent_reflections>\n{lines}</recent_reflections>"));
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refl(insight: &str) -> ReflectionRow {
        ReflectionRow { id: insight.into(), insight: insight.into(), confidence: 0.5, created_at: 0, archived_at: None }
    }

    #[test]
    fn render_transcript_caps_each_message() {
        let msgs = vec![
            TranscriptMessage { role: "user".into(), content: "hi".into(), created_at: 1 },
            TranscriptMessage { role: "assistant".into(), content: "长".repeat(2500), created_at: 2 },
        ];
        let t = render_transcript(&msgs);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "user: hi");
        assert_eq!(lines[1].chars().count(), "assistant: ".len() + TRANSCRIPT_MESSAGE_MAX_CHARS + 1);
        assert!(lines[1].ends_with('…'));
    }

    #[test]
    fn render_growth_block_caps_and_orders() {
        assert_eq!(render_growth_block(None, &[]), None);
        assert_eq!(render_growth_block(Some("  "), &[]), None, "blank model and no reflections");

        let rs = vec![refl("r1"), refl("r2"), refl("r3"), refl("r4")];
        let block = render_growth_block(Some("Ships small PRs."), &rs).unwrap();
        let um = block.find("<user_model>\nShips small PRs.\n</user_model>").expect("user model first");
        let rr = block.find("<recent_reflections>").expect("reflections block");
        assert!(um < rr);
        assert!(block.contains("- r1\n- r2\n- r3\n</recent_reflections>"));
        assert!(!block.contains("r4"), "only the newest 3");

        let long = render_growth_block(Some(&"模".repeat(5000)), &[refl(&"反".repeat(900))]).unwrap();
        let um_body = long.split("<user_model>\n").nth(1).unwrap().split("\n</user_model>").next().unwrap();
        assert_eq!(um_body.chars().count(), USER_MODEL_MAX_CHARS + 1);
        assert!(um_body.ends_with('…'));
        let line = long.lines().find(|l| l.starts_with("- ")).unwrap();
        assert_eq!(line.chars().count(), 2 + REFLECTION_MAX_CHARS + 1);

        let only_refl = render_growth_block(None, &[refl("x")]).unwrap();
        assert!(!only_refl.contains("<user_model>"));
    }
}
