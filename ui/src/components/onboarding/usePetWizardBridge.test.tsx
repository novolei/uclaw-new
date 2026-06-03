import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { minicpmWizardAtom, INITIAL_WIZARD } from '@/atoms/minicpm-wizard'

// Capture listen callbacks so tests can trigger them manually
const listeners: Record<string, (e: any) => void> = {}
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: (e: any) => void) => {
    listeners[name] = cb
    return Promise.resolve(() => {})
  }),
  emit: vi.fn(),
}))

import { usePetWizardBridge } from './usePetWizardBridge'

describe('usePetWizardBridge', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
    store.set(minicpmWizardAtom, { ...INITIAL_WIZARD, step: null })
  })

  function wrapper({ children }: { children: React.ReactNode }) {
    return <Provider store={store}>{children}</Provider>
  }

  it('registers a pet://open-wizard listener on mount', async () => {
    const { listen: mockListen } = await import('@tauri-apps/api/event')
    renderHook(() => usePetWizardBridge(), { wrapper })
    // Allow the promise microtask to settle
    await act(async () => {})
    expect(mockListen).toHaveBeenCalledWith('pet://open-wizard', expect.any(Function))
  })

  it('sets wizard step to intro when pet://open-wizard fires', async () => {
    renderHook(() => usePetWizardBridge(), { wrapper })
    await act(async () => {})

    // Trigger the captured listener
    act(() => {
      listeners['pet://open-wizard']?.({})
    })

    expect(store.get(minicpmWizardAtom).step).toBe('intro')
  })

  it('preserves other wizard state when opening', async () => {
    store.set(minicpmWizardAtom, { ...INITIAL_WIZARD, step: null, error: 'prev-error' })
    renderHook(() => usePetWizardBridge(), { wrapper })
    await act(async () => {})

    act(() => {
      listeners['pet://open-wizard']?.({})
    })

    const state = store.get(minicpmWizardAtom)
    expect(state.step).toBe('intro')
    expect(state.error).toBe('prev-error')
  })
})
