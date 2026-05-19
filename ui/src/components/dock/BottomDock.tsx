import * as React from 'react'
import { motion, Reorder } from 'motion/react'
import { useAtomValue, useSetAtom } from 'jotai'
import { DockItem } from './DockItem'
import { DockPinnedItem } from './DockPinnedItem'
import { ConnectionIndicator } from './ConnectionIndicator'
import { DockDragHandle } from './DockDragHandle'
import { useConnectionStatus } from './useConnectionStatus'
import { useLongPressDrag } from './useLongPressDrag'
import { bottomDockEnabledAtom, dockOrderAtom, dockBounceKeysAtom, ensureCanonicalModes, type DockItemSpec } from '@/atoms/dock-atoms'
import { useDockLiveness } from '@/hooks/useDockLiveness'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import { connectionsPanelOpenAtom, alertPanelOpenAtom } from '@/atoms/dock-placeholder-atoms'
import { conversationsAtom } from '@/atoms/chat-atoms'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { humaneSpecsAtom } from '@/atoms/automation'
import { automationSelectedSpecIdAtom } from '@/atoms/automation-ui'
import { useOpenSession } from '@/hooks/useOpenSession'
import chatIcon from '@/assets/dock-icons/chat.webp'
import agentIcon from '@/assets/dock-icons/agent.webp'
import symphonyIcon from '@/assets/dock-icons/symphony.webp'
import memoryIcon from '@/assets/dock-icons/memory.webp'
import kaleidoscopeIcon from '@/assets/dock-icons/kaleidoscope.webp'
import homeIcon from '@/assets/dock-icons/home-office.webp'
import connectionsIcon from '@/assets/dock-icons/connections.webp'
import alertIcon from '@/assets/dock-icons/alert.webp'
import settingsIcon from '@/assets/dock-icons/settings.webp'

interface BottomDockProps {
  /** Controlled from BottomDockHoverRegion. Drives slide animation. */
  revealed: boolean
}

interface NavCtx {
  appMode: AppMode
  topLevelView: TopLevelView
  kaleidoscopeModule: KaleidoscopeModuleId
  homeOfficeOpen: boolean
  settingsOpen: boolean
  connectionsOpen: boolean
  alertOpen: boolean
}

interface ActionCtx {
  setAppMode: (m: AppMode) => void
  setTopLevelView: (v: TopLevelView) => void
  setKaleidoscopeModule: (m: KaleidoscopeModuleId) => void
  setHomeOfficeOpen: (v: boolean) => void
  setSettingsOpen: (v: boolean) => void
  setConnectionsOpen: (v: boolean) => void
  setAlertOpen: (v: boolean) => void
}

type ModeId = 'chat' | 'agent' | 'symphony' | 'memory' | 'kaleidoscope' | 'home' | 'connections' | 'alert' | 'settings'

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
  symphony: {
    iconSrc: symphonyIcon,
    label: 'Symphony',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'symphony' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('symphony')
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
  home: {
    iconSrc: homeIcon,
    label: '家',
    // HomeOfficeView renders inside WorkspaceShell when its atom is true,
    // so isActive requires both surface=workspace AND the panel toggle.
    isActive: ({ topLevelView, homeOfficeOpen }) =>
      topLevelView === 'workspace' && homeOfficeOpen,
    onClick: ({ setTopLevelView, setHomeOfficeOpen }) => {
      setTopLevelView('workspace')
      setHomeOfficeOpen(true)
    },
  },
  connections: {
    iconSrc: connectionsIcon,
    label: '连接',
    isActive: ({ connectionsOpen }) => connectionsOpen,
    onClick: ({ setConnectionsOpen }) => setConnectionsOpen(true),
  },
  alert: {
    iconSrc: alertIcon,
    label: '通知',
    isActive: ({ alertOpen }) => alertOpen,
    onClick: ({ setAlertOpen }) => setAlertOpen(true),
  },
  settings: {
    iconSrc: settingsIcon,
    label: '设置',
    isActive: ({ settingsOpen }) => settingsOpen,
    onClick: ({ setSettingsOpen }) => setSettingsOpen(true),
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
  const homeOfficeOpen = useAtomValue(homeOfficePanelOpenAtom)
  const settingsOpen = useAtomValue(settingsOpenAtom)
  const connectionsOpen = useAtomValue(connectionsPanelOpenAtom)
  const alertOpen = useAtomValue(alertPanelOpenAtom)
  const setHomeOfficeOpen = useSetAtom(homeOfficePanelOpenAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setConnectionsOpen = useSetAtom(connectionsPanelOpenAtom)
  const setAlertOpen = useSetAtom(alertPanelOpenAtom)
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

  // Reset magnification when collapsed so reopening doesn't briefly show
  // a stale hovered icon before mouse lands somewhere new.
  React.useEffect(() => {
    if (!revealed) setHoveredIndex(null)
  }, [revealed])

  // One-shot migration: existing localStorage orders predate home/
  // connections/alert/settings — append any canonical mode missing from
  // the persisted list. The helper is referentially stable when nothing
  // is missing, so this is a no-op after the first apply.
  React.useEffect(() => {
    setDockOrder((current) => ensureCanonicalModes(current))
  }, [setDockOrder])

  if (!isDockEnabled) return null

  const navCtx: NavCtx = {
    appMode,
    topLevelView,
    kaleidoscopeModule,
    homeOfficeOpen,
    settingsOpen,
    connectionsOpen,
    alertOpen,
  }
  const actionCtx: ActionCtx = {
    setAppMode,
    setTopLevelView,
    setKaleidoscopeModule,
    setHomeOfficeOpen,
    setSettingsOpen,
    setConnectionsOpen,
    setAlertOpen,
  }

  // For each item we render a sibling that owns the visual + click target.
  // The reorder logic lives on `<Reorder.Item>` wrapping that visual — it
  // applies `layout` animations (so neighbors squeeze and slide out of
  // the way as the dragged item passes) and `whileDrag` styling (so the
  // dragged item lifts iOS Springboard-style and follows the cursor).
  const firstPinIdx = dockOrder.findIndex((s) => s.kind !== 'mode')
  const renderItemContent = (spec: DockItemSpec, i: number): React.ReactElement | null => {
    const sortableId = specToSortableId(spec)
    switch (spec.kind) {
      case 'mode': {
        const meta = MODE_REGISTRY[spec.mode]
        return (
          <DockItem
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
      }
      case 'pinned-conversation': {
        const matchingConv = spec.type === 'chat'
          ? conversations.find((c) => c.id === spec.sessionId)
          : agentSessions.find((s) => s.id === spec.sessionId)
        const label = matchingConv?.title ?? `Conversation ${spec.sessionId.slice(0, 6)}`
        const emoji = spec.type === 'agent'
          ? agentSessions.find((s) => s.id === spec.sessionId)?.titleEmoji
          : undefined
        return (
          <DockPinnedItem
            sortableId={sortableId}
            label={label}
            emoji={emoji}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={setHoveredIndex}
            onClick={() => openSession(spec.type, spec.sessionId, label)}
          />
        )
      }
      case 'pinned-workspace': {
        const matchingSpace = workspaces.find((w) => w.id === spec.spaceId)
        const label = matchingSpace?.name ?? `Workspace ${spec.spaceId.slice(0, 6)}`
        return (
          <DockPinnedItem
            sortableId={sortableId}
            label={label}
            emoji={matchingSpace?.icon}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={setHoveredIndex}
            onClick={() => setActiveWorkspaceId(spec.spaceId)}
          />
        )
      }
      case 'pinned-automation': {
        const matchingSpec = automationSpecs.find((s) => s.id === spec.specId)
        const label = matchingSpec?.name ?? `Automation ${spec.specId.slice(0, 6)}`
        return (
          <DockPinnedItem
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
      }
    }
  }

  return (
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
      <Reorder.Group
        axis="x"
        as="div"
        values={dockOrder}
        onReorder={setDockOrder}
        className="flex items-end gap-1"
        // Ensure layoutScroll is on so motion factors in the parent flex
        // container when measuring deltas — without this, layout
        // animations can mismeasure when the dock itself is sliding.
        layoutScroll
      >
        {dockOrder.map((spec, i) => {
          const sortableId = specToSortableId(spec)
          const dividerBefore = firstPinIdx !== -1 && i === firstPinIdx ? (
            <DockSectionDivider key={`divider-before-${sortableId}`} />
          ) : null
          return (
            <React.Fragment key={sortableId}>
              {dividerBefore}
              <DockReorderItem spec={spec} sortableId={sortableId}>
                {renderItemContent(spec, i)}
              </DockReorderItem>
            </React.Fragment>
          )
        })}
      </Reorder.Group>
      <div className="flex items-center self-center pb-1 pr-1">
        <ConnectionIndicator />
      </div>
    </motion.div>
  )
}

/**
 * Non-animated section divider — sits between mode and pinned items.
 * Kept as its own non-Reorder element so motion's layout pipeline never
 * treats it as a sortable value.
 */
function DockSectionDivider(): React.ReactElement {
  return (
    <div
      data-dock-section-divider
      className="mx-2 h-7 w-px self-center bg-border/50"
      aria-hidden="true"
    />
  )
}

/**
 * `Reorder.Item` wrapper that owns drag activation + lift visuals.
 *
 * - Long-press 200 ms gate via `useLongPressDrag` (keeps simple taps
 *   responsive — only after the user holds does motion start tracking).
 * - `whileDrag` lifts the icon iOS-style (scale 1.18, drop shadow, slight
 *   tilt) so the dragged item visibly leaves the dock plane.
 * - `layout` + `transition` give neighbors a springy slide-and-squeeze as
 *   the dragged item passes over their slot.
 *
 * The child visual (DockItem / DockPinnedItem) handles hover magnify +
 * click; this wrapper purely owns drag/reorder mechanics.
 */
const DRAG_LIFT_TRANSITION = {
  type: 'spring' as const,
  stiffness: 520,
  damping: 34,
  mass: 0.7,
}
const REORDER_LAYOUT_TRANSITION = {
  type: 'spring' as const,
  stiffness: 480,
  damping: 36,
  mass: 0.6,
}

function DockReorderItem({
  spec,
  sortableId,
  children,
}: {
  spec: DockItemSpec
  sortableId: string
  children: React.ReactNode
}): React.ReactElement {
  const { dragControls, pointerHandlers } = useLongPressDrag({
    delayMs: 200,
    tolerancePx: 5,
  })
  return (
    <Reorder.Item
      value={spec}
      as="div"
      data-reorder-id={sortableId}
      // Bypass motion's default pointer listener so taps don't immediately
      // start dragging — long-press gate above will start the drag manually.
      dragListener={false}
      dragControls={dragControls}
      // `whileDrag` is the iOS Springboard lift — scale up, lift, glow.
      // motion's portal tracks the cursor automatically while these styles
      // apply, so the dragged icon visibly leaves the dock and follows the
      // finger / cursor 1:1.
      whileDrag={{
        scale: 1.18,
        y: -8,
        rotate: -1.5,
        zIndex: 50,
        filter:
          'brightness(1.06) drop-shadow(0 16px 26px hsl(var(--foreground) / 0.35)) drop-shadow(0 4px 8px hsl(var(--foreground) / 0.22))',
        cursor: 'grabbing',
      }}
      // No drag axis lock — motion will translate freely. The visual layer
      // will follow the cursor; the `onReorder` callback fires when the
      // dragged item crosses neighbor centerlines, swapping live so the
      // dock stays visually consistent with the array order.
      transition={REORDER_LAYOUT_TRANSITION}
      // dragTransition tames the "snap-back if dropped on empty space"
      // motion-default — small power → quick settle to the new slot.
      dragTransition={{ bounceStiffness: 600, bounceDamping: 28, power: 0.18 }}
      layout
      {...pointerHandlers}
      // Drag controls need a hint: spring out of the way as we follow.
      style={{ touchAction: 'none' }}
    >
      {children}
    </Reorder.Item>
  )
}

// Suppress unused-import lint for the drag-lift transition tuning constant
// (kept exported-shape for future tuning — same name pattern as
// REVEAL_TRANSITION / HIDE_TRANSITION at the top of this file).
void DRAG_LIFT_TRANSITION
