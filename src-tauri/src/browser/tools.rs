/// CDP tool registry for the AI Browser.
/// Tools registered into the agent ToolRegistry when BrowserService is active.
/// Full CDP implementation added in Phase 3 (requires ai-browser feature).

pub const BROWSER_TOOLS: &[(&str, &str)] = &[
    ("browser_navigate",    "Navigate to a URL in the browser tab"),
    ("browser_screenshot",  "Capture a screenshot of the current page"),
    ("browser_click",       "Click an element by CSS selector or coordinates"),
    ("browser_fill",        "Fill an input field with text"),
    ("browser_hover",       "Hover over an element"),
    ("browser_press_key",   "Press a keyboard key"),
    ("browser_scroll",      "Scroll the page or a specific element"),
    ("browser_drag",        "Drag from one element to another"),
    ("browser_evaluate",    "Execute JavaScript in the page context"),
    ("browser_inspect",     "Get the accessibility tree or DOM snapshot"),
    ("browser_snapshot",    "Get an accessibility snapshot of the page"),
    ("browser_wait_for",    "Wait for an element or condition"),
    ("browser_download",    "Download a file from the current URL"),
    ("browser_tab",         "Open, close, or switch between browser tabs"),
];

pub fn browser_tool_count() -> usize {
    BROWSER_TOOLS.len()
}
