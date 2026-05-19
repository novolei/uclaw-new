import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
  installRightClickBridge,
  _uninstallRightClickBridgeForTests,
} from './right-click-bridge'

describe('right-click-bridge', () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="target">hi</div>'
    vi.useFakeTimers()
    installRightClickBridge()
  })

  afterEach(() => {
    _uninstallRightClickBridgeForTests()
    vi.useRealTimers()
  })

  it('synthesizes a contextmenu event when no native one fires', () => {
    const target = document.getElementById('target')!
    const handler = vi.fn()
    target.addEventListener('contextmenu', handler)

    target.dispatchEvent(
      new MouseEvent('mousedown', { button: 2, bubbles: true, clientX: 10, clientY: 20 }),
    )
    // Bridge's setTimeout fires; with no native contextmenu, it dispatches one.
    vi.runAllTimers()

    expect(handler).toHaveBeenCalledTimes(1)
    const evt = handler.mock.calls[0][0] as MouseEvent
    expect(evt.button).toBe(2)
    expect(evt.clientX).toBe(10)
    expect(evt.clientY).toBe(20)
    expect(evt.bubbles).toBe(true)
    expect(evt.cancelable).toBe(true)
  })

  it('does NOT synthesize when a native contextmenu has already fired', () => {
    const target = document.getElementById('target')!
    const handler = vi.fn()
    target.addEventListener('contextmenu', handler)

    target.dispatchEvent(
      new MouseEvent('mousedown', { button: 2, bubbles: true }),
    )
    // Native contextmenu (e.g. from the platform) arrives first.
    target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }))
    expect(handler).toHaveBeenCalledTimes(1)

    vi.runAllTimers()
    // Still 1 — bridge didn't synthesize because it saw the native fire.
    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('ignores non-right mousedown events', () => {
    const target = document.getElementById('target')!
    const handler = vi.fn()
    target.addEventListener('contextmenu', handler)

    target.dispatchEvent(new MouseEvent('mousedown', { button: 0, bubbles: true }))
    target.dispatchEvent(new MouseEvent('mousedown', { button: 1, bubbles: true }))
    vi.runAllTimers()

    expect(handler).not.toHaveBeenCalled()
  })

  it('install is idempotent — second call does not double-dispatch', () => {
    installRightClickBridge() // second install
    const target = document.getElementById('target')!
    const handler = vi.fn()
    target.addEventListener('contextmenu', handler)

    target.dispatchEvent(new MouseEvent('mousedown', { button: 2, bubbles: true }))
    vi.runAllTimers()

    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('preventDefaults the contextmenu event so WebKit native menu does not show', () => {
    const target = document.getElementById('target')!
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    target.dispatchEvent(evt)
    // defaultPrevented = true means WKWebView/WebKit will skip the
    // native "Reload / Inspect Element" menu.
    expect(evt.defaultPrevented).toBe(true)
  })

  it('still lets React/Radix listeners see the event (no stopPropagation)', () => {
    const target = document.getElementById('target')!
    const handler = vi.fn()
    target.addEventListener('contextmenu', handler)
    target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    // Listeners on the same target should still fire; only the
    // browser's default action is suppressed.
    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('allows native contextmenu on inputs (text-selection menu kept)', () => {
    document.body.innerHTML = '<input id="text" type="text" />'
    const input = document.getElementById('text')!
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    input.dispatchEvent(evt)
    expect(evt.defaultPrevented).toBe(false)
  })

  it('allows native contextmenu on opted-in elements', () => {
    document.body.innerHTML = '<div data-allow-native-contextmenu="true" id="optin">x</div>'
    const optin = document.getElementById('optin')!
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    optin.dispatchEvent(evt)
    expect(evt.defaultPrevented).toBe(false)
  })
})
