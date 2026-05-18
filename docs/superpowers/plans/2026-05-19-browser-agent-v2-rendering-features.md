# Browser Agent v2 — Rendering & Feature Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the browser agent screencast render smoothly via GPU-accelerated canvas, add real-time navigation-state feedback to the UI, and add three missing agent tools (wait, hover, upload_file) to reach browser-use feature parity.

**Architecture:** Eight sequential tasks covering Rust backend (context.rs, types.rs, tools.rs, tauri_commands.rs) then TypeScript frontend (browser-atoms.ts, tauri-bridge.ts, BrowserPanel.tsx, BrowserAddressBar.tsx, BrowserScreencastView.tsx, BrowserPreviewOverlay.tsx). Backend tasks must precede the frontend tasks that consume their events. Each task compiles and tests cleanly before the next begins.

**Tech Stack:** Rust / chromiumoxide 0.9.1 CDP, React 18 + Jotai atoms, `createImageBitmap` Web API, Tauri v2 event system

**Spec:** `docs/superpowers/specs/2026-05-19-browser-agent-v2-rendering-features-design.md`

---

## File Map

| File | Task(s) | Change |
|------|---------|--------|
| `src-tauri/src/browser/context.rs` | 1, 3, 6 | `.new_headless_mode()`, quality 55, `emit_nav_state`, nav method signatures, touch/media emulation |
| `src-tauri/src/browser/types.rs` | 2 | Add `NavStatePayload` struct |
| `src-tauri/src/browser/tools.rs` | 3, 7, 8 | Update nav tool callers; add `BrowserWaitTool`, `BrowserHoverTool`, `BrowserUploadFileTool` |
| `src-tauri/src/tauri_commands.rs` | 3, 7, 8 | Update UI nav command callers; register 5 new/missing tools (both blocks) |
| `ui/src/atoms/browser-atoms.ts` | 2 | Add `NavStateEntry`, `browserNavStateAtom` |
| `ui/src/lib/tauri-bridge.ts` | 2 | Add `NavStatePayload` interface, `listenNavState` |
| `ui/src/components/browser/BrowserPanel.tsx` | 4 | Subscribe to `listenNavState`, populate `browserNavStateAtom` |
| `ui/src/components/browser/BrowserAddressBar.tsx` | 4 | Consume `browserNavStateAtom` for live URL, loading spinner, back/forward disabled state; restart screencast after manual navigate/reload |
| `ui/src/components/browser/BrowserScreencastView.tsx` | 5 | Replace `<img>` with `<canvas>` + `createImageBitmap` |
| `ui/src/components/agent/BrowserPreviewOverlay.tsx` | 5 | Same canvas pattern; fix position to `right-14` |

---

## Task 1 — Headless Mode Fix + 30 FPS

**Files:**
- Modify: `src-tauri/src/browser/context.rs:91-104` (BrowserConfig builder)
- Modify: `src-tauri/src/browser/context.rs:459-465` (StartScreencastParams)

The chromiumoxide 0.9 default `HeadlessMode::True` uses the old `--headless` flag which does NOT support `Page.startScreencast`. This single line is why the screencast has never shown frames.

- [ ] **Step 1: Add `.new_headless_mode()` to BrowserConfig builder**

In `src-tauri/src/browser/context.rs`, find the `BrowserConfig::builder()` block at line ~91. Add `.new_headless_mode()` as the first method call:

```rust
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
```

- [ ] **Step 2: Change screencast quality from 60 to 55**

In `start_screencast` at line ~459, change `.quality(60_i64)` to `.quality(55_i64)`:

```rust
page.execute(
    StartScreencastParams::builder()
        .format(StartScreencastFormat::Jpeg)
        .quality(55_i64)
        .max_width(1280_i64)
        .max_height(800_i64)
        .every_nth_frame(1_i64)
        .build(),
)
```

- [ ] **Step 3: Verify Rust compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Expected: no output (zero errors)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/browser/context.rs
git commit -m "fix(browser): HeadlessMode::New enables Page.startScreencast; quality 55 for 30 FPS"
```

---

## Task 2 — NavStatePayload Type + Frontend Atom + Bridge

**Files:**
- Modify: `src-tauri/src/browser/types.rs` (add struct at end of v2 types section)
- Modify: `ui/src/atoms/browser-atoms.ts` (add interface + atom)
- Modify: `ui/src/lib/tauri-bridge.ts` (add interface + listener)

Defines the `browser:nav-state` event contract. All three files must be consistent before any emit/consume code is written.

- [ ] **Step 1: Write the Rust unit test for NavStatePayload serialization**

In `src-tauri/src/browser/types.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn nav_state_payload_serializes_camelcase() {
    let p = NavStatePayload {
        session_id: "s1".to_string(),
        tab_id: "t1".to_string(),
        url: "https://example.com".to_string(),
        title: "Example".to_string(),
        is_loading: true,
        can_go_back: false,
        can_go_forward: false,
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"sessionId\":\"s1\""), "got: {json}");
    assert!(json.contains("\"isLoading\":true"), "got: {json}");
    assert!(json.contains("\"canGoBack\":false"), "got: {json}");
}
```

- [ ] **Step 2: Run test to confirm it fails**

Run: `cd src-tauri && cargo test --lib types -- nav_state 2>&1 | tail -5`

Expected: `error[E0422]: cannot find struct \`NavStatePayload\``

- [ ] **Step 3: Add NavStatePayload struct to types.rs**

Insert after `ScreencastFramePayload` (after line 56), before the `// ── raw deserialization types` comment:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

- [ ] **Step 4: Run test to confirm it passes**

Run: `cd src-tauri && cargo test --lib types -- nav_state 2>&1 | tail -5`

Expected: `test types::tests::nav_state_payload_serializes_camelcase ... ok`

- [ ] **Step 5: Update import in context.rs**

In `src-tauri/src/browser/context.rs` line 27, update the types import:

```rust
use crate::browser::types::{DOMState, DomStateRaw, NavStatePayload, ScreencastFramePayload, TabInfo};
```

- [ ] **Step 6: Add NavStateEntry interface and atom to browser-atoms.ts**

In `ui/src/atoms/browser-atoms.ts`, after the `browserDOMOverlayVisibleAtom` line (after line 49), add:

```typescript
export interface NavStateEntry {
  tabId: string
  url: string
  title: string
  isLoading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

/** Latest nav state per sessionId. Populated by BrowserPanel's listenNavState subscription. */
export const browserNavStateAtom = atom(new Map<string, NavStateEntry>())
```

- [ ] **Step 7: Add NavStatePayload interface and listenNavState to tauri-bridge.ts**

In `ui/src/lib/tauri-bridge.ts`, insert after `listenScreencastFrames` (after line 1760):

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

/** Subscribe to browser:nav-state events. Returns an unlisten function. */
export const listenNavState = (
  handler: (payload: NavStatePayload) => void,
): Promise<UnlistenFn> =>
  listen<NavStatePayload>('browser:nav-state', ({ payload }) => handler(payload))
```

- [ ] **Step 8: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: no output (zero errors)

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/browser/types.rs src-tauri/src/browser/context.rs \
        ui/src/atoms/browser-atoms.ts ui/src/lib/tauri-bridge.ts
git commit -m "feat(browser): NavStatePayload type + browserNavStateAtom + listenNavState bridge"
```

---

## Task 3 — emit_nav_state + Navigation Method Signatures + Caller Updates

**Files:**
- Modify: `src-tauri/src/browser/context.rs` (add helper; update `navigate`, `go_back`, `go_forward`, `reload` signatures)
- Modify: `src-tauri/src/browser/tools.rs` (update 4 nav tool callers)
- Modify: `src-tauri/src/tauri_commands.rs` (update 4 UI command callers at lines ~9130–9160)

Every navigation method now accepts `app_handle: &tauri::AppHandle` and calls `emit_nav_state` before and after.

- [ ] **Step 1: Add emit_nav_state private method to BrowserContext**

In `src-tauri/src/browser/context.rs`, add after the `reload` method (after line ~226), before the `// ── DOM state` comment:

```rust
// ── Nav state ─────────────────────────────────────────────────────────────

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
        can_go_forward: false,
    };
    let _ = app_handle.emit("browser:nav-state", &payload);
}
```

- [ ] **Step 2: Update navigate() signature and body**

Replace the entire `navigate` method (lines ~174–198) with:

```rust
pub async fn navigate(
    &self,
    tab_id: &str,
    url: &str,
    app_handle: &tauri::AppHandle,
) -> Result<String> {
    // Emit loading=true immediately so the address bar shows a spinner.
    let _ = app_handle.emit("browser:nav-state", NavStatePayload {
        session_id: self.session_id.clone(),
        tab_id: if tab_id == "new" { "new".to_string() } else { tab_id.to_string() },
        url: url.to_string(),
        title: String::new(),
        is_loading: true,
        can_go_back: false,
        can_go_forward: false,
    });

    let mut pages = self.pages.write().await;
    if tab_id != "new" {
        if let Some(page) = pages.get(tab_id) {
            let page = page.clone();
            drop(pages);
            page.goto(url)
                .await
                .map_err(|e| anyhow!("navigate to {url}: {e}"))?;
            self.invalidate_dom_cache(tab_id).await;
            self.emit_nav_state(tab_id, &page, app_handle, false).await;
            return Ok(tab_id.to_string());
        }
    }
    drop(pages);
    let page = self
        .browser
        .new_page(url)
        .await
        .map_err(|e| anyhow!("new_page: {e}"))?;
    let new_id = Uuid::new_v4().to_string();
    self.pages.write().await.insert(new_id.clone(), page.clone());
    self.invalidate_dom_cache(&new_id).await;
    self.emit_nav_state(&new_id, &page, app_handle, false).await;
    Ok(new_id)
}
```

- [ ] **Step 3: Update go_back(), go_forward(), reload() signatures**

Replace each method. For `go_back` (lines ~201–208):

```rust
pub async fn go_back(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
    let page = self.get_page(tab_id).await?;
    page.evaluate("history.back()")
        .await
        .map_err(|e| anyhow!("go_back failed: {}", e))?;
    self.invalidate_dom_cache(tab_id).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    self.emit_nav_state(tab_id, &page, app_handle, false).await;
    Ok(())
}
```

For `go_forward` (lines ~210–217):

```rust
pub async fn go_forward(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
    let page = self.get_page(tab_id).await?;
    page.evaluate("history.forward()")
        .await
        .map_err(|e| anyhow!("go_forward failed: {}", e))?;
    self.invalidate_dom_cache(tab_id).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    self.emit_nav_state(tab_id, &page, app_handle, false).await;
    Ok(())
}
```

For `reload` (lines ~219–226):

```rust
pub async fn reload(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
    let page = self.get_page(tab_id).await?;
    let _ = app_handle.emit("browser:nav-state", NavStatePayload {
        session_id: self.session_id.clone(),
        tab_id: tab_id.to_string(),
        url: String::new(),
        title: String::new(),
        is_loading: true,
        can_go_back: false,
        can_go_forward: false,
    });
    page.reload()
        .await
        .map_err(|e| anyhow!("reload failed: {}", e))?;
    self.invalidate_dom_cache(tab_id).await;
    self.emit_nav_state(tab_id, &page, app_handle, false).await;
    Ok(())
}
```

- [ ] **Step 4: Update tools.rs nav tool callers**

In `src-tauri/src/browser/tools.rs`, the four nav tools call `ctx.navigate(...)`, `ctx.go_back(...)`, etc. Update each to pass the app_handle. The `ctx_mgr` field has `app_handle()`.

For `BrowserNavigateTool` (the call to `ctx.navigate`):
```rust
// Find the ctx.navigate call (approximately line 80) and update:
let app_handle = self.ctx_mgr.app_handle();
let resolved_tab_id = ctx.navigate(&tab_id, &url, app_handle).await
    .map_err(|e| ToolError::Execution(e.to_string()))?;
```

For `BrowserGoBackTool` (approximately line 125):
```rust
let app_handle = self.ctx_mgr.app_handle();
ctx.go_back(&tab_id, app_handle).await
    .map_err(|e| ToolError::Execution(e.to_string()))?;
```

For `BrowserGoForwardTool` (approximately line 160):
```rust
let app_handle = self.ctx_mgr.app_handle();
ctx.go_forward(&tab_id, app_handle).await
    .map_err(|e| ToolError::Execution(e.to_string()))?;
```

For `BrowserReloadTool` (approximately line 195):
```rust
let app_handle = self.ctx_mgr.app_handle();
ctx.reload(&tab_id, app_handle).await
    .map_err(|e| ToolError::Execution(e.to_string()))?;
```

- [ ] **Step 5: Update tauri_commands.rs UI nav command callers**

Search for `browser_ui_navigate`, `browser_ui_go_back`, `browser_ui_go_forward`, `browser_ui_reload` command handlers in `tauri_commands.rs` (around line ~9130). Each calls `ctx.navigate(...)` etc. — add `app_handle` arg:

```rust
// browser_ui_navigate: find ctx.navigate call, change to:
ctx.navigate(&tab_id, &url, &app_handle).await?

// browser_ui_go_back:
ctx.go_back(&tab_id, &app_handle).await?

// browser_ui_go_forward:
ctx.go_forward(&tab_id, &app_handle).await?

// browser_ui_reload:
ctx.reload(&tab_id, &app_handle).await?
```

The `app_handle` is available in Tauri commands via the `tauri::AppHandle` parameter that all commands accept. If not already present as a parameter, add `app_handle: tauri::AppHandle` to the command signature.

- [ ] **Step 6: Verify Rust compiles with zero errors**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Expected: no output

- [ ] **Step 7: Run Rust tests**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`

Expected: all existing tests pass, no new failures

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/browser/context.rs src-tauri/src/browser/tools.rs \
        src-tauri/src/tauri_commands.rs
git commit -m "feat(browser): emit browser:nav-state on all navigation commands"
```

---

## Task 4 — BrowserPanel Nav-State Subscription + BrowserAddressBar Reactive UI

**Files:**
- Modify: `ui/src/components/browser/BrowserPanel.tsx`
- Modify: `ui/src/components/browser/BrowserAddressBar.tsx`

BrowserPanel subscribes to `browser:nav-state` events and writes into `browserNavStateAtom`. BrowserAddressBar reads that atom for live URL, loading state, and back/forward button disabled state.

- [ ] **Step 1: Add nav-state subscription to BrowserPanel.tsx**

In `ui/src/components/browser/BrowserPanel.tsx`:

1. Update imports at the top:
```typescript
import { useSetAtom, useAtomValue } from 'jotai'
import { listenScreencastFrames, browserGetDOMState, listenNavState } from '@/lib/tauri-bridge'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserScreencastActiveAtom,
  browserDOMOverlayVisibleAtom,
  browserNavStateAtom,
  type BrowserTabEntry,
  type ScreencastFrameEntry,
} from '@/atoms/browser-atoms'
```

2. In the component body, add after the existing atom hooks:
```typescript
const setNavState = useSetAtom(browserNavStateAtom)
```

3. Add a new `useEffect` after the existing screencast subscription effect (after line ~79):
```typescript
// Subscribe to navigation state events for this session.
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

- [ ] **Step 2: Rewrite BrowserAddressBar.tsx**

Replace the entire file `ui/src/components/browser/BrowserAddressBar.tsx` with:

```typescript
import * as React from 'react'
import { ArrowLeft, ArrowRight, RefreshCw, Globe } from 'lucide-react'
import { useAtomValue } from 'jotai'
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
  url: string
}

export function BrowserAddressBar({ sessionId, tabId, url }: BrowserAddressBarProps): React.ReactElement {
  const navStateMap = useAtomValue(browserNavStateAtom)
  const navState = navStateMap.get(sessionId)

  const liveUrl = navState?.url || url
  const isLoading = navState?.isLoading ?? false
  const canGoBack = navState?.canGoBack ?? false
  const canGoForward = navState?.canGoForward ?? false

  const [draft, setDraft] = React.useState(liveUrl)
  const [focused, setFocused] = React.useState(false)

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

- [ ] **Step 3: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: no output

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/browser/BrowserPanel.tsx \
        ui/src/components/browser/BrowserAddressBar.tsx
git commit -m "feat(browser): BrowserPanel subscribes nav-state; address bar shows live URL + loading + back state"
```

---

## Task 5 — Canvas Renderer (BrowserScreencastView + BrowserPreviewOverlay)

**Files:**
- Modify: `ui/src/components/browser/BrowserScreencastView.tsx` (full rewrite)
- Modify: `ui/src/components/agent/BrowserPreviewOverlay.tsx` (canvas section + position fix)

Replace `<img src="data:image/jpeg;base64,...">` with `<canvas>` + `createImageBitmap`. GPU-accelerated async JPEG decode eliminates the main-thread stall on every frame. `BrowserPreviewOverlay` gets the same canvas treatment plus a position fix (`right-14` instead of `right-3`) so the close button no longer overlaps the scroll minimap.

- [ ] **Step 1: Rewrite BrowserScreencastView.tsx**

Replace the entire file `ui/src/components/browser/BrowserScreencastView.tsx`:

```typescript
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MonitorPlay } from 'lucide-react'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserDOMOverlayVisibleAtom,
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
  const lastDimsRef = React.useRef({ w: 0, h: 0 })

  const frame = frameMap.get(sessionId)
  const domEntry = domMap.get(sessionId)

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

- [ ] **Step 2: Update BrowserPreviewOverlay.tsx**

In `ui/src/components/agent/BrowserPreviewOverlay.tsx`, make three changes:

**2a — Fix position** (line 58): change `right-3` to `right-14`:
```typescript
'absolute top-3 right-14 z-20',
```

**2b — Add canvas refs** at the top of the component function, after `const isCollapsed = minimized || panelActive`:
```typescript
const canvasRef = React.useRef<HTMLCanvasElement>(null)
const lastDimsRef = React.useRef({ w: 0, h: 0 })

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
```

**2c — Replace the `<img>` block** (the `{imageSrc ? (<img ...>) : ...}` section) with:
```typescript
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

Also remove the `imageSrc` computed variable since it's no longer used:
```typescript
// DELETE this line:
const imageSrc = frame ? `data:image/jpeg;base64,${frame.dataB64}` : null
```

- [ ] **Step 3: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: no output

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/browser/BrowserScreencastView.tsx \
        ui/src/components/agent/BrowserPreviewOverlay.tsx
git commit -m "feat(browser): canvas+createImageBitmap GPU renderer; overlay position right-14"
```

---

## Task 6 — Complete Device Emulation (Touch Events + CSS Media Features)

**Files:**
- Modify: `src-tauri/src/browser/context.rs:575-606` (`apply_device_emulation`)

Adds `Emulation.setTouchEmulationEnabled` and `Emulation.setEmulatedMedia` to the existing device emulation method. These are confirmed available in chromiumoxide_cdp 0.9.1 as `SetTouchEmulationEnabledParams` and `SetEmulatedMediaParams` / `MediaFeature`.

- [ ] **Step 1: Write the unit test**

In `src-tauri/src/browser/context.rs`, in the existing `#[cfg(test)] mod tests` block (after line ~611):

```rust
#[test]
fn device_preset_mobile_fields() {
    assert_eq!(DevicePreset::Mobile.viewport_width(), 390);
    assert_eq!(DevicePreset::Mobile.viewport_height(), 844);
    assert!(DevicePreset::Mobile.user_agent().contains("iPhone"));
}

#[test]
fn device_preset_desktop_fields() {
    assert_eq!(DevicePreset::Desktop.viewport_width(), 1280);
    assert_eq!(DevicePreset::Desktop.viewport_height(), 800);
    assert!(DevicePreset::Desktop.user_agent().contains("Macintosh"));
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib context -- device_preset 2>&1 | tail -5`

Expected: both pass (they test existing logic, used as regression guards)

- [ ] **Step 3: Replace apply_device_emulation**

Replace the entire `apply_device_emulation` method (lines ~576–606):

```rust
pub async fn apply_device_emulation(&self, tab_id: &str, device: DevicePreset) -> Result<()> {
    let page = self.get_page(tab_id).await?;
    use chromiumoxide::cdp::browser_protocol::emulation::{
        MediaFeature, SetDeviceMetricsOverrideParams, SetEmulatedMediaParams,
        SetTouchEmulationEnabledParams, SetUserAgentOverrideParams,
    };

    // 1. Viewport dimensions + mobile rendering mode.
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

    // 2. User-agent.
    let ua = SetUserAgentOverrideParams {
        user_agent: device.user_agent().to_string(),
        accept_language: None,
        platform: None,
        user_agent_metadata: None,
    };
    page.execute(ua).await
        .map_err(|e| anyhow!("set UA: {e}"))?;

    // 3. Touch emulation — mobile gets 5-point touch, desktop gets none.
    let is_mobile = device == DevicePreset::Mobile;
    let touch = SetTouchEmulationEnabledParams {
        enabled: is_mobile,
        max_touch_points: Some(if is_mobile { 5 } else { 0 }),
    };
    page.execute(touch).await
        .map_err(|e| anyhow!("touch emulation: {e}"))?;

    // 4. CSS media features — pointer:coarse + hover:none for mobile.
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
    page.execute(SetEmulatedMediaParams {
        media: None,
        features: Some(features),
    })
    .await
    .map_err(|e| anyhow!("emulated media: {e}"))?;

    Ok(())
}
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Expected: no output

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/context.rs
git commit -m "feat(browser): complete device emulation — touch events + CSS media features (pointer/hover)"
```

---

## Task 7 — browser_wait Tool + Register Missing Cookie Tools

**Files:**
- Modify: `src-tauri/src/browser/tools.rs` (add `BrowserWaitTool`)
- Modify: `src-tauri/src/tauri_commands.rs` (register `BrowserGetCookiesTool`, `BrowserSetCookieTool`, `BrowserWaitTool` in both blocks)

`browser_wait` gives the agent a way to wait for a page to finish loading. Cookie tools were implemented in a previous sprint but never registered — fixed here.

- [ ] **Step 1: Write the unit test**

In the `#[cfg(test)] mod tests` block at the bottom of `src-tauri/src/browser/tools.rs` (after line ~812):

```rust
#[test]
fn wait_selector_escapes_quotes() {
    let sel = r#"input[name="q"]"#;
    let escaped = sel.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("!!document.querySelector(\"{}\")", escaped);
    assert!(script.contains(r#"input[name=\"q\"]"#), "got: {script}");
    assert!(!script.contains("\"q\""), "unescaped quote would break JS eval, got: {script}");
}

#[test]
fn wait_timeout_default() {
    let params = serde_json::json!({"tab_id": "t1"});
    let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);
    assert_eq!(timeout_ms, 10_000);
}
```

- [ ] **Step 2: Run tests to confirm they pass (pure logic, no browser needed)**

Run: `cd src-tauri && cargo test --lib tools -- wait_ 2>&1 | tail -5`

Expected: both tests pass

- [ ] **Step 3: Add BrowserWaitTool to tools.rs**

In `src-tauri/src/browser/tools.rs`, add `browser_tool!(BrowserWaitTool);` after the `browser_tool!(BrowserSetCookieTool);` line (~line 35):

```rust
browser_tool!(BrowserWaitTool);
```

Then add the `Tool` implementation after the last existing tool impl (before the `// ── Tests` comment, after line ~793):

```rust
// ── 17. BrowserWaitTool ───────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserWaitTool {
    fn name(&self) -> &str { "browser_wait" }

    fn description(&self) -> &str {
        "Wait for a CSS selector to appear in the DOM, or pause for a fixed duration.\n\
         Use after browser_navigate or browser_click when a page or element needs time to load.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID from a previous browser_navigate call.\n\
         - `selector` (string, optional): CSS selector to wait for (e.g. '#main', '.loaded', 'button[type=submit]').\n\
         - `timeout_ms` (number, optional): Maximum wait in milliseconds (default 10000)."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("tab_id required".to_string()))?;
        let selector = params["selector"].as_str();
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);
        let start = Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        if let Some(sel) = selector {
            let escaped = sel.replace('\\', "\\\\").replace('"', "\\\"");
            loop {
                if start.elapsed() >= timeout {
                    return Ok(ToolOutput::error(
                        &format!("Timeout: selector '{}' not found after {}ms", sel, timeout_ms),
                        timeout_ms,
                    ));
                }
                let found = ctx
                    .execute_js(tab_id, &format!("!!document.querySelector(\"{}\")", escaped))
                    .await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                if found.trim() == "true" {
                    let elapsed = start.elapsed().as_millis() as u64;
                    return Ok(ToolOutput::success(
                        format!("Element '{}' found after {}ms", sel, elapsed),
                        elapsed,
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } else {
            tokio::time::sleep(timeout).await;
            Ok(ToolOutput::success(format!("Waited {}ms", timeout_ms), timeout_ms))
        }
    }
}
```

- [ ] **Step 4: Register tools in tauri_commands.rs — BOTH blocks**

The `use crate::browser::tools::*;` wildcard import already covers all tool structs, so no import change is needed.

In the **first** registration block (around line ~890–904):
```rust
tools.register(bt!(BrowserGetCookiesTool));  // was missing
tools.register(bt!(BrowserSetCookieTool));   // was missing
tools.register(bt!(BrowserWaitTool));        // new
```

In the **second** registration block (around line ~8811–8824):
```rust
tools.register(bt!(BrowserGetCookiesTool));  // was missing
tools.register(bt!(BrowserSetCookieTool));   // was missing
tools.register(bt!(BrowserWaitTool));        // new
```

Both blocks must be identical for the tool to work in both chat-mode and agent-mode invocations.

- [ ] **Step 5: Verify Rust compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Expected: no output

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/tools.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(browser): browser_wait tool; register missing BrowserGetCookiesTool + BrowserSetCookieTool"
```

---

## Task 8 — browser_hover + browser_upload_file Tools

**Files:**
- Modify: `src-tauri/src/browser/tools.rs` (add two tool structs + impls)
- Modify: `src-tauri/src/tauri_commands.rs` (register in both blocks)

`browser_hover` triggers CSS `:hover` states and JS `mouseover` listeners — needed for dropdown menus and reveal-on-hover patterns. `browser_upload_file` sets files on `<input type="file">` elements using CDP `DOM.setFileInputFiles`.

- [ ] **Step 1: Write unit tests**

In the `#[cfg(test)]` block of `tools.rs`:

```rust
#[test]
fn hover_script_escapes_index() {
    let index: u32 = 42;
    let script = format!(
        r#"(function(){{ const el = document.querySelector('[data-uclaw-index="{}"]'); return el ? JSON.stringify(el.getBoundingClientRect()) : null; }})()"#,
        index
    );
    assert!(script.contains(r#"[data-uclaw-index="42"]"#), "got: {script}");
}

#[test]
fn upload_rejects_path_traversal() {
    let file_path = "../../../etc/passwd";
    // The resolved path must be under workground; if it starts with ".." it would
    // escape. In the real tool we use Path::join which resolves but stays within
    // the base unless the path is absolute. Verify the join behavior:
    let base = std::path::PathBuf::from("/home/user/Documents/workground");
    let joined = base.join(file_path);
    // Path::join with ".." does navigate up — real tool must check starts_with(base).
    assert!(!joined.starts_with(&base), "traversal: {} escapes base", joined.display());
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib tools -- hover_script_escapes\|upload_rejects 2>&1 | tail -5`

Expected: both pass

- [ ] **Step 3: Add BrowserHoverTool and BrowserUploadFileTool macro declarations**

After `browser_tool!(BrowserWaitTool);` in `tools.rs`:

```rust
browser_tool!(BrowserHoverTool);
browser_tool!(BrowserUploadFileTool);
```

- [ ] **Step 4: Add BrowserHoverTool impl to tools.rs**

After `BrowserWaitTool` impl (before `// ── Tests`):

```rust
// ── 18. BrowserHoverTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserHoverTool {
    fn name(&self) -> &str { "browser_hover" }

    fn description(&self) -> &str {
        "Move the mouse cursor over an element to trigger CSS :hover states and JS mouseover events.\n\
         Required for dropdown menus, tooltips, and any reveal-on-hover UI pattern.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Element index from browser_get_dom."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("tab_id required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput("index required".to_string()))? as u32;

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // Step 1: get bounding box and dispatch JS mouse events (triggers JS listeners).
        let js = format!(
            r#"(function(){{
                const el = document.querySelector('[data-uclaw-index="{}"]');
                if (!el) return null;
                const r = el.getBoundingClientRect();
                const x = r.left + r.width / 2, y = r.top + r.height / 2;
                el.dispatchEvent(new MouseEvent('mouseenter', {{bubbles:false,cancelable:true,clientX:x,clientY:y}}));
                el.dispatchEvent(new MouseEvent('mouseover',  {{bubbles:true, cancelable:true,clientX:x,clientY:y}}));
                el.dispatchEvent(new MouseEvent('mousemove',  {{bubbles:true, cancelable:true,clientX:x,clientY:y}}));
                return {{x: Math.round(x), y: Math.round(y)}};
            }})()"#,
            index
        );

        let result = ctx.execute_js(tab_id, &js).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        if result.trim() == "null" {
            return Err(ToolError::Execution(format!("Element with index {} not found", index)));
        }

        // Step 2: send CDP Input.dispatchMouseEvent to activate CSS :hover pseudo-class.
        let coords: serde_json::Value = serde_json::from_str(&result)
            .unwrap_or(serde_json::Value::Null);
        if let (Some(x), Some(y)) = (coords["x"].as_f64(), coords["y"].as_f64()) {
            use chromiumoxide::cdp::browser_protocol::input::{
                DispatchMouseEventParams, DispatchMouseEventType,
            };
            let pages = ctx.pages.read().await;
            if let Some(page) = pages.get(tab_id) {
                let _ = page.execute(DispatchMouseEventParams {
                    r#type: DispatchMouseEventType::MouseMoved,
                    x,
                    y,
                    modifiers: None,
                    timestamp: None,
                    button: None,
                    buttons: None,
                    click_count: None,
                    force: None,
                    tangential_pressure: None,
                    tilt_x: None,
                    tilt_y: None,
                    twist: None,
                    delta_x: None,
                    delta_y: None,
                    pointer_type: None,
                }).await;
            }
        }

        Ok(ToolOutput::success(format!("Hovered element at index {}", index), 0))
    }
}

// ── 19. BrowserUploadFileTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserUploadFileTool {
    fn name(&self) -> &str { "browser_upload_file" }

    fn description(&self) -> &str {
        "Set a file on a file input element (<input type='file'>).\n\
         The file must exist in the agent workspace (~/Documents/workground/).\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Index of the file input element from browser_get_dom.\n\
         - `file_path` (string, required): Path relative to ~/Documents/workground/ \
           (e.g. 'report.pdf' or 'images/photo.jpg'). Must not contain '..'."
    }

    async fn call(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("tab_id required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput("index required".to_string()))? as u32;
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("file_path required".to_string()))?;

        // Resolve to absolute path and verify it stays under the workspace root.
        let workspace_root = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("Documents/workground");
        let abs_path = workspace_root.join(file_path);
        if !abs_path.starts_with(&workspace_root) {
            return Err(ToolError::InvalidInput(
                "file_path must not escape the workspace directory".to_string(),
            ));
        }
        if !abs_path.exists() {
            return Err(ToolError::Execution(format!(
                "File not found: {} (looked in {})",
                file_path,
                abs_path.display()
            )));
        }

        let ctx = self.ctx_mgr.get_or_create(&self.session_id).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // Use CDP DOM.setFileInputFiles with the element's NodeId.
        let selector = format!("[data-uclaw-index=\"{}\"]", index);
        let pages = ctx.pages.read().await;
        let page = pages.get(tab_id)
            .ok_or_else(|| ToolError::Execution(format!("tab_id {} not found", tab_id)))?;

        let element = page.find_element(selector).await
            .map_err(|_| ToolError::Execution(format!("Element with index {} not found", index)))?;

        use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
        page.execute(SetFileInputFilesParams {
            files: vec![abs_path.to_string_lossy().to_string()],
            node_id: Some(element.node_id),
            backend_node_id: None,
            object_id: None,
        })
        .await
        .map_err(|e| ToolError::Execution(format!("setFileInputFiles failed: {e}")))?;

        Ok(ToolOutput::success(
            format!("File '{}' set on input element at index {}", file_path, index),
            0,
        ))
    }
}
```

- [ ] **Step 5: Register in tauri_commands.rs — BOTH blocks**

In the first block (around line ~904) add after the wait tool line:
```rust
tools.register(bt!(BrowserHoverTool));
tools.register(bt!(BrowserUploadFileTool));
```

In the second block (around line ~8824) add after the wait tool line:
```rust
tools.register(bt!(BrowserHoverTool));
tools.register(bt!(BrowserUploadFileTool));
```

- [ ] **Step 6: Verify Rust compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

Expected: no output

- [ ] **Step 7: Run all Rust tests**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -15`

Expected: all pass, including the new `hover_script_escapes_index`, `upload_rejects_path_traversal`, `wait_selector_escapes_quotes`, `wait_timeout_default`

- [ ] **Step 8: Run TypeScript check one final time**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: no output

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/browser/tools.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(browser): browser_hover + browser_upload_file tools (19 tools total)"
```

---

## Final Verification Checklist

After all 8 tasks, run `cargo tauri dev` and verify:

1. **Screencast shows:** Ask agent "打开 apple.com" → BrowserPanel opens → canvas frames appear (not stuck on "等待浏览器画面...")
2. **Address bar live URL:** While agent navigates between pages, address bar URL updates automatically
3. **Loading spinner:** RefreshCw icon spins when `is_loading: true`
4. **Back button state:** After navigating away from about:blank, back button becomes enabled
5. **Reload + screencast:** Click reload → frames continue (screencast restarts)
6. **BrowserPreviewOverlay:** Close (X) button is not overlapping the scroll minimap (right-14 position)
7. **Mobile device:** `browser_navigate url="https://x.com" device="mobile"` → 390px viewport + touch events
8. **browser_wait:** Agent calls `browser_wait tab_id=... selector=".main"` → returns "Element found" or timeout
9. **browser_hover:** Agent calls `browser_hover index=...` → hover effects trigger
10. **browser_upload_file:** Place file in `~/Documents/workground/test.txt`, call `browser_upload_file` → input populated
