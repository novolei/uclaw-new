import { cn } from '@/lib/utils'

interface SettingsCardProps {
  children: React.ReactNode
  className?: string
}

export function SettingsCard({ children, className }: SettingsCardProps) {
  return (
    <div
      className={cn(
        'rounded-xl border border-border bg-card p-4',
        className
      )}
    >
      {children}
    </div>
  )
}
