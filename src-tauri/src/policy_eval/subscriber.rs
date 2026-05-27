//! PolicySpecSubscriber —— 把 PolicySpec 接入共享 HookBus(Sprint 3 ②)。
//! 5 个 decision-capable 事件 → ActionRequest → evaluate → HookDecision。
use crate::agent::hook_bus::{HookEvent, HookEventKind, HookSubscriber, SubscriberId};
use crate::policy_eval::{evaluate, ActionRequest, PolicySpec};
use crate::runtime::contracts::{HookDecision, RiskClass};
use async_trait::async_trait;

/// 把一个 decision-capable HookEvent 映射成 PolicySpec 的 ActionRequest。
/// 不关心 / 非映射的事件返回 None(订阅者据此放行)。
pub(crate) fn action_request_from_event(event: &HookEvent) -> Option<ActionRequest> {
    match event {
        HookEvent::PreToolUse { tool_name, .. } =>
            Some(ActionRequest::new("tool_use", tool_name.clone(), RiskClass::Low)),
        HookEvent::MemoryWrite { topic, .. } =>
            Some(ActionRequest::new("memory_write", topic.clone(), RiskClass::Low)),
        HookEvent::PrePermission { action, target, .. } =>
            Some(ActionRequest::new(action.clone(), target.clone(), RiskClass::Low)),
        HookEvent::PreLlmCall { model, .. } =>
            Some(ActionRequest::new("llm_call", model.clone(), RiskClass::Low)),
        HookEvent::PreContextInject { .. } =>
            Some(ActionRequest::new("context_inject", "", RiskClass::Low)),
        _ => None,
    }
}

pub struct PolicySpecSubscriber {
    spec: PolicySpec,
}

impl PolicySpecSubscriber {
    pub fn new(spec: PolicySpec) -> Self { Self { spec } }
}

#[async_trait]
impl HookSubscriber for PolicySpecSubscriber {
    fn id(&self) -> SubscriberId { SubscriberId::new("policy-spec") }
    fn interest_in(&self) -> &'static [HookEventKind] {
        &[
            HookEventKind::PreToolUse,
            HookEventKind::PreLlmCall,
            HookEventKind::PrePermission,
            HookEventKind::PreContextInject,
            HookEventKind::MemoryWrite,
        ]
    }
    async fn on_event(&self, event: &HookEvent) -> Option<HookDecision> {
        let req = action_request_from_event(event)?;
        let (decision, _rule_id) = evaluate(&self.spec, &req);
        Some(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_eval::{MatchPattern, PolicyRule};

    fn deny_tool(name: &str) -> PolicySpec {
        PolicySpec::new().with_rule(PolicyRule::new(
            "deny-one-tool",
            MatchPattern::ExactTarget { action_class: "tool_use".into(), target: name.into() },
            HookDecision::Deny { reason: format!("policy denies {name}") },
        ))
    }

    #[tokio::test]
    async fn denies_matching_tool() {
        let sub = PolicySpecSubscriber::new(deny_tool("bash"));
        let d = sub.on_event(&HookEvent::PreToolUse {
            task_id: "t".into(), tool_name: "bash".into(), args_json: "{}".into(),
        }).await;
        assert!(matches!(d, Some(HookDecision::Deny { .. })));
    }

    #[tokio::test]
    async fn allows_non_matching_tool() {
        let sub = PolicySpecSubscriber::new(deny_tool("bash"));
        let d = sub.on_event(&HookEvent::PreToolUse {
            task_id: "t".into(), tool_name: "read_file".into(), args_json: "{}".into(),
        }).await;
        assert!(matches!(d, Some(HookDecision::Allow)));
    }

    #[tokio::test]
    async fn empty_policy_allows() {
        let sub = PolicySpecSubscriber::new(PolicySpec::new());
        let d = sub.on_event(&HookEvent::MemoryWrite {
            task_id: "t".into(), topic: "x".into(), size_bytes: 1,
        }).await;
        assert!(matches!(d, Some(HookDecision::Allow)));
    }

    #[test]
    fn interest_covers_five_decision_kinds() {
        let sub = PolicySpecSubscriber::new(PolicySpec::new());
        assert_eq!(sub.interest_in().len(), 5);
    }
}
