//! Dual interactive queues — Pi convergence Sprint 2 item 3.
//!
//! `SteeringQueue`: user injections while the agent is working; the run loop
//! drains it at the start of each turn (mid-run steering).
//! `FollowUpQueue`: only when the agent stops naturally (Response) does the loop
//! pop OneAtATime and re-enter. Both are Arc<Mutex> + Clone (clone shares inner),
//! shared by producers (Tauri commands) and the consumer (loop).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::agent::types::ChatMessage;

/// One steering injection plus the frontend banner-card uuid it came from (if any).
#[derive(Clone)]
pub struct SteeringItem {
    pub uuid: Option<String>,
    pub message: ChatMessage,
}

/// One follow-up task (a batch of messages) plus its frontend banner-card uuid.
#[derive(Clone)]
pub struct FollowUpTask {
    pub uuid: Option<String>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Clone, Default)]
pub struct SteeringQueue {
    inner: Arc<Mutex<VecDeque<SteeringItem>>>,
}
impl SteeringQueue {
    pub fn push(&self, uuid: Option<String>, message: ChatMessage) {
        self.inner.lock().unwrap().push_back(SteeringItem { uuid, message });
    }
    pub fn drain(&self) -> Vec<SteeringItem> {
        self.inner.lock().unwrap().drain(..).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

#[derive(Clone, Default)]
pub struct FollowUpQueue {
    inner: Arc<Mutex<VecDeque<FollowUpTask>>>,
}
impl FollowUpQueue {
    pub fn push_task(&self, uuid: Option<String>, messages: Vec<ChatMessage>) {
        self.inner.lock().unwrap().push_back(FollowUpTask { uuid, messages });
    }
    pub fn next(&self) -> Option<FollowUpTask> {
        self.inner.lock().unwrap().pop_front()
    }
    /// Remove the pending follow-up task matching this banner-card uuid (used when
    /// the user steers an already-queued message — steering supersedes follow-up).
    pub fn remove_by_uuid(&self, uuid: &str) -> Option<FollowUpTask> {
        let mut q = self.inner.lock().unwrap();
        if let Some(pos) = q.iter().position(|t| t.uuid.as_deref() == Some(uuid)) {
            q.remove(pos)
        } else {
            None
        }
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::ChatMessage;

    #[test]
    fn steering_push_drain() {
        let q = SteeringQueue::default();
        assert!(q.is_empty());
        q.push(None, ChatMessage::user("a"));
        q.push(Some("u2".into()), ChatMessage::user("b"));
        let drained = q.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[1].uuid.as_deref(), Some("u2"));
        assert!(q.is_empty());
        assert!(q.drain().is_empty());
    }

    #[test]
    fn followup_one_at_a_time() {
        let q = FollowUpQueue::default();
        assert!(q.is_empty());
        q.push_task(Some("t1".into()), vec![ChatMessage::user("t1")]);
        q.push_task(Some("t2".into()), vec![ChatMessage::user("t2")]);
        let first = q.next().unwrap();
        assert_eq!(first.messages.len(), 1);
        assert_eq!(first.uuid.as_deref(), Some("t1"));
        assert!(!q.is_empty());
        let _second = q.next().unwrap();
        assert!(q.is_empty());
        assert!(q.next().is_none());
    }

    #[test]
    fn followup_remove_by_uuid() {
        let q = FollowUpQueue::default();
        q.push_task(Some("a".into()), vec![ChatMessage::user("a")]);
        q.push_task(Some("b".into()), vec![ChatMessage::user("b")]);
        let removed = q.remove_by_uuid("a").unwrap();
        assert_eq!(removed.uuid.as_deref(), Some("a"));
        assert!(q.remove_by_uuid("a").is_none()); // already gone
        let rest = q.next().unwrap();
        assert_eq!(rest.uuid.as_deref(), Some("b")); // only b remains
    }

    #[test]
    fn clone_shares_inner() {
        let q = SteeringQueue::default();
        let q2 = q.clone();
        q.push(None, ChatMessage::user("x"));
        assert!(!q2.is_empty());
    }
}
