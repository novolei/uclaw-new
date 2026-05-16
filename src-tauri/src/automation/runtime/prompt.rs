use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, Subscription};

/// Build the system prompt for an automation run from the spec's `system_prompt` field.
/// Appends a <system_info> block with current time so the agent treats it as authoritative.
pub fn build_system_prompt(spec: &HumaneAutomationSpec) -> String {
    let mut prompt = spec.system_prompt.clone();
    prompt.push('\n');
    prompt.push_str(&build_system_time_block());
    prompt
}

/// Build the <system_info> block with current date/time for authoritative time context.
/// Mirrors `ChatDelegate::build_system_time_block()` for consistency.
pub fn build_system_time_block() -> String {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "周一",
        chrono::Weekday::Tue => "周二",
        chrono::Weekday::Wed => "周三",
        chrono::Weekday::Thu => "周四",
        chrono::Weekday::Fri => "周五",
        chrono::Weekday::Sat => "周六",
        chrono::Weekday::Sun => "周日",
    };
    let time = format!(
        "{}年{}月{}日 {} {:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        weekday,
        now.hour(),
        now.minute(),
    );
    format!(
        "<system_info>\n当前时间: {}\n注意: 以上时间由系统提供，你不需要使用工具（如 bash date）获取时间，直接使用此信息回答即可。\n</system_info>",
        time
    )
}

/// Context needed when resuming a run after a user resolved an escalation.
pub struct EscalationResolution {
    pub question: String,
    pub user_choice: String,
    pub user_note: Option<String>,
}

/// Build the initial user message injected at the start of every automation run.
///
/// Includes the trigger type, payload, and user-supplied config values.
/// When resuming from an escalation, appends the resolution block so the
/// agent understands the decision that was made.
pub fn build_initial_message(
    subscription: Option<&Subscription>,
    trigger_payload: &serde_json::Value,
    user_config: &serde_json::Value,
    resumption: Option<&EscalationResolution>,
) -> String {
    let source_type = subscription.map(|_| "subscription").unwrap_or("manual");
    let mut out = format!(
        "## Trigger\n type={}\n payload={}\n config={}",
        source_type,
        serde_json::to_string(trigger_payload).unwrap_or_default(),
        serde_json::to_string(user_config).unwrap_or_default(),
    );
    if let Some(r) = resumption {
        out.push_str(&format!(
            "\n\n## Resuming from escalation\n question={}\n user_choice={}\n user_note={}",
            r.question,
            r.user_choice,
            r.user_note.as_deref().unwrap_or(""),
        ));
    }
    out
}

/// Like `build_initial_message`, but prepends a `## Memory` block with the
/// spec's Tier-1 persistent memory so the agent starts the run already
/// knowing its accumulated context (design §0.8). An empty `memory` string
/// produces no block.
pub fn build_initial_message_with_memory(
    subscription: Option<&Subscription>,
    trigger_payload: &serde_json::Value,
    user_config: &serde_json::Value,
    resumption: Option<&EscalationResolution>,
    memory: &str,
) -> String {
    let base = build_initial_message(subscription, trigger_payload, user_config, resumption);
    if memory.trim().is_empty() {
        base
    } else {
        format!("## Memory\n{}\n\n{}", memory.trim(), base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initial_message_contains_trigger_block() {
        let m = build_initial_message(None, &json!({"foo": 1}), &json!({}), None);
        assert!(m.contains("## Trigger"));
        assert!(m.contains("payload"));
        assert!(m.contains("type=manual"));
    }

    #[test]
    fn initial_message_with_resumption() {
        let r = EscalationResolution {
            question: "Q?".into(),
            user_choice: "a".into(),
            user_note: None,
        };
        let m = build_initial_message(None, &json!({}), &json!({}), Some(&r));
        assert!(m.contains("Resuming from escalation"));
        assert!(m.contains("user_choice=a"));
        assert!(m.contains("question=Q?"));
    }

    #[test]
    fn initial_message_with_resumption_note() {
        let r = EscalationResolution {
            question: "Do X?".into(),
            user_choice: "yes".into(),
            user_note: Some("please proceed".into()),
        };
        let m = build_initial_message(None, &json!({}), &json!({}), Some(&r));
        assert!(m.contains("user_note=please proceed"));
    }

    #[test]
    fn build_system_prompt_returns_spec_field() {
        let spec = serde_json::from_value::<HumaneAutomationSpec>(json!({
            "type": "automation",
            "name": "test-spec",
            "version": "1.0.0",
            "author": "tester",
            "description": "test",
            "system_prompt": "You are a test agent.",
        }))
        .unwrap();
        assert_eq!(build_system_prompt(&spec), "You are a test agent.");
    }

    #[test]
    fn initial_message_includes_memory_block_when_present() {
        let m = build_initial_message_with_memory(
            None, &json!({}), &json!({}), None, "remembered: the API key rotates monthly",
        );
        assert!(m.contains("## Memory"));
        assert!(m.contains("API key rotates monthly"));
        assert!(m.contains("## Trigger"));
    }

    #[test]
    fn initial_message_omits_memory_block_when_empty() {
        let m = build_initial_message_with_memory(None, &json!({}), &json!({}), None, "");
        assert!(!m.contains("## Memory"));
        assert!(m.contains("## Trigger"));
    }
}
