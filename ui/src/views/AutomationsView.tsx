import * as React from 'react'
import { useAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { cn } from '@/lib/utils'
import { AutomationHub } from '@/components/automation/AutomationHub'
import { AppsTab } from '@/components/automation/AppsTab'
import { StoreView } from '@/components/automation/StoreView'
import { StoreDetail } from '@/components/automation/StoreDetail'
import { automationsSubviewAtom } from '@/atoms/marketplace'

const TABS: { id: 'humans' | 'apps' | 'store'; label: string }[] = [
  { id: 'humans', label: '我的数字人' },
  { id: 'apps', label: '我的应用' },
  { id: 'store', label: '应用商店' },
]

export function AutomationsView(): React.ReactElement {
  const [subview, setSubview] = useAtom(automationsSubviewAtom)

  // Normalise 'store-detail' to 'store' for tab highlighting
  const activeTab = subview === 'store-detail' ? 'store' : subview

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Top tab strip */}
      <div className="flex items-center gap-1 px-6 py-2 border-b border-border/50 flex-shrink-0">
        {TABS.map((tab) => {
          const active = activeTab === tab.id
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setSubview(tab.id)}
              className={cn(
                'relative px-3 py-1.5 text-[13px] rounded-md transition-colors',
                active
                  ? 'bg-muted text-foreground font-medium'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent/30',
              )}
            >
              {active && <span className="absolute left-0 top-2 bottom-2 w-[2px] bg-primary rounded-r" />}
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Sub-view body */}
      <div className="flex-1 min-h-0 relative">
        <AnimatePresence mode="wait">
          <motion.div
            key={subview}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="absolute inset-0"
          >
            {subview === 'humans' && <AutomationHub />}
            {subview === 'apps' && <AppsTab />}
            {subview === 'store' && <StoreView />}
            {subview === 'store-detail' && <StoreDetail />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
