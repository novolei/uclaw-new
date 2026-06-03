import { describe, it, expect, vi, beforeEach } from 'vitest'

// ---------------------------------------------------------------------------
// Hoist mock vars so they are available in the vi.mock factory (which is
// moved to the top of the file at compile time by Vitest).
// ---------------------------------------------------------------------------
const { MockWebviewWindow, fakeWin, getByLabelMock } = vi.hoisted(() => {
  const fakeWin = {
    show: vi.fn().mockResolvedValue(undefined),
    setFocus: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    setPosition: vi.fn().mockResolvedValue(undefined),
  }

  const getByLabelMock = vi.fn()

  const MockWebviewWindow = vi.fn()
  // Attach static method so callers can do WebviewWindow.getByLabel(...)
  ;(MockWebviewWindow as unknown as { getByLabel: typeof getByLabelMock }).getByLabel =
    getByLabelMock

  return { MockWebviewWindow, fakeWin, getByLabelMock }
})

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: MockWebviewWindow,
}))

// ---------------------------------------------------------------------------
// Import after mock is registered
// ---------------------------------------------------------------------------
import { openPetWindow, closePetWindow, togglePetWindow, PET_WINDOW_LABEL } from './pet-window'

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('pet-window', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Re-attach static after clearAllMocks resets the fn
    ;(MockWebviewWindow as unknown as { getByLabel: typeof getByLabelMock }).getByLabel =
      getByLabelMock
  })

  describe('openPetWindow()', () => {
    it('constructs a new WebviewWindow when no existing window', async () => {
      getByLabelMock.mockResolvedValue(null)

      await openPetWindow()

      expect(MockWebviewWindow).toHaveBeenCalledTimes(1)
      const [label, opts] = MockWebviewWindow.mock.calls[0] as [string, Record<string, unknown>]
      expect(label).toBe(PET_WINDOW_LABEL)
      expect(opts.transparent).toBe(true)
      expect(opts.decorations).toBe(false)
      expect(opts.alwaysOnTop).toBe(true)
      expect(opts.skipTaskbar).toBe(true)
      expect(opts.resizable).toBe(false)
      expect(opts.shadow).toBe(false)
      expect(String(opts.url)).toContain('uclawWindow=pet')
    })

    it('calls show() + setFocus() and does NOT construct a new window when existing', async () => {
      getByLabelMock.mockResolvedValue(fakeWin)

      await openPetWindow()

      expect(MockWebviewWindow).not.toHaveBeenCalled()
      expect(fakeWin.show).toHaveBeenCalledTimes(1)
      expect(fakeWin.setFocus).toHaveBeenCalledTimes(1)
    })
  })

  describe('closePetWindow()', () => {
    it('calls close() on the existing window', async () => {
      getByLabelMock.mockResolvedValue(fakeWin)

      await closePetWindow()

      expect(fakeWin.close).toHaveBeenCalledTimes(1)
    })

    it('does nothing if no window exists', async () => {
      getByLabelMock.mockResolvedValue(null)

      await expect(closePetWindow()).resolves.toBeUndefined()
    })
  })

  describe('togglePetWindow()', () => {
    it('calls openPetWindow path when enabled=true', async () => {
      getByLabelMock.mockResolvedValue(null)

      await togglePetWindow(true)

      expect(MockWebviewWindow).toHaveBeenCalledTimes(1)
    })

    it('calls closePetWindow path when enabled=false', async () => {
      getByLabelMock.mockResolvedValue(fakeWin)

      await togglePetWindow(false)

      expect(fakeWin.close).toHaveBeenCalledTimes(1)
      expect(MockWebviewWindow).not.toHaveBeenCalled()
    })
  })
})
