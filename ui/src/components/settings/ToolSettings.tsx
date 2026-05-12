import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsCard } from './primitives/SettingsCard'
import { listMcpServers, listSkills, toggleSkill, forkSkillToUser, reloadSkills } from '@/lib/tauri-bridge'
import type { McpServerInfo, SkillInfo } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { McpServerForm } from './McpServerForm'
import { toast } from 'sonner'

const PROVENANCE_BADGE: Record<NonNullable<SkillInfo['provenance']>, { label: string; className: string }> = {
  bundled: { label: 'Bundled', className: 'bg-primary/10 text-primary border-primary/20' },
  user:    { label: 'User',    className: 'bg-emerald-500/10 text-emerald-600 border-emerald-500/20 dark:text-emerald-400' },
  project: { label: 'Project', className: 'bg-muted text-muted-foreground border-border' },
}

export function ToolSettings() {
  const [mcpServers, setMcpServers] = useState<McpServerInfo[]>([])
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [showMcpForm, setShowMcpForm] = useState(false)
  const [forkingName, setForkingName] = useState<string | null>(null)

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

  const handleFork = async (name: string) => {
    setForkingName(name)
    try {
      const destPath = await forkSkillToUser(name)
      toast.success(`已 Fork 到 ${destPath}`, {
        description: '现在可以在 ~/.uclaw/skills/ 下编辑这份 skill；它会自动覆盖 bundled 原版。',
      })
      // Backend re-discovers internally; refresh the local list so the
      // provenance badge flips to "User" without a manual reload.
      const fresh = await reloadSkills()
      setSkills(fresh)
    } catch (e) {
      toast.error('Fork 失败', {
        description: String(e),
      })
    } finally {
      setForkingName(null)
    }
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

      <SettingsSection
        title="内置技能"
        description="启用或禁用 Agent 可用的技能。Bundled = 应用自带（只读）；User = 你的 ~/.uclaw/skills/ 副本；Fork 可把 Bundled 复制到 User 以便编辑。"
      >
        <div className="space-y-2">
          {skills.map((skill) => {
            const tier = skill.provenance ?? 'project'
            const badge = PROVENANCE_BADGE[tier]
            return (
              <SettingsCard key={skill.name}>
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-sm font-medium truncate">{skill.name}</span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded border ${badge.className}`}>
                        {badge.label}
                      </span>
                      {skill.category && (
                        <span className="text-[10px] text-muted-foreground/60">{skill.category}</span>
                      )}
                    </div>
                    {skill.description && (
                      <div className="text-xs text-muted-foreground mt-1 break-words">
                        {skill.description}
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-2 flex-shrink-0">
                    {tier === 'bundled' && (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={forkingName === skill.name}
                        onClick={() => handleFork(skill.name)}
                      >
                        {forkingName === skill.name ? 'Forking…' : 'Fork 到我的'}
                      </Button>
                    )}
                    <SettingsToggle
                      label=""
                      description=""
                      checked={skill.enabled}
                      onCheckedChange={(checked) => handleSkillToggle(skill.name, checked)}
                    />
                  </div>
                </div>
              </SettingsCard>
            )
          })}
        </div>
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
