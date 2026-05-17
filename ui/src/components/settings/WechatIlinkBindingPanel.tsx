import { useState, useEffect, useRef, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import QRCode from 'qrcode'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'

type BindState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'qr-shown'; qrcode: string }
  | { kind: 'scanning'; qrcode: string }
  | { kind: 'confirmed' }
  | { kind: 'qr-expired' }
  | { kind: 'error'; message: string }

interface Props {
  instanceId: string
  accountId?: string
  status: ImChannelStatus | undefined
  onSaved: () => void
  onDisconnect: () => void
}

export function WechatIlinkBindingPanel({
  instanceId, accountId, status, onSaved, onDisconnect,
}: Props) {
  const [bindState, setBindState] = useState<BindState>(
    accountId ? { kind: 'confirmed' } : { kind: 'idle' }
  )
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const pollStartRef = useRef<number>(0)

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => () => { stopPolling() }, [stopPolling])

  // Render QR canvas whenever qr-shown or scanning state is entered
  useEffect(() => {
    if (
      (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') &&
      canvasRef.current
    ) {
      QRCode.toCanvas(canvasRef.current, bindState.qrcode, { width: 128 }).catch(() => {})
    }
  }, [bindState])

  // Auto-trigger QR fetch on iLink session expiry (-14)
  useEffect(() => {
    if (status?.state === 'needs_rebind') {
      fetchQr()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status?.state])

  async function fetchQr() {
    stopPolling()
    setBindState({ kind: 'loading' })
    try {
      const result = await invoke<{ qrcode: string }>(
        'request_wechat_ilink_qrcode',
        { instanceId }
      )
      setBindState({ kind: 'qr-shown', qrcode: result.qrcode })
      startPolling(result.qrcode)
    } catch (e) {
      setBindState({ kind: 'error', message: String(e) })
    }
  }

  function startPolling(qrcode: string) {
    stopPolling()
    pollStartRef.current = Date.now()
    pollRef.current = setInterval(async () => {
      if (Date.now() - pollStartRef.current > 120_000) {
        stopPolling()
        setBindState({ kind: 'qr-expired' })
        return
      }
      try {
        const result = await invoke<{
          status: string
          bot_token?: string
          account_id?: string
        }>('poll_wechat_ilink_qrcode_status', { instanceId, qrcode })

        if (result.status === 'scaned') {
          setBindState({ kind: 'scanning', qrcode })
        } else if (result.status === 'confirmed' && result.bot_token && result.account_id) {
          stopPolling()
          await saveToken(result.bot_token, result.account_id, qrcode)
        } else if (result.status === 'expired') {
          stopPolling()
          setBindState({ kind: 'qr-expired' })
        }
      } catch {
        // Network error during poll — keep retrying
      }
    }, 2000)
  }

  async function saveToken(botToken: string, accId: string, qrcode: string) {
    try {
      await invoke('save_wechat_ilink_token', {
        instanceId,
        botToken,
        accountId: accId,
      })
      setBindState({ kind: 'confirmed' })
      onSaved()
    } catch (e) {
      toast.error('保存绑定信息失败：' + String(e))
      setBindState({ kind: 'qr-shown', qrcode })
      startPolling(qrcode)
    }
  }

  async function handleDisconnect() {
    stopPolling()
    try {
      await invoke('disconnect_wechat_ilink', { instanceId })
      setBindState({ kind: 'idle' })
      onDisconnect()
    } catch (e) {
      toast.error('断开失败：' + String(e))
    }
  }

  if (bindState.kind === 'idle') {
    return (
      <div className="flex flex-col items-center gap-3 py-4">
        <p className="text-xs text-muted-foreground text-center">
          扫描二维码将此渠道与您的微信账号绑定，即可收发消息
        </p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          获取二维码
        </button>
      </div>
    )
  }

  if (bindState.kind === 'loading') {
    return (
      <div className="flex items-center justify-center py-8">
        <span className="text-sm text-muted-foreground">正在获取二维码…</span>
      </div>
    )
  }

  if (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') {
    return (
      <div className="flex flex-col items-center gap-2 py-3">
        <canvas ref={canvasRef} width={128} height={128} className="rounded border border-border" />
        <p className="text-xs text-muted-foreground">
          {bindState.kind === 'scanning' ? '已扫码，等待确认…' : '用微信扫码绑定账号'}
        </p>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={fetchQr}
            className="text-xs text-muted-foreground hover:underline"
          >
            刷新二维码
          </button>
          <span className="text-xs text-muted-foreground">·</span>
          <button
            type="button"
            onClick={() => { stopPolling(); setBindState({ kind: 'idle' }) }}
            className="text-xs text-muted-foreground hover:underline"
          >
            取消
          </button>
        </div>
      </div>
    )
  }

  if (bindState.kind === 'qr-expired') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-amber-500">二维码已过期</p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          重新获取
        </button>
      </div>
    )
  }

  if (bindState.kind === 'error') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-destructive text-center">{bindState.message}</p>
        <button
          type="button"
          onClick={() => setBindState({ kind: 'idle' })}
          className="text-xs text-muted-foreground hover:underline"
        >
          重试
        </button>
      </div>
    )
  }

  // confirmed
  return (
    <div className="rounded border border-success/30 bg-success/5 p-3 space-y-2">
      <div className="flex items-center gap-2">
        <span className="w-2 h-2 rounded-full bg-success flex-shrink-0" />
        <span className="text-xs font-medium text-success">已绑定</span>
      </div>
      {accountId && (
        <p className="text-xs text-muted-foreground">账号: {accountId}</p>
      )}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          onClick={fetchQr}
          className="text-xs text-muted-foreground hover:underline"
        >
          重新绑定
        </button>
        <span className="text-xs text-muted-foreground">·</span>
        <button
          type="button"
          onClick={handleDisconnect}
          className="text-xs text-destructive hover:underline"
        >
          断开连接
        </button>
      </div>
    </div>
  )
}
