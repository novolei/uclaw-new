use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::browser::context::DevicePreset;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::dom_state::format_dom_state_for_llm;

// ── Macro: declare all 14 tool structs ────────────────────────────────────────

macro_rules! browser_tool {
    ($name:ident) => {
        pub struct $name {
            pub ctx_mgr: Arc<BrowserContextManager>,
            pub session_id: String,
        }
    };
}

browser_tool!(BrowserNavigateTool);
browser_tool!(BrowserGoBackTool);
browser_tool!(BrowserGoForwardTool);
browser_tool!(BrowserReloadTool);
browser_tool!(BrowserGetDomTool);
browser_tool!(BrowserScreenshotTool);
browser_tool!(BrowserExtractTool);
browser_tool!(BrowserClickTool);
browser_tool!(BrowserTypeTool);
browser_tool!(BrowserSelectTool);
browser_tool!(BrowserScrollTool);
browser_tool!(BrowserSendKeysTool);
browser_tool!(BrowserEvaluateTool);
browser_tool!(BrowserManageTabsTool);
browser_tool!(BrowserGetCookiesTool);
browser_tool!(BrowserSetCookieTool);

// ── 1. BrowserNavigateTool ────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str { "browser_navigate" }

    fn description(&self) -> &str {
        "Navigate to a URL in the browser. Launches the browser if not running. \
         Returns the tab_id for subsequent operations.\n\
         \n\
         **Parameters**\n\
         - `url` (string, required): URL to navigate to.\n\
         - `tab_id` (string, optional): Tab ID to reuse, or 'new' to open a new tab (default 'new').\n\
         - `device` (string, optional): \"mobile\" sets 390\u{d7}844 + iPhone UA; \"desktop\" (default) sets 1280\u{d7}800."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                },
                "tab_id": {
                    "type": "string",
                    "description": "Tab ID to reuse, or 'new' to open a new tab (default 'new')"
                },
                "device": {
                    "type": "string",
                    "enum": ["desktop", "mobile"],
                    "description": "Device preset: 'mobile' sets 390x844 + iPhone UA; 'desktop' (default) sets 1280x800"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let url = params["url"].as_str()
            .ok_or_else(|| ToolError::Execution("url is required".to_string()))?;
        let tab_id = params["tab_id"].as_str().unwrap_or("new");
        let device = params["device"].as_str()
            .map(DevicePreset::from_str)
            .unwrap_or(DevicePreset::Desktop);

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let resolved_id = ctx.navigate(tab_id, url).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        if let Err(e) = ctx.apply_device_emulation(&resolved_id, device).await {
            tracing::warn!("device emulation failed (non-fatal): {e}");
        }

        Ok(ToolOutput::success(
            &format!("Navigated to {}. tab_id={}", url, resolved_id),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 2. BrowserGoBackTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGoBackTool {
    fn name(&self) -> &str { "browser_go_back" }

    fn description(&self) -> &str {
        "Navigate backward in the browser history for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.go_back(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success("Navigated back.", start.elapsed().as_millis() as u64))
    }
}

// ── 3. BrowserGoForwardTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGoForwardTool {
    fn name(&self) -> &str { "browser_go_forward" }

    fn description(&self) -> &str {
        "Navigate forward in the browser history for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.go_forward(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success("Navigated forward.", start.elapsed().as_millis() as u64))
    }
}

// ── 4. BrowserReloadTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserReloadTool {
    fn name(&self) -> &str { "browser_reload" }

    fn description(&self) -> &str {
        "Reload the current page for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.reload(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success("Page reloaded.", start.elapsed().as_millis() as u64))
    }
}

// ── 5. BrowserGetDomTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGetDomTool {
    fn name(&self) -> &str { "browser_get_dom" }

    fn description(&self) -> &str {
        "Return the interactive DOM elements of the current page as an indexed list. \
         Always call browser_get_dom AFTER navigating and BEFORE interacting. \
         Indexes are reassigned on each call; stale indexes will click the wrong element."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let state = ctx.get_dom_state(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let formatted = format_dom_state_for_llm(&state);

        Ok(ToolOutput::success(&formatted, start.elapsed().as_millis() as u64))
    }
}

// ── 6. BrowserScreenshotTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str { "browser_screenshot" }

    fn description(&self) -> &str {
        "Capture a PNG screenshot of the current browser page. Returns base64-encoded PNG."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to screenshot" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let data = ctx.screenshot(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let elapsed = start.elapsed().as_millis() as u64;
        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": true,
                "data": data,
                "width": 1280,
                "height": 800,
            }),
            elapsed,
        ))
    }
}

// ── 7. BrowserExtractTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserExtractTool {
    fn name(&self) -> &str { "browser_extract" }

    fn description(&self) -> &str {
        "Extract the visible text content from the current browser page or a specific element."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to extract text from (default 'body')"
                }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let selector = params["selector"].as_str().unwrap_or("body");

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // Escape single quotes in the selector to avoid JS injection.
        let safe_selector = selector.replace('\'', "\\'");
        let script = format!(
            "(function(){{\
                var el = document.querySelector('{selector}') || document.body;\
                return (el.innerText || el.textContent || '').substring(0, 40000);\
            }})()",
            selector = safe_selector,
        );

        let text = ctx.execute_js(tab_id, &script).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(&text, start.elapsed().as_millis() as u64))
    }
}

// ── 8. BrowserClickTool ───────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str { "browser_click" }

    fn description(&self) -> &str {
        "Click an interactive element by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "index": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Element index from browser_get_dom"
                }
            },
            "required": ["tab_id", "index"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"].as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))? as u32;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.click(tab_id, index).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Clicked element [{}].", index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 9. BrowserTypeTool ────────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserTypeTool {
    fn name(&self) -> &str { "browser_type" }

    fn description(&self) -> &str {
        "Type text into a form field identified by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "index": {
                    "type": "integer",
                    "description": "Element index from browser_get_dom"
                },
                "text": { "type": "string", "description": "Text to type" }
            },
            "required": ["tab_id", "index", "text"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"].as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))? as u32;
        let text = params["text"].as_str()
            .ok_or_else(|| ToolError::Execution("text is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.type_text(tab_id, index, text).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Typed into element [{}].", index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 10. BrowserSelectTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSelectTool {
    fn name(&self) -> &str { "browser_select" }

    fn description(&self) -> &str {
        "Select an option in a <select> element identified by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "index": {
                    "type": "integer",
                    "description": "Element index from browser_get_dom"
                },
                "value": { "type": "string", "description": "Option value to select" }
            },
            "required": ["tab_id", "index", "value"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"].as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))? as u32;
        let value = params["value"].as_str()
            .ok_or_else(|| ToolError::Execution("value is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.select_option(tab_id, index, value).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Selected value '{}' in element [{}].", value, index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 11. BrowserScrollTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserScrollTool {
    fn name(&self) -> &str { "browser_scroll" }

    fn description(&self) -> &str {
        "Scroll the page or a specific element in a direction by a number of pixels."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "pixels": {
                    "type": "integer",
                    "description": "Number of pixels to scroll (default 300)"
                },
                "index": {
                    "type": "integer",
                    "description": "Element index to scroll within (optional; scrolls the window if omitted)"
                }
            },
            "required": ["tab_id", "direction"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let direction = params["direction"].as_str()
            .ok_or_else(|| ToolError::Execution("direction is required".to_string()))?;
        let pixels = params["pixels"].as_u64().unwrap_or(300) as u32;
        let index = params["index"].as_u64().map(|i| i as u32);

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.scroll(tab_id, index, direction, pixels).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Scrolled {} {}px.", direction, pixels),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 12. BrowserSendKeysTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSendKeysTool {
    fn name(&self) -> &str { "browser_send_keys" }

    fn description(&self) -> &str {
        "Send keyboard key events to the page (e.g. 'Enter', 'Escape', 'Tab')."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "keys": {
                    "type": "string",
                    "description": "Key name to send (e.g. 'Enter', 'Escape', 'Tab', 'ArrowDown')"
                }
            },
            "required": ["tab_id", "keys"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let keys = params["keys"].as_str()
            .ok_or_else(|| ToolError::Execution("keys is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.send_keys(tab_id, keys).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Sent key: {}.", keys),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 13. BrowserEvaluateTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserEvaluateTool {
    fn name(&self) -> &str { "browser_evaluate" }

    fn description(&self) -> &str {
        "Execute a JavaScript snippet in the current tab and return the result as a JSON string."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "script": {
                    "type": "string",
                    "description": "JavaScript expression or function to evaluate"
                }
            },
            "required": ["tab_id", "script"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let script = params["script"].as_str()
            .ok_or_else(|| ToolError::Execution("script is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let result = ctx.execute_js(tab_id, script).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(&result, start.elapsed().as_millis() as u64))
    }
}

// ── 14. BrowserManageTabsTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserManageTabsTool {
    fn name(&self) -> &str { "browser_manage_tabs" }

    fn description(&self) -> &str {
        "List all open tabs or close a specific tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": {
                    "type": "string",
                    "description": "Tab ID (used for 'close' action; ignored for 'list')"
                },
                "action": {
                    "type": "string",
                    "enum": ["list", "close"],
                    "description": "'list' returns all open tabs; 'close' closes the specified tab"
                }
            },
            "required": ["tab_id", "action"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let action = params["action"].as_str()
            .ok_or_else(|| ToolError::Execution("action is required".to_string()))?;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        match action {
            "list" => {
                let tabs = ctx.get_all_tabs().await;
                let json = serde_json::to_string_pretty(&tabs)
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(ToolOutput::success(&json, start.elapsed().as_millis() as u64))
            }
            "close" => {
                ctx.close_tab(tab_id).await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(ToolOutput::success(
                    &format!("Closed tab {}.", tab_id),
                    start.elapsed().as_millis() as u64,
                ))
            }
            _ => Err(ToolError::Execution(format!(
                "Unknown action '{}'; expected 'list' or 'close'", action
            ))),
        }
    }
}

// ── 15. BrowserGetCookiesTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGetCookiesTool {
    fn name(&self) -> &str { "browser_get_cookies" }

    fn description(&self) -> &str {
        r#"Retrieve cookies from the current browser session.

Returns all cookies visible to the specified tab. Use `url_filter` to scope
to a specific origin.

**Parameters**
- `tab_id` (string, required): Tab ID from browser_navigate or browser_get_dom.
- `url_filter` (string, optional): Only return cookies for this URL.

**Returns** JSON array of cookie objects: name, value, domain, path, secure,
http_only, same_site, expires.

**Example**
{"tab_id":"tab-1","url_filter":"https://example.com"}
→ [{"name":"session","value":"abc123","domain":"example.com",...}]
"#
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID from browser_navigate or browser_get_dom" },
                "url_filter": { "type": "string", "description": "Only return cookies matching this URL" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let url_filter = params["url_filter"].as_str();
        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        match ctx.get_cookies(tab_id, url_filter).await {
            Ok(cookies) => {
                let json = serde_json::to_string_pretty(&cookies)
                    .unwrap_or_else(|_| "[]".to_string());
                Ok(ToolOutput::success(&json, start.elapsed().as_millis() as u64))
            }
            Err(e) => Ok(ToolOutput::error(&e.to_string(), start.elapsed().as_millis() as u64)),
        }
    }
}

// ── 16. BrowserSetCookieTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSetCookieTool {
    fn name(&self) -> &str { "browser_set_cookie" }

    fn description(&self) -> &str {
        r#"Set a single cookie in the current browser session.

Use this to inject authentication cookies, bypass consent banners, or persist
session tokens before navigating to a page that requires them.

**Parameters**
- `tab_id` (string, required): Tab ID from browser_navigate.
- `name` (string, required): Cookie name.
- `value` (string, required): Cookie value.
- `domain` (string, required): Cookie domain, e.g. "example.com".
- `path` (string, optional): Cookie path. Defaults to "/".
- `secure` (boolean, optional): Set Secure flag. Default false.
- `http_only` (boolean, optional): Set HttpOnly flag. Default false.

**Returns** "Cookie set successfully." on success.
"#
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID from browser_navigate" },
                "name": { "type": "string", "description": "Cookie name" },
                "value": { "type": "string", "description": "Cookie value" },
                "domain": { "type": "string", "description": "Cookie domain, e.g. \"example.com\"" },
                "path": { "type": "string", "description": "Cookie path (optional, defaults to '/')" },
                "secure": { "type": "boolean", "description": "Set Secure flag (default false)" },
                "http_only": { "type": "boolean", "description": "Set HttpOnly flag (default false)" }
            },
            "required": ["tab_id", "name", "value", "domain"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let name = params["name"].as_str()
            .ok_or_else(|| ToolError::Execution("name is required".to_string()))?;
        let value = params["value"].as_str()
            .ok_or_else(|| ToolError::Execution("value is required".to_string()))?;
        let domain = params["domain"].as_str()
            .ok_or_else(|| ToolError::Execution("domain is required".to_string()))?;
        let path = params["path"].as_str();
        let secure = params["secure"].as_bool().unwrap_or(false);
        let http_only = params["http_only"].as_bool().unwrap_or(false);
        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        match ctx.set_cookie(tab_id, name, value, domain, path, secure, http_only).await {
            Ok(_) => Ok(ToolOutput::success("Cookie set successfully.", start.elapsed().as_millis() as u64)),
            Err(e) => Ok(ToolOutput::error(&e.to_string(), start.elapsed().as_millis() as u64)),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn navigate_params_defaults() {
        let params = serde_json::json!({});
        assert!(params["url"].as_str().is_none());
        assert_eq!(params["tab_id"].as_str().unwrap_or("new"), "new");
    }

    #[test]
    fn scroll_pixels_default() {
        let params = serde_json::json!({"tab_id": "t1", "direction": "down"});
        let pixels = params["pixels"].as_u64().unwrap_or(300) as u32;
        assert_eq!(pixels, 300);
    }
}
