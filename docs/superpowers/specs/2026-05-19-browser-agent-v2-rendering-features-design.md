# Browser Agent v2 — Rendering & Feature Parity Sprint Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make uclaw's browser agent render smoothly (Canvas GPU pipeline, 30 FPS) and match browser-use's open-source feature set (wait, hover, file upload), while adding real-time nav-state feedback to the UI.

**Architecture:** Two parallel improvements — (A) rendering pipeline: replace `<img>` JPEG tag with `<canvas>` + `createImageBitmap` GPU decode and emit `browser:nav-state` events from Rust on every navigation; (B) agent capability: add three missing tools (`browser_wait`, `browser_hover`, `browser_upload_file`) and complete device emulation with touch events + CSS media features. All changes are self-contained to the `browser/` Rust module and four frontend browser components.

**Tech Stack:** Rust/chromiumoxide CDP, React 18 + Jotai atoms, `createImageBitmap` Web API

---

## Background

**Why Canvas instead of `<img>`:** Each base64 JPEG swap on `<img src>` forces the browser to decode JPEG on the main thread and triggers layout+paint. `createImageBitmap(blob)` decodes asynchronously on the GPU compositor thread, then `ctx.drawImage(bitmap)` paints without any layout step.

**Why nav-state events:** Currently the address bar URL only updates when the agent calls `browser_navigate` (via `sessionBrowserPreviewMapAtom`). User-initiated back/forward navigation, page redirects, and SPAs that change the URL silently never update the bar. The back/forward buttons have no disabled state. A `browser:nav-state` Tauri event emitted from Rust on every navigation solves all three.

**browser-use gap analysis:** Three tools present in browser-use but missing from uclaw: `browser_wait` (wait for selector or fixed delay — agent has no way to know when a page finishes loading), `browser_hover` (trigger CSS :hover and JS mouseover — many menus/tooltips require it), `browser_upload_file` (CDP `DOM.setFileInputFiles` — file inputs can't be filled otherwise).

**Cookie tools gap:** `BrowserGetCookiesTool` and `BrowserSetCookieTool` are implemented in `tools.rs` but never registered in `tauri_commands.rs` (both registration blocks at lines 890–904 and 8811–8824). Fix included here.

**Headless mode prerequisite:** chromiumoxide 0.9 defaults to `HeadlessMode::True` (old `--headless` flag). Old headless does NOT support `Page.startScreencast`. Must add `.new_headless_mode()` to `BrowserConfig::builder()` in `context.rs::launch`.

**Overlay positioning:** `BrowserPreviewOverlay` is at `absolute top-3 right-3` which overlaps the scroll area's right edge in `AgentView`. Move to `right-14` (56px) to clear the scrollbar.

---

## File Structure

| File | Action | What changes |
|------|--------|-------------|
| `src-tauri/src/browser/context.rs` | Modify | `.new_headless_mode()`, quality 55, `emit_nav_state()` helper, `Page.loadEventFired` listener in `start_screencast`, complete `apply_device_emulation` |
| `src-tauri/src/browser/types.rs` | Modify | Add `NavStatePayload` struct |
| `src-tauri/src/browser/tools.rs` | Modify | Add `browser_tool!` macros + `Tool` impls for `BrowserWaitTool`, `BrowserHoverTool`, `BrowserUploadFileTool` |
| `src-tauri/src/tauri_commands.rs` | Modify | Register 5 tools: `BrowserGetCookiesTool`, `BrowserSetCookieTool`, `BrowserWaitTool`, `BrowserHoverTool`, `BrowserUploadFileTool` (both registration blocks at lines ~890–904 and ~8811–8824) |
| `ui/src/atoms/browser-atoms.ts` | Modify | Add `NavStateEntry` interface, `browserNavStateAtom` |
| `ui/src/lib/tauri-bridge.ts` | Modify | Add `listenNavState(sessionId, handler)` |
| `ui/src/components/browser/BrowserScreencastView.tsx` | Rewrite | `<canvas>` + `createImageBitmap` pipeline, preserve `BrowserDOMOverlay` |
| `ui/src/components/browser/BrowserAddressBar.tsx` | Modify | Subscribe to `browserNavStateAtom`, loading spinner, disabled back/forward, screencast restart on URL change |
| `ui/src/components/agent/BrowserPreviewOverlay.tsx` | Modify | Replace `<img>` with `<canvas>`, `right-14` position |

---

## Task 1 — Headless Mode Fix + 30 FPS Screencast

**Files:** Modify `src-tauri/src/browser/context.rs`

Fix the chromiumoxide headless mode (prerequisite for ALL screencast features) and increase frame rate.

**Change 1 — `launch()` at line ~91:**
```rust
let config = BrowserConfig::builder()
    .new_headless_mode()   // add this line — required for Page.startScreencast
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
```

**Change 2 — `start_screencast()` at line ~459:** Change `quality(60_i64)` to `quality(55_i64)`.

Verify: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Commit: `fix(browser): new headless mode + 55% quality for 30 FPS screencast`

---

## Task 2 — NavStatePayload Type + listenNavState Bridge

**Files:** Modify `src-tauri/src/browser/types.rs`, `ui/src/lib/tauri-bridge.ts`, `ui/src/atoms/browser-atoms.ts`

### 2a — Rust type in `types.rs`

Add after existing structs:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NavStatePayload {
    pub session_id: String,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}
```

Add to `use` imports in `context.rs`:
```rust
use crate::browser::types::{DOMState, DomStateRaw, NavStatePayload, ScreencastFramePayload, TabInfo};
```

### 2b — `emit_nav_state` helper in `context.rs`

Add private method to `impl BrowserContext`:
```rust
async fn emit_nav_state(
    &self,
    tab_id: &str,
    page: &Page,
    app_handle: &tauri::AppHandle,
    is_loading: bool,
) {
    let url = page
        .evaluate("window.location.href")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();
    let title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();
    // history.length > 1 is a proxy for can_go_back; not perfect but reliable
    // for typical agent-driven browsing flows.
    let history_len = page
        .evaluate("history.length")
        .await
        .ok()
        .and_then(|v| v.into_value::<i64>().ok())
        .unwrap_or(1);
    let payload = NavStatePayload {
        session_id: self.session_id.clone(),
        tab_id: tab_id.to_string(),
        url,
        title,
        is_loading,
        can_go_back: history_len > 1,
        can_go_forward: false, // always false — forward detection requires tracking history index, YAGNI
    };
    let _ = app_handle.emit("browser:nav-state", &payload);
}
```

### 2c — Frontend atom in `browser-atoms.ts`

```typescript
export interface NavStateEntry {
  tabId: string
  url: string
  title: string
  isLoading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

/** Latest nav state per sessionId. Updated on every browser:nav-state event. */
export const browserNavStateAtom = atom(new Map<string, NavStateEntry>())
```

### 2d — `listenNavState` in `tauri-bridge.ts`

Add near `listenScreencastFrames`:
```typescript
export interface NavStatePayload {
  sessionId: string
  tabId: string
  url: string
  title: string
  isLoading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

export const listenNavState = (
  handler: (payload: NavStatePayload) => void
): Promise<UnlistenFn> =>
  listen<NavStatePayload>('browser:nav-state', (e) => handler(e.payload))
```

Verify: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Commit: `feat(browser): NavStatePayload type + browserNavStateAtom + listenNavState`

---

## Task 3 — Emit Nav-State from Rust Navigation Commands

**Files:** Modify `src-tauri/src/browser/context.rs`

Wire `emit_nav_state` into navigation methods. This requires passing `app_handle` into navigation calls, which means `context.rs` needs to store a reference or methods need to accept it as a parameter.

**Design decision:** Pass `app_handle: Option<&tauri::AppHandle>` to each navigation method. Callers in `tools.rs` have `ctx_mgr` which has `app_handle()`. This avoids storing `AppHandle` in `BrowserContext` (avoids lifetime complexity).

**Change to `navigate()` signature:**
```rust
pub async fn navigate(
    &self,
    tab_id: &str,
    url: &str,
    app_handle: Option<&tauri::AppHandle>,
) -> Result<String> {
```
After existing navigate logic, at the end (before `Ok(new_id)`):
```rust
if let Some(ah) = app_handle {
    let resolved_page = self.get_page(&new_id).await?;
    self.emit_nav_state(&new_id, &resolved_page, ah, false).await;
}
```

**Change to `go_back()` and `go_forward()` signatures:**
```rust
pub async fn go_back(&self, tab_id: &str, app_handle: Option<&tauri::AppHandle>) -> Result<()>
pub async fn go_forward(&self, tab_id: &str, app_handle: Option<&tauri::AppHandle>) -> Result<()>
pub async fn reload(&self, tab_id: &str, app_handle: Option<&tauri::AppHandle>) -> Result<()>
```
After the JS call in each, add:
```rust
if let Some(ah) = app_handle {
    // brief yield for history to update
    tokio::time::sleep(Duration::from_millis(100)).await;
    let page = self.get_page(tab_id).await?;
    self.emit_nav_state(tab_id, &page, ah, false).await;
}
```

**Also emit `is_loading: true` at the START of navigate:**
Insert before the `page.goto(url)` call:
```rust
if let Some(ah) = app_handle {
    let _ = ah.emit("browser:nav-state", NavStatePayload {
        session_id: self.session_id.clone(),
        tab_id: tab_id.to_string(),
        url: url.to_string(),
        title: String::new(),
        is_loading: true,
        can_go_back: false,
        can_go_forward: false,
    });
}
```

**Update callers in `tools.rs`:**

In `BrowserNavigateTool::call`, the `ctx.navigate(tab_id, url)` call becomes:
```rust
let app_handle = &self.ctx_mgr.app_handle();
let resolved = ctx.navigate(tab_id, url, Some(app_handle)).await?;
```
Similarly for go_back, go_forward, reload tools.

**Tauri UI commands in `tauri_commands.rs`:** The `browser_ui_navigate`, `browser_ui_go_back`, etc. commands also call these methods. Update them to pass `Some(&app_handle)`.

Verify: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Commit: `feat(browser): emit browser:nav-state on all navigation commands`

---

## Task 4 — BrowserAddressBar Nav-State Integration

**Files:** Modify `ui/src/components/browser/BrowserAddressBar.tsx`

Replace the current static `url` prop-driven state with live `browserNavStateAtom` subscription. Also restart screencast after manual navigation.

```typescript
import * as React from 'react'
import { ArrowLeft, ArrowRight, RefreshCw, Globe } from 'lucide-react'
import { useAtomValue, useSetAtom } from 'jotai'
import { cn } from '@/lib/utils'
import {
  browserUIGoBack,
  browserUIGoForward,
  browserUIReload,
  browserUINavigate,
  browserStartScreencast,
} from '@/lib/tauri-bridge'
import { browserNavStateAtom } from '@/atoms/browser-atoms'

interface BrowserAddressBarProps {
  sessionId: string
  tabId: string | null
  url: string  // fallback when nav-state not yet available
}

export function BrowserAddressBar({ sessionId, tabId, url }: BrowserAddressBarProps): React.ReactElement {
  const navStateMap = useAtomValue(browserNavStateAtom)
  const navState = navStateMap.get(sessionId)

  const liveUrl = navState?.url || url
  const isLoading = navState?.isLoading ?? false
  const canGoBack = navState?.canGoBack ?? !!tabId
  const canGoForward = navState?.canGoForward ?? false

  const [draft, setDraft] = React.useState(liveUrl)
  const [focused, setFocused] = React.useState(false)

  // Sync address bar when URL changes from nav events (only when not focused)
  React.useEffect(() => {
    if (!focused) setDraft(liveUrl)
  }, [liveUrl, focused])

  const navigate = () => {
    if (!tabId) return
    let target = draft.trim()
    if (target && !target.includes('://')) target = 'https://' + target
    browserUINavigate(sessionId, tabId, target)
      .then(() => browserStartScreencast(sessionId, tabId!))
      .catch(console.error)
  }

  return (
    <div className="flex items-center gap-1 px-2 py-1.5 border-b border-border/50 bg-muted/20">
      <button
        onClick={() => tabId && browserUIGoBack(sessionId, tabId).catch(console.error)}
        disabled={!canGoBack}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="后退"
      >
        <ArrowLeft size={13} />
      </button>
      <button
        onClick={() => tabId && browserUIGoForward(sessionId, tabId).catch(console.error)}
        disabled={!canGoForward}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="前进"
      >
        <ArrowRight size={13} />
      </button>
      <button
        onClick={() => tabId && browserUIReload(sessionId, tabId)
          .then(() => browserStartScreencast(sessionId, tabId!))
          .catch(console.error)}
        disabled={!tabId}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="刷新"
      >
        <RefreshCw size={13} className={cn(isLoading && 'animate-spin')} />
      </button>

      <div className="flex flex-1 items-center gap-1.5 bg-popover border border-border/60 rounded-md px-2 h-7">
        <Globe size={11} className="text-muted-foreground shrink-0" />
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') navigate() }}
          onFocus={() => setFocused(true)}
          onBlur={() => { setFocused(false); setDraft(liveUrl) }}
          className="flex-1 bg-transparent text-[12px] outline-none text-foreground placeholder:text-muted-foreground min-w-0"
          placeholder="输入网址…"
          spellCheck={false}
        />
      </div>
    </div>
  )
}
```

**Also: wire `listenNavState` in `BrowserPanel.tsx`:** Add a `useEffect` in `BrowserPanel` that subscribes to `listenNavState` for this session and updates `browserNavStateAtom`.

```typescript
// In BrowserPanel.tsx, add:
import { listenNavState } from '@/lib/tauri-bridge'
import { browserNavStateAtom, type NavStateEntry } from '@/atoms/browser-atoms'

// In the component body:
const setNavState = useSetAtom(browserNavStateAtom)

React.useEffect(() => {
  let unlisten: (() => void) | null = null
  listenNavState((payload) => {
    if (payload.sessionId !== agentSessionId) return
    setNavState((prev) => {
      const next = new Map(prev)
      next.set(agentSessionId, {
        tabId: payload.tabId,
        url: payload.url,
        title: payload.title,
        isLoading: payload.isLoading,
        canGoBack: payload.canGoBack,
        canGoForward: payload.canGoForward,
      })
      return next
    })
  }).then((fn) => { unlisten = fn })
  return () => { if (unlisten) unlisten() }
}, [agentSessionId, setNavState])
```

Verify: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Commit: `feat(browser): BrowserAddressBar reacts to nav-state (URL, loading, back/forward)`

---

## Task 5 — Canvas Renderer for BrowserScreencastView + BrowserPreviewOverlay

**Files:** Rewrite `ui/src/components/browser/BrowserScreencastView.tsx`, modify `ui/src/components/agent/BrowserPreviewOverlay.tsx`

### 5a — BrowserScreencastView canvas rewrite

Replace `<img>` with `<canvas>` + `createImageBitmap`. Preserve the `BrowserDOMOverlay` layer.

```typescript
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MonitorPlay } from 'lucide-react'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserDOMOverlayVisibleAtom,
  type ScreencastFrameEntry,
} from '@/atoms/browser-atoms'
import { BrowserDOMOverlay } from './BrowserDOMOverlay'

interface BrowserScreencastViewProps {
  sessionId: string
}

export function BrowserScreencastView({ sessionId }: BrowserScreencastViewProps): React.ReactElement {
  const frameMap = useAtomValue(browserScreencastFrameAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)

  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const [displaySize, setDisplaySize] = React.useState({ w: 0, h: 0 })
  // Track last frame dimensions to avoid unnecessary canvas resize
  const lastDimsRef = React.useRef({ w: 0, h: 0 })

  const frame = frameMap.get(sessionId)
  const domEntry = domMap.get(sessionId)

  // ResizeObserver for DOM overlay coordinate scaling
  React.useLayoutEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const obs = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect
      setDisplaySize({ w: width, h: height })
    })
    obs.observe(canvas)
    return () => obs.disconnect()
  }, [])

  // Decode and draw each new frame via GPU-accelerated createImageBitmap
  React.useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !frame) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    // Convert base64 to Blob for createImageBitmap
    const binary = atob(frame.dataB64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    const blob = new Blob([bytes], { type: 'image/jpeg' })

    let cancelled = false
    createImageBitmap(blob).then((bitmap) => {
      if (cancelled) { bitmap.close(); return }

      // Only resize canvas when frame dimensions change (avoids clearing)
      if (lastDimsRef.current.w !== bitmap.width || lastDimsRef.current.h !== bitmap.height) {
        canvas.width = bitmap.width
        canvas.height = bitmap.height
        lastDimsRef.current = { w: bitmap.width, h: bitmap.height }
      }

      ctx.drawImage(bitmap, 0, 0)
      bitmap.close()
    }).catch(() => {/* ignore decode errors on stale frames */})

    return () => { cancelled = true }
  }, [frame])

  if (!frame) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground bg-muted/10">
        <MonitorPlay size={36} className="opacity-20 mb-2" />
        <span className="text-sm opacity-40">等待浏览器画面...</span>
      </div>
    )
  }

  return (
    <div className="flex-1 relative overflow-hidden bg-black">
      <canvas
        ref={canvasRef}
        className="w-full h-full object-contain"
        style={{ display: 'block' }}
      />
      {overlayVisible && domEntry && displaySize.w > 0 && (
        <BrowserDOMOverlay
          elements={domEntry.elements}
          pageWidth={frame.pageWidth}
          pageHeight={frame.pageHeight}
          displayWidth={displaySize.w}
          displayHeight={displaySize.h}
        />
      )}
    </div>
  )
}
```

### 5b — BrowserPreviewOverlay canvas + reposition

In `BrowserPreviewOverlay.tsx`:
1. Replace `'absolute top-3 right-3 z-20'` → `'absolute top-3 right-14 z-20'`
2. Replace the `<img>` inside the screencast section with a canvas implementation:

```typescript
// Add refs at the top of the component:
const canvasRef = React.useRef<HTMLCanvasElement>(null)
const lastDimsRef = React.useRef({ w: 0, h: 0 })

// Add frame-draw effect (same pattern as BrowserScreencastView):
React.useEffect(() => {
  const canvas = canvasRef.current
  if (!canvas || !frame) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const binary = atob(frame.dataB64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  const blob = new Blob([bytes], { type: 'image/jpeg' })
  let cancelled = false
  createImageBitmap(blob).then((bitmap) => {
    if (cancelled) { bitmap.close(); return }
    if (lastDimsRef.current.w !== bitmap.width || lastDimsRef.current.h !== bitmap.height) {
      canvas.width = bitmap.width
      canvas.height = bitmap.height
      lastDimsRef.current = { w: bitmap.width, h: bitmap.height }
    }
    ctx.drawImage(bitmap, 0, 0)
    bitmap.close()
  }).catch(() => {})
  return () => { cancelled = true }
}, [frame])

// Remove imageSrc computation. Replace the <img> JSX:
{!isCollapsed && (
  <div className="relative bg-muted/20" style={{ aspectRatio: '16/10' }}>
    {frame ? (
      <canvas
        ref={canvasRef}
        className="w-full h-full object-cover object-top"
        style={{ display: 'block' }}
      />
    ) : (
      <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
        <MonitorPlay size={22} className="opacity-30" />
        <span className="text-[11px] opacity-50">等待画面...</span>
      </div>
    )}
  </div>
)}
```

Verify: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Commit: `feat(browser): canvas+createImageBitmap renderer + overlay reposition right-14`

---

## Task 6 — Complete Device Emulation (Touch + CSS Media)

**Files:** Modify `src-tauri/src/browser/context.rs`

Extend `apply_device_emulation` to add the full hello-halo `applyDeviceMode` stack: touch events and CSS media features.

The existing method at line ~575 ends after `SetDeviceMetricsOverride` + `SetUserAgentOverride`. Add:

```rust
pub async fn apply_device_emulation(&self, tab_id: &str, device: DevicePreset) -> Result<()> {
    let page = self.get_page(tab_id).await?;
    use chromiumoxide::cdp::browser_protocol::emulation::{
        SetDeviceMetricsOverrideParams,
        SetEmulatedMediaParams,
        SetTouchEmulationEnabledParams,
        SetUserAgentOverrideParams,
        MediaFeature,
    };

    // 1. Viewport + mobile flag (existing)
    let metrics = SetDeviceMetricsOverrideParams { /* existing fields */ };
    page.execute(metrics).await.map_err(|e| anyhow!("device metrics: {e}"))?;

    // 2. User-agent (existing)
    let ua = SetUserAgentOverrideParams {
        user_agent: device.user_agent().to_string(),
        ..Default::default()
    };
    page.execute(ua).await.map_err(|e| anyhow!("user-agent: {e}"))?;

    // 3. Touch emulation (NEW)
    let is_mobile = device == DevicePreset::Mobile;
    let touch = SetTouchEmulationEnabledParams {
        enabled: is_mobile,
        max_touch_points: Some(if is_mobile { 5 } else { 0 }),
    };
    page.execute(touch).await.map_err(|e| anyhow!("touch emulation: {e}"))?;

    // 4. CSS media features (NEW) — hover:none + pointer:coarse for mobile
    let features = if is_mobile {
        vec![
            MediaFeature { name: "hover".to_string(), value: "none".to_string() },
            MediaFeature { name: "pointer".to_string(), value: "coarse".to_string() },
        ]
    } else {
        vec![
            MediaFeature { name: "hover".to_string(), value: "hover".to_string() },
            MediaFeature { name: "pointer".to_string(), value: "fine".to_string() },
        ]
    };
    page.execute(SetEmulatedMediaParams { media: None, features: Some(features), vision_deficiency: None })
        .await
        .map_err(|e| anyhow!("emulated media: {e}"))?;

    Ok(())
}
```

Note: If `SetEmulatedMediaParams` / `MediaFeature` are not exposed as typed structs in chromiumoxide 0.9, fall back to a generic CDP call via `page.execute_cdp("Emulation.setEmulatedMedia", json!({ "features": [...] }))` using the `serde_json::json!` macro.

Verify: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Commit: `feat(browser): complete device emulation — touch events + CSS media features`

---

## Task 7 — `browser_wait` Tool

**Files:** Modify `src-tauri/src/browser/tools.rs`, `src-tauri/src/tauri_commands.rs`

### 7a — Add to tools.rs

```rust
browser_tool!(BrowserWaitTool);

#[async_trait]
impl Tool for BrowserWaitTool {
    fn name(&self) -> &str { "browser_wait" }

    fn description(&self) -> &str {
        "Wait for a CSS selector to appear in the DOM, or wait a fixed duration.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID from a previous browser_navigate call.\n\
         - `selector` (string, optional): CSS selector to wait for (e.g. '#main', '.loaded').\n\
         - `timeout_ms` (number, optional): Maximum wait in ms (default 10000).\n\
         \n\
         Use this after browser_navigate or browser_click when the page needs time to load."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"].as_str().ok_or_else(|| ToolError::invalid("tab_id required"))?;
        let selector = params["selector"].as_str();
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::execution(e.to_string()))?;

        let start = Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        if let Some(sel) = selector {
            let escaped = sel.replace('\\', "\\\\").replace('"', "\\\"");
            loop {
                if start.elapsed() >= timeout {
                    return Err(ToolError::execution(format!(
                        "Timeout: selector '{}' not found after {}ms", sel, timeout_ms
                    )));
                }
                let found = ctx.execute_js(
                    tab_id,
                    &format!("!!document.querySelector(\"{}\")", escaped),
                ).await.map_err(|e| ToolError::execution(e.to_string()))?;
                if found.trim() == "true" {
                    let elapsed = start.elapsed().as_millis();
                    return Ok(ToolOutput::success(
                        format!("Element '{}' found after {}ms", sel, elapsed),
                        elapsed as u64,
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } else {
            tokio::time::sleep(timeout).await;
            return Ok(ToolOutput::success(
                format!("Waited {}ms", timeout_ms),
                timeout_ms,
            ));
        }
    }
}
```

### 7b — Register in tauri_commands.rs

In **both** registration blocks (lines ~890–904 and ~8811–8824), add:
```rust
tools.register(bt!(BrowserGetCookiesTool));
tools.register(bt!(BrowserSetCookieTool));
tools.register(bt!(BrowserWaitTool));
```

Also add to the `use` import at the top of the browser tools block:
```rust
use crate::browser::tools::{
    // ... existing ...
    BrowserGetCookiesTool, BrowserSetCookieTool, BrowserWaitTool,
};
```

Verify: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Commit: `feat(browser): browser_wait tool + register cookie tools (both dispatcher blocks)`

---

## Task 8 — `browser_hover` and `browser_upload_file` Tools

**Files:** Modify `src-tauri/src/browser/tools.rs`, `src-tauri/src/tauri_commands.rs`

### 8a — `browser_hover` in tools.rs

```rust
browser_tool!(BrowserHoverTool);

#[async_trait]
impl Tool for BrowserHoverTool {
    fn name(&self) -> &str { "browser_hover" }

    fn description(&self) -> &str {
        "Move the mouse over an element to trigger hover effects (CSS :hover, JS mouseover events).\n\
         Required for tooltips, dropdown menus, and reveal-on-hover UI patterns.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Element index from browser_get_dom."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"].as_str().ok_or_else(|| ToolError::invalid("tab_id required"))?;
        let index = params["index"].as_u64().ok_or_else(|| ToolError::invalid("index required"))? as u32;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::execution(e.to_string()))?;

        // Get element bounding box, then dispatch CDP mouse move + JS events.
        let script = format!(
            r#"
            (function() {{
                const el = document.querySelector('[data-uclaw-index="{}"]');
                if (!el) return null;
                const r = el.getBoundingClientRect();
                const x = r.left + r.width / 2;
                const y = r.top + r.height / 2;
                // Dispatch JS mouse events (triggers JS listeners + some CSS)
                el.dispatchEvent(new MouseEvent('mouseenter', {{ bubbles: false, cancelable: true, clientX: x, clientY: y }}));
                el.dispatchEvent(new MouseEvent('mouseover', {{ bubbles: true, cancelable: true, clientX: x, clientY: y }}));
                el.dispatchEvent(new MouseEvent('mousemove', {{ bubbles: true, cancelable: true, clientX: x, clientY: y }}));
                return {{ x: Math.round(x), y: Math.round(y) }};
            }})()
            "#,
            index
        );

        let result = ctx.execute_js(tab_id, &script).await
            .map_err(|e| ToolError::execution(e.to_string()))?;

        if result.trim() == "null" {
            return Err(ToolError::execution(format!("Element with index {} not found", index)));
        }

        // Also send a CDP Input.dispatchMouseEvent to activate CSS :hover
        let coords_script = format!(
            "JSON.stringify((function(){{ const el = document.querySelector('[data-uclaw-index=\"{}\"]'); \
             if (!el) return null; const r = el.getBoundingClientRect(); \
             return {{ x: r.left + r.width/2, y: r.top + r.height/2 }}; }}()))",
            index
        );
        let coords_json = ctx.execute_js(tab_id, &coords_script).await
            .map_err(|e| ToolError::execution(e.to_string()))?;

        if coords_json.trim() != "null" {
            if let Ok(coords) = serde_json::from_str::<serde_json::Value>(&coords_json) {
                let x = coords["x"].as_f64().unwrap_or(0.0);
                let y = coords["y"].as_f64().unwrap_or(0.0);
                use chromiumoxide::cdp::browser_protocol::input::{
                    DispatchMouseEventParams, DispatchMouseEventType,
                };
                let page = ctx.pages.read().await.get(tab_id).cloned();
                if let Some(page) = page {
                    let _ = page.execute(
                        DispatchMouseEventParams::builder()
                            .r#type(DispatchMouseEventType::MouseMoved)
                            .x(x)
                            .y(y)
                            .build()
                    ).await;
                }
            }
        }

        Ok(ToolOutput::success(format!("Hovered element at index {}", index), 0))
    }
}
```

### 8b — `browser_upload_file` in tools.rs

```rust
browser_tool!(BrowserUploadFileTool);

#[async_trait]
impl Tool for BrowserUploadFileTool {
    fn name(&self) -> &str { "browser_upload_file" }

    fn description(&self) -> &str {
        "Set a file on a file input element (<input type='file'>). \
         The file must exist in the agent workspace (~/Documents/workground/).\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Index of the file input element from browser_get_dom.\n\
         - `file_path` (string, required): Relative path from ~/Documents/workground/ \
           (e.g. 'report.pdf' or 'images/photo.jpg')."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"].as_str().ok_or_else(|| ToolError::invalid("tab_id required"))?;
        let index = params["index"].as_u64().ok_or_else(|| ToolError::invalid("index required"))? as u32;
        let file_path = params["file_path"].as_str().ok_or_else(|| ToolError::invalid("file_path required"))?;

        // Resolve to absolute path under workspace root
        let workspace_root = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("Documents/workground");
        let abs_path = workspace_root.join(file_path);

        if !abs_path.exists() {
            return Err(ToolError::execution(format!(
                "File not found: {} (resolved to {})", file_path, abs_path.display()
            )));
        }

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::execution(e.to_string()))?;

        // Use CDP DOM.setFileInputFiles via JS evaluation approach:
        // chromiumoxide doesn't expose DOM.setFileInputFiles directly, so we use
        // page.find_element + set_input_files if available, or fall back to CDP command.
        let abs_path_str = abs_path.to_string_lossy().to_string();
        let page = ctx.pages.read().await.get(tab_id).cloned()
            .ok_or_else(|| ToolError::execution(format!("tab_id {} not found", tab_id)))?;

        // Try chromiumoxide's Element::set_input_files
        let selector = format!("[data-uclaw-index=\"{}\"]", index);
        match page.find_element(selector.clone()).await {
            Ok(el) => {
                el.set_input_files(vec![abs_path_str.clone()]).await
                    .map_err(|e| ToolError::execution(format!("set_input_files failed: {e}")))?;
            }
            Err(_) => {
                return Err(ToolError::execution(format!("Element with index {} not found", index)));
            }
        }

        Ok(ToolOutput::success(
            format!("File '{}' set on element at index {}", file_path, index),
            0,
        ))
    }
}
```

### 8c — Register new tools in tauri_commands.rs

In **both** registration blocks, add:
```rust
tools.register(bt!(BrowserHoverTool));
tools.register(bt!(BrowserUploadFileTool));
```

Verify: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Commit: `feat(browser): browser_hover + browser_upload_file tools`

---

## Testing

### Manual verification checklist (run after `cargo tauri dev`)

1. **Screencast works:** Ask agent "打开 apple.com"  → BrowserPanel opens → live canvas frames appear within 3s
2. **30 FPS:** Scroll the page in address bar → motion should feel smooth (~30 FPS JPEG)
3. **Address bar live:** While agent navigates between pages, address bar should update URL without user interaction
4. **Loading spinner:** Address bar RefreshCw spins during page load
5. **Back button state:** After navigating to a page from about:blank, back button becomes enabled
6. **Reload + canvas:** Click reload → frames continue (screencast restarts)
7. **BrowserPreviewOverlay:** Close button (X) not overlapping scroll minimap on right edge
8. **Mobile emulation:** `browser_navigate url="..." device="mobile"` → DevTools shows 390px viewport + touch events
9. **browser_wait:** Agent uses `browser_wait tab_id=... selector=".main"` → waits for element
10. **browser_hover:** Agent uses `browser_hover index=...` → CSS hover effects trigger
11. **browser_upload_file:** Agent places a file in workground, calls `browser_upload_file` → file input populated

### Unit tests (Rust)

- `browser_wait` with selector found: mock DOM returns true immediately
- `browser_wait` timeout: mock DOM never returns true, verify timeout error
- `apply_device_emulation` mobile: verify `SetTouchEmulationEnabled` with `enabled=true, maxTouchPoints=5`
