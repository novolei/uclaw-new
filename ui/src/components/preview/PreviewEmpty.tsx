import * as React from 'react'
import { FileText, AlertTriangle, Loader2 } from 'lucide-react'

export interface PreviewEmptyProps {
  status: 'idle' | 'loading' | 'error'
  message?: string
}

export function PreviewEmpty({ status, message }: PreviewEmptyProps): React.ReactElement {
  if (status === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-6 text-muted-foreground/60 animate-spin mb-3 motion-reduce:animate-none" aria-hidden />
        <div className="text-[12px] text-muted-foreground">正在读取文件…</div>
      </div>
    )
  }
  if (status === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-6 text-destructive mb-3" aria-hidden />
        <div className="text-[12px] text-destructive">读取失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[280px] break-words">
          {message ?? '未知错误'}
        </div>
      </div>
    )
  }
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <FileText className="size-10 text-muted-foreground/40 mb-3" aria-hidden />
      <div className="text-[12px] text-muted-foreground">还没选中文件</div>
      <div className="mt-1 text-[11px] text-muted-foreground/60 max-w-[260px]">
        在左侧文件树点击任意文件开始预览
      </div>
    </div>
  )
}
