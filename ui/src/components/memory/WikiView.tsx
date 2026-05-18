/**
 * WikiView — AI Wiki tab.
 *
 * Memory OS Foundation Phase 3 (Task 3.4).
 *
 * Three regions, top-to-bottom:
 *   1. Header — workspace label, regenerate buttons, last-generated badge.
 *   2. Overview panel — collapsible, shows `wiki_artifacts(kind="overview")`
 *      rendered as markdown. Empty-state prompts the user to regenerate.
 *   3. Index + detail — two-column split. Left column lists every
 *      EntityPage in the space (grouped by subkind), right column shows
 *      the selected page's compiled_truth + timeline.
 *
 * Theme:
 *   All colours go through theme tokens (`bg-popover`, `text-muted-foreground`,
 *   `border-border`, …) so the 11 uClaw themes render correctly without
 *   per-theme styling.
 *
 * LLM stub awareness:
 *   When the WikiSynthesizer descriptor on the regenerate response says
 *   "stub:no-llm", the overview panel shows a "stub" badge so the user
 *   knows the markdown they're reading isn't a real synthesis yet.
 */

import * as React from 'react'
import ReactMarkdown from 'react-markdown'
import { Loader2, RefreshCw, ChevronRight, ChevronDown, FileText, Sparkles, Wand2, FolderDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Separator } from '@/components/ui/separator'
import { toast } from 'sonner'
import { cn, formatDateTime } from '@/lib/utils'
import {
  memoryWikiGetOverview,
  memoryWikiGetIndex,
  memoryWikiRegenerate,
  memoryEntityPageList,
  memoryEntityPageGet,
  memoryEntityPageSynthesizeNow,
  memoryWikiExport,
} from '@/lib/tauri-bridge'
import type {
  WikiArtifactDto,
  WikiRegenerateOutcome,
  EntityPageMetadata,
  EntitySynthesisOutcome,
} from '@/lib/types'

// ─── Types ──────────────────────────────────────────────────────────────

interface EntityPageSummary {
  nodeId: string
  title: string
  slug?: string
  subkind?: string
  updatedAt: string
}

interface WikiViewProps {
  spaceId?: string
  className?: string
}

// Display order for subkind groups in the index column. Future subkinds
// (anything not in this list) fall through to "Other".
const SUBKIND_DISPLAY_ORDER: ReadonlyArray<{ key: string; label: string }> = [
  { key: 'entity', label: 'Entity' },
  { key: 'concept', label: 'Concept' },
  { key: 'comparison', label: 'Comparison' },
  { key: 'question', label: 'Question' },
  { key: 'synthesis', label: 'Synthesis' },
  { key: 'decision', label: 'Decision' },
  { key: 'gap', label: 'Gap' },
  { key: 'default', label: 'Other' },
]

// ─── Component ──────────────────────────────────────────────────────────

export function WikiView({ spaceId, className }: WikiViewProps): React.ReactElement {
  const space = spaceId ?? 'default'
  const [overview, setOverview] = React.useState<WikiArtifactDto | null>(null)
  const [pages, setPages] = React.useState<EntityPageSummary[]>([])
  const [selectedNodeId, setSelectedNodeId] = React.useState<string | null>(null)
  const [overviewExpanded, setOverviewExpanded] = React.useState<boolean>(true)
  const [loadingPages, setLoadingPages] = React.useState<boolean>(true)
  const [loadingOverview, setLoadingOverview] = React.useState<boolean>(true)
  const [regenerating, setRegenerating] = React.useState<'index' | 'overview' | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [stubBadge, setStubBadge] = React.useState<boolean>(false)

  // ─── Fetchers ─────────────────────────────────────────────────────────

  const fetchOverview = React.useCallback(async () => {
    setLoadingOverview(true)
    try {
      const got = await memoryWikiGetOverview({ spaceId: space })
      setOverview(got)
      // The wire shape doesn't carry the synthesizer descriptor on read
      // (only on regenerate response). We infer the stub badge by
      // looking for the literal "stub" marker the StubSynthesizer emits.
      // Real-LLM artifacts won't have that header.
      setStubBadge(!!got && got.content.includes('# Wiki Overview (stub)'))
    } catch (e) {
      setError(`Failed to load overview: ${String(e)}`)
    } finally {
      setLoadingOverview(false)
    }
  }, [space])

  const fetchPages = React.useCallback(async () => {
    setLoadingPages(true)
    try {
      const list = (await memoryEntityPageList({ spaceId: space, limit: 200 })) as Array<{
        node: { id: string; title: string; metadata?: EntityPageMetadata; updatedAt: string }
      }>
      const summaries: EntityPageSummary[] = (list ?? []).map((d) => ({
        nodeId: d.node.id,
        title: d.node.title,
        slug: d.node.metadata?.slug,
        subkind: d.node.metadata?.subkind ?? 'default',
        updatedAt: d.node.updatedAt,
      }))
      setPages(summaries)
    } catch (e) {
      setError(`Failed to load entity pages: ${String(e)}`)
    } finally {
      setLoadingPages(false)
    }
  }, [space])

  React.useEffect(() => {
    void fetchOverview()
    void fetchPages()
  }, [fetchOverview, fetchPages])

  // ─── Regenerate handlers ──────────────────────────────────────────────

  const handleRegenerateIndex = async (): Promise<void> => {
    setRegenerating('index')
    setError(null)
    try {
      await memoryWikiRegenerate({ spaceId: space, kind: 'index' })
      await fetchPages()
    } catch (e) {
      setError(`Index regenerate failed: ${String(e)}`)
    } finally {
      setRegenerating(null)
    }
  }

  const handleRegenerateOverview = async (): Promise<void> => {
    setRegenerating('overview')
    setError(null)
    try {
      const outcome: WikiRegenerateOutcome = await memoryWikiRegenerate({
        spaceId: space,
        kind: 'overview',
      })
      // Use the descriptor returned by the IPC call to decide whether
      // the new overview is from the stub or a real LLM.
      setStubBadge(outcome.synthesizerDescriptor === 'stub:no-llm')
      await fetchOverview()
    } catch (e) {
      setError(`Overview regenerate failed: ${String(e)}`)
    } finally {
      setRegenerating(null)
    }
  }

  // Phase 7.1 — export all EntityPages to ~/Documents/workground/brain
  // as markdown files. Idempotent: unchanged pages short-circuit on
  // SHA-256 in the backend.
  const [exporting, setExporting] = React.useState(false)
  const handleExportToDisk = async (): Promise<void> => {
    if (exporting) return
    setExporting(true)
    setError(null)
    try {
      const outcome = await memoryWikiExport({ spaceId: space })
      const parts: string[] = []
      if (outcome.pages_written > 0)
        parts.push(`${outcome.pages_written} written`)
      if (outcome.pages_unchanged > 0)
        parts.push(`${outcome.pages_unchanged} unchanged`)
      if (outcome.overview_written) parts.push('overview')
      if (outcome.index_written) parts.push('index')
      const summary = parts.length > 0 ? parts.join(', ') : 'nothing to export'
      if (outcome.errors.length > 0) {
        toast.warning(
          `Exported with ${outcome.errors.length} error(s): ${summary}`,
        )
      } else {
        toast.success(`Exported to brain dir: ${summary}`)
      }
    } catch (e) {
      const msg = String(e)
      toast.error(`Export failed: ${msg}`)
      setError(msg)
    } finally {
      setExporting(false)
    }
  }

  // ─── Derived: pages grouped by subkind ────────────────────────────────

  const grouped = React.useMemo(() => {
    const buckets = new Map<string, EntityPageSummary[]>()
    for (const p of pages) {
      const k = p.subkind && p.subkind.length > 0 ? p.subkind : 'default'
      const arr = buckets.get(k) ?? []
      arr.push(p)
      buckets.set(k, arr)
    }
    // Sort within each bucket by title.
    for (const arr of buckets.values()) {
      arr.sort((a, b) => a.title.localeCompare(b.title))
    }
    return buckets
  }, [pages])

  // ─── Render ───────────────────────────────────────────────────────────

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-popover text-foreground',
        className,
      )}
      data-testid="wiki-view"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2">
          <FileText className="size-4 text-muted-foreground" />
          <span className="text-xs font-medium">Wiki · workspace `{space}`</span>
          {stubBadge && (
            <Badge
              variant="outline"
              className="text-[10px] px-1.5 py-0 border-amber-500/50 text-amber-500"
              title="Overview is from the stub synthesizer. Plug in a real LLM client to generate a true narrative."
            >
              stub LLM
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="text-xs h-7 gap-1"
            onClick={handleRegenerateIndex}
            disabled={regenerating !== null}
          >
            <RefreshCw
              className={cn('size-3', regenerating === 'index' && 'animate-spin')}
            />
            Index
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="text-xs h-7 gap-1"
            onClick={handleRegenerateOverview}
            disabled={regenerating !== null}
            title="Regenerate the overview via the configured WikiSynthesizer. Phase 3 ships a stub; replace with a real LLM client to get true synthesis."
          >
            <Sparkles
              className={cn('size-3', regenerating === 'overview' && 'animate-spin')}
            />
            Overview
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="text-xs h-7 gap-1"
            onClick={handleExportToDisk}
            disabled={exporting}
            title="Export every EntityPage in this workspace to ~/Documents/workground/brain/<subkind>/<slug>.md. Edit those files in Obsidian/VSCode, then sync back (Phase 7.2)."
          >
            {exporting ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <FolderDown className="size-3" />
            )}
            Export
          </Button>
        </div>
      </div>

      {/* Overview panel */}
      <div className="border-b border-border/50">
        <button
          type="button"
          className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted/40 transition-colors text-xs"
          onClick={() => setOverviewExpanded((v) => !v)}
          aria-expanded={overviewExpanded}
        >
          {overviewExpanded ? (
            <ChevronDown className="size-3 text-muted-foreground" />
          ) : (
            <ChevronRight className="size-3 text-muted-foreground" />
          )}
          <span className="font-medium">Overview</span>
          {overview && (
            <span className="text-[10px] text-muted-foreground/70 ml-auto">
              generated {formatDateTime(new Date(overview.generatedAt).toISOString())}
            </span>
          )}
        </button>
        {overviewExpanded && (
          <div className="px-4 py-2 bg-muted/20">
            {loadingOverview ? (
              <div className="flex items-center gap-2 py-2 text-xs text-muted-foreground">
                <Loader2 className="size-3 animate-spin" />
                Loading overview…
              </div>
            ) : overview ? (
              <ScrollArea className="max-h-48">
                <div className="prose prose-sm dark:prose-invert max-w-none text-xs">
                  <ReactMarkdown>{overview.content}</ReactMarkdown>
                </div>
              </ScrollArea>
            ) : (
              <div className="text-xs text-muted-foreground py-2">
                No overview yet.{' '}
                <button
                  className="underline hover:text-foreground"
                  onClick={handleRegenerateOverview}
                  disabled={regenerating !== null}
                >
                  Generate one
                </button>
                .
              </div>
            )}
          </div>
        )}
      </div>

      {/* Error banner */}
      {error && (
        <div className="px-3 py-1.5 bg-destructive/10 text-destructive text-xs">
          {error}
        </div>
      )}

      {/* Index + detail split */}
      <div className="flex flex-1 min-h-0">
        {/* Left column: index */}
        <div className="w-64 border-r border-border/50 flex flex-col">
          <div className="px-3 py-1.5 text-xs font-medium text-muted-foreground border-b border-border/30">
            Index ({pages.length})
          </div>
          {loadingPages && pages.length === 0 ? (
            <div className="flex items-center justify-center py-10">
              <Loader2 className="size-3 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <ScrollArea className="flex-1">
              <div className="p-2 space-y-3">
                {SUBKIND_DISPLAY_ORDER.map(({ key, label }) => {
                  const entries = grouped.get(key)
                  if (!entries || entries.length === 0) return null
                  return (
                    <IndexGroup
                      key={key}
                      label={label}
                      entries={entries}
                      selectedNodeId={selectedNodeId}
                      onSelect={setSelectedNodeId}
                    />
                  )
                })}
                {/* Any subkinds outside the canonical list */}
                {Array.from(grouped.entries())
                  .filter(([k]) => !SUBKIND_DISPLAY_ORDER.some((s) => s.key === k))
                  .map(([k, entries]) => (
                    <IndexGroup
                      key={k}
                      label={k}
                      entries={entries}
                      selectedNodeId={selectedNodeId}
                      onSelect={setSelectedNodeId}
                    />
                  ))}
                {pages.length === 0 && !loadingPages && (
                  <div className="text-xs text-muted-foreground px-2 py-4 text-center">
                    No entity pages yet. Use QuickCapture (Cmd+Shift+M) to
                    create one.
                  </div>
                )}
              </div>
            </ScrollArea>
          )}
        </div>

        {/* Right column: detail */}
        <div className="flex-1 min-w-0">
          {selectedNodeId ? (
            <EntityPageDetail nodeId={selectedNodeId} />
          ) : (
            <div className="h-full flex items-center justify-center">
              <span className="text-xs text-muted-foreground">
                Select an entity page from the left to view its details.
              </span>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

// ─── IndexGroup ────────────────────────────────────────────────────────

function IndexGroup({
  label,
  entries,
  selectedNodeId,
  onSelect,
}: {
  label: string
  entries: EntityPageSummary[]
  selectedNodeId: string | null
  onSelect: (id: string) => void
}): React.ReactElement {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-muted-foreground/70 px-2 mb-1">
        {label} ({entries.length})
      </div>
      <div className="space-y-0.5">
        {entries.map((p) => {
          const selected = p.nodeId === selectedNodeId
          return (
            <button
              key={p.nodeId}
              type="button"
              className={cn(
                'w-full text-left px-2 py-1 text-xs rounded-sm transition-colors',
                'hover:bg-muted/60',
                selected && 'bg-accent text-accent-foreground',
              )}
              onClick={() => onSelect(p.nodeId)}
            >
              <span className="block truncate font-medium">{p.title}</span>
              {p.slug && (
                <span className="block truncate text-[10px] text-muted-foreground/70">
                  {p.slug}
                </span>
              )}
            </button>
          )
        })}
      </div>
    </div>
  )
}

// ─── EntityPageDetail ──────────────────────────────────────────────────

interface EntityPageDetailData {
  node: { id: string; title: string; metadata?: EntityPageMetadata; updatedAt: string; kind: string }
  activeVersion?: { content: string; createdAt: string } | null
}

function EntityPageDetail({ nodeId }: { nodeId: string }): React.ReactElement {
  const [data, setData] = React.useState<EntityPageDetailData | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)
  // Phase 6.3 — manual synthesis state. `synthesizing` blocks the
  // button during the LLM call; `lastSynthOutcome` retains the most
  // recent token cost / descriptor so the badge can show "real" vs
  // "stub" after the call resolves.
  const [synthesizing, setSynthesizing] = React.useState(false)
  const [lastSynthOutcome, setLastSynthOutcome] =
    React.useState<EntitySynthesisOutcome | null>(null)

  const loadDetail = React.useCallback(async () => {
    try {
      const detail = (await memoryEntityPageGet({ nodeId })) as EntityPageDetailData | null
      if (detail === null) {
        setError('Entity page not found (or its kind changed).')
        return
      }
      setData(detail)
      setError(null)
    } catch (e) {
      setError(`Failed to load: ${String(e)}`)
    }
  }, [nodeId])

  React.useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    setData(null)
    setLastSynthOutcome(null)
    void (async () => {
      try {
        const detail = (await memoryEntityPageGet({ nodeId })) as EntityPageDetailData | null
        if (!cancelled) {
          if (detail === null) {
            setError('Entity page not found (or its kind changed).')
          } else {
            setData(detail)
          }
        }
      } catch (e) {
        if (!cancelled) setError(`Failed to load: ${String(e)}`)
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [nodeId])

  const handleSynthesize = React.useCallback(async () => {
    if (synthesizing) return
    setSynthesizing(true)
    try {
      const outcome = await memoryEntityPageSynthesizeNow({ nodeId })
      setLastSynthOutcome(outcome)
      const isStub = outcome.synthesizer_descriptor.startsWith('stub')
      toast.success(
        isStub
          ? 'Synthesized (stub) — flip memory_os.entity_synthesizer_enabled to use a real LLM'
          : `Synthesized via ${outcome.llm_model ?? 'LLM'} (${outcome.token_cost} tokens)`,
      )
      // Refresh the detail panel so the new compiled_truth + aliases show.
      await loadDetail()
    } catch (e) {
      toast.error(`Synthesize failed: ${String(e)}`)
    } finally {
      setSynthesizing(false)
    }
  }, [nodeId, synthesizing, loadDetail])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      </div>
    )
  }
  if (error) {
    return (
      <div className="p-3 text-xs text-destructive">{error}</div>
    )
  }
  if (!data) return <></>

  const meta = data.node.metadata
  const timeline = meta?.timeline ?? []
  const compiledTruth = data.activeVersion?.content ?? ''

  return (
    <ScrollArea className="h-full">
      <div className="p-4 space-y-3">
        <div className="flex items-start justify-between gap-2 flex-wrap">
          <div className="flex items-center gap-2 flex-wrap flex-1 min-w-0">
            <h2 className="text-sm font-semibold">{data.node.title}</h2>
            {meta?.slug && (
              <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                {meta.slug}
              </Badge>
            )}
            {meta?.subkind && (
              <Badge variant="outline" className="text-[10px] px-1.5 py-0">
                {meta.subkind}
              </Badge>
            )}
            <TierBadge tier={meta?.enrichment_tier} />
            {lastSynthOutcome && (
              <Badge
                variant={
                  lastSynthOutcome.synthesizer_descriptor.startsWith('stub')
                    ? 'outline'
                    : 'default'
                }
                className="text-[10px] px-1.5 py-0"
                title={`Synthesizer: ${lastSynthOutcome.synthesizer_descriptor}`}
              >
                {lastSynthOutcome.synthesizer_descriptor.startsWith('stub')
                  ? 'stub synth'
                  : `${lastSynthOutcome.llm_model ?? 'llm'} · ${lastSynthOutcome.token_cost}t`}
              </Badge>
            )}
          </div>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-[11px] gap-1.5 shrink-0"
            onClick={handleSynthesize}
            disabled={synthesizing}
            title="Re-compile compiled_truth from the current timeline via the configured synthesizer (Stub or LLM)"
          >
            {synthesizing ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Wand2 className="size-3" />
            )}
            Synthesize
          </Button>
        </div>
        <div className="flex items-center gap-3 flex-wrap">
          <span className="text-[10px] text-muted-foreground">
            updated {formatDateTime(data.node.updatedAt)}
          </span>
          {meta?.last_synthesized_at && (
            <span className="text-[10px] text-muted-foreground">
              synthesized {formatDateTime(meta.last_synthesized_at)}
            </span>
          )}
          {meta?.last_escalated_at && (
            <span className="text-[10px] text-muted-foreground">
              tier reviewed {formatDateTime(meta.last_escalated_at)}
            </span>
          )}
          {meta?.aliases && meta.aliases.length > 0 && (
            <span className="text-[10px] text-muted-foreground">
              aliases: {meta.aliases.join(', ')}
            </span>
          )}
        </div>

        <Separator />

        {compiledTruth.trim().length === 0 ? (
          <div className="text-xs text-muted-foreground italic">
            No compiled_truth yet. Edit via QuickCapture or the EntityPage
            editor (Phase 9 will add inline editing).
          </div>
        ) : (
          <div className="prose prose-sm dark:prose-invert max-w-none text-xs">
            <ReactMarkdown>{compiledTruth}</ReactMarkdown>
          </div>
        )}

        {timeline.length > 0 && (
          <>
            <Separator />
            <div>
              <div className="text-xs font-medium mb-2">Timeline ({timeline.length})</div>
              <div className="space-y-1">
                {timeline
                  .slice()
                  .sort((a, b) => a.date.localeCompare(b.date))
                  .map((entry, i) => (
                    <div
                      key={`${entry.date}-${i}`}
                      className="flex gap-2 text-xs"
                    >
                      <span className="text-muted-foreground shrink-0 font-mono text-[10px] pt-0.5">
                        {entry.date}
                      </span>
                      <span className="flex-1 min-w-0">{entry.text}</span>
                    </div>
                  ))}
              </div>
            </div>
          </>
        )}

        {/* Phase 5 Task 5.4 — surface lint-detected contradictions.
            metadata.contradictions[] is populated by memory_lint when
            the analyzer detects two timeline entries that disagree. We
            render them as a separate, visually distinct section so the
            user can see at a glance that this page has unresolved
            conflicts. */}
        {meta?.contradictions && meta.contradictions.length > 0 && (
          <>
            <Separator />
            <div>
              <div className="text-xs font-medium mb-2 text-amber-500 flex items-center gap-1">
                <span>⚠</span>
                Contradictions ({meta.contradictions.length})
              </div>
              <div className="space-y-2">
                {meta.contradictions.map((c, i) => (
                  <div
                    key={`contradiction-${i}`}
                    className="border border-amber-500/40 rounded-sm p-2 bg-amber-500/5 text-xs"
                  >
                    <div className="grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5">
                      <span className="text-[10px] font-mono text-amber-500/80">A:</span>
                      <span>{c.claim_a}</span>
                      <span className="text-[10px] font-mono text-amber-500/80">B:</span>
                      <span>{c.claim_b}</span>
                    </div>
                    {c.between_source_ids && c.between_source_ids.length > 0 && (
                      <div className="text-[10px] text-muted-foreground/70 mt-1">
                        sources: {c.between_source_ids.map((s) => s.slice(0, 8)).join(' / ')}
                      </div>
                    )}
                    <div className="text-[10px] text-muted-foreground/50 mt-0.5">
                      noticed {c.noticed_at}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </>
        )}

        {/* Aliases are now rendered inline in the header metadata strip
            (Phase 6.3 layout) — the standalone bottom block is
            redundant. Kept the conditional render in the header so the
            visual treatment of aliases stays consistent with last_synthesized_at
            and last_escalated_at. */}
      </div>
    </ScrollArea>
  )
}

// ─── TierBadge ─────────────────────────────────────────────────────────
//
// Phase 6.3 — colour-coded enrichment_tier badge.
// Tier 1 (full):  accent — important hub
// Tier 2 (rich):  secondary — moderate enrichment
// Tier 3 (stub):  outline — initial state
// undefined:      no badge (page predates Phase 1 or tier_escalator
//                 hasn't run yet)
//
// Colours go through theme tokens so all 11 uClaw themes render
// consistently. The variant choice + a tiny title attribute give the
// user a tooltip explaining what each tier means.

function TierBadge({ tier }: { tier?: number }): React.ReactElement | null {
  if (tier === undefined || tier === null) return null
  const meta: Record<number, { variant: 'default' | 'secondary' | 'outline'; label: string; tooltip: string }> = {
    1: {
      variant: 'default',
      label: 'Tier 1 (full)',
      tooltip: '≥8 mentions — full LLM profile, frequent re-synthesis',
    },
    2: {
      variant: 'secondary',
      label: 'Tier 2 (rich)',
      tooltip: '3-7 mentions — LLM-written 200-500 char summary',
    },
    3: {
      variant: 'outline',
      label: 'Tier 3 (stub)',
      tooltip: '1-2 mentions — one-sentence stub, eligible for upgrade',
    },
  }
  const cfg = meta[tier] ?? { variant: 'outline' as const, label: `Tier ${tier}`, tooltip: 'Unknown tier' }
  return (
    <Badge variant={cfg.variant} className="text-[10px] px-1.5 py-0" title={cfg.tooltip}>
      {cfg.label}
    </Badge>
  )
}
