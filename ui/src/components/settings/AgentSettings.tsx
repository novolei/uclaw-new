import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsSelect } from './primitives/SettingsSelect'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsCard } from './primitives/SettingsCard'
import { getActiveModel, listProviders, setActiveModel } from '@/lib/tauri-bridge'
import type { ProviderInfo } from '@/lib/types'

export function AgentSettings() {
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [activeProvider, setActiveProvider] = useState('')
  const [activeModelId, setActiveModelId] = useState('')
  const [streamResponse, setStreamResponse] = useState(true)
  const [autoTitle, setAutoTitle] = useState(true)

  useEffect(() => {
    listProviders().then(setProviders)
    getActiveModel().then((m) => {
      if (m) {
        setActiveProvider(m.providerId)
        setActiveModelId(m.modelId)
      }
    })
  }, [])

  const handleProviderChange = async (providerId: string) => {
    setActiveProvider(providerId)
    // Reset model when provider changes
    setActiveModelId('')
  }

  const handleModelChange = async (modelId: string) => {
    setActiveModelId(modelId)
    if (activeProvider && modelId) {
      await setActiveModel(activeProvider, modelId)
    }
  }

  const providerOptions = providers.map((p) => ({
    value: p.id,
    label: p.displayName,
  }))

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">Agent 配置</h2>

      <SettingsSection title="默认模型">
        <SettingsCard>
          <SettingsRow label="Provider" description="选择 AI 模型供应商">
            <SettingsSelect
              value={activeProvider}
              onValueChange={handleProviderChange}
              options={providerOptions}
              placeholder="选择 Provider..."
            />
          </SettingsRow>
          <SettingsRow label="模型" description="选择具体模型">
            <SettingsSelect
              value={activeModelId}
              onValueChange={handleModelChange}
              options={[]}
              placeholder="选择模型..."
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="行为设置">
        <SettingsToggle
          label="流式响应"
          description="实时显示 AI 生成内容"
          checked={streamResponse}
          onCheckedChange={setStreamResponse}
        />
        <SettingsToggle
          label="自动生成标题"
          description="根据对话内容自动命名会话"
          checked={autoTitle}
          onCheckedChange={setAutoTitle}
        />
      </SettingsSection>
    </div>
  )
}
