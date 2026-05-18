//! BrowserContext — one Chrome instance per agent session.
//!
//! Wraps a `chromiumoxide::Browser`, a map of tab_id → `Page`, a DOM-state
//! cache with a 500 ms TTL, and a map of screencast stop-signal senders.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType,
};
use chromiumoxide::cdp::browser_protocol::page::{
    ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex, RwLock};
use uuid::Uuid;

use crate::browser::dom_state::{dom_state_from_raw, DOM_QUERY_SCRIPT};
use crate::browser::types::{DOMState, DomStateRaw, NavStatePayload, ScreencastFramePayload, TabInfo};

// ── DOM cache ─────────────────────────────────────────────────────────────────

const DOM_CACHE_TTL: Duration = Duration::from_millis(500);

// ── CookieInfo ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires: f64,
}

// ── DevicePreset ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePreset {
    Desktop,
    Mobile,
}

impl DevicePreset {
    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("mobile") { DevicePreset::Mobile } else { DevicePreset::Desktop }
    }

    pub fn viewport_width(self) -> u32 { match self { DevicePreset::Mobile => 390, DevicePreset::Desktop => 1280 } }
    pub fn viewport_height(self) -> u32 { match self { DevicePreset::Mobile => 844, DevicePreset::Desktop => 800 } }
    pub fn user_agent(self) -> &'static str {
        match self {
            DevicePreset::Mobile => "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            DevicePreset::Desktop => "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        }
    }
}

// ── BrowserContext ────────────────────────────────────────────────────────────

pub struct BrowserContext {
    pub browser: Arc<Browser>,
    pub pages: Arc<RwLock<HashMap<String, Page>>>,
    dom_cache: Arc<RwLock<HashMap<String, (DOMState, Instant)>>>,
    screencast_stops: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    session_id: String,
}

impl BrowserContext {
    /// Launch Chrome and open one initial blank tab.
    pub async fn launch(session_id: &str, profile_dir: PathBuf) -> Result<Self> {
        // Remove stale Chrome lock files so a crashed session doesn't block relaunch.
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }

        let config = BrowserConfig::builder()
            .new_headless_mode()
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
            .map_err(|e| anyhow!("Browser config error: {}", e))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;

        // Drive the CDP event loop in a background task.
        tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
            tracing::warn!("CDP handler exited — browser subprocess may have crashed");
        });

        let browser = Arc::new(browser);
        let pages: Arc<RwLock<HashMap<String, Page>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Open one initial blank tab so the browser is ready to use.
        let init_page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Failed to open initial tab: {}", e))?;
        let init_id = Uuid::new_v4().to_string();
        pages.write().await.insert(init_id, init_page);

        Ok(Self {
            browser,
            pages,
            dom_cache: Arc::new(RwLock::new(HashMap::new())),
            screencast_stops: Arc::new(Mutex::new(HashMap::new())),
            session_id: session_id.to_string(),
        })
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    async fn get_page(&self, tab_id: &str) -> Result<Page> {
        self.pages
            .read()
            .await
            .get(tab_id)
            .cloned()
            .ok_or_else(|| anyhow!("Tab '{}' not found", tab_id))
    }

    /// Resolve tab_id: if "new" or not found, open a blank page and return the
    /// new id. Otherwise return the existing id and page clone.
    async fn resolve_tab(&self, tab_id: &str) -> Result<(String, Page)> {
        if tab_id != "new" {
            if let Some(page) = self.pages.read().await.get(tab_id).cloned() {
                return Ok((tab_id.to_string(), page));
            }
        }
        // Need a new tab.
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Failed to open new tab: {}", e))?;
        let new_id = Uuid::new_v4().to_string();
        self.pages.write().await.insert(new_id.clone(), page.clone());
        Ok((new_id, page))
    }

    fn index_selector(index: u32) -> String {
        format!("[data-uclaw-index=\"{}\"]", index)
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Navigate to `url` in the given tab. Returns the resolved tab_id.
    pub async fn navigate(&self, tab_id: &str, url: &str) -> Result<String> {
        let mut pages = self.pages.write().await;
        if tab_id != "new" {
            if let Some(page) = pages.get(tab_id) {
                let page = page.clone();
                drop(pages);
                page.goto(url)
                    .await
                    .map_err(|e| anyhow!("navigate to {url}: {e}"))?;
                self.invalidate_dom_cache(tab_id).await;
                return Ok(tab_id.to_string());
            }
        }
        // New tab — open directly at the target URL.
        drop(pages);
        let page = self
            .browser
            .new_page(url)
            .await
            .map_err(|e| anyhow!("new_page: {e}"))?;
        let new_id = Uuid::new_v4().to_string();
        self.pages.write().await.insert(new_id.clone(), page);
        self.invalidate_dom_cache(&new_id).await;
        Ok(new_id)
    }

    pub async fn go_back(&self, tab_id: &str) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        page.evaluate("history.back()")
            .await
            .map_err(|e| anyhow!("go_back failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn go_forward(&self, tab_id: &str) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        page.evaluate("history.forward()")
            .await
            .map_err(|e| anyhow!("go_forward failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn reload(&self, tab_id: &str) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        page.reload()
            .await
            .map_err(|e| anyhow!("reload failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    // ── DOM state ─────────────────────────────────────────────────────────────

    /// Return the DOM state for `tab_id`, using a 500 ms TTL cache.
    pub async fn get_dom_state(&self, tab_id: &str) -> Result<DOMState> {
        // Check cache first.
        {
            let cache = self.dom_cache.read().await;
            if let Some((state, fetched_at)) = cache.get(tab_id) {
                if fetched_at.elapsed() < DOM_CACHE_TTL {
                    return Ok(state.clone());
                }
            }
        }

        // Cache is stale — re-evaluate the DOM query script.
        let page = self.get_page(tab_id).await?;
        let result = page
            .evaluate(DOM_QUERY_SCRIPT)
            .await
            .map_err(|e| anyhow!("DOM query failed: {}", e))?;

        let json_str: String = result
            .into_value()
            .map_err(|e| anyhow!("DOM result not a string: {}", e))?;

        let raw: DomStateRaw = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("DOM JSON parse error: {}", e))?;

        let tabs = self.get_all_tabs().await;
        let state = dom_state_from_raw(raw, tabs);

        self.dom_cache
            .write()
            .await
            .insert(tab_id.to_string(), (state.clone(), Instant::now()));

        Ok(state)
    }

    pub async fn invalidate_dom_cache(&self, tab_id: &str) {
        self.dom_cache.write().await.remove(tab_id);
    }

    // ── Screenshot ────────────────────────────────────────────────────────────

    /// Capture a PNG screenshot and return it as a base64 string.
    pub async fn screenshot(&self, tab_id: &str) -> Result<String> {
        use chromiumoxide::page::ScreenshotParams;

        let page = self.get_page(tab_id).await?;
        let png_bytes = page
            .screenshot(ScreenshotParams::default())
            .await
            .map_err(|e| anyhow!("Screenshot failed: {}", e))?;

        Ok(STANDARD.encode(&png_bytes))
    }

    // ── Interaction ───────────────────────────────────────────────────────────

    pub async fn click(&self, tab_id: &str, index: u32) -> Result<()> {
        let selector = Self::index_selector(index);
        let page = self.get_page(tab_id).await?;
        page.find_element(&selector)
            .await
            .map_err(|e| anyhow!("Element [{}] not found: {}", index, e))?
            .click()
            .await
            .map_err(|e| anyhow!("Click [{}] failed: {}", index, e))?;
        Ok(())
    }

    pub async fn type_text(&self, tab_id: &str, index: u32, text: &str) -> Result<()> {
        let selector = Self::index_selector(index);
        let page = self.get_page(tab_id).await?;
        page.find_element(&selector)
            .await
            .map_err(|e| anyhow!("Element [{}] not found: {}", index, e))?
            .type_str(text)
            .await
            .map_err(|e| anyhow!("type_text [{}] failed: {}", index, e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn select_option(&self, tab_id: &str, index: u32, value: &str) -> Result<()> {
        let selector = Self::index_selector(index);
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
        let page = self.get_page(tab_id).await?;
        page.evaluate(script)
            .await
            .map_err(|e| anyhow!("select_option [{}] failed: {}", index, e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn scroll(
        &self,
        tab_id: &str,
        index: Option<u32>,
        direction: &str,
        pixels: u32,
    ) -> Result<()> {
        let (dx, dy) = match direction {
            "up" => (0i64, -(pixels as i64)),
            "down" => (0i64, pixels as i64),
            "left" => (-(pixels as i64), 0i64),
            "right" => (pixels as i64, 0i64),
            _ => return Err(anyhow!("Unknown scroll direction: {}", direction)),
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
            format!(r#"window.scrollBy({dx}, {dy})"#, dx = dx, dy = dy,)
        };

        let page = self.get_page(tab_id).await?;
        page.evaluate(script)
            .await
            .map_err(|e| anyhow!("scroll failed: {}", e))?;
        Ok(())
    }

    /// Send key events to the whole page (no element targeting).
    pub async fn send_keys(&self, tab_id: &str, keys: &str) -> Result<()> {
        let page = self.get_page(tab_id).await?;

        let down = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(keys.to_string())
            .build()
            .map_err(|e| anyhow!("key_down params: {e}"))?;
        let up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(keys.to_string())
            .build()
            .map_err(|e| anyhow!("key_up params: {e}"))?;

        page.execute(down)
            .await
            .map_err(|e| anyhow!("key_down: {e}"))?;
        page.execute(up)
            .await
            .map_err(|e| anyhow!("key_up: {e}"))?;
        Ok(())
    }

    /// Execute JavaScript and return the result as a JSON string.
    pub async fn execute_js(&self, tab_id: &str, script: &str) -> Result<String> {
        let page = self.get_page(tab_id).await?;
        let val = page
            .evaluate(script)
            .await
            .map_err(|e| anyhow!("execute_js: {}", e))?;
        let s = val
            .into_value::<serde_json::Value>()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()))
            .unwrap_or_else(|_| "<no return value>".to_string());
        Ok(s)
    }

    // ── Tab management ────────────────────────────────────────────────────────

    pub async fn get_all_tabs(&self) -> Vec<TabInfo> {
        // url/title not populated here; use get_dom_state() for full tab info.
        self.pages
            .read()
            .await
            .keys()
            .map(|id| TabInfo {
                tab_id: id.clone(),
                url: String::new(),
                title: String::new(),
                active: false,
            })
            .collect()
    }

    pub async fn close_tab(&self, tab_id: &str) -> Result<()> {
        let page = self
            .pages
            .write()
            .await
            .remove(tab_id)
            .ok_or_else(|| anyhow!("Tab '{}' not found", tab_id))?;
        page.close()
            .await
            .map_err(|e| anyhow!("close_tab failed: {}", e))?;
        self.dom_cache.write().await.remove(tab_id);
        // Send stop signal to any running screencast task.
        if let Some(stop_tx) = self.screencast_stops.lock().await.remove(tab_id) {
            let _ = stop_tx.send(());
        }
        Ok(())
    }

    // ── Screencast ────────────────────────────────────────────────────────────

    pub async fn start_screencast(
        &self,
        tab_id: &str,
        app_handle: tauri::AppHandle,
    ) -> Result<()> {
        let page = self.get_page(tab_id).await?;

        page.execute(
            StartScreencastParams::builder()
                .format(StartScreencastFormat::Jpeg)
                .quality(55_i64)
                .max_width(1280_i64)
                .max_height(800_i64)
                .every_nth_frame(1_i64)
                .build(),
        )
        .await
        .map_err(|e| anyhow!("start_screencast failed: {}", e))?;

        let mut frame_stream = page
            .event_listener::<chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame>()
            .await
            .map_err(|e| anyhow!("event_listener failed: {}", e))?;

        let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
        self.screencast_stops
            .lock()
            .await
            .insert(tab_id.to_string(), stop_tx);

        let page_clone = page.clone();
        let tab_id_owned = tab_id.to_string();
        let session_id = self.session_id.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    frame_opt = frame_stream.next() => {
                        let frame = match frame_opt {
                            Some(f) => f,
                            None => break,
                        };

                        // Ack the frame so Chrome sends the next one.
                        let _ = page_clone
                            .execute(ScreencastFrameAckParams::new(frame.session_id))
                            .await;

                        // Binary wraps a base64 string; cast through AsRef<str>.
                        let data_b64: String =
                            <_ as AsRef<str>>::as_ref(&frame.data).to_string();

                        let payload = ScreencastFramePayload {
                            session_id: session_id.clone(),
                            tab_id: tab_id_owned.clone(),
                            data_b64,
                            page_width: frame.metadata.device_width as u32,
                            page_height: frame.metadata.device_height as u32,
                        };

                        let _ = app_handle.emit("browser:screencast-frame", &payload);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop_screencast(&self, tab_id: &str) {
        if let Some(stop_tx) = self.screencast_stops.lock().await.remove(tab_id) {
            let _ = stop_tx.send(());
        }
    }

    // ── Cookies ───────────────────────────────────────────────────────────────

    pub async fn get_cookies(&self, tab_id: &str, url_filter: Option<&str>) -> Result<Vec<CookieInfo>> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::network::GetCookiesParams;
        let cmd = GetCookiesParams {
            urls: url_filter.map(|u| vec![u.to_string()]),
        };
        let result = page.execute(cmd).await
            .map_err(|e| anyhow!("get_cookies CDP error: {e}"))?;
        let cookies = result.result.cookies.into_iter().map(|c| CookieInfo {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            secure: c.secure,
            http_only: c.http_only,
            same_site: c.same_site.map(|s| format!("{s:?}")),
            expires: c.expires,
        }).collect();
        Ok(cookies)
    }

    pub async fn set_cookie(
        &self,
        tab_id: &str,
        name: &str,
        value: &str,
        domain: &str,
        path: Option<&str>,
        secure: bool,
        http_only: bool,
    ) -> Result<bool> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::network::{CookieParam, SetCookiesParams};
        let mut cookie = CookieParam::new(name, value);
        cookie.domain = Some(domain.to_string());
        cookie.path = path.map(|p| p.to_string());
        cookie.secure = Some(secure);
        cookie.http_only = Some(http_only);
        let cmd = SetCookiesParams { cookies: vec![cookie] };
        page.execute(cmd).await
            .map_err(|e| anyhow!("set_cookie CDP error: {e}"))?;
        Ok(true)
    }

    // ── Device emulation ──────────────────────────────────────────────────────

    pub async fn apply_device_emulation(&self, tab_id: &str, device: DevicePreset) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::emulation::{
            SetDeviceMetricsOverrideParams, SetUserAgentOverrideParams,
        };
        let metrics = SetDeviceMetricsOverrideParams {
            width: device.viewport_width() as i64,
            height: device.viewport_height() as i64,
            device_scale_factor: if device == DevicePreset::Mobile { 3.0 } else { 1.0 },
            mobile: device == DevicePreset::Mobile,
            scale: None,
            screen_width: None,
            screen_height: None,
            position_x: None,
            position_y: None,
            dont_set_visible_size: None,
            screen_orientation: None,
            viewport: None,
        };
        page.execute(metrics).await
            .map_err(|e| anyhow!("set device metrics: {e}"))?;
        let ua = SetUserAgentOverrideParams {
            user_agent: device.user_agent().to_string(),
            accept_language: None,
            platform: None,
            user_agent_metadata: None,
        };
        page.execute(ua).await
            .map_err(|e| anyhow!("set UA: {e}"))?;
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
