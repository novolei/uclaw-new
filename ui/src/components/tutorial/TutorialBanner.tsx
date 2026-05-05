/**
 * TutorialBanner — 教程提示横幅
 *
 * 在对话视图顶部显示新手提示/教程入口。
 * 用户可以关闭并记住选择。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import { X, GraduationCap, ExternalLink } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const STORAGE_KEY = 'uclaw-tutorial-dismissed'

interface TutorialBannerProps {
  className?: string
}

export function TutorialBanner({ className }: TutorialBannerProps): React.ReactElement | null {
  const [dismissed, setDismissed] = React.useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) === 'true'
    } catch {
      return false
    }
  })

  const handleDismiss = React.useCallback(() => {
    setDismissed(true)
    try {
      localStorage.setItem(STORAGE_KEY, 'true')
    } catch {
      // ignore
    }
  }, [])

  if (dismissed) return null

  return (
    <div
      className={cn(
        'flex items-center gap-3 px-4 py-2 bg-primary/5 border-b border-primary/10',
        className,
      )}
    >
      <GraduationCap className="size-4 text-primary/60 shrink-0" />
      <p className="text-xs text-foreground/70 flex-1">
        <strong className="font-medium">新手指南：</strong>
        使用 <kbd className="px-1 py-0.5 rounded bg-muted/80 text-[10px] font-mono mx-0.5">Cmd+N</kbd> 创建新对话，
        <kbd className="px-1 py-0.5 rounded bg-muted/80 text-[10px] font-mono mx-0.5">Cmd+B</kbd> 切换侧边栏。
      </p>
      <Button
        variant="ghost"
        size="icon"
        className="size-5 shrink-0 text-muted-foreground/50 hover:text-foreground"
        onClick={handleDismiss}
      >
        <X className="size-3" />
      </Button>
    </div>
  )
}
