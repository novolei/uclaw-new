import * as React from 'react'
import { Award, ScrollText } from 'lucide-react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

export function PersonaBondTimeline(): React.ReactElement {
  return (
    <SettingsSection
      title="关系时间线"
      description="纪念物、亲密度和勋章只记录共同工作的经历，不改变 Agent 能力。"
    >
      <SettingsCard>
        <div className="space-y-4 p-3 text-sm">
          <div>
            <div className="text-xs text-muted-foreground">亲密度</div>
            <div className="mt-1 flex items-end gap-2">
              <div className="text-2xl font-semibold leading-none text-foreground">未启用</div>
              <div className="text-xs text-muted-foreground">MVP 先展示计算边界</div>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
              <div className="h-full w-0 bg-primary" />
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-md border border-border/50 p-3">
              <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                <ScrollText size={14} className="text-muted-foreground" />
                纪念物
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                成功合作后，UClaw 可以提议一张经历卡，由你确认后保存。
              </div>
            </div>

            <div className="rounded-md border border-border/50 p-3">
              <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                <Award size={14} className="text-muted-foreground" />
                勋章
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                勋章来自可解释的共同经历，只改变关系叙事，不提供额外能力。
              </div>
            </div>
          </div>
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}
