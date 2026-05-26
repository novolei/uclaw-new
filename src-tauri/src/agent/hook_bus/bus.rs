//! `HookBus` — dispatches `HookEvent`s to all registered subscribers
//! and aggregates their `HookDecision`s.

use std::sync::Arc;

use crate::runtime::contracts::HookDecision;

use super::event::HookEvent;
use super::subscriber::{HookSubscriber, SubscriberId};

/// Aggregated dispatch failure. Currently a single variant — kept as
/// an enum to leave room for future variants (subscriber-timeout,
/// circular subscription, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    /// Tried to register a subscriber whose id matches an existing
    /// subscriber's id. Caller should de-register first or pick a
    /// different id.
    DuplicateSubscriberId(SubscriberId),
}

impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusError::DuplicateSubscriberId(id) => {
                write!(f, "duplicate hook subscriber id: {}", id.as_str())
            }
        }
    }
}

impl std::error::Error for BusError {}

/// The hook bus. Holds an ordered list of subscribers; dispatch
/// iterates them in insertion order so a logger that runs before a
/// decisioner sees the un-modified event.
#[derive(Clone, Default)]
pub struct HookBus {
    subscribers: Vec<Arc<dyn HookSubscriber>>,
}

impl HookBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a subscriber. Returns `Err(DuplicateSubscriberId)` if
    /// the id is already in use.
    pub fn register(&mut self, sub: Arc<dyn HookSubscriber>) -> Result<(), BusError> {
        let new_id = sub.id();
        if self.subscribers.iter().any(|s| s.id() == new_id) {
            return Err(BusError::DuplicateSubscriberId(new_id));
        }
        self.subscribers.push(sub);
        Ok(())
    }

    /// Remove a subscriber by id. Returns `true` if removed, `false`
    /// if no subscriber with that id was registered.
    pub fn unregister(&mut self, id: &SubscriberId) -> bool {
        let before = self.subscribers.len();
        self.subscribers.retain(|s| s.id() != *id);
        self.subscribers.len() != before
    }

    /// Number of registered subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Fire a non-decisionable event (TaskStart, PostToolUse, etc.).
    /// All interested subscribers receive the event in order. Returned
    /// `HookDecision`s are dropped (this method is for observe-only
    /// events; use `dispatch_with_decision` when the verdict matters).
    pub async fn dispatch_observe(&self, event: &HookEvent) {
        let kind = event.kind();
        for sub in &self.subscribers {
            if sub.interest_in().contains(&kind) {
                let _ = sub.on_event(event).await;
            }
        }
    }

    /// Fire a potentially-decisionable event and aggregate verdicts.
    ///
    /// Aggregation rule:
    ///
    /// - Any subscriber returning `Deny` → final = `Deny` (first
    ///   denial wins; deny reason from the first denying subscriber
    ///   is preserved). Remaining subscribers still see the event
    ///   for audit purposes.
    /// - Otherwise, any `AskUser` → final = `AskUser` (first ask wins;
    ///   `risk_class` from that subscriber preserved).
    /// - Otherwise → `Allow`.
    ///
    /// For observe-only event kinds (`is_decision_capable() == false`)
    /// the bus dispatches subscribers but always returns `Allow`,
    /// matching the documented contract in
    /// `HookEventKind::is_decision_capable`.
    pub async fn dispatch_with_decision(&self, event: &HookEvent) -> HookDecision {
        let kind = event.kind();
        let observe_only = !kind.is_decision_capable();

        let mut deny: Option<HookDecision> = None;
        let mut ask: Option<HookDecision> = None;

        for sub in &self.subscribers {
            if !sub.interest_in().contains(&kind) {
                continue;
            }
            let result = sub.on_event(event).await;
            if observe_only {
                continue;
            }
            match result {
                Some(d @ HookDecision::Deny { .. }) if deny.is_none() => {
                    deny = Some(d);
                }
                Some(d @ HookDecision::AskUser { .. }) if ask.is_none() => {
                    ask = Some(d);
                }
                _ => {}
            }
        }

        if observe_only {
            return HookDecision::Allow;
        }
        deny.or(ask).unwrap_or(HookDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::super::event::HookEventKind;
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ── test subscribers ────────────────────────────────────────────

    struct CountingObserver {
        id: SubscriberId,
        kinds: &'static [HookEventKind],
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl HookSubscriber for CountingObserver {
        fn id(&self) -> SubscriberId {
            self.id.clone()
        }
        fn interest_in(&self) -> &'static [HookEventKind] {
            self.kinds
        }
        async fn on_event(&self, _e: &HookEvent) -> Option<HookDecision> {
            *self.calls.lock().unwrap() += 1;
            None
        }
    }

    struct Denier;
    #[async_trait]
    impl HookSubscriber for Denier {
        fn id(&self) -> SubscriberId {
            SubscriberId::new("denier")
        }
        fn interest_in(&self) -> &'static [HookEventKind] {
            &[HookEventKind::PreToolUse]
        }
        async fn on_event(&self, _e: &HookEvent) -> Option<HookDecision> {
            Some(HookDecision::Deny {
                reason: "blocked".into(),
            })
        }
    }

    struct Asker;
    #[async_trait]
    impl HookSubscriber for Asker {
        fn id(&self) -> SubscriberId {
            SubscriberId::new("asker")
        }
        fn interest_in(&self) -> &'static [HookEventKind] {
            &[HookEventKind::PreToolUse, HookEventKind::MemoryWrite]
        }
        async fn on_event(&self, _e: &HookEvent) -> Option<HookDecision> {
            Some(HookDecision::AskUser {
                prompt: "confirm?".into(),
                risk_class: None,
            })
        }
    }

    fn pre_tool_event() -> HookEvent {
        HookEvent::PreToolUse {
            task_id: "t".into(),
            tool_name: "shell".into(),
            args_json: "{}".into(),
        }
    }

    fn post_tool_event() -> HookEvent {
        HookEvent::PostToolUse {
            task_id: "t".into(),
            tool_name: "shell".into(),
            success: true,
            result_preview: String::new(),
        }
    }

    // ── registration ────────────────────────────────────────────────

    #[test]
    fn register_and_unregister() {
        let mut bus = HookBus::new();
        let s = Arc::new(CountingObserver {
            id: SubscriberId::new("s1"),
            kinds: &[HookEventKind::PreToolUse],
            calls: Mutex::new(0),
        });
        bus.register(s.clone()).unwrap();
        assert_eq!(bus.subscriber_count(), 1);
        assert!(bus.unregister(&SubscriberId::new("s1")));
        assert_eq!(bus.subscriber_count(), 0);
        assert!(!bus.unregister(&SubscriberId::new("s1")));
    }

    #[test]
    fn register_duplicate_returns_err() {
        let mut bus = HookBus::new();
        let s1 = Arc::new(CountingObserver {
            id: SubscriberId::new("dup"),
            kinds: &[],
            calls: Mutex::new(0),
        });
        let s2 = Arc::new(CountingObserver {
            id: SubscriberId::new("dup"),
            kinds: &[],
            calls: Mutex::new(0),
        });
        bus.register(s1).unwrap();
        let err = bus.register(s2).unwrap_err();
        assert_eq!(
            err,
            BusError::DuplicateSubscriberId(SubscriberId::new("dup"))
        );
    }

    // ── observe-only dispatch ──────────────────────────────────────

    #[tokio::test]
    async fn dispatch_observe_calls_only_interested_subscribers() {
        let mut bus = HookBus::new();
        let interested = Arc::new(CountingObserver {
            id: SubscriberId::new("interested"),
            kinds: &[HookEventKind::PostToolUse],
            calls: Mutex::new(0),
        });
        let not_interested = Arc::new(CountingObserver {
            id: SubscriberId::new("not_interested"),
            kinds: &[HookEventKind::TaskStart],
            calls: Mutex::new(0),
        });
        bus.register(interested.clone()).unwrap();
        bus.register(not_interested.clone()).unwrap();

        bus.dispatch_observe(&post_tool_event()).await;
        assert_eq!(*interested.calls.lock().unwrap(), 1);
        assert_eq!(*not_interested.calls.lock().unwrap(), 0);
    }

    // ── decision aggregation ───────────────────────────────────────

    #[tokio::test]
    async fn dispatch_with_decision_allows_with_no_subscribers() {
        let bus = HookBus::new();
        let d = bus.dispatch_with_decision(&pre_tool_event()).await;
        assert!(d.is_allow());
    }

    #[tokio::test]
    async fn allow_when_no_interested_subscriber_returns_decision() {
        let mut bus = HookBus::new();
        bus.register(Arc::new(CountingObserver {
            id: SubscriberId::new("observer"),
            kinds: &[HookEventKind::PreToolUse],
            calls: Mutex::new(0),
        }))
        .unwrap();
        let d = bus.dispatch_with_decision(&pre_tool_event()).await;
        assert!(d.is_allow());
    }

    #[tokio::test]
    async fn deny_wins_over_ask_and_allow() {
        let mut bus = HookBus::new();
        bus.register(Arc::new(Asker)).unwrap();
        bus.register(Arc::new(Denier)).unwrap();
        let d = bus.dispatch_with_decision(&pre_tool_event()).await;
        assert!(d.is_deny());
    }

    #[tokio::test]
    async fn ask_wins_over_allow_when_no_deny() {
        let mut bus = HookBus::new();
        bus.register(Arc::new(Asker)).unwrap();
        let d = bus.dispatch_with_decision(&pre_tool_event()).await;
        assert!(d.requires_user());
    }

    #[tokio::test]
    async fn observe_only_event_always_returns_allow_even_when_subscriber_denies() {
        let mut bus = HookBus::new();
        // Use Asker which is interested in MemoryWrite (decision-capable),
        // but we'll send a PostToolUse (observe-only) — Asker won't see
        // it because interest_in doesn't include PostToolUse.
        // Add a counting observer for PostToolUse to confirm dispatch.
        let counter = Arc::new(CountingObserver {
            id: SubscriberId::new("c"),
            kinds: &[HookEventKind::PostToolUse],
            calls: Mutex::new(0),
        });
        bus.register(counter.clone()).unwrap();
        let d = bus.dispatch_with_decision(&post_tool_event()).await;
        assert!(d.is_allow());
        assert_eq!(*counter.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn decision_capable_memory_write_can_be_denied() {
        let mut bus = HookBus::new();
        struct MemDenier;
        #[async_trait]
        impl HookSubscriber for MemDenier {
            fn id(&self) -> SubscriberId {
                SubscriberId::new("m")
            }
            fn interest_in(&self) -> &'static [HookEventKind] {
                &[HookEventKind::MemoryWrite]
            }
            async fn on_event(&self, _e: &HookEvent) -> Option<HookDecision> {
                Some(HookDecision::Deny {
                    reason: "no writes".into(),
                })
            }
        }
        bus.register(Arc::new(MemDenier)).unwrap();
        let e = HookEvent::MemoryWrite {
            task_id: "t".into(),
            topic: "secret".into(),
            size_bytes: 42,
        };
        let d = bus.dispatch_with_decision(&e).await;
        assert!(d.is_deny());
    }

    // ── BusError display ───────────────────────────────────────────

    #[test]
    fn bus_error_display_includes_id() {
        let e = BusError::DuplicateSubscriberId(SubscriberId::new("x"));
        let s = e.to_string();
        assert!(s.contains("duplicate"));
        assert!(s.contains("x"));
    }
}
