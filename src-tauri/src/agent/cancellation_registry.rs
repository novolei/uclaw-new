//! Per-conversation cancellation registry. Holds `CancellationToken` keyed by
//! `conversation_id` so the UI can fire `cancel_conversation(id)` from a
//! "stop" button and have it propagate through `ReasoningContext` →
//! `stream_completion` + `ToolDispatcher::dispatch`'s biased `select!`.
//!
//! Each `send_message` / `send_agent_message` call:
//!   1. Creates a fresh `CancellationToken`
//!   2. Stores it under `conversation_id` (replacing any prior token — old
//!      requests for the same conversation are abandoned)
//!   3. Installs it on `ReasoningContext` via `with_cancellation`
//!   4. On completion (success/error/timeout), removes it from the registry
//!
//! The lock is held only briefly during lookup/insert/remove, so a std Mutex
//! is appropriate here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub struct CancellationRegistry {
    inner: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl CancellationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fresh token for the conversation. If a prior token exists
    /// for the same conversation, it is CANCELLED first (the prior in-flight
    /// request is abandoned in favor of the new one) and then replaced.
    pub fn register(&self, conversation_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        let mut guard = self.inner.lock().expect("cancellation registry poisoned");
        if let Some(prior) = guard.insert(conversation_id.to_string(), token.clone()) {
            tracing::info!(
                conversation_id,
                "[cancel] superseding prior in-flight token for conversation"
            );
            prior.cancel();
        }
        token
    }

    /// Remove the token for this conversation (typically on completion).
    /// Does NOT fire the token — caller does that if needed.
    pub fn unregister(&self, conversation_id: &str) {
        let mut guard = self.inner.lock().expect("cancellation registry poisoned");
        guard.remove(conversation_id);
    }

    /// Fire the token for this conversation. Returns `true` if a token was
    /// found and fired, `false` if no in-flight request for that conversation.
    pub fn cancel(&self, conversation_id: &str) -> bool {
        let guard = self.inner.lock().expect("cancellation registry poisoned");
        match guard.get(conversation_id) {
            Some(token) => {
                tracing::info!(conversation_id, "[cancel] firing cancellation token");
                token.cancel();
                true
            }
            None => {
                tracing::debug!(conversation_id, "[cancel] no in-flight token to cancel");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_registry_cancel_fires_token() {
        let reg = CancellationRegistry::new();
        let token = reg.register("conv-1");
        assert!(!token.is_cancelled());
        assert!(reg.cancel("conv-1"));
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancellation_registry_supersede_fires_prior_token() {
        let reg = CancellationRegistry::new();
        let t1 = reg.register("conv-1");
        let t2 = reg.register("conv-1"); // supersedes t1
        assert!(t1.is_cancelled(), "prior token should be cancelled on supersede");
        assert!(!t2.is_cancelled());
    }

    #[tokio::test]
    async fn cancellation_registry_unregister_no_cancel() {
        let reg = CancellationRegistry::new();
        let token = reg.register("conv-1");
        reg.unregister("conv-1");
        assert!(!token.is_cancelled(), "unregister should NOT fire the token");
        assert!(!reg.cancel("conv-1"), "no token registered → returns false");
    }
}
