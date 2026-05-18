import { useEffect, useRef } from 'react'
import { useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import {
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
} from '@/atoms/dock-atoms'

const POLL_INTERVAL_MS = 30_000

export function useConnectionStatus() {
  const setInternet = useSetAtom(internetOnlineAtom)
  const setBackend = useSetAtom(backendOnlineAtom)
  const setMemu = useSetAtom(memuOnlineAtom)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    setInternet(navigator.onLine)

    const onOnline = () => setInternet(true)
    const onOffline = () => setInternet(false)
    window.addEventListener('online', onOnline)
    window.addEventListener('offline', onOffline)

    async function poll() {
      if (!navigator.onLine) return
      try {
        await invoke('get_app_health')
        setBackend(true)
      } catch {
        setBackend(false)
      }
      try {
        const result = await invoke<{ online: boolean }>('get_memu_status')
        setMemu(result.online)
      } catch {
        setMemu(false)
      }
    }

    void poll()
    timerRef.current = setInterval(poll, POLL_INTERVAL_MS)

    return () => {
      window.removeEventListener('online', onOnline)
      window.removeEventListener('offline', onOffline)
      if (timerRef.current !== null) clearInterval(timerRef.current)
    }
  }, [setInternet, setBackend, setMemu])
}
