import { Assets, Texture } from 'pixi.js'

/**
 * Decode an animated WebP into an array of PIXI.Textures via the browser's
 * ImageDecoder API. Tauri webview (WebKit on macOS, WebView2 on Win,
 * WebKitGTK on Linux) supports ImageDecoder as of mid-2024.
 *
 * Fallback (no ImageDecoder available, e.g. jsdom): returns a single static
 * texture so callers degrade gracefully.
 */
export async function loadAnimatedWebp(url: string): Promise<Texture[]> {
  if (typeof window === 'undefined' || !('ImageDecoder' in window)) {
    const tex = await Assets.load<Texture>(url)
    return [tex]
  }

  const res = await fetch(url)
  if (!res.ok || !res.body) {
    const tex = await Assets.load<Texture>(url)
    return [tex]
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Decoder = (window as any).ImageDecoder
  const decoder = new Decoder({ data: res.body, type: 'image/webp' })
  await decoder.completed

  const track = decoder.tracks.selectedTrack
  const frameCount: number = track?.frameCount ?? 1
  const textures: Texture[] = []
  for (let i = 0; i < frameCount; i++) {
    const { image } = await decoder.decode({ frameIndex: i })
    const bitmap = await createImageBitmap(image)
    textures.push(Texture.from(bitmap))
  }
  return textures
}

const cache = new Map<string, Promise<Texture[]>>()

export function loadAnimatedWebpCached(url: string): Promise<Texture[]> {
  let p = cache.get(url)
  if (!p) {
    p = loadAnimatedWebp(url)
    cache.set(url, p)
  }
  return p
}

export function clearSpriteCache() {
  cache.clear()
}
