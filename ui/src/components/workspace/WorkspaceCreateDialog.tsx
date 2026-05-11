import * as React from 'react'
import { FolderOpen } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import * as bridge from '@/lib/tauri-bridge'
import { IconPicker } from './IconPicker'
import { DEFAULT_WORKSPACE_ICON, getWorkspaceIcon } from '@/lib/workspace-icons'

interface WorkspaceCreateDialogProps {
  open: boolean
  onClose: () => void
  onCreated: (ws: { id: string; name: string; icon: string }) => void
}

/**
 * Best-effort client-side slug preview matching the backend's slugify():
 * ASCII lowercase, non-alphanumeric → '-', collapse repeats, trim, max 32.
 * Informational only — the backend's compute_workspace_dir is authoritative.
 */
function slugifyPreview(name: string): string {
  return name.toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32)
}

export function WorkspaceCreateDialog({
  open,
  onClose,
  onCreated,
}: WorkspaceCreateDialogProps): React.ReactElement {
  const [name, setName] = React.useState('')
  const [icon, setIcon] = React.useState(DEFAULT_WORKSPACE_ICON)
  const [overridePath, setOverridePath] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  // Reset all dialog state on close.
  const resetAndClose = React.useCallback(() => {
    setName('')
    setIcon(DEFAULT_WORKSPACE_ICON)
    setOverridePath(null)
    onClose()
  }, [onClose])

  const SelectedIcon = getWorkspaceIcon(icon)

  const computedPath = React.useMemo(() => {
    if (overridePath) return overridePath
    const slug = slugifyPreview(name)
    return slug ? `~/Documents/workground/${slug}` : '~/Documents/workground/...'
  }, [name, overridePath])

  const handlePickFolder = async () => {
    try {
      const picked = await bridge.openFolderDialog()
      if (picked) setOverridePath(picked.path)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`选择文件夹失败: ${msg}`)
    }
  }

  const handleCreate = async () => {
    if (!name.trim()) return
    setLoading(true)
    try {
      const ws = await bridge.createWorkspace(name.trim(), overridePath ?? undefined, icon)
      onCreated(ws)
      resetAndClose()
    } catch (e) {
      console.error('[workspace] create failed', e)
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && resetAndClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>New Workspace</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3 py-2">
          {/* Name input with the selected icon inline as a visual prefix. */}
          <div className="flex items-center gap-2">
            <div
              className="flex items-center justify-center size-8 rounded-md
                         bg-primary/15 text-primary shrink-0"
              aria-label="当前选中的图标"
            >
              <SelectedIcon className="size-4" aria-hidden />
            </div>
            <Input
              placeholder="Workspace name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
              autoFocus
              className="flex-1"
            />
          </div>

          {/* Icon picker grid. */}
          <div className="flex flex-col gap-1.5">
            <label className="text-xs text-muted-foreground">图标</label>
            <IconPicker value={icon} onChange={setIcon} />
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-xs text-muted-foreground">目录</label>
            <div className="font-mono text-xs text-muted-foreground/80 truncate" title={computedPath}>
              {computedPath}
            </div>
            <div className="flex items-center gap-2 mt-1">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handlePickFolder}
                className="text-xs h-7 gap-1.5"
              >
                <FolderOpen className="size-3" />
                选择其他位置...
              </Button>
              {overridePath && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setOverridePath(null)}
                  className="text-xs h-7 text-muted-foreground hover:text-foreground"
                >
                  清除
                </Button>
              )}
            </div>
            {!overridePath && slugifyPreview(name) === '' && name.trim() && (
              <p className="text-[10px] text-muted-foreground/70 mt-0.5">
                名称只含非 ASCII 字符,将自动生成 workspace-xxx 目录。
              </p>
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={resetAndClose}>Cancel</Button>
          <Button onClick={handleCreate} disabled={!name.trim() || loading}>
            {loading ? 'Creating…' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
