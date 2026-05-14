import { useEffect } from 'react'
import { useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { homeOfficeStateAtom } from '@/atoms/home-office-atoms'

const SUCCESS_LINGER_MS = 4000

export function useHomeOfficeAgentSync() {
  const setState = useSetAtom(homeOfficeStateAtom)

  useEffect(() => {
    const unlisten: Array<() => void> = []
    let successTimer: ReturnType<typeof setTimeout> | null = null

    const clearSuccessTimer = () => {
      if (successTimer) {
        clearTimeout(successTimer)
        successTimer = null
      }
    }

    listen('chat:stream-chunk', () => {
      clearSuccessTimer()
      setState('typing')
    }).then(u => unlisten.push(u))

    listen('chat:stream-tool-activity', () => {
      clearSuccessTimer()
      setState('tool_activity')
    }).then(u => unlisten.push(u))

    listen('chat:stream-complete', () => {
      clearSuccessTimer()
      setState('success')
      successTimer = setTimeout(() => {
        setState('idle')
        successTimer = null
      }, SUCCESS_LINGER_MS)
    }).then(u => unlisten.push(u))

    listen('chat:stream-error', () => {
      clearSuccessTimer()
      setState('error')
    }).then(u => unlisten.push(u))

    listen('agent:stream-reset', () => {
      clearSuccessTimer()
      setState('idle')
    }).then(u => unlisten.push(u))

    return () => {
      clearSuccessTimer()
      unlisten.forEach(u => u())
    }
  }, [setState])
}
