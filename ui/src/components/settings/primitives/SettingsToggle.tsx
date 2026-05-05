import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'

interface SettingsToggleProps {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  label?: string
  description?: string
  disabled?: boolean
  className?: string
}

export function SettingsToggle({
  checked,
  onCheckedChange,
  label,
  description,
  disabled = false,
  className,
}: SettingsToggleProps) {
  return (
    <div className={cn('flex items-center justify-between gap-4 py-2', className)}>
      {(label || description) && (
        <div className="flex-1 min-w-0">
          {label && <div className="text-sm font-medium text-foreground">{label}</div>}
          {description && <div className="text-xs text-muted-foreground mt-0.5">{description}</div>}
        </div>
      )}
      <Switch checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
    </div>
  )
}
