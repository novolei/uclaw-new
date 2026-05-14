import { describe, test, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders } from '@/test-utils/render'
import {
  marketplaceDetailAtom,
  marketplaceDetailLoadingAtom,
  marketplaceSelectedSlugAtom,
} from '@/atoms/marketplace'
import type { MarketplaceDetail } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  // Return a never-resolving promise by default; tests that need the component
  // to render with a pre-seeded atom will have detail already in the store
  // so the effect's result is discarded.
  getMarketplaceDetail: vi.fn().mockResolvedValue(undefined),
}))

// Stub motion/react so animations don't interfere with jsdom assertions
vi.mock('motion/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('motion/react')>()
  return {
    ...actual,
    AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
    // motion.div / motion.span etc. become plain HTML elements
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

import { StoreDetail } from './StoreDetail'
import * as React from 'react'

const makeDetail = (overrides: Partial<MarketplaceDetail> & { appType?: string } = {}): MarketplaceDetail => ({
  item: {
    slug: 'test-pkg',
    name: 'Test Package',
    version: '1.0.0',
    author: 'tester',
    description: 'A test package',
    appType: overrides.appType ?? 'automation',
    category: 'test',
    icon: null,
    tags: [],
    sizeBytes: null,
    minAppVersion: null,
    locale: null,
    i18n: {},
    ...((overrides.item) ?? {}),
  },
  specYaml: '',
  parsedSpecJson: null,
  requiresMcps: [],
  requiresSkills: [],
  installedVersion: null,
  ...overrides,
})

function seedStore(detail: MarketplaceDetail) {
  const store = createStore()
  store.set(marketplaceDetailAtom, detail)
  store.set(marketplaceDetailLoadingAtom, false)
  // Do NOT set marketplaceSelectedSlugAtom: the useEffect fires when slug changes
  // and would set loading=true, wiping out the pre-seeded state. Keeping slug null
  // means the effect returns early (if (!slug) return) and the pre-seeded detail renders.
  return store
}

describe('StoreDetail — skill layout', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  test('安装技能 button present for skill appType', () => {
    const detail = makeDetail({
      appType: 'skill',
      parsedSpecJson: { system_prompt: 'You are a helpful assistant.' },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('安装技能')).toBeInTheDocument()
  })

  test('依赖 tab absent for skill appType', () => {
    const detail = makeDetail({
      appType: 'skill',
      parsedSpecJson: { system_prompt: 'Do stuff.' },
    })
    const { queryByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(queryByText('依赖')).not.toBeInTheDocument()
  })

  test('skill with no config_schema has no 配置 tab', () => {
    const detail = makeDetail({
      appType: 'skill',
      parsedSpecJson: { system_prompt: 'Do stuff.' },
    })
    const { queryByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(queryByText('配置')).not.toBeInTheDocument()
  })

  test('skill with config_schema shows 配置 tab', () => {
    const detail = makeDetail({
      appType: 'skill',
      parsedSpecJson: {
        system_prompt: 'Do stuff.',
        config_schema: [{ key: 'api_key', label: 'API Key', type: 'string', required: true }],
      },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('配置')).toBeInTheDocument()
  })

  test('skill with update shows upgrade affordance button', () => {
    const detail = makeDetail({
      appType: 'skill',
      parsedSpecJson: { system_prompt: 'Do stuff.' },
      installedVersion: '0.9.0',
      item: {
        slug: 'test-skill',
        name: 'Test Skill',
        version: '1.0.0',
        author: 'tester',
        description: 'A skill',
        appType: 'skill',
        category: 'test',
        icon: null,
        tags: [],
        sizeBytes: null,
        minAppVersion: null,
        locale: null,
        i18n: {},
      },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    // When isInstalled && hasUpdate → upgrade button
    expect(getByText(/升级到 v1\.0\.0/)).toBeInTheDocument()
  })
})

describe('StoreDetail — mcp layout', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  test('安装 MCP button present for mcp appType', () => {
    const detail = makeDetail({
      appType: 'mcp',
      parsedSpecJson: {
        mcp_server: { command: 'npx', args: ['-y', 'pg'] },
      },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('安装 MCP')).toBeInTheDocument()
  })

  test('mcp_server command rendered in overview panel', () => {
    const detail = makeDetail({
      appType: 'mcp',
      parsedSpecJson: {
        mcp_server: { command: 'npx', args: ['-y', 'pg'] },
      },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('npx')).toBeInTheDocument()
  })

  test('mcp args rendered in overview panel', () => {
    const detail = makeDetail({
      appType: 'mcp',
      parsedSpecJson: {
        mcp_server: { command: 'npx', args: ['-y', 'pg-mcp-server'] },
      },
    })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('-y pg-mcp-server')).toBeInTheDocument()
  })

  test('依赖 tab absent for mcp appType', () => {
    const detail = makeDetail({
      appType: 'mcp',
      parsedSpecJson: { mcp_server: { command: 'uvx', args: [] } },
    })
    const { queryByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(queryByText('依赖')).not.toBeInTheDocument()
  })

  test('mcp with no config_schema has no 配置 tab', () => {
    const detail = makeDetail({
      appType: 'mcp',
      parsedSpecJson: { mcp_server: { command: 'uvx', args: [] } },
    })
    const { queryByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(queryByText('配置')).not.toBeInTheDocument()
  })
})

describe('StoreDetail — automation layout (regression)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  test('automation shows 安装 button when not installed', () => {
    const detail = makeDetail({ appType: 'automation' })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('安装')).toBeInTheDocument()
  })

  test('automation shows all four tabs', () => {
    const detail = makeDetail({ appType: 'automation' })
    const { getByText } = renderWithProviders(<StoreDetail />, { store: seedStore(detail) })
    expect(getByText('概览')).toBeInTheDocument()
    expect(getByText('配置')).toBeInTheDocument()
    expect(getByText('依赖')).toBeInTheDocument()
    expect(getByText('提示词')).toBeInTheDocument()
  })
})
