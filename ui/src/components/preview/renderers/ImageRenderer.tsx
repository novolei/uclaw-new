/**
 * ImageRenderer — Renders an image file via the Tauri asset:// protocol.
 *
 * Uses `convertFileSrc` (already imported elsewhere in the codebase) so
 * the URL is `asset.localhost/...` and the webview can fetch it directly
 * without round-tripping through preview_read_bytes.
 *
 * For SVG, we still use the asset URL — the file is sandboxed by the
 * `asset:` protocol scope ('**' in tauri.conf.json).
 */

import * as React from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'

interface ImageRendererProps {
  /** Absolute file path. */
  resolvedPath: string
  /** Display name for alt + error message. */
  name: string
}

export function ImageRenderer({ resolvedPath, name }: ImageRendererProps): React.ReactElement {
  const [errored, setErrored] = React.useState(false)
  const src = React.useMemo(() => convertFileSrc(resolvedPath), [resolvedPath])

  if (errored) {
    return (
      <div className="flex items-center justify-center h-full p-6 text-center text-[12px] text-muted-foreground">
        无法加载图片：{name}
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-full overflow-auto p-4 bg-muted/30">
      <img
        src={src}
        alt={name}
        onError={() => setErrored(true)}
        className="max-w-full max-h-full object-contain shadow-sm rounded-md"
        draggable={false}
      />
    </div>
  )
}
