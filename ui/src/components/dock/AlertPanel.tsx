import * as React from 'react'
import { useAtom } from 'jotai'
import { Bell } from 'lucide-react'
import { alertPanelOpenAtom } from '@/atoms/dock-placeholder-atoms'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'

/**
 * Placeholder panel for the dock's Alert icon. The real Alert center
 * (approval queue, escalation backlog, proactive notifications) is
 * scheduled for a later phase — this dialog exists so the dock icon can
 * be wired today and reveal something rather than nothing.
 */
export function AlertPanel(): React.ReactElement {
  const [open, setOpen] = useAtom(alertPanelOpenAtom)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Bell className="size-4 text-primary" />
            通知
          </DialogTitle>
          <DialogDescription>
            构建中 — 这里将聚合审批请求、人机协作升级与主动提醒。
          </DialogDescription>
        </DialogHeader>
        <div className="rounded-md border border-dashed border-border/60 bg-muted/30 px-4 py-6 text-center text-xs text-muted-foreground">
          🚧 即将上线
        </div>
      </DialogContent>
    </Dialog>
  )
}
