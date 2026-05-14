/**
 * WorkspaceSwitcherBar — ARC-style horizontal bar at the bottom of the
 * left sidebar.
 *
 * Layout: [workspace icons or dots] | [+]
 *
 * Icons are lucide-react glyphs mapped from the workspace's stored
 * emoji (workspace.icon). The compact bar prefers glyphs for visual
 * density; the human-readable emoji is preserved in the
 * WorkspaceHeader (top of sidebar) and in the hover tooltip.
 *
 * ≤5 workspaces → all show as full 24px icon buttons.
 * >5 workspaces → only the active one renders as full icon; others
 *   collapse to 6px dots (hover tooltip remains the only way to
 *   identify them visually).
 *
 * Each icon/dot supports:
 * - Hover tooltip (ARC-style pill: name + ⌘ + digit chips)
 * - Click → selectWorkspaceAtom
 * - Drag-reorder (horizontal, via Phase 2/3 reorderWorkspacesAtom)
 * - Running indicator (pulse dot when sessions in this workspace are
 *   executing)
 *
 * Note: The automation entry that lived in this bar's zone 1 in earlier
 * iterations was hoisted to its own row above the bar in LeftSidebar.tsx —
 * it's per-workspace context, not cross-workspace navigation.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Plus, type LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import {
  Tooltip, TooltipContent, TooltipProvider, TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  selectWorkspaceAtom,
  reorderWorkspacesAtom,
  refreshWorkspacesAtom,
  type WorkspaceInfo,
} from '@/atoms/workspace'
import {
  agentSessionsAtom,
  agentSessionIndicatorMapAtom,
} from '@/atoms/agent-atoms'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { KaleidoscopeIcon } from '@/views/Kaleidoscope/KaleidoscopeIcon'

const isMac = typeof navigator !== 'undefined'
  && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)
const modGlyph = isMac ? '⌘' : 'Ctrl'

/**
 * Per-icon width budget for capacity computation. `size-7` icon button
 * (28px) + `gap-1` (4px) = 32px per slot. The first icon doesn't pay
 * for the leading gap, so we subtract one gap from the budget once.
 */
const ICON_SLOT_WIDTH = 32
const ICON_LEADING_GAP = 4

/**
 * Measure the workspace-icons container width and return whether all
 * workspaces fit at full icon size. Uses ResizeObserver so the mode
 * flips smoothly when the user widens / narrows the sidebar.
 *
 * Returns `null` until the first measurement lands — callers default
 * to comfortable mode on first paint to avoid a dots-then-icons flash
 * on initial render.
 */
function useFitsComfortably(
  ref: React.RefObject<HTMLDivElement>,
  count: number,
): boolean | null {
  const [fits, setFits] = React.useState<boolean | null>(null)

  React.useEffect(() => {
    const node = ref.current
    if (!node) return
    const measure = (): void => {
      const budget = node.clientWidth + ICON_LEADING_GAP // first icon has no leading gap
      const capacity = Math.max(0, Math.floor(budget / ICON_SLOT_WIDTH))
      setFits(capacity >= count)
    }
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(node)
    return () => ro.disconnect()
  }, [ref, count])

  return fits
}

/**
 * Resolve a workspace's stored icon value to a lucide component. Handles
 * both new-style icon names ('Folder', 'Star', ...) and legacy emoji
 * values from before Phase 4b's icon-picker switch. Defined in the
 * shared catalog so the WorkspaceHeader + IconPicker stay aligned.
 */
const iconForWorkspace = (icon: string): LucideIcon => getWorkspaceIcon(icon)

/** Tooltip body — rendered inside the theme's standard TooltipContent
 *  surface (bg-tooltip/90, tooltip-foreground text). Icon glyph + name
 *  on the left, ⌘ + digit chips on the right (first 9). Uses tooltip-*
 *  tokens so every theme's tooltip surface stays consistent. */
function WorkspaceTooltip({
  workspace, indexForShortcut,
}: { workspace: WorkspaceInfo; indexForShortcut: number | null }): React.ReactElement {
  const Icon = getWorkspaceIcon(workspace.icon)
  return (
    <div className="flex items-center gap-1.5 font-medium">
      <Icon className="size-3.5 shrink-0 text-tooltip-muted" aria-hidden />
      <span>{workspace.name}</span>
      {indexForShortcut !== null && indexForShortcut < 9 && (
        <span className="ml-1 flex items-center gap-0.5">
          <kbd className="px-1.5 py-0.5 rounded bg-tooltip-foreground/10
                          text-tooltip-foreground text-[10px] font-mono leading-none">
            {modGlyph}
          </kbd>
          <kbd className="px-1.5 py-0.5 rounded bg-tooltip-foreground/10
                          text-tooltip-foreground text-[10px] font-mono leading-none">
            {indexForShortcut + 1}
          </kbd>
        </span>
      )}
    </div>
  )
}

interface WorkspaceItemProps {
  workspace: WorkspaceInfo
  index: number
  active: boolean
  running: boolean
  /** Roving tabindex — 0 on the currently-focused item, -1 elsewhere.
   *  Lets the user Tab into the bar then Arrow between icons.  */
  tabIndex: number
  /** Callback ref so the parent can imperatively focus an icon when the
   *  focused index changes via keyboard navigation. */
  buttonRef: (el: HTMLButtonElement | null) => void
  onSelect: (id: string) => void
  /** Pointer-event drag-reorder (replaced HTML5 drag — see useDragReorder). */
  onPointerDown: (id: string, index: number, e: React.PointerEvent) => void
  /** Visual state from the parent's drag controller. */
  dragOffsetX: number      // 0 unless this is the dragged item
  shiftX: number           // signed px shift for items between source and target
  isDragging: boolean      // true on the dragged item
}

function WorkspaceIcon({
  workspace, index, active, running, tabIndex, buttonRef, onSelect,
  onPointerDown, dragOffsetX, shiftX, isDragging,
}: WorkspaceItemProps): React.ReactElement {
  // Transform: the dragged icon follows the cursor instantly (no transition);
  // every other icon glides to its shifted position via CSS transition. The
  // combination produces the iOS-style "icons part to make room" animation.
  const transform = isDragging
    ? `translate3d(${dragOffsetX}px, 0, 0)`
    : shiftX !== 0
      ? `translate3d(${shiftX}px, 0, 0)`
      : undefined
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          ref={buttonRef}
          type="button"
          tabIndex={tabIndex}
          onPointerDown={(e) => onPointerDown(workspace.id, index, e)}
          onClick={() => void onSelect(workspace.id)}
          aria-label={`工作区: ${workspace.name}`}
          aria-current={active ? 'true' : undefined}
          style={{
            transform,
            // Dragged item: no transition so it tracks the cursor 1:1.
            // Others: ease-out glide as they shift into the dragged item's
            // vacated slot.
            transition: isDragging ? 'none' : 'transform 220ms cubic-bezier(0.2, 0.8, 0.2, 1)',
            zIndex: isDragging ? 10 : undefined,
            // Override any inherited user-select:none so click + pointer
            // events stay clean.
            userSelect: 'auto',
            WebkitUserSelect: 'auto',
            touchAction: 'none',
          } as React.CSSProperties}
          className={cn(
            'titlebar-no-drag relative inline-flex items-center justify-center',
            'size-7 rounded-md transition-colors',
            // Default button cursor on hover (pointer); only switch to
            // grabbing once an actual drag is in flight. Earlier versions
            // showed cursor-grab on every hover, which signaled "drag me"
            // too aggressively for what's primarily a click target.
            isDragging && 'cursor-grabbing scale-[1.08] shadow-md',
            // ARC-style active state: soft filled background tint + tinted icon.
            active
              ? 'bg-primary/15 text-primary'
              : 'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.05]',
          )}
        >
          {React.createElement(iconForWorkspace(workspace.icon), {
            className: 'size-4',
            'aria-hidden': true,
          } as React.ComponentProps<LucideIcon>)}
          {running && (
            <span
              className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full
                         bg-primary animate-pulse ring-1 ring-background
                         shadow-[0_0_4px_hsl(var(--primary))]"
              aria-label="该工作区有任务执行中"
            />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={6}>
        <WorkspaceTooltip workspace={workspace} indexForShortcut={index} />
      </TooltipContent>
    </Tooltip>
  )
}

function WorkspaceDot({
  workspace, index, running, tabIndex, buttonRef, onSelect,
  onPointerDown, dragOffsetX, shiftX, isDragging,
}: WorkspaceItemProps): React.ReactElement {
  const Icon = iconForWorkspace(workspace.icon)
  const transform = isDragging
    ? `translate3d(${dragOffsetX}px, 0, 0)`
    : shiftX !== 0
      ? `translate3d(${shiftX}px, 0, 0)`
      : undefined
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          ref={buttonRef}
          type="button"
          tabIndex={tabIndex}
          onPointerDown={(e) => onPointerDown(workspace.id, index, e)}
          onClick={() => void onSelect(workspace.id)}
          aria-label={`工作区: ${workspace.name} (workspace dot)`}
          style={{
            transform,
            transition: isDragging ? 'none' : 'transform 220ms cubic-bezier(0.2, 0.8, 0.2, 1)',
            zIndex: isDragging ? 10 : undefined,
            userSelect: 'auto',
            WebkitUserSelect: 'auto',
            touchAction: 'none',
          } as React.CSSProperties}
          className={cn(
            'group titlebar-no-drag relative inline-flex items-center justify-center',
            // Larger hit target (12px) than visible glyph (6px) for easier
            // clicking — the visible circle is rendered via the inner span.
            'size-3 rounded-full',
            // Default hover cursor; grabbing only while a drag is in flight.
            isDragging && 'cursor-grabbing scale-[1.4]',
          )}
        >
          {/* Default dot — shrinks + fades out when this button is hovered,
              so the morph into the icon feels continuous instead of a
              hard opacity swap. */}
          <span
            aria-hidden
            className="size-1.5 rounded-full bg-foreground/40
                       transition-all duration-200 ease-out
                       group-hover:scale-50 group-hover:opacity-0"
          />
          {/* Icon overlay — grows + fades in on hover. Combined with the
              dot's shrink-and-fade above, the two transitions cross-fade
              into a single morph. Sized smaller than the full WorkspaceIcon
              (size-5 / 20px) so dots at the bar's left/right edges don't
              clip when their overlay overflows the 12px button bounds.
              Color tone — text-foreground/60 — matches the muted weight
              of the unhovered dot (bg-foreground/40), so the morph reads
              as a continuous change in shape rather than a jump from
              quiet to loud. */}
          <span
            aria-hidden
            className="pointer-events-none absolute left-1/2 top-1/2
                       inline-flex items-center justify-center size-5 rounded-md
                       text-foreground/60
                       -translate-x-1/2 -translate-y-1/2 scale-50 opacity-0
                       transition-all duration-200 ease-out
                       group-hover:scale-100 group-hover:opacity-100"
          >
            <Icon className="size-3" />
          </span>
          {running && (
            <span
              className="absolute -top-px -right-px size-1 rounded-full
                         bg-primary animate-pulse"
              aria-label="该工作区有任务执行中"
            />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={6}>
        <WorkspaceTooltip workspace={workspace} indexForShortcut={index} />
      </TooltipContent>
    </Tooltip>
  )
}

export function WorkspaceSwitcherBar(): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const reorderWorkspaces = useSetAtom(reorderWorkspacesAtom)
  const refresh = useSetAtom(refreshWorkspacesAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const topLevelView = useAtomValue(topLevelViewAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)

  const [createOpen, setCreateOpen] = React.useState(false)

  // ===== Pointer-based drag-reorder with iOS-style "icons part to make room"
  //
  // We don't use HTML5 drag-and-drop because WebKit / WKWebView refuses to
  // start a drag on a descendant of any user-select:none ancestor — a
  // documented quirk that broke after PR #89 added user-select:none to
  // .titlebar-drag-region. The pointer-events approach also gives us the
  // iOS-style sliding animation, which HTML5 drag-and-drop can't.
  //
  // Algorithm:
  // - On pointerdown, record the source index + starting cursor X. Don't
  //   set drag state yet (small mouse movements during click shouldn't
  //   trigger a drag).
  // - On pointermove, once movement exceeds DRAG_THRESHOLD, enter drag
  //   state. Compute targetIndex from cursor delta vs. measured item
  //   stride (button width + flex gap).
  // - On pointerup, commit the reorder if source !== target, and
  //   suppress the upcoming click via capture-phase click listener.
  //
  // Render: the dragged icon translates instantly with the cursor (no
  // transition); every other icon glides to its shifted position via
  // CSS transition. The combination is the iOS Springboard feel.
  interface DragState {
    id: string
    sourceIdx: number
    targetIdx: number
    deltaX: number
    stride: number   // button center-to-center distance, measured at drag-start
  }
  const [drag, setDrag] = React.useState<DragState | null>(null)
  const DRAG_THRESHOLD = 5  // px before mousedown becomes a drag

  const startDrag = React.useCallback((id: string, idx: number, e: React.PointerEvent) => {
    if (e.button !== 0) return  // primary mouse only

    const startX = e.clientX
    const target = e.currentTarget as HTMLElement
    const sibling = (target.nextElementSibling ?? target.previousElementSibling) as HTMLElement | null
    const stride = sibling
      ? Math.abs(sibling.getBoundingClientRect().left - target.getBoundingClientRect().left)
      : target.getBoundingClientRect().width + 4

    let dragStarted = false

    const onMove = (me: PointerEvent): void => {
      const dx = me.clientX - startX
      if (!dragStarted) {
        if (Math.abs(dx) < DRAG_THRESHOLD) return
        dragStarted = true
      }
      const newTargetIdx = Math.max(
        0,
        Math.min(workspaces.length - 1, idx + Math.round(dx / stride)),
      )
      setDrag({ id, sourceIdx: idx, targetIdx: newTargetIdx, deltaX: dx, stride })
    }

    const onUp = async (): Promise<void> => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      if (!dragStarted) return

      // Suppress the click event that follows pointerup — we DON'T want
      // the workspace to also activate after the user drags it.
      const blockClick = (ev: Event): void => {
        ev.preventDefault()
        ev.stopPropagation()
      }
      window.addEventListener('click', blockClick, { once: true, capture: true })

      // Commit reorder using the latest state.
      setDrag((curr) => {
        if (curr && curr.sourceIdx !== curr.targetIdx) {
          const reordered = [...workspaces]
          const [moved] = reordered.splice(curr.sourceIdx, 1)
          reordered.splice(curr.targetIdx, 0, moved!)
          void reorderWorkspaces(reordered.map((w) => w.id)).catch((err) =>
            console.error('[workspace-switcher] reorder failed', err),
          )
        }
        return null
      })
    }

    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }, [workspaces, reorderWorkspaces])

  // Roving tabindex — see https://w3c.github.io/aria-practices/#kbd_roving_tabindex
  // `focusedIndex` is the index of the workspace currently in the tab order.
  // Default = active workspace (so Tab into the bar lands on the user's
  // current context). Arrow keys move it; pressing Enter activates the
  // focused icon.
  const buttonRefs = React.useRef<Array<HTMLButtonElement | null>>([])
  const [focusedIndex, setFocusedIndex] = React.useState(0)
  // Distinguishes keyboard-driven focus changes (where we want to imperatively
  // refocus the new element) from index updates caused by active workspace
  // switching via mouse (don't steal focus from wherever the user is).
  const shouldFocusRef = React.useRef(false)

  // Keep focusedIndex pointing at the active workspace by default. Skipped
  // when the user is keyboard-navigating (shouldFocusRef would have just
  // set focusedIndex independently).
  React.useEffect(() => {
    const activeIdx = workspaces.findIndex((w) => w.id === activeId)
    if (activeIdx !== -1) setFocusedIndex(activeIdx)
  }, [activeId, workspaces])

  // After focusedIndex changes from keyboard nav, move actual DOM focus
  // to the now-tabbable button. The ref toggle prevents stealing focus
  // when activeId changes via mouse click.
  React.useLayoutEffect(() => {
    if (!shouldFocusRef.current) return
    shouldFocusRef.current = false
    buttonRefs.current[focusedIndex]?.focus()
  }, [focusedIndex])

  /** Set of workspace ids that have at least one running session. */
  const runningWorkspaceIds = React.useMemo(() => {
    const set = new Set<string>()
    for (const s of agentSessions) {
      if (indicatorMap.get(s.id) === 'running' && s.workspaceId) {
        set.add(s.workspaceId)
      }
    }
    return set
  }, [agentSessions, indicatorMap])

  const handleSelect = React.useCallback((id: string) => {
    void selectWorkspace(id)
  }, [selectWorkspace])

  // Arrow / Home / End move the roving focus; Enter or Space activates
  // the focused workspace. Listening on the toolbar container so each
  // icon button stays simple. Tooltip clicks won't reach here (they're
  // portaled out of the DOM tree).
  const handleToolbarKeyDown = React.useCallback((e: React.KeyboardEvent): void => {
    if (workspaces.length === 0) return
    let next: number | null = null
    if (e.key === 'ArrowRight') {
      next = (focusedIndex + 1) % workspaces.length
    } else if (e.key === 'ArrowLeft') {
      next = (focusedIndex - 1 + workspaces.length) % workspaces.length
    } else if (e.key === 'Home') {
      next = 0
    } else if (e.key === 'End') {
      next = workspaces.length - 1
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      const w = workspaces[focusedIndex]
      if (w) handleSelect(w.id)
      return
    }
    if (next !== null) {
      e.preventDefault()
      shouldFocusRef.current = true
      setFocusedIndex(next)
    }
  }, [focusedIndex, workspaces, handleSelect])

  // Measure the icons container to pick comfortable vs compact mode.
  // `null` before first measurement → default to comfortable so the
  // initial paint isn't dots-then-icons.
  const iconsContainerRef = React.useRef<HTMLDivElement>(null)
  const fitsComfortably = useFitsComfortably(iconsContainerRef, workspaces.length)
  const collapsed = fitsComfortably === false

  return (
    <>
      <TooltipProvider delayDuration={0}>
        <div className="flex items-center gap-1.5 px-3 py-2 border-t border-border/40">
          {/* Kaleidoscope 入口 —— 它不是一个 workspace，所以跟 workspace
              dots 之间用一条竖 hairline 隔开。 */}
          <KaleidoscopeIcon
            active={topLevelView === 'kaleidoscope'}
            onClick={() => setTopLevelView('kaleidoscope')}
          />
          <div className="w-px h-[18px] bg-border/60 shrink-0" aria-hidden />

          {/* Workspace icons or dots */}
          <div
            ref={iconsContainerRef}
            role="toolbar"
            aria-label="工作区切换"
            aria-orientation="horizontal"
            onKeyDown={handleToolbarKeyDown}
            className={cn(
              // overflow-x-clip (not -auto) so we can keep overflow-y-visible
              // — `auto` on the x-axis forces y to also clip per CSS spec,
              // which was cutting off the running-indicator badge's
              // -top-0.5 overflow at the bar's top edge.
              'flex items-center gap-1 flex-1 min-w-0 overflow-x-clip overflow-y-visible',
              // Compact mode: spread items to occupy the full bar width.
              // gap-1 stays the minimum; justify-between only adds extra
              // spacing when there's leftover room (items + min-gaps <
              // container). Items don't shrink — they just space out.
              collapsed && 'justify-between',
            )}
          >
            {workspaces.map((w, i) => {
              const active = w.id === activeId
              const running = runningWorkspaceIds.has(w.id)
              const isDragging = drag?.id === w.id

              // Compute the shift for non-dragged icons. When dragging from
              // source → target, items strictly between (source, target]
              // shift LEFT by one stride (source < target case) or right
              // (source > target case). The dragged icon overlays them; on
              // release the actual list reorders.
              let shiftX = 0
              if (drag && !isDragging) {
                const { sourceIdx, targetIdx, stride } = drag
                if (sourceIdx < targetIdx && i > sourceIdx && i <= targetIdx) {
                  shiftX = -stride
                } else if (sourceIdx > targetIdx && i >= targetIdx && i < sourceIdx) {
                  shiftX = stride
                }
              }

              const shouldRenderAsDot = collapsed && !active

              const commonProps = {
                workspace: w, index: i, active, running,
                tabIndex: i === focusedIndex ? 0 : -1,
                buttonRef: (el: HTMLButtonElement | null) => {
                  buttonRefs.current[i] = el
                },
                onSelect: handleSelect,
                onPointerDown: startDrag,
                dragOffsetX: isDragging ? (drag?.deltaX ?? 0) : 0,
                shiftX,
                isDragging,
              }

              return shouldRenderAsDot
                ? <WorkspaceDot key={w.id} {...commonProps} />
                : <WorkspaceIcon key={w.id} {...commonProps} />
            })}
          </div>

          {/* + create new workspace */}
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => setCreateOpen(true)}
                aria-label="新建工作区"
                className="titlebar-no-drag inline-flex items-center justify-center
                           size-7 rounded-md text-foreground/55 hover:text-foreground
                           hover:bg-foreground/[0.05] transition-colors shrink-0"
              >
                <Plus className="size-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top" sideOffset={6}>
              <div className="font-medium">新建工作区</div>
            </TooltipContent>
          </Tooltip>
        </div>
      </TooltipProvider>

      <WorkspaceCreateDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={async (ws) => {
          await refresh()
          void selectWorkspace(ws.id)
        }}
      />
    </>
  )
}
