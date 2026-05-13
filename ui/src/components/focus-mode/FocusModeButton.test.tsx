import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FocusModeButton } from './FocusModeButton'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'

describe('FocusModeButton', () => {
  it('renders the enter-focus title when focus mode is OFF', () => {
    renderWithProviders(<FocusModeButton />)
    const btn = screen.getByRole('button', { name: /进入专注模式/ })
    expect(btn).not.toBeNull()
  })

  it('flips title to exit-focus when focus mode is ON, and click toggles the atom', async () => {
    const { store, user } = renderWithProviders(<FocusModeButton />)
    expect(store.get(focusModeAtom)).toBe(false)
    await user.click(screen.getByRole('button', { name: /进入专注模式/ }))
    expect(store.get(focusModeAtom)).toBe(true)
    // After toggle, the button's aria-label updates
    expect(screen.getByRole('button', { name: /退出专注模式/ })).not.toBeNull()
  })
})
