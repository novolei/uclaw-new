import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Loader2, Check, AlertCircle, X } from 'lucide-react'
import { toast } from 'sonner'
import { listen } from '@tauri-apps/api/event'
import { cn } from '@/lib/utils'
import { installWizardAtom, marketplaceDetailAtom } from '@/atoms/marketplace'
import { humaneSpecsAtom } from '@/atoms/automation'
import { installMarketplaceHuman } from '@/lib/tauri-bridge'
import type { MarketplaceInstallProgress } from '@/lib/tauri-bridge'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'

// skill/mcp aren't workspace-scoped — they skip the 'scope' step
function getStepSequence(appType: string | null): readonly string[] {
  if (appType === 'skill' || appType === 'mcp') {
    return ['config', 'confirm', 'progress'] as const
  }
  return ['scope', 'config', 'confirm', 'progress'] as const
}

export function InstallWizard(): React.ReactElement | null {
  const [state, setState] = useAtom(installWizardAtom)
  const detail = useAtomValue(marketplaceDetailAtom)
  const [, setSpecs] = useAtom(humaneSpecsAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)

  const steps = getStepSequence(state.appType)
  // Visible steps in stepper header (exclude 'progress')
  const visibleSteps = steps.filter((s) => s !== 'progress')

  // Default selected space to current active workspace
  React.useEffect(() => {
    if (state.step === 'scope' && state.spaceId === null && activeWorkspaceId) {
      setState((s) => ({ ...s, spaceId: activeWorkspaceId }))
    }
  }, [state.step, state.spaceId, activeWorkspaceId, setState])

  // Subscribe to install progress events when in progress step
  React.useEffect(() => {
    if (state.step !== 'progress' || !state.slug) return
    const channel = `install_progress_${state.slug}`
    let unlisten: (() => void) | undefined
    listen<MarketplaceInstallProgress>(channel, (event) => {
      const { phase, percent, message } = event.payload
      setState((s) => ({ ...s, progress: { phase, percent, message: message ?? undefined } }))
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [state.step, state.slug, setState])

  if (state.step === null) return null

  const close = () => setState({ step: null, slug: null, appType: null, spaceId: null, userConfig: {}, progress: null, error: null })

  const submit = async () => {
    // spaceId is only required for automation; skill/mcp don't need a workspace
    if (!state.slug || (state.appType === 'automation' && !state.spaceId)) return
    setState((s) => ({ ...s, step: 'progress', progress: { phase: 'fetching_spec', percent: 5 } }))
    try {
      const outcome = await installMarketplaceHuman(
        state.slug,
        state.spaceId ?? undefined,
        state.userConfig,
        `install_progress_${state.slug}`,
      )
      // Only the automation path returns a spec row to merge into the list.
      if (outcome.kind === 'automation') {
        const row = outcome.spec
        setSpecs((prev) => [row, ...prev.filter((s) => s.id !== row.id)])
        toast.success(`已安装 ${row.name}`)
      } else {
        toast.success(`已安装 ${state.slug}`)
      }
      setTimeout(close, 800)
    } catch (err) {
      setState((s) => ({ ...s, error: String(err) }))
    }
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center p-6 bg-foreground/10 backdrop-blur-sm">
      <motion.div
        initial={{ opacity: 0, scale: 0.992 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.992 }}
        transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
        className="w-full max-w-xl bg-content-area rounded-xl shadow-2xl border border-border/50 overflow-hidden"
      >
        {/* Header with progress dots + close */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border/50">
          <div className="flex items-center gap-2">
            {visibleSteps.map((step, i) => {
              const stepIdx = steps.indexOf(state.step ?? visibleSteps[0])
              const completed = i < stepIdx
              const current = i === stepIdx
              return (
                <div key={step} className="flex items-center gap-2">
                  <div className={cn(
                    'w-2 h-2 rounded-full transition-colors',
                    current ? 'bg-primary' : completed ? 'bg-primary/40' : 'bg-muted',
                  )} />
                  {i < visibleSteps.length - 1 && <div className={cn('w-4 h-px', i < stepIdx ? 'bg-primary/40' : 'bg-muted')} />}
                </div>
              )
            })}
            <span className="ml-2 text-[12px] text-muted-foreground">
              {state.step === 'scope' && `选择空间 (1/${visibleSteps.length})`}
              {state.step === 'config' && `填写配置 (${visibleSteps.indexOf('config') + 1}/${visibleSteps.length})`}
              {state.step === 'confirm' && `确认安装 (${visibleSteps.indexOf('confirm') + 1}/${visibleSteps.length})`}
              {state.step === 'progress' && '安装中...'}
            </span>
          </div>
          <button onClick={close} className="text-muted-foreground hover:text-foreground" title="关闭 (Esc)">
            <X size={14} />
          </button>
        </div>

        <AnimatePresence mode="wait">
          <motion.div
            key={state.step}
            initial={{ opacity: 0, x: 8 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -8 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="p-5 min-h-[200px]"
          >
            {state.step === 'scope' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">选择安装到哪个工作区</h3>
                <div className="space-y-1">
                  {workspaces.map((ws) => (
                    <button
                      key={ws.id}
                      type="button"
                      onClick={() => setState((s) => ({ ...s, spaceId: ws.id }))}
                      className={cn(
                        'w-full flex items-center justify-between px-3 py-2 rounded-md text-[13px] transition-colors',
                        state.spaceId === ws.id
                          ? 'bg-primary/10 text-foreground border border-primary/30'
                          : 'border border-border/50 hover:bg-accent/30',
                      )}
                    >
                      <span className="truncate">{ws.name}</span>
                      {state.spaceId === ws.id && <Check size={14} className="text-primary" />}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {state.step === 'config' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">填写运行参数</h3>
                <ConfigForm
                  parsedSpecJson={detail?.parsedSpecJson}
                  values={state.userConfig}
                  onChange={(v) => setState((s) => ({ ...s, userConfig: v }))}
                />
              </div>
            )}

            {state.step === 'confirm' && (
              <div>
                <h3 className="text-[14px] font-semibold mb-3">确认安装</h3>
                <dl className="text-[12px] space-y-2">
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">数字员工</dt>
                    <dd>{detail?.item.name}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">版本</dt>
                    <dd>v{detail?.item.version}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">工作区</dt>
                    <dd>{workspaces.find((w) => w.id === state.spaceId)?.name ?? '?'}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">配置项</dt>
                    <dd>{Object.keys(state.userConfig).length} 项</dd>
                  </div>
                </dl>
                {state.error && (
                  <div className="flex items-start gap-2 mt-3 p-2 rounded-md bg-danger-bg text-danger text-[11px]">
                    <AlertCircle size={12} className="mt-0.5" />
                    <span>{state.error}</span>
                  </div>
                )}
              </div>
            )}

            {state.step === 'progress' && (
              <div className="flex flex-col items-center gap-3 py-6">
                <Loader2 size={20} className="animate-spin text-primary" />
                <div className="text-[13px]">{state.progress?.message ?? '处理中...'}</div>
                <div className="w-full max-w-xs bg-muted rounded-full overflow-hidden h-1">
                  <div
                    className="bg-primary h-full transition-all"
                    style={{ width: `${state.progress?.percent ?? 0}%` }}
                  />
                </div>
                <div className="text-[10px] text-muted-foreground tabular-nums">
                  {state.progress?.percent ?? 0}% · {state.progress?.phase ?? ''}
                </div>
              </div>
            )}
          </motion.div>
        </AnimatePresence>

        {/* Footer */}
        {state.step !== 'progress' && (
          <div className="flex items-center justify-between px-5 py-3 border-t border-border/50 bg-card/30">
            <button
              type="button"
              onClick={() => {
                const idx = steps.indexOf(state.step ?? steps[0])
                if (idx <= 0) {
                  close()
                } else {
                  setState((s) => ({ ...s, step: steps[idx - 1] as typeof s.step }))
                }
              }}
              className="text-[12px] text-muted-foreground hover:text-foreground transition-colors"
            >
              {steps.indexOf(state.step ?? steps[0]) === 0 ? '取消' : '← 返回'}
            </button>
            <button
              type="button"
              onClick={() => {
                const idx = steps.indexOf(state.step ?? steps[0])
                const next = steps[idx + 1]
                if (next === 'progress') {
                  void submit()
                } else if (next) {
                  setState((s) => ({ ...s, step: next as typeof s.step }))
                }
              }}
              disabled={state.step === 'scope' && !state.spaceId}
              className={cn(
                'px-3 py-1.5 text-[12px] rounded-md font-medium transition-colors',
                'bg-primary text-primary-foreground hover:bg-primary/90',
                'disabled:opacity-50 disabled:cursor-not-allowed',
              )}
            >
              {state.step === 'confirm' ? '安装' : '继续 →'}
            </button>
          </div>
        )}
      </motion.div>
    </div>
  )
}

interface ConfigFormProps {
  parsedSpecJson: unknown | null
  values: Record<string, unknown>
  onChange: (v: Record<string, unknown>) => void
}

function ConfigForm({ parsedSpecJson, values, onChange }: ConfigFormProps): React.ReactElement {
  if (!parsedSpecJson || typeof parsedSpecJson !== 'object') {
    return (
      <p className="text-[12px] text-muted-foreground italic">
        spec 解析失败，将以默认配置安装。
      </p>
    )
  }
  const schema = (parsedSpecJson as Record<string, unknown>).config_schema
  if (!Array.isArray(schema) || schema.length === 0) {
    return (
      <p className="text-[12px] text-muted-foreground italic">
        此数字员工无可配置项，可直接进入下一步。
      </p>
    )
  }

  const setField = (key: string, v: unknown) => onChange({ ...values, [key]: v })

  return (
    <div className="space-y-3 max-h-[300px] overflow-y-auto">
      {(schema as Array<Record<string, unknown>>).map((field, idx) => {
        const key = String(field.key ?? `field-${idx}`)
        const label = String(field.label ?? key)
        const type = String(field.type ?? 'text')
        const required = field.required === true
        const placeholder = typeof field.placeholder === 'string' ? field.placeholder : undefined
        const description = typeof field.description === 'string' ? field.description : undefined
        const current = values[key] ?? field.default ?? ''
        return (
          <div key={key}>
            <label className="block text-[12px] font-medium mb-1">
              {label}
              {required && <span className="text-danger ml-0.5">*</span>}
            </label>
            {description && <p className="text-[11px] text-muted-foreground mb-1">{description}</p>}
            {type === 'boolean' ? (
              <input
                type="checkbox"
                checked={!!current}
                onChange={(e) => setField(key, e.target.checked)}
              />
            ) : type === 'number' ? (
              <input
                type="number"
                value={String(current)}
                onChange={(e) => setField(key, Number(e.target.value))}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            ) : type === 'select' && Array.isArray(field.options) ? (
              <select
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              >
                {(field.options as Array<unknown>).map((opt, i) => {
                  const o = opt as Record<string, unknown>
                  return (
                    <option key={i} value={String(o.value ?? o)}>
                      {String(o.label ?? o.value ?? o)}
                    </option>
                  )
                })}
              </select>
            ) : type === 'text' ? (
              <textarea
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[60px]"
              />
            ) : (
              <input
                type="text"
                value={String(current)}
                onChange={(e) => setField(key, e.target.value)}
                placeholder={placeholder}
                className="w-full px-2 py-1 text-[12px] rounded-md border border-border/50 bg-card focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            )}
          </div>
        )
      })}
    </div>
  )
}
