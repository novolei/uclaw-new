//! `HookSubscriber` trait — implemented by anything wanting to react
//! to hook events. SafetyManager, AuditLogger, RolloutWriter, and
//! user-installed plugin hooks all implement this.

use async_trait::async_trait;

use crate::runtime::contracts::HookDecision;

use super::event::{HookEvent, HookEventKind};

/// Stable id for a subscriber. Lets the bus de-register one without
/// touching the others.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub String);

impl SubscriberId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SubscriberId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

/// Async hook callback contract.
///
/// `on_event` is called once per hook event the subscriber registered
/// interest in. The subscriber returns:
///
/// - `None` → no opinion (default Allow)
/// - `Some(HookDecision)` → contributes to the aggregated verdict
///
/// For observe-only event kinds (`HookEventKind::is_decision_capable()
/// == false`) the bus ignores returned decisions and forces Allow.
#[async_trait]
pub trait HookSubscriber: Send + Sync {
    /// Stable identifier — used by the bus for dedup + diagnostic logs.
    fn id(&self) -> SubscriberId;

    /// Which event kinds this subscriber wants to receive. Bus calls
    /// `on_event` only for these kinds. Empty slice = no events.
    fn interest_in(&self) -> &'static [HookEventKind];

    /// Callback. Implementations should be fast — the bus dispatches
    /// in series and blocks the agent loop until all subscribers
    /// return.
    async fn on_event(&self, event: &HookEvent) -> Option<HookDecision>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriber_id_constructors() {
        let a = SubscriberId::new("safety");
        let b: SubscriberId = "audit".into();
        assert_eq!(a.as_str(), "safety");
        assert_eq!(b.as_str(), "audit");
        assert_ne!(a, b);
    }

    #[test]
    fn subscriber_id_is_hashable() {
        use std::collections::HashSet;
        let mut s = HashSet::new();
        s.insert(SubscriberId::new("a"));
        s.insert(SubscriberId::new("b"));
        assert!(s.contains(&SubscriberId::new("a")));
        assert_eq!(s.len(), 2);
    }
}
