import { invoke } from '@tauri-apps/api/core'

// ─── Space ───────────────────────────────────────────────────────────────────
// All entity-page commands scope to a space. uClaw's memory_graph always uses
// "default" in the frontend today; the backend falls back to "default" when the
// field is omitted, but we pass it explicitly for clarity.

export const SPACE_ID = 'default'

// ─── Wire DTOs from memory_graph backend ─────────────────────────────────────
//
// These mirror the Rust structs in src-tauri/src/memory_graph/models.rs
// (all annotated #[serde(rename_all = "camelCase")]).

/** Minimal node struct nested inside MemoryNodeDetail. */
interface MemoryNode {
  id: string
  spaceId: string
  kind: string
  title: string
  metadata: Record<string, unknown> | null
  createdAt: string
  updatedAt: string
}

/** Active-version struct nested inside MemoryNodeDetail. */
interface MemoryVersion {
  id: string
  nodeId: string
  content: string
  createdAt: string
}

/** Full page detail returned by list / find_by_slug / put / revert. */
interface MemoryNodeDetail {
  node: MemoryNode
  activeVersion: MemoryVersion | null
  routes: unknown[]
  keywords: string[]
}

// ─── Exported TS DTOs (kept identical to the old gbrain-browse.ts shapes) ────
//
// WikiView imports these directly; keeping field names stable avoids cascading
// edits in the component.

export interface PageSummary {
  /** Stable slug derived from metadata.slug (or node.id as fallback). */
  slug: string
  title: string
  /** Subkind from metadata ("person", "concept", …) — maps to old `type` field. */
  type: string
  updated_at: string | null
  /** node_id exposed so callers can pass it to backlinks/versions/revert. */
  node_id: string
}

export interface PageDetail {
  slug: string
  title: string
  type: string
  /** Active version content — the full raw markdown stored in memory_graph. */
  compiled_truth: string
  frontmatter: unknown
  created_at: string | null
  updated_at: string | null
  tags: string[]
  raw_markdown: string
  node_id: string
}

export interface SearchHit {
  slug: string
  title: string
  snippet: string
  /** Not provided by memory_entity_page_search; set to 0 for compat. */
  similarity: number
}

export interface Backlink {
  /** Wire field name from EntityBacklink (camelCase → fromSlug). */
  from_slug: string
  link_type: string
}

export interface VersionMeta {
  /** Wire field: versionId (EntityPageVersionMeta). Stored as string from Rust. */
  id: string
  snapshot_at: string | null
  compiled_truth: string
}

export interface BrainStats {
  page_count: number
  chunk_count: number | null
  embedded_count: number | null
  /** Not available from memory_entity_page_stats — always 0. */
  link_count: number
  /** Not available from memory_entity_page_stats — always 0. */
  tag_count: number
}

export interface OrphanSummary {
  total_orphans: number
  /** Not returned by memory_entity_page_orphans; set to 0 for compat. */
  total_pages: number
}

// Retained for any other code that imports this sentinel, even though
// the "not connected" path is rephrased in WikiView to a generic error.
export const GBRAIN_NOT_CONNECTED = 'gbrain_not_connected'

// ─── Wire response types from the new commands ───────────────────────────────

interface WireEntityBacklink {
  fromSlug: string
  linkType: string
}

interface WireEntityPageVersionMeta {
  versionId: string
  createdAt: string
  content: string
}

interface WireEntityPageStats {
  pageCount: number
  chunkCount: number | null
  embeddedCount: number | null
}

interface WireEntityPageSummary {
  slug: string
  title: string
}

interface WireEntitySearchHit {
  slug: string
  title: string
  snippet: string
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Extract the canonical slug from a MemoryNode's metadata blob. */
function slugFromNode(node: MemoryNode): string {
  const meta = node.metadata as { slug?: string } | null
  return meta?.slug ?? node.id
}

/** Extract subkind (→ type field) from a MemoryNode's metadata blob. */
function subkindFromNode(node: MemoryNode): string {
  const meta = node.metadata as { subkind?: string } | null
  return meta?.subkind ?? ''
}

/** Map a MemoryNodeDetail to a PageSummary. */
function toPageSummary(d: MemoryNodeDetail): PageSummary {
  return {
    slug: slugFromNode(d.node),
    title: d.node.title,
    type: subkindFromNode(d.node),
    updated_at: d.node.updatedAt,
    node_id: d.node.id,
  }
}

/** Map a MemoryNodeDetail to a PageDetail. */
function toPageDetail(d: MemoryNodeDetail): PageDetail {
  const content = d.activeVersion?.content ?? ''
  return {
    slug: slugFromNode(d.node),
    title: d.node.title,
    type: subkindFromNode(d.node),
    compiled_truth: content,
    frontmatter: null,
    created_at: d.node.createdAt,
    updated_at: d.node.updatedAt,
    tags: d.keywords ?? [],
    raw_markdown: content,
    node_id: d.node.id,
  }
}

// ─── invoke wrappers ─────────────────────────────────────────────────────────

export const gbrainListPages = (_params: {
  limit?: number
  sort?: string
  pageType?: string
  tag?: string
  updatedAfter?: string
}): Promise<PageSummary[]> =>
  invoke<MemoryNodeDetail[]>('memory_entity_page_list', {
    input: {
      spaceId: SPACE_ID,
      limit: _params.limit ?? 100,
    },
  }).then((list) => list.map(toPageSummary))

export const gbrainGetPage = (slug: string): Promise<PageDetail> =>
  invoke<MemoryNodeDetail | null>('memory_entity_page_find_by_slug', {
    input: {
      spaceId: SPACE_ID,
      slug,
    },
  }).then((d) => {
    if (!d) throw new Error(`Page not found: ${slug}`)
    return toPageDetail(d)
  })

export const gbrainSearch = (
  query: string,
  limit = 20,
  _offset = 0,
): Promise<SearchHit[]> =>
  invoke<WireEntitySearchHit[]>('memory_entity_page_search', {
    input: {
      spaceId: SPACE_ID,
      query,
      limit,
    },
  }).then((hits) =>
    hits.map((h) => ({ slug: h.slug, title: h.title, snippet: h.snippet, similarity: 0 })),
  )

/**
 * Get backlinks for a node.
 * Accepts node_id (preferred — avoids an extra round-trip) or falls back to
 * resolving the slug via find_by_slug when called with a plain slug string.
 * WikiView passes node_id after loading the page detail.
 */
export const gbrainGetBacklinks = (nodeId: string): Promise<Backlink[]> =>
  invoke<WireEntityBacklink[]>('memory_entity_page_backlinks', {
    input: {
      nodeId,
    },
  }).then((links) =>
    links.map((l) => ({ from_slug: l.fromSlug, link_type: l.linkType })),
  )

export const gbrainGetStats = (): Promise<BrainStats> =>
  invoke<WireEntityPageStats>('memory_entity_page_stats', {
    input: {
      spaceId: SPACE_ID,
    },
  }).then((s) => ({
    page_count: s.pageCount,
    chunk_count: s.chunkCount,
    embedded_count: s.embeddedCount,
    link_count: 0,
    tag_count: 0,
  }))

export const gbrainFindOrphans = (): Promise<OrphanSummary> =>
  invoke<WireEntityPageSummary[]>('memory_entity_page_orphans', {
    input: {
      spaceId: SPACE_ID,
    },
  }).then((list) => ({
    total_orphans: list.length,
    total_pages: 0,
  }))

/**
 * Get version history for an EntityPage.
 * Accepts node_id (not slug — the underlying command takes a node_id).
 */
export const gbrainGetVersions = (nodeId: string): Promise<VersionMeta[]> =>
  invoke<WireEntityPageVersionMeta[]>('memory_entity_page_versions', {
    input: {
      nodeId,
    },
  }).then((versions) =>
    versions.map((v) => ({
      id: v.versionId,
      snapshot_at: v.createdAt,
      compiled_truth: v.content,
    })),
  )

/**
 * Revert to a prior version.
 * Both arguments are string IDs (the Rust DTO uses node_id + version_id).
 */
export const gbrainRevertVersion = (
  nodeId: string,
  versionId: string,
): Promise<PageDetail> =>
  invoke<MemoryNodeDetail>('memory_entity_page_revert', {
    input: {
      nodeId,
      versionId,
    },
  }).then(toPageDetail)

export const gbrainPutPage = (
  slug: string,
  content: string,
): Promise<PageDetail> =>
  invoke<MemoryNodeDetail>('memory_entity_page_put', {
    input: {
      spaceId: SPACE_ID,
      slug,
      rawMarkdown: content,
    },
  }).then(toPageDetail)

// ─── Types retained for import compat (DualNebulaView / buildUnifiedScene) ──

export interface KnowledgeNode {
  slug: string
  title: string
  type: string
}

export interface KnowledgeEdge {
  from_slug: string
  to_slug: string
  link_type: string
}

export interface KnowledgeGraph {
  nodes: KnowledgeNode[]
  edges: KnowledgeEdge[]
}

// ─── DualNebulaView graph fetch — native memory_entity_page_full_graph (Step 2c).
// gbrainTraverseGraph was never called in the FE (2a-left stub) — removed.
// gbrainFullGraph name is kept stable so MemoryModule.tsx needs no change.

export const gbrainFullGraph = (limit?: number): Promise<KnowledgeGraph> =>
  invoke('memory_entity_page_full_graph', { spaceId: null, limit })
