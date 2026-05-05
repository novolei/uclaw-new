import { cn } from '@/lib/utils'

interface SettingsSectionProps {
  title?: string
  description?: string
  children: React.ReactNode
  className?: string
}

export function SettingsSection({ title, description, children, className }: SettingsSectionProps) {
  return (
    <div className={cn('space-y-4', className)}>
      {(title || description) && (
        <div className="space-y-1">
          {title && <h3 className="text-sm font-medium text-foreground">{title}</h3>}
          {description && <p className="text-xs text-muted-foreground">{description}</p>}
        </div>
      )}
      <div className="space-y-3">
        {children}
      </div>
    </div>
  )
}
