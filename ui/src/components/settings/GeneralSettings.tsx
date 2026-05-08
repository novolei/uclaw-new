import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsSelect } from './primitives/SettingsSelect'
import { SettingsToggle } from './primitives/SettingsToggle'
import { getSettings, patchSettings } from '@/lib/tauri-bridge'

const LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
]

export function GeneralSettings() {
  const [language, setLanguage] = useState('zh-CN')
  const [sendOnEnter, setSendOnEnter] = useState(true)
  const [showTimestamp, setShowTimestamp] = useState(true)

  useEffect(() => {
    getSettings().then((s) => {
      if (s.language) setLanguage(s.language)
    })
  }, [])

  const handleLanguageChange = async (value: string) => {
    setLanguage(value)
    await patchSettings({ language: value })
  }

  return (
    <div className="space-y-6">
      <SettingsSection title="语言与地区">
        <SettingsCard>
          <SettingsRow label="界面语言" description="切换后需要重新加载">
            <SettingsSelect
              value={language}
              onValueChange={handleLanguageChange}
              options={LANGUAGE_OPTIONS}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="消息">
        <SettingsCard>
          <SettingsToggle
            label="按 Enter 发送消息"
            description="关闭后使用 Ctrl+Enter 发送"
            checked={sendOnEnter}
            onCheckedChange={setSendOnEnter}
          />
          <SettingsToggle
            label="显示消息时间戳"
            checked={showTimestamp}
            onCheckedChange={setShowTimestamp}
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
