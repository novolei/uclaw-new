import * as React from 'react'
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, fireEvent, act } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { BottomDockHoverRegion } from './BottomDockHoverRegion'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'

// BottomDock renders BottomDock → DockItem → motion + tauri invoke (via
// useConnectionStatus). Stub the noisy bits so the hover-state-machine
// is what we're actually testing.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue({}) }))
vi.mock('./useConnectionStatus', () => ({ useConnectionStatus: () => {} }))
vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style: _style, ...rest }, ref) => <button ref={ref} {...rest} />),
    div: ({
      // strip motion-only props so React doesn't warn
      initial: _i,
      animate: _a,
      transition: _t,
      ...rest
    }: React.ComponentPropsWithoutRef<'div'> & Record<string, unknown>) =>
      <div {...rest} />,
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
  MotionConfig: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}))

function renderRegion(enabled = true) {
  const store = createStore()
  store.set(bottomDockEnabledAtom, enabled)
  return render(
    <JotaiProvider store={store}>
      <BottomDockHoverRegion />
    </JotaiProvider>,
  )
}

describe('BottomDockHoverRegion', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('starts collapsed (data-revealed=false)', () => {
    const { container } = renderRegion()
    expect(container.querySelector('[data-revealed="false"]')).toBeTruthy()
  })

  it('reveals on mouseEnter and hides on mouseLeave (debounced)', () => {
    const { container } = renderRegion()
    const region = container.querySelector('[data-revealed]') as HTMLElement

    fireEvent.mouseEnter(region)
    expect(region.dataset.revealed).toBe('true')

    fireEvent.mouseLeave(region)
    // still revealed during the 220ms debounce window
    expect(region.dataset.revealed).toBe('true')

    act(() => {
      vi.advanceTimersByTime(250)
    })
    expect(region.dataset.revealed).toBe('false')
  })

  it('re-entering during the hide debounce cancels the hide', () => {
    const { container } = renderRegion()
    const region = container.querySelector('[data-revealed]') as HTMLElement

    fireEvent.mouseEnter(region)
    fireEvent.mouseLeave(region)
    // Re-enter before the 220ms debounce fires
    act(() => {
      vi.advanceTimersByTime(120)
    })
    fireEvent.mouseEnter(region)
    act(() => {
      vi.advanceTimersByTime(600)
    })
    expect(region.dataset.revealed).toBe('true')
  })

  it('keeps the container open during the dock exit animation', () => {
    // After the hide debounce fires, the container stays at its wide bounds
    // for HIDE_ANIM_DURATION_MS so re-entry can catch the dock mid-fall. The
    // dock's `revealed` flips false at debounce, container width collapses
    // only after the dock animation has played out.
    const { container } = renderRegion()
    const region = container.querySelector('[data-revealed]') as HTMLElement

    fireEvent.mouseEnter(region)
    fireEvent.mouseLeave(region)
    act(() => {
      vi.advanceTimersByTime(250) // past hide debounce
    })
    expect(region.dataset.revealed).toBe('false')
    expect(region.dataset.containerOpen).toBe('true')

    act(() => {
      vi.advanceTimersByTime(500) // past dock exit animation
    })
    expect(region.dataset.containerOpen).toBe('false')
  })
})
