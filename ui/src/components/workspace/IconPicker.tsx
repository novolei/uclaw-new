/**
 * IconPicker — grid of lucide icons for picking a workspace's icon.
 *
 * Used by WorkspaceCreateDialog (creation) and WorkspaceHeader
 * (inline icon swap during edit). Controlled component — parent owns
 * the selected icon name string.
 */

import * as React from 'react'
import { cn } from '@/lib/utils'
import { WORKSPACE_ICON_CATALOG } from '@/lib/workspace-icons'

interface IconPickerProps {
  value: string
  onChange: (iconName: string) => void
  /** Grid column count. Default 8 fits 24px buttons in ~250px width. */
  columns?: number
  className?: string
}

export function IconPicker({
  value,
  onChange,
  columns = 8,
  className,
}: IconPickerProps): React.ReactElement {
  return (
    <div
      className={cn('grid gap-1', className)}
      style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
      role="radiogroup"
      aria-label="选择工作区图标"
    >
      {WORKSPACE_ICON_CATALOG.map(({ name, component: Icon }) => {
        const selected = name === value
        return (
          <button
            key={name}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={name}
            title={name}
            onClick={() => onChange(name)}
            className={cn(
              'inline-flex items-center justify-center size-7 rounded-md',
              'transition-colors',
              selected
                ? 'bg-primary/15 text-primary ring-1 ring-primary/30'
                : 'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.05]',
            )}
          >
            <Icon className="size-4" aria-hidden />
          </button>
        )
      })}
    </div>
  )
}
