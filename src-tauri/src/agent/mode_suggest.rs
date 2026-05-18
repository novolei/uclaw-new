//! Backend keyword detector for plan-mode auto-suggest.
//!
//! Pure function — no I/O, no state — so it's trivially testable and
//! callable from the request hot path with zero overhead.

use crate::safety::SafetyMode;

#[derive(Debug, Clone, PartialEq)]
pub struct PlanModeHint {
    /// The matched pattern string. Used as the telemetry key.
    pub pattern: &'static str,
    /// Display copy shown in the banner when no agent reason is provided.
    pub display_reason: &'static str,
}

/// Starter pattern table. Bilingual, high-recall. Tuned down post-ship
/// via the plan_mode_calibration scenario (Task 10).
///
/// IMPORTANT: longer/more-specific patterns must come before shorter ones
/// that are sub-strings of them (e.g. "怎么实现" before "实现"), so the
/// first match is the most precise one.
static PATTERNS: &[(&str, &str)] = &[
    // Chinese "how should we" questions — more specific, listed first
    ("怎么实现", "建议先 Plan 一下实现路径"),
    ("如何实现", "建议先 Plan 一下实现路径"),
    ("怎么搭", "建议先 Plan 一下搭建步骤"),
    ("怎么做", "如果涉及多步，建议先 Plan"),
    ("怎么搞", "如果涉及多步，建议先 Plan"),
    // Chinese verbs — shorter, listed after more-specific phrases
    ("计划", "建议先在 Plan 模式过一遍方案"),
    ("规划", "建议先在 Plan 模式过一遍方案"),
    ("设计", "设计类任务先 Plan 一下结构更稳"),
    ("实现", "多步实现先 Plan 一下"),
    ("搭建", "搭建类任务建议先 Plan"),
    ("构建", "构建类任务建议先 Plan"),
    ("重构", "重构涉及多文件，建议先 Plan"),
    ("开发", "开发任务先 Plan 一下"),
    // English
    ("how should", "Sounds like planning — try Plan mode?"),
    ("how do i", "Sounds like planning — try Plan mode?"),
    ("how to ", "Sounds like planning — try Plan mode?"),
    ("let's build", "Build it — Plan mode first?"),
    ("plan", "Worth planning first?"),
    ("design", "Design-heavy — try Plan mode?"),
    ("refactor", "Refactor — try Plan mode?"),
];

/// Returns Some(hint) when the user message looks like it should be
/// planned before executed. Gates (cheapest first):
///   - session dedupe already fired → None
///   - already in a safer mode (Plan/AcceptEdits/Ask) → None
///   - msg shorter than 15 chars → None
///   - no pattern match → None
pub fn suggest_plan_mode(
    user_msg: &str,
    current_mode: &SafetyMode,
    already_suggested_this_session: bool,
    disabled_patterns: &[String],
) -> Option<PlanModeHint> {
    if already_suggested_this_session {
        return None;
    }
    if !matches!(current_mode, SafetyMode::Supervised | SafetyMode::Yolo) {
        return None;
    }
    if user_msg.chars().count() < 15 {
        return None;
    }
    let lower = user_msg.to_lowercase();
    for (pat, reason) in PATTERNS {
        if disabled_patterns.iter().any(|d| d == pat) {
            continue;
        }
        // Match case-insensitively for English, case-preserving for CJK.
        if lower.contains(pat) || user_msg.contains(pat) {
            return Some(PlanModeHint { pattern: pat, display_reason: reason });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_disabled() -> Vec<String> { Vec::new() }

    // ── Positive matches (should suggest) ─────────────────────────
    #[test]
    fn chinese_verb_planning() {
        let hint = suggest_plan_mode(
            "帮我做个网页五子棋开发计划，要支持悔棋",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "计划");
    }

    #[test]
    fn chinese_how_to_question() {
        let hint = suggest_plan_mode(
            "这个登录流程怎么实现比较合理？",
            &SafetyMode::Supervised, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "怎么实现");
    }

    #[test]
    fn english_let_us_build() {
        let hint = suggest_plan_mode(
            "Let's build a multiplayer chess game in React",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "let's build");
    }

    // ── Negative: gates fire ──────────────────────────────────────
    #[test]
    fn already_suggested_short_circuits() {
        let hint = suggest_plan_mode(
            "帮我做个网页五子棋开发计划",
            &SafetyMode::Yolo, /*already=*/true, &no_disabled(),
        );
        assert!(hint.is_none());
    }

    #[test]
    fn safer_mode_short_circuits() {
        for mode in [SafetyMode::Plan, SafetyMode::AcceptEdits, SafetyMode::Ask] {
            let hint = suggest_plan_mode(
                "帮我做个完整的开发计划",
                &mode, false, &no_disabled(),
            );
            assert!(hint.is_none(), "mode {:?} should not suggest", mode);
        }
    }

    #[test]
    fn short_message_short_circuits() {
        // "做计划" is 3 chars < 15
        let hint = suggest_plan_mode("做计划", &SafetyMode::Yolo, false, &no_disabled());
        assert!(hint.is_none());
    }

    #[test]
    fn disabled_pattern_skipped() {
        let disabled = vec!["计划".to_string()];
        // "计划" disabled → should fall through to "实现" match
        let hint = suggest_plan_mode(
            "做个五子棋计划，主要实现五连珠胜负检测",
            &SafetyMode::Yolo, false, &disabled,
        );
        assert_eq!(hint.unwrap().pattern, "实现");
    }

    #[test]
    fn no_match_returns_none() {
        let hint = suggest_plan_mode(
            "今天天气怎么样啊，北京下雨了吗",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert!(hint.is_none());
    }

    // ── Edge case: unrelated message that contains a pattern word
    #[test]
    fn pattern_in_unrelated_context_still_fires() {
        // Acceptable trade-off — calibration loop suppresses bad patterns
        // post-hoc. v1 favors recall over precision.
        let hint = suggest_plan_mode(
            "我已经有计划了，今天不需要你帮忙",
            &SafetyMode::Yolo, false, &no_disabled(),
        );
        assert_eq!(hint.unwrap().pattern, "计划");
    }
}
