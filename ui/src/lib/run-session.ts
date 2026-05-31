import { useEffect } from 'react'

export function scheduleRunSessionRefresh(refresh: () => void | Promise<void>, delaysMs: number[] = [250, 1000]) {
  const timers = delaysMs.map((delay) => window.setTimeout(() => { void refresh() }, delay))
  return () => {
    for (const timer of timers) window.clearTimeout(timer)
  }
}

export function useRunSessionPolling(active: boolean, refresh: () => void | Promise<void>, intervalMs = 3000) {
  useEffect(() => {
    if (!active) return
    const id = window.setInterval(() => { void refresh() }, intervalMs)
    return () => window.clearInterval(id)
  }, [active, intervalMs, refresh])
}
