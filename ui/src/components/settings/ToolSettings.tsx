import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsCard } from './primitives/SettingsCard'
import { listMcpServers, listSkills, toggleSkill } from '@/lib/tauri-bridge'
import type { McpServerInfo, SkillInfo } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { McpServerForm } from './McpServerForm'

export function ToolSettings() {
  const [mcpServers, setMcpServers] = useState<McpServerInfo[]>([])
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [showMcpForm, setShowMcpForm] = useState(false)

  useEffect(() => {
    listMcpServers().then(setMcpServers)
    listSkills().then(setSkills)
  }, [])

  const handleSkillToggle = async (name: string, enabled: boolean) => {
    await toggleSkill({ name, enabled })
    setSkills((prev) =>
      prev.map((s) => (s.name === name ? { ...s, enabled } : s))
    )
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">工具设置</h2>

      <SettingsSection title="MCP 服务器" description="管理 Model Context Protocol 服务器">
        {mcpServers.length > 0 ? (
          <div className="space-y-2">
            {mcpServers.map((server) => (
              <SettingsCard key={server.id}>
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-sm font-medium">{server.name}</div>
                    <div className="text-xs text-muted-foreground">{server.command}</div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className={`w-2 h-2 rounded-full ${server.status === 'connected' ? 'bg-green-500' : 'bg-red-500'}`} />
                    <span className="text-xs text-muted-foreground">
                      {server.status === 'connected' ? '已连接' : server.status}
                    </span>
                  </div>
                </div>
              </SettingsCard>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground py-4 text-center">
            暂无 MCP 服务器
          </div>
        )}
        <Button variant="outline" size="sm" onClick={() => setShowMcpForm(true)}>
          添加 MCP 服务器
        </Button>
      </SettingsSection>

      <SettingsSection title="内置技能" description="启用或禁用 Agent 可用的技能">
        {skills.map((skill) => (
          <SettingsToggle
            key={skill.name}
            label={skill.name}
            description={skill.description}
            checked={skill.enabled}
            onCheckedChange={(checked) => handleSkillToggle(skill.name, checked)}
          />
        ))}
      </SettingsSection>

      {showMcpForm && (
        <McpServerForm
          onClose={() => setShowMcpForm(false)}
          onAdded={() => {
            setShowMcpForm(false)
            listMcpServers().then(setMcpServers)
          }}
        />
      )}
    </div>
  )
}
