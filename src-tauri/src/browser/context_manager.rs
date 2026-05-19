//! `BrowserContextManager` — per-session Chrome lifecycle.
//!
//! Each agent session that uses browser tools gets its own Chrome subprocess
//! and profile directory under `~/.uclaw/browser-profiles/{session_id}/`.
//! Chrome is launched lazily on the first `get_or_create` call and kept alive
//! until `destroy` is called (typically on session close).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
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

    /// Compute the profile directory path for a session plus optional auth
    /// profile. The identity suffix keeps durable browser state isolated while
    /// still making the relationship to the agent session visible on disk.
    pub fn profile_path_for_identity(
        base: &Path,
        session_id: &str,
        identity_profile_id: Option<&str>,
    ) -> Result<PathBuf> {
        match identity_profile_id {
            Some(profile_id) => Ok(base.join(format!(
                "{}--identity-{}",
                sanitize_profile_segment(session_id)?,
                sanitize_profile_segment(profile_id)?
            ))),
            None => Ok(Self::profile_path_for(base, session_id)),
        }
    }

    /// Get the existing context for `session_id`, or launch a new Chrome
    /// process and create one. Thread-safe; concurrent callers for the same
    /// session_id will only launch one Chrome.
    pub async fn get_or_create(&self, session_id: &str) -> Result<Arc<BrowserContext>> {
        self.get_or_create_with_identity(session_id, None).await
    }

    /// Get or launch a browser context for a session with an optional auth
    /// profile. This is the browser-side entry point for the auth profile
    /// broker; the broker resolves/imports identity material, while the
    /// context manager guarantees profile-directory isolation.
    pub async fn get_or_create_with_identity(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
    ) -> Result<Arc<BrowserContext>> {
        let context_key = context_key_for(session_id, identity_profile_id)?;

        // Fast path: context already exists.
        {
            let contexts = self.contexts.read().await;
            if let Some(ctx) = contexts.get(&context_key) {
                return Ok(Arc::clone(ctx));
            }
        }

        // Slow path: launch a new Chrome.
        let profile_dir =
            Self::profile_path_for_identity(&self.profile_base, session_id, identity_profile_id)?;
        let ctx = BrowserContext::launch(session_id, profile_dir).await?;
        let ctx = Arc::new(ctx);
        self.contexts
            .write()
            .await
            .insert(context_key, Arc::clone(&ctx));
        Ok(ctx)
    }

    /// Destroy the context for `session_id` (stops screencast, closes Chrome).
    /// No-op if not found.
    pub async fn destroy(&self, session_id: &str) {
        let removed_contexts = {
            let mut contexts = self.contexts.write().await;
            let keys: Vec<String> = contexts
                .keys()
                .filter(|key| key_for_session(key, session_id))
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| contexts.remove(&key))
                .collect::<Vec<_>>()
        };
        for ctx in removed_contexts {
            // Stop all active screencasts.
            let tabs = ctx.get_all_tabs().await;
            for tab in tabs {
                ctx.stop_screencast(&tab.tab_id).await;
            }
            // chromiumoxide drops the Browser on Arc::drop → child process exits.
        }
    }

    /// Destroy every live browser context.
    pub async fn destroy_all(&self) -> usize {
        let session_ids = self.list_active_sessions().await;
        let count = session_ids.len();
        for session_id in session_ids {
            self.destroy(&session_id).await;
        }
        count
    }

    /// Returns true if `session_id` has a live Chrome context.
    /// Used to gate lazy browser-tool registration (saves ~6 500 tokens/turn
    /// for non-browser sessions).
    pub async fn has_context(&self, session_id: &str) -> bool {
        self.contexts.read().await.contains_key(session_id)
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

fn context_key_for(session_id: &str, identity_profile_id: Option<&str>) -> Result<String> {
    match identity_profile_id {
        Some(profile_id) => Ok(format!(
            "{}::identity::{}",
            sanitize_profile_segment(session_id)?,
            sanitize_profile_segment(profile_id)?
        )),
        None => Ok(session_id.to_string()),
    }
}

fn key_for_session(key: &str, session_id: &str) -> bool {
    key == session_id
        || sanitize_profile_segment(session_id)
            .map(|safe_session_id| key.starts_with(&format!("{safe_session_id}::identity::")))
            .unwrap_or(false)
}

fn sanitize_profile_segment(segment: &str) -> Result<String> {
    let sanitized: String = segment
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        Err(anyhow!("browser profile segment cannot be empty"))
    } else {
        Ok(sanitized)
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

    #[test]
    fn profile_path_for_identity_isolated_and_sanitized() {
        let base = PathBuf::from("/tmp/test-profiles");
        let path = BrowserContextManager::profile_path_for_identity(
            &base,
            "session abc",
            Some("auth/github.com"),
        )
        .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/test-profiles/session-abc--identity-auth-github.com")
        );
    }

    #[test]
    fn context_key_matches_session_identity_variants() {
        let key = context_key_for("session-abc", Some("auth-123")).unwrap();
        assert_eq!(key, "session-abc::identity::auth-123");
        assert!(key_for_session(&key, "session-abc"));
        assert!(key_for_session("session-abc", "session-abc"));
        assert!(!key_for_session(
            "session-abcd::identity::auth-123",
            "session-abc"
        ));
    }
}
