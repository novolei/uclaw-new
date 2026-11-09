// SPDX-License-Identifier: Apache-2.0
//! Per-session agent turn serialization (context continuity P1).
//!
//! `send_agent_message` acquires the session's lock before reading the
//! transcript and holds the owned guard inside the spawned turn until the
//! loop's (or the caller-side) transcript finalize and turn-close are done,
//! so a stop → immediate re-send can't build history from — or append to —
//! a transcript the previous turn is still writing. Stop/cancel never takes
//! this lock (they fire the cancellation token), so they can't deadlock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Registry of per-session turn mutexes. Cheap to clone (shared map).
#[derive(Clone, Default)]
pub struct SessionTurnLocks {
    inner: Arc<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

impl SessionTurnLocks {
    /// The session's mutex (created on first use).
    pub fn mutex_for(&self, session_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(map.entry(session_id.to_string()).or_default())
    }

    /// Wait for the session's previous turn to finish; the owned guard can
    /// move into the spawned turn task (dropped on every exit, incl. panic).
    pub async fn acquire(&self, session_id: &str) -> OwnedMutexGuard<()> {
        self.mutex_for(session_id).lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn same_session_shares_one_mutex() {
        let locks = SessionTurnLocks::default();
        assert!(Arc::ptr_eq(&locks.mutex_for("s1"), &locks.mutex_for("s1")));
        assert!(!Arc::ptr_eq(&locks.mutex_for("s1"), &locks.mutex_for("s2")));
    }

    #[tokio::test]
    async fn second_acquire_waits_until_first_guard_drops() {
        let locks = SessionTurnLocks::default();
        let first = locks.acquire("s1").await;
        // Another session is independent.
        let other = tokio::time::timeout(Duration::from_millis(50), locks.acquire("s2")).await;
        assert!(other.is_ok());
        // Same session blocks while `first` is held…
        let blocked = tokio::time::timeout(Duration::from_millis(50), locks.acquire("s1")).await;
        assert!(blocked.is_err(), "second turn must wait for the first");
        // …and proceeds once it is dropped (e.g. moved into a task that ends).
        let waiter = {
            let locks = locks.clone();
            tokio::spawn(async move { locks.acquire("s1").await; })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished());
        drop(first);
        tokio::time::timeout(Duration::from_secs(1), waiter).await.unwrap().unwrap();
    }
}
