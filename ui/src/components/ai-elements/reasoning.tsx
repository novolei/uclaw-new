// [PLACEHOLDER] ai-elements/reasoning — 推理折叠组件
import * as React from 'react'
import { ChevronRight } from 'lucide-react'

interface ReasoningProps {
  children: React.ReactNode
  isStreaming?: boolean
  defaultOpen?: boolean
}

export function Reasoning({ children, isStreaming, defaultOpen = false }: ReasoningProps): React.ReactElement {
  const [open, setOpen] = React.useState(defaultOpen)

  React.useEffect(() => {
    if (defaultOpen) setOpen(true)
  }, [defaultOpen])

  return (
    <div className="mb-2">
      {React.Children.map(children, (child) => {
        if (React.isValidElement(child)) {
          if (child.type === ReasoningTrigger) {
            return React.cloneElement(child as React.ReactElement<any>, {
              open,
              onClick: () => setOpen(!open),
              isStreaming,
            })
          }
          if (child.type === ReasoningContent) {
            return open ? child : null
          }
        }
        return child
      })}
    </div>
  )
}

export function ReasoningTrigger(props: {
  open?: boolean
  onClick?: () => void
  isStreaming?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors mb-1"
      onClick={props.onClick}
    >
      <ChevronRight className={`size-3 transition-transform ${props.open ? 'rotate-90' : ''}`} />
      <span>{props.isStreaming ? '思考中...' : '查看推理过程'}</span>
    </button>
  )
}

export function ReasoningContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <div className="ml-4 pl-3 border-l-2 border-muted text-sm text-muted-foreground whitespace-pre-wrap">
      {children}
    </div>
  )
}
