import { useState } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsInput } from './primitives/SettingsInput'
import { SettingsTextArea } from './primitives/SettingsTextArea'

export function BotDefaultSettings() {
  const [botName, setBotName] = useState('uClaw Assistant')
  const [greeting, setGreeting] = useState('你好！有什么我可以帮助你的吗？')
  const [enableHistory, setEnableHistory] = useState(true)
  const [enableTools, setEnableTools] = useState(true)

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">Bot 默认设置</h2>

      <SettingsSection title="Bot 配置" description="新建 Bot 时的默认配置">
        <SettingsInput
          label="Bot 名称"
          value={botName}
          onChange={(e) => setBotName(e.target.value)}
          placeholder="输入 Bot 名称"
        />
        <SettingsTextArea
          label="欢迎语"
          value={greeting}
          onChange={(e) => setGreeting(e.target.value)}
          placeholder="Bot 的开场白"
          rows={3}
        />
      </SettingsSection>

      <SettingsSection title="默认能力">
        <SettingsToggle
          label="对话历史"
          description="Bot 可以记住对话上下文"
          checked={enableHistory}
          onCheckedChange={setEnableHistory}
        />
        <SettingsToggle
          label="工具调用"
          description="Bot 可以使用已启用的工具"
          checked={enableTools}
          onCheckedChange={setEnableTools}
        />
      </SettingsSection>
    </div>
  )
}
