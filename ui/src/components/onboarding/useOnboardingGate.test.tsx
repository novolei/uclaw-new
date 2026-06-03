import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { Provider, createStore, useAtomValue } from 'jotai'
import * as React from 'react'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

const getOnboardingState = vi.fn()
vi.mock('@/lib/tauri-bridge', () => ({ getOnboardingState: () => getOnboardingState() }))

import { useOnboardingGate } from './useOnboardingGate'

function setup() {
  const store = createStore()
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    <Provider store={store}>{children}</Provider>
  const r = renderHook(() => { useOnboardingGate(); return useAtomValue(minicpmWizardAtom) }, { wrapper })
  return { result: r.result }
}

beforeEach(() => getOnboardingState.mockReset())

describe('useOnboardingGate', () => {
  it('opens wizard when pending', async () => {
    getOnboardingState.mockResolvedValue('pending')
    const { result } = setup()
    await waitFor(() => expect(result.current.step).toBe('intro'))
  })
  it('opens wizard when deferred', async () => {
    getOnboardingState.mockResolvedValue('deferred')
    const { result } = setup()
    await waitFor(() => expect(result.current.step).toBe('intro'))
  })
  it('does NOT open when completed', async () => {
    getOnboardingState.mockResolvedValue('completed')
    const { result } = setup()
    await new Promise((r) => setTimeout(r, 30))
    expect(result.current.step).toBeNull()
  })
  it('does NOT open when skipped', async () => {
    getOnboardingState.mockResolvedValue('skipped')
    const { result } = setup()
    await new Promise((r) => setTimeout(r, 30))
    expect(result.current.step).toBeNull()
  })
})
