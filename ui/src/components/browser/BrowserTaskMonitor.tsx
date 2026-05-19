import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Brain, CheckCircle2, Eye, MousePointer2, RefreshCw, XCircle } from 'lucide-react'
import { browserTaskRunAtom, type BrowserTaskStepEntry, type BrowserTaskStepPhase } from '@/atoms/browser-atoms'
import { cn } from '@/lib/utils'

interface BrowserTaskMonitorProps {
  sessionId: string
}

function PhaseIcon({ phase, ok }: { phase: BrowserTaskStepPhase; ok: boolean }): React.ReactElement {
  if (!ok) return <XCircle className="size-3 text-destructive/80" />
  if (phase === 'observe') return <Eye className="size-3 text-sky-500/80" />
  if (phase === 'decide') return <Brain className="size-3 text-violet-500/80" />
  if (phase === 'recover') return <RefreshCw className="size-3 text-amber-500/80" />
  if (phase === 'done') return <CheckCircle2 className="size-3 text-emerald-500/80" />
  return <MousePointer2 className="size-3 text-primary/80" />
}

function stepText(step: BrowserTaskStepEntry): string {
  return step.error || step.message || step.reasoning || step.actionName
}

function StepRow({ step }: { step: BrowserTaskStepEntry }): React.ReactElement {
  const text = stepText(step)
  return (
    <div className="flex min-w-0 items-center gap-2 px-2 py-1 text-[11px]">
      <PhaseIcon phase={step.phase} ok={step.ok} />
      <span className="shrink-0 font-mono text-muted-foreground/45">
        {String(step.stepIndex).padStart(2, '0')}
      </span>
      <span className="shrink-0 text-muted-foreground/65">
        {step.actionName}
      </span>
      <span className={cn('min-w-0 flex-1 truncate', step.ok ? 'text-foreground/65' : 'text-destructive/80')}>
        {text}
      </span>
    </div>
  )
}

export function BrowserTaskMonitor({ sessionId }: BrowserTaskMonitorProps): React.ReactElement | null {
  const runMap = useAtomValue(browserTaskRunAtom)
  const run = runMap.get(sessionId)
  if (!run || run.steps.length === 0) return null

  const latest = run.steps.slice(-8)
  const done = run.status === 'completed'
  const failed = run.status === 'failed' || run.status === 'stopped'
  const needsUser = run.status === 'needs_user_intervention'

  return (
    <div className="border-t border-border/50 bg-background/95">
      <div className="flex items-center justify-between px-2.5 py-1 border-b border-border/30">
        <div className="flex min-w-0 items-center gap-1.5">
          <Brain className="size-3.5 text-muted-foreground/70" />
          <span className="truncate text-[12px] font-medium text-foreground/70">
            {run.task || 'Browser task'}
          </span>
        </div>
        <span className={cn(
          'shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium',
          done && 'bg-emerald-500/10 text-emerald-600',
          failed && 'bg-destructive/10 text-destructive',
          needsUser && 'bg-amber-500/10 text-amber-600',
          !done && !failed && !needsUser && 'bg-blue-500/10 text-blue-600',
        )}>
          {run.status}
        </span>
      </div>
      <div className="max-h-36 overflow-y-auto py-1">
        {latest.map((step) => (
          <StepRow key={step.stepIndex} step={step} />
        ))}
      </div>
    </div>
  )
}
