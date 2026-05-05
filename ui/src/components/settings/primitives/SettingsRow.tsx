import { cn } from '@/lib/utils'

interface SettingsRowProps {
  label: string
  description?: string
  children: React.ReactNode
  className?: string
  vertical?: boolean
}

export function SettingsRow({ label, description, children, className, vertical = false }: SettingsRowProps) {
  return (
    <div
      className={cn(
        vertical ? 'flex flex-col gap-2' : 'flex items-center justify-between gap-4',
        'min-h-[40px] py-2',
        className
      )}
    >
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-foreground">{label}</div>
        {description && (
          <div className="text-xs text-muted-foreground mt-0.5">{description}</div>
        )}
      </div>
      <div className={cn(vertical ? 'w-full' : 'shrink-0')}>
        {children}
      </div>
    </div>
  )
}
