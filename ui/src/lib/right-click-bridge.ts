/**
 * Right-click bridge — ensures the browser `contextmenu` event fires for
 * every right-button mousedown across the app, even in environments where
 * the webview's default behavior swallows it (a known nuisance on some
 * macOS Tauri/WKWebView configurations: certain non-selectable elements
 * never receive a native `contextmenu` event, so Radix's `ContextMenu`
 * trigger never opens).
 *
 * Strategy:
 *  1. Listen for `mousedown` button === 2 at the document level (bubble
 *     phase, so the page's own preventDefaults can still cancel us by
 *     calling `stopPropagation` — which is what an input's text-selection
 *     menu does).
 *  2. Watch the same target for a native `contextmenu` event in the next
 *     tick. If one arrives, we do nothing (the platform handled it).
 *  3. If the native event never arrives within ~16ms, dispatch a synthetic
 *     `contextmenu` MouseEvent that bubbles + is cancelable. Radix's
 *     trigger picks it up exactly like the native one.
 *
 * Idempotent: calling install() twice without uninstall is a no-op (a
 * module-level flag tracks whether the listener is already attached).
 */

let installed = false

function bridgeRightClick(event: MouseEvent): void {
  if (event.button !== 2) return
  const target = event.target as EventTarget | null
  if (!target) return

  let nativeFired = false
  const onNative = (): void => {
    nativeFired = true
  }
  // Capture phase so we observe the event even if some intermediate
  // handler later calls stopPropagation.
  target.addEventListener('contextmenu', onNative, { once: true, capture: true })

  // One animation frame is plenty of time for the platform's own
  // contextmenu dispatch — we only need to bridge environments where it
  // never fires at all. Avoiding 0ms keeps us out of the same microtask
  // queue as the mousedown's native follow-up.
  window.setTimeout(() => {
    target.removeEventListener('contextmenu', onNative, true)
    if (nativeFired) return
    const synthetic = new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      button: 2,
      buttons: event.buttons,
      clientX: event.clientX,
      clientY: event.clientY,
      screenX: event.screenX,
      screenY: event.screenY,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      metaKey: event.metaKey,
    })
    target.dispatchEvent(synthetic)
  }, 16)
}

export function installRightClickBridge(): void {
  if (installed) return
  installed = true
  document.addEventListener('mousedown', bridgeRightClick, true)
}

// Test-only helper. Production code should treat the bridge as
// fire-and-forget — there's no scenario where the app wants to suppress
// app-wide right-click after install.
export function _uninstallRightClickBridgeForTests(): void {
  if (!installed) return
  installed = false
  document.removeEventListener('mousedown', bridgeRightClick, true)
}
