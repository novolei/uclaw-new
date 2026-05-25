import { Badge } from '@/components/ui/badge'

interface PlaywrightMcpBuiltinCardProps {
  selected: boolean
  onClick: () => void
}

export function PlaywrightMcpBuiltinCard({
  selected,
  onClick,
}: PlaywrightMcpBuiltinCardProps) {
  return (
    <button
      type="button"
      aria-label="Open Playwright MCP integration"
      onClick={onClick}
      className={[
        'min-h-[88px] rounded-lg border p-4 text-left transition-colors',
        selected ? 'border-foreground bg-muted/60' : 'border-border bg-card hover:bg-muted/40',
      ].join(' ')}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="text-sm font-medium text-foreground">Playwright MCP</div>
        <Badge variant="secondary">Advanced</Badge>
      </div>
      <div className="mt-1 text-xs text-muted-foreground">
        Built-in browser provider · wrapped actions only
      </div>
    </button>
  )
}
