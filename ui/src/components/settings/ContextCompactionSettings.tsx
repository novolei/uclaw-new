/**
 * ContextCompactionSettings — "上下文与压缩" card.
 *
 * Edits the agent transcript compaction threshold plus the four agent
 * context settings (tool-result clearing, summary length, file reload).
 * K-unit fields are shown in thousands of tokens; the backend stores raw
 * tokens.
 */
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { toast } from 'sonner'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  getAgentCompactionThreshold,
  setAgentCompactionThreshold,
  getAgentContextSettings,
  setAgentContextSettings,
} from '@/lib/tauri-bridge'
import type { AgentContextSettings } from '@/lib/types'

const DEFAULT_K = 128
const MIN_K = 32
const MAX_K = 1000
const STEP_K = 8

const RANGE_ERROR = `请输入 ${MIN_K}K–${MAX_K}K 之间的整数`

function validate(raw: string): number | null {
  const trimmed = raw.trim()
  if (!/^\d+$/.test(trimmed)) return null
  const k = Number(trimmed)
  return k >= MIN_K && k <= MAX_K ? k : null
}

type LoadState = 'loading' | 'loaded' | 'error'

/**
 * Load a backend setting once on mount (and on `load()` retry). Until the
 * value arrives `saved` is null and `loadState` is 'loading': callers keep
 * their inputs empty and disabled, so a default is never shown as if it were
 * the real setting and nothing is saved from a placeholder.
 */
function useLoadedSetting<T>(fetchValue: () => Promise<T>, what: string) {
  const [loadState, setLoadState] = useState<LoadState>('loading')
  const [saved, setSaved] = useState<T | null>(null)
  const mounted = useRef(true)

  const load = useCallback(() => {
    setLoadState('loading')
    fetchValue()
      .then((v) => {
        if (!mounted.current) return
        setSaved(v)
        setLoadState('loaded')
      })
      .catch((e) => {
        console.error(`[ContextCompactionSettings] get ${what} failed`, e)
        if (mounted.current) setLoadState('error')
      })
  }, [fetchValue, what])

  useEffect(() => {
    mounted.current = true
    load()
    return () => {
      mounted.current = false
    }
  }, [load])

  return { loadState, loaded: loadState === 'loaded', saved, setSaved, load }
}

function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="flex items-center gap-2">
      <p role="alert" className="text-xs text-destructive">
        {message}
      </p>
      <Button size="sm" variant="ghost" className="h-6 text-xs" onClick={onRetry}>
        重试
      </Button>
    </div>
  )
}

type ContextKey = keyof AgentContextSettings

interface ContextField {
  key: ContextKey
  label: string
  ariaLabel: string
  description: string
  /** Tokens per displayed unit (1000 for K fields). */
  scale: number
  unit: string
  min: number
  max: number
}

const CONTEXT_FIELDS: ContextField[] = [
  {
    key: 'keepToolResultGroups',
    label: '保留最近工具结果组数',
    ariaLabel: '保留最近工具结果组数',
    description: '更早的工具结果在上下文接近阈值时会被替换为一行摘要',
    scale: 1,
    unit: '组',
    min: 1,
    max: 20,
  },
  {
    key: 'toolClearFloorTokens',
    label: '批量清理下限',
    ariaLabel: '批量清理下限 (K)',
    description: '可清理内容少于此值时不清理，避免频繁打断缓存',
    scale: 1000,
    unit: 'K',
    min: 5,
    max: 200,
  },
  {
    key: 'summaryTargetTokens',
    label: '摘要目标长度',
    ariaLabel: '摘要目标长度 (K)',
    description: '压缩时生成的摘要长度',
    scale: 1000,
    unit: 'K',
    min: 1,
    max: 8,
  },
  {
    key: 'reloadRecentFiles',
    label: '压缩后重载文件数',
    ariaLabel: '压缩后重载文件数',
    description: '压缩后重新放入最近改过的文件内容（单个超过 5K 只给路径）',
    scale: 1,
    unit: '个',
    min: 0,
    max: 10,
  },
]

type ContextDrafts = Record<ContextKey, string>

const EMPTY_DRAFTS: ContextDrafts = {
  keepToolResultGroups: '',
  toolClearFloorTokens: '',
  summaryTargetTokens: '',
  reloadRecentFiles: '',
}

function displayValue(s: AgentContextSettings, f: ContextField): string {
  return String(Math.round(s[f.key] / f.scale))
}

function draftsFrom(s: AgentContextSettings): ContextDrafts {
  const out = { ...EMPTY_DRAFTS }
  for (const f of CONTEXT_FIELDS) out[f.key] = displayValue(s, f)
  return out
}

export function ContextCompactionSettings() {
  const inputId = useId()
  const errorId = `${inputId}-error`

  // — Compaction threshold —
  const threshold = useLoadedSetting(getAgentCompactionThreshold, 'threshold')
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const savedK = threshold.saved === null ? DEFAULT_K : Math.round(threshold.saved / 1000)
  const loaded = threshold.loaded

  useEffect(() => {
    if (threshold.saved !== null) setValue(String(Math.round(threshold.saved / 1000)))
  }, [threshold.saved])

  const save = async (k: number) => {
    setSaving(true)
    try {
      const tokens = await setAgentCompactionThreshold(k * 1000)
      threshold.setSaved(tokens)
      setValue(String(Math.round(tokens / 1000)))
      setError(null)
    } catch (e) {
      toast.error('保存压缩阈值失败', { description: String(e) })
      setValue(String(savedK))
    } finally {
      setSaving(false)
    }
  }

  const handleChange = (raw: string) => {
    if (!loaded) return
    setValue(raw)
    setError(validate(raw) === null ? RANGE_ERROR : null)
  }

  const commit = () => {
    if (!loaded) return
    const k = validate(value)
    if (k === null) {
      setError(RANGE_ERROR)
      return
    }
    if (k !== savedK) void save(k)
  }

  const handleReset = () => {
    if (!loaded) return
    setError(null)
    setValue(String(DEFAULT_K))
    void save(DEFAULT_K)
  }

  // — Agent context settings —
  const ctx = useLoadedSetting(getAgentContextSettings, 'context settings')
  const [drafts, setDrafts] = useState<ContextDrafts>(EMPTY_DRAFTS)
  const [ctxSaving, setCtxSaving] = useState(false)
  // Enter commits and the following blur would commit again from a stale
  // closure; the ref drops the second call while a save is in flight.
  const ctxSavingRef = useRef(false)
  const ctxSaved = ctx.saved

  useEffect(() => {
    if (ctxSaved !== null) setDrafts(draftsFrom(ctxSaved))
  }, [ctxSaved])

  const commitContext = async (f: ContextField) => {
    if (!ctx.loaded || ctxSaved === null || ctxSavingRef.current) return
    const raw = drafts[f.key].trim()
    const restore = () => setDrafts((d) => ({ ...d, [f.key]: displayValue(ctxSaved, f) }))
    if (!/^\d+$/.test(raw)) {
      restore()
      return
    }
    if (raw === displayValue(ctxSaved, f)) return
    ctxSavingRef.current = true
    setCtxSaving(true)
    try {
      // The backend clamps out-of-range values and returns what it saved.
      const next = await setAgentContextSettings({ ...ctxSaved, [f.key]: Number(raw) * f.scale })
      ctx.setSaved(next)
      setDrafts(draftsFrom(next))
    } catch (e) {
      toast.error('保存上下文设置失败', { description: String(e) })
      restore()
    } finally {
      ctxSavingRef.current = false
      setCtxSaving(false)
    }
  }

  return (
    <SettingsSection
      title="上下文与压缩"
      description="对话超过该长度时，会把较早的内容压缩成摘要，并保留最近几轮原文。数值越大记得越多，费用也越高。"
    >
      <SettingsCard>
        <SettingsRow
          label="压缩阈值"
          description={`单位 K（千 token），范围 ${MIN_K}K–${MAX_K}K，默认 ${DEFAULT_K}K`}
        >
          <div className="flex flex-col items-end gap-1">
            <div className="flex items-center gap-2">
              <Input
                id={inputId}
                type="number"
                inputMode="numeric"
                aria-label="压缩阈值 (K)"
                aria-invalid={error !== null}
                aria-describedby={error ? errorId : undefined}
                min={MIN_K}
                max={MAX_K}
                step={STEP_K}
                value={value}
                placeholder={threshold.loadState === 'loading' ? '…' : undefined}
                disabled={saving || !loaded}
                onChange={(e) => handleChange(e.target.value)}
                onBlur={commit}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commit()
                }}
                className={cn('h-8 w-24 text-sm', error && 'border-destructive')}
              />
              <span className="text-sm text-muted-foreground">K</span>
              <Button
                size="sm"
                variant="outline"
                className="h-8 text-xs"
                onClick={handleReset}
                disabled={saving || !loaded}
              >
                恢复默认
              </Button>
            </div>
            {threshold.loadState === 'error' && (
              <LoadError message="读取压缩阈值失败" onRetry={threshold.load} />
            )}
            {error && (
              <p id={errorId} role="alert" className="text-xs text-destructive">
                {error}
              </p>
            )}
          </div>
        </SettingsRow>
        {CONTEXT_FIELDS.map((f, i) => (
          <SettingsRow key={f.key} label={f.label} description={f.description}>
            <div className="flex flex-col items-end gap-1">
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  inputMode="numeric"
                  aria-label={f.ariaLabel}
                  min={f.min}
                  max={f.max}
                  step={1}
                  value={drafts[f.key]}
                  placeholder={ctx.loadState === 'loading' ? '…' : undefined}
                  disabled={ctxSaving || !ctx.loaded}
                  onChange={(e) => {
                    if (!ctx.loaded) return
                    const raw = e.target.value
                    setDrafts((d) => ({ ...d, [f.key]: raw }))
                  }}
                  onBlur={() => void commitContext(f)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void commitContext(f)
                  }}
                  className="h-8 w-24 text-sm"
                />
                <span className="w-4 text-sm text-muted-foreground">{f.unit}</span>
              </div>
              {i === 0 && ctx.loadState === 'error' && (
                <LoadError message="读取上下文设置失败" onRetry={ctx.load} />
              )}
            </div>
          </SettingsRow>
        ))}
      </SettingsCard>
    </SettingsSection>
  )
}
