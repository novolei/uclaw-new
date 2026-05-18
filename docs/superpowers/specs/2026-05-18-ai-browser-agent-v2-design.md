# AI Browser Agent v2 — Design Spec

**Status:** Design approved 2026-05-18.  
**Supersedes:** `2026-04-30-phase3-ai-browser-design.md` (Svelte 5 era, partially implemented).  
**Coordinates with:** `2026-05-18-preview-panel-multi-tab-design.md` (Browser Tab integration into preview panel).  
**Base branch:** `main` at the commit where this spec is authored.

---

## 0. Audit Summary: Current State vs Target

### What exists today

| File | LOC | Status |
|---|---|---|
| `src-tauri/src/browser/mod.rs` | 212 | Single global `BrowserService`, chromiumoxide 0.9 |
| `src-tauri/src/browser/tools.rs` | 234 | 6 tools: navigate, screenshot, extract, click, type, wait |
| `src-tauri/src/browser/types.rs` | 54 | `BrowserState`, `BrowserTab` |
| `ui/src/components/agent/BrowserPreviewOverlay.tsx` | 115 | Floating overlay, static screenshot polling (1.2s + 3.5s delays) |
| `ui/src/atoms/browser-atoms.ts` | 28 | `browserStateAtom`, `isBrowserLoadingAtom` |
| `ui/src/atoms/agent-atoms.ts` (L967-981) | — | `sessionBrowserPreviewMapAtom` |
| `ui/src/lib/tauri-bridge.ts` (L1670-1685) | — | 4 Tauri commands wired |
| `ui/src/hooks/useGlobalAgentListeners.ts` (L492-567) | — | Tool-result → screenshot trigger |

### Critical gaps vs browser-use / hello-halo parity

1. **DOM representation**: tools use raw CSS selectors — LLM must guess selector strings, high hallucination rate.
2. **Missing actions**: scroll, keyboard, back/forward, reload, JS eval, dropdown select, cookies.
3. **No per-session isolation**: single global Chrome instance, concurrent sessions share cookies/tabs.
4. **Static screenshots**: overlay polling is delayed and disconnected from live page state.
5. **No structured page summary**: LLM cannot "see" the page structure without a screenshot.
6. **No error retry / loop detection**: agent can spin forever on broken pages.
7. **No live browser panel**: no user-accessible browser view beyond the small overlay.

---

## 1. Goals

- **G1** Full browser-use action parity: 14 tools covering navigation, DOM interaction, extraction, JS eval, tabs, keyboard, scroll.
- **G2** Index-based element addressing: LLM references elements as integers `[1]…[N]`, not CSS selectors.
- **G3** Per-session `BrowserContext` isolation: each agent session gets its own Chrome process + profile directory.
- **G4** CDP Screencast live rendering: BrowserPreviewOverlay upgraded from polling to real-time JPEG stream.
- **G5** Browser Panel Tab: new "Browser" tab type in the preview panel (coordinated with preview-panel-multi-tab spec).
- **G6** DOM index overlay in panel: visual badges showing element indices over the live screen.
- **G7** Phase 3 robustness: loop detection, error retry with short/long-term memory, cookie import/export, device emulation.

---

## 2. Architecture Overview

```
┌─────────────── uclaw (Tauri v2) ──────────────────────────────────────────┐
│                                                                            │
│  Frontend (React 18 + Jotai)                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │  BrowserPreviewOverlay.tsx  (upgraded: CDP Screencast live frames)  │  │
│  │                                                                     │  │
│  │  PreviewPanel  ←  preview-panel-multi-tab-design.md                │  │
│  │    ├── PreviewTabBar (file tabs + Browser tab)                     │  │
│  │    └── PreviewContent                                              │  │
│  │         ├── FileRenderer   (existing)                              │  │
│  │         └── BrowserPanel  (NEW)                                    │  │
│  │              ├── BrowserAddressBar                                 │  │
│  │              ├── BrowserTabBar                                     │  │
│  │              ├── BrowserScreencastView  (live JPEG stream)        │  │
│  │              └── BrowserDOMOverlay  (index badges)               │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                              ↕ Tauri IPC / events                         │
│  Backend (Rust / uclaw_core)                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │  browser/                                                           │  │
│  │    context_manager.rs   BrowserContextManager (replaces BrowserService)│
│  │    context.rs           BrowserContext — per-session state machine  │  │
│  │    dom_state.rs         DOMState snapshot + index mapping           │  │
│  │    tools.rs             14 agent tools (index-addressed)            │  │
│  │    screencast.rs        CDP Screencast frame stream → Tauri events  │  │
│  │    types.rs             Extended types                              │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                              ↕ CDP WebSocket                              │
│  ┌─ Per-session (lazy) ──────────────────────────────────────────────┐    │
│  │  Chromium process 1  (~/.uclaw/browser-profiles/{session_id_1}/)  │    │
│  │  Chromium process 2  (~/.uclaw/browser-profiles/{session_id_2}/)  │    │
│  │  …                                                                │    │
│  └───────────────────────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Backend Design

### 3.1 Module structure

```
src-tauri/src/browser/
  context_manager.rs   ← NEW: replaces mod.rs BrowserService as AppState member
  context.rs           ← NEW: BrowserContext (per-session lifecycle + page map)
  dom_state.rs         ← NEW: DOMState snapshot generation via JS injection
  tools.rs             ← REWRITE: 14 tools with index addressing
  screencast.rs        ← NEW: CDP Screencast frame relay → Tauri events
  types.rs             ← EXTEND: DOMElement, BoundingBox, ScreencastFrame, TabInfo
  mod.rs               ← THIN: re-exports BrowserContextManager for AppState
```

`AppState` change in `app.rs`: replace `browser_service: Arc<BrowserService>` with `browser_ctx_mgr: Arc<BrowserContextManager>`.

### 3.2 BrowserContextManager

```rust
pub struct BrowserContextManager {
    /// session_id → active context
    contexts: Arc<RwLock<HashMap<String, Arc<BrowserContext>>>>,
    /// ~/.uclaw/browser-profiles/
    profile_base: PathBuf,
}

impl BrowserContextManager {
    /// Lazy-init: first tool call for a session creates the Chrome process.
    pub async fn get_or_create(&self, session_id: &str) -> Result<Arc<BrowserContext>>;

    /// Called by SessionManager on session close.
    pub async fn destroy(&self, session_id: &str) -> Result<()>;

    /// For Tauri command browser_list_sessions.
    pub fn list_active_sessions(&self) -> Vec<String>;
}
```

### 3.3 BrowserContext

One Chrome process per active session. Profile directory: `~/.uclaw/browser-profiles/{session_id}/`.
Stale lock file cleanup on launch (reuse existing logic from old `mod.rs`).

```rust
pub struct BrowserContext {
    pub session_id: String,
    browser:        Arc<Browser>,                            // chromiumoxide
    pages:          Arc<RwLock<HashMap<String, Page>>>,     // tab_id (UUID) → Page
    active_tab:     Arc<RwLock<Option<String>>>,
    /// Cached DOMState snapshots. Invalidated on every navigation / interaction.
    dom_cache:      Arc<RwLock<HashMap<String, DOMState>>>,
    /// Active screencast sender. None when screencast is off.
    screencast_tx:  Arc<Mutex<Option<broadcast::Sender<ScreencastFrame>>>>,
    profile_dir:    PathBuf,
}

impl BrowserContext {
    pub async fn navigate(&self, tab_id: &str, url: &str) -> Result<String>;  // returns resolved tab_id
    pub async fn get_dom_state(&self, tab_id: &str) -> Result<DOMState>;
    pub async fn invalidate_dom_cache(&self, tab_id: &str);
    pub async fn screenshot(&self, tab_id: &str, full_page: bool) -> Result<String>; // base64 PNG
    pub async fn execute_js(&self, tab_id: &str, code: &str) -> Result<serde_json::Value>;
    pub async fn start_screencast(&self, tab_id: &str, quality: u8, max_width: u32, fps: u8) -> Result<broadcast::Receiver<ScreencastFrame>>;
    pub async fn stop_screencast(&self, tab_id: &str) -> Result<()>;
    pub fn get_all_tabs(&self) -> Vec<TabInfo>;
    pub fn get_active_tab_id(&self) -> Option<String>;
}
```

### 3.4 DOMState — index-based element addressing

Generated by injecting a JS evaluation script into the page. No CDP Accessibility calls needed (faster, no extra CDP roundtrip).

```rust
pub struct DOMState {
    pub url:         String,
    pub title:       String,
    pub elements:    Vec<DOMElement>,
    pub page_text:   String,          // truncated to 40 000 chars
    pub tabs:        Vec<TabInfo>,
    pub captured_at: u64,             // unix ms
}

pub struct DOMElement {
    pub index:        u32,
    pub tag:          String,         // "button", "input", "a", "select", "textarea"
    pub text:         String,         // visible text or placeholder
    pub attributes:   IndexMap<String, String>,   // href, type, value, placeholder, aria-label, name, id
    pub is_in_viewport: bool,
    pub xpath:        String,         // internal: used by click/type to re-locate element
    pub bounding_box: Option<BoundingBox>,        // {x, y, width, height} — for DOM overlay
}

pub struct BoundingBox {
    pub x: f64, pub y: f64, pub width: f64, pub height: f64,
}

pub struct TabInfo {
    pub tab_id: String,
    pub url:    String,
    pub title:  String,
    pub active: bool,
}
```

**JS injection script** (injected via `page.evaluate(JS_QUERY_SCRIPT)`):
- Queries all interactive elements: `button, a[href], input:not([type=hidden]), select, textarea, [role=button], [role=link], [role=checkbox], [role=menuitem]`
- Filters: not `display:none`, not `visibility:hidden`, not `disabled`
- Assigns sequential `data-uclaw-index` attribute on each element
- Returns JSON: `[{ index, tag, text, attributes, isInViewport, xpath, boundingBox }]`
- XPath generated via `document.evaluate('...')` for re-location
- Truncates page text to 40 000 chars (first 40 000 chars of `document.body.innerText`)

**LLM-facing representation** (returned by `browser_get_dom` tool as string):

```
URL: https://login.example.com/
Title: 登录 - Example
Tabs: [0] 当前页(active)  [1] https://example.com/

INTERACTIVE ELEMENTS
[1] <input> type=email  placeholder="邮箱地址"
[2] <input> type=password  placeholder="密码"
[3] <button> "登录"
[4] <a> href="/forgot-password"  "忘记密码？"
[5] <a> href="/signup"  "注册新账户"

PAGE TEXT (first 500 chars)
欢迎登录 Example。请输入您的邮箱和密码…
```

### 3.5 Screencast relay (screencast.rs)

```rust
pub struct ScreencastRelay {
    session_id: String,
    tab_id:     String,
}

impl ScreencastRelay {
    /// Calls CDP Page.startScreencast and pumps frames into a broadcast channel.
    /// Each Page.screencastFrame CDP event → ScreencastFrame → emit Tauri event
    /// `browser:screencast-frame` with payload ScreencastFramePayload.
    /// After emitting, calls Page.screencastFrameAck to request next frame.
    pub async fn start(
        page:      &Page,
        quality:   u8,     // JPEG quality 0–100, default 75
        max_width: u32,    // default 1280
        fps:       u8,     // maps to every_nth_frame = max(1, 60 / fps)
        app_handle: AppHandle,
        session_id: String,
        tab_id:    String,
    ) -> Result<Self>;

    pub async fn stop(&self, page: &Page) -> Result<()>;
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreencastFramePayload {
    pub session_id:   String,
    pub tab_id:       String,
    pub data_b64:     String,     // base64 JPEG
    pub timestamp_ms: u64,
    pub page_width:   u32,
    pub page_height:  u32,
}
```

Tauri event name: `browser:screencast-frame`.

### 3.6 Tool set (tools.rs) — 14 tools

All tools accept `session_id: String` (added to tool schema) so `BrowserContextManager::get_or_create` is called implicitly.  All interaction tools call `context.invalidate_dom_cache(tab_id)` after execution.

#### Group A: Navigation (4 tools)

| Tool | Key params | Notes |
|---|---|---|
| `browser_navigate` | `session_id`, `url`, `tab_id?` ("new" → new tab) | Auto-starts context. Returns `{ tab_id }`. |
| `browser_go_back` | `session_id`, `tab_id` | History.back() via `page.evaluate`. |
| `browser_go_forward` | `session_id`, `tab_id` | History.forward() via `page.evaluate`. |
| `browser_reload` | `session_id`, `tab_id`, `hard: bool` | `hard=true` bypasses cache (`Page.reload({ ignoreCache: true })`). |

#### Group B: Page awareness (3 tools)

| Tool | Key params | Notes |
|---|---|---|
| `browser_get_dom` | `session_id`, `tab_id`, `include_screenshot: bool` | Returns DOMState as LLM-readable string. When `include_screenshot=true`, appends base64 PNG as image content block. This is the tool LLM calls every step. |
| `browser_screenshot` | `session_id`, `tab_id`, `full_page: bool` | Standalone visual capture. Returns base64 PNG. |
| `browser_extract` | `session_id`, `tab_id`, `query: String` | Passes `page_text` + `query` to the configured LLM to extract structured data. Returns extracted JSON or text. |

#### Group C: Interaction (5 tools)

| Tool | Key params | Notes |
|---|---|---|
| `browser_click` | `session_id`, `tab_id`, `index: u32` | Re-locates element via stored XPath → `DOM.scrollIntoViewIfNeeded` → center-point click via `Input.dispatchMouseEvent`. Invalidates DOM cache. |
| `browser_type` | `session_id`, `tab_id`, `index: u32`, `text: String`, `clear: bool` | `clear=true` selects-all then deletes before typing. Invalidates DOM cache. |
| `browser_select` | `session_id`, `tab_id`, `index: u32`, `value: String` | For `<select>` and ARIA listbox/combobox. Matches by option text (case-insensitive). |
| `browser_scroll` | `session_id`, `tab_id`, `direction: "up"\|"down"\|"left"\|"right"`, `pages: f32`, `index?: u32` | `pages=1.0` = one viewport height/width. When `index` given, scrolls that element's container; otherwise scrolls `window`. |
| `browser_send_keys` | `session_id`, `tab_id`, `keys: String` | Dispatches key sequence. Supports modifier combos: `Escape`, `Enter`, `Tab`, `Control+a`, `Meta+k`, etc. via `Input.dispatchKeyEvent`. |

#### Group D: Advanced (2 tools)

| Tool | Key params | Notes |
|---|---|---|
| `browser_evaluate` | `session_id`, `tab_id`, `code: String` | Executes JS as IIFE: `(function() { return (${code}); })()`. Returns JSON-serialized result. Escape hatch for anything other tools can't handle. |
| `browser_manage_tabs` | `session_id`, `action: "list"\|"close"`, `tab_id?: String` | `list` returns `Vec<TabInfo>`. `close` closes the specified tab (refuses to close the last tab). New tabs are created via `browser_navigate(tab_id="new")`. |

#### Tool description design (hello-halo teaching-doc pattern)

Every tool description field is a teaching manual:

```
browser_click: 点击由 browser_get_dom 返回的 index 指向的页面元素。
必须先调用 browser_get_dom 获取最新 DOM 快照，再用 index 整数调用此工具。
⚠️ 点击后页面可能变化，必须重新调用 browser_get_dom 才能获取更新后的 index 映射。
对于 <select> 下拉菜单，使用 browser_select 而非此工具。
若元素不在视口内，工具会自动滚动到该元素再点击。
若点击后触发导航，等待页面加载完成后再继续。
```

#### Legacy tool deprecation

Old tools `browser_click(selector)`, `browser_type(selector)`, `browser_wait(selector)` are removed. The new `tools.rs` replaces the file entirely.

---

## 4. Tauri Commands

New commands added to `tauri_commands.rs` and registered in `invoke_handler!` in `main.rs`:

```rust
// Context lifecycle
browser_list_sessions(state)  → Vec<String>
browser_destroy_session(state, session_id: String)  → bool

// Screencast control
browser_start_screencast(state, session_id: String, tab_id: String, quality: u8, max_width: u32, fps: u8)  → ()
browser_stop_screencast(state, session_id: String, tab_id: String)  → ()

// DOM state (for frontend overlay rendering)
browser_get_dom_state(state, session_id: String, tab_id: String)  → DOMStateResponse

// Manual navigation from UI address bar
browser_ui_navigate(state, session_id: String, url: String, tab_id: String)  → TabInfo
browser_ui_go_back(state, session_id: String, tab_id: String)  → bool
browser_ui_go_forward(state, session_id: String, tab_id: String)  → bool
browser_ui_reload(state, session_id: String, tab_id: String)  → bool
browser_ui_close_tab(state, session_id: String, tab_id: String)  → bool

// Existing commands (keep, update to use BrowserContextManager)
browser_get_state(state)  → BrowserStateResponse  // now aggregates across all sessions
browser_launch(state)     → bool                  // deprecated but kept for backward compat
browser_shutdown(state)   → bool                  // destroys all contexts
browser_take_screenshot(state, tab_id: String)    → String  // removed session_id for backward compat
```

Tauri event emitted by backend: `browser:screencast-frame` (payload: `ScreencastFramePayload`).

---

## 5. Frontend Design

### 5.1 Atom additions (`ui/src/atoms/browser-atoms.ts`)

```typescript
// Screencast frames: session_id → latest JPEG base64
export const browserScreencastFrameAtom = atom<Map<string, string>>(new Map())

// DOM state: session_id → latest DOMStateResponse (for overlay)
export const browserDOMStateAtom = atom<Map<string, DOMStateResponse>>(new Map())

// Which sessions have active screencast
export const browserScreencastActiveAtom = atom<Set<string>>(new Set())

// DOM overlay toggle (per-session)
export const browserDOMOverlayVisibleAtom = atom<Map<string, boolean>>(new Map())
```

`DOMStateResponse` TypeScript interface mirrors `DOMState` Rust struct (url, title, elements with boundingBox, tabs, capturedAt).

### 5.2 BrowserPreviewOverlay.tsx — upgrade to live screencast

Remove: 1200ms + 3500ms delayed `browserTakeScreenshot` calls.  
Add: subscribe to `browser:screencast-frame` Tauri event on mount; update `<img src="data:image/jpeg;base64,…">` on each frame.

Screencast lifecycle:
- On mount (agent session becomes active + BrowserPreviewState.visible=true): call `browser_start_screencast(sessionId, tabId, 75, 1280, 15)`.
- On unmount / minimize / session end: call `browser_stop_screencast`.
- When BrowserPanel Tab is open and active: overlay auto-minimizes to compact mode (64×40px thumbnail in corner) to avoid redundant display.

Compact mode (new): when minimized, show only 64×40 live thumbnail + hostname badge. Tap to expand. Expand behavior unchanged.

### 5.3 Preview panel Browser Tab integration

**Coordination with preview-panel-multi-tab-design.md:**

Extend `PreviewTabItem` in `ui/src/atoms/preview-panel-atoms.ts`:

```typescript
export type PreviewTabType = 'file' | 'browser'

export interface PreviewTabItem {
  // Existing file-tab fields
  mountId:      string
  relPath:      string
  name:         string
  absolutePath: string
  sessionId?:   string
  source:       PreviewTabSource
  addedAt:      number
  // New discriminant
  type:         PreviewTabType   // default 'file' for backward compat
  // Browser-specific (only when type === 'browser')
  browser?: {
    agentSessionId: string   // links to agent session
    initialUrl:    string
  }
}
```

Identity key for browser tab: `browser:${agentSessionId}` (via `previewTabKey` helper extension).

New action `openBrowserTabAction`:

```typescript
export const openBrowserTabAction = atom(
  null,
  (_get, set, payload: { agentSessionId: string; initialUrl?: string }) => {
    const tab: PreviewTabItem = {
      mountId:      'browser',
      relPath:      payload.agentSessionId,
      name:         '浏览器',
      absolutePath: '',
      source:       'agent',
      addedAt:      Date.now(),
      type:         'browser',
      browser: {
        agentSessionId: payload.agentSessionId,
        initialUrl:    payload.initialUrl ?? '',
      },
    }
    set(openPreviewTabAction, { target: tab as any, source: 'agent' })
  }
)
```

`useGlobalAgentListeners.ts` calls `openBrowserTabAction` on first `browser_navigate` tool result per session (same logic that currently updates `sessionBrowserPreviewMapAtom`).

### 5.4 BrowserPanel.tsx (new component)

Rendered by `PreviewContent` when `activeTab.type === 'browser'`.

```
┌──────────────────────────────────────────────────────────────┐
│ ← → ↻  [ https://login.example.com/               ] [⊕]   │  AddressBar
├──────────────────────────────────────────────────────────────┤
│ [● 登录页面]  [示例主页]  [+ 新标签]                          │  BrowserTabBar
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   ┌── BrowserScreencastView ──────────────────────────────┐ │
│   │  <img src="data:image/jpeg;base64,…" />               │ │  Live JPEG stream
│   │                                                        │ │
│   │  BrowserDOMOverlay (canvas, z-index above img)         │ │
│   │    [1]──────────────────────────                       │ │  Index badges
│   │    [2]──────────────────────────                       │ │  (toggle on/off)
│   │    [3] 🟦 登录                                         │ │
│   └────────────────────────────────────────────────────────┘ │
├──────────────────────────────────────────────────────────────┤
│ [🔢 DOM 索引 ON]  [🖥 1280×800]   session: abc-1234    [⛶]  │  StatusBar
└──────────────────────────────────────────────────────────────┘
```

**Sub-components:**

`BrowserAddressBar` — url input field + Enter to navigate + back/forward/reload buttons. Calls `browser_ui_navigate` / `browser_ui_go_back` / `browser_ui_go_forward` / `browser_ui_reload`.

`BrowserTabBar` — renders `tabs` from latest `DOMStateResponse.tabs`. Click to switch active tab (calls `browser_navigate` with existing tab_id). `[+]` button calls `browser_navigate(tabId="new", url="about:blank")`.

`BrowserScreencastView` — `<div style="position:relative">` containing:
- `<img>` subscribing to `browserScreencastFrameAtom[sessionId]`
- `<canvas>` overlay (BrowserDOMOverlay) positioned absolutely over the img

`BrowserDOMOverlay` — renders on canvas over the screencast image:
- Reads `browserDOMStateAtom[sessionId].elements[i].boundingBox`
- Scales from page coordinates to display coordinates (pageWidth/displayWidth ratio from `ScreencastFramePayload`)
- Draws per element: semi-transparent rounded rect + index badge (color coded: blue=button/link, orange=input, green=select)
- Hover → tooltip showing `[N] <tag> "text" attribute=value`
- When agent executes a tool call with an index, that badge pulses (driven by `chat:stream-tool-activity` event)

`BrowserStatusBar` — DOM overlay toggle switch + current viewport dimensions + session id badge + fullscreen toggle.

### 5.5 Tauri bridge additions (`ui/src/lib/tauri-bridge.ts`)

```typescript
// Screencast
export const browserStartScreencast = (
  sessionId: string, tabId: string, quality: number, maxWidth: number, fps: number
): Promise<void> => invoke('browser_start_screencast', { session_id: sessionId, tab_id: tabId, quality, max_width: maxWidth, fps })

export const browserStopScreencast = (sessionId: string, tabId: string): Promise<void> =>
  invoke('browser_stop_screencast', { session_id: sessionId, tab_id: tabId })

// DOM state for overlay
export const browserGetDOMState = (sessionId: string, tabId: string): Promise<DOMStateResponse> =>
  invoke('browser_get_dom_state', { session_id: sessionId, tab_id: tabId })

// UI navigation (from address bar / buttons, not agent tools)
export const browserUINavigate = (sessionId: string, url: string, tabId: string): Promise<TabInfoResponse> =>
  invoke('browser_ui_navigate', { session_id: sessionId, url, tab_id: tabId })

export const browserUIGoBack = (sessionId: string, tabId: string): Promise<boolean> =>
  invoke('browser_ui_go_back', { session_id: sessionId, tab_id: tabId })

export const browserUIGoForward = (sessionId: string, tabId: string): Promise<boolean> =>
  invoke('browser_ui_go_forward', { session_id: sessionId, tab_id: tabId })

export const browserUIReload = (sessionId: string, tabId: string): Promise<boolean> =>
  invoke('browser_ui_reload', { session_id: sessionId, tab_id: tabId })
```

Tauri event listener (in `useGlobalAgentListeners` or a new `useBrowserScreencast` hook):

```typescript
const unlisten = await listen<ScreencastFramePayload>('browser:screencast-frame', (event) => {
  set(browserScreencastFrameAtom, prev =>
    new Map(prev).set(event.payload.sessionId, event.payload.dataB64))
})
```

---

## 6. Phase 3 Robustness Features

### 6.1 Loop detection

Sliding window of 20 agent steps. Each step hash = `{tool_name}:{normalized_args}`.  
`browser_click` normalizes to `click:{index}`. `browser_navigate` normalizes to `navigate:{url}`.  
When same hash appears ≥ 3 times in window: emit a `BrowserLoopWarning` tool result  
(not a hard stop) advising the LLM to try a different approach.

Implemented in `browser/loop_detector.rs`, called by `tools.rs` before tool execution.

### 6.2 Error retry

`browser_click`, `browser_type` return `BrowserToolError` when element not found (XPath lookup fails after DOM changed). The error includes the failed index and suggests calling `browser_get_dom` first to refresh the snapshot.

`browser_navigate` retries once after 3s if DOM is empty after navigation (matches browser-use's empty-page health check).

### 6.3 Cookie / storage management (Phase 3 tools, 2 additional tools)

| Tool | Key params | Notes |
|---|---|---|
| `browser_get_cookies` | `session_id`, `tab_id`, `domain?: String` | Returns `Vec<CookieInfo>` as JSON string. |
| `browser_set_cookies` | `session_id`, `cookies: Vec<CookieInfo>` | Imports cookies (e.g., from saved login state). |

`CookieInfo`: `{ name, value, domain, path, expires, httpOnly, secure }`.

### 6.4 Device emulation

`browser_navigate` accepts optional `device: "mobile" | "desktop"`. Calls CDP `Emulation.setDeviceMetricsOverride`:
- `desktop`: 1280×800, `deviceScaleFactor=1`, `mobile=false`
- `mobile`: 430×932, `deviceScaleFactor=3`, `mobile=true`, iPhone UA string

Stored per-tab in `BrowserContext` for subsequent screenshots to report correct dimensions to screencast relay.

---

## 7. Data Flow

### Agent navigates and tool result triggers live preview

```
agent calls browser_navigate(session_id, url)
  → BrowserContextManager::get_or_create(session_id)  [lazy: launches Chrome if first call]
    → BrowserContext::navigate(tab_id, url)
      → chromiumoxide::Page::goto(url)
        → emit Tauri event "browser:page-changed" { session_id, tab_id, url, title }
          → useGlobalAgentListeners: update sessionBrowserPreviewMapAtom
          → useGlobalAgentListeners: call openBrowserTabAction(session_id)  [opens Browser Tab in preview panel]
          → BrowserPreviewOverlay: call browser_start_screencast(session_id, tab_id, 75, 1280, 15)
            → CDP Page.startScreencast → frames → Tauri "browser:screencast-frame" events
              → browserScreencastFrameAtom updated at ~15 FPS
                → BrowserPreviewOverlay <img> live update
                → BrowserPanel BrowserScreencastView <img> live update
```

### LLM requests DOM state

```
agent calls browser_get_dom(session_id, tab_id, include_screenshot=false)
  → BrowserContext::get_dom_state(tab_id)
    → check dom_cache: if fresh (<500ms old), return cached
    → else: page.evaluate(JS_QUERY_SCRIPT) → Vec<DOMElement>
      → cache result → return DOMState as LLM string
        → emit Tauri event "browser:dom-state-updated" { session_id, tab_id, domState }
          → browserDOMStateAtom updated
            → BrowserDOMOverlay re-renders index badges
```

### User navigates via address bar

```
user types URL + Enter in BrowserAddressBar
  → invoke browser_ui_navigate(session_id, url, tab_id)
    → BrowserContext::navigate(tab_id, url)  [same as agent path]
      → dom_cache invalidated
      → "browser:page-changed" event → screencast auto-updates
```

---

## 8. Error Handling

| Layer | Strategy |
|---|---|
| Tool layer | Returns `ToolOutput::Error(msg)` never panics. Agent receives error text and can retry. |
| DOM index stale | Error message: "Element [N] not found — call browser_get_dom to refresh the index." |
| Chrome not found | `get_or_create` returns `Err(BrowserError::ChromiumNotFound)` → tool returns instructive error. |
| Chrome crash | On CDP disconnect, `BrowserContext` marks itself `crashed=true`. Next tool call returns error and triggers `destroy` + optional auto-restart. |
| Screencast lag | If `browser:screencast-frame` events stop for >5s, frontend shows "connecting…" overlay on screencast view. |
| Navigation timeout | `page.goto` has 30s timeout. On timeout: returns partial state with `timed_out: true` in tool output. |

---

## 9. Testing

| Test | Location | Coverage |
|---|---|---|
| DOMState JS script | `browser/dom_state.rs` `#[cfg(test)]` | Mock HTML → expected element list |
| BrowserContextManager | `browser/context_manager.rs` `#[cfg(test)]` | create/get/destroy lifecycle |
| Loop detector | `browser/loop_detector.rs` `#[cfg(test)]` | Window sliding, hash normalization |
| Tool output format | `browser/tools.rs` `#[cfg(test)]` | DOMState → LLM string rendering |
| previewTabsAtom (browser type) | `ui/src/atoms/preview-panel-atoms.test.ts` | openBrowserTabAction, key uniqueness, close |
| BrowserPanel render | `ui/src/components/preview/BrowserPanel.test.tsx` | addressBar input, tabBar tabs count, overlay toggle |
| BrowserDOMOverlay | `ui/src/components/preview/BrowserDOMOverlay.test.tsx` | bounding box scaling, badge rendering |

---

## 10. File Map

### Backend

| File | Action |
|---|---|
| `src-tauri/src/browser/mod.rs` | THIN re-export only |
| `src-tauri/src/browser/context_manager.rs` | NEW |
| `src-tauri/src/browser/context.rs` | NEW |
| `src-tauri/src/browser/dom_state.rs` | NEW |
| `src-tauri/src/browser/tools.rs` | REWRITE (old 6 → new 14 tools) |
| `src-tauri/src/browser/screencast.rs` | NEW |
| `src-tauri/src/browser/loop_detector.rs` | NEW (Phase 3) |
| `src-tauri/src/browser/types.rs` | EXTEND |
| `src-tauri/src/app.rs` | MODIFY: replace `browser_service` → `browser_ctx_mgr` |
| `src-tauri/src/tauri_commands.rs` | EXTEND: new commands |
| `src-tauri/src/main.rs` | EXTEND: register new commands |

### Frontend

| File | Action |
|---|---|
| `ui/src/atoms/browser-atoms.ts` | EXTEND: screencast/DOM state atoms |
| `ui/src/atoms/preview-panel-atoms.ts` | EXTEND: PreviewTabType, browser field, openBrowserTabAction |
| `ui/src/atoms/preview-panel-atoms.test.ts` | EXTEND: browser tab tests |
| `ui/src/components/agent/BrowserPreviewOverlay.tsx` | MODIFY: screencast subscription, compact mode |
| `ui/src/components/preview/BrowserPanel.tsx` | NEW |
| `ui/src/components/preview/BrowserAddressBar.tsx` | NEW |
| `ui/src/components/preview/BrowserTabBar.tsx` | NEW |
| `ui/src/components/preview/BrowserScreencastView.tsx` | NEW |
| `ui/src/components/preview/BrowserDOMOverlay.tsx` | NEW |
| `ui/src/components/preview/BrowserDOMOverlay.test.tsx` | NEW |
| `ui/src/components/preview/BrowserPanel.test.tsx` | NEW |
| `ui/src/components/preview/PreviewContent.tsx` | MODIFY: add `browser` type case |
| `ui/src/hooks/useGlobalAgentListeners.ts` | MODIFY: openBrowserTabAction on first navigate |
| `ui/src/lib/tauri-bridge.ts` | EXTEND: new commands |

---

## 11. Phased Delivery

### Phase 1 — Core backend + 14 tools + per-session isolation

Deliverables:
- `BrowserContextManager` + `BrowserContext` + `DOMState` (Rust)
- All 14 tools with index addressing
- `session_id` plumbed through all tools and Tauri commands
- Deprecate old 6-tool interface
- Cargo builds, `cargo test` passes for new browser/ unit tests

Success criteria: agent can navigate, read DOM, click by index, type by index, scroll, evaluate JS, manage tabs in isolated per-session Chrome.

### Phase 2 — Live screencast + Browser Panel Tab UI

Deliverables:
- `screencast.rs` + `browser_start_screencast` / `browser_stop_screencast` commands
- `BrowserPreviewOverlay.tsx` upgraded to live JPEG stream + compact mode
- `BrowserPanel.tsx` + all sub-components
- `PreviewTabItem` extended with `type: 'browser'`
- `openBrowserTabAction` + `PreviewContent` routing
- `useGlobalAgentListeners` auto-opens browser tab on first navigate
- `browserDOMStateAtom` + `BrowserDOMOverlay` with index badges

Success criteria: agent navigation is visible in real-time in both overlay and panel tab; DOM index badges render over live screen; address bar navigation works.

### Phase 3 — Robustness

Deliverables:
- Loop detection (`loop_detector.rs`)
- Error retry on navigation + element-not-found
- Cookie get/set tools (2 additional tools, total 16)
- Device emulation in `browser_navigate`
- `browser_evaluate` escape hatch fully tested

Success criteria: agent completes multi-step form fill + login on a test site without manual intervention; loop detection fires correctly on a synthetic repeat scenario.

---

## 12. Spec Self-Review

- **Placeholders**: None. All struct fields, CDP method names, tool parameter names, and Tauri command names are fully specified.
- **Internal consistency**: `session_id` is threaded through all layers (Rust → Tauri commands → TS bridge → Jotai atoms). Preview panel tab identity uses `browser:${agentSessionId}` key consistently.
- **Scope**: Focused entirely on browser agent capability. No scope for unrelated automation or MCP changes.
- **Ambiguity**: The one deferred item — exact `PreviewContent.tsx` render switch location — is clearly tagged as "MODIFY" with the change described; no ambiguity in what to change.
- **Coordination**: Explicit references to `preview-panel-multi-tab-design.md` for the PreviewTabItem extension. No duplication of that spec's atom/action design; only additive changes.
- **Supersession**: Old `2026-04-30-phase3-ai-browser-design.md` (Svelte 5 era) is explicitly superseded. Its tool list (15 tools, a11y UID addressing) is replaced by this spec's 14-tool index-addressing design.
