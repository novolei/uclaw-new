import * as React from 'react'
import { motion } from 'motion/react'
import { useAtomValue, useSetAtom } from 'jotai'
import { MessageSquare, Bot, Brain, Sparkles } from 'lucide-react'
import { DockItem } from './DockItem'
import { ConnectionIndicator } from './ConnectionIndicator'
import { useConnectionStatus } from './useConnectionStatus'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'

interface BottomDockProps {
  revealed: boolean
  onRevealChange: (revealed: boolean) => void
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

interface NavItem {
  id: string
  icon: React.ReactNode
  label: string
  isActive: (ctx: NavCtx) => boolean
  onClick: (ctx: ActionCtx) => void
}

const NAV_ITEMS: NavItem[] = [
  {
    id: 'chat',
    icon: <MessageSquare size={18} className="text-white/80" />,
    label: '聊天',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'chat' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('chat')
      setTopLevelView('workspace')
    },
  },
  {
    id: 'agent',
    icon: <Bot size={18} className="text-white/80" />,
    label: 'Agent',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'agent' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('agent')
      setTopLevelView('workspace')
    },
  },
  {
    id: 'memory',
    icon: <Brain size={18} className="text-white/80" />,
    label: '记忆',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule === 'memory',
    onClick: ({ setKaleidoscopeModule, setTopLevelView }) => {
      setKaleidoscopeModule('memory')
      setTopLevelView('kaleidoscope')
    },
  },
  {
    id: 'kaleidoscope',
    icon: <Sparkles size={18} className="text-white/80" />,
    label: '万花筒',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule !== 'memory',
    onClick: ({ setTopLevelView }) => {
      setTopLevelView('kaleidoscope')
    },
  },
]

export function BottomDock({ revealed, onRevealChange }: BottomDockProps) {
  const isDockEnabled = useAtomValue(bottomDockEnabledAtom)
  const appMode = useAtomValue(appModeAtom)
  const topLevelView = useAtomValue(topLevelViewAtom)
  const kaleidoscopeModule = useAtomValue(kaleidoscopeModuleAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)

  const [hoveredIndex, setHoveredIndex] = React.useState<number | null>(null)
  const hideTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // NOTE: useConnectionStatus must be called before the early-return guard below
  // to comply with the Rules of Hooks (no conditional hook calls).
  useConnectionStatus()

  // Clean up any pending hide timer on unmount.
  React.useEffect(() => {
    return () => {
      if (hideTimerRef.current !== null) clearTimeout(hideTimerRef.current)
    }
  }, [])

  if (!isDockEnabled) return null

  const navCtx: NavCtx = { appMode, topLevelView, kaleidoscopeModule }
  const actionCtx: ActionCtx = { setAppMode, setTopLevelView, setKaleidoscopeModule }

  const handleDockMouseEnter = () => {
    if (hideTimerRef.current !== null) clearTimeout(hideTimerRef.current)
  }

  const handleDockMouseLeave = () => {
    setHoveredIndex(null)
    hideTimerRef.current = setTimeout(() => onRevealChange(false), 200)
  }

  return (
    <motion.div
      className="fixed bottom-0 left-1/2 -translate-x-1/2 z-[70] pointer-events-auto"
      animate={{ y: revealed ? 0 : 'calc(100% + 8px)' }}
      initial={{ y: 'calc(100% + 8px)' }}
      transition={{ type: 'spring', stiffness: 300, damping: 28 }}
      onMouseEnter={handleDockMouseEnter}
      onMouseLeave={handleDockMouseLeave}
    >
      <div className="flex items-end gap-2 px-4 pt-3 pb-2 rounded-t-2xl bg-black/70 backdrop-blur-xl border-t border-x border-white/[0.08] shadow-[0_-4px_24px_rgba(0,0,0,0.4)]">
        {NAV_ITEMS.map((item, i) => (
          <DockItem
            key={item.id}
            icon={item.icon}
            label={item.label}
            isActive={item.isActive(navCtx)}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={(idx) => setHoveredIndex(idx)}
            onClick={() => item.onClick(actionCtx)}
          />
        ))}
        <div className="ml-3 mb-1 flex items-center self-center">
          <ConnectionIndicator />
        </div>
      </div>
    </motion.div>
  )
}
