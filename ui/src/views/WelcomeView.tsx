import * as React from 'react'
import { useAtomValue, useSetAtom, useAtom } from 'jotai'
import { ArrowUp, Plus, FolderOpen } from 'lucide-react'
import { cn } from '@/lib/utils'
import { workspacesAtom, activeWorkspaceIdAtom, refreshWorkspacesAtom } from '@/atoms/workspace'
import { agentSessionsAtom, agentChannelIdAtom, agentModelIdAtom, agentSessionChannelMapAtom, agentSessionModelMapAtom, currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import { activeViewAtom } from '@/atoms/active-view'
import { WorkspaceCreateDialog } from '@/components/workspace/WorkspaceCreateDialog'
import { useOpenSession } from '@/hooks/useOpenSession'
import { createAgentSession } from '@/lib/tauri-bridge'
import type { AgentSessionMeta } from '@/lib/agent-types'

function formatRelativeTime(updatedAt: number): string {
  const now = Date.now()
  const diff = now - updatedAt
  const mins = Math.floor(diff / 60_000)
  if (mins < 1) return '刚刚'
  if (mins < 60) return `${mins} 分钟前`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours} 小时前`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days} 天前`
  return new Date(updatedAt).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })
}

export default function WelcomeView(): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [agentSessions, setAgentSessions] = useAtom(agentSessionsAtom)
  const agentChannelId = useAtomValue(agentChannelIdAtom)
  const agentModelId = useAtomValue(agentModelIdAtom)
  const setSessionChannelMap = useSetAtom(agentSessionChannelMapAtom)
  const setSessionModelMap = useSetAtom(agentSessionModelMapAtom)
  const setCurrentAgentWorkspaceId = useSetAtom(currentAgentWorkspaceIdAtom)
  const setActiveView = useSetAtom(activeViewAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)
  const openSession = useOpenSession()

  const [input, setInput] = React.useState('')
  const [submitting, setSubmitting] = React.useState(false)
  const [createDialogOpen, setCreateDialogOpen] = React.useState(false)
  const textareaRef = React.useRef<HTMLTextAreaElement>(null)

  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId) ?? workspaces[0] ?? null
  const workspaceName = activeWorkspace?.name ?? '工作区'

  const recentSessions: AgentSessionMeta[] = React.useMemo(() => {
    const wsId = activeWorkspace?.id
    const filtered = wsId
      ? agentSessions.filter((s) => s.workspaceId === wsId && !s.archived)
      : agentSessions.filter((s) => !s.archived)
    return [...filtered].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 8)
  }, [agentSessions, activeWorkspace])

  const workspaceNameMap = React.useMemo(() => {
    const map = new Map<string, string>()
    for (const w of workspaces) map.set(w.id, w.name)
    return map
  }, [workspaces])

  const handleSubmit = async (): Promise<void> => {
    const title = input.trim()
    if (!title || submitting) return
    setSubmitting(true)
    try {
      const meta = await createAgentSession(title, agentChannelId || undefined, activeWorkspace?.id || undefined)
      setAgentSessions((prev: any) => [meta, ...prev])
      if (agentChannelId) setSessionChannelMap((prev) => { const map = new Map(prev); map.set(meta.id, agentChannelId); return map })
      if (agentModelId) setSessionModelMap((prev) => { const map = new Map(prev); map.set(meta.id, agentModelId); return map })
      openSession('agent', meta.id, meta.title)
      setActiveView('conversations')
      setInput('')
    } catch (e) {
      console.error('[WelcomeView] create session failed', e)
    } finally {
      setSubmitting(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>): void => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSubmit()
    }
  }

  const handleSelectSession = (session: AgentSessionMeta): void => {
    openSession('agent', session.id, session.title)
    setActiveView('conversations')
  }

  const handleWorkspaceCreated = (ws: { id: string; name: string; icon: string }): void => {
    refreshWorkspaces()
    setCurrentAgentWorkspaceId(ws.id)
  }

  React.useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  }, [input])

  // Auto-focus input on mount
  React.useEffect(() => {
    textareaRef.current?.focus()
  }, [])

  return (
    <div className="flex-1 flex flex-col items-center justify-center px-6 py-12 overflow-y-auto">
      <div className="w-full max-w-[640px] flex flex-col gap-8">

        {/* Heading */}
        <div className="flex flex-col gap-1">
          <p className="text-[13px] text-muted-foreground">
            {activeWorkspace
              ? <span>今天想在 <span className="font-semibold text-foreground">{workspaceName}</span> 里完成什么？</span>
              : '今天想完成什么？'}
          </p>
          <h1 className="text-[28px] font-semibold tracking-tight text-foreground leading-tight">
            你好，有什么需要帮助的？
          </h1>
        </div>

        {/* Input box */}
        <div className="relative w-full rounded-2xl border border-border bg-muted/30 shadow-sm focus-within:border-primary/40 focus-within:shadow-md transition-all duration-150">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="描述你的任务，按 Enter 开始…"
            rows={1}
            className="w-full bg-transparent resize-none px-4 pt-4 pb-12 text-[14px] leading-relaxed text-foreground placeholder:text-muted-foreground outline-none"
            style={{ minHeight: 56 }}
          />

          {/* Context chips row + send button */}
          <div className="absolute bottom-3 left-3 right-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              {activeWorkspace && (
                <span className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-background/80 border border-border text-[11px] text-foreground/60 select-none">
                  <FolderOpen size={11} className="text-foreground/40" />
                  {activeWorkspace.icon} {activeWorkspace.name}
                </span>
              )}
            </div>

            <button
              onClick={handleSubmit}
              disabled={!input.trim() || submitting}
              className={cn(
                'flex items-center justify-center w-8 h-8 rounded-xl transition-all duration-100',
                input.trim() && !submitting
                  ? 'bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm'
                  : 'bg-muted text-muted-foreground cursor-not-allowed opacity-50'
              )}
            >
              <ArrowUp size={15} />
            </button>
          </div>
        </div>

        {/* No workspaces empty state */}
        {workspaces.length === 0 && (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <p className="text-[13px] text-muted-foreground">创建一个工作区来整理你的对话</p>
            <button
              onClick={() => setCreateDialogOpen(true)}
              className="flex items-center gap-2 px-4 py-2 rounded-xl bg-primary/10 text-primary text-[13px] font-medium hover:bg-primary/15 transition-colors"
            >
              <Plus size={14} />
              新建工作区
            </button>
          </div>
        )}

        {/* Recent sessions */}
        {recentSessions.length > 0 && (
          <div className="flex flex-col gap-2">
            <p className="text-[11px] font-medium text-muted-foreground px-1 select-none">最近对话</p>
            <div className="flex flex-col gap-0.5">
              {recentSessions.map((session) => {
                const wsName = session.workspaceId ? workspaceNameMap.get(session.workspaceId) : undefined
                return (
                  <button
                    key={session.id}
                    onClick={() => handleSelectSession(session)}
                    className="flex items-center gap-3 px-3 py-2.5 rounded-xl hover:bg-muted/60 transition-colors text-left group"
                  >
                    <span className="text-[15px] leading-none flex-shrink-0">{session.titleEmoji || '💬'}</span>
                    <span className="flex-1 min-w-0 text-[13px] text-foreground/80 group-hover:text-foreground truncate">
                      {session.title}
                    </span>
                    <div className="flex items-center gap-2 flex-shrink-0">
                      {wsName && (
                        <span className="text-[10px] text-muted-foreground/60 px-1.5 py-0.5 rounded-full bg-foreground/[0.05] max-w-[80px] truncate">
                          {wsName}
                        </span>
                      )}
                      <span className="text-[11px] text-muted-foreground/50 tabular-nums">
                        {formatRelativeTime(session.updatedAt)}
                      </span>
                    </div>
                  </button>
                )
              })}
            </div>
          </div>
        )}

        {/* Empty recent sessions when workspace exists */}
        {workspaces.length > 0 && recentSessions.length === 0 && (
          <div className="text-center py-4">
            <p className="text-[13px] text-muted-foreground">还没有对话，输入任务开始吧</p>
          </div>
        )}
      </div>

      <WorkspaceCreateDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onCreated={handleWorkspaceCreated}
      />
    </div>
  )
}
