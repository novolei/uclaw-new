import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsSegmentedControl } from './primitives/SettingsSegmentedControl'
import { getSettings, patchSettings } from '@/lib/tauri-bridge'
import { cn } from '@/lib/utils'

const THEMES = [
  { value: 'system', label: '跟随系统', color: 'bg-gradient-to-br from-white to-zinc-900' },
  { value: 'light', label: '浅色', color: 'bg-white' },
  { value: 'dark', label: '深色', color: 'bg-zinc-900' },
  { value: 'midnight', label: '午夜蓝', color: 'bg-slate-950' },
  { value: 'forest', label: '森林绿', color: 'bg-emerald-950' },
  { value: 'sunset', label: '日落橙', color: 'bg-orange-950' },
  { value: 'ocean', label: '海洋蓝', color: 'bg-sky-950' },
  { value: 'rose', label: '玫瑰粉', color: 'bg-rose-950' },
]

const FONT_SIZE_OPTIONS = [
  { value: 'small', label: '小' },
  { value: 'medium', label: '中' },
  { value: 'large', label: '大' },
]

export function AppearanceSettings() {
  const [theme, setTheme] = useState('system')
  const [fontSize, setFontSize] = useState('medium')

  useEffect(() => {
    getSettings().then((s) => {
      if (s.theme) setTheme(s.theme)
    })
  }, [])

  const handleThemeChange = async (value: string) => {
    setTheme(value)
    await patchSettings({ theme: value })
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">外观设置</h2>

      <SettingsSection title="主题" description="选择你喜欢的界面主题">
        <div className="grid grid-cols-4 gap-3">
          {THEMES.map((t) => (
            <button
              key={t.value}
              type="button"
              onClick={() => handleThemeChange(t.value)}
              className={cn(
                'flex flex-col items-center gap-2 p-3 rounded-lg border-2 transition-colors',
                theme === t.value
                  ? 'border-primary bg-accent'
                  : 'border-transparent hover:border-border'
              )}
            >
              <div className={cn('w-10 h-10 rounded-full border border-border', t.color)} />
              <span className="text-xs text-muted-foreground">{t.label}</span>
            </button>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection title="字体大小">
        <SettingsRow label="界面字体大小">
          <SettingsSegmentedControl
            value={fontSize}
            onValueChange={setFontSize}
            options={FONT_SIZE_OPTIONS}
          />
        </SettingsRow>
      </SettingsSection>
    </div>
  )
}
