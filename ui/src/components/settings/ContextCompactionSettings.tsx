/**
 * ContextCompactionSettings — "上下文与压缩" card.
 *
 * Edits the agent transcript compaction threshold. The UI unit is K
 * (thousands of tokens); the backend stores raw tokens.
 */
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { toast } from 'sonner'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { getAgentCompactionThreshold, setAgentCompactionThreshold } from '@/lib/tauri-bridge'

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

export function ContextCompactionSettings() {
  const inputId = useId()
  const errorId = `${inputId}-error`
  // Until the backend value arrives the input is empty and disabled: never
  // show the default as if it were the real setting, and never let an edit
  // made before the load be overwritten by it (or saved from a placeholder).
  const [loadState, setLoadState] = useState<LoadState>('loading')
  const [value, setValue] = useState('')
  const [savedK, setSavedK] = useState(DEFAULT_K)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const mounted = useRef(true)

  const load = useCallback(() => {
    setLoadState('loading')
    getAgentCompactionThreshold()
      .then((tokens) => {
        if (!mounted.current) return
        const k = Math.round(tokens / 1000)
        setSavedK(k)
        setValue(String(k))
        setLoadState('loaded')
      })
      .catch((e) => {
        console.error('[ContextCompactionSettings] get threshold failed', e)
        if (mounted.current) setLoadState('error')
      })
  }, [])

  useEffect(() => {
    mounted.current = true
    load()
    return () => {
      mounted.current = false
    }
  }, [load])

  const loaded = loadState === 'loaded'

  const save = async (k: number) => {
    setSaving(true)
    try {
      const tokens = await setAgentCompactionThreshold(k * 1000)
      const next = Math.round(tokens / 1000)
      setSavedK(next)
      setValue(String(next))
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
                placeholder={loadState === 'loading' ? '…' : undefined}
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
            {loadState === 'error' && (
              <div className="flex items-center gap-2">
                <p role="alert" className="text-xs text-destructive">
                  读取压缩阈值失败
                </p>
                <Button size="sm" variant="ghost" className="h-6 text-xs" onClick={load}>
                  重试
                </Button>
              </div>
            )}
            {error && (
              <p id={errorId} role="alert" className="text-xs text-destructive">
                {error}
              </p>
            )}
          </div>
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}
