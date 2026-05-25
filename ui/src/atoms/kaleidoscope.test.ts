import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { topLevelViewAtom } from './top-level-view'
import {
  kaleidoscopeModuleAtom,
  KALEIDOSCOPE_MODULES,
  selectedBuiltinIntegrationAtom,
} from './kaleidoscope'

describe('top-level-view / kaleidoscope atoms', () => {
  it('topLevelViewAtom defaults to "workspace"', () => {
    const store = createStore()
    expect(store.get(topLevelViewAtom)).toBe('workspace')
  })

  it('kaleidoscopeModuleAtom defaults to "humans"', () => {
    const store = createStore()
    expect(store.get(kaleidoscopeModuleAtom)).toBe('humans')
  })

  it('KALEIDOSCOPE_MODULES lists 8 modules: 4 asset + 4 capability', () => {
    expect(KALEIDOSCOPE_MODULES).toHaveLength(8)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'asset')).toHaveLength(4)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'capability')).toHaveLength(4)
  })

  it('every module id is unique and humans is first', () => {
    const ids = KALEIDOSCOPE_MODULES.map((m) => m.id)
    expect(new Set(ids).size).toBe(8)
    expect(ids[0]).toBe('humans')
  })

  it('can route to the Playwright MCP built-in integration', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'integrations')
    store.set(selectedBuiltinIntegrationAtom, 'playwright_mcp')

    expect(store.get(kaleidoscopeModuleAtom)).toBe('integrations')
    expect(store.get(selectedBuiltinIntegrationAtom)).toBe('playwright_mcp')
  })
})
