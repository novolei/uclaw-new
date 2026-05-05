import { atom } from 'jotai'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

export interface UpdateStatus {
  status: 'idle' | 'checking' | 'available' | 'not-available' | 'downloading' | 'error'
  version?: string
  releaseNotes?: string
  error?: string
  /** bytes downloaded so far */
  downloaded?: number
  /** total bytes to download (undefined if unknown) */
  contentLength?: number
}

export const updateStatusAtom = atom<UpdateStatus>({ status: 'idle' })

export const hasUpdateAtom = atom((get) => {
  const { status } = get(updateStatusAtom)
  return status === 'available'
})

export const updaterAvailableAtom = atom<boolean>(true)

export function initializeUpdater(
  _setStatus: (status: UpdateStatus) => void,
): () => void {
  return () => {}
}

/**
 * Check for updates. Returns version + body when an update is available, null otherwise.
 */
export async function checkForUpdates(): Promise<{
  version: string
  body: string
} | null> {
  const update = await check()
  if (!update?.available) return null
  return {
    version: update.version,
    body: update.body ?? '',
  }
}

/**
 * Download and install the update, then relaunch.
 * Calls onProgress(downloaded, contentLength | undefined) during download.
 */
export async function installAndRelaunch(
  onProgress?: (downloaded: number, contentLength: number | undefined) => void,
): Promise<void> {
  const update = await check()
  if (!update?.available) return

  let totalBytes: number | undefined
  await update.downloadAndInstall((event) => {
    if (event.event === 'Started') {
      totalBytes = event.data.contentLength
    } else if (event.event === 'Progress') {
      onProgress?.(event.data.chunkLength, totalBytes)
    }
  })

  await relaunch()
}
