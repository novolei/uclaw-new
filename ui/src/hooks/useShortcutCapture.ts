/**
 * useShortcutCapture — capture-mode for a single keybinding row.
 *
 * Activate via `start()`. While active, the next keydown event is parsed
 * into a uClaw shortcut string ("Cmd+Shift+P" / "Alt+F" / etc.) and
 * delivered through `onCapture`. The user can press Escape to cancel
 * (delivers `null`).
 *
 * Modifier-only presses (just Cmd, just Shift, etc.) are ignored — we
 * wait for a non-modifier key to land. This is the same UX as VS Code's
 * keybinding picker.
 *
 * Mac Option+letter caveat: macOS turns Option+F into the special glyph
 * ƒ at the event.key layer. We capture via event.code (physical key)
 * when Alt is in the combo, so the recorded combo string is canonical
 * ("Alt+F" not "Alt+ƒ"). This mirrors useShortcut.ts:matchesShortcut.
 */

import * as React from 'react'

interface UseShortcutCaptureArgs {
  /** Whether capture mode is currently active. */
  active: boolean
  /**
   * Called with the captured combo string (e.g. `"Cmd+Shift+P"`) or
   * `null` if the user pressed Escape. The host should clear `active`
   * after receiving the result.
   */
  onCapture: (combo: string | null) => void
}

/** Physical-key codes we treat as "modifier only" — these never complete
 *  a combo by themselves; we wait for a real key to be pressed alongside. */
const MODIFIER_KEYS = new Set([
  'Meta', 'MetaLeft', 'MetaRight',
  'Control', 'ControlLeft', 'ControlRight',
  'Alt', 'AltLeft', 'AltRight',
  'Shift', 'ShiftLeft', 'ShiftRight',
  'OS',
])

/**
 * Translate a `KeyboardEvent.code` (e.g. `"KeyF"`, `"Digit3"`, `"Slash"`)
 * back into the shortcut-string fragment uClaw expects (`"F"`, `"3"`,
 * `"/"`). Returns `null` if the code is unrecognized (we don't want to
 * capture, say, function keys via opaque codes).
 */
function codeToShortcutKey(code: string): string | null {
  if (code.startsWith('Key') && code.length === 4) return code.slice(3) // KeyF → F
  if (code.startsWith('Digit') && code.length === 6) return code.slice(5) // Digit3 → 3
  // Common navigation / special keys that ARE accepted as combos
  const named: Record<string, string> = {
    Escape: 'Escape',
    Enter: 'Enter',
    Backspace: 'Backspace',
    Tab: 'Tab',
    Space: 'Space',
    ArrowLeft: 'ArrowLeft',
    ArrowRight: 'ArrowRight',
    ArrowUp: 'ArrowUp',
    ArrowDown: 'ArrowDown',
    Comma: ',',
    Period: '.',
    Slash: '/',
    Backslash: '\\',
    Minus: '-',
    Equal: '=',
    Semicolon: ';',
    Quote: "'",
    BracketLeft: '[',
    BracketRight: ']',
    Backquote: '`',
  }
  return named[code] ?? null
}

/**
 * Build the canonical uClaw shortcut string from a captured KeyboardEvent.
 * Returns null when only modifiers are pressed or the physical key is
 * unrecognized.
 */
export function eventToShortcut(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.code)) return null
  const keyFragment = codeToShortcutKey(e.code)
  if (!keyFragment) return null
  const parts: string[] = []
  // Order matches the registry convention: Cmd / Ctrl > Shift > Alt
  if (e.metaKey) parts.push('Cmd')
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey) parts.push('Alt')
  parts.push(keyFragment)
  return parts.join('+')
}

export function useShortcutCapture({ active, onCapture }: UseShortcutCaptureArgs): void {
  // Stash the callback in a ref so the listener stays stable across renders.
  const onCaptureRef = React.useRef(onCapture)
  React.useEffect(() => { onCaptureRef.current = onCapture }, [onCapture])

  React.useEffect(() => {
    if (!active) return

    const onKeyDown = (e: KeyboardEvent) => {
      // Escape (without modifiers) → cancel capture.
      if (e.code === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        e.preventDefault()
        e.stopPropagation()
        onCaptureRef.current(null)
        return
      }
      const combo = eventToShortcut(e)
      if (!combo) return  // modifier-only press, keep waiting
      e.preventDefault()
      e.stopPropagation()
      onCaptureRef.current(combo)
    }

    // Capture phase so we beat the global useShortcut listeners — otherwise
    // a press of, say, Cmd+Shift+F here would also trigger the existing
    // search-conversations shortcut while we were trying to record it.
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [active])
}
