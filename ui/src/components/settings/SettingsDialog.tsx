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
        {/* Overlay — gentle fade with backdrop-blur. The blur adds focus
            without dimming the rest of the chrome too aggressively. */}
        <DialogPrimitive.Overlay
          className={cn(
            "fixed inset-0 z-[100] bg-black/30 backdrop-blur-[2px]",
            "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-300 data-[state=open]:ease-out",
            "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-200 data-[state=closed]:ease-in",
          )}
        />
        {/* Content — lifts up from below on entry (slide-in-from-bottom-4)
            with a soft scale, settles into place with a longer duration
            (350ms) and a smooth-out cubic curve for the "natural
            arrival" feel. Exit is faster + slides back down. */}
        <DialogPrimitive.Content
          className={cn(
            "fixed left-[50%] top-[50%] z-[100] translate-x-[-50%] translate-y-[-50%]",
            "w-[900px] h-[600px] bg-background shadow-2xl rounded-2xl overflow-hidden",
            "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=open]:slide-in-from-bottom-4 data-[state=open]:duration-350 data-[state=open]:ease-out",
            "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=closed]:slide-out-to-bottom-2 data-[state=closed]:duration-220 data-[state=closed]:ease-in",
          )}
        >
          <DialogPrimitive.Title className="sr-only">设置</DialogPrimitive.Title>
          <SettingsPanel />
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
