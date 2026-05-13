use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, Subscription};

/// Build the system prompt for an automation run from the spec's `system_prompt` field.
pub fn build_system_prompt(spec: &HumaneAutomationSpec) -> String {
    spec.system_prompt.clone()
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
}
