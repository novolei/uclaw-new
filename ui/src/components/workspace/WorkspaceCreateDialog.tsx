import * as React from 'react'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import * as bridge from '@/lib/tauri-bridge'

interface WorkspaceCreateDialogProps {
  open: boolean
  onClose: () => void
  onCreated: (ws: { id: string; name: string; icon: string }) => void
}

const EMOJI_CHOICES = ['📁', '💼', '🚀', '🔬', '✍️', '🎯', '🏠', '⚙️']

export function WorkspaceCreateDialog({
  open,
  onClose,
  onCreated,
}: WorkspaceCreateDialogProps): React.ReactElement {
  const [name, setName] = React.useState('')
  const [icon, setIcon] = React.useState('📁')
  const [loading, setLoading] = React.useState(false)

  const handleCreate = async () => {
    if (!name.trim()) return
    setLoading(true)
    try {
      const ws = await bridge.createWorkspace(name.trim(), undefined, icon)
      onCreated(ws)
      setName('')
      setIcon('📁')
      onClose()
    } catch (e) {
      console.error('[workspace] create failed', e)
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>New Workspace</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3 py-2">
          <div className="flex gap-2 flex-wrap">
            {EMOJI_CHOICES.map((e) => (
              <button
                key={e}
                onClick={() => setIcon(e)}
                className={`text-xl p-1 rounded ${icon === e ? 'ring-2 ring-primary' : ''}`}
              >
                {e}
              </button>
            ))}
          </div>
          <Input
            placeholder="Workspace name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
            autoFocus
          />
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button onClick={handleCreate} disabled={!name.trim() || loading}>
            {loading ? 'Creating…' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
