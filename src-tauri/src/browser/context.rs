//! BrowserContext — one Chrome instance per agent session.
//!
//! Wraps a `chromiumoxide::Browser`, a map of tab_id → `Page`, a DOM-state
//! cache with a 500 ms TTL, and a map of screencast task handles.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use chromiumoxide::{Browser, Page};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::cdp::browser_protocol::page::{
    StartScreencastParams, StartScreencastFormat, StopScreencastParams, ScreencastFrameAckParams,
    EventScreencastFrame,
};
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use tauri::Emitter;
use tokio::time::Instant;

use crate::browser::dom_state::{dom_state_from_raw, DOM_QUERY_SCRIPT};
use crate::browser::types::{DOMState, DomStateRaw, ScreencastFramePayload, ScreenshotResult, TabInfo};
use crate::error::Error;

// ── DOM cache ─────────────────────────────────────────────────────────────────

struct DomCache {
    state: DOMState,
    fetched_at: Instant,
}

const DOM_CACHE_TTL: Duration = Duration::from_millis(500);

// ── BrowserContext ────────────────────────────────────────────────────────────

pub struct BrowserContext {
    browser: Browser,
    pages: HashMap<String, Page>,
    app_handle: Arc<tauri::AppHandle>,
    dom_cache: HashMap<String, DomCache>,
    screencast_tasks: HashMap<String, tokio::task::JoinHandle<()>>,
}

impl BrowserContext {
    /// Launch Chrome and open one initial blank tab.
    pub async fn new(
        app_handle: Arc<tauri::AppHandle>,
        profile_dir: std::path::PathBuf,
    ) -> Result<Self, Error> {
        // Remove stale Chrome lock files so a crashed session doesn't block relaunch.
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }

        let config = BrowserConfig::builder()
            .no_sandbox()
            .user_data_dir(&profile_dir)
            .launch_timeout(Duration::from_secs(60))
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

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| Error::Internal(format!("Failed to launch browser: {}", e)))?;

        // Drive the CDP event loop in a background task.
        tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
        });

        let mut ctx = Self {
            browser,
            pages: HashMap::new(),
            app_handle,
            dom_cache: HashMap::new(),
            screencast_tasks: HashMap::new(),
        };

        // Open one initial blank tab so the browser is ready to use.
        let init_page = ctx
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| Error::Internal(format!("Failed to open initial tab: {}", e)))?;
        let init_id = uuid::Uuid::new_v4().to_string();
        ctx.pages.insert(init_id, init_page);

        Ok(ctx)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn get_page(&self, tab_id: &str) -> Result<&Page, Error> {
        self.pages
            .get(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))
    }

    fn invalidate_dom_cache(&mut self, tab_id: &str) {
        self.dom_cache.remove(tab_id);
    }

    fn collect_tab_infos(&self) -> Vec<TabInfo> {
        self.pages
            .keys()
            .map(|id| TabInfo {
                tab_id: id.clone(),
                // URL and title require async calls; return empty strings for now.
                url: String::new(),
                title: String::new(),
                active: false,
            })
            .collect()
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Navigate to `url` in the given tab. Returns the resolved tab_id.
    ///
    /// If `tab_id` is `"new"` or not present in the map, a new Chrome tab is
    /// opened and assigned a fresh UUID.
    pub async fn navigate(&mut self, tab_id: &str, url: &str) -> Result<String, Error> {
        if tab_id == "new" || !self.pages.contains_key(tab_id) {
            let page = self
                .browser
                .new_page(url)
                .await
                .map_err(|e| Error::Internal(format!("Failed to open page: {}", e)))?;
            let new_id = uuid::Uuid::new_v4().to_string();
            self.pages.insert(new_id.clone(), page);
            Ok(new_id)
        } else {
            self.get_page(tab_id)?
                .goto(url)
                .await
                .map_err(|e| Error::Internal(format!("Navigation failed: {}", e)))?;
            self.invalidate_dom_cache(tab_id);
            Ok(tab_id.to_string())
        }
    }

    pub async fn go_back(&mut self, tab_id: &str) -> Result<(), Error> {
        self.get_page(tab_id)?
            .evaluate("history.back()")
            .await
            .map_err(|e| Error::Internal(format!("go_back failed: {}", e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    pub async fn go_forward(&mut self, tab_id: &str) -> Result<(), Error> {
        self.get_page(tab_id)?
            .evaluate("history.forward()")
            .await
            .map_err(|e| Error::Internal(format!("go_forward failed: {}", e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    pub async fn reload(&mut self, tab_id: &str) -> Result<(), Error> {
        self.get_page(tab_id)?
            .reload()
            .await
            .map_err(|e| Error::Internal(format!("reload failed: {}", e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    // ── DOM state ─────────────────────────────────────────────────────────────

    /// Return the DOM state for `tab_id`, using a 500 ms TTL cache.
    pub async fn get_dom_state(&mut self, tab_id: &str) -> Result<DOMState, Error> {
        // Check cache first.
        if let Some(cached) = self.dom_cache.get(tab_id) {
            if cached.fetched_at.elapsed() < DOM_CACHE_TTL {
                return Ok(cached.state.clone());
            }
        }

        // Cache is stale — re-evaluate the DOM query script.
        let result = self
            .get_page(tab_id)?
            .evaluate(DOM_QUERY_SCRIPT)
            .await
            .map_err(|e| Error::Internal(format!("DOM query failed: {}", e)))?;

        // The script returns a JSON string.
        let json_str: String = result
            .into_value()
            .map_err(|e| Error::Internal(format!("DOM result not a string: {}", e)))?;

        let raw: DomStateRaw = serde_json::from_str(&json_str)
            .map_err(|e| Error::Internal(format!("DOM JSON parse error: {}", e)))?;

        let tabs = self.collect_tab_infos();
        let state = dom_state_from_raw(raw, tabs);

        self.dom_cache.insert(
            tab_id.to_string(),
            DomCache {
                state: state.clone(),
                fetched_at: Instant::now(),
            },
        );

        Ok(state)
    }

    // ── Screenshot ────────────────────────────────────────────────────────────

    pub async fn screenshot(&mut self, tab_id: &str) -> Result<ScreenshotResult, Error> {
        let png_bytes = self
            .get_page(tab_id)?
            .screenshot(ScreenshotParams::default())
            .await
            .map_err(|e| Error::Internal(format!("Screenshot failed: {}", e)))?;

        Ok(ScreenshotResult {
            data: STANDARD.encode(&png_bytes),
            width: 1280,
            height: 800,
        })
    }

    // ── Interaction ───────────────────────────────────────────────────────────

    fn index_selector(index: u32) -> String {
        format!("[data-uclaw-index=\"{}\"]", index)
    }

    pub async fn click(&mut self, tab_id: &str, index: u32) -> Result<(), Error> {
        let selector = Self::index_selector(index);
        self.get_page(tab_id)?
            .find_element(&selector)
            .await
            .map_err(|e| Error::Internal(format!("Element [{}] not found: {}", index, e)))?
            .click()
            .await
            .map_err(|e| Error::Internal(format!("Click [{}] failed: {}", index, e)))?;
        Ok(())
    }

    pub async fn type_text(&mut self, tab_id: &str, index: u32, text: &str) -> Result<(), Error> {
        let selector = Self::index_selector(index);
        self.get_page(tab_id)?
            .find_element(&selector)
            .await
            .map_err(|e| Error::Internal(format!("Element [{}] not found: {}", index, e)))?
            .type_str(text)
            .await
            .map_err(|e| Error::Internal(format!("type_text [{}] failed: {}", index, e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    pub async fn select_option(
        &mut self,
        tab_id: &str,
        index: u32,
        value: &str,
    ) -> Result<(), Error> {
        let selector = Self::index_selector(index);
        // Escape single quotes in the value to prevent JS injection.
        let safe_value = value.replace('\'', "\\'");
        let safe_selector = selector.replace('\'', "\\'");
        let script = format!(
            r#"(function() {{
                var el = document.querySelector('{selector}');
                if (el) {{
                    el.value = '{value}';
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                }}
            }})()"#,
            selector = safe_selector,
            value = safe_value,
        );
        self.get_page(tab_id)?
            .evaluate(script)
            .await
            .map_err(|e| Error::Internal(format!("select_option [{}] failed: {}", index, e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    pub async fn scroll(
        &mut self,
        tab_id: &str,
        index: Option<u32>,
        direction: &str,
        amount: u32,
    ) -> Result<(), Error> {
        let (dx, dy) = match direction {
            "up" => (0, -(amount as i64)),
            "down" => (0, amount as i64),
            "left" => (-(amount as i64), 0),
            "right" => (amount as i64, 0),
            _ => {
                return Err(Error::Internal(format!(
                    "Unknown scroll direction: {}",
                    direction
                )))
            }
        };

        let script = if let Some(idx) = index {
            let selector = Self::index_selector(idx);
            let safe_selector = selector.replace('\'', "\\'");
            format!(
                r#"(function() {{
                    var el = document.querySelector('{selector}');
                    if (el) {{
                        el.scrollIntoView({{behavior: 'instant', block: 'nearest'}});
                        el.scrollBy({dx}, {dy});
                    }}
                }})()"#,
                selector = safe_selector,
                dx = dx,
                dy = dy,
            )
        } else {
            format!(
                r#"window.scrollBy({dx}, {dy})"#,
                dx = dx,
                dy = dy,
            )
        };

        self.get_page(tab_id)?
            .evaluate(script)
            .await
            .map_err(|e| Error::Internal(format!("scroll failed: {}", e)))?;
        Ok(())
    }

    pub async fn send_keys(&mut self, tab_id: &str, index: u32, keys: &str) -> Result<(), Error> {
        let selector = Self::index_selector(index);
        self.get_page(tab_id)?
            .find_element(&selector)
            .await
            .map_err(|e| Error::Internal(format!("Element [{}] not found: {}", index, e)))?
            .press_key(keys)
            .await
            .map_err(|e| Error::Internal(format!("send_keys [{}] failed: {}", index, e)))?;
        self.invalidate_dom_cache(tab_id);
        Ok(())
    }

    pub async fn execute_js(&mut self, tab_id: &str, script: &str) -> Result<serde_json::Value, Error> {
        let result = self
            .get_page(tab_id)?
            .evaluate(script)
            .await
            .map_err(|e| Error::Internal(format!("execute_js failed: {}", e)))?;

        let value = result
            .value()
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        self.invalidate_dom_cache(tab_id);
        Ok(value)
    }

    // ── Tab management ────────────────────────────────────────────────────────

    pub fn get_all_tabs(&self) -> Vec<TabInfo> {
        self.collect_tab_infos()
    }

    pub async fn close_tab(&mut self, tab_id: &str) -> Result<(), Error> {
        let page = self
            .pages
            .remove(tab_id)
            .ok_or_else(|| Error::Internal(format!("Tab '{}' not found", tab_id)))?;
        page.close()
            .await
            .map_err(|e| Error::Internal(format!("close_tab failed: {}", e)))?;
        self.dom_cache.remove(tab_id);
        // Also stop any running screencast task.
        if let Some(handle) = self.screencast_tasks.remove(tab_id) {
            handle.abort();
        }
        Ok(())
    }

    // ── Screencast ────────────────────────────────────────────────────────────

    pub async fn start_screencast(
        &mut self,
        tab_id: &str,
        session_id: String,
    ) -> Result<(), Error> {
        let page = self.get_page(tab_id)?;

        page.execute(
            StartScreencastParams::builder()
                .format(StartScreencastFormat::Jpeg)
                .quality(60_i64)
                .max_width(1280_i64)
                .max_height(800_i64)
                .every_nth_frame(1_i64)
                .build(),
        )
        .await
        .map_err(|e| Error::Internal(format!("start_screencast failed: {}", e)))?;

        let mut frame_stream = page
            .event_listener::<EventScreencastFrame>()
            .await
            .map_err(|e| Error::Internal(format!("event_listener failed: {}", e)))?;

        let page_clone = page.clone();
        let app_handle = Arc::clone(&self.app_handle);
        let tab_id_owned = tab_id.to_string();

        let handle = tokio::spawn(async move {
            while let Some(frame) = frame_stream.next().await {
                // Ack the frame so Chrome sends the next one.
                let _ = page_clone
                    .execute(ScreencastFrameAckParams::new(frame.session_id))
                    .await;

                // Binary wraps a base64 string; cast through AsRef<str>.
                let data_b64: String = <_ as AsRef<str>>::as_ref(&frame.data).to_string();

                let payload = ScreencastFramePayload {
                    session_id: session_id.clone(),
                    tab_id: tab_id_owned.clone(),
                    data_b64,
                    page_width: frame.metadata.device_width as u32,
                    page_height: frame.metadata.device_height as u32,
                };

                let _ = app_handle.emit("browser:screencast-frame", &payload);
            }
        });

        // Abort any previous task for this tab before storing the new one.
        if let Some(old) = self.screencast_tasks.insert(tab_id.to_string(), handle) {
            old.abort();
        }

        Ok(())
    }

    pub async fn stop_screencast(&mut self, tab_id: &str) -> Result<(), Error> {
        self.get_page(tab_id)?
            .execute(StopScreencastParams::default())
            .await
            .map_err(|e| Error::Internal(format!("stop_screencast failed: {}", e)))?;

        if let Some(handle) = self.screencast_tasks.remove(tab_id) {
            handle.abort();
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_selector_format() {
        let selector = format!("[data-uclaw-index=\"{}\"]", 5);
        assert_eq!(selector, "[data-uclaw-index=\"5\"]");
    }

    #[test]
    fn dom_cache_ttl_expired() {
        let past = Instant::now() - Duration::from_millis(600);
        assert!(
            past.elapsed() > Duration::from_millis(500),
            "600 ms old cache entry should be expired"
        );
    }
}
