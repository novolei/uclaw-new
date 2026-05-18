//! `BrowserContextManager` — per-session Chrome lifecycle.
//!
//! Each agent session that uses browser tools gets its own Chrome subprocess
//! and profile directory under `~/.uclaw/browser-profiles/{session_id}/`.
//! Chrome is launched lazily on the first `get_or_create` call and kept alive
//! until `destroy` is called (typically on session close).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use crate::browser::context::BrowserContext;

pub struct BrowserContextManager {
    /// Live contexts keyed by session_id.
    contexts: Arc<RwLock<HashMap<String, Arc<BrowserContext>>>>,
    /// Base directory for per-session Chrome profiles.
    profile_base: PathBuf,
    /// Tauri app handle, used to emit screencast events.
    app_handle: tauri::AppHandle,
}

impl BrowserContextManager {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        let profile_base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".uclaw")
            .join("browser-profiles");
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
            profile_base,
            app_handle,
        }
    }

    /// Compute the profile directory path for a given session.
    pub fn profile_path_for(base: &Path, session_id: &str) -> PathBuf {
        base.join(session_id)
    }

    /// Get the existing context for `session_id`, or launch a new Chrome
    /// process and create one. Thread-safe; concurrent callers for the same
    /// session_id will only launch one Chrome.
    pub async fn get_or_create(&self, session_id: &str) -> Result<Arc<BrowserContext>> {
        // Fast path: context already exists.
        {
            let contexts = self.contexts.read().await;
            if let Some(ctx) = contexts.get(session_id) {
                return Ok(Arc::clone(ctx));
            }
        }

        // Slow path: launch a new Chrome.
        let profile_dir = Self::profile_path_for(&self.profile_base, session_id);
        let ctx = BrowserContext::launch(session_id, profile_dir).await?;
        let ctx = Arc::new(ctx);
        self.contexts.write().await.insert(session_id.to_string(), Arc::clone(&ctx));
        Ok(ctx)
    }

    /// Destroy the context for `session_id` (stops screencast, closes Chrome).
    /// No-op if not found.
    pub async fn destroy(&self, session_id: &str) {
        let ctx = {
            let mut contexts = self.contexts.write().await;
            contexts.remove(session_id)
        };
        if let Some(ctx) = ctx {
            // Stop all active screencasts.
            let tabs = ctx.get_all_tabs().await;
            for tab in tabs {
                ctx.stop_screencast(&tab.tab_id).await;
            }
            // chromiumoxide drops the Browser on Arc::drop → child process exits.
        }
    }

    /// List session IDs that currently have a live Chrome context.
    pub async fn list_active_sessions(&self) -> Vec<String> {
        self.contexts.read().await.keys().cloned().collect()
    }

    /// Expose the app handle to callers that need to start a screencast.
    pub fn app_handle(&self) -> &tauri::AppHandle {
        &self.app_handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn profile_path_per_session() {
        let base = PathBuf::from("/tmp/test-profiles");
        let path = BrowserContextManager::profile_path_for(&base, "session-abc");
        assert_eq!(path, PathBuf::from("/tmp/test-profiles/session-abc"));
    }
}
