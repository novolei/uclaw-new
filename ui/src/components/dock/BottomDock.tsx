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
import { DockPinnedItem } from './DockPinnedItem'
import { ConnectionIndicator } from './ConnectionIndicator'
import { DockDragHandle } from './DockDragHandle'
import { useConnectionStatus } from './useConnectionStatus'
import { bottomDockEnabledAtom, dockOrderAtom, applyDockReorder, dockBounceKeysAtom, type DockItemSpec } from '@/atoms/dock-atoms'
import { useDockLiveness } from '@/hooks/useDockLiveness'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import { conversationsAtom } from '@/atoms/chat-atoms'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { humaneSpecsAtom } from '@/atoms/automation'
import { automationSelectedSpecIdAtom } from '@/atoms/automation-ui'
import { useOpenSession } from '@/hooks/useOpenSession'
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
  const bounceKeys = useAtomValue(dockBounceKeysAtom)
  const livenessMap = useDockLiveness()
  const appMode = useAtomValue(appModeAtom)
  const topLevelView = useAtomValue(topLevelViewAtom)
  const kaleidoscopeModule = useAtomValue(kaleidoscopeModuleAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setDockOrder = useSetAtom(dockOrderAtom)
  const conversations = useAtomValue(conversationsAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const automationSpecs = useAtomValue(humaneSpecsAtom)
  const setActiveWorkspaceId = useSetAtom(activeWorkspaceIdAtom)
  const setAutomationSelectedSpecId = useSetAtom(automationSelectedSpecIdAtom)
  const openSession = useOpenSession()

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
          {(() => {
            // Find the first non-mode index — divider sits before it. If no
            // non-mode entries exist, no divider renders.
            const firstPinIdx = dockOrder.findIndex((s) => s.kind !== 'mode')
            return dockOrder.map((spec, i) => {
              const sortableId = specToSortableId(spec)
              const dividerBefore = firstPinIdx !== -1 && i === firstPinIdx ? (
                <div
                  key="dock-section-divider"
                  data-dock-section-divider
                  className="mx-2 h-7 w-px self-center bg-border/50"
                  aria-hidden="true"
                />
              ) : null

              let body: React.ReactElement | null = null
              switch (spec.kind) {
                case 'mode': {
                  const meta = MODE_REGISTRY[spec.mode]
                  body = (
                    <DockItem
                      key={sortableId}
                      sortableId={sortableId}
                      bounceKey={bounceKeys[sortableId]}
                      liveness={livenessMap[sortableId]}
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
                  break
                }
                case 'pinned-conversation': {
                  const matchingConv = spec.type === 'chat'
                    ? conversations.find((c) => c.id === spec.sessionId)
                    : agentSessions.find((s) => s.id === spec.sessionId)
                  const label = matchingConv?.title
                    ?? `Conversation ${spec.sessionId.slice(0, 6)}`
                  const emoji = spec.type === 'agent'
                    ? agentSessions.find((s) => s.id === spec.sessionId)?.titleEmoji
                    : undefined
                  body = (
                    <DockPinnedItem
                      key={sortableId}
                      sortableId={sortableId}
                      label={label}
                      emoji={emoji}
                      index={i}
                      hoveredIndex={hoveredIndex}
                      onHoverIndexChange={setHoveredIndex}
                      onClick={() => openSession(spec.type, spec.sessionId, label)}
                    />
                  )
                  break
                }
                case 'pinned-workspace': {
                  const matchingSpace = workspaces.find((w) => w.id === spec.spaceId)
                  const label = matchingSpace?.name ?? `Workspace ${spec.spaceId.slice(0, 6)}`
                  const emoji = matchingSpace?.icon
                  body = (
                    <DockPinnedItem
                      key={sortableId}
                      sortableId={sortableId}
                      label={label}
                      emoji={emoji}
                      index={i}
                      hoveredIndex={hoveredIndex}
                      onHoverIndexChange={setHoveredIndex}
                      onClick={() => setActiveWorkspaceId(spec.spaceId)}
                    />
                  )
                  break
                }
                case 'pinned-automation': {
                  const matchingSpec = automationSpecs.find((s) => s.id === spec.specId)
                  const label = matchingSpec?.name ?? `Automation ${spec.specId.slice(0, 6)}`
                  body = (
                    <DockPinnedItem
                      key={sortableId}
                      sortableId={sortableId}
                      label={label}
                      index={i}
                      hoveredIndex={hoveredIndex}
                      onHoverIndexChange={setHoveredIndex}
                      onClick={() => {
                        setAutomationSelectedSpecId(spec.specId)
                        setKaleidoscopeModule('humans')
                        setTopLevelView('kaleidoscope')
                      }}
                    />
                  )
                  break
                }
              }

              return (
                <React.Fragment key={sortableId}>
                  {dividerBefore}
                  {body}
                </React.Fragment>
              )
            })
          })()}
          <div className="flex items-center self-center pb-1 pr-1">
            <ConnectionIndicator />
          </div>
        </motion.div>
      </SortableContext>
    </DndContext>
  )
}
