import { describe, test, expect, vi } from 'vitest'
import { createStore } from 'jotai'
import * as React from 'react'
import { renderWithProviders } from '@/test-utils/render'
import { installWizardAtom } from '@/atoms/marketplace'

// Stub motion/react so animations don't interfere with jsdom assertions
vi.mock('motion/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('motion/react')>()
  return {
    ...actual,
    AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
    motion: new Proxy(
      {},
      {
        get(_target, tag: string) {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          return ({ children, ...props }: any) => React.createElement(tag, props, children)
        },
      },
    ),
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('@/lib/tauri-bridge', () => ({
  installMarketplaceHuman: vi.fn(),
}))

import { InstallWizard } from './InstallWizard'

import type { InstallWizardState } from '@/atoms/marketplace'

function makeWizardStore(overrides: Partial<InstallWizardState> = {}) {
  const store = createStore()
  store.set(installWizardAtom, {
    step: 'config',
    slug: 'test-skill',
    appType: 'skill',
    spaceId: null,
    userConfig: {},
    progress: null,
    error: null,
    ...overrides,
  })
  return store
}

describe('InstallWizard — type-aware step sequence', () => {
  test('skill item skips the scope step', () => {
    const store = makeWizardStore({ appType: 'skill', step: 'config' })
    const { queryByText } = renderWithProviders(<InstallWizard />, { store })
    // Scope step label must not appear for skill
    expect(queryByText('选择空间 (1/3)')).not.toBeInTheDocument()
    // Config is step 1/2 for skill (no scope step)
    expect(queryByText('填写配置 (1/2)')).toBeInTheDocument()
  })

  test('mcp item skips the scope step', () => {
    const store = makeWizardStore({ appType: 'mcp', step: 'config' })
    const { queryByText } = renderWithProviders(<InstallWizard />, { store })
    expect(queryByText('选择空间 (1/3)')).not.toBeInTheDocument()
    expect(queryByText('填写配置 (1/2)')).toBeInTheDocument()
  })

  test('automation item still shows the scope step', () => {
    const store = makeWizardStore({ appType: 'automation', step: 'scope', spaceId: null })
    const { getByText } = renderWithProviders(<InstallWizard />, { store })
    // Scope step label IS present for automation
    expect(getByText('选择空间 (1/3)')).toBeInTheDocument()
  })

  test('skill stepper renders only 2 dots (config + confirm)', () => {
    const store = makeWizardStore({ appType: 'skill', step: 'config' })
    const { container } = renderWithProviders(<InstallWizard />, { store })
    // The stepper dots are rendered as div.w-2.h-2.rounded-full — one per visible step
    const dots = container.querySelectorAll('.w-2.h-2.rounded-full')
    expect(dots).toHaveLength(2)
  })

  test('automation stepper renders 3 dots (scope + config + confirm)', () => {
    const store = makeWizardStore({ appType: 'automation', step: 'scope' })
    const { container } = renderWithProviders(<InstallWizard />, { store })
    const dots = container.querySelectorAll('.w-2.h-2.rounded-full')
    expect(dots).toHaveLength(3)
  })
})
