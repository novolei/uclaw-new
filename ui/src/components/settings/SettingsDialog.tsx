import { useAtom } from 'jotai'
import * as DialogPrimitive from '@radix-ui/react-dialog'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import { cn } from '@/lib/utils'
import SettingsPanel from './SettingsPanel'

export function SettingsDialog() {
  const [open, setOpen] = useAtom(settingsOpenAtom)

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        {/* Overlay — pure fade with backdrop-blur for soft focus. */}
        <DialogPrimitive.Overlay
          className={cn(
            "fixed inset-0 z-[100] bg-black/30 backdrop-blur-[2px]",
            "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-220 data-[state=open]:ease-out",
            "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-160 data-[state=closed]:ease-in",
          )}
        />
        {/* Content — pure fade, no slide/scale. Matches alert-dialog +
            dialog primitives for a single coherent motion language. */}
        <DialogPrimitive.Content
          className={cn(
            "fixed left-[50%] top-[50%] z-[100] translate-x-[-50%] translate-y-[-50%]",
            "w-[900px] h-[600px] bg-background shadow-2xl rounded-2xl overflow-hidden",
            "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-220 data-[state=open]:ease-out",
            "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-160 data-[state=closed]:ease-in",
          )}
        >
          <DialogPrimitive.Title className="sr-only">设置</DialogPrimitive.Title>
          <SettingsPanel />
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
