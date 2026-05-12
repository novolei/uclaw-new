/**
 * WriteApprovalDialog — modal shown when preview_write_text returns
 * NeedsApproval. Consumes the 'preview:write_approval_request' Tauri
 * event and dispatches approve_preview_write(approvalId, allowed) on
 * user decision.
 *
 * Mounted ONCE at PreviewSurface level (not per-editor) — the event
 * is global, and the dialog state is global.
 */

import * as React from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '@/components/ui/dialog'

interface ApprovalPayload {
  approvalId: string
  path: string
  reason: string
}

export function WriteApprovalDialog(): React.ReactElement {
  const [pending, setPending] = React.useState<ApprovalPayload | null>(null)

  React.useEffect(() => {
    let cancelled = false
    let unlisten: undefined | (() => void)
    void listen<ApprovalPayload>('preview:write_approval_request', (event) => {
      if (cancelled) return
      setPending(event.payload)
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  const resolve = async (allowed: boolean) => {
    if (!pending) return
    await invoke('approve_preview_write', { approvalId: pending.approvalId, allowed })
    setPending(null)
  }

  return (
    <Dialog open={pending !== null} onOpenChange={(o) => { if (!o) void resolve(false) }}>
      <DialogContent>
        <DialogTitle className="flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 text-amber-600" />
          需要批准写入
        </DialogTitle>
        <DialogDescription>{pending?.reason}</DialogDescription>
        <div className="mt-2 break-all rounded bg-muted p-2 font-mono text-[11px]">
          {pending?.path}
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => void resolve(false)}
            className={cn(
              'rounded-md border border-border bg-popover px-3 py-1.5 text-[12px]',
              'hover:bg-accent hover:text-accent-foreground transition-colors',
            )}
          >
            拒绝
          </button>
          <button
            type="button"
            onClick={() => void resolve(true)}
            className={cn(
              'rounded-md bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground',
              'hover:opacity-90 transition-opacity',
            )}
          >
            允许
          </button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
