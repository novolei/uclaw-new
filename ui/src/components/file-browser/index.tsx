// [PLACEHOLDER] file-browser — 文件浏览器组件占位
import * as React from 'react'

export interface FileBrowserProps {
  rootPath?: string
  onSelect?: (path: string) => void
  onAddToChat?: (entry: any) => void | Promise<void>
  hideToolbar?: boolean
  embedded?: boolean
  hideEmpty?: boolean
  className?: string
  [key: string]: unknown
}

export function FileBrowser(_props: FileBrowserProps): React.ReactElement {
  return <div className="p-4 text-muted-foreground text-sm">FileBrowser placeholder</div>
}

export interface FileDropZoneProps {
  onDrop?: (files: File[]) => void
  children?: React.ReactNode
  className?: string
  workspaceSlug?: string
  sessionId?: string
  target?: string
  onFilesUploaded?: () => void
  onAttachFolder?: () => void | Promise<void>
  onFoldersDropped?: (folderPaths: string[]) => void | Promise<void>
  [key: string]: unknown
}

export function FileDropZone({ children }: FileDropZoneProps): React.ReactElement {
  return <>{children}</>
}

export interface FileTypeIconProps {
  filename?: string
  isDirectory?: boolean
  className?: string
}

export function FileTypeIcon(_props: FileTypeIconProps): React.ReactElement {
  return <span />
}
