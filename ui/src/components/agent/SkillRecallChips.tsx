/**
 * SkillRecallChips — renders chips for each skill_search / load_skill
 * invocation in the current session. Distinct from SkillCitationChips
 * (which renders post-application "applied skill X" pills).
 *
 * See docs/superpowers/specs/2026-05-12-skill-recall-design.md §6.
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Search, BookOpen } from 'lucide-react'
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

export function SkillRecallChips({ sessionId, className }: SkillRecallChipsProps): React.ReactElement | null {
  const recallsMap = useAtomValue(skillRecallsMapAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  const recalls = recallsMap.get(sessionId) ?? []
  if (recalls.length === 0) return null

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
