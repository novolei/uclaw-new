import { useState } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsTextArea } from './primitives/SettingsTextArea'
import { Button } from '@/components/ui/button'

export function PromptSettings() {
  const [systemPrompt, setSystemPrompt] = useState('')
  const [saved, setSaved] = useState(false)

  const handleSave = () => {
    // [PLACEHOLDER - Tauri adaptation needed] Save custom system prompt
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">提示词设置</h2>

      <SettingsSection
        title="自定义系统提示词"
        description="追加到系统提示词末尾，用于自定义 Agent 行为"
      >
        <SettingsTextArea
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          placeholder="例如：你是一个专注于代码审查的助手，请始终使用中文回复。"
          rows={6}
        />
        <div className="flex items-center gap-2">
          <Button size="sm" onClick={handleSave}>
            保存
          </Button>
          {saved && (
            <span className="text-xs text-green-600">已保存</span>
          )}
        </div>
      </SettingsSection>
    </div>
  )
}
