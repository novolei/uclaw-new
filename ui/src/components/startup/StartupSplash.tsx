import * as React from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  CircleDashed,
  Loader2,
  ShieldAlert,
  Settings,
  XCircle,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  clampStartupProgress,
  deriveStartupDoctorViewModel,
  type StartupDoctorCheck,
  type StartupDoctorCheckStatus,
  type StartupDoctorPhase,
  type StartupDoctorViewModel,
} from '@/lib/startup/startup-doctor'

export interface StartupSplashProps {
  viewModel?: StartupDoctorViewModel
  detailsExpanded?: boolean
  onDetailsExpandedChange?: (expanded: boolean) => void
  onOpenBrowserRuntimeSettings?: () => void
}

const phaseTone: Record<StartupDoctorPhase, string> = {
  brand: 'text-foreground',
  checking: 'text-foreground',
  ready: 'text-[hsl(var(--success))]',
  degraded: 'text-[hsl(var(--warning))]',
  failed: 'text-[hsl(var(--danger))]',
}

interface StartupRecoverySurface {
  title: string
  message: string
  tone: 'warning' | 'danger'
}

function CheckStatusIcon({ status }: { status: StartupDoctorCheckStatus }): React.ReactElement {
  if (status === 'passed') return <CheckCircle2 aria-hidden className="h-4 w-4 text-[hsl(var(--success))]" />
  if (status === 'warning') return <AlertTriangle aria-hidden className="h-4 w-4 text-[hsl(var(--warning))]" />
  if (status === 'failed') return <XCircle aria-hidden className="h-4 w-4 text-[hsl(var(--danger))]" />
  if (status === 'running') return <Loader2 aria-hidden className="h-4 w-4 animate-spin text-primary" />
  return <CircleDashed aria-hidden className="h-4 w-4 text-muted-foreground" />
}

function StartupCheckRow({ check }: { check: StartupDoctorCheck }): React.ReactElement {
  return (
    <li className="grid grid-cols-[1rem_minmax(0,1fr)_auto] items-start gap-3 py-2">
      <CheckStatusIcon status={check.status} />
      <div className="min-w-0">
        <p className="truncate text-sm font-medium text-foreground">{check.label}</p>
        {check.detail ? (
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{check.detail}</p>
        ) : null}
      </div>
      <span className="rounded-sm border border-border px-2 py-0.5 text-[11px] font-medium uppercase tracking-normal text-muted-foreground">
        {check.status}
      </span>
    </li>
  )
}

function startupRecoverySurface(viewModel: StartupDoctorViewModel): StartupRecoverySurface | null {
  const attentionCheck = viewModel.checks.find((check) => check.status === 'failed' || check.status === 'warning')
  const detail = attentionCheck?.detail ? ` ${attentionCheck.detail}` : ''

  if (viewModel.phase === 'failed') {
    return {
      title: 'Recovery needed',
      message: `uClaw can keep the startup doctor open while this local runtime issue is repaired.${detail}`,
      tone: 'danger',
    }
  }

  if (viewModel.phase === 'degraded') {
    return {
      title: 'Continuing in the background',
      message: `uClaw can continue while this check recovers or waits for user confirmation.${detail}`,
      tone: 'warning',
    }
  }

  return null
}

function hasBrowserRuntimeAttention(checks: StartupDoctorCheck[]): boolean {
  return checks.some(
    (check) =>
      check.id.includes('browser-runtime') &&
      (check.status === 'failed' || check.status === 'warning'),
  )
}

export function StartupSplash({
  viewModel = deriveStartupDoctorViewModel(),
  detailsExpanded,
  onDetailsExpandedChange,
  onOpenBrowserRuntimeSettings,
}: StartupSplashProps): React.ReactElement {
  const [internalExpanded, setInternalExpanded] = React.useState(viewModel.detailsRecommended)
  const isControlled = detailsExpanded !== undefined
  const expanded = isControlled ? detailsExpanded : internalExpanded
  const progress = clampStartupProgress(viewModel.progress)
  const recoverySurface = startupRecoverySurface(viewModel)
  const showBrowserRuntimeSettings =
    Boolean(onOpenBrowserRuntimeSettings) && hasBrowserRuntimeAttention(viewModel.checks)

  const setExpanded = (next: boolean) => {
    if (!isControlled) setInternalExpanded(next)
    onDetailsExpandedChange?.(next)
  }

  return (
    <section
      className="relative flex h-screen min-h-[520px] overflow-hidden bg-background text-foreground"
      aria-label="Startup status"
    >
      <div
        aria-hidden
        className="absolute inset-0 opacity-70"
        style={{
          backgroundImage:
            'linear-gradient(120deg, hsl(var(--background)) 0%, hsl(var(--muted) / 0.72) 46%, hsl(var(--background)) 100%)',
        }}
      />
      <div
        aria-hidden
        className="absolute inset-0 opacity-[0.18]"
        style={{
          backgroundImage:
            'linear-gradient(hsl(var(--foreground) / 0.12) 1px, transparent 1px), linear-gradient(90deg, hsl(var(--foreground) / 0.10) 1px, transparent 1px)',
          backgroundSize: '44px 44px',
        }}
      />
      <div
        aria-hidden
        className="absolute inset-x-0 top-0 h-1 opacity-70"
        style={{
          backgroundImage:
            'linear-gradient(90deg, hsl(var(--primary) / 0.78), hsl(var(--focus-glow) / 0.72), hsl(var(--success) / 0.62), hsl(var(--warning) / 0.58))',
        }}
      />
      <div className="relative z-10 flex w-full items-center justify-center px-6 py-10">
        <div className="w-full max-w-[720px]">
          <div className="mb-8 flex items-center gap-3">
            <div className="flex h-11 w-11 items-center justify-center rounded-md border border-border bg-background/80 shadow-sm">
              <span className="text-xl font-semibold tracking-normal">u</span>
            </div>
            <div>
              <h1 className="text-xl font-semibold tracking-normal text-foreground">uClaw</h1>
              <p className="text-sm text-muted-foreground">Local agent workbench</p>
            </div>
          </div>

          <div className="border-l border-border pl-7">
            <p className={cn('text-3xl font-semibold tracking-normal sm:text-4xl', phaseTone[viewModel.phase])}>
              {viewModel.statusLine}
            </p>
            <p className="mt-3 max-w-[34rem] text-sm leading-6 text-muted-foreground">
              Checking local runtime readiness and workspace state.
            </p>

            {recoverySurface ? (
              <div
                className={cn(
                  'mt-6 border-l-2 bg-background/72 px-4 py-3 shadow-sm',
                  recoverySurface.tone === 'danger'
                    ? 'border-[hsl(var(--danger))]'
                    : 'border-[hsl(var(--warning))]',
                )}
                role="status"
                aria-label="Startup recovery"
              >
                <div className="flex items-start gap-3">
                  <ShieldAlert
                    aria-hidden
                    className={cn(
                      'mt-0.5 h-4 w-4 shrink-0',
                      recoverySurface.tone === 'danger'
                        ? 'text-[hsl(var(--danger))]'
                        : 'text-[hsl(var(--warning))]',
                    )}
                  />
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-foreground">{recoverySurface.title}</p>
                    <p className="mt-1 text-sm leading-6 text-muted-foreground">{recoverySurface.message}</p>
                    {!expanded || showBrowserRuntimeSettings ? (
                      <div className="mt-3 flex flex-wrap gap-2">
                        {!expanded ? (
                          <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="h-8"
                            onClick={() => setExpanded(true)}
                          >
                            View diagnostics
                          </Button>
                        ) : null}
                        {showBrowserRuntimeSettings ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-8 gap-2"
                            onClick={onOpenBrowserRuntimeSettings}
                          >
                            <Settings aria-hidden className="h-3.5 w-3.5" />
                            Browser Runtime Settings
                          </Button>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                </div>
              </div>
            ) : null}

            <div className="mt-8" aria-label={`Startup progress ${progress}%`}>
              <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-primary transition-[width] duration-500 ease-out motion-reduce:transition-none"
                  style={{ width: `${progress}%` }}
                />
              </div>
              <div className="mt-3 flex items-center justify-between gap-3 text-xs text-muted-foreground">
                <span>{progress}%</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-8 px-2"
                  aria-expanded={expanded}
                  onClick={() => setExpanded(!expanded)}
                >
                  Details
                  <ChevronDown
                    aria-hidden
                    className={cn('transition-transform motion-reduce:transition-none', expanded && 'rotate-180')}
                  />
                </Button>
              </div>
            </div>

            {expanded ? (
              <ol className="mt-5 divide-y divide-border" aria-label="Startup doctor checks">
                {viewModel.checks.map((check) => (
                  <StartupCheckRow key={check.id} check={check} />
                ))}
              </ol>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  )
}
