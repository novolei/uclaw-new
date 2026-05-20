import * as React from 'react'
import { useSetAtom } from 'jotai'
import {
  browserCaptureScreenshot,
  browserStartScreencast,
  browserStopScreencast,
  listenScreencastFrames,
} from '@/lib/tauri-bridge'
import {
  browserScreencastActiveAtom,
  browserScreencastFrameAtom,
  type ScreencastFrameEntry,
} from '@/atoms/browser-atoms'

const activeConsumers = new Map<string, number>()
const liveFrameSeen = new Map<string, number>()

function screencastKey(sessionId: string, tabId: string): string {
  return `${sessionId}:${tabId}`
}

export function useBrowserScreencast(sessionId: string, tabId: string | null): void {
  const setFrameMap = useSetAtom(browserScreencastFrameAtom)
  const setActiveSet = useSetAtom(browserScreencastActiveAtom)

  React.useEffect(() => {
    if (!tabId) return

    const key = screencastKey(sessionId, tabId)
    let unlisten: (() => void) | null = null
    let cancelled = false

    listenScreencastFrames((payload) => {
      if (payload.sessionId !== sessionId) return
      setFrameMap((prev) => {
        const next = new Map(prev)
        const entry: ScreencastFrameEntry = {
          tabId: payload.tabId,
          dataB64: payload.dataB64,
          mimeType: 'image/jpeg',
          pageWidth: payload.pageWidth,
          pageHeight: payload.pageHeight,
          timestamp: Date.now(),
        }
        liveFrameSeen.set(screencastKey(sessionId, payload.tabId), entry.timestamp)
        next.set(sessionId, entry)
        return next
      })
    }).then((fn) => {
      if (cancelled) {
        fn()
        return
      }

      unlisten = fn
      const prevCount = activeConsumers.get(key) ?? 0
      activeConsumers.set(key, prevCount + 1)

      if (prevCount === 0) {
        setActiveSet((prev) => {
          const next = new Set(prev)
          next.add(sessionId)
          return next
        })
        browserStartScreencast(sessionId, tabId).catch(console.error)
      }

      let stopped = false
      let fallbackTried = false
      const captureFallback = () => {
        const lastLiveFrame = liveFrameSeen.get(key) ?? 0
        if (stopped || fallbackTried || lastLiveFrame > 0 || Date.now() - lastLiveFrame < 2_000) return
        fallbackTried = true
        browserCaptureScreenshot(sessionId, tabId)
          .then((dataB64) => {
            if (stopped) return
            setFrameMap((prev) => {
              const next = new Map(prev)
              next.set(sessionId, {
                tabId,
                dataB64,
                mimeType: 'image/png',
                pageWidth: 1280,
                pageHeight: 800,
                timestamp: Date.now(),
              })
              return next
            })
          })
          .catch(console.error)
      }
      const fallbackTimer = window.setInterval(captureFallback, 500)
      const prevUnlisten = unlisten
      unlisten = () => {
        stopped = true
        window.clearInterval(fallbackTimer)
        prevUnlisten?.()
      }
    })

    return () => {
      cancelled = true
      if (unlisten) unlisten()

      const prevCount = activeConsumers.get(key) ?? 0
      if (prevCount <= 1) {
        activeConsumers.delete(key)
        setActiveSet((prev) => {
          const next = new Set(prev)
          next.delete(sessionId)
          return next
        })
        browserStopScreencast(sessionId, tabId).catch(() => {})
      } else {
        activeConsumers.set(key, prevCount - 1)
      }
    }
  }, [sessionId, tabId, setFrameMap, setActiveSet])
}
