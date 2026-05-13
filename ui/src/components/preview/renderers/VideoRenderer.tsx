/**
 * VideoRenderer — Plays a video file via the Tauri asset:// protocol.
 *
 * Uses `convertFileSrc` (already used by ImageRenderer) so the URL is
 * `asset.localhost/...` and the webview's <video> element can stream
 * directly without round-tripping through preview_read_bytes — important
 * because video files routinely exceed the IPC byte cap.
 *
 * Supported formats are gated by the browser engine: mp4 (h264) / webm
 * (vp8/vp9) / ogg are universal; m4v / mov work on macOS and modern
 * Chromium. avi / mkv are routed to BinaryFallback by the classifier
 * because they typically require external codecs.
 *
 * If the codec is unavailable on the host (e.g. h265 mp4 on Linux),
 * the <video> element fires `onError` and we fall back to a hint card
 * pointing the user at the OS native player via `reveal_path_in_file_manager`.
 */

import * as React from 'react'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import { FileVideo, ExternalLink } from 'lucide-react'

interface VideoRendererProps {
  resolvedPath: string
  name: string
}

export function VideoRenderer({ resolvedPath, name }: VideoRendererProps): React.ReactElement {
  const [errored, setErrored] = React.useState(false)
  const src = React.useMemo(() => convertFileSrc(resolvedPath), [resolvedPath])

  const handleOpenExternal = React.useCallback(async () => {
    try {
      await invoke('reveal_path_in_file_manager', { path: resolvedPath })
    } catch (err) {
      toast.error('无法在文件管理器中打开', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [resolvedPath])

  if (errored) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center select-none bg-popover">
        <FileVideo className="size-7 text-muted-foreground/60 mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-foreground/70">
          浏览器无法播放此视频
        </div>
        <div className="mt-1 text-[11px] text-muted-foreground/65 max-w-[320px] break-words">
          {name} — 编解码器可能不被 Webview 支持。
        </div>
        <button
          type="button"
          onClick={handleOpenExternal}
          className="mt-4 inline-flex items-center gap-1.5 h-8 px-3 rounded-md border border-border bg-foreground/[0.04] text-[12px] text-foreground/85 hover:bg-foreground/[0.08] hover:text-foreground transition-colors"
        >
          <ExternalLink className="size-3.5" />
          在外部播放器打开
        </button>
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-full overflow-auto p-6 bg-popover">
      {/* Wrapper holds the rounded clip + ring + shadow stack so the
          native <video> controls (the dark pill at the bottom + volume
          puck on top-right) inherit the rounded corners. Bg matches the
          surrounding popover so letterbox bars blend in instead of
          showing as harsh black framing. */}
      <div
        className="relative max-w-full max-h-full rounded-2xl overflow-hidden bg-popover ring-1 ring-foreground/[0.06] shadow-[0_18px_48px_-12px_rgba(0,0,0,0.35),0_8px_16px_-8px_rgba(0,0,0,0.25)]"
      >
        <video
          key={src}
          src={src}
          controls
          preload="metadata"
          onError={() => setErrored(true)}
          className="block max-w-full max-h-[78vh] outline-none"
        >
          {name}
        </video>
      </div>
    </div>
  )
}
