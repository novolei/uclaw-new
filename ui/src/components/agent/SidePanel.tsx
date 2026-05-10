/**
 * SidePanel — Agent 侧面板容器
 *
 * 直接展示文件浏览器，默认打开状态。
 * 切换按钮在面板关闭时显示活动指示点。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { FolderOpen, RefreshCw, Info, FolderHeart } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { FileBrowser, FileDropZone } from '@/components/file-browser'
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
  agentWorkspacesAtom,
  agentPendingFilesAtom,
} from '@/atoms/agent-atoms'
import type { FileEntry } from '@/lib/chat-types'
import type { AgentPendingFile } from '@/lib/agent-types'

interface WorkspaceFilesViewProps {
  sessionId: string
  sessionPath: string | null
}

/**
 * 工作区文件视图——直接作为 RightSidePanel 的 Files tab 内容渲染。
 * 不再是独立的"侧面板",没有自己的圆角/阴影/关闭按钮——这些由 RightSidePanel 提供。
 */
export function WorkspaceFilesView({ sessionId, sessionPath }: WorkspaceFilesViewProps): React.ReactElement {
  // 自动打开右侧面板的状态(被 RightSidePanel 的 Files tab 容器条件渲染,
  // 这里只负责在文件变化时把外层 isOpen 设回 true)。
  const setSidePanelOpenMap = useSetAtom(agentSidePanelOpenMapAtom)

  const filesVersion = useAtomValue(workspaceFilesVersionAtom)
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)

  // Derive current workspace from the session, fall back to the global
  // selection. (slug is removed from AgentWorkspace in Task 8 — we use id
  // throughout now.)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId

  // 文件上传完成后递增版本号，触发 FileBrowser 刷新
  const handleFilesUploaded = React.useCallback(() => {
    setFilesVersion((prev) => prev + 1)
  }, [setFilesVersion])

  // 手动刷新文件列表
  const handleRefresh = React.useCallback(() => {
    setFilesVersion((prev) => prev + 1)
  }, [setFilesVersion])

  // 添加文件到聊天
  const pendingFiles = useAtomValue(agentPendingFilesAtom)
  const setPendingFiles = useSetAtom(agentPendingFilesAtom)
  const handleAddToChat = React.useCallback((entry: FileEntry) => {
    if (pendingFiles.some((f) => f.sourcePath === entry.path)) return

    // Image preview was driven by readAttachedFile (phantom IPC). Without it
    // we just record the path; the agent input bar / send pipeline reads
    // the file from the path at send time. Phase 2 will restore previews.
    const ext = entry.name.split('.').pop()?.toLowerCase() ?? ''
    const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])
    const mimeExt = ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext
    const mediaType = imageExts.has(ext) ? `image/${mimeExt}` : 'application/octet-stream'

    const pending: AgentPendingFile = {
      id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      filename: entry.name,
      mediaType,
      size: 0,
      previewUrl: undefined,
      sourcePath: entry.path,
    }

    setPendingFiles((prev) => [...prev, pending])
  }, [pendingFiles, setPendingFiles])

  // 面包屑：显示根路径最后两段
  const breadcrumb = React.useMemo(() => {
    if (!sessionPath) return ''
    const parts = sessionPath.split('/').filter(Boolean)
    return parts.length > 2 ? `.../${parts.slice(-2).join('/')}` : sessionPath
  }, [sessionPath])

  // Workspace files path: derive from the workspace's path column (set when
  // the workspace was created with a path; null otherwise — backend's
  // active_workspace_root falls back to the global workground root).
  const workspaces = useAtomValue(agentWorkspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = workspaces.find((w) => w.id === currentWorkspaceId)
    // AgentWorkspace doesn't carry path; fall back to null. The FileBrowser
    // below only renders when this is non-null. Phase 2 will add path to
    // the AgentWorkspace shape when create_workspace returns full data.
    return ws ? null : null
  }, [workspaces, currentWorkspaceId])

  // 自动打开右侧面板：文件变化时把外层 isOpen 设为 true
  const prevFilesVersionRef = React.useRef(filesVersion)
  React.useEffect(() => {
    if (filesVersion > prevFilesVersionRef.current && sessionPath) {
      setSidePanelOpenMap((prev) => {
        const map = new Map(prev)
        map.set(sessionId, true)
        return map
      })
    }
    prevFilesVersionRef.current = filesVersion
  }, [filesVersion, sessionPath, sessionId, setSidePanelOpenMap])

  return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* ===== Session files section (only if sessionPath exists) ===== */}
          {sessionPath && (
            <>
              <div className="flex items-center gap-1 pl-3 pr-2 h-[32px] flex-shrink-0">
                <FolderOpen className="size-3 text-muted-foreground" />
                <span className="text-[11px] font-medium text-muted-foreground">会话文件</span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Info className="size-3 text-muted-foreground/50 cursor-help" />
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="max-w-[200px]">
                    <p>当前会话的专属文件，仅本次对话的 Agent 可以访问</p>
                  </TooltipContent>
                </Tooltip>
                <span
                  className="text-[10px] text-muted-foreground/75 truncate flex-1"
                  title={sessionPath}
                >
                  {breadcrumb}
                </span>
                <button
                  type="button"
                  onClick={handleRefresh}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="刷新文件列表"
                >
                  <RefreshCw className="size-2.5" />
                </button>
              </div>
              {/* Session files content (independent scroll) */}
              <div className="flex-1 min-h-0 overflow-y-auto">
                <FileBrowser
                  rootPath={sessionPath}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
                <FileDropZone
                  sessionId={sessionId}
                  target="session"
                  onFilesUploaded={handleFilesUploaded}
                />
              </div>
              {/* ===== Divider ===== */}
              <div className="mx-3 my-3 border-t border-muted-foreground/20" />
            </>
          )}

          {/* ===== Workspace files section ===== */}
          <div className="flex-1 min-h-0 flex flex-col mx-2 mb-2">
            <div className="flex items-center gap-1 px-2 h-[32px] flex-shrink-0">
              <FolderHeart className="size-3 text-muted-foreground" />
              <span className="text-[11px] font-medium text-muted-foreground">工作区文件</span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Info className="size-3 text-muted-foreground/50 cursor-help" />
                </TooltipTrigger>
                <TooltipContent side="bottom" className="max-w-[220px]">
                  <p>工作区内所有会话可访问的文件和文件夹，每个新对话都可以自动读取</p>
                </TooltipContent>
              </Tooltip>
            </div>
            {/* Workspace files content (independent scroll) */}
            <div className="flex-1 min-h-0 overflow-y-auto pb-1">
              {workspaceFilesPath && (
                <FileBrowser
                  rootPath={workspaceFilesPath}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
              )}
              <FileDropZone
                target="workspace"
                onFilesUploaded={handleFilesUploaded}
              />
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
          请选择工作区
        </div>
      )}
    </div>
  )
}
