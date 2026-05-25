import * as React from 'react'
import { ArrowLeft, ArrowRight, RefreshCw, Globe } from 'lucide-react'
import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import {
  browserUIGoBack,
  browserUIGoForward,
  browserUIReload,
  browserUINavigate,
} from '@/lib/tauri-bridge'
import { isRealBrowserTabId } from '@/lib/browser-tabs'
import { browserNavStateAtom } from '@/atoms/browser-atoms'

interface BrowserAddressBarProps {
  sessionId: string
  tabId: string | null
  url: string
}

export function BrowserAddressBar({ sessionId, tabId, url }: BrowserAddressBarProps): React.ReactElement {
  const navStateMap = useAtomValue(browserNavStateAtom)
  const navState = navStateMap.get(sessionId)
  const realTabId = isRealBrowserTabId(tabId) ? tabId : null

  const liveUrl = navState?.url || url
  const isLoading = navState?.isLoading ?? false
  const canGoBack = navState?.canGoBack ?? false
  const canGoForward = navState?.canGoForward ?? false

  const [draft, setDraft] = React.useState(liveUrl)
  const [focused, setFocused] = React.useState(false)

  React.useEffect(() => {
    if (!focused) setDraft(liveUrl)
  }, [liveUrl, focused])

  const navigate = () => {
    let target = draft.trim()
    if (!target) return
    if (target && !target.includes('://')) target = 'https://' + target
    browserUINavigate(sessionId, realTabId ?? 'new', target)
      .catch(console.error)
  }

  return (
    <div className="flex items-center gap-1 px-2 py-1.5 border-b border-border/50 bg-muted/20">
      <button
        onClick={() => realTabId && browserUIGoBack(sessionId, realTabId).catch(console.error)}
        disabled={!canGoBack}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="后退"
      >
        <ArrowLeft size={13} />
      </button>
      <button
        onClick={() => realTabId && browserUIGoForward(sessionId, realTabId).catch(console.error)}
        disabled={!canGoForward}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="前进"
      >
        <ArrowRight size={13} />
      </button>
      <button
        onClick={() => realTabId && browserUIReload(sessionId, realTabId).catch(console.error)}
        disabled={!realTabId}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="刷新"
      >
        <RefreshCw size={13} className={cn(isLoading && 'animate-spin')} />
      </button>

      <div className="flex flex-1 items-center gap-1.5 bg-popover border border-border/60 rounded-md px-2 h-7">
        <Globe size={11} className="text-muted-foreground shrink-0" />
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') navigate() }}
          onFocus={() => setFocused(true)}
          onBlur={() => { setFocused(false); setDraft(liveUrl) }}
          className="flex-1 bg-transparent text-[12px] outline-none text-foreground placeholder:text-muted-foreground min-w-0"
          placeholder="输入网址…"
          spellCheck={false}
        />
      </div>
    </div>
  )
}
