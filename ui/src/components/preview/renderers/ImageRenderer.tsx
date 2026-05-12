/**
 * ImageRenderer — Renders an image file via the Tauri asset:// protocol.
 *
 * Uses `convertFileSrc` (already imported elsewhere in the codebase) so
 * the URL is `asset.localhost/...` and the webview can fetch it directly
 * without round-tripping through preview_read_bytes.
 *
 * For SVG, we still use the asset URL — the file is sandboxed by the
 * `asset:` protocol scope ('**' in tauri.conf.json).
 *
 * Background uses a checkerboard pattern so transparent PNGs / SVGs are
 * visually distinct from the panel chrome. The pattern is built from
 * theme tokens so it works in all 11 themes.
 */

import * as React from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { ImageOff } from 'lucide-react'

interface ImageRendererProps {
  /** Absolute file path. */
  resolvedPath: string
  /** Display name for alt + error message. */
  name: string
}

/**
 * Checkerboard background using radial-gradient on a striped layout.
 * Tile size 16 × 16. Uses `--muted` so the contrast tracks the theme.
 */
const CHECKER_STYLE: React.CSSProperties = {
  backgroundImage:
    'linear-gradient(45deg, hsl(var(--muted)) 25%, transparent 25%), ' +
    'linear-gradient(-45deg, hsl(var(--muted)) 25%, transparent 25%), ' +
    'linear-gradient(45deg, transparent 75%, hsl(var(--muted)) 75%), ' +
    'linear-gradient(-45deg, transparent 75%, hsl(var(--muted)) 75%)',
  backgroundSize: '16px 16px',
  backgroundPosition: '0 0, 0 8px, 8px -8px, -8px 0',
}

export function ImageRenderer({ resolvedPath, name }: ImageRendererProps): React.ReactElement {
  const [errored, setErrored] = React.useState(false)
  const src = React.useMemo(() => convertFileSrc(resolvedPath), [resolvedPath])

  if (errored) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center select-none">
        <ImageOff className="size-7 text-muted-foreground/60 mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-foreground/70">无法加载图片</div>
        <div className="mt-1 text-[11px] text-muted-foreground/65 max-w-[280px] break-words">
          {name}
        </div>
      </div>
    )
  }

  return (
    <div
      className="flex items-center justify-center h-full overflow-auto p-4 bg-popover/40"
      style={CHECKER_STYLE}
    >
      <img
        src={src}
        alt={name}
        onError={() => setErrored(true)}
        className="max-w-full max-h-full object-contain rounded-md shadow-md"
        draggable={false}
      />
    </div>
  )
}
