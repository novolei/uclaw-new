import * as React from 'react'
import { useAtom } from 'jotai'
import { Network } from 'lucide-react'
import { connectionsPanelOpenAtom } from '@/atoms/dock-placeholder-atoms'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'

/**
 * Placeholder panel for the dock's Connections icon. The real Connections
 * view (IM channels overview, MCP server health, integration status) is
 * scheduled for a later phase — this dialog exists so the dock icon can
 * be wired today and reveal something rather than nothing.
 */
export function ConnectionsPanel(): React.ReactElement {
  const [open, setOpen] = useAtom(connectionsPanelOpenAtom)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Network className="size-4 text-primary" />
            连接
          </DialogTitle>
          <DialogDescription>
            构建中 — 这里将展示 IM 渠道、MCP 服务器与外部集成的连接状态。
          </DialogDescription>
        </DialogHeader>
        <div className="rounded-md border border-dashed border-border/60 bg-muted/30 px-4 py-6 text-center text-xs text-muted-foreground">
          {/* Placeholder visual; replaced when the real connections view ships. */}
          🚧 即将上线
        </div>
      </DialogContent>
    </Dialog>
  )
}
