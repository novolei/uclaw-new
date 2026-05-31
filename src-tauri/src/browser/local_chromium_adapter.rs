use anyhow::Result;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::action_registry::BrowserActionRegistry;

pub struct LocalChromiumProviderAdapter<'a> {
    action_registry: &'a BrowserActionRegistry,
}

impl<'a> LocalChromiumProviderAdapter<'a> {
    pub fn new(action_registry: &'a BrowserActionRegistry) -> Self {
        Self { action_registry }
    }

    pub async fn execute_action(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
        action: BrowserAction,
    ) -> Result<BrowserActionResult> {
        self.action_registry
            .execute_with_identity(session_id, identity_profile_id, action)
            .await
    }
}
