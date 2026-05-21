//! `classify` — rule-based message → risk + autonomy + capabilities.

use std::collections::BTreeMap;

use crate::runtime::contracts::{AutonomyLevel, CapabilityQuery, RiskClass};

/// What the classifier emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub risk_class: RiskClass,
    pub autonomy_target: AutonomyLevel,
    pub requested_capabilities: Vec<CapabilityQuery>,
    /// Signals that fired, sorted by severity desc for diagnostic
    /// display.
    pub signals: Vec<RiskSignal>,
}

/// One rule that fired. Helps the UI show "request flagged as
/// Restricted because of 'rm -rf'".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskSignal {
    pub keyword: String,
    pub risk: RiskClass,
}

/// Tunable thresholds + keyword lists. Defaults are baked in;
/// override per-deployment via `with_*` builders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifierConfig {
    pub destructive: Vec<String>,
    pub sensitive: Vec<String>,
    pub external_write: Vec<String>,
    /// Tool keyword → CapabilityQuery template.
    pub tool_hints: BTreeMap<String, CapabilityQuery>,
}

impl ClassifierConfig {
    /// ADR §M3-T5 default keyword set.
    pub fn default_config() -> Self {
        let mut tool_hints = BTreeMap::new();
        for (kw, kind) in &[
            ("browse", "browser"),
            ("web search", "web"),
            ("search the web", "web"),
            ("check my email", "email"),
            ("send an email", "email"),
            ("read my calendar", "calendar"),
            ("schedule a meeting", "calendar"),
            ("send a slack", "slack"),
            ("open a file", "filesystem"),
            ("read a file", "filesystem"),
            ("create a spreadsheet", "spreadsheet"),
            ("query my notes", "memory"),
            ("recall what i said", "memory"),
        ] {
            tool_hints.insert(
                kw.to_string().to_lowercase(),
                CapabilityQuery {
                    kind: kind.to_string(),
                    name: None,
                    tags: BTreeMap::new(),
                },
            );
        }
        Self {
            destructive: vec![
                "delete".into(),
                "rm -rf".into(),
                "drop table".into(),
                "format disk".into(),
                "wipe".into(),
                "uninstall".into(),
                "force push".into(),
                "git reset --hard".into(),
            ],
            sensitive: vec![
                "password".into(),
                "secret".into(),
                "billing".into(),
                "credit card".into(),
                "api key".into(),
                "token".into(),
                "private key".into(),
            ],
            external_write: vec![
                "send".into(),
                "post".into(),
                "publish".into(),
                "deploy".into(),
                "merge".into(),
                "release".into(),
                "share".into(),
                "tweet".into(),
            ],
            tool_hints,
        }
    }
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Classify a user message.
///
/// Algorithm:
/// 1. Lowercase the message once.
/// 2. Scan destructive → sensitive → external_write keyword lists in
///    order. Each match contributes a `RiskSignal`. Highest risk
///    found wins for `risk_class`.
/// 3. `autonomy_target` is derived from `risk_class` (Restricted →
///    AssistedAction, High → SupervisedTask, Medium → SupervisedTask,
///    Low → DelegatedTask).
/// 4. Scan tool_hints for matching phrases → emit `CapabilityQuery`s
///    (deduplicated by `kind`).
pub fn classify(message: &str, config: &ClassifierConfig) -> Classification {
    let lower = message.to_ascii_lowercase();
    let mut signals: Vec<RiskSignal> = Vec::new();
    let mut risk = RiskClass::Low;

    for kw in &config.destructive {
        if lower.contains(kw) {
            signals.push(RiskSignal {
                keyword: kw.clone(),
                risk: RiskClass::Restricted,
            });
            risk = RiskClass::Restricted;
        }
    }
    for kw in &config.sensitive {
        if lower.contains(kw) {
            signals.push(RiskSignal {
                keyword: kw.clone(),
                risk: RiskClass::High,
            });
            if matches!(risk, RiskClass::Low | RiskClass::Medium) {
                risk = RiskClass::High;
            }
        }
    }
    for kw in &config.external_write {
        if lower.contains(kw) {
            signals.push(RiskSignal {
                keyword: kw.clone(),
                risk: RiskClass::Medium,
            });
            if matches!(risk, RiskClass::Low) {
                risk = RiskClass::Medium;
            }
        }
    }

    // Sort signals by severity desc for UI display.
    signals.sort_by_key(|s| -(rung(s.risk) as i64));

    // Capability extraction — dedup by kind, keep first match per kind.
    let mut seen_kinds: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut caps = Vec::new();
    for (kw, q) in &config.tool_hints {
        if lower.contains(kw) && seen_kinds.insert(q.kind.clone()) {
            caps.push(q.clone());
        }
    }

    let autonomy = match risk {
        RiskClass::Restricted => AutonomyLevel::AssistedAction,
        RiskClass::High => AutonomyLevel::SupervisedTask,
        RiskClass::Medium => AutonomyLevel::SupervisedTask,
        RiskClass::Low => AutonomyLevel::DelegatedTask,
    };

    Classification {
        risk_class: risk,
        autonomy_target: autonomy,
        requested_capabilities: caps,
        signals,
    }
}

fn rung(r: RiskClass) -> u8 {
    match r {
        RiskClass::Low => 0,
        RiskClass::Medium => 1,
        RiskClass::High => 2,
        RiskClass::Restricted => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_classify(msg: &str) -> Classification {
        classify(msg, &ClassifierConfig::default())
    }

    // ── benign Low ──────────────────────────────────────────────────

    #[test]
    fn pure_chat_classifies_as_low_delegated() {
        let c = default_classify("Hi, can you explain quantum entanglement?");
        assert_eq!(c.risk_class, RiskClass::Low);
        assert_eq!(c.autonomy_target, AutonomyLevel::DelegatedTask);
        assert!(c.signals.is_empty());
        assert!(c.requested_capabilities.is_empty());
    }

    // ── Medium (external-write) ─────────────────────────────────────

    #[test]
    fn external_send_classifies_as_medium_supervised() {
        let c = default_classify("Please send a message to the team.");
        assert_eq!(c.risk_class, RiskClass::Medium);
        assert_eq!(c.autonomy_target, AutonomyLevel::SupervisedTask);
        assert!(c.signals.iter().any(|s| s.keyword == "send"));
    }

    #[test]
    fn deploy_classifies_as_medium() {
        let c = default_classify("Deploy the staging build please");
        assert_eq!(c.risk_class, RiskClass::Medium);
    }

    // ── High (sensitive) ────────────────────────────────────────────

    #[test]
    fn password_classifies_as_high_supervised() {
        let c = default_classify("Show me my password for github");
        assert_eq!(c.risk_class, RiskClass::High);
        assert_eq!(c.autonomy_target, AutonomyLevel::SupervisedTask);
    }

    #[test]
    fn api_key_classifies_as_high() {
        let c = default_classify("Print my API key from .env");
        assert_eq!(c.risk_class, RiskClass::High);
    }

    // ── Restricted (destructive) ───────────────────────────────────

    #[test]
    fn rm_rf_classifies_as_restricted_assisted_action() {
        let c = default_classify("Run rm -rf on /tmp/junk");
        assert_eq!(c.risk_class, RiskClass::Restricted);
        assert_eq!(c.autonomy_target, AutonomyLevel::AssistedAction);
        assert!(c.signals.iter().any(|s| s.keyword == "rm -rf"));
    }

    #[test]
    fn delete_classifies_as_restricted() {
        let c = default_classify("Delete all .log files older than 30 days");
        assert_eq!(c.risk_class, RiskClass::Restricted);
    }

    #[test]
    fn force_push_classifies_as_restricted() {
        let c = default_classify("Do a force push to main");
        assert_eq!(c.risk_class, RiskClass::Restricted);
    }

    // ── highest-risk wins when multiple signals fire ───────────────

    #[test]
    fn destructive_overrides_external_write() {
        // "send" and "delete" both fire — destructive wins.
        let c = default_classify("Send a delete request to the server");
        assert_eq!(c.risk_class, RiskClass::Restricted);
        // Both signals recorded.
        let kws: Vec<&str> = c.signals.iter().map(|s| s.keyword.as_str()).collect();
        assert!(kws.contains(&"delete"));
        assert!(kws.contains(&"send"));
        // Highest risk first.
        assert_eq!(c.signals[0].risk, RiskClass::Restricted);
    }

    #[test]
    fn sensitive_overrides_external_write_but_not_destructive() {
        let c = default_classify("Send the password somewhere");
        // password (High) > send (Medium)
        assert_eq!(c.risk_class, RiskClass::High);
    }

    // ── case insensitivity ────────────────────────────────────────

    #[test]
    fn keywords_are_case_insensitive() {
        let c = default_classify("RM -RF the temp dir");
        assert_eq!(c.risk_class, RiskClass::Restricted);
    }

    // ── capability extraction ────────────────────────────────────

    #[test]
    fn web_search_emits_web_capability() {
        let c = default_classify("Can you search the web for rust async patterns?");
        assert!(c
            .requested_capabilities
            .iter()
            .any(|q| q.kind == "web"));
    }

    #[test]
    fn email_emits_email_capability() {
        let c = default_classify("Send an email to my boss");
        assert!(c
            .requested_capabilities
            .iter()
            .any(|q| q.kind == "email"));
    }

    #[test]
    fn capabilities_dedup_by_kind() {
        // "search the web" and "browse" would both emit web-related
        // capability — only one entry per kind.
        let c = default_classify("Search the web and browse the results");
        let web_count = c
            .requested_capabilities
            .iter()
            .filter(|q| q.kind == "web" || q.kind == "browser")
            .count();
        // 2 distinct kinds (web + browser), 2 capabilities.
        assert_eq!(web_count, 2);
        let just_web = c
            .requested_capabilities
            .iter()
            .filter(|q| q.kind == "web")
            .count();
        assert_eq!(just_web, 1);
    }

    // ── config override ────────────────────────────────────────────

    #[test]
    fn custom_config_changes_classification() {
        let mut cfg = ClassifierConfig::default();
        cfg.destructive.push("eat my socks".into());
        let c = classify("please eat my socks", &cfg);
        assert_eq!(c.risk_class, RiskClass::Restricted);
        assert!(c.signals.iter().any(|s| s.keyword == "eat my socks"));
    }

    // ── empty message ────────────────────────────────────────────

    #[test]
    fn empty_message_is_low() {
        let c = default_classify("");
        assert_eq!(c.risk_class, RiskClass::Low);
        assert!(c.signals.is_empty());
    }
}
