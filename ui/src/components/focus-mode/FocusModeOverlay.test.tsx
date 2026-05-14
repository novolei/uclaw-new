import { describe, it, expect, vi } from 'vitest'

// Mock lottie-react so the test doesn't need a real canvas/animation runtime.
vi.mock('lottie-react', () => ({
  default: () => <div data-testid="lottie-stub" />,
}))
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FocusModeOverlay } from './FocusModeOverlay'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

describe('FocusModeOverlay', () => {
  it('renders nothing when focus mode is OFF', () => {
    renderWithProviders(<FocusModeOverlay />)
    expect(screen.queryByTestId('focus-glow-left')).toBeNull()
    expect(screen.queryByTestId('focus-glow-right')).toBeNull()
  })

  it('renders both glow indicators when focus mode is ON and preview is open', () => {
    const store = createStore()
    store.set(previewPanelOpenAtom, true)
    store.set(focusModeAtom, true)
    renderWithProviders(<FocusModeOverlay />, { store })
    expect(screen.queryByTestId('focus-glow-left')).not.toBeNull()
    expect(screen.queryByTestId('focus-glow-right')).not.toBeNull()
  })
})
