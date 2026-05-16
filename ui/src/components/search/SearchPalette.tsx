/**
 * SearchPalette — global ⌘K command palette.
 *
 * Two modes:
 *   - Empty input  → browse: recent threads + settings shortcuts + workspaces
 *   - Typing       → filter the same three sections client-side, plus show
 *                    server-side FTS results for content matches
 *
 * Toggle via Cmd/Ctrl+K. Esc / backdrop click closes. cmdk handles arrow-
 * key navigation + aria-selected highlight; we do all filtering manually.
 *
 * Visual design ports if2Ai's GlobalSearch:
 *   - Frosted-glass panel with layered shadow
 *   - Group headings in tracked uppercase
 *   - Workspace badges + relative-time chips on thread rows
 *   - Footer with kbd keyboard hints
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { Command } from 'cmdk'
import {
  Search,
  MessageSquare,
  Bot,
  Folder,
  FolderOpen,
  Clock,
  SlidersHorizontal,
  Brain,
  Settings as SettingsIcon,
  Hash,
  type LucideIcon,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { searchPaletteOpenAtom, searchPaletteScopeAtom } from '@/atoms/search-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import { groupHitsByWorkspace } from '@/lib/group-search-hits'
import { listRecentThreads, listSpaces, searchFragments } from '@/lib/tauri-bridge'
import type { FragmentSearchHit, FragmentItem } from '@/lib/tauri-bridge'
import type { RecentThread } from '@/lib/agent-types'
import type { SpaceSummary } from '@/lib/types'
import { FragmentDetailPopover } from '@/components/memory/FragmentDetailPopover'
import { SUBTYPE_COLORS } from '@/components/memory/FragmentCard'

// ===== Types =====

interface SearchHit {
  id: string
  title: string
  snippet: string
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'agent_message' | 'file'
  sourceId: string
  messageId?: string
  workspaceId?: string
  createdAt: string
}

type WorkspaceSummary = SpaceSummary

interface SettingsItem {
  id: string
  label: string
  hint: string
  icon: LucideIcon
}

const SETTINGS_ITEMS: SettingsItem[] = [
  {
    id: 'settings:providers',
    label: '服务商配置',
    hint: 'Provider / API Key / Base URL',
    icon: SlidersHorizontal,
  },
  {
    id: 'settings:models',
    label: '模型配置',
    hint: '主聊天模型 / Thinking 支持',
    icon: Brain,
  },
  {
    id: 'settings:memory',
    label: '记忆设置',
    hint: 'Memory / 编译 / 晋升',
    icon: Brain,
  },
  {
    id: 'settings:appearance',
    label: '外观设置',
    hint: '主题 / 字体 / 衬线',
    icon: SettingsIcon,
  },
]

const MAX_RECENT_BROWSE = 8
const MAX_RECENT_SEARCH = 5
const FTS_DEBOUNCE_MS = 150

// ===== Helpers =====

function formatAge(updatedAt: string): string {
  // updatedAt is either RFC3339 (chat) or i64 ms (agent)
  let ts: number
  const asNum = Number(updatedAt)
  if (Number.isFinite(asNum) && asNum > 1_000_000_000_000) {
    ts = asNum
  } else {
    const parsed = Date.parse(updatedAt)
    if (Number.isNaN(parsed)) return ''
    ts = parsed
  }
  const ageMs = Date.now() - ts
  if (ageMs < 60_000) return '刚刚'
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)}分钟前`
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)}小时前`
  return `${Math.floor(ageMs / 86_400_000)}天前`
}

// ===== Component =====

export interface SearchPaletteProps {
  onSelect?: (item:
    | { kind: 'thread'; thread: RecentThread }
    | { kind: 'workspace'; workspace: WorkspaceSummary }
    | { kind: 'settings'; settings: SettingsItem }
    | { kind: 'search_hit'; hit: SearchHit }
  ) => void
}

export function SearchPalette({ onSelect }: SearchPaletteProps): React.ReactElement | null {
  const [open, setOpen] = useAtom(searchPaletteOpenAtom)
  const [scope, setScope] = useAtom(searchPaletteScopeAtom)
  const appMode = useAtomValue(appModeAtom)
  const currentConversationId = useAtomValue(currentConversationIdAtom)
  const currentAgentSessionId = useAtomValue(currentAgentSessionIdAtom)

  const activeSessionTarget = React.useMemo<{
    id: string
    label: string
  } | null>(() => {
    if (appMode === 'agent' && currentAgentSessionId) {
      return { id: currentAgentSessionId, label: '当前 Agent 会话' }
    }
    if (appMode === 'chat' && currentConversationId) {
      return { id: currentConversationId, label: '当前聊天' }
    }
    return null
  }, [appMode, currentConversationId, currentAgentSessionId])

  const [query, setQuery] = React.useState('')
  const [recents, setRecents] = React.useState<RecentThread[]>([])
  const [workspaces, setWorkspaces] = React.useState<WorkspaceSummary[]>([])
  const [hits, setHits] = React.useState<SearchHit[]>([])
  const [fragmentResults, setFragmentResults] = React.useState<FragmentSearchHit[]>([])
  const [searching, setSearching] = React.useState(false)
  const debounceRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)
  const [selectedFragment, setSelectedFragment] = React.useState<FragmentItem | null>(null)
  const [detailOpen, setDetailOpen] = React.useState(false)

  // Global ⌘K toggle + Tab scope toggle + two-step Esc
  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Outside the palette: ⌘K / Ctrl-K opens.
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen((v) => !v)
        return
      }
      if (!open) return
      if (e.key === 'Escape') {
        e.preventDefault()
        // Two-step Esc: first clear scope, second close.
        if (scope !== 'all') {
          setScope('all')
        } else {
          setOpen(false)
        }
        return
      }
      if (e.key === 'Tab' && activeSessionTarget) {
        e.preventDefault()
        setScope((s) =>
          s === 'all'
            ? { kind: 'session', id: activeSessionTarget.id, label: activeSessionTarget.label }
            : 'all',
        )
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, setOpen, scope, setScope, activeSessionTarget])

  // Reset query + scope when palette closes
  React.useEffect(() => {
    if (!open) {
      setQuery('')
      setScope('all')
    }
  }, [open, setScope])

  // Fetch browse data on open
  React.useEffect(() => {
    if (!open) return
    let cancelled = false
    Promise.all([
      listRecentThreads().catch(() => [] as RecentThread[]),
      listSpaces().catch(() => [] as WorkspaceSummary[]),
    ]).then(([r, w]) => {
      if (cancelled) return
      setRecents(r)
      setWorkspaces(w as WorkspaceSummary[])
    })
    return () => { cancelled = true }
  }, [open])

  // Debounced FTS search + fragment search
  React.useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    if (!open || query.trim().length < 1) {
      setHits([])
      setFragmentResults([])
      setSearching(false)
      return
    }
    setSearching(true)
    const trimmed = query.trim()
    const isFragmentOnly = trimmed.startsWith('@fragment ')
    const searchQuery = isFragmentOnly ? trimmed.slice(10).trim() : trimmed

    debounceRef.current = setTimeout(async () => {
      try {
        if (isFragmentOnly) {
          // @fragment 前缀: 仅搜索碎片
          const fragments = searchQuery.length > 0
            ? await searchFragments(searchQuery).catch(() => [] as FragmentSearchHit[])
            : []
          setFragmentResults(fragments.slice(0, 5))
          setHits([])
        } else {
          // 并行搜索: 常规 + 碎片
          const scopeArg = scope === 'all' ? null : `session:${scope.id}`
          const [result, fragments] = await Promise.all([
            invoke<SearchHit[]>('search_conversations', {
              input: { query: searchQuery, scope: scopeArg },
            }).catch(() => [] as SearchHit[]),
            searchFragments(searchQuery).catch(() => [] as FragmentSearchHit[]),
          ])
          setHits(result ?? [])
          setFragmentResults(fragments.slice(0, 5))
        }
      } catch (err) {
        console.error('[SearchPalette] search failed:', err)
        setHits([])
        setFragmentResults([])
      } finally {
        setSearching(false)
      }
    }, FTS_DEBOUNCE_MS)
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [open, query, scope])

  // Client-side filtering for the three browse sections
  const q = query.trim().toLowerCase()
  const filteredRecents = React.useMemo(() => {
    if (!q) return recents.slice(0, MAX_RECENT_BROWSE)
    return recents
      .filter(
        (t) =>
          t.title.toLowerCase().includes(q) ||
          t.workspaceName.toLowerCase().includes(q),
      )
      .slice(0, MAX_RECENT_SEARCH)
  }, [recents, q])
  const filteredWorkspaces = React.useMemo(() => {
    if (!q) return workspaces
    return workspaces.filter((w) => w.name.toLowerCase().includes(q))
  }, [workspaces, q])
  const filteredSettings = React.useMemo(() => {
    if (!q) return SETTINGS_ITEMS.slice(0, 3)
    return SETTINGS_ITEMS.filter((s) =>
      `${s.label} ${s.hint}`.toLowerCase().includes(q),
    )
  }, [q])

  const workspaceList = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)

  const hitGroups = React.useMemo(
    () => groupHitsByWorkspace(hits, workspaceList, activeWorkspaceId),
    [hits, workspaceList, activeWorkspaceId],
  )

  if (!open) return null

  const handleFragmentClick = (hit: FragmentSearchHit) => {
    const adapted: FragmentItem = {
      id: hit.id,
      title: hit.title,
      content: hit.snippet,
      source: hit.source,
      tags: hit.tags,
      createdAt: hit.createdAt,
    }
    setSelectedFragment(adapted)
    setDetailOpen(true)
  }

  const totalRendered =
    filteredRecents.length +
    filteredSettings.length +
    filteredWorkspaces.length +
    fragmentResults.length +
    hitGroups.reduce((sum, g) => sum + g.visibleHits.length, 0)

  const handle = (
    payload: Parameters<NonNullable<SearchPaletteProps['onSelect']>>[0],
  ) => {
    setOpen(false)
    onSelect?.(payload)
  }

  return (
    <div
      className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-foreground/10 backdrop-blur-sm"
      onClick={() => setOpen(false)}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'global-search-panel',
          'w-[min(92vw,640px)] mx-4 rounded-2xl border border-border/60',
          'bg-popover/95 backdrop-blur-2xl backdrop-saturate-150',
          'shadow-2xl shadow-black/20 ring-1 ring-black/5 dark:ring-white/10',
          'overflow-hidden',
        )}
      >
        <Command label="Global search" loop shouldFilter={false}>
          {/* Input row */}
          <div className="flex items-center gap-3 border-b border-border/50 px-4 py-3.5">
            <Search className="size-4 shrink-0 text-muted-foreground/60" />
            {scope !== 'all' && (
              <button
                type="button"
                onClick={() => setScope('all')}
                className="flex shrink-0 items-center gap-1.5 rounded-md bg-accent/60 px-2 py-1 text-[11.5px] text-accent-foreground/90 hover:bg-accent transition-colors"
                title="按 Esc 清除范围"
              >
                <Hash className="size-3" />
                {scope.label}
                <span className="text-accent-foreground/50">×</span>
              </button>
            )}
            <Command.Input
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder={scope === 'all' ? '搜索线程、项目...' : '在当前会话内搜索...'}
              className="flex-1 bg-transparent outline-none text-[13.5px] text-foreground placeholder:text-muted-foreground/40"
            />
            {searching && (
              <span className="text-[10.5px] text-muted-foreground/40 tabular-nums">…</span>
            )}
          </div>

          {/* Body */}
          <Command.List
            className={cn(
              'max-h-[440px] overflow-y-auto overflow-x-hidden px-1.5 py-1.5 scrollbar-thin',
              // Group headings
              '[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2',
              '[&_[cmdk-group-heading]]:text-[10.5px] [&_[cmdk-group-heading]]:font-semibold',
              '[&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-widest',
              '[&_[cmdk-group-heading]]:text-muted-foreground/70',
            )}
          >
            {totalRendered === 0 && q.length >= 1 && !searching ? (
              <Command.Empty className="flex flex-col items-center gap-2 py-10 text-center">
                <Hash className="size-6 text-muted-foreground/40" />
                <span className="text-[12.5px] text-muted-foreground/65">
                  未找到「{query}」相关内容
                </span>
              </Command.Empty>
            ) : null}

            {/* 1. Recent threads */}
            {filteredRecents.length > 0 && (
              <Command.Group heading={q ? '线程' : '最近线程'}>
                {filteredRecents.map((t) => (
                  <Command.Item
                    key={`thread:${t.kind}:${t.id}`}
                    value={`thread-${t.kind}-${t.id}`}
                    onSelect={() => handle({ kind: 'thread', thread: t })}
                    className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
                  >
                    {t.titleEmoji ? (
                      <span className="size-4 shrink-0 text-center text-[14px] leading-none">
                        {t.titleEmoji}
                      </span>
                    ) : t.kind === 'agent' ? (
                      <Bot className="size-4 shrink-0 text-muted-foreground/75" />
                    ) : (
                      <MessageSquare className="size-4 shrink-0 text-muted-foreground/75" />
                    )}
                    <span className="flex-1 truncate">{t.title}</span>
                    <span className="flex shrink-0 items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-[10.5px] text-muted-foreground/85 border border-border/40 max-w-[120px] truncate">
                      <Folder className="size-2.5 shrink-0" />
                      <span className="truncate">{t.workspaceName}</span>
                    </span>
                    <span className="flex shrink-0 items-center gap-1 text-[10.5px] text-muted-foreground/65 tabular-nums">
                      <Clock className="size-2.5" />
                      {formatAge(t.updatedAt)}
                    </span>
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {filteredRecents.length > 0 && filteredSettings.length > 0 && (
              <div className="mx-2 my-1 h-px bg-border/40" />
            )}

            {/* 2. Settings & commands */}
            {filteredSettings.length > 0 && (
              <Command.Group heading="设置与命令">
                {filteredSettings.map((s) => (
                  <Command.Item
                    key={s.id}
                    value={s.id}
                    onSelect={() => handle({ kind: 'settings', settings: s })}
                    className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
                  >
                    <s.icon className="size-4 shrink-0 text-muted-foreground/75" />
                    <span className="flex-1 truncate">{s.label}</span>
                    <span className="shrink-0 truncate text-[10.5px] text-muted-foreground/65 max-w-[280px]">
                      {s.hint}
                    </span>
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {(filteredRecents.length > 0 || filteredSettings.length > 0) && filteredWorkspaces.length > 0 && (
              <div className="mx-2 my-1 h-px bg-border/40" />
            )}

            {/* 3. Workspaces / projects */}
            {filteredWorkspaces.length > 0 && (
              <Command.Group heading="项目">
                {filteredWorkspaces.map((w) => {
                  const count = w.conversationCount ?? 0
                  return (
                    <Command.Item
                      key={`ws:${w.id}`}
                      value={`ws-${w.id}`}
                      onSelect={() => handle({ kind: 'workspace', workspace: w })}
                      className="relative flex cursor-pointer select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
                    >
                      <FolderOpen className="size-4 shrink-0 text-muted-foreground/75" />
                      <span className="flex-1 truncate">{w.icon ? `${w.icon} ` : ''}{w.name}</span>
                      {count > 0 && (
                        <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10.5px] text-muted-foreground/85 border border-border/40 tabular-nums">
                          {count} 个线程
                        </span>
                      )}
                    </Command.Item>
                  )
                })}
              </Command.Group>
            )}

            {hitGroups.length > 0 && (filteredRecents.length > 0 || filteredSettings.length > 0 || filteredWorkspaces.length > 0) && (
              <div className="mx-2 my-1 h-px bg-border/40" />
            )}

            {/* 4. 记忆碎片结果 */}
            {fragmentResults.length > 0 && (
              <>
                {(filteredRecents.length > 0 || filteredSettings.length > 0 || filteredWorkspaces.length > 0) && (
                  <div className="mx-2 my-1 h-px bg-border/40" />
                )}
                <Command.Group heading="记忆碎片">
                  {fragmentResults.slice(0, 5).map((hit) => (
                    <Command.Item
                      key={`fragment:${hit.id}`}
                      value={`fragment-${hit.id}`}
                      onSelect={() => handleFragmentClick(hit)}
                      className="relative flex cursor-pointer select-none items-start gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
                    >
                      <span 
                        className={cn(
                          'size-2 shrink-0 mt-1.5 rounded-full',
                          SUBTYPE_COLORS[hit.subtype || hit.tags?.[0] || 'daily']?.dot || 'bg-blue-500'
                        )}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="truncate font-medium text-foreground/85">
                          {hit.title || hit.snippet}
                        </div>
                        <div className="truncate text-[11.5px] text-muted-foreground/65">
                          {hit.snippet}
                        </div>
                      </div>
                    </Command.Item>
                  ))}
                </Command.Group>
              </>
            )}

            {/* 5. Server-side FTS hits — grouped by workspace */}
            {hitGroups.map((group) => {
              const Icon = getWorkspaceIcon(group.workspaceIcon)
              return (
                <Command.Group
                  key={`ws-group-${group.workspaceId}`}
                  heading={`${group.workspaceName} · ${group.hits.length}`}
                >
                  <div className="flex items-center gap-1.5 px-2.5 pt-1 pb-0.5" aria-hidden="true">
                    <span className="inline-flex items-center justify-center size-4 rounded bg-primary/15 text-primary">
                      <Icon className="size-3" />
                    </span>
                  </div>
                  {group.visibleHits.map((h) => (
                    <Command.Item
                      key={`hit:${h.id}`}
                      value={`hit-${h.id}`}
                      onSelect={() => handle({ kind: 'search_hit', hit: h as SearchHit })}
                      className="relative flex cursor-pointer select-none items-start gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] text-foreground/80 outline-none transition-colors aria-selected:bg-accent aria-selected:text-accent-foreground aria-selected:ring-1 aria-selected:ring-border/40"
                    >
                      {h.source === 'agent_turn' || h.source === 'agent_message' ? (
                        <Bot className="size-4 shrink-0 mt-0.5 text-muted-foreground/75" />
                      ) : (
                        <MessageSquare className="size-4 shrink-0 mt-0.5 text-muted-foreground/75" />
                      )}
                      <div className="flex-1 min-w-0">
                        <div className="truncate font-medium text-foreground/85">
                          {h.title || '(untitled)'}
                        </div>
                        <div
                          className="truncate text-[11.5px] text-muted-foreground/65"
                          dangerouslySetInnerHTML={{ __html: h.snippet }}
                        />
                      </div>
                    </Command.Item>
                  ))}
                  {group.overflowCount > 0 && (
                    <div className="px-2.5 py-1 text-[10.5px] text-muted-foreground/60 italic">
                      在该工作区内还有 {group.overflowCount} 条
                    </div>
                  )}
                </Command.Group>
              )
            })}
          </Command.List>

          {/* Footer */}
          <div className="global-search-footer flex items-center justify-end gap-3 border-t border-border/50 bg-muted/30 px-3.5 py-2 text-[10.5px] text-muted-foreground/75">
            {activeSessionTarget && (
              <span className="flex items-center gap-1">
                <kbd className="rounded bg-muted px-1 py-0.5 font-mono text-[10px] text-muted-foreground border border-border/40">Tab</kbd>
                {scope === 'all' ? '限定当前会话' : '取消限定'}
              </span>
            )}
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-muted text-muted-foreground border border-border/40 px-1 py-0.5 font-mono text-[10px]">↑↓</kbd>
              导航
            </span>
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-muted text-muted-foreground border border-border/40 px-1 py-0.5 font-mono text-[10px]">↵</kbd>
              打开
            </span>
            <span className="flex items-center gap-1">
              <kbd className="rounded bg-muted text-muted-foreground border border-border/40 px-1 py-0.5 font-mono text-[10px]">Esc</kbd>
              关闭
            </span>
          </div>
        </Command>
      </div>

      {/* 碎片详情浮层 */}
      <FragmentDetailPopover
        fragment={selectedFragment}
        open={detailOpen}
        onClose={() => { setDetailOpen(false); setSelectedFragment(null) }}
      />
    </div>
  )
}
