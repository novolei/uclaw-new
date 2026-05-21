//! `ActionRequest`, `PolicyRule`, `PolicySpec`, and the `evaluate`
//! function.

use serde::{Deserialize, Serialize};

use crate::runtime::contracts::{HookDecision, RiskClass};

/// One guarded action a task wants to perform. Carries enough to
/// decide whether it's allowed.
///
/// `action_class` is the coarse category (`"tool_use"`,
/// `"network"`, `"file_write"`, `"memory_write"`, ...); `target`
/// is the specific item being acted on. `risk_class` is the
/// caller's pre-assessment — the policy may override or refine it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub action_class: String,
    pub target: String,
    pub risk_class: RiskClass,
}

impl ActionRequest {
    pub fn new(
        action_class: impl Into<String>,
        target: impl Into<String>,
        risk_class: RiskClass,
    ) -> Self {
        Self {
            action_class: action_class.into(),
            target: target.into(),
            risk_class,
        }
    }
}

/// How a rule matches against an `ActionRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MatchPattern {
    /// Match when `action_class` equals the given string. Target is
    /// ignored.
    AnyTarget { action_class: String },
    /// Match when both `action_class` and `target` equal exactly.
    ExactTarget {
        action_class: String,
        target: String,
    },
    /// Match when `action_class` equals AND `target` starts with the
    /// given prefix. Common for path-scoped rules
    /// (e.g. `target_prefix: "/etc/"`).
    TargetPrefix {
        action_class: String,
        target_prefix: String,
    },
    /// Match any request whose risk class is at least the given
    /// threshold (`risk_class.is_at_least(threshold)`). Useful for
    /// catch-all "ask user for anything HIGH or above" rules.
    AtLeastRisk { risk: RiskClass },
}

impl MatchPattern {
    pub fn matches(&self, request: &ActionRequest) -> bool {
        match self {
            MatchPattern::AnyTarget { action_class } => &request.action_class == action_class,
            MatchPattern::ExactTarget {
                action_class,
                target,
            } => &request.action_class == action_class && &request.target == target,
            MatchPattern::TargetPrefix {
                action_class,
                target_prefix,
            } => {
                &request.action_class == action_class
                    && request.target.starts_with(target_prefix)
            }
            MatchPattern::AtLeastRisk { risk } => risk_at_least(request.risk_class, *risk),
        }
    }
}

fn risk_at_least(actual: RiskClass, threshold: RiskClass) -> bool {
    risk_rung(actual) >= risk_rung(threshold)
}

fn risk_rung(r: RiskClass) -> u8 {
    match r {
        RiskClass::Low => 0,
        RiskClass::Medium => 1,
        RiskClass::High => 2,
        RiskClass::Restricted => 3,
    }
}

/// One rule with a matcher + an outcome template. The outcome is a
/// `HookDecision` returned (cloned) when the rule matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    /// Stable rule id for diagnostic logging (which rule matched?).
    pub id: String,
    pub pattern: MatchPattern,
    pub outcome: HookDecision,
}

impl PolicyRule {
    pub fn new(id: impl Into<String>, pattern: MatchPattern, outcome: HookDecision) -> Self {
        Self {
            id: id.into(),
            pattern,
            outcome,
        }
    }
}

/// Ordered list of rules. First matching rule wins; if none match,
/// the policy returns `Allow`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySpec {
    pub rules: Vec<PolicyRule>,
}

impl PolicySpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style: append a rule, return Self.
    pub fn with_rule(mut self, rule: PolicyRule) -> Self {
        self.rules.push(rule);
        self
    }
}

/// Evaluate `request` against `spec`. Walks rules in declaration
/// order; first match's outcome wins. Falls through to
/// `HookDecision::Allow` when no rule matches.
///
/// Returns both the decision and the matching rule id (if any) so
/// callers can log which rule fired.
pub fn evaluate<'a>(
    spec: &'a PolicySpec,
    request: &ActionRequest,
) -> (HookDecision, Option<&'a str>) {
    for rule in &spec.rules {
        if rule.pattern.matches(request) {
            return (rule.outcome.clone(), Some(rule.id.as_str()));
        }
    }
    (HookDecision::Allow, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(reason: &str) -> HookDecision {
        HookDecision::Deny {
            reason: reason.into(),
        }
    }

    fn ask(prompt: &str) -> HookDecision {
        HookDecision::AskUser {
            prompt: prompt.into(),
            risk_class: None,
        }
    }

    fn req(class: &str, target: &str, risk: RiskClass) -> ActionRequest {
        ActionRequest::new(class, target, risk)
    }

    // ── MatchPattern ───────────────────────────────────────────────

    #[test]
    fn any_target_matches_by_class() {
        let p = MatchPattern::AnyTarget {
            action_class: "tool_use".into(),
        };
        assert!(p.matches(&req("tool_use", "shell", RiskClass::Low)));
        assert!(!p.matches(&req("network", "shell", RiskClass::Low)));
    }

    #[test]
    fn exact_target_requires_both_fields() {
        let p = MatchPattern::ExactTarget {
            action_class: "file_write".into(),
            target: "/etc/passwd".into(),
        };
        assert!(p.matches(&req("file_write", "/etc/passwd", RiskClass::High)));
        assert!(!p.matches(&req("file_write", "/tmp/x", RiskClass::High)));
        assert!(!p.matches(&req("file_read", "/etc/passwd", RiskClass::Low)));
    }

    #[test]
    fn target_prefix_matches_path_subtree() {
        let p = MatchPattern::TargetPrefix {
            action_class: "file_write".into(),
            target_prefix: "/etc/".into(),
        };
        assert!(p.matches(&req("file_write", "/etc/passwd", RiskClass::High)));
        assert!(p.matches(&req("file_write", "/etc/", RiskClass::High)));
        assert!(!p.matches(&req("file_write", "/tmp/etc/x", RiskClass::High)));
    }

    #[test]
    fn at_least_risk_matches_threshold_and_above() {
        let p = MatchPattern::AtLeastRisk {
            risk: RiskClass::High,
        };
        assert!(p.matches(&req("any", "any", RiskClass::High)));
        assert!(p.matches(&req("any", "any", RiskClass::Restricted)));
        assert!(!p.matches(&req("any", "any", RiskClass::Medium)));
        assert!(!p.matches(&req("any", "any", RiskClass::Low)));
    }

    // ── evaluate: order + first-match-wins ─────────────────────────

    #[test]
    fn no_rules_returns_allow() {
        let spec = PolicySpec::new();
        let (d, rule_id) = evaluate(&spec, &req("any", "any", RiskClass::Low));
        assert!(d.is_allow());
        assert!(rule_id.is_none());
    }

    #[test]
    fn first_matching_rule_wins_even_when_later_one_also_matches() {
        let spec = PolicySpec::new()
            .with_rule(PolicyRule::new(
                "first-ask",
                MatchPattern::AnyTarget {
                    action_class: "tool_use".into(),
                },
                ask("first wins"),
            ))
            .with_rule(PolicyRule::new(
                "second-deny",
                MatchPattern::AnyTarget {
                    action_class: "tool_use".into(),
                },
                deny("would never be reached"),
            ));
        let (d, rule_id) = evaluate(&spec, &req("tool_use", "shell", RiskClass::Low));
        assert!(d.requires_user());
        assert_eq!(rule_id, Some("first-ask"));
    }

    #[test]
    fn no_match_falls_through_to_allow() {
        let spec = PolicySpec::new().with_rule(PolicyRule::new(
            "deny-shell",
            MatchPattern::ExactTarget {
                action_class: "tool_use".into(),
                target: "shell".into(),
            },
            deny("shell banned"),
        ));
        let (d, rule_id) = evaluate(&spec, &req("tool_use", "read_file", RiskClass::Low));
        assert!(d.is_allow());
        assert!(rule_id.is_none());
    }

    // ── outcome cloning ────────────────────────────────────────────

    #[test]
    fn outcome_cloned_on_match_so_spec_can_be_reused() {
        let spec = PolicySpec::new().with_rule(PolicyRule::new(
            "block",
            MatchPattern::AnyTarget {
                action_class: "x".into(),
            },
            deny("nope"),
        ));
        let (d1, _) = evaluate(&spec, &req("x", "a", RiskClass::Low));
        let (d2, _) = evaluate(&spec, &req("x", "b", RiskClass::Low));
        assert!(d1.is_deny());
        assert!(d2.is_deny());
        // Spec still has 1 rule (we didn't move).
        assert_eq!(spec.rules.len(), 1);
    }

    // ── realistic policies ─────────────────────────────────────────

    #[test]
    fn realistic_filesystem_policy() {
        // Deny writes to /etc, ask for /home, allow elsewhere.
        let spec = PolicySpec::new()
            .with_rule(PolicyRule::new(
                "etc-deny",
                MatchPattern::TargetPrefix {
                    action_class: "file_write".into(),
                    target_prefix: "/etc/".into(),
                },
                deny("system config"),
            ))
            .with_rule(PolicyRule::new(
                "home-ask",
                MatchPattern::TargetPrefix {
                    action_class: "file_write".into(),
                    target_prefix: "/home/".into(),
                },
                ask("write to home dir?"),
            ));
        // /etc → deny
        let (d, _) = evaluate(&spec, &req("file_write", "/etc/passwd", RiskClass::High));
        assert!(d.is_deny());
        // /home → ask
        let (d, _) = evaluate(&spec, &req("file_write", "/home/me/x.txt", RiskClass::Low));
        assert!(d.requires_user());
        // /tmp → allow (no rule)
        let (d, _) = evaluate(&spec, &req("file_write", "/tmp/x", RiskClass::Low));
        assert!(d.is_allow());
    }

    #[test]
    fn realistic_risk_catchall_after_specific_rules() {
        let spec = PolicySpec::new()
            .with_rule(PolicyRule::new(
                "allow-low-shell",
                MatchPattern::ExactTarget {
                    action_class: "tool_use".into(),
                    target: "shell".into(),
                },
                HookDecision::Allow,
            ))
            .with_rule(PolicyRule::new(
                "ask-high-actions",
                MatchPattern::AtLeastRisk {
                    risk: RiskClass::High,
                },
                ask("high-risk — confirm?"),
            ));
        // shell + low → allow via first rule
        let (d, id) = evaluate(&spec, &req("tool_use", "shell", RiskClass::Low));
        assert!(d.is_allow());
        assert_eq!(id, Some("allow-low-shell"));
        // shell + high — first rule still matches (ExactTarget) → allow.
        let (d, _) = evaluate(&spec, &req("tool_use", "shell", RiskClass::High));
        assert!(d.is_allow());
        // OTHER tool + high — falls through to catchall → ask.
        let (d, id) = evaluate(&spec, &req("tool_use", "rm_rf", RiskClass::High));
        assert!(d.requires_user());
        assert_eq!(id, Some("ask-high-actions"));
    }

    // ── serde ──────────────────────────────────────────────────────

    #[test]
    fn match_pattern_serde_tag_snake_case() {
        let p = MatchPattern::TargetPrefix {
            action_class: "file_write".into(),
            target_prefix: "/etc/".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        // Enum-level rename_all uses snake_case for the discriminator
        // tag. Variant struct fields keep their Rust snake_case names
        // (enum rename_all doesn't propagate to fields).
        assert_eq!(v["kind"], "target_prefix");
        assert_eq!(v["action_class"], "file_write");
        assert_eq!(v["target_prefix"], "/etc/");
    }

    #[test]
    fn policy_spec_serde_roundtrip() {
        let spec = PolicySpec::new().with_rule(PolicyRule::new(
            "x",
            MatchPattern::AtLeastRisk {
                risk: RiskClass::Restricted,
            },
            HookDecision::Deny {
                reason: "restricted".into(),
            },
        ));
        let json = serde_json::to_string(&spec).unwrap();
        let back: PolicySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }
}
