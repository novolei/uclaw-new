/**
 * WorkspaceFilesView — RightSidePanel 的 Files tab 内容渲染
 *
 * 三个区:
 *   - 附加目录(workspace 级 + session 级):用户主动 attach 的外部文件夹
 *   - 会话文件(sessionPath 存在时):当前 agent 会话的专属文件树
 *   - 工作区文件:当前工作区的共享文件树
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { FolderOpen, RefreshCw, Info, FolderHeart, Plus, X, ExternalLink, FolderPlus } from 'lucide-react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { FileBrowser, FileDropZone } from '@/components/file-browser'
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
  agentPendingFilesAtom,
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
} from '@/atoms/agent-atoms'
import { workspacesAtom } from '@/atoms/workspace'
import { toast } from 'sonner'
import {
  attachWorkspaceDirectory,
  detachWorkspaceDirectory,
  attachSessionDirectory,
  detachSessionDirectory,
  openFolderDialog,
  showInFinder,
  uploadWorkspaceFile,
} from '@/lib/tauri-bridge'
import type { FileEntry } from '@/lib/chat-types'
import type { AgentPendingFile } from '@/lib/agent-types'

interface WorkspaceFilesViewProps {
  sessionId: string
  sessionPath: string | null
}

export function WorkspaceFilesView({ sessionId, sessionPath }: WorkspaceFilesViewProps): React.ReactElement {
  const setSidePanelOpenMap = useSetAtom(agentSidePanelOpenMapAtom)

  const filesVersion = useAtomValue(workspaceFilesVersionAtom)
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)

  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId

  const workspaces = useAtomValue(workspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = workspaces.find((w) => w.id === currentWorkspaceId)
    return ws?.path ?? null
  }, [workspaces, currentWorkspaceId])

  // Attached dirs (workspace + session).
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedMap.get(currentWorkspaceId) ?? []) : []

  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const sessionAttachedDirs = sessionAttachedMap.get(sessionId) ?? []

  const handleAttachWorkspaceDir = React.useCallback(async () => {
    if (!currentWorkspaceId) return
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachWorkspaceDirectory(currentWorkspaceId, picked.path)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(currentWorkspaceId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] attach workspace dir failed', err)
    }
  }, [currentWorkspaceId, setWsAttachedMap])

  const handleDetachWorkspaceDir = React.useCallback(async (dirPath: string) => {
    if (!currentWorkspaceId) return
    try {
      const updated = await detachWorkspaceDirectory(currentWorkspaceId, dirPath)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(currentWorkspaceId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] detach workspace dir failed', err)
    }
  }, [currentWorkspaceId, setWsAttachedMap])

  const handleAttachSessionDir = React.useCallback(async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachSessionDirectory(sessionId, picked.path)
      setSessionAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(sessionId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] attach session dir failed', err)
    }
  }, [sessionId, setSessionAttachedMap])

  const handleDetachSessionDir = React.useCallback(async (dirPath: string) => {
    try {
      const updated = await detachSessionDirectory(sessionId, dirPath)
      setSessionAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(sessionId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] detach session dir failed', err)
    }
  }, [sessionId, setSessionAttachedMap])

  const handleFilesDropped = React.useCallback(async (files: File[]) => {
    if (!currentWorkspaceId) {
      toast.error('请先选择工作区')
      return
    }
    for (const file of files) {
      try {
        const buf = await file.arrayBuffer()
        const bytes = Array.from(new Uint8Array(buf))
        const writtenPath = await uploadWorkspaceFile(currentWorkspaceId, file.name, bytes)
        console.debug('[upload] wrote', writtenPath)
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        toast.error(`上传 ${file.name} 失败: ${msg}`)
      }
    }
    setFilesVersion((v) => v + 1)
  }, [currentWorkspaceId, setFilesVersion])

  const handleRefresh = React.useCallback(() => {
    setFilesVersion((prev) => prev + 1)
  }, [setFilesVersion])

  // Add file to chat — image previews via convertFileSrc (Tauri asset protocol).
  const pendingFiles = useAtomValue(agentPendingFilesAtom)
  const setPendingFiles = useSetAtom(agentPendingFilesAtom)
  const handleAddToChat = React.useCallback((entry: FileEntry) => {
    if (pendingFiles.some((f) => f.sourcePath === entry.path)) return
    const ext = entry.name.split('.').pop()?.toLowerCase() ?? ''
    const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])
    const mimeExt = ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext
    const mediaType = imageExts.has(ext) ? `image/${mimeExt}` : 'application/octet-stream'
    const previewUrl = imageExts.has(ext) ? convertFileSrc(entry.path) : undefined

    const pending: AgentPendingFile = {
      id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      filename: entry.name,
      mediaType,
      size: 0,
      previewUrl,
      sourcePath: entry.path,
    }

    setPendingFiles((prev) => [...prev, pending])
  }, [pendingFiles, setPendingFiles])

  const breadcrumb = React.useMemo(() => {
    if (!sessionPath) return ''
    const parts = sessionPath.split('/').filter(Boolean)
    return parts.length > 2 ? `.../${parts.slice(-2).join('/')}` : sessionPath
  }, [sessionPath])

  // Auto-open right panel when files change (Phase 1 behavior preserved).
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

  // Combined attached-dirs list — workspace level (globe) + session level (chat).
  const allAttachedDirs = React.useMemo(() => {
    const out: Array<{ path: string; scope: 'workspace' | 'session' }> = []
    for (const p of wsAttachedDirs) out.push({ path: p, scope: 'workspace' })
    for (const p of sessionAttachedDirs) out.push({ path: p, scope: 'session' })
    return out
  }, [wsAttachedDirs, sessionAttachedDirs])

  return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">

          {/* ===== Attached directories section ===== */}
          {(allAttachedDirs.length > 0 || currentWorkspaceId) && (
            <div className="flex-shrink-0 border-b border-border/40">
              <div className="flex items-center gap-1 px-3 pt-2 pb-1 h-[28px]">
                <FolderPlus className="size-3 text-muted-foreground" />
                <span className="text-[11px] font-medium text-muted-foreground">附加目录</span>
                <div className="flex-1" />
                {currentWorkspaceId && (
                  <button
                    type="button"
                    onClick={handleAttachWorkspaceDir}
                    className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                    title="附加目录到工作区"
                  >
                    <Plus className="size-3" />
                  </button>
                )}
                {sessionPath && (
                  <button
                    type="button"
                    onClick={handleAttachSessionDir}
                    className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                    title="附加目录到会话"
                  >
                    <Plus className="size-3" />
                  </button>
                )}
              </div>
              {allAttachedDirs.map((d) => (
                <div key={`${d.scope}:${d.path}`} className="group flex items-center gap-1 px-3 py-0.5 text-[11px]">
                  <span className="text-muted-foreground/60">{d.scope === 'workspace' ? '🌐' : '💬'}</span>
                  <span className="truncate flex-1" title={d.path}>{d.path}</span>
                  <button
                    type="button"
                    onClick={() => d.scope === 'workspace' ? handleDetachWorkspaceDir(d.path) : handleDetachSessionDir(d.path)}
                    className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive p-0.5 rounded"
                    title="移除"
                  >
                    <X className="size-3" />
                  </button>
                </div>
              ))}
            </div>
          )}

          {/* ===== Session files section ===== */}
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
                    <p>当前会话的专属文件,仅本次对话的 Agent 可以访问</p>
                  </TooltipContent>
                </Tooltip>
                <span className="text-[10px] text-muted-foreground/75 truncate flex-1" title={sessionPath}>
                  {breadcrumb}
                </span>
                <button
                  type="button"
                  onClick={() => showInFinder(sessionPath)}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="在 Finder 打开"
                >
                  <ExternalLink className="size-2.5" />
                </button>
                <button
                  type="button"
                  onClick={handleRefresh}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="刷新文件列表"
                >
                  <RefreshCw className="size-2.5" />
                </button>
              </div>
              <div className="flex-1 min-h-0 overflow-y-auto">
                <FileBrowser
                  rootPath={sessionPath}
                  version={filesVersion}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
                <FileDropZone
                  hint="拖入会话目录"
                  onDrop={handleFilesDropped}
                />
              </div>
              <div className="mx-3 my-3 border-t border-muted-foreground/20" />
            </>
          )}

          {/* ===== Workspace files section ===== */}
          <div className="flex-1 min-h-0 flex flex-col mx-2 mb-2">
            <div className="flex items-center gap-1 px-2 h-[32px] flex-shrink-0">
              <FolderHeart className="size-3 text-muted-foreground" />
              <span className="text-[11px] font-medium text-muted-foreground shrink-0">工作区文件</span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Info className="size-3 text-muted-foreground/50 cursor-help shrink-0" />
                </TooltipTrigger>
                <TooltipContent side="bottom" className="max-w-[220px]">
                  <p>工作区内所有会话可访问的文件和文件夹,每个新对话都可以自动读取</p>
                </TooltipContent>
              </Tooltip>
              {workspaceFilesPath && (
                <span
                  className="text-[10px] text-muted-foreground/60 truncate flex-1 min-w-0"
                  title={workspaceFilesPath}
                >
                  {workspaceFilesPath.split('/').pop() || workspaceFilesPath}
                </span>
              )}
              {!workspaceFilesPath && <div className="flex-1" />}
              {workspaceFilesPath && (
                <button
                  type="button"
                  onClick={() => showInFinder(workspaceFilesPath)}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors shrink-0"
                  title="在 Finder 打开"
                >
                  <ExternalLink className="size-2.5" />
                </button>
              )}
            </div>
            <div className="flex-1 min-h-0 overflow-y-auto pb-1">
              {workspaceFilesPath && (
                <FileBrowser
                  rootPath={workspaceFilesPath}
                  version={filesVersion}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
              )}
              <FileDropZone
                hint="拖入工作区文件 / 文件夹"
                onDrop={handleFilesDropped}
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
