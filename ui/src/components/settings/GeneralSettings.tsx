import { useState, useEffect } from 'react'
import { useAtom } from 'jotai'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsSelect } from './primitives/SettingsSelect'
import { SettingsToggle } from './primitives/SettingsToggle'
import { getSettings, patchSettings } from '@/lib/tauri-bridge'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'

const LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
]

export function GeneralSettings() {
  const [language, setLanguage] = useState('zh-CN')
  const [sendOnEnter, setSendOnEnter] = useState(true)
  const [showTimestamp, setShowTimestamp] = useState(true)
  const [bottomDockEnabled, setBottomDockEnabled] = useAtom(bottomDockEnabledAtom)

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

      <SettingsSection title="外观">
        <SettingsCard>
          <SettingsToggle
            label="底部 Dock 导航栏"
            description="触底滑出，macOS Dock 风格快速导航。开启后鼠标移至窗口底边缘时 Dock 自动滑出，移开后自动收回。"
            checked={bottomDockEnabled}
            onCheckedChange={setBottomDockEnabled}
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
