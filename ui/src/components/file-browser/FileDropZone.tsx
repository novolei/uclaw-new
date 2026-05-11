/**
 * FileDropZone — 文件拖拽区域
 *
 * 支持拖拽文件/目录到指定区域。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import { Upload } from 'lucide-react'
import { cn } from '@/lib/utils'

interface FileDropZoneProps {
  /** 拖拽完成回调 */
  onDrop?: (files: File[]) => void
  /** 是否禁用 */
  disabled?: boolean
  /** 自定义类名 */
  className?: string
  /** 子元素 */
  children?: React.ReactNode
  /** 提示文字 */
  hint?: string
}

export function FileDropZone({
  onDrop,
  disabled = false,
  className,
  children,
  hint = '拖拽文件到此处',
}: FileDropZoneProps): React.ReactElement {
  const [isDragging, setIsDragging] = React.useState(false)

  const handleDragOver = React.useCallback(
    (e: React.DragEvent) => {
      if (disabled) return
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(true)
    },
    [disabled],
  )

  const handleDragLeave = React.useCallback(
    (e: React.DragEvent) => {
      if (disabled) return
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(false)
    },
    [disabled],
  )

  const handleDrop = React.useCallback(
    (e: React.DragEvent) => {
      if (disabled) return
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(false)

      if (e.dataTransfer?.files?.length) {
        const files = Array.from(e.dataTransfer.files)
        onDrop?.(files)
      }

      // [PLACEHOLDER] Tauri native file drop with paths
      // Tauri 2.x 使用 onDragDrop 事件获取路径
    },
    [disabled, onDrop],
  )

  return (
    <div
      className={cn(
        'relative transition-colors',
        isDragging && !disabled && 'ring-2 ring-primary/40 bg-primary/5 rounded-lg',
        className,
      )}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {children}
      {isDragging && !disabled && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-primary/5 backdrop-blur-[1px] rounded-lg z-10 pointer-events-none">
          <Upload className="size-6 text-primary/60 mb-1" />
          <span className="text-xs text-primary/80 font-medium">{hint}</span>
        </div>
      )}
    </div>
  )
}
