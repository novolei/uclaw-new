/**
 * IntelligenceTab — composes ModelSettings + AgentSettings + PromptsSettings
 * as vertically-stacked sub-sections within a single tab.
 */
import * as React from 'react'
import { ModelSettings } from './ModelSettings'
import { AgentSettings } from './AgentSettings'
import { PromptsSettings } from './PromptsSettings'

export function IntelligenceTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="模型分配">
        <ModelSettings />
      </section>
      <section data-settings-section="Agent 行为">
        <AgentSettings />
      </section>
      <section data-settings-section="提示词">
        <PromptsSettings />
      </section>
    </div>
  )
}
