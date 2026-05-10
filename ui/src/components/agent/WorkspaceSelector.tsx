/**
 * WorkspaceSelector — Agent 工作区切换器
 *
 * 垂直列表展示所有工作区，支持新建、重命名、删除、切换和拖拽排序。
 * 切换工作区后持久化到 settings。
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { FolderOpen, Plus, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import {
  agentWorkspacesAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'
import { workspaceListHeightAtom } from '@/atoms/sidebar-atoms'
import type { AgentWorkspace } from '@/lib/agent-types'
import {
  updateSettings,
  createWorkspace,
  deleteWorkspace,
} from '@/lib/tauri-bridge'

export function WorkspaceSelector(): React.ReactElement {
  const [workspaces, setWorkspaces] = useAtom(agentWorkspacesAtom)
  const [currentWorkspaceId, setCurrentWorkspaceId] = useAtom(currentAgentWorkspaceIdAtom)
  const [listHeight, setListHeight] = useAtom(workspaceListHeightAtom)

  // 高度拖拽调整
  const listRef = React.useRef<HTMLDivElement>(null)
  const resizing = React.useRef(false)
  const startY = React.useRef(0)
  const startH = React.useRef(0)
  const cleanupResizeRef = React.useRef<(() => void) | null>(null)

  React.useEffect(() => {
    return () => { cleanupResizeRef.current?.() }
  }, [])

  const handleResizeStart = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      resizing.current = true
      startY.current = e.clientY
      // 用实际渲染高度作为起点，避免 maxHeight > 实际高度时不跟手
      startH.current = listRef.current?.getBoundingClientRect().height ?? 120

      const onMove = (ev: MouseEvent): void => {
        if (!resizing.current) return
        const delta = ev.clientY - startY.current
        const next = Math.min(400, Math.max(80, startH.current + delta))
        setListHeight(next)
      }
      const onUp = (): void => {
        resizing.current = false
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
        document.body.style.cursor = ''
        document.body.style.userSelect = ''
        cleanupResizeRef.current = null
      }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
      document.body.style.cursor = 'row-resize'
      document.body.style.userSelect = 'none'
      cleanupResizeRef.current = onUp
    },
    [setListHeight],
  )

  // 新建状态
  const [creating, setCreating] = React.useState(false)
  const [newName, setNewName] = React.useState('')
  const createInputRef = React.useRef<HTMLInputElement>(null)
  /** 防止连续 Enter 触发多次创建请求 */
  const createInFlightRef = React.useRef(false)

  // 删除确认状态
  const [deleteTargetId, setDeleteTargetId] = React.useState<string | null>(null)

  /** 切换工作区 */
  const handleSelect = (workspace: AgentWorkspace): void => {
    setCurrentWorkspaceId(workspace.id)

    updateSettings({
      agentWorkspaceId: workspace.id,
    }).catch(console.error)
  }

  // ===== 新建 =====

  const handleStartCreate = (): void => {
    setCreating(true)
    setNewName('')
    requestAnimationFrame(() => {
      createInputRef.current?.focus()
    })
  }

  const handleCreate = async (): Promise<void> => {
    const trimmed = newName.trim()
    if (!trimmed) {
      setCreating(false)
      return
    }
    if (createInFlightRef.current) return
    createInFlightRef.current = true

    try {
      // Real backend command: returns { id, name, icon, path, createdAt }.
      // Adapter normalizes to AgentWorkspace shape (slug removed in Task 8).
      const created = await createWorkspace(trimmed)
      const workspace: AgentWorkspace = {
        id: created.id,
        name: created.name,
        createdAt: Date.parse(created.createdAt) || Date.now(),
        updatedAt: Date.parse(created.createdAt) || Date.now(),
      }
      setWorkspaces((prev) => [workspace, ...prev])
      setCurrentWorkspaceId(workspace.id)
      setCreating(false)

      updateSettings({
        agentWorkspaceId: workspace.id,
      }).catch(console.error)
    } catch (error) {
      const msg = error instanceof Error ? error.message : '创建失败'
      toast.error(msg)
      setCreating(false)
    } finally {
      createInFlightRef.current = false
    }
  }

  const handleCreateKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      if (e.nativeEvent.isComposing) return
      e.preventDefault()
      handleCreate()
    } else if (e.key === 'Escape') {
      setCreating(false)
    }
  }

  // ===== 删除 =====

  const handleStartDelete = (e: React.MouseEvent, wsId: string): void => {
    e.stopPropagation()
    setDeleteTargetId(wsId)
  }

  const handleConfirmDelete = async (): Promise<void> => {
    if (!deleteTargetId) return

    try {
      await deleteWorkspace(deleteTargetId)
      const remaining = workspaces.filter((w) => w.id !== deleteTargetId)
      setWorkspaces(remaining)

      if (deleteTargetId === currentWorkspaceId && remaining.length > 0) {
        setCurrentWorkspaceId(remaining[0]!.id)
        updateSettings({
          agentWorkspaceId: remaining[0]!.id,
        }).catch(console.error)
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : '删除失败'
      toast.error(msg)
    } finally {
      setDeleteTargetId(null)
    }
  }

  const canDelete = (ws: AgentWorkspace): boolean => {
    return ws.id !== 'default' && workspaces.length > 1
  }

  return (
    <>
      <div className="rounded-lg border border-border/60 overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between px-2.5 py-1.5 border-b border-border/40">
          <span className="text-[11px] font-medium text-foreground/50 uppercase tracking-wide">工作区</span>
          <button
            onClick={handleStartCreate}
            className="p-1 rounded hover:bg-foreground/[0.06] text-foreground/35 hover:text-foreground/60 transition-colors titlebar-no-drag"
            title="新建工作区"
          >
            <Plus size={13} />
          </button>
        </div>

        {/* 工作区列表 */}
        <div
          ref={listRef}
          className="overflow-y-auto scrollbar-thin flex flex-col p-1"
          style={{ maxHeight: listHeight }}
        >
          {workspaces.map((ws) => (
            <div key={ws.id} className="relative">
              <div
                onClick={() => handleSelect(ws)}
                className={cn(
                  'group w-full flex items-center gap-1 px-1 py-[5px] rounded-md text-[13px] transition-colors duration-100 cursor-pointer titlebar-no-drag',
                  ws.id === currentWorkspaceId
                    ? 'workspace-item-selected bg-foreground/[0.08] text-foreground shadow-[0_1px_2px_0_rgba(0,0,0,0.05)]'
                    : 'text-foreground/70 hover:bg-foreground/[0.04]',
                )}
              >
                <FolderOpen size={13} className="flex-shrink-0 text-foreground/40" />

                <span className="flex-1 min-w-0 truncate">{ws.name}</span>

                <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                  {canDelete(ws) && (
                    <button
                      onClick={(e) => handleStartDelete(e, ws.id)}
                      className="p-0.5 rounded hover:bg-destructive/10 text-foreground/30 hover:text-destructive transition-colors"
                      title="删除"
                    >
                      <Trash2 size={12} />
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}

          {/* 新建工作区输入框 */}
          {creating && (
            <div className="flex items-center gap-2 px-2 py-[5px]">
              <FolderOpen size={13} className="flex-shrink-0 text-foreground/40" />
              <input
                ref={createInputRef}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={handleCreateKeyDown}
                onBlur={() => setCreating(false)}
                placeholder="工作区名称..."
                className="flex-1 min-w-0 bg-transparent text-[13px] text-foreground border-b border-primary/50 outline-none px-0.5"
                maxLength={50}
              />
            </div>
          )}
        </div>

        {/* 拖拽调整高度的 handle */}
        <div
          onMouseDown={handleResizeStart}
          className="h-1 cursor-row-resize group/resize flex items-center justify-center hover:bg-foreground/[0.06] transition-colors titlebar-no-drag"
        >
          <div className="w-8 h-[2px] rounded-full bg-foreground/0 group-hover/resize:bg-foreground/20 transition-colors" />
        </div>
      </div>

      {/* 删除确认弹窗 */}
      <AlertDialog
        open={deleteTargetId !== null}
        onOpenChange={(v) => { if (!v) setDeleteTargetId(null) }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除工作区</AlertDialogTitle>
            <AlertDialogDescription>
              删除后工作区配置将被移除，但目录文件会保留。确定要删除吗？
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
