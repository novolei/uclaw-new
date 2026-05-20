use std::collections::HashMap;

use super::types::*;

#[derive(Debug, Clone, Default)]
pub struct ModerationLedger {
    warnings: HashMap<String, u32>,
}

pub fn decide_moderation(
    cfg: &ModerationConfig,
    ledger: &mut ModerationLedger,
    comments: &[LiveComment],
    now_ms: i64,
) -> ModerationDecision {
    let mut by_author: HashMap<&str, Vec<&LiveComment>> = HashMap::new();
    let window_start = now_ms - cfg.spam_window_seconds * 1000;
    for comment in comments.iter().filter(|c| c.timestamp_ms >= window_start) {
        if cfg.whitelisted_author_ids.contains(&comment.author_id) {
            continue;
        }
        by_author.entry(&comment.author_id).or_default().push(comment);
    }

    let mut actions = Vec::new();
    for (author_id, items) in by_author {
        if items.len() < cfg.spam_threshold {
            continue;
        }
        let warning_count = ledger.warnings.entry(author_id.to_string()).or_insert(0);
        let kind = if *warning_count >= 2 {
            ModerationActionKind::Mute
        } else {
            *warning_count += 1;
            ModerationActionKind::Warn
        };
        actions.push(ModerationAction {
            kind,
            author_id: author_id.to_string(),
            reason: "spam_repeated".to_string(),
            evidence_comment_ids: items
                .iter()
                .map(|comment| comment.platform_comment_id.clone())
                .collect(),
        });
    }
    ModerationDecision { actions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(author_id: &str, text: &str, at: i64) -> LiveComment {
        LiveComment {
            platform: "douyin".into(),
            platform_comment_id: format!("{author_id}-{at}"),
            author_id: author_id.into(),
            author_name: author_id.into(),
            text: text.into(),
            timestamp_ms: at,
            badges: vec![],
            is_new: true,
        }
    }

    #[test]
    fn repeated_spam_warns_first_then_mutes_after_two_warnings() {
        let mut ledger = ModerationLedger::default();
        let cfg = ModerationConfig::default();
        let comments = vec![
            comment("u1", "buy now", 0),
            comment("u1", "buy now", 10_000),
            comment("u1", "buy now", 20_000),
            comment("u1", "buy now", 30_000),
            comment("u1", "buy now", 40_000),
        ];
        let first = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert_eq!(first.actions[0].kind, ModerationActionKind::Warn);
        let second = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert_eq!(second.actions[0].kind, ModerationActionKind::Warn);
        let third = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert_eq!(third.actions[0].kind, ModerationActionKind::Mute);
    }

    #[test]
    fn whitelisted_users_are_never_punished() {
        let mut ledger = ModerationLedger::default();
        let cfg = ModerationConfig {
            whitelisted_author_ids: vec!["host".into()],
            ..ModerationConfig::default()
        };
        let comments = vec![comment("host", "spam", 0), comment("host", "spam", 1)];
        let decision = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert!(decision.actions.is_empty());
    }

    #[test]
    fn old_comments_outside_window_do_not_trigger_action() {
        let mut ledger = ModerationLedger::default();
        let cfg = ModerationConfig::default();
        let comments = vec![
            comment("u1", "buy now", 0),
            comment("u1", "buy now", 10_000),
            comment("u1", "buy now", 20_000),
            comment("u1", "buy now", 30_000),
            comment("u1", "buy now", 40_000),
        ];
        let decision = decide_moderation(&cfg, &mut ledger, &comments, 180_000);
        assert!(decision.actions.is_empty());
    }
}
