/**
 * GlobalShortcuts — 全局快捷键监听器
 *
 * 在 App 层级监听全局快捷键（新建对话、切换侧边栏、设置等）。
 * 不渲染任何 UI。
 * 从 Proma 迁移。
 */

import { useAtomValue, useSetAtom } from 'jotai'
import { useShortcuts } from '@/hooks/useShortcut'
import { workspacesAtom, selectWorkspaceAtom } from '@/atoms/workspace'

export function GlobalShortcuts(): null {
  const workspaces = useAtomValue(workspacesAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)

  const workspaceShortcuts = Array.from({ length: 9 }, (_, i) => ({
    id: `switch-workspace-${i + 1}`,
    handler: () => {
      const ws = workspaces[i]
      if (!ws) return
      void selectWorkspace(ws.id)
    },
  }))

  useShortcuts([
    {
      id: 'new-chat',
      handler: () => {
        // [PLACEHOLDER] 新建对话 — 需要接入 useCreateSession hook
        console.log('[GlobalShortcuts] new-chat triggered')
      },
    },
    {
      id: 'new-agent-session',
      handler: () => {
        // [PLACEHOLDER] 新建 Agent 会话
        console.log('[GlobalShortcuts] new-agent-session triggered')
      },
    },
    {
      id: 'open-settings',
      handler: () => {
        // [PLACEHOLDER] 打开设置 — 需要接入 tab-atoms
        console.log('[GlobalShortcuts] open-settings triggered')
      },
    },
    {
      id: 'global-search',
      handler: () => {
        // [PLACEHOLDER] 全局搜索
        console.log('[GlobalShortcuts] global-search triggered')
      },
    },
    {
      id: 'focus-input',
      handler: () => {
        // 尝试聚焦输入框
        const input = document.querySelector<HTMLTextAreaElement>('textarea[data-input-main]')
        input?.focus()
      },
    },
    ...workspaceShortcuts,
  ])

  return null
}
