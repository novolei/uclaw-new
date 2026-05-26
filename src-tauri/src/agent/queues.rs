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

#[derive(Clone, Default)]
pub struct SteeringQueue {
    inner: Arc<Mutex<VecDeque<ChatMessage>>>,
}
impl SteeringQueue {
    pub fn push(&self, msg: ChatMessage) {
        self.inner.lock().unwrap().push_back(msg);
    }
    pub fn drain(&self) -> Vec<ChatMessage> {
        self.inner.lock().unwrap().drain(..).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

#[derive(Clone, Default)]
pub struct FollowUpQueue {
    inner: Arc<Mutex<VecDeque<Vec<ChatMessage>>>>,
}
impl FollowUpQueue {
    pub fn push_task(&self, messages: Vec<ChatMessage>) {
        self.inner.lock().unwrap().push_back(messages);
    }
    pub fn next(&self) -> Option<Vec<ChatMessage>> {
        self.inner.lock().unwrap().pop_front()
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
        q.push(ChatMessage::user("a"));
        q.push(ChatMessage::user("b"));
        let drained = q.drain();
        assert_eq!(drained.len(), 2);
        assert!(q.is_empty());           // drain takes all
        assert!(q.drain().is_empty());   // drain again -> empty
    }

    #[test]
    fn followup_one_at_a_time() {
        let q = FollowUpQueue::default();
        assert!(q.is_empty());
        q.push_task(vec![ChatMessage::user("t1")]);
        q.push_task(vec![ChatMessage::user("t2")]);
        let first = q.next().unwrap();    // pop one task
        assert_eq!(first.len(), 1);
        assert!(!q.is_empty());           // one remains
        let second = q.next().unwrap();
        assert!(q.is_empty());
        assert!(q.next().is_none());      // empty -> None
        let _ = second;
    }

    #[test]
    fn clone_shares_inner() {
        let q = SteeringQueue::default();
        let q2 = q.clone();
        q.push(ChatMessage::user("x"));
        assert!(!q2.is_empty());          // clone shares inner Arc (producer/consumer share)
    }
}
