import * as React from 'react'
import { motion } from 'motion/react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  SortableContext,
  horizontalListSortingStrategy,
} from '@dnd-kit/sortable'
import { DockItem } from './DockItem'
import { ConnectionIndicator } from './ConnectionIndicator'
import { DockDragHandle } from './DockDragHandle'
import { useConnectionStatus } from './useConnectionStatus'
import { bottomDockEnabledAtom, dockOrderAtom, applyDockReorder, type DockItemSpec } from '@/atoms/dock-atoms'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import chatIcon from '@/assets/dock-icons/chat.png'
import agentIcon from '@/assets/dock-icons/agent.png'
import memoryIcon from '@/assets/dock-icons/memory.png'
import kaleidoscopeIcon from '@/assets/dock-icons/kaleidoscope.png'

interface BottomDockProps {
  /** Controlled from BottomDockHoverRegion. Drives slide animation. */
  revealed: boolean
}

interface NavCtx {
  appMode: AppMode
  topLevelView: TopLevelView
  kaleidoscopeModule: KaleidoscopeModuleId
}

interface ActionCtx {
  setAppMode: (m: AppMode) => void
  setTopLevelView: (v: TopLevelView) => void
  setKaleidoscopeModule: (m: KaleidoscopeModuleId) => void
}

type ModeId = 'chat' | 'agent' | 'memory' | 'kaleidoscope'

interface ModeMeta {
  iconSrc: string
  label: string
  isActive: (ctx: NavCtx) => boolean
  onClick: (ctx: ActionCtx) => void
}

const MODE_REGISTRY: Record<ModeId, ModeMeta> = {
  chat: {
    iconSrc: chatIcon,
    label: '聊天',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'chat' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('chat')
      setTopLevelView('workspace')
    },
  },
  agent: {
    iconSrc: agentIcon,
    label: 'Agent',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'agent' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('agent')
      setTopLevelView('workspace')
    },
  },
  memory: {
    iconSrc: memoryIcon,
    label: '记忆',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule === 'memory',
    onClick: ({ setKaleidoscopeModule, setTopLevelView }) => {
      setKaleidoscopeModule('memory')
      setTopLevelView('kaleidoscope')
    },
  },
  kaleidoscope: {
    iconSrc: kaleidoscopeIcon,
    label: '万花筒',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule !== 'memory',
    onClick: ({ setTopLevelView }) => {
      setTopLevelView('kaleidoscope')
    },
  },
}

function specToSortableId(spec: DockItemSpec): string {
  switch (spec.kind) {
    case 'mode':
      return `mode-${spec.mode}`
    case 'pinned-conversation':
      return `conv-${spec.sessionId}`
    case 'pinned-workspace':
      return `space-${spec.spaceId}`
    case 'pinned-automation':
      return `auto-${spec.specId}`
  }
}

const SLIDE_HIDDEN_Y = 96 // px; large enough to clear dock height in any theme

// macOS Dock-style asymmetric transitions: spring when coming UP (feels
// alive, slight settle), but tween + opacity fade on the way DOWN so the
// dock literally "slips away" — no rebound, no overshoot, no slam at the
// bottom. Curve is Apple's standard smooth ease used in iOS system
// transitions; opacity fades a touch faster than translate so the dock is
// already mostly invisible before reaching its hidden y.
const REVEAL_TRANSITION = {
  y: { type: 'spring' as const, stiffness: 300, damping: 30, mass: 0.8 },
  opacity: { duration: 0.16, ease: 'easeOut' as const },
}
const HIDE_TRANSITION = {
  y: {
    duration: 0.42,
    ease: [0.32, 0.72, 0, 1] as [number, number, number, number],
  },
  opacity: {
    duration: 0.3,
    ease: [0.4, 0, 0.6, 1] as [number, number, number, number],
  },
}

export function BottomDock({ revealed }: BottomDockProps): React.ReactElement | null {
  const isDockEnabled = useAtomValue(bottomDockEnabledAtom)
  const dockOrder = useAtomValue(dockOrderAtom)
  const appMode = useAtomValue(appModeAtom)
  const topLevelView = useAtomValue(topLevelViewAtom)
  const kaleidoscopeModule = useAtomValue(kaleidoscopeModuleAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setDockOrder = useSetAtom(dockOrderAtom)

  const [hoveredIndex, setHoveredIndex] = React.useState<number | null>(null)

  // Rules of Hooks: keep before any early return.
  useConnectionStatus()

  // Long-press 200 ms before drag activates — keeps simple taps responsive.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { delay: 200, tolerance: 5 } }),
  )

  const sortableIds = React.useMemo(
    () => dockOrder.map(specToSortableId),
    [dockOrder],
  )

  const handleDragEnd = React.useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event
      if (!over) return
      setDockOrder((current) =>
        applyDockReorder(current, sortableIds, String(active.id), String(over.id)),
      )
    },
    [sortableIds, setDockOrder],
  )

  // Reset magnification when collapsed so reopening doesn't briefly show
  // a stale hovered icon before mouse lands somewhere new.
  React.useEffect(() => {
    if (!revealed) setHoveredIndex(null)
  }, [revealed])

  if (!isDockEnabled) return null

  const navCtx: NavCtx = { appMode, topLevelView, kaleidoscopeModule }
  const actionCtx: ActionCtx = {
    setAppMode,
    setTopLevelView,
    setKaleidoscopeModule,
  }

  return (
    <DndContext sensors={sensors} onDragEnd={handleDragEnd}>
      <SortableContext items={sortableIds} strategy={horizontalListSortingStrategy}>
        <motion.div
          role="navigation"
          aria-label="底部导航"
          data-dock-dnd-root
          className="group relative flex items-end gap-1 px-3 pt-3 pb-2 rounded-t-2xl bg-popover/85 backdrop-blur-xl border-t border-x border-border/40 shadow-[0_-10px_30px_-12px_rgba(0,0,0,0.35)] supports-[backdrop-filter]:bg-popover/70 will-change-transform"
          initial={false}
          animate={{ y: revealed ? 0 : SLIDE_HIDDEN_Y, opacity: revealed ? 1 : 0 }}
          transition={revealed ? REVEAL_TRANSITION : HIDE_TRANSITION}
          onMouseLeave={() => setHoveredIndex(null)}
        >
          <DockDragHandle />
          {dockOrder.map((spec, i) => {
            if (spec.kind !== 'mode') return null
            const meta = MODE_REGISTRY[spec.mode]
            const sortableId = specToSortableId(spec)
            return (
              <DockItem
                key={sortableId}
                sortableId={sortableId}
                icon={
                  <img
                    src={meta.iconSrc}
                    alt={meta.label}
                    draggable={false}
                    className="w-7 h-7 select-none pointer-events-none"
                  />
                }
                label={meta.label}
                isActive={meta.isActive(navCtx)}
                index={i}
                hoveredIndex={hoveredIndex}
                onHoverIndexChange={setHoveredIndex}
                onClick={() => meta.onClick(actionCtx)}
              />
            )
          })}
          <div
            className="mx-2 h-7 w-px self-center bg-border/50"
            aria-hidden="true"
          />
          <div className="flex items-center self-center pb-1 pr-1">
            <ConnectionIndicator />
          </div>
        </motion.div>
      </SortableContext>
    </DndContext>
  )
}
