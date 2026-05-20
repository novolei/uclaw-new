import { invoke } from '@tauri-apps/api/core'

// ─── 类型（镜像 Rust browse.rs）──────────────────────────────────────────

export interface PageSummary {
  slug: string
  title: string
  type: string
  updated_at: string | null
}

export interface PageDetail {
  slug: string
  title: string
  type: string
  compiled_truth: string
  frontmatter: unknown
  created_at: string | null
  updated_at: string | null
  tags: string[]
  raw_markdown: string
}

export interface SearchHit {
  slug: string
  title: string
  snippet: string
  similarity: number
}

export interface Backlink {
  from_slug: string
  link_type: string
}

export interface VersionMeta {
  id: number
  snapshot_at: string | null
  compiled_truth: string
}

export interface BrainStats {
  page_count: number
  chunk_count: number
  embedded_count: number
  link_count: number
  tag_count: number
}

export interface OrphanSummary {
  total_orphans: number
  total_pages: number
}

// 命令返回的稳定错误前缀（前端按此分支空状态）
export const GBRAIN_NOT_CONNECTED = 'gbrain_not_connected'

// ─── invoke 包装 ─────────────────────────────────────────────────────────

export const gbrainListPages = (params: {
  limit?: number
  sort?: string
  pageType?: string
  tag?: string
  updatedAfter?: string
}): Promise<PageSummary[]> =>
  invoke('gbrain_list_pages', {
    limit: params.limit,
    sort: params.sort,
    pageType: params.pageType,
    tag: params.tag,
    updatedAfter: params.updatedAfter,
  })

export const gbrainGetPage = (slug: string): Promise<PageDetail> =>
  invoke('gbrain_get_page', { slug })

export const gbrainSearch = (
  query: string,
  limit = 20,
  offset = 0,
): Promise<SearchHit[]> => invoke('gbrain_search', { query, limit, offset })

export const gbrainGetBacklinks = (slug: string): Promise<Backlink[]> =>
  invoke('gbrain_get_backlinks', { slug })

export const gbrainTraverseGraph = (
  slug: string,
  depth = 2,
  direction?: string,
): Promise<unknown> => invoke('gbrain_traverse_graph', { slug, depth, direction })

export const gbrainGetVersions = (slug: string): Promise<VersionMeta[]> =>
  invoke('gbrain_get_versions', { slug })

export const gbrainRevertVersion = (
  slug: string,
  versionId: number,
): Promise<PageDetail> =>
  invoke('gbrain_revert_version', { slug, versionId })

export const gbrainPutPage = (
  slug: string,
  content: string,
): Promise<PageDetail> => invoke('gbrain_put_page', { slug, content })

export const gbrainGetStats = (): Promise<BrainStats> =>
  invoke('gbrain_get_stats', {})

export const gbrainFindOrphans = (): Promise<OrphanSummary> =>
  invoke('gbrain_find_orphans', {})
