/**
 * FileBrowser — 文件浏览器树形视图
 *
 * 在 Agent 侧面板中展示工作区文件树。
 * 支持展开/折叠目录、单击打开文件。
 * 从 Proma 迁移，文件读取适配 Tauri。
 */

import * as React from 'react'
import { ChevronRight, RefreshCw, FolderOpen, Plus, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from './FileTypeIcon'
import type { FileEntry } from '@/lib/chat-types'
import { listDirectoryEntries } from '@/lib/tauri-bridge'

interface FileBrowserProps {
  /** 根目录路径 */
  rootPath: string
  /** 初始文件列表 */
  files?: FileEntry[]
  /** 点击文件回调 */
  onFileClick?: (entry: FileEntry) => void
  /** 点击目录回调 */
  onDirectoryClick?: (entry: FileEntry) => void
  /** 添加到聊天回调 */
  onAddToChat?: (entry: FileEntry) => void
  /** 删除文件回调 */
  onDelete?: (entry: FileEntry) => void
  /** 刷新回调 */
  onRefresh?: () => void
  /** 是否正在加载 */
  loading?: boolean
  /** 隐藏工具栏 */
  hideToolbar?: boolean
  /** 嵌入模式（无边框） */
  embedded?: boolean
  /** 无文件时隐藏 */
  hideEmpty?: boolean
  /** 自定义类名 */
  className?: string
  /** External refresh signal — bumping this re-fetches from disk
   *  (only meaningful when `files` prop is not supplied — i.e. auto-load mode). */
  version?: number
}

/** 单个文件/目录节点 */
function FileTreeNode({
  entry,
  depth = 0,
  onFileClick,
  onDirectoryClick,
  onAddToChat,
  onDelete,
}: {
  entry: FileEntry
  depth?: number
  onFileClick?: (entry: FileEntry) => void
  onDirectoryClick?: (entry: FileEntry) => void
  onAddToChat?: (entry: FileEntry) => void
  onDelete?: (entry: FileEntry) => void
}): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)

  const handleClick = React.useCallback(() => {
    // Directories: expand/collapse on click. Files: no-op by default —
    // attaching requires the explicit + button, deletion the trash button.
    // Consumers can still opt into file-click behavior via onFileClick.
    if (entry.isDirectory) {
      setExpanded((prev) => !prev)
      onDirectoryClick?.(entry)
    } else if (onFileClick) {
      onFileClick(entry)
    }
  }, [entry, onFileClick, onDirectoryClick])

  return (
    <div>
      <div
        className={cn(
          'group flex items-center gap-1 w-full px-2 py-0.5 text-sm hover:bg-accent/50 rounded-sm transition-colors',
          'text-foreground/80 hover:text-foreground',
        )}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        <button
          type="button"
          className="flex items-center gap-1 flex-1 min-w-0 text-left"
          onClick={handleClick}
        >
          {entry.isDirectory ? (
            <ChevronRight
              className={cn('size-3.5 shrink-0 transition-transform text-muted-foreground/60', expanded && 'rotate-90')}
            />
          ) : (
            <span className="size-3.5 shrink-0" />
          )}
          <FileTypeIcon name={entry.name} isDirectory={entry.isDirectory} isOpen={expanded} size={14} />
          <span className="truncate text-[13px]">{entry.name}</span>
        </button>
        {!entry.isDirectory && (onAddToChat || onDelete) && (
          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
            {onAddToChat && (
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); onAddToChat(entry) }}
                className="p-0.5 rounded text-muted-foreground hover:text-foreground hover:bg-foreground/[0.08]"
                title="添加到聊天"
              >
                <Plus className="size-3" />
              </button>
            )}
            {onDelete && (
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); onDelete(entry) }}
                className="p-0.5 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                title="删除文件"
              >
                <Trash2 className="size-3" />
              </button>
            )}
          </div>
        )}
      </div>
      {entry.isDirectory && expanded && entry.children && (
        <div>
          {entry.children
            .sort((a, b) => {
              if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
              return a.name.localeCompare(b.name)
            })
            .map((child) => (
              <FileTreeNode
                key={child.path}
                entry={child}
                depth={depth + 1}
                onFileClick={onFileClick}
                onDirectoryClick={onDirectoryClick}
                onAddToChat={onAddToChat}
                onDelete={onDelete}
              />
            ))}
        </div>
      )}
    </div>
  )
}

export function FileBrowser({
  rootPath,
  files: filesProp,
  onFileClick,
  onDirectoryClick,
  onAddToChat,
  onDelete,
  onRefresh,
  loading: loadingProp = false,
  hideToolbar = false,
  embedded = false,
  hideEmpty = false,
  className,
  version,
}: FileBrowserProps): React.ReactElement {
  // If parent supplies `files`, render those directly (legacy mode).
  // Otherwise auto-load the immediate children of `rootPath` from disk.
  const [autoFiles, setAutoFiles] = React.useState<FileEntry[]>([])
  const [autoLoading, setAutoLoading] = React.useState(false)
  const [reloadKey, setReloadKey] = React.useState(0)

  React.useEffect(() => {
    if (filesProp !== undefined) return
    if (!rootPath) {
      setAutoFiles([])
      return
    }
    let cancelled = false
    setAutoLoading(true)
    listDirectoryEntries(rootPath)
      .then((rows) => {
        if (cancelled) return
        const mapped: FileEntry[] = rows.map((r) => ({
          name: r.name,
          path: r.path,
          isDirectory: r.isDirectory,
          isFile: r.isFile,
          size: r.size,
          extension: r.extension,
        }))
        setAutoFiles(mapped)
      })
      .catch((err) => {
        console.error('[FileBrowser] listDirectoryEntries failed', err)
        if (!cancelled) setAutoFiles([])
      })
      .finally(() => { if (!cancelled) setAutoLoading(false) })
    return () => { cancelled = true }
  }, [rootPath, filesProp, reloadKey, version])

  const files = filesProp ?? autoFiles
  const loading = loadingProp || autoLoading
  const effectiveOnRefresh = onRefresh ?? (filesProp === undefined ? () => setReloadKey((k) => k + 1) : undefined)

  if (loading) {
    return (
      <div className={cn('flex items-center justify-center py-8 text-muted-foreground/60', className)}>
        <RefreshCw className="size-4 animate-spin mr-2" />
        <span className="text-xs">加载文件...</span>
      </div>
    )
  }

  if (files.length === 0 && hideEmpty) {
    return <></>
  }

  if (files.length === 0) {
    return (
      <div className={cn('flex flex-col items-center justify-center py-8 gap-2 text-muted-foreground/60', className)}>
        <FolderOpen className="size-6" />
        <span className="text-xs">暂无文件</span>
        {effectiveOnRefresh && (
          <button
            type="button"
            className="text-xs text-primary/60 hover:text-primary underline"
            onClick={effectiveOnRefresh}
          >
            刷新
          </button>
        )}
      </div>
    )
  }

  return (
    <div className={cn('py-1', className)}>
      {files
        .sort((a, b) => {
          if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
          return a.name.localeCompare(b.name)
        })
        .map((entry) => (
          <FileTreeNode
            key={entry.path}
            entry={entry}
            onFileClick={onFileClick}
            onDirectoryClick={onDirectoryClick}
            onAddToChat={onAddToChat}
            onDelete={onDelete}
          />
        ))}
    </div>
  )
}
