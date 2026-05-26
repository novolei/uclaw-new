import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useAtom, useAtomValue } from 'jotai'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { PersonaStudio } from './PersonaStudio'
import { PersonaBondTimeline } from './PersonaBondTimeline'
import { activeProviderModelAtom } from '@/atoms/active-model'
import { planModeSuggestEnabledAtom } from '@/atoms/ui-preferences'
import { Cpu } from 'lucide-react'

export function AgentSettings() {
  const activeModel = useAtomValue(activeProviderModelAtom)
  const [streamResponse, setStreamResponse] = useState(true)
  const [autoTitle, setAutoTitle] = useState(true)
  const [planSuggestEnabled, setPlanSuggestEnabled] = useAtom(planModeSuggestEnabledAtom)

  const handlePlanSuggestChange = async (v: boolean) => {
    setPlanSuggestEnabled(v)
    try {
      await invoke('set_plan_mode_suggest_enabled', { enabled: v })
    } catch (e) {
      console.error('[AgentSettings] set_plan_mode_suggest_enabled failed', e)
    }
  }

  return (
    <div className="space-y-6">
      <PersonaStudio />
      <PersonaBondTimeline />

      <SettingsSection title="当前模型">
        <SettingsCard>
          <SettingsRow
            label="活跃模型"
            description="在会话输入框底部工具栏的模型选择器中切换"
            icon={<Cpu size={15} className="text-muted-foreground" />}
          >
            <span className="text-sm text-muted-foreground">
              {activeModel ? `${activeModel.providerId} / ${activeModel.modelId}` : '未选择'}
            </span>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="行为设置">
        <SettingsCard>
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
          <SettingsToggle
            label="为复杂任务建议 Plan 模式"
            description="检测到多步骤构建/重构/设计请求时弹出建议横幅；可被 agent 主动调用，也按关键词触发。"
            checked={planSuggestEnabled}
            onCheckedChange={handlePlanSuggestChange}
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
