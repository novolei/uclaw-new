/**
 * FirstRunDialog — Elegant first-click model-download flow.
 *
 * Spec section 7 (深度优化 #5). Three states inline:
 *   1. invite      — explain SenseVoice + ~230MB cost + start button
 *   2. downloading — per-file progress, mirror fallback indicator, cancel option
 *   3. ready       — 3s auto-countdown to recording, "立即开始" / "取消"
 *
 * Trigger: SpeechButton calls `onShowDownloadDialog` when modelStatus !== 'ready'.
 * After ready, the caller's `onReady` is invoked — that's where the recording starts.
 */
import * as React from 'react'
import { useAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Mic, CheckCircle2, X, Loader2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { modelStatusAtom } from '@/atoms/stt-atoms'

interface FirstRunDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Called once the model is ready AND the user opts to start recording. */
  onReady: () => void
}

interface DownloadProgressPayload {
  file: string
  downloaded: number
  total: number | null
  percent: number
  source?: string
}

const AUTO_START_COUNTDOWN_MS = 3000

export function FirstRunDialog({
  open,
  onOpenChange,
  onReady,
}: FirstRunDialogProps): React.ReactElement {
  const [modelStatus, setModelStatus] = useAtom(modelStatusAtom)
  const [activeMirror, setActiveMirror] = React.useState<'hf' | 'mirror' | null>(null)
  const [countdown, setCountdown] = React.useState<number>(0)

  // Listen for download progress events from backend.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    void listen<DownloadProgressPayload>(
      'stt:openflow-download-progress',
      (event) => {
        const p = event.payload
        setModelStatus({
          kind: 'downloading',
          file: p.file,
          downloaded: p.downloaded,
          total: p.total,
          percent: p.percent < 0 ? 0 : p.percent,
        })
        if (p.source === 'mirror') setActiveMirror('mirror')
        else if (p.source === 'hf') setActiveMirror('hf')
      },
    ).then((u) => {
      unlisten = u
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [setModelStatus])

  // 3-second auto-start countdown once ready.
  React.useEffect(() => {
    if (modelStatus.kind !== 'ready' || !open) {
      setCountdown(0)
      return
    }
    setCountdown(Math.ceil(AUTO_START_COUNTDOWN_MS / 1000))
    const start = Date.now()
    const tick = setInterval(() => {
      const left = Math.ceil((AUTO_START_COUNTDOWN_MS - (Date.now() - start)) / 1000)
      if (left <= 0) {
        clearInterval(tick)
        setCountdown(0)
        onReady()
        onOpenChange(false)
      } else {
        setCountdown(left)
      }
    }, 250)
    return () => clearInterval(tick)
  }, [modelStatus.kind, open, onReady, onOpenChange])

  const handleStartDownload = React.useCallback(async () => {
    setModelStatus({
      kind: 'downloading',
      file: 'model_quant.onnx',
      downloaded: 0,
      total: null,
      percent: 0,
    })
    try {
      const dir = (await invoke('stt_download_model', {
        request: { preset: 'quantized', force: false },
      })) as string
      setModelStatus({ kind: 'ready', modelDir: dir })
    } catch (e) {
      setModelStatus({
        kind: 'error',
        message: String((e as Error)?.message ?? e),
      })
    }
  }, [setModelStatus])

  const handleImmediateStart = React.useCallback(() => {
    onReady()
    onOpenChange(false)
  }, [onReady, onOpenChange])

  const handleClose = React.useCallback(() => {
    onOpenChange(false)
  }, [onOpenChange])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[420px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Mic className="size-4" /> 启用语音输入
          </DialogTitle>
          <DialogDescription>
            {modelStatus.kind === 'downloading'
              ? '模型下载中，下载完成后将自动开始录音'
              : modelStatus.kind !== 'ready'
                ? 'SenseVoice — 完全离线 · 5 种语言 · 自动标点'
                : undefined}
          </DialogDescription>
        </DialogHeader>

        {modelStatus.kind !== 'downloading' && modelStatus.kind !== 'ready' && (
          <div className="space-y-3 py-2">
            <ul className="space-y-1 text-sm text-muted-foreground">
              <li>· 完全离线 — 录音永不离开你的设备</li>
              <li>· 5 种语言 — 中 / 英 / 粤 / 日 / 韩 + 自动</li>
              <li>· 自动标点 — 输出可直接发送</li>
            </ul>
            <div className="rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
              首次需下载 <span className="font-medium text-foreground">~230MB</span>{' '}
              模型 · 一次性 · 永久离线
              <br />
              来源：HuggingFace（含 hf-mirror 国内镜像自动 fallback）
            </div>
          </div>
        )}

        {modelStatus.kind === 'downloading' && (
          <div className="space-y-3 py-2">
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              下载中{' '}
              {activeMirror === 'mirror'
                ? '· 已切到 hf-mirror 国内镜像'
                : activeMirror === 'hf'
                  ? '· HuggingFace'
                  : ''}
            </div>
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs">
                <span className="font-mono">{modelStatus.file}</span>
                <span className="tabular-nums text-muted-foreground">
                  {modelStatus.percent}%
                  {modelStatus.total !== null && (
                    <>
                      {' · '}
                      {(modelStatus.downloaded / 1024 / 1024).toFixed(0)}/
                      {(modelStatus.total / 1024 / 1024).toFixed(0)}MB
                    </>
                  )}
                </span>
              </div>
              <div className="h-1.5 rounded-full bg-muted overflow-hidden">
                <div
                  className="h-full bg-primary transition-all duration-200"
                  style={{ width: `${modelStatus.percent}%` }}
                />
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              下载完成后将自动开始录音
            </p>
          </div>
        )}

        {modelStatus.kind === 'ready' && (
          <div className="space-y-3 py-4 text-center">
            <CheckCircle2 className="size-8 text-primary mx-auto" />
            <p className="text-sm font-medium">模型已就绪</p>
            {countdown > 0 && (
              <p className="text-xs text-muted-foreground">
                {countdown} 秒后自动开始录音…
              </p>
            )}
          </div>
        )}

        <DialogFooter>
          {modelStatus.kind !== 'downloading' && modelStatus.kind !== 'ready' && (
            <>
              <Button variant="ghost" onClick={handleClose}>
                稍后
              </Button>
              <Button onClick={handleStartDownload}>开始下载并录音</Button>
            </>
          )}
          {modelStatus.kind === 'downloading' && (
            <>
              <Button variant="ghost" onClick={handleClose}>
                后台继续
              </Button>
              <Button variant="ghost" onClick={handleClose}>
                <X className="size-3 mr-1" /> 取消
              </Button>
            </>
          )}
          {modelStatus.kind === 'ready' && (
            <>
              <Button variant="ghost" onClick={handleClose}>
                取消
              </Button>
              <Button onClick={handleImmediateStart}>立即开始</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
