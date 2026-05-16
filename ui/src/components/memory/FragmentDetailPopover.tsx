import React from 'react'
import { cn, safeParseDate } from '@/lib/utils'
import * as Dialog from '@radix-ui/react-dialog'
import { X, Copy, Clock } from 'lucide-react'
import { SUBTYPE_COLORS } from './FragmentCard'
import type { FragmentItem } from '@/lib/tauri-bridge'

const SUBTYPE_LABELS: Record<string, string> = {
  daily: '日常', credential: '凭证', location: '位置',
  reminder: '提醒', inspiration: '灵感', bookmark: '书签',
}

interface FragmentDetailPopoverProps {
  fragment: FragmentItem | null
  open: boolean
  onClose: () => void
}

export function FragmentDetailPopover({ fragment, open, onClose }: FragmentDetailPopoverProps) {
  if (!fragment) return null
  
  const tag = fragment.tags?.[0] || 'daily'
  const colors = SUBTYPE_COLORS[tag] || SUBTYPE_COLORS.daily

  const copyContent = () => {
    navigator.clipboard.writeText(fragment.content)
  }

  const timeStr = (() => {
    const date = safeParseDate(fragment.createdAt)
    return date ? date.toLocaleString('zh-CN', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit',
    }) : '—'
  })()

  const sourceLabel = fragment.source === 'voice' ? '语音' : fragment.source === 'clipboard' ? '剪贴板' : '文本'

  return (
    <Dialog.Root open={open} onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/40 z-50 animate-in fade-in" />
        <Dialog.Content className={cn(
          'fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-50',
          'w-[90vw] max-w-[420px] rounded-xl border bg-popover shadow-xl',
          'animate-in fade-in zoom-in-95 p-5 space-y-4',
          colors.border,
        )}>
          {/* 头部 */}
          <div className="flex items-start justify-between">
            <div className="space-y-1">
              {fragment.title && (
                <h3 className="text-[16px] font-semibold">{fragment.title}</h3>
              )}
              <div className="flex items-center gap-2">
                <span className={cn(
                  'inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium',
                  colors.bg, colors.text,
                )}>
                  {SUBTYPE_LABELS[tag] || tag}
                </span>
                <span className="text-[11px] text-muted-foreground">{sourceLabel}</span>
                <span className="text-[11px] text-muted-foreground">{timeStr}</span>
              </div>
            </div>
            <Dialog.Close asChild>
              <button type="button" className="p-1 rounded-md hover:bg-muted">
                <X className="size-4" />
              </button>
            </Dialog.Close>
          </div>

          {/* 内容 */}
          <div className="text-[14px] leading-relaxed text-foreground/90 whitespace-pre-wrap max-h-[300px] overflow-y-auto">
            {fragment.content}
          </div>

          {/* 复习状态 */}
          {fragment.reviewStatus && (
            <div className="flex items-center gap-2 text-[12px] text-muted-foreground border-t pt-3">
              <Clock className="size-3.5" />
              {fragment.reviewStatus.completed 
                ? '已完成全部复习'
                : `复习进度: ${fragment.reviewStatus.reviewCount}/4`
              }
            </div>
          )}

          {/* 操作按钮 */}
          <div className="flex gap-2 pt-1">
            <button
              type="button"
              onClick={copyContent}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] bg-muted hover:bg-muted/80 transition-colors"
            >
              <Copy className="size-3" />
              复制内容
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
