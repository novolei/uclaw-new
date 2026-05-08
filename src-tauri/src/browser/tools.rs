use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use super::BrowserService;

macro_rules! browser_tool {
    ($name:ident, $tool_name:expr, $desc:expr) => {
        pub struct $name {
            browser: Arc<BrowserService>,
        }
        impl $name {
            pub fn new(browser: Arc<BrowserService>) -> Self {
                Self { browser }
            }
        }
        #[async_trait]
        impl Tool for $name {
            fn name(&self) -> &str { $tool_name }
            fn description(&self) -> &str { $desc }
        }
    };
}

pub struct BrowserNavigateTool {
    browser: Arc<BrowserService>,
}
impl BrowserNavigateTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str { "browser_navigate" }
    fn description(&self) -> &str { "Navigate to a URL in the browser. Launches the browser if not running. Returns the tab_id for subsequent operations." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" },
                "tab_id": { "type": "string", "description": "Tab ID to reuse, or 'new' to open a new tab (default 'new')" }
            },
            "required": ["url"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let url = params["url"].as_str()
            .ok_or_else(|| ToolError::Execution("url is required".to_string()))?;
        let tab_id = params["tab_id"].as_str().unwrap_or("new");

        if let Err(e) = self.browser.launch().await {
            tracing::warn!("Browser launch (pre-navigate): {}", e);
        }

        let resolved_tab_id = self.browser.navigate(tab_id, url).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Navigated to {}. tab_id={}", url, resolved_tab_id),
            start.elapsed().as_millis() as u64,
        ))
    }
}

pub struct BrowserScreenshotTool {
    browser: Arc<BrowserService>,
}
impl BrowserScreenshotTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str { "browser_screenshot" }
    fn description(&self) -> &str { "Capture a PNG screenshot of the current browser page. Returns base64-encoded PNG." }
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

        let result = self.browser.screenshot(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let summary = serde_json::json!({
            "width": result.width,
            "height": result.height,
            "data": result.data,
        });
        Ok(ToolOutput::success(
            &summary.to_string(),
            start.elapsed().as_millis() as u64,
        ))
    }
}

pub struct BrowserExtractTool {
    browser: Arc<BrowserService>,
}
impl BrowserExtractTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserExtractTool {
    fn name(&self) -> &str { "browser_extract" }
    fn description(&self) -> &str { "Extract the visible text content from the current browser page." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to extract text from" }
            },
            "required": ["tab_id"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        let text = self.browser.extract_text(tab_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(&text, start.elapsed().as_millis() as u64))
    }
}

pub struct BrowserClickTool {
    browser: Arc<BrowserService>,
}
impl BrowserClickTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str { "browser_click" }
    fn description(&self) -> &str { "Click an element identified by a CSS selector in the browser page." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "selector": { "type": "string", "description": "CSS selector of the element to click" }
            },
            "required": ["tab_id", "selector"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let selector = params["selector"].as_str()
            .ok_or_else(|| ToolError::Execution("selector is required".to_string()))?;

        self.browser.click(tab_id, selector).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Clicked '{}'", selector),
            start.elapsed().as_millis() as u64,
        ))
    }
}

pub struct BrowserTypeTool {
    browser: Arc<BrowserService>,
}
impl BrowserTypeTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserTypeTool {
    fn name(&self) -> &str { "browser_type" }
    fn description(&self) -> &str { "Type text into a form field identified by a CSS selector." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "selector": { "type": "string", "description": "CSS selector of the input element" },
                "text": { "type": "string", "description": "Text to type" }
            },
            "required": ["tab_id", "selector", "text"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let selector = params["selector"].as_str()
            .ok_or_else(|| ToolError::Execution("selector is required".to_string()))?;
        let text = params["text"].as_str()
            .ok_or_else(|| ToolError::Execution("text is required".to_string()))?;

        self.browser.type_text(tab_id, selector, text).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Typed {} chars into '{}'", text.len(), selector),
            start.elapsed().as_millis() as u64,
        ))
    }
}

pub struct BrowserWaitTool {
    browser: Arc<BrowserService>,
}
impl BrowserWaitTool {
    pub fn new(browser: Arc<BrowserService>) -> Self { Self { browser } }
}
#[async_trait]
impl Tool for BrowserWaitTool {
    fn name(&self) -> &str { "browser_wait" }
    fn description(&self) -> &str { "Wait for a CSS selector to appear in the browser page." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID" },
                "selector": { "type": "string", "description": "CSS selector to wait for" },
                "timeout_ms": { "type": "integer", "description": "Maximum wait time in milliseconds (default 10000)" }
            },
            "required": ["tab_id", "selector"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"].as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let selector = params["selector"].as_str()
            .ok_or_else(|| ToolError::Execution("selector is required".to_string()))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);

        self.browser.wait_for_selector(tab_id, selector, timeout_ms).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("'{}' appeared", selector),
            start.elapsed().as_millis() as u64,
        ))
    }
}
