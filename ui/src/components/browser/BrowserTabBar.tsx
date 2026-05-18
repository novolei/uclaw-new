import * as React from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { BrowserTabEntry } from '@/atoms/browser-atoms'
import { browserUICloseTab } from '@/lib/tauri-bridge'

interface BrowserTabBarProps {
  sessionId: string
  tabs: BrowserTabEntry[]
  activeTabId: string | null
  onSelectTab: (tabId: string) => void
}

export function BrowserTabBar({ sessionId, tabs, activeTabId, onSelectTab }: BrowserTabBarProps): React.ReactElement | null {
  if (tabs.length <= 1) return null

  return (
    <div className="flex items-stretch gap-0.5 px-1 pt-1 bg-muted/30 border-b border-border/40 overflow-x-auto scrollbar-none">
      {tabs.map((tab) => (
        <button
          key={tab.tabId}
          onClick={() => onSelectTab(tab.tabId)}
          className={cn(
            'group flex items-center gap-1.5 px-3 py-1.5 rounded-t-md text-[12px] max-w-[160px] min-w-0 shrink-0',
            'border border-b-0 transition-colors',
            tab.tabId === activeTabId
              ? 'bg-popover border-border/60 text-foreground'
              : 'bg-transparent border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/50',
          )}
        >
          <span className="truncate flex-1">{tab.title || tab.url}</span>
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => {
              e.stopPropagation()
              browserUICloseTab(sessionId, tab.tabId).catch(console.error)
            }}
            onKeyDown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); browserUICloseTab(sessionId, tab.tabId).catch(console.error) } }}
            className="p-0.5 rounded opacity-0 group-hover:opacity-60 hover:!opacity-100 hover:bg-accent transition-all"
          >
            <X size={10} />
          </span>
        </button>
      ))}
    </div>
  )
}
