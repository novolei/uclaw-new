import * as React from 'react'
import { LoaderCircle, MoreHorizontal, FolderInput, Trash2, Pin, PinOff, Archive, ArchiveRestore } from 'lucide-react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { imChannelDisplay } from '@/lib/im-channel-display'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { dockOrderAtom, addDockPin } from '@/atoms/dock-atoms'

interface SessionItemProps {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  isActive: boolean
  /** Whether the agent loop is currently running for this session. */
  running?: boolean
  /** True when the session has a non-null pinned_at; drives menu label
   *  and (eventually) any visual pin indicator the rail wants to show. */
  isPinned?: boolean
  /** True when this session has at least one open tab. Renders a subtle
   *  left-edge marker (Mail.app-style) so users can see at a glance
   *  which rows in the rail are already loaded in the tab bar. */
  isOpen?: boolean
  /** IM channel this session originated from (wechat_ilink, wecom_bot, …).
   *  When present, the leading emoji is replaced with `[channelEmoji]`
   *  and a tooltip surfaces the channel name. */
  imChannelType?: string
  onClick: () => void
  onDelete?: () => void
  onMove?: () => void
  /** Toggle the session's pin state. Omitted = menu item hidden. */
  onTogglePin?: () => void
  /** Toggle the session's archive state. Omitted = menu item hidden. */
  onToggleArchive?: () => void
  /** True when session is currently archived. */
  isArchived?: boolean
}

function SessionItemImpl({
  id,
  title,
  titleEmoji,
  titlePending,
  isActive,
  running,
  isPinned,
  isOpen,
  imChannelType,
  onClick,
  onDelete,
  onMove,
  onTogglePin,
  onToggleArchive,
  isArchived,
}: SessionItemProps): React.ReactElement {
  const hasMenu = Boolean(onDelete || onMove || onTogglePin || onToggleArchive)
  const channel = imChannelDisplay(imChannelType)

  const dockOrder = useAtomValue(dockOrderAtom)
  const setDockOrder = useSetAtom(dockOrderAtom)

  const handlePinToDock = React.useCallback(() => {
    const next = addDockPin(dockOrder, {
      kind: 'pinned-conversation',
      sessionId: id,
      type: 'agent',
    })
    if (next === dockOrder) {
      toast.info('已经在 Dock 中')
      return
    }
    setDockOrder(next)
    toast.success('已固定到 Dock')
  }, [dockOrder, setDockOrder, id])

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          onClick={onClick}
          // content-visibility: auto lets Chromium skip layout/paint for rows
          // scrolled off-screen — gives near-virtualization for free, which
          // dramatically reduces the first-render cost when switching to a
          // workspace with many sessions. contain-intrinsic-size reserves
          // ~32 px so the scrollbar sizing stays stable.
          style={{ contentVisibility: 'auto', containIntrinsicSize: '0 32px' } as React.CSSProperties}
          className={cn(
            'group relative flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer',
            'text-[13px] transition-colors duration-100',
            isActive
              ? 'bg-sidebar-accent text-sidebar-primary font-medium'
              : 'text-muted-foreground hover:bg-muted hover:text-foreground'
          )}
        >
          {/* Open-tab indicator — 2px primary-tinted stripe on the left edge.
              Mail.app-style "this thread is open" affordance. Shows on every
              open row (including the active one — `isActive` says "currently
              focused tab", `isOpen` says "loaded as a tab somewhere", which
              are independent facts the user wants to see together). */}
          {isOpen && (
            <span
              aria-hidden
              className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-full bg-primary/60"
            />
          )}
          <span
            className="shrink-0 inline-flex items-center justify-center text-primary"
            style={{ width: '18px' }}
            title={channel ? `来自 ${channel.label}` : undefined}
          >
            {titlePending ? (
              <LoaderCircle size={14} strokeWidth={2} className="animate-spin" />
            ) : channel?.logoSrc ? (
              <img
                src={channel.logoSrc}
                alt={`来自 ${channel.label}`}
                className="w-[14px] h-[14px] object-contain rounded-sm"
                draggable={false}
              />
            ) : channel ? (
              <span
                className="text-[14px] leading-none"
                style={{ fontFamily: "'Noto Emoji', sans-serif" }}
                aria-label={`来自 ${channel.label}`}
              >
                {channel.emoji}
              </span>
            ) : (
              <span className="text-[14px] leading-none" style={{ fontFamily: "'Noto Emoji', sans-serif" }}>
                {titleEmoji || '💬'}
              </span>
            )}
          </span>
          {titlePending ? (
            <span className="flex-1 h-3.5 rounded bg-muted-foreground/20 animate-pulse" />
          ) : (
            <span className="flex-1 truncate">{title || 'New session'}</span>
          )}
          {/* Always-visible running indicator — pulsing primary dot when this
              session has an active agent loop. Lets the user spot in-flight
              tasks across sessions without switching tabs. */}
          {running && !titlePending && (
            <span
              className="shrink-0 size-1.5 rounded-full bg-primary animate-pulse shadow-[0_0_6px_hsl(var(--primary))] group-hover:opacity-0 transition-opacity"
              title="任务执行中"
            />
          )}
          {hasMenu && (
            // Wrap the trigger in a click-eater so click + focus + pointer
            // events on the 3-dot stay scoped here and never reach the
            // parent div's onClick (which would open the session as a side
            // effect of choosing "delete" / "move" from the menu).
            //
            // Visible only when the row is hovered / has focus inside it /
            // the dropdown is open — keeps the rail visually quiet.
            // `[&:has([data-state=open])]:opacity-100` ensures the button
            // stays visible while the menu is open even if the cursor
            // leaves the row (Radix portals the menu items elsewhere).
            <div
              onClick={(e) => e.stopPropagation()}
              onPointerDown={(e) => e.stopPropagation()}
              onMouseDown={(e) => e.stopPropagation()}
              className={cn(
                'shrink-0 transition-opacity duration-150',
                'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100',
                '[&:has([data-state=open])]:opacity-100',
              )}
            >
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    className="shrink-0 opacity-60 hover:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded hover:bg-foreground/[0.08]"
                    title="更多"
                  >
                    <MoreHorizontal className="size-3.5" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" side="bottom" sideOffset={4} className="w-40 min-w-0 p-0.5 z-[100]">
                  {onTogglePin && (
                    <DropdownMenuItem
                      className="text-xs py-1 [&>svg]:size-3.5"
                      onSelect={() => { onTogglePin() }}
                    >
                      {isPinned ? <PinOff /> : <Pin />}
                      {isPinned ? '取消固定' : '固定'}
                    </DropdownMenuItem>
                  )}
                  {onToggleArchive && (
                    <DropdownMenuItem
                      className="text-xs py-1 [&>svg]:size-3.5"
                      onSelect={() => { onToggleArchive() }}
                    >
                      {isArchived ? <ArchiveRestore /> : <Archive />}
                      {isArchived ? '取消归档' : '归档'}
                    </DropdownMenuItem>
                  )}
                  {onMove && (
                    <DropdownMenuItem
                      className="text-xs py-1 [&>svg]:size-3.5"
                      onSelect={() => { onMove() }}
                    >
                      <FolderInput />
                      移动到...
                    </DropdownMenuItem>
                  )}
                  {onDelete && (
                    <DropdownMenuItem
                      className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
                      onSelect={() => { onDelete() }}
                    >
                      <Trash2 />
                      删除
                    </DropdownMenuItem>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={handlePinToDock}>
          <Pin size={14} className="mr-2" />
          固定到 Dock
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

// Memo wraps the impl. Inline lambdas from WorkspaceRail (onClick / onDelete /
// onMove / onTogglePin) defeat memo on their own, but for a workspace switch
// the IDs (title, isActive, isOpen) change in lockstep — memo still saves
// re-rendering rows whose underlying data is unchanged when only one row
// changes (e.g., a session's running indicator flicks).
export const SessionItem = React.memo(SessionItemImpl)
