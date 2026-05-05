/**
 * capabilities-toast — 能力变化提示
 *
 * 当工作区的 MCP 服务器或 Skills 配置发生变化时，
 * 生成对应的 toast 提示。
 *
 * 从 Proma 迁移。
 */

import { toast } from 'sonner'
import type { WorkspaceCapabilities } from '@/lib/proma-types'

export interface CapabilityChange {
  type: 'mcp_added' | 'mcp_removed' | 'mcp_toggled' | 'skill_added' | 'skill_removed'
  name: string
  enabled?: boolean
}

/**
 * 对比两组 capabilities，返回变化列表
 */
export function diffCapabilities(
  prev: WorkspaceCapabilities,
  next: WorkspaceCapabilities,
): CapabilityChange[] {
  const changes: CapabilityChange[] = []

  const prevMcpMap = new Map(prev.mcpServers.map((s) => [s.name, s]))
  const nextMcpMap = new Map(next.mcpServers.map((s) => [s.name, s]))

  // 新增 MCP
  for (const [name, server] of nextMcpMap) {
    if (!prevMcpMap.has(name)) {
      changes.push({ type: 'mcp_added', name, enabled: server.enabled })
    } else {
      const prevServer = prevMcpMap.get(name)!
      if (prevServer.enabled !== server.enabled) {
        changes.push({ type: 'mcp_toggled', name, enabled: server.enabled })
      }
    }
  }

  // 移除 MCP
  for (const name of prevMcpMap.keys()) {
    if (!nextMcpMap.has(name)) {
      changes.push({ type: 'mcp_removed', name })
    }
  }

  const prevSkillNames = new Set(prev.skills.map((s) => s.name))
  const nextSkillNames = new Set(next.skills.map((s) => s.name))

  // 新增 Skills
  for (const name of nextSkillNames) {
    if (!prevSkillNames.has(name)) {
      changes.push({ type: 'skill_added', name })
    }
  }

  // 移除 Skills
  for (const name of prevSkillNames) {
    if (!nextSkillNames.has(name)) {
      changes.push({ type: 'skill_removed', name })
    }
  }

  return changes
}

/**
 * 将能力变化转换为用户可读的提示消息
 */
export function formatCapabilityChange(change: CapabilityChange): string {
  switch (change.type) {
    case 'mcp_added':
      return `MCP 服务 "${change.name}" 已添加`
    case 'mcp_removed':
      return `MCP 服务 "${change.name}" 已移除`
    case 'mcp_toggled':
      return `MCP 服务 "${change.name}" 已${change.enabled ? '启用' : '禁用'}`
    case 'skill_added':
      return `技能 "${change.name}" 已添加`
    case 'skill_removed':
      return `技能 "${change.name}" 已移除`
  }
}

/**
 * 显示能力变化的 toast 提示
 * 由 App Shell 层调用
 */
export function showCapabilityChangeToasts(changes: CapabilityChange[]): void {
  for (const change of changes) {
    const message = formatCapabilityChange(change)
    console.info('[Capabilities]', message)
    toast.info(message)
  }
}
