/**
 * MemoryGrowthSection — 记忆成长设置：开关、每 N 轮反思一次、每日 token 预算。
 * 需要先开启学习模型（memory_os.learning_enabled）才会实际运行。
 * 保存后显示后端实际保存的值（会被夹到允许范围内）。
 */
import * as React from 'react'
import { getGrowthSettings, setGrowthSettings } from '@/lib/tauri-bridge'
import type { GrowthSettings } from '@/lib/types'
import { SettingsToggle } from './primitives/SettingsToggle'
import { Button } from '@/components/ui/button'

function NumberField({
  id,
  label,
  description,
  value,
  min,
  max,
  onChange,
}: {
  id: string
  label: string
  description: string
  value: number
  min: number
  max: number
  onChange: (v: number) => void
}): React.ReactElement {
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-[11px] font-medium">
        {label}
      </label>
      <input
        id={id}
        type="number"
        min={min}
        max={max}
        value={Number.isFinite(value) ? value : ''}
        onChange={(e) => {
          const n = parseInt(e.target.value, 10)
          onChange(Number.isFinite(n) ? n : 0)
        }}
        className="w-40 px-2 py-1.5 rounded border border-input bg-background text-[11px] font-mono"
      />
      <span className="text-[10px] text-muted-foreground">{description}</span>
    </div>
  )
}

export function MemoryGrowthSection(): React.ReactElement {
  const [saved, setSaved] = React.useState<GrowthSettings | null>(null)
  const [draft, setDraft] = React.useState<GrowthSettings | null>(null)
  const [saving, setSaving] = React.useState(false)
  const [message, setMessage] = React.useState<string | null>(null)

  React.useEffect(() => {
    getGrowthSettings()
      .then((s) => {
        setSaved(s)
        setDraft(s)
      })
      .catch((e) => setMessage(`读取失败：${String(e)}`))
  }, [])

  if (!draft || !saved) {
    return <div className="text-xs text-muted-foreground">{message ?? '加载中…'}</div>
  }

  const dirty = JSON.stringify(draft) !== JSON.stringify(saved)

  const save = async () => {
    setSaving(true)
    setMessage(null)
    try {
      const s = await setGrowthSettings(draft)
      setSaved(s)
      setDraft(s)
      setMessage('已保存')
    } catch (e) {
      setMessage(`保存失败：${String(e)}`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-sm font-medium">记忆成长</h3>
        <p className="text-xs text-muted-foreground mt-1">
          每隔几轮 Agent 对话，后台把新对话提炼成一条「反思」，并根据已学到的事实更新「用户模型」；两者会附在之后每轮对话里。需要先开启学习模型才会运行。
        </p>
      </div>
      <SettingsToggle
        label="启用记忆成长"
        checked={draft.enabled}
        onCheckedChange={(enabled) => setDraft({ ...draft, enabled })}
      />
      <div className="flex flex-wrap gap-6">
        <NumberField
          id="growth-every-n"
          label="每 N 轮反思一次"
          description="5–200 轮，默认 20"
          value={draft.everyNTurns}
          min={5}
          max={200}
          onChange={(everyNTurns) => setDraft({ ...draft, everyNTurns })}
        />
        <NumberField
          id="growth-budget"
          label="每日 token 预算"
          description="0 表示停用，默认 60000"
          value={draft.dailyBudget}
          min={0}
          max={500000}
          onChange={(dailyBudget) => setDraft({ ...draft, dailyBudget })}
        />
      </div>
      <div className="flex items-center gap-3">
        <Button size="sm" disabled={!dirty || saving} onClick={() => void save()}>
          保存
        </Button>
        {message && <span className="text-xs text-muted-foreground">{message}</span>}
      </div>
    </div>
  )
}
