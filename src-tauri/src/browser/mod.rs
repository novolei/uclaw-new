pub mod tools;
pub mod types;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::Error;
use self::types::BrowserState;

/// BrowserService manages the lifecycle of a headless Chromium instance via CDP.
/// Phase 3 implementation — currently provides state management skeleton only.
/// Full implementation requires the `ai-browser` feature flag with chromiumoxide.
pub struct BrowserService {
    state: Arc<RwLock<BrowserState>>,
}

impl BrowserService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(BrowserState {
                running: false,
                tabs: vec![],
                active_tab_id: None,
            })),
        }
    }

    pub async fn get_state(&self) -> BrowserState {
        self.state.read().await.clone()
    }

    /// Launch headless Chromium and establish CDP connection.
    /// Requires the `ai-browser` feature flag (chromiumoxide).
    pub async fn launch(&self) -> Result<(), Error> {
        Err(Error::Internal("AI Browser not yet implemented — enable feature 'ai-browser' and implement Phase 3".into()))
    }

    /// Stop Chromium and clean up CDP connections.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.running = false;
        state.tabs.clear();
        state.active_tab_id = None;
        Ok(())
    }
}

impl Default for BrowserService {
    fn default() -> Self {
        Self::new()
    }
}
