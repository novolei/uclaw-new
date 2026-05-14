import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import type { MarketplaceItem, MarketplaceUpdate, MarketplaceDetail } from '@/lib/tauri-bridge'

// Type filter — All / Digital Human / Skill / MCP
export type MarketplaceItemTypeFilter = 'all' | 'automation' | 'skill' | 'mcp'

// 3-sub-view atom for AutomationsView (我的数字人 / 我的应用 / 应用商店)
export type AutomationsSubview = 'humans' | 'apps' | 'store' | 'store-detail'

export const automationsSubviewAtom = atomWithStorage<AutomationsSubview>(
  'uclaw-automations-subview',
  'humans',
)

// Store filters — debounced search lives upstream in the StoreHeader component
export interface MarketplaceFilters {
  search: string
  itemType: MarketplaceItemTypeFilter
  category: string | null
}

export const marketplaceFiltersAtom = atom<MarketplaceFilters>({
  search: '',
  itemType: 'all',
  category: null,
})

// Paged item list (accumulator — page 0 replaces, page > 0 appends)
export const marketplaceItemsAtom = atom<MarketplaceItem[]>([])
export const marketplacePageAtom = atom<number>(0)
export const marketplaceHasMoreAtom = atom<boolean>(false)
export const marketplaceTotalAtom = atom<number>(0)
export const marketplaceLoadingAtom = atom<boolean>(false)
export const marketplaceLoadErrorAtom = atom<string | null>(null)

// Category counts for StoreHeader chips
export const marketplaceCategoryCountsAtom = atom<Record<string, number>>({})

// Current detail view target (null = grid view, slug = detail view)
export const marketplaceSelectedSlugAtom = atom<string | null>(null)
export const marketplaceDetailAtom = atom<MarketplaceDetail | null>(null)
export const marketplaceDetailLoadingAtom = atom<boolean>(false)

// Detail page sub-tab (概览 / 配置 / 依赖 / 提示词)
export type DetailSubTab = 'overview' | 'config' | 'requires' | 'prompt'
export const marketplaceDetailSubtabAtom = atom<DetailSubTab>('overview')

// Available updates badge (Updates check polling)
export const marketplaceUpdatesAtom = atom<MarketplaceUpdate[]>([])

// Install wizard state
export type InstallWizardStep = 'scope' | 'config' | 'confirm' | 'progress' | null

export interface InstallWizardState {
  step: InstallWizardStep
  slug: string | null
  spaceId: string | null
  userConfig: Record<string, unknown>
  progress: { phase: string; percent: number; message?: string } | null
  error: string | null
}

export const installWizardAtom = atom<InstallWizardState>({
  step: null,
  slug: null,
  spaceId: null,
  userConfig: {},
  progress: null,
  error: null,
})

// Sandbox try-install state (deferred to Phase 3b — atoms stay here for future use)
export interface SandboxState {
  active: boolean
  slug: string | null
  workspaceId: string | null
  startedAt: number | null
}
export const sandboxStateAtom = atom<SandboxState>({
  active: false,
  slug: null,
  workspaceId: null,
  startedAt: null,
})
