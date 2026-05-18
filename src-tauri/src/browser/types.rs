use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Task 1: DOM state types ──────────────────────────────────────────────────

/// A single interactive DOM element after normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOMElement {
    pub index: usize,
    pub tag: String,
    pub text: String,
    pub attributes: HashMap<String, Option<String>>,
    pub is_in_viewport: bool,
    pub xpath: String,
    pub bounding_box: Option<BoundingBox>,
}

/// Normalized bounding box (page coordinates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Full DOM snapshot returned by the injection script after normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOMState {
    pub url: String,
    pub title: String,
    pub elements: Vec<DOMElement>,
    pub page_text: String,
    pub tabs: Vec<TabInfo>,
}

/// Lightweight tab descriptor used in DOMState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub active: bool,
}

// ── Raw (deserialized directly from JS JSON) variants ───────────────────────

/// Raw DOM element as returned by the JS injection script (before normalization).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomElementRaw {
    pub index: usize,
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,
    pub is_in_viewport: bool,
    pub xpath: String,
    pub bounding_box: Option<BoundingBoxRaw>,
}

/// Raw bounding box as returned by the JS injection script.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBoxRaw {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Raw DOM state as returned by the JS injection script (before normalization).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomStateRaw {
    pub url: String,
    pub title: String,
    pub elements: Vec<DomElementRaw>,
    pub page_text: String,
}

// ── End Task 1 types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub tab_id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub data: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateInput {
    pub tab_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickInput {
    pub tab_id: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillInput {
    pub tab_id: String,
    pub selector: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateInput {
    pub tab_id: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub running: bool,
    pub tabs: Vec<BrowserTab>,
    pub active_tab_id: Option<String>,
}
