use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use url::Url;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

/// Default request timeout in milliseconds.
const DEFAULT_TIMEOUT_MS: u64 = 15_000;

/// Maximum response body size (1 MB).
const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

/// User-Agent header.
const USER_AGENT: &str = "uClaw/0.1";

/// Validate a URL to prevent SSRF attacks.
/// Rejects non-http(s) schemes and private/loopback addresses.
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http/https URLs are allowed".into());
    }
    if let Some(host) = parsed.host_str() {
        if host == "localhost" || host == "::1" || host.starts_with("127.")
            || host.starts_with("10.") || host.starts_with("192.168.")
            || host.starts_with("169.254.") {
            return Err("Access to private/loopback addresses is not allowed".into());
        }
        // Check 172.16.0.0 - 172.31.255.255
        if host.starts_with("172.") {
            if let Some(second) = host.split('.').nth(1) {
                if let Ok(n) = second.parse::<u8>() {
                    if (16..=31).contains(&n) {
                        return Err("Access to private addresses is not allowed".into());
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WebFetchTool — fetch a web page and return plain text
// ---------------------------------------------------------------------------

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }

    /// Naive HTML → plain-text extractor.
    /// Strips `<script>` / `<style>` blocks and tags, keeps text content.
    fn extract_text(html: &str) -> String {
        let mut result = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;
        let mut tag_name = String::new();

        for ch in html.chars() {
            match ch {
                '<' => {
                    in_tag = true;
                    tag_name.clear();
                }
                '>' => {
                    in_tag = false;
                    let lower = tag_name.to_ascii_lowercase();
                    if lower == "script" {
                        in_script = true;
                    } else if lower == "/script" {
                        in_script = false;
                    } else if lower == "style" {
                        in_style = true;
                    } else if lower == "/style" {
                        in_style = false;
                    } else if matches!(
                        lower.as_str(),
                        "br" | "p" | "/p" | "div" | "/div" | "h1" | "/h1" | "h2" | "/h2"
                            | "h3" | "/h3" | "h4" | "/h4" | "li" | "/li" | "tr" | "/tr"
                    ) {
                        result.push('\n');
                    }
                }
                _ if in_tag => {
                    // Accumulate tag name (stop at space or / for attributes)
                    if tag_name.len() < 20 && ch != ' ' && ch != '/' {
                        tag_name.push(ch);
                    }
                }
                _ if !in_tag && !in_script && !in_style => {
                    result.push(ch);
                }
                _ => {}
            }
        }

        // Decode common HTML entities
        let result = result
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ");

        // Collapse blank lines
        result
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<&str>>()
            .join("\n")
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page and return its text content. \
         HTML is automatically converted to plain text (scripts and styles are removed)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Request timeout in milliseconds (default 15000)"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters to return (default 50000)"
                }
            },
            "required": ["url"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);
        let max_length = params["max_length"].as_u64().unwrap_or(50_000) as usize;

        // Validate URL to prevent SSRF
        if let Err(reason) = validate_url(url) {
            warn!(url, reason = %reason, "web_fetch: URL rejected");
            return Err(ToolError::InvalidParams(reason));
        }

        info!(url, timeout_ms, "Fetching web page");

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(format!("Failed to create HTTP client: {}", e)))?;

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::Execution(format!("Failed to fetch {}: {}", url, e)))?;

        let status = resp.status();
        if !status.is_success() {
            warn!(url, %status, "Non-success HTTP status");
        }

        // Read body with size limit
        let content_length = resp.content_length().unwrap_or(0) as usize;
        if content_length > MAX_RESPONSE_SIZE {
            return Err(ToolError::Execution(format!(
                "Response too large ({} bytes, max {} bytes)",
                content_length, MAX_RESPONSE_SIZE
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::Execution(format!("Failed to read response body: {}", e)))?;

        if body.len() > MAX_RESPONSE_SIZE {
            return Err(ToolError::Execution(format!(
                "Response body too large ({} bytes, max {} bytes)",
                body.len(),
                MAX_RESPONSE_SIZE
            )));
        }

        let text = Self::extract_text(&body);
        let truncated = if text.len() > max_length {
            format!(
                "{}...\n[Truncated: showing {}/{} characters]",
                &text[..max_length],
                max_length,
                text.len()
            )
        } else {
            text
        };

        debug!(url, chars = truncated.len(), "Web page fetched");
        Ok(ToolOutput::success(
            &truncated,
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ---------------------------------------------------------------------------
// HttpRequestTool — generic HTTP request
// ---------------------------------------------------------------------------

pub struct HttpRequestTool;

impl HttpRequestTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request. Supports GET, POST, PUT, DELETE, PATCH methods \
         with custom headers and JSON body."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "The URL to send the request to"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers as key-value pairs"
                },
                "body": {
                    "type": "string",
                    "description": "Optional request body (JSON string)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Request timeout in milliseconds (default 15000)"
                }
            },
            "required": ["method", "url"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let method_str = params["method"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("method is required".into()))?;
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);

        // Validate URL to prevent SSRF
        if let Err(reason) = validate_url(url) {
            warn!(method = method_str, url, reason = %reason, "http_request: URL rejected");
            return Err(ToolError::InvalidParams(reason));
        }

        info!(method = method_str, url, "Making HTTP request");

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(format!("Failed to create HTTP client: {}", e)))?;

        let method = match method_str.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            "PATCH" => reqwest::Method::PATCH,
            other => {
                return Err(ToolError::InvalidParams(format!(
                    "Unsupported HTTP method: {}",
                    other
                )))
            }
        };

        let mut request = client.request(method, url);

        // Apply custom headers
        if let Some(headers) = params["headers"].as_object() {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    request = request.header(key.as_str(), val);
                }
            }
        }

        // Apply body
        if let Some(body) = params["body"].as_str() {
            if !body.is_empty() {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body.to_string());
            }
        }

        let resp = request
            .send()
            .await
            .map_err(|e| ToolError::Execution(format!("HTTP request failed: {}", e)))?;

        let status = resp.status().as_u16();
        let resp_headers: HashMap<String, String> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::Execution(format!("Failed to read response body: {}", e)))?;

        // Truncate very large bodies
        let body_display = if body.len() > MAX_RESPONSE_SIZE {
            format!(
                "{}...\n[Truncated: {}/{} bytes]",
                &body[..MAX_RESPONSE_SIZE],
                MAX_RESPONSE_SIZE,
                body.len()
            )
        } else {
            body
        };

        let result = serde_json::json!({
            "ok": true,
            "status": status,
            "headers": resp_headers,
            "body": body_display
        });

        debug!(method = method_str, url, status, "HTTP request completed");
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}
