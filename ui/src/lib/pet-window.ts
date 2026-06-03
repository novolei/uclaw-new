import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

export const PET_WINDOW_LABEL = 'pet'

const PET_WINDOW_OPTS = {
  url: 'index.html?uclawWindow=pet',
  width: 360,
  height: 480,
  transparent: true,
  decorations: false,
  alwaysOnTop: true,
  skipTaskbar: true,
  resizable: false,
  shadow: false,
  focus: false,
} as const

export async function openPetWindow(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(PET_WINDOW_LABEL)
  if (existing) {
    await existing.show()
    await existing.setFocus().catch(() => {})
    return
  }
  // eslint-disable-next-line no-new
  new WebviewWindow(PET_WINDOW_LABEL, PET_WINDOW_OPTS)
}

export async function closePetWindow(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(PET_WINDOW_LABEL)
  await existing?.close()
}

export async function togglePetWindow(enabled: boolean): Promise<void> {
  if (enabled) await openPetWindow()
  else await closePetWindow()
}
