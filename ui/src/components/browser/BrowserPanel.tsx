/**
 * BrowserPanel — full-size browser view mounted inside the preview panel.
 *
 * Subscribes to browser:screencast-frame and routes frames into
 * browserScreencastFrameAtom. Fetches DOM state on demand when overlay is on.
 */

import * as React from 'react'
import { useSetAtom, useAtomValue } from 'jotai'
import { listenScreencastFrames, browserGetDOMState } from '@/lib/tauri-bridge'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserScreencastActiveAtom,
  browserDOMOverlayVisibleAtom,
  type BrowserTabEntry,
  type ScreencastFrameEntry,
} from '@/atoms/browser-atoms'
import { sessionBrowserPreviewMapAtom } from '@/atoms/agent-atoms'
import { BrowserAddressBar } from './BrowserAddressBar'
import { BrowserTabBar } from './BrowserTabBar'
import { BrowserScreencastView } from './BrowserScreencastView'
import { BrowserStatusBar } from './BrowserStatusBar'

interface BrowserPanelProps {
  agentSessionId: string
}

export function BrowserPanel({ agentSessionId }: BrowserPanelProps): React.ReactElement {
  const setFrameMap = useSetAtom(browserScreencastFrameAtom)
  const setDomMap = useSetAtom(browserDOMStateAtom)
  const setActiveSet = useSetAtom(browserScreencastActiveAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)
  const previewMap = useAtomValue(sessionBrowserPreviewMapAtom)

  const preview = previewMap.get(agentSessionId)
  const activeTabId = preview?.tabId ?? null
  const currentUrl = preview?.url ?? ''

  const domMap = useAtomValue(browserDOMStateAtom)
  const domEntry = domMap.get(agentSessionId)
  const tabs: BrowserTabEntry[] = domEntry?.tabs ?? []
  const displayUrl = domEntry?.url ?? currentUrl

  // Subscribe to CDP screencast frames for this session.
  React.useEffect(() => {
    let unlisten: (() => void) | null = null
    listenScreencastFrames((payload) => {
      if (payload.sessionId !== agentSessionId) return
      setFrameMap((prev) => {
        const next = new Map(prev)
        const entry: ScreencastFrameEntry = {
          tabId: payload.tabId,
          dataB64: payload.dataB64,
          pageWidth: payload.pageWidth,
          pageHeight: payload.pageHeight,
          timestamp: Date.now(),
        }
        next.set(agentSessionId, entry)
        return next
      })
    }).then((fn) => {
      unlisten = fn
      setActiveSet((prev) => {
        const next = new Set(prev)
        next.add(agentSessionId)
        return next
      })
    })

    return () => {
      if (unlisten) unlisten()
      setActiveSet((prev) => {
        const next = new Set(prev)
        next.delete(agentSessionId)
        return next
      })
    }
  }, [agentSessionId, setFrameMap, setActiveSet])

  // Fetch DOM state when overlay is turned on.
  React.useEffect(() => {
    if (!overlayVisible || !activeTabId) return
    browserGetDOMState(agentSessionId, activeTabId)
      .then((state) => {
        setDomMap((prev) => {
          const next = new Map(prev)
          next.set(agentSessionId, {
            url: state.url,
            title: state.title,
            elements: state.elements.map((el) => ({
              index: el.index,
              tag: el.tag,
              text: el.text,
              isInViewport: el.isInViewport,
              boundingBox: el.boundingBox,
            })),
            pageText: state.pageText,
            tabs: state.tabs,
            timestamp: Date.now(),
          })
          return next
        })
      })
      .catch(console.error)
  }, [overlayVisible, activeTabId, agentSessionId, setDomMap])

  return (
    <div className="flex flex-col h-full w-full bg-popover">
      <BrowserTabBar
        sessionId={agentSessionId}
        tabs={tabs}
        activeTabId={activeTabId}
        onSelectTab={() => { /* future: switch active tab */ }}
      />
      <BrowserAddressBar
        sessionId={agentSessionId}
        tabId={activeTabId}
        url={displayUrl}
      />
      <BrowserScreencastView sessionId={agentSessionId} />
      <BrowserStatusBar sessionId={agentSessionId} />
    </div>
  )
}
