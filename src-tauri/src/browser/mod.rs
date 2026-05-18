pub mod dom_state;
pub mod tools;
pub mod types;

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
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_state(&self) -> BrowserState {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            None => BrowserState { running: false, tabs: vec![], active_tab_id: None },
            Some(inner) => {
                let tabs: Vec<BrowserTab> = inner.pages.iter().map(|(id, _)| BrowserTab {
                    tab_id: id.clone(),
                    url: String::new(),
                    title: String::new(),
                }).collect();
                let active_tab_id = tabs.first().map(|t| t.tab_id.clone());
                BrowserState { running: true, tabs, active_tab_id }
            }
        }
    }

    pub async fn launch(&self) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        if guard.is_some() {
            return Ok(());
        }

        // Dedicated profile dir so we don't collide with the user's real Chrome.
        let profile_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".uclaw")
            .join("browser-profile");
        if let Err(e) = std::fs::create_dir_all(&profile_dir) {
            tracing::warn!("Could not create browser profile dir: {}", e);
        }

        // Remove stale Chrome singleton lock files that prevent relaunch after
        // a crash or force-quit.
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() {
                tracing::info!("Removing stale browser lock: {:?}", path);
                let _ = std::fs::remove_file(&path);
            }
        }

        let config = BrowserConfig::builder()
            .no_sandbox()
            .user_data_dir(&profile_dir)
            // Give Chrome up to 60 s to start (default 20 s is too short on first
            // run when the profile dir is freshly created and Chrome sets up extensions).
            .launch_timeout(Duration::from_secs(60))
            // Suppress first-run UI and background services that slow startup.
            .args([
                "--no-first-run",
                "--disable-default-apps",
                "--disable-infobars",
                "--disable-notifications",
                "--disable-translate",
                "--disable-extensions",
            ])
            .build()
            .map_err(|e| Error::Internal(format!("Browser config error: {}", e)))?;

        let (browser, mut handler) = Browser::launch(config).await
            .map_err(|e| Error::Internal(format!("Failed to launch browser: {}", e)))?;

        // Drive the CDP event loop in the background
        tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
        });

        *guard = Some(BrowserInner {
            browser,
            pages: HashMap::new(),
        });
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        *guard = None;
        Ok(())
    }

    /// Navigate to a URL, creating a new tab if tab_id is "new" or not found.
    /// Returns the resolved tab_id.
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

    /// Capture a PNG screenshot of the given tab and return it as base64.
    pub async fn screenshot(&self, tab_id: &str) -> Result<ScreenshotResult, Error> {
        use chromiumoxide::page::ScreenshotParams;
        use base64::{Engine, engine::general_purpose::STANDARD};

        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;

        let png_bytes = page.screenshot(ScreenshotParams::default()).await
            .map_err(|e| Error::Internal(format!("Screenshot failed: {}", e)))?;

        Ok(ScreenshotResult {
            data: STANDARD.encode(&png_bytes),
            width: 1280,
            height: 800,
        })
    }

    /// Extract text content from the page body.
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

    /// Click an element by CSS selector.
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

    /// Type text into an element (focus then type).
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

    /// Wait for an element to appear (up to timeout_ms).
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
    fn default() -> Self {
        Self::new()
    }
}
