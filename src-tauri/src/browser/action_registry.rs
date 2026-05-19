use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::context_manager::BrowserContextManager;

pub struct BrowserActionRegistry {
    ctx_mgr: Arc<BrowserContextManager>,
}

impl BrowserActionRegistry {
    pub fn new(ctx_mgr: Arc<BrowserContextManager>) -> Self {
        Self { ctx_mgr }
    }

    pub async fn execute(
        &self,
        session_id: &str,
        action: BrowserAction,
    ) -> Result<BrowserActionResult> {
        let started = Instant::now();
        let ctx = self.ctx_mgr.get_or_create(session_id).await?;
        let mut result = match action {
            BrowserAction::Navigate { url, tab_id } => {
                let id = ctx
                    .navigate(
                        tab_id.as_deref().unwrap_or("new"),
                        &url,
                        self.ctx_mgr.app_handle(),
                    )
                    .await?;
                let mut r = BrowserActionResult::success(
                    "browser_navigate",
                    Some(format!("Navigated to {url}")),
                );
                r.tab_id = Some(id);
                r
            }
            BrowserAction::Click { tab_id, index } => {
                ctx.click(&tab_id, index).await?;
                BrowserActionResult::success(
                    "browser_click",
                    Some(format!("Clicked element [{index}]")),
                )
            }
            BrowserAction::Type {
                tab_id,
                index,
                text,
            } => {
                ctx.type_text(&tab_id, index, &text).await?;
                BrowserActionResult::success(
                    "browser_type",
                    Some(format!("Typed into element [{index}]")),
                )
            }
            BrowserAction::Scroll {
                tab_id,
                direction,
                pixels,
                index,
            } => {
                ctx.scroll(&tab_id, index, &direction, pixels.unwrap_or(300))
                    .await?;
                BrowserActionResult::success(
                    "browser_scroll",
                    Some(format!("Scrolled {direction}")),
                )
            }
            BrowserAction::SendKeys { tab_id, keys } => {
                ctx.send_keys(&tab_id, &keys).await?;
                BrowserActionResult::success(
                    "browser_send_keys",
                    Some(format!("Sent key: {keys}")),
                )
            }
            BrowserAction::Evaluate { tab_id, script } => {
                let output = ctx.execute_js(&tab_id, &script).await?;
                BrowserActionResult::success("browser_evaluate", Some(output))
            }
            BrowserAction::GetState {
                tab_id,
                include_screenshot,
            } => {
                let observation = ctx.observe(&tab_id, include_screenshot).await?;
                let mut r = BrowserActionResult::success(
                    "browser_get_state",
                    Some("Observed page state".into()),
                );
                r.observation_json = Some(serde_json::to_value(observation)?);
                r
            }
        };
        result.duration_ms = started.elapsed().as_millis() as u64;
        Ok(result)
    }
}
