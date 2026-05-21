/**
 * QueuedMessagesBanner — Codex-style banner above the agent composer
 * showing messages the user typed while the agent was already
 * streaming. Each queued message has three actions:
 *
 *   - 引导 (steer)  — send now + interrupt current turn
 *   - 编辑 (edit)   — pop back into the composer for editing
 *   - 删除 (trash)  — discard
 *
 * If the user takes no action, the agent's current turn completes
 * naturally and AgentView auto-dispatches the oldest queued message
 * (FIFO) — see the streaming-transition effect in AgentView.tsx.
 *
 * Visual design follows the screenshot the user shared: stacked
 * rounded cards above the composer, each with a small "queue indent"
 * icon, the message text, and a trailing actions group.
 */

import * as React from 'react'
import { CornerDownLeft, Trash2, Pencil, MoreHorizontal } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { QueuedAgentMessage } from '@/atoms/agent-queue-messages'

export interface QueuedMessagesBannerProps {
  messages: QueuedAgentMessage[]
  /** Steer = send-now-with-interrupt. Fires the message into the running agent loop. */
  onSteer: (msg: QueuedAgentMessage) => void
  /** Edit = pop from queue, restore to composer for further editing. */
  onEdit: (msg: QueuedAgentMessage) => void
  /** Delete = discard from queue. */
  onDelete: (msg: QueuedAgentMessage) => void
}

export function QueuedMessagesBanner({
  messages,
  onSteer,
  onEdit,
  onDelete,
}: QueuedMessagesBannerProps): React.ReactElement | null {
  if (messages.length === 0) return null

  return (
    <div className="flex flex-col gap-1.5 px-3 pb-1.5">
      {messages.map((msg) => (
        <QueuedMessageRow
          key={msg.id}
          msg={msg}
          onSteer={() => onSteer(msg)}
          onEdit={() => onEdit(msg)}
          onDelete={() => onDelete(msg)}
        />
      ))}
    </div>
  )
}

interface QueuedMessageRowProps {
  msg: QueuedAgentMessage
  onSteer: () => void
  onEdit: () => void
  onDelete: () => void
}

function QueuedMessageRow({
  msg,
  onSteer,
  onEdit,
  onDelete,
}: QueuedMessageRowProps): React.ReactElement {
  const [menuOpen, setMenuOpen] = React.useState(false)

  return (
    <div
      className={cn(
        'flex items-center gap-2 rounded-2xl border border-border/40 bg-muted/40',
        'px-3 py-1.5 text-sm transition-colors hover:bg-muted/60',
      )}
    >
      {/* Queue-indent indicator on the left */}
      <CornerDownLeft
        className="size-4 shrink-0 -scale-y-100 text-foreground/35"
        aria-hidden
      />

      {/* Message preview — single line, truncate */}
      <span className="flex-1 truncate text-foreground/85" title={msg.text}>
        {msg.text}
      </span>

      {/* Actions — 引导 / 删除 / more */}
      <div className="flex shrink-0 items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              onClick={onSteer}
              className={cn(
                'inline-flex items-center gap-1 rounded-md px-2 py-0.5',
                'text-xs font-medium text-foreground/70',
                'hover:bg-accent hover:text-foreground transition-colors',
              )}
            >
              <CornerDownLeft className="size-3" />
              引导
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">
            <p>立即把这条消息引导给运行中的 Agent（会打断当前 turn）</p>
          </TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              onClick={onDelete}
              className={cn(
                'inline-flex items-center justify-center rounded-md p-1',
                'text-foreground/55 hover:text-destructive hover:bg-destructive/10',
                'transition-colors',
              )}
              aria-label="删除排队消息"
            >
              <Trash2 className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">
            <p>丢弃这条排队消息</p>
          </TooltipContent>
        </Tooltip>

        {/* More menu — only houses 编辑 for now; future: 排序 / 标记 etc */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setMenuOpen((v) => !v)}
            onBlur={(e) => {
              // Close when focus moves outside this menu container.
              const next = e.relatedTarget as HTMLElement | null
              if (!next || !e.currentTarget.parentElement?.contains(next)) {
                // Small delay so the click handler on the menu item still
                // fires before the menu unmounts.
                setTimeout(() => setMenuOpen(false), 100)
              }
            }}
            className={cn(
              'inline-flex items-center justify-center rounded-md p-1',
              'text-foreground/55 hover:text-foreground hover:bg-accent',
              'transition-colors',
            )}
            aria-label="更多选项"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
          >
            <MoreHorizontal className="size-3.5" />
          </button>
          {menuOpen && (
            <div
              role="menu"
              className={cn(
                'absolute right-0 top-full mt-1 z-10',
                'min-w-[140px] rounded-md border border-border/60 bg-popover shadow-md',
                'py-1',
              )}
            >
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false)
                  onEdit()
                }}
                className={cn(
                  'flex w-full items-center gap-2 px-2.5 py-1.5',
                  'text-xs text-foreground/85 hover:bg-accent',
                  'transition-colors',
                )}
              >
                <Pencil className="size-3.5" />
                编辑后重发
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
