use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};

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
            BrowserAction::ListTabs => {
                let tabs = ctx.get_all_tabs().await;
                let mut r = BrowserActionResult::success(
                    "browser_list_tabs",
                    Some(format!("Listed {} tab(s)", tabs.len())),
                );
                r.observation_json = Some(serde_json::json!({ "tabs": tabs }));
                r
            }
            BrowserAction::SwitchTab { tab_id } => {
                ctx.switch_tab(&tab_id, self.ctx_mgr.app_handle()).await?;
                let mut r = BrowserActionResult::success(
                    "browser_switch_tab",
                    Some(format!("Switched to tab {tab_id}")),
                );
                r.tab_id = Some(tab_id);
                r
            }
            BrowserAction::CloseTab { tab_id } => {
                ctx.close_tab(&tab_id).await?;
                let mut r = BrowserActionResult::success(
                    "browser_close_tab",
                    Some(format!("Closed tab {tab_id}")),
                );
                r.tab_id = ctx.active_or_first_tab_id().await;
                r
            }
            BrowserAction::UploadFile { tab_id, index, file_path } => {
                let resolved = resolve_workspace_file_path(&file_path)?;
                ctx.upload_file(&tab_id, index, &resolved).await?;
                BrowserActionResult::success(
                    "browser_upload_file",
                    Some(format!("Uploaded file '{file_path}' into element [{index}]")),
                )
            }
        };
        result.duration_ms = started.elapsed().as_millis() as u64;
        Ok(result)
    }
}

fn resolve_workspace_file_path(file_path: &str) -> Result<std::path::PathBuf> {
    if file_path.contains("..") {
        return Err(anyhow!("file_path must not contain '..'"));
    }
    let workspace_root = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("Documents/workground");
    let abs_path = workspace_root.join(file_path);
    let canonical_root = workspace_root.canonicalize().unwrap_or(workspace_root);
    let canonical_abs = abs_path
        .canonicalize()
        .map_err(|_| anyhow!("File not found: {} (looked in {})", file_path, abs_path.display()))?;
    if !canonical_abs.starts_with(&canonical_root) {
        return Err(anyhow!("file_path must not escape the workspace directory"));
    }
    Ok(canonical_abs)
}
