/**
 * SkillRecallChips — renders chips for each skill_search / load_skill
 * invocation in the current session. Distinct from SkillCitationChips
 * (which renders post-application "applied skill X" pills).
 *
 * Phase 4 (G13): When multiple skills from the same category appear in
 * search results, a ⚠️ conflict indicator is shown with a tooltip
 * listing the conflicting skill pairs.
 *
 * See docs/superpowers/specs/2026-05-12-skill-recall-design.md §6.
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Search, BookOpen, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { skillRecallsMapAtom, type SkillRecall } from '@/atoms/agent-atoms'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'

interface SkillRecallChipsProps {
  sessionId: string
  className?: string
}

interface ConflictGroup {
  category: string
  skills: string[]
}

/**
 * Detect category conflicts: when a single search result set contains
 * multiple skills with the same category, they may give contradictory
 * advice.
 */
function detectConflicts(recalls: SkillRecall[]): ConflictGroup[] {
  const byCategory = new Map<string, Set<string>>()

  for (const r of recalls) {
    if (r.kind === 'search' && r.results) {
      for (const result of r.results) {
        if (result.category) {
          const existing = byCategory.get(result.category)
          if (existing) {
            existing.add(result.name)
          } else {
            byCategory.set(result.category, new Set([result.name]))
          }
        }
      }
    }
  }

  return [...byCategory.entries()]
    .filter(([, names]) => names.size > 1)
    .map(([cat, names]) => ({ category: cat, skills: [...names] }))
}

export function SkillRecallChips({ sessionId, className }: SkillRecallChipsProps): React.ReactElement | null {
  const recallsMap = useAtomValue(skillRecallsMapAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  const recalls = recallsMap.get(sessionId) ?? []
  if (recalls.length === 0) return null

  const conflicts = detectConflicts(recalls)

  const handleClick = (): void => {
    setSettingsTab('tools')
    setSettingsOpen(true)
  }

  return (
    <div className={cn('flex flex-wrap gap-1.5 mt-2 pl-[46px]', className)}>
      <TooltipProvider delayDuration={200}>
        {recalls.map((r) => (
          <ChipFor key={r.toolCallId} recall={r} onClick={handleClick} />
        ))}
        {conflicts.length > 0 && (
          <ConflictIndicator conflicts={conflicts} />
        )}
      </TooltipProvider>
    </div>
  )
}

function ChipFor({ recall, onClick }: { recall: SkillRecall; onClick: () => void }): React.ReactElement {
  const isSearch = recall.kind === 'search'
  const Icon = isSearch ? Search : BookOpen
  const label = isSearch
    ? `搜索"${recall.query ?? ''}" → ${recall.results?.length ?? 0} 命中`
    : `加载"${recall.name ?? ''}"`
  const tooltipText = isSearch
    ? (recall.results && recall.results.length > 0
        ? recall.results.map((r) => `• ${r.name} (${r.provenance})`).join('\n')
        : '0 命中')
    : (recall.reason ?? '')

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
            'text-[11px] leading-tight',
            'bg-secondary/15 text-secondary-foreground border border-secondary/30',
            'hover:bg-secondary/25 hover:border-secondary/50',
            'transition-colors'
          )}
        >
          <Icon className="size-3" />
          <span className="truncate max-w-[280px]">{label}</span>
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText || '—'}
      </TooltipContent>
    </Tooltip>
  )
}

function ConflictIndicator({ conflicts }: { conflicts: ConflictGroup[] }): React.ReactElement {
  const tooltipText = [
    '检测到同类技能可能存在建议冲突：',
    ...conflicts.map(
      (c) => `• [${c.category}] ${c.skills.join(' vs ')}`
    ),
  ].join('\n')

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-[10px] text-amber-600 dark:text-amber-400 bg-amber-500/10 border border-amber-500/25 cursor-help">
          <AlertTriangle className="size-3" />
          冲突
        </span>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  )
}
