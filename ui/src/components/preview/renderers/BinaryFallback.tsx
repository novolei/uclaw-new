/**
 * BinaryFallback — Shown when the file extension isn't recognized.
 */

import * as React from 'react'
import { FileQuestion } from 'lucide-react'

interface BinaryFallbackProps {
  name: string
  /** Size in bytes for display formatting. */
  size: number
  ext: string
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
  return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

export function BinaryFallback({ name, size, ext }: BinaryFallbackProps): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <FileQuestion className="size-12 text-muted-foreground/60 mb-3" aria-hidden />
      <div className="text-[13px] text-foreground/80 font-mono mb-1">{name}</div>
      <div className="text-[11px] text-muted-foreground">
        {ext ? `.${ext} · ` : ''}{formatBytes(size)} · 暂不支持预览
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground/60 max-w-[280px]">
        点击右上角按钮在 Finder 中打开，或拖入聊天框作为附件发送
      </div>
    </div>
  )
}
