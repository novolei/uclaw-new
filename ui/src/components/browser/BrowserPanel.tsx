/**
 * BrowserPanel — full-size browser view mounted inside the preview panel.
 *
 * Subscribes to browser:screencast-frame and routes frames into
 * browserScreencastFrameAtom. Fetches DOM state on demand when overlay is on.
 */

import * as React from 'react'
import { useSetAtom, useAtomValue } from 'jotai'
import {
  listenScreencastFrames,
  listenNavState,
  browserGetDOMState,
  browserStartScreencast,
  browserStopScreencast,
} from '@/lib/tauri-bridge'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserScreencastActiveAtom,
  browserDOMOverlayVisibleAtom,
  browserNavStateAtom,
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
  const setNavState = useSetAtom(browserNavStateAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)
  const previewMap = useAtomValue(sessionBrowserPreviewMapAtom)

  const preview = previewMap.get(agentSessionId)
  const activeTabId = preview?.tabId ?? null
  const currentUrl = preview?.url ?? ''

  const domMap = useAtomValue(browserDOMStateAtom)
  const domEntry = domMap.get(agentSessionId)
  const tabs: BrowserTabEntry[] = domEntry?.tabs ?? []
  const displayUrl = domEntry?.url ?? currentUrl

  // CDP screencast lifecycle: subscribe to the frame stream FIRST, then
  // tell the backend to start emitting. Tauri's `listen()` is async — the
  // listener isn't registered until its Promise resolves. If we call
  // `browserStartScreencast` before that, Chrome may emit its initial frame
  // into the void: for a static page (e.g. after the first paint) Chrome
  // only emits another frame on the next paint, so the UI sits at
  // "等待浏览器画面..." forever.
  React.useEffect(() => {
    if (!activeTabId) return
    let unlisten: (() => void) | null = null
    let cancelled = false
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
      if (cancelled) { fn(); return }
      unlisten = fn
      setActiveSet((prev) => {
        const next = new Set(prev)
        next.add(agentSessionId)
        return next
      })
      browserStartScreencast(agentSessionId, activeTabId).catch(console.error)
    })

    return () => {
      cancelled = true
      if (unlisten) unlisten()
      setActiveSet((prev) => {
        const next = new Set(prev)
        next.delete(agentSessionId)
        return next
      })
      browserStopScreencast(agentSessionId, activeTabId).catch(() => {})
    }
  }, [agentSessionId, activeTabId, setFrameMap, setActiveSet])

  // Subscribe to navigation state events for this session.
  React.useEffect(() => {
    let unlisten: (() => void) | null = null
    listenNavState((payload) => {
      if (payload.sessionId !== agentSessionId) return
      setNavState((prev) => {
        const next = new Map(prev)
        next.set(agentSessionId, {
          tabId: payload.tabId,
          url: payload.url,
          title: payload.title,
          isLoading: payload.isLoading,
          canGoBack: payload.canGoBack,
          canGoForward: payload.canGoForward,
        })
        return next
      })
    }).then((fn) => { unlisten = fn })
    return () => { if (unlisten) unlisten() }
  }, [agentSessionId, setNavState])

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
