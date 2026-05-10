/**
 * SkillCitationChips — renders a row of pill-shaped chips beneath an
 * assistant message body, one per `> 应用技能：X — Y` citation found
 * by `parseSkillCitations`.
 *
 * Behavior:
 *   - Hover shows the LLM's reason (one-line tooltip).
 *   - Click navigates to Settings → 已学技能 (we don't yet jump to a
 *     specific skill row; that's nice-to-have for later).
 *   - On first mount per (messageKey, citation) pair, fires
 *     `recordSkillCited(title)` exactly once so the backend bumps
 *     `cited_count`. Dedupe key is module-level so a re-render doesn't
 *     double-count, and so streaming → finalized message doesn't
 *     double-count either.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import type { SkillCitation } from '@/lib/skill-citation'
import { recordSkillCited } from '@/lib/tauri-bridge'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'

interface SkillCitationChipsProps {
  citations: SkillCitation[]
  /** Stable identifier for the message — e.g. message id or `streaming-${sessionId}`.
   *  Used to dedupe `recordSkillCited` calls across re-renders. */
  messageKey: string
  className?: string
}

// Module-level dedup. Streaming + finalized message can both render
// the same citation; we only want one `recordSkillCited` per logical
// citation per page lifetime.
const recordedKeys = new Set<string>()

export function SkillCitationChips({
  citations,
  messageKey,
  className,
}: SkillCitationChipsProps): React.ReactElement | null {
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  React.useEffect(() => {
    if (citations.length === 0) return
    for (const c of citations) {
      const key = `${messageKey}::${c.title}`
      if (recordedKeys.has(key)) continue
      recordedKeys.add(key)
      // Fire-and-forget: bumping cited_count is best-effort observability,
      // never block UI on it. Failures get logged in the bridge layer.
      recordSkillCited(c.title).catch(() => {
        // Swallow — backend logs the actual error. UI shouldn't surface
        // a transient bump failure.
      })
    }
  }, [citations, messageKey])

  if (citations.length === 0) return null

  return (
    <div className={cn('flex flex-wrap gap-1.5 mt-2 pl-[46px]', className)}>
      <TooltipProvider delayDuration={200}>
        {citations.map((c, idx) => (
          <Tooltip key={`${c.title}-${idx}`}>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => {
                  setSettingsTab('skills')
                  setSettingsOpen(true)
                }}
                className={cn(
                  'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
                  'text-[11px] leading-tight',
                  'bg-primary/10 text-primary border border-primary/20',
                  'hover:bg-primary/15 hover:border-primary/30',
                  'transition-colors',
                )}
              >
                <Sparkles size={10} className="shrink-0" />
                <span className="max-w-[260px] truncate">{c.title}</span>
              </button>
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-[320px]">
              <div className="space-y-1">
                <div className="font-medium text-[12px]">应用了技能：{c.title}</div>
                <div className="text-[11px] text-muted-foreground">
                  原因：{c.reason}
                </div>
                <div className="text-[10px] text-muted-foreground/70">
                  点击打开「已学技能」设置
                </div>
              </div>
            </TooltipContent>
          </Tooltip>
        ))}
      </TooltipProvider>
    </div>
  )
}
