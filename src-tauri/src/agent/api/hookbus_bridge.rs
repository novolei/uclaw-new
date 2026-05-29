//! Event ↔ HookEvent translation for the P3-3 bridge.
//!
//! AgentApi has 13 EventKinds; HookBus has 13 HookEvent variants. ~5 events
//! overlap semantically; the rest are AgentApi-only (no HookBus dispatch).
//! `event_to_hook_event` returns `None` for non-overlapping events.

use crate::agent::api::events::{Event, EventKind, EventOutcome, EventPayload};
use crate::agent::hook_bus::HookEvent;
use crate::runtime::contracts::HookDecision;

/// Translate an AgentApi `Event` into a `HookEvent` for HookBus dispatch.
/// Returns `None` for events that have no HookBus peer (AgentApi-only kinds).
///
/// `task_id` for HookEvent maps to `event.session_id` — uClaw's HookBus
/// callers (dispatcher, tool_dispatch) use session_id as the grouping key
/// today, so this matches existing semantics.
pub fn event_to_hook_event(event: &Event) -> Option<HookEvent> {
    let task_id = event.session_id.clone();
    match (&event.kind, &event.payload) {
        (EventKind::ToolCall, EventPayload::ToolCall { tool_name, args }) => {
            Some(HookEvent::PreToolUse {
                task_id,
                tool_name: tool_name.clone(),
                args_json: args.to_string(),
            })
        }
        (EventKind::ToolResult, EventPayload::ToolResult { tool_name, result }) => {
            // Heuristic: success = no "error" key at top level of result.
            let success = result.get("error").is_none();
            let result_preview = {
                let s = result.to_string();
                if s.len() > 200 {
                    format!("{}…", &s[..200])
                } else {
                    s
                }
            };
            Some(HookEvent::PostToolUse {
                task_id,
                tool_name: tool_name.clone(),
                success,
                result_preview,
            })
        }
        (
            EventKind::BeforeProviderRequest,
            EventPayload::BeforeProviderRequest { provider, model },
        ) => Some(HookEvent::PreLlmCall {
            task_id,
            provider: provider.clone(),
            model: model.clone(),
            prompt_tokens_estimate: 0,
        }),
        (
            EventKind::AfterProviderResponse,
            EventPayload::AfterProviderResponse {
                provider,
                model,
                token_count,
            },
        ) => Some(HookEvent::PostLlmCall {
            task_id,
            provider: provider.clone(),
            model: model.clone(),
            // AfterProviderResponse carries a single token_count; map to
            // output_tokens (the count most callers care about for billing).
            // input_tokens is unknown at this point — default to 0.
            input_tokens: 0,
            output_tokens: *token_count,
        }),
        (EventKind::BeforeContextAssembly, EventPayload::BeforeContextAssembly { .. }) => {
            Some(HookEvent::PreContextInject {
                task_id,
                // AgentApi's BeforeContextAssembly payload carries no fragment
                // list yet; fragment enumeration lives in the context pipeline
                // that fires after this event. Provide empty slice as a
                // conservative default — hooks can observe the event kind even
                // without fragment detail at this stage.
                fragment_ids: Vec::new(),
                total_tokens_estimate: 0,
            })
        }
        // AgentApi-only kinds — no HookBus dispatch:
        _ => None,
    }
}

/// Translate a `HookDecision` back to an `EventOutcome`. Used by
/// `AgentApi.emit_with_decision()` when folding HookBus's decision into
/// the loop's outcome.
pub fn hook_decision_to_event_outcome(d: HookDecision) -> EventOutcome {
    match d {
        HookDecision::Allow => EventOutcome::Continue,
        HookDecision::Deny { reason } => EventOutcome::Abort(reason),
        HookDecision::AskUser { prompt, risk_class } => {
            // Encode as "askuser:<risk_class>:<prompt>" so callers can parse
            // both the risk level and the user-facing message without a
            // separate struct. risk_class defaults to "medium" when absent.
            let risk = match risk_class {
                Some(r) => format!("{r:?}").to_lowercase(),
                None => "medium".to_string(),
            };
            EventOutcome::Abort(format!("askuser:{}:{}", risk, prompt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    fn make_event(kind: EventKind, payload: EventPayload) -> Event {
        Event {
            kind,
            payload,
            session_id: "s1".into(),
            cancellation_token: CancellationToken::new(),
        }
    }

    #[test]
    fn tool_call_translates_to_pre_tool_use() {
        let ev = make_event(
            EventKind::ToolCall,
            EventPayload::ToolCall {
                tool_name: "echo".into(),
                args: serde_json::json!({"msg": "hi"}),
            },
        );
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PreToolUse {
                task_id,
                tool_name,
                args_json,
            } => {
                assert_eq!(task_id, "s1");
                assert_eq!(tool_name, "echo");
                assert!(args_json.contains("hi"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_result_translates_to_post_tool_use_with_success_heuristic() {
        let ev_ok = make_event(
            EventKind::ToolResult,
            EventPayload::ToolResult {
                tool_name: "echo".into(),
                result: serde_json::json!({"out": "ok"}),
            },
        );
        let h = event_to_hook_event(&ev_ok).unwrap();
        match h {
            HookEvent::PostToolUse { success, .. } => {
                assert!(success, "no error key → success")
            }
            _ => panic!(),
        }

        let ev_err = make_event(
            EventKind::ToolResult,
            EventPayload::ToolResult {
                tool_name: "echo".into(),
                result: serde_json::json!({"error": "fail"}),
            },
        );
        let h = event_to_hook_event(&ev_err).unwrap();
        match h {
            HookEvent::PostToolUse { success, .. } => {
                assert!(!success, "error key → not success")
            }
            _ => panic!(),
        }
    }

    #[test]
    fn before_provider_request_translates_to_pre_llm_call() {
        let ev = make_event(
            EventKind::BeforeProviderRequest,
            EventPayload::BeforeProviderRequest {
                provider: "anthropic".into(),
                model: "claude-opus-4-7".into(),
            },
        );
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PreLlmCall { provider, model, .. } => {
                assert_eq!(provider, "anthropic");
                assert_eq!(model, "claude-opus-4-7");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn after_provider_response_translates_to_post_llm_call() {
        let ev = make_event(
            EventKind::AfterProviderResponse,
            EventPayload::AfterProviderResponse {
                provider: "anthropic".into(),
                model: "claude-opus-4-7".into(),
                token_count: 42,
            },
        );
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PostLlmCall {
                output_tokens,
                input_tokens,
                ..
            } => {
                assert_eq!(output_tokens, 42);
                assert_eq!(input_tokens, 0);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn before_context_assembly_translates_to_pre_context_inject() {
        let ev = make_event(
            EventKind::BeforeContextAssembly,
            EventPayload::BeforeContextAssembly {
                session_id: "s1".into(),
            },
        );
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PreContextInject {
                task_id,
                fragment_ids,
                total_tokens_estimate,
            } => {
                assert_eq!(task_id, "s1");
                assert!(fragment_ids.is_empty());
                assert_eq!(total_tokens_estimate, 0);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn agentapi_only_kinds_return_none() {
        let kinds_and_payloads: Vec<(EventKind, EventPayload)> = vec![
            (
                EventKind::SessionStart,
                EventPayload::SessionStart {
                    session_id: "s".into(),
                },
            ),
            (
                EventKind::SessionShutdown,
                EventPayload::SessionShutdown {
                    session_id: "s".into(),
                },
            ),
            (
                EventKind::TurnStart,
                EventPayload::TurnStart {
                    turn_id: "t".into(),
                },
            ),
            (
                EventKind::TurnEnd,
                EventPayload::TurnEnd {
                    turn_id: "t".into(),
                    duration_ms: 0,
                },
            ),
            (
                EventKind::MessageStart,
                EventPayload::MessageStart {
                    message_id: "m".into(),
                },
            ),
            (
                EventKind::MessageEnd,
                EventPayload::MessageEnd {
                    message_id: "m".into(),
                },
            ),
            (
                EventKind::BeforeCancellation,
                EventPayload::BeforeCancellation {
                    reason: "r".into(),
                },
            ),
            (
                EventKind::PluginShutdown,
                EventPayload::PluginShutdown {
                    plugin_id: "p".into(),
                },
            ),
        ];
        for (kind, payload) in kinds_and_payloads {
            let ev = make_event(kind, payload);
            assert!(
                event_to_hook_event(&ev).is_none(),
                "{:?} should not translate",
                kind
            );
        }
    }

    #[test]
    fn hook_decision_allow_maps_to_continue() {
        assert!(matches!(
            hook_decision_to_event_outcome(HookDecision::Allow),
            EventOutcome::Continue
        ));
    }

    #[test]
    fn hook_decision_deny_maps_to_abort_with_reason() {
        let d = HookDecision::Deny {
            reason: "policy denied".to_string(),
        };
        if let EventOutcome::Abort(reason) = hook_decision_to_event_outcome(d) {
            assert_eq!(reason, "policy denied");
        } else {
            panic!("expected Abort");
        }
    }

    #[test]
    fn hook_decision_askuser_with_risk_class_maps_to_abort_askuser() {
        use crate::runtime::contracts::RiskClass;
        let d = HookDecision::AskUser {
            prompt: "Confirm deletion?".to_string(),
            risk_class: Some(RiskClass::High),
        };
        if let EventOutcome::Abort(reason) = hook_decision_to_event_outcome(d) {
            assert!(reason.starts_with("askuser:"), "prefix missing: {reason}");
            assert!(reason.contains("high"), "risk class missing: {reason}");
            assert!(reason.contains("Confirm deletion?"), "prompt missing: {reason}");
        } else {
            panic!("expected Abort");
        }
    }

    #[test]
    fn hook_decision_askuser_without_risk_class_defaults_to_medium() {
        let d = HookDecision::AskUser {
            prompt: "Are you sure?".to_string(),
            risk_class: None,
        };
        if let EventOutcome::Abort(reason) = hook_decision_to_event_outcome(d) {
            assert!(reason.starts_with("askuser:medium:"), "expected medium default: {reason}");
        } else {
            panic!("expected Abort");
        }
    }
}
