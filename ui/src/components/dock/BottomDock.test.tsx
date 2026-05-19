import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { render, screen } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { MotionConfig } from 'motion/react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { BottomDock } from './BottomDock'
import { bottomDockEnabledAtom, dockOrderAtom, type DockItemSpec } from '@/atoms/dock-atoms'

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

function renderDockWithOrder(order: DockItemSpec[]) {
  const store = createStore()
  store.set(bottomDockEnabledAtom, true)
  store.set(dockOrderAtom, order)
  return render(
    <JotaiProvider store={store}>
      <BottomDock revealed />
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

describe('BottomDock · atom-driven order', () => {
  it('renders one button per dockOrderAtom entry, in atom order', () => {
    const { container } = renderDock()
    const buttons = container.querySelectorAll('button[aria-label]')
    // Default atom seed produces 聊天 / Agent / 记忆 / 万花筒, in that order.
    expect(buttons[0].getAttribute('aria-label')).toBe('聊天')
    expect(buttons[1].getAttribute('aria-label')).toBe('Agent')
    expect(buttons[2].getAttribute('aria-label')).toBe('记忆')
    expect(buttons[3].getAttribute('aria-label')).toBe('万花筒')
  })

  it('reflects a reordered atom in render order', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ])
    const buttons = container.querySelectorAll('button[aria-label]')
    expect(buttons[0].getAttribute('aria-label')).toBe('Agent')
    expect(buttons[1].getAttribute('aria-label')).toBe('聊天')
  })

  it('mounts inside a DndContext (data-dock-dnd-root marker present)', () => {
    const { container } = renderDock()
    const marker = container.querySelector('[data-dock-dnd-root]')
    expect(marker).not.toBeNull()
  })
})

describe('BottomDock · pinned-* dispatch + dynamic divider', () => {
  it('renders a DockPinnedItem for each pinned-* entry', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'pinned-workspace', spaceId: 'space-1' },
      { kind: 'mode', mode: 'agent' },
    ])
    const pins = container.querySelectorAll('[data-dock-pin]')
    expect(pins.length).toBe(1)
    expect(pins[0].getAttribute('data-sortable-id')).toBe('space-space-1')
  })

  it('renders the section divider between the last contiguous mode and the first non-mode', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'pinned-workspace', spaceId: 'space-1' },
      { kind: 'pinned-workspace', spaceId: 'space-2' },
    ])
    const divider = container.querySelector('[data-dock-section-divider]')
    expect(divider).not.toBeNull()
    const buttons = container.querySelectorAll('button')
    const dividerEl = divider as HTMLElement
    const all = Array.from(
      container.querySelectorAll('button, [data-dock-section-divider]')
    )
    const dividerPos = all.indexOf(dividerEl)
    const agentPos = all.indexOf(buttons[1])
    const firstPinPos = all.indexOf(buttons[2])
    expect(dividerPos).toBeGreaterThan(agentPos)
    expect(dividerPos).toBeLessThan(firstPinPos)
  })

  it('omits the divider entirely when no pinned entries are present', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ])
    expect(container.querySelector('[data-dock-section-divider]')).toBeNull()
  })
})
