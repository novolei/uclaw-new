pub mod action;
pub mod action_registry;
pub mod agent_loop;
pub mod boundary;
pub mod context;
pub mod context_manager;
pub mod decision;
pub mod dom_state;
pub mod loop_detector; // stub — full implementation in Plan 2 Task 15
pub mod observation;
pub mod recovery;
pub mod session_state;
pub mod task_store;
pub mod tools;
pub mod types;

// Re-export the two primary public types so callers can write
// `crate::browser::BrowserContextManager` without the extra path.
pub use context_manager::BrowserContextManager;
pub use types::{DOMState, ScreencastFramePayload};

// ── Legacy BrowserService ─────────────────────────────────────────────
// Kept as-is to power the four existing backward-compat Tauri commands:
// browser_get_state, browser_launch, browser_shutdown, browser_take_screenshot.
// Do NOT add new features here — use BrowserContext / BrowserContextManager.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use futures::StreamExt;
use chromiumoxide::{Browser, Page};
use chromiumoxide::browser::BrowserConfig;
use crate::error::Error;
use self::types::{BrowserState, BrowserTab, ScreenshotResult};

struct BrowserInner {
    browser: Browser,
    pages: HashMap<String, Page>,
}

pub struct BrowserService {
    inner: Arc<RwLock<Option<BrowserInner>>>,
}

impl BrowserService {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(None)) }
    }

    pub async fn get_state(&self) -> BrowserState {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            None => BrowserState { running: false, tabs: vec![], active_tab_id: None },
            Some(inner) => {
                let tabs: Vec<BrowserTab> = inner.pages.iter().map(|(id, _)| BrowserTab {
                    tab_id: id.clone(), url: String::new(), title: String::new(),
                }).collect();
                let active_tab_id = tabs.first().map(|t| t.tab_id.clone());
                BrowserState { running: true, tabs, active_tab_id }
            }
        }
    }

    pub async fn launch(&self) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        if guard.is_some() { return Ok(()); }
        let profile_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".uclaw").join("browser-profile");
        if let Err(e) = std::fs::create_dir_all(&profile_dir) {
            tracing::warn!("Could not create browser profile dir: {}", e);
        }
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() { let _ = std::fs::remove_file(&path); }
        }
        let config = BrowserConfig::builder()
            .no_sandbox().user_data_dir(&profile_dir)
            .launch_timeout(Duration::from_secs(60))
            .args(["--no-first-run","--disable-default-apps","--disable-infobars",
                   "--disable-notifications","--disable-translate","--disable-extensions"])
            .build()
            .map_err(|e| Error::Internal(format!("Browser config error: {}", e)))?;
        let (browser, mut handler) = Browser::launch(config).await
            .map_err(|e| Error::Internal(format!("Failed to launch browser: {}", e)))?;
        tokio::spawn(async move { while let Some(_) = handler.next().await {} });
        *guard = Some(BrowserInner { browser, pages: HashMap::new() });
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        *self.inner.write().await = None;
        Ok(())
    }

    pub async fn navigate(&self, tab_id: &str, url: &str) -> Result<String, Error> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        if tab_id == "new" || !inner.pages.contains_key(tab_id) {
            let page = inner.browser.new_page(url).await
                .map_err(|e| Error::Internal(format!("Failed to open page: {}", e)))?;
            let new_id = uuid::Uuid::new_v4().to_string();
            inner.pages.insert(new_id.clone(), page);
            Ok(new_id)
        } else {
            let page = inner.pages.get(tab_id).unwrap();
            page.goto(url).await
                .map_err(|e| Error::Internal(format!("Navigation failed: {}", e)))?;
            Ok(tab_id.to_string())
        }
    }

    pub async fn screenshot(&self, tab_id: &str) -> Result<ScreenshotResult, Error> {
        use chromiumoxide::page::ScreenshotParams;
        use base64::{Engine, engine::general_purpose::STANDARD};
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        let png_bytes = page.screenshot(ScreenshotParams::default()).await
            .map_err(|e| Error::Internal(format!("Screenshot failed: {}", e)))?;
        Ok(ScreenshotResult { data: STANDARD.encode(&png_bytes), width: 1280, height: 800 })
    }

    pub async fn extract_text(&self, tab_id: &str) -> Result<String, Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        let text = page.evaluate("document.body.innerText").await
            .map_err(|e| Error::Internal(format!("Extract failed: {}", e)))?
            .into_value::<String>()
            .unwrap_or_default();
        Ok(text)
    }

    pub async fn click(&self, tab_id: &str, selector: &str) -> Result<(), Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        page.find_element(selector).await
            .map_err(|e| Error::Internal(format!("Element '{}' not found: {}", selector, e)))?
            .click().await
            .map_err(|e| Error::Internal(format!("Click failed: {}", e)))?;
        Ok(())
    }

    pub async fn type_text(&self, tab_id: &str, selector: &str, text: &str) -> Result<(), Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        page.find_element(selector).await
            .map_err(|e| Error::Internal(format!("Element '{}' not found: {}", selector, e)))?
            .type_str(text).await
            .map_err(|e| Error::Internal(format!("Type failed: {}", e)))?;
        Ok(())
    }

    pub async fn wait_for_selector(&self, tab_id: &str, selector: &str, timeout_ms: u64) -> Result<(), Error> {
        use tokio::time::{timeout, Duration};
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        timeout(Duration::from_millis(timeout_ms), page.find_element(selector)).await
            .map_err(|_| Error::Internal(format!("Timeout waiting for '{}'", selector)))?
            .map_err(|e| Error::Internal(format!("Wait failed: {}", e)))?;
        Ok(())
    }
}

impl Default for BrowserService {
    fn default() -> Self { Self::new() }
}
