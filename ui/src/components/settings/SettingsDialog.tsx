import { useAtom } from 'jotai'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import {
  Dialog,
  DialogContent,
} from '@/components/ui/dialog'
import SettingsPanel from './SettingsPanel'

export function SettingsDialog() {
  const [open, setOpen] = useAtom(settingsOpenAtom)

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-[900px] h-[600px] p-0 gap-0 overflow-hidden">
        <SettingsPanel />
      </DialogContent>
    </Dialog>
  )
}
