import * as React from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  CircleDashed,
  Loader2,
  Settings,
  ShieldAlert,
  XCircle,
} from 'lucide-react'
import uclawIconSrc from '@/assets/startup/uclaw-icon.png'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  clampStartupProgress,
  deriveStartupDoctorViewModel,
  deriveStartupDoctorViewModelFromRuntimePackStatus,
  type StartupDoctorCheck,
  type StartupDoctorCheckStatus,
  type StartupDoctorViewModel,
} from '@/lib/startup/startup-doctor'
import { getBrowserRuntimeStatus } from '@/lib/tauri-bridge'

export interface StartupSplashProps {
  viewModel?: StartupDoctorViewModel
  detailsExpanded?: boolean
  onDetailsExpandedChange?: (expanded: boolean) => void
  onOpenBrowserRuntimeSettings?: () => void
}

const PROJECT_NAME = 'uClaw'

interface StartupRecoverySurface {
  title: string
  message: string
  tone: 'warning' | 'danger'
}

function CheckStatusIcon({ status }: { status: StartupDoctorCheckStatus }): React.ReactElement {
  if (status === 'passed') return <CheckCircle2 aria-hidden className="h-4 w-4 text-[hsl(var(--success))]" />
  if (status === 'warning') return <AlertTriangle aria-hidden className="h-4 w-4 text-[hsl(var(--warning))]" />
  if (status === 'failed') return <XCircle aria-hidden className="h-4 w-4 text-[hsl(var(--danger))]" />
  if (status === 'running') return <Loader2 aria-hidden className="h-4 w-4 animate-spin text-[var(--startup-brand-orange)] motion-reduce:animate-none" />
  return <CircleDashed aria-hidden className="h-4 w-4 text-black/32" />
}

function StartupCheckRow({ check }: { check: StartupDoctorCheck }): React.ReactElement {
  return (
    <li className="grid grid-cols-[1rem_minmax(0,1fr)_auto] items-start gap-3 py-2.5">
      <CheckStatusIcon status={check.status} />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold text-black/72">{check.label}</p>
        {check.detail ? (
          <p className="mt-0.5 text-xs leading-5 text-black/48">{check.detail}</p>
        ) : null}
      </div>
      <span className="rounded-sm border border-black/10 bg-white/44 px-2 py-0.5 text-[11px] font-semibold uppercase tracking-normal text-black/42">
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

function makeSplashBuildCode(): string {
  return Math.random().toString(36).slice(2, 10).toUpperCase()
}

function appBuildLabel(fallback: string): string {
  if (typeof __APP_COMMIT__ === 'string' && __APP_COMMIT__ && __APP_COMMIT__ !== 'unknown') {
    return __APP_COMMIT__.toUpperCase()
  }
  return fallback
}

function appVersionLabel(): string {
  return typeof __APP_VERSION__ === 'string' && __APP_VERSION__ ? `v${__APP_VERSION__}` : 'dev'
}

export function StartupSplash({
  viewModel: providedViewModel,
  detailsExpanded,
  onDetailsExpandedChange,
  onOpenBrowserRuntimeSettings,
}: StartupSplashProps): React.ReactElement {
  const [liveViewModel, setLiveViewModel] = React.useState<StartupDoctorViewModel | undefined>()
  const viewModel = providedViewModel ?? liveViewModel ?? deriveStartupDoctorViewModel()
  const [internalExpanded, setInternalExpanded] = React.useState(viewModel.detailsRecommended)
  const [buildCode] = React.useState(makeSplashBuildCode)
  const isControlled = detailsExpanded !== undefined
  const expanded = isControlled ? detailsExpanded : internalExpanded
  const progress = clampStartupProgress(viewModel.progress)
  const recoverySurface = startupRecoverySurface(viewModel)
  const isRecovery = Boolean(recoverySurface)
  const showBrowserRuntimeSettings =
    Boolean(onOpenBrowserRuntimeSettings) && hasBrowserRuntimeAttention(viewModel.checks)
  const visualTitle = PROJECT_NAME

  React.useEffect(() => {
    if (providedViewModel) {
      setLiveViewModel(undefined)
      return
    }

    let cancelled = false
    void getBrowserRuntimeStatus()
      .then((report) => {
        if (!cancelled) {
          setLiveViewModel(deriveStartupDoctorViewModelFromRuntimePackStatus(report))
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLiveViewModel(undefined)
        }
      })

    return () => {
      cancelled = true
    }
  }, [providedViewModel])

  React.useEffect(() => {
    if (!isControlled && viewModel.detailsRecommended) {
      setInternalExpanded(true)
    }
  }, [isControlled, viewModel.detailsRecommended])

  const setExpanded = (next: boolean) => {
    if (!isControlled) setInternalExpanded(next)
    onDetailsExpandedChange?.(next)
  }

  return (
    <section
      className="startup-splash-if2ai relative flex h-screen min-h-[560px] cursor-default overflow-hidden text-[var(--startup-ink)]"
      aria-label="Startup status"
    >
      <div aria-hidden className="startup-splash-if2ai__backdrop absolute inset-0" />
      <div aria-hidden className="startup-splash-if2ai__grid absolute inset-0 overflow-hidden opacity-70" />
      <div aria-hidden className="absolute inset-0 opacity-35">
        <div className="absolute left-0 right-0 top-1/2 h-px -translate-y-1/2 bg-gradient-to-r from-transparent via-[rgb(255_127_50_/_0.16)] to-transparent" />
        <div className="absolute bottom-0 left-1/2 top-0 w-px -translate-x-1/2 bg-gradient-to-b from-transparent via-[rgb(255_127_50_/_0.12)] to-transparent" />
      </div>
      <div className="absolute right-5 top-5 z-10 flex items-center gap-3 text-[11px] font-semibold tracking-[0.08em] text-black/20 sm:right-6 sm:top-6 sm:text-xs">
        <span>{appBuildLabel(buildCode)}</span>
        <span aria-hidden>•</span>
        <span>{appVersionLabel()}</span>
      </div>

      <div
        className={cn(
          'relative z-10 grid w-full px-5 py-7 sm:px-8 sm:py-10',
          isRecovery ? 'grid-rows-[auto_minmax(0,1fr)]' : 'grid-rows-[minmax(0,1fr)_auto]',
        )}
      >
        <div
          className={cn(
            'flex min-h-0 flex-col items-center text-center',
            isRecovery ? 'justify-start pt-3 sm:justify-center sm:pt-0' : 'justify-center',
          )}
        >
          <div className="relative">
            <div className="absolute inset-0 rounded-md bg-[radial-gradient(circle_at_50%_50%,rgb(255_141_71_/_0.18),transparent_62%)] blur-2xl" />
            <div className="relative overflow-hidden rounded-md border border-white/80 bg-white/64 p-1 shadow-[0_10px_40px_rgb(255_127_50_/_0.08),0_0_0_1px_rgba(255,255,255,0.66)] backdrop-blur-xl">
              <img
                src={uclawIconSrc}
                alt={`${PROJECT_NAME} logo`}
                className={cn(
                  'rounded-md object-cover opacity-35',
                  isRecovery ? 'h-[64px] w-[64px] sm:h-[96px] sm:w-[96px]' : 'h-[86px] w-[86px] sm:h-[110px] sm:w-[110px]',
                )}
                draggable={false}
              />
              <span
                aria-hidden="true"
                className={cn(
                  'absolute inset-1 flex items-center justify-center rounded-md bg-[radial-gradient(circle_at_45%_30%,rgb(255_255_255_/_0.54),transparent_40%),linear-gradient(145deg,rgb(255_141_71_/_0.18),rgb(69_106_128_/_0.20))] font-black leading-none text-[var(--startup-brand-orange-dark)] drop-shadow-[0_8px_18px_rgb(255_106_31_/_0.22)]',
                  isRecovery ? 'text-[40px] sm:text-[62px]' : 'text-[54px] sm:text-[70px]',
                )}
              >
                u
              </span>
            </div>
          </div>

          <div className={cn(isRecovery ? 'mt-6 sm:mt-10' : 'mt-10 sm:mt-14')}>
            <div
              className={cn(
                'relative inline-flex items-center justify-center',
                isRecovery ? 'min-h-[4.25rem] sm:min-h-[5.75rem]' : 'min-h-[5.75rem] sm:min-h-[6.5rem]',
              )}
            >
              <span
                aria-hidden="true"
                className={cn(
                  'startup-splash-if2ai__title-glow pointer-events-none absolute inset-0 font-black leading-none text-[rgb(255_127_50_/_0.40)] blur-[10px]',
                  isRecovery ? 'text-[52px] sm:text-[76px]' : 'text-[68px] sm:text-[88px]',
                )}
              >
                {PROJECT_NAME}
              </span>
              <h1
                className={cn(
                  'startup-splash-if2ai__title relative font-bold leading-none',
                  isRecovery ? 'text-[52px] sm:text-[76px]' : 'text-[68px] sm:text-[88px]',
                  'startup-splash-if2ai__glitch-base',
                )}
                aria-label={PROJECT_NAME}
                data-text={PROJECT_NAME}
              >
                <span className="relative z-10 bg-[linear-gradient(180deg,var(--startup-brand-orange-light)_0%,var(--startup-brand-orange-blend)_46%,var(--startup-brand-orange-dark)_100%)] bg-clip-text text-transparent drop-shadow-[0_10px_18px_rgb(255_106_31_/_0.18)]">
                  {visualTitle}
                  <span
                    aria-hidden
                    className="ml-1 inline-block h-[0.9em] w-[0.08em] translate-y-[0.08em] rounded-full bg-[var(--startup-brand-orange-blend)] align-baseline opacity-0"
                  />
                </span>
              </h1>
              <span
                aria-hidden="true"
                className={cn(
                  'startup-splash-if2ai__glitch-layer startup-splash-if2ai__glitch-layer-a startup-splash-if2ai__title absolute inset-0 font-bold leading-none text-[var(--startup-brand-orange-glow)]',
                  isRecovery ? 'text-[52px] sm:text-[76px]' : 'text-[68px] sm:text-[88px]',
                )}
              >
                {PROJECT_NAME}
              </span>
              <span
                aria-hidden="true"
                className={cn(
                  'startup-splash-if2ai__glitch-layer startup-splash-if2ai__glitch-layer-b startup-splash-if2ai__title absolute inset-0 font-bold leading-none text-[var(--startup-brand-orange-dark)]',
                  isRecovery ? 'text-[52px] sm:text-[76px]' : 'text-[68px] sm:text-[88px]',
                )}
              >
                {PROJECT_NAME}
              </span>
            </div>
          </div>

          {!recoverySurface ? <WaveDotsAnimation /> : null}

          {!recoverySurface ? (
            <p className="mt-8 text-sm font-semibold tracking-[0.03em] text-black/42">
              {viewModel.statusLine}
            </p>
          ) : null}
        </div>

        <div className={cn('mx-auto w-full max-w-3xl', isRecovery ? 'mt-3 min-h-0 overflow-y-auto pb-1' : 'mt-6')}>
          {recoverySurface ? (
            <div
              className={cn(
                'mb-4 rounded-md border bg-white/58 px-4 py-3 text-left shadow-[0_14px_34px_rgb(0_0_0_/_0.06)] backdrop-blur-xl',
                recoverySurface.tone === 'danger'
                  ? 'border-[hsl(var(--danger)_/_0.28)]'
                  : 'border-[hsl(var(--warning)_/_0.28)]',
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
                  <p className="text-sm font-semibold text-black/72">{recoverySurface.title}</p>
                  <p className="mt-0.5 text-sm font-semibold text-black/62">{viewModel.statusLine}</p>
                  <p className="mt-1 text-sm leading-6 text-black/50">{recoverySurface.message}</p>
                  {!expanded || showBrowserRuntimeSettings ? (
                    <div className="mt-3 flex flex-wrap gap-2">
                      {!expanded ? (
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          className="h-8 bg-white/70 text-black/70 hover:bg-white"
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
                          className="h-8 gap-2 border-black/12 bg-white/46 text-black/68 hover:bg-white/80"
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

          <div className="rounded-md border border-white/64 bg-white/46 px-4 py-3 shadow-[0_16px_42px_rgb(0_0_0_/_0.055)] backdrop-blur-xl">
            <div className="flex items-center justify-between gap-3 text-xs font-semibold text-black/42">
              <span aria-label={`Startup progress ${progress}%`}>{progress}%</span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-black/50 hover:bg-white/60 hover:text-black/74"
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
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-black/8">
              <div
                className="h-full rounded-full bg-[linear-gradient(90deg,var(--startup-brand-orange),var(--startup-jade-dim))] transition-[width] duration-500 ease-out motion-reduce:transition-none"
                style={{ width: `${progress}%` }}
              />
            </div>

            {expanded ? (
              <ol className="mt-4 divide-y divide-black/8" aria-label="Startup doctor checks">
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

function WaveDotsAnimation(): React.ReactElement {
  return (
    <div className="startup-splash-if2ai__wave mt-8 sm:mt-10" aria-hidden="true">
      {Array.from({ length: 10 }, (_, index) => (
        <span key={`top-${index}`} className="startup-splash-if2ai__wave-dot startup-splash-if2ai__wave-dot-top" />
      ))}
      {Array.from({ length: 10 }, (_, index) => (
        <span key={`bottom-${index}`} className="startup-splash-if2ai__wave-dot startup-splash-if2ai__wave-dot-bottom" />
      ))}
    </div>
  )
}
