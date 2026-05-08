/**
 * BrowserViewer — live status panel for the AI Browser (Phase 3).
 * Shows browser running state, open tabs, and a launch/shutdown control.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Globe, Power, RefreshCw } from 'lucide-react'
import { browserStateAtom, isBrowserLoadingAtom } from '@/atoms/browser-atoms'
import { browserGetState, browserLaunch, browserShutdown } from '@/lib/tauri-bridge'

export function BrowserViewer(): React.ReactElement {
  const [state, setState] = useAtom(browserStateAtom)
  const [loading, setLoading] = useAtom(isBrowserLoadingAtom)

  const refresh = React.useCallback(async () => {
    try {
      const s = await browserGetState()
      setState(s)
    } catch {
      // ignore
    }
  }, [setState])

  React.useEffect(() => {
    refresh()
  }, [refresh])

  const handleLaunch = async () => {
    setLoading(true)
    try {
      await browserLaunch()
      await refresh()
    } catch (e) {
      console.error('Browser launch error:', e)
    } finally {
      setLoading(false)
    }
  }

  const handleShutdown = async () => {
    setLoading(true)
    try {
      await browserShutdown()
      await refresh()
    } catch (e) {
      console.error('Browser shutdown error:', e)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="flex flex-col h-full p-3 gap-3">
      {/* Status header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Globe size={14} className={state.running ? 'text-green-500' : 'text-muted-foreground'} />
          <span className="text-[12px] font-medium">
            {state.running ? 'Browser Running' : 'Browser Idle'}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={refresh}
            disabled={loading}
            className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
          <button
            onClick={state.running ? handleShutdown : handleLaunch}
            disabled={loading}
            className={[
              'flex items-center gap-1 px-2 py-1 rounded text-[11px] font-medium transition-colors',
              state.running
                ? 'bg-red-500/10 text-red-500 hover:bg-red-500/20'
                : 'bg-green-500/10 text-green-500 hover:bg-green-500/20',
            ].join(' ')}
          >
            <Power size={11} />
            {loading ? '…' : state.running ? 'Stop' : 'Launch'}
          </button>
        </div>
      </div>

      {/* Tabs list */}
      {state.running && (
        <div className="flex flex-col gap-1">
          <p className="text-[11px] text-muted-foreground font-medium">
            Open Tabs ({state.tabs.length})
          </p>
          {state.tabs.length === 0 ? (
            <p className="text-[11px] text-muted-foreground italic">No tabs open yet</p>
          ) : (
            state.tabs.map((tab) => (
              <div
                key={tab.tabId}
                className={[
                  'flex items-center gap-2 px-2 py-1.5 rounded text-[11px]',
                  state.activeTabId === tab.tabId
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground bg-muted/30',
                ].join(' ')}
              >
                <Globe size={10} />
                <span className="flex-1 truncate" title={tab.url}>
                  {tab.url || tab.tabId}
                </span>
              </div>
            ))
          )}
        </div>
      )}

      {!state.running && (
        <p className="text-[11px] text-muted-foreground">
          The AI Browser enables the agent to navigate websites, take screenshots, and fill forms.
          Launch it to allow the agent to use browser tools.
        </p>
      )}
    </div>
  )
}
