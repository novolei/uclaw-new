import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { render, screen } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { MotionConfig } from 'motion/react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { BottomDock } from './BottomDock'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue({}) }))
vi.mock('./useConnectionStatus', () => ({ useConnectionStatus: () => {} }))
vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style: _style, ...rest }, ref) => <button ref={ref} {...rest} />),
    div: ({ initial: _i, animate: _a, transition: _t, ...rest }: React.ComponentPropsWithoutRef<'div'> & Record<string, unknown>) =>
      <div {...rest} />,
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
  MotionConfig: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}))

function renderDock(enabled = true) {
  const store = createStore()
  store.set(bottomDockEnabledAtom, enabled)
  return render(
    <JotaiProvider store={store}>
      <MotionConfig reducedMotion="always">
        <TooltipProvider>
          <BottomDock revealed />
        </TooltipProvider>
      </MotionConfig>
    </JotaiProvider>,
  )
}

describe('BottomDock · icons', () => {
  it('renders an <img> for every dock item (not lucide svg)', () => {
    renderDock()
    for (const label of ['聊天', 'Agent', '记忆', '万花筒']) {
      const btn = screen.getByRole('button', { name: label })
      const img = btn.querySelector('img')
      const svg = btn.querySelector('svg')
      expect(img, `${label} should render an <img>`).not.toBeNull()
      expect(svg, `${label} must no longer render an <svg> icon`).toBeNull()
    }
  })

  it('image src points to the PNG asset bundled by Vite', () => {
    renderDock()
    const chatImg = screen
      .getByRole('button', { name: '聊天' })
      .querySelector('img') as HTMLImageElement
    expect(chatImg.src).toMatch(/chat\.png/)
  })
})
