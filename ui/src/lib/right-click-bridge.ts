/**
 * Right-click bridge — owns app-wide `contextmenu` event behavior for the
 * uClaw desktop window.
 *
 * Two responsibilities:
 *
 *  1. **Suppress WebKit's default context menu.** On macOS Tauri (WKWebView)
 *     dev builds, right-clicking shows "Reload / Inspect Element" by
 *     default — a browser dev affordance that has no place in a shipping
 *     desktop app. We `preventDefault()` every contextmenu event so the
 *     native menu never appears. Editable form controls (`<input>`,
 *     `<textarea>`, `[contenteditable]`) keep their native menu because
 *     that's where users expect text-selection / spellcheck operations.
 *
 *  2. **Synthesize a `contextmenu` event when the platform doesn't.**
 *     Some macOS Tauri configurations never dispatch a JS contextmenu
 *     event for non-selectable elements — so Radix `ContextMenu` triggers
 *     never open. When that happens, we fire a synthetic event ourselves
 *     so React's `onContextMenu` handlers still see a fresh event.
 *
 * Both behaviors are installed by `installRightClickBridge()`, called once
 * from `main.tsx` before React mounts. Idempotent — second install() is
 * a no-op.
 */

let installed = false

/** Match elements where the native context menu should be preserved. */
function shouldAllowNativeMenu(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false
  // Form controls and contenteditable surfaces — users expect cut/copy/
  // paste / spellcheck / dictation entries via the system menu here.
  if (target.closest('input, textarea, [contenteditable=""], [contenteditable="true"]')) {
    return true
  }
  // Opt-in escape hatch for any future component that explicitly wants
  // the native menu (e.g. a debug panel showing "Reload / Inspect").
  if (target.closest('[data-allow-native-contextmenu="true"]')) {
    return true
  }
  return false
}

/**
 * Document-level contextmenu listener. Runs in capture phase so we see
 * the event BEFORE React's listeners — but we don't stopPropagation, so
 * Radix and other React onContextMenu handlers still fire normally.
 *
 * Returning early on allow-native-menu targets lets the native menu
 * appear there (no preventDefault).
 */
function suppressDefaultMenu(event: Event): void {
  if (shouldAllowNativeMenu(event.target)) return
  event.preventDefault()
}

/**
 * mousedown fallback: in environments where the platform doesn't fire
 * a contextmenu event at all on right-button mousedown, we dispatch a
 * synthetic one ourselves so Radix triggers still activate.
 */
function bridgeRightClick(event: MouseEvent): void {
  if (event.button !== 2) return
  const target = event.target as EventTarget | null
  if (!target) return

  let nativeFired = false
  const onNative = (): void => {
    nativeFired = true
  }
  target.addEventListener('contextmenu', onNative, { once: true, capture: true })

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
  // Capture phase so this runs before any React-level handlers; we don't
  // stopPropagation, so Radix etc. still see the event.
  document.addEventListener('contextmenu', suppressDefaultMenu, true)
  document.addEventListener('mousedown', bridgeRightClick, true)
}

// Test-only helper.
export function _uninstallRightClickBridgeForTests(): void {
  if (!installed) return
  installed = false
  document.removeEventListener('contextmenu', suppressDefaultMenu, true)
  document.removeEventListener('mousedown', bridgeRightClick, true)
}
