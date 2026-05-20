import * as React from 'react'
import ReactMarkdown from 'react-markdown'
import { Loader2, FileText, Search as SearchIcon, RefreshCw, Pencil, History, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import {
  gbrainListPages,
  gbrainGetPage,
  gbrainSearch,
  gbrainGetBacklinks,
  gbrainGetStats,
  gbrainFindOrphans,
  gbrainPutPage,
  gbrainGetVersions,
  gbrainRevertVersion,
  GBRAIN_NOT_CONNECTED,
  type PageSummary,
  type PageDetail,
  type SearchHit,
  type Backlink,
  type BrainStats,
  type OrphanSummary,
  type VersionMeta,
} from '@/lib/gbrain-browse'

interface WikiViewProps {
  spaceId?: string
  className?: string
}

function isNotConnected(e: unknown): boolean {
  return String(e).includes(GBRAIN_NOT_CONNECTED)
}

export function WikiView({ className }: WikiViewProps): React.ReactElement {
  const [pages, setPages] = React.useState<PageSummary[]>([])
  const [stats, setStats] = React.useState<BrainStats | null>(null)
  const [orphans, setOrphans] = React.useState<OrphanSummary | null>(null)
  const [selectedSlug, setSelectedSlug] = React.useState<string | null>(null)
  const [detail, setDetail] = React.useState<PageDetail | null>(null)
  const [backlinks, setBacklinks] = React.useState<Backlink[]>([])
  const [typeFilter, setTypeFilter] = React.useState<string>('')
  const [searchQuery, setSearchQuery] = React.useState<string>('')
  const [searchHits, setSearchHits] = React.useState<SearchHit[] | null>(null)
  const [loadingList, setLoadingList] = React.useState(true)
  const [loadingDetail, setLoadingDetail] = React.useState(false)
  const [notConnected, setNotConnected] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  // 防竞态：仅最后一次 openPage 的响应能写入 detail/backlinks。
  const latestSlugRef = React.useRef<string | null>(null)
  const [editing, setEditing] = React.useState(false)
  const [draft, setDraft] = React.useState('')
  const [saving, setSaving] = React.useState(false)
  const [versionsOpen, setVersionsOpen] = React.useState(false)
  const [versions, setVersions] = React.useState<VersionMeta[]>([])
  const [loadingVersions, setLoadingVersions] = React.useState(false)

  const loadList = React.useCallback(async () => {
    setLoadingList(true)
    setError(null)
    try {
      // gbrain 服务端把 list 上限钳到 100；V1 不做分页/“加载更多”（留待后续）。
      const list = await gbrainListPages({ limit: 100, sort: 'updated_desc' })
      setPages(list)
      setNotConnected(false)
    } catch (e) {
      if (isNotConnected(e)) {
        setNotConnected(true)
      } else {
        setError(`加载页面列表失败: ${String(e)}`)
      }
    } finally {
      setLoadingList(false)
    }
    try {
      setStats(await gbrainGetStats())
    } catch {
      setStats(null)
    }
    try {
      setOrphans(await gbrainFindOrphans())
    } catch {
      setOrphans(null)
    }
  }, [])

  React.useEffect(() => {
    void loadList()
  }, [loadList])

  const openPage = React.useCallback(async (slug: string) => {
    setSelectedSlug(slug)
    setLoadingDetail(true)
    setError(null)
    // 切页时丢弃未保存的编辑态与版本抽屉，避免把 A 页草稿存到 B 页。
    setEditing(false)
    setDraft('')
    setVersionsOpen(false)
    setVersions([])
    latestSlugRef.current = slug
    try {
      const [d, bl] = await Promise.all([
        gbrainGetPage(slug),
        gbrainGetBacklinks(slug).catch(() => [] as Backlink[]),
      ])
      if (latestSlugRef.current !== slug) return
      setDetail(d)
      setBacklinks(bl)
    } catch (e) {
      if (latestSlugRef.current !== slug) return
      setError(`加载页面失败: ${String(e)}`)
      setDetail(null)
    } finally {
      if (latestSlugRef.current === slug) setLoadingDetail(false)
    }
  }, [])

  const runSearch = React.useCallback(async () => {
    const q = searchQuery.trim()
    if (!q) {
      setSearchHits(null)
      return
    }
    setError(null)
    try {
      setSearchHits(await gbrainSearch(q, 30))
    } catch (e) {
      setError(`搜索失败: ${String(e)}`)
    }
  }, [searchQuery])

  const startEdit = React.useCallback(() => {
    if (!detail) return
    setDraft(detail.raw_markdown)
    setEditing(true)
  }, [detail])

  const saveEdit = React.useCallback(async () => {
    if (!detail) return
    setSaving(true)
    setError(null)
    try {
      const updated = await gbrainPutPage(detail.slug, draft)
      setDetail(updated)
      setEditing(false)
      gbrainGetBacklinks(detail.slug).then(setBacklinks).catch(() => {})
    } catch (e) {
      setError(`保存失败: ${String(e)}`)
    } finally {
      setSaving(false)
    }
  }, [detail, draft])

  const openVersions = React.useCallback(async () => {
    if (!detail) return
    setVersionsOpen(true)
    setLoadingVersions(true)
    try {
      setVersions(await gbrainGetVersions(detail.slug))
    } catch (e) {
      setError(`加载版本史失败: ${String(e)}`)
    } finally {
      setLoadingVersions(false)
    }
  }, [detail])

  const revertTo = React.useCallback(async (versionId: number) => {
    if (!detail) return
    try {
      const reverted = await gbrainRevertVersion(detail.slug, versionId)
      setDetail(reverted)
      setVersionsOpen(false)
    } catch (e) {
      setError(`回滚失败: ${String(e)}`)
    }
  }, [detail])

  const types = React.useMemo(
    () => Array.from(new Set(pages.map((p) => p.type).filter(Boolean))).sort(),
    [pages],
  )
  const filteredPages = React.useMemo(
    () => (typeFilter ? pages.filter((p) => p.type === typeFilter) : pages),
    [pages, typeFilter],
  )

  if (notConnected) {
    return (
      <div
        className={cn('flex flex-col items-center justify-center h-full bg-popover text-foreground gap-3', className)}
        data-testid="wiki-view"
      >
        <FileText className="size-8 text-muted-foreground" />
        <p className="text-sm text-muted-foreground">gbrain 未连接</p>
        <p className="text-xs text-muted-foreground">请到 设置 › 系统 检查 gbrain MCP 状态</p>
        <Button size="sm" variant="outline" onClick={() => void loadList()}>
          <RefreshCw className="size-3 mr-1" /> 重试
        </Button>
      </div>
    )
  }

  return (
    <div
      className={cn('relative flex flex-col h-full bg-popover text-foreground', className)}
      data-testid="wiki-view"
    >
      <div className="px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2 mb-2">
          <FileText className="size-4 text-muted-foreground" />
          <span className="text-xs font-medium">知识 Wiki · gbrain</span>
          {stats && (
            <span className="text-[10px] text-muted-foreground">
              {stats.page_count} 页 · {stats.chunk_count} 块 ·{' '}
              {stats.chunk_count > 0
                ? Math.round((stats.embedded_count / stats.chunk_count) * 100)
                : 0}
              % 已嵌入
            </span>
          )}
          {orphans && orphans.total_orphans > 0 && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0 border-amber-500/50 text-amber-500">
              {orphans.total_orphans} 孤儿页
            </Badge>
          )}
          <Button size="sm" variant="ghost" className="ml-auto h-7 text-xs gap-1" onClick={() => void loadList()}>
            <RefreshCw className="size-3" /> 刷新
          </Button>
        </div>
        <div className="flex items-center gap-1">
          <SearchIcon className="size-3 text-muted-foreground" />
          <input
            className="flex-1 bg-muted/20 rounded px-2 py-1 text-xs outline-none focus:bg-muted/40"
            placeholder="搜索知识库…"
            aria-label="搜索知识库"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void runSearch()
            }}
            data-testid="wiki-search-input"
          />
          {searchHits !== null && (
            <Button size="sm" variant="ghost" className="h-6 text-[10px]" onClick={() => { setSearchQuery(''); setSearchHits(null) }}>
              清除
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="px-3 py-1.5 bg-destructive/10 text-destructive text-xs">{error}</div>
      )}

      <div className="flex flex-1 min-h-0">
        <div className="w-64 border-r border-border/50 flex flex-col min-h-0">
          {searchHits === null && (
            <div className="px-2 py-1.5 border-b border-border/40">
              <select
                className="w-full bg-muted/20 rounded px-1.5 py-1 text-xs outline-none"
                value={typeFilter}
                onChange={(e) => setTypeFilter(e.target.value)}
                data-testid="wiki-type-filter"
              >
                <option value="">全部类型 ({pages.length})</option>
                {types.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
          )}
          <ScrollArea className="flex-1">
            {loadingList ? (
              <div className="flex items-center justify-center p-4">
                <Loader2 className="size-4 animate-spin text-muted-foreground" />
              </div>
            ) : searchHits !== null ? (
              searchHits.length === 0 ? (
                <p className="p-3 text-xs text-muted-foreground">无搜索结果</p>
              ) : (
                searchHits.map((h, i) => (
                  <button
                    type="button"
                    key={`${h.slug}-${i}`}
                    className={cn(
                      'w-full text-left px-3 py-1.5 text-xs hover:bg-muted/60',
                      selectedSlug === h.slug && 'bg-accent text-accent-foreground',
                    )}
                    onClick={() => void openPage(h.slug)}
                  >
                    <div className="font-medium truncate">{h.title || h.slug}</div>
                    <div className="text-[10px] text-muted-foreground truncate">{h.snippet}</div>
                  </button>
                ))
              )
            ) : filteredPages.length === 0 ? (
              <p className="p-3 text-xs text-muted-foreground">无页面</p>
            ) : (
              filteredPages.map((p) => (
                <button
                  type="button"
                  key={p.slug}
                  className={cn(
                    'w-full text-left px-3 py-1.5 text-xs hover:bg-muted/60',
                    selectedSlug === p.slug && 'bg-accent text-accent-foreground',
                  )}
                  onClick={() => void openPage(p.slug)}
                  data-testid="wiki-list-item"
                >
                  <div className="font-medium truncate">{p.title || p.slug}</div>
                  <div className="text-[10px] text-muted-foreground">{p.type}</div>
                </button>
              ))
            )}
          </ScrollArea>
        </div>

        <div className="flex-1 flex flex-col min-h-0">
          {loadingDetail ? (
            <div className="flex items-center justify-center flex-1">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : detail ? (
            <ScrollArea className="flex-1">
              <div className="p-4">
                <div className="flex items-center gap-2 mb-2">
                  <h2 className="text-sm font-semibold">{detail.title || detail.slug}</h2>
                  <Badge variant="outline" className="text-[10px]">{detail.type}</Badge>
                  <div className="ml-auto flex items-center gap-1">
                    {!editing && (
                      <>
                        <Button type="button" size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={startEdit} data-testid="wiki-edit-btn">
                          <Pencil className="size-3" /> 编辑
                        </Button>
                        <Button type="button" size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={() => void openVersions()} data-testid="wiki-versions-btn">
                          <History className="size-3" /> 版本史
                        </Button>
                      </>
                    )}
                  </div>
                </div>
                {editing ? (
                  <div className="flex flex-col gap-2">
                    <textarea
                      className="w-full min-h-[300px] bg-muted/20 rounded p-2 text-xs font-mono outline-none focus:bg-muted/30"
                      aria-label="编辑页面 markdown"
                      data-testid="wiki-editor"
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                    />
                    <div className="flex items-center gap-2">
                      <Button type="button" size="sm" className="h-7 text-xs" onClick={() => void saveEdit()} disabled={saving} data-testid="wiki-save-btn">
                        {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : null}
                        保存
                      </Button>
                      <Button type="button" size="sm" variant="ghost" className="h-7 text-xs" onClick={() => setEditing(false)} disabled={saving}>
                        取消
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="prose prose-sm dark:prose-invert max-w-none text-xs" data-testid="wiki-detail-body">
                    <ReactMarkdown>{detail.compiled_truth}</ReactMarkdown>
                  </div>
                )}
                <div className="mt-4 pt-3 border-t border-border/40">
                  <div className="text-[10px] uppercase text-muted-foreground mb-1">反向链接</div>
                  {backlinks.length === 0 ? (
                    <p className="text-xs text-muted-foreground">无反向链接</p>
                  ) : (
                    <div className="flex flex-col gap-0.5" data-testid="wiki-backlinks">
                      {backlinks.map((b, i) => (
                        <button
                          type="button"
                          key={`${b.from_slug}-${b.link_type}-${i}`}
                          className="text-left text-xs text-muted-foreground hover:text-foreground hover:underline"
                          onClick={() => void openPage(b.from_slug)}
                        >
                          · {b.from_slug} <span className="opacity-60">({b.link_type})</span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </ScrollArea>
          ) : (
            <div className="flex items-center justify-center flex-1">
              <p className="text-xs text-muted-foreground">选择一个页面查看</p>
            </div>
          )}
        </div>
      </div>
      {versionsOpen && (
        <div className="absolute inset-0 bg-background/80 flex justify-end" data-testid="wiki-version-drawer">
          <div className="w-80 h-full bg-popover border-l border-border/50 flex flex-col">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
              <span className="text-xs font-medium">版本史</span>
              <Button type="button" size="sm" variant="ghost" className="h-6 w-6 p-0" aria-label="关闭版本史" onClick={() => setVersionsOpen(false)}>
                <X className="size-3" />
              </Button>
            </div>
            <ScrollArea className="flex-1">
              {loadingVersions ? (
                <div className="flex justify-center p-4"><Loader2 className="size-4 animate-spin text-muted-foreground" /></div>
              ) : versions.length === 0 ? (
                <p className="p-3 text-xs text-muted-foreground">无历史版本</p>
              ) : (
                versions.map((v) => (
                  <div key={v.id} className="px-3 py-2 border-b border-border/30">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-muted-foreground">{v.snapshot_at ?? `#${v.id}`}</span>
                      <Button type="button" size="sm" variant="ghost" className="h-5 text-[10px]" onClick={() => void revertTo(v.id)}>
                        回滚到此版本
                      </Button>
                    </div>
                    <div className="text-[10px] text-muted-foreground mt-1 line-clamp-2">{v.compiled_truth.slice(0, 120)}</div>
                  </div>
                ))
              )}
            </ScrollArea>
          </div>
        </div>
      )}
    </div>
  )
}
