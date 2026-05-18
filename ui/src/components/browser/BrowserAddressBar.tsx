import * as React from 'react'
import { ArrowLeft, ArrowRight, RefreshCw, Globe } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  browserUIGoBack,
  browserUIGoForward,
  browserUIReload,
  browserUINavigate,
  browserStartScreencast,
} from '@/lib/tauri-bridge'

interface BrowserAddressBarProps {
  sessionId: string
  tabId: string | null
  url: string
  isLoading?: boolean
}

export function BrowserAddressBar({ sessionId, tabId, url, isLoading }: BrowserAddressBarProps): React.ReactElement {
  const [draft, setDraft] = React.useState(url)

  React.useEffect(() => { setDraft(url) }, [url])

  const navigate = () => {
    if (!tabId) return
    let target = draft.trim()
    if (target && !target.includes('://')) target = 'https://' + target
    browserUINavigate(sessionId, tabId, target)
      .then(() => browserStartScreencast(sessionId, tabId!))
      .catch(console.error)
  }

  return (
    <div className="flex items-center gap-1 px-2 py-1.5 border-b border-border/50 bg-muted/20">
      <button
        onClick={() => tabId && browserUIGoBack(sessionId, tabId).catch(console.error)}
        disabled={!tabId}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="后退"
      >
        <ArrowLeft size={13} />
      </button>
      <button
        onClick={() => tabId && browserUIGoForward(sessionId, tabId).catch(console.error)}
        disabled={!tabId}
        className="p-1 rounded hover:bg-accent disabled:opacity-30 text-muted-foreground hover:text-foreground transition-colors"
        title="前进"
      >
        <ArrowRight size={13} />
      </button>
      <button
        onClick={() => tabId && browserUIReload(sessionId, tabId).then(() => browserStartScreencast(sessionId, tabId!)).catch(console.error)}
        disabled={!tabId}
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
          onBlur={() => setDraft(url)}
          className="flex-1 bg-transparent text-[12px] outline-none text-foreground placeholder:text-muted-foreground min-w-0"
          placeholder="输入网址…"
          spellCheck={false}
        />
      </div>
    </div>
  )
}
