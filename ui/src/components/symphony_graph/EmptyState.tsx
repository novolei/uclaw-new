import { useCallback, useState } from 'react'
import { ExternalLink, FileUp, Loader2, Network, Plus } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import {
  symphonyImportWorkflowMd,
  symphonySaveWorkflow,
  type SymphonySaveResult,
} from '@/lib/tauri-bridge'

import {
  SYMPHONY_TEMPLATES,
  blankTemplate,
  type MiniDag,
  type StarterTemplate,
} from './templates'

interface SymphonyEmptyStateProps {
  /**
   * Called after a workflow is successfully saved. Parent should swap the
   * current Symphony tab's sessionId to the new workflow id and refresh
   * `symphonyWorkflowsAtom`.
   */
  onCreated: (result: { workflowId: string; name: string }) => void
}

/**
 * Pure SVG render of a MiniDag at the parent's currentColor. Used in
 * the template cards to preview the DAG shape without instantiating
 * a real ReactFlow.
 */
function MiniDagPreview({ mini }: { mini: MiniDag }) {
  return (
    <svg
      viewBox={`0 0 ${mini.width} ${mini.height}`}
      className="h-12 w-full text-muted-foreground transition-colors group-hover:text-primary"
      aria-hidden
    >
      {mini.edges.map((e, i) => {
        const from = mini.nodes[e.from]
        const to = mini.nodes[e.to]
        // Curve handle: lift the control point along Y to give a gentle arc.
        const cx = (from.x + to.x) / 2
        const cy = (from.y + to.y) / 2 - 4
        return (
          <path
            key={i}
            d={`M${from.x} ${from.y} Q${cx} ${cy} ${to.x} ${to.y}`}
            stroke="currentColor"
            strokeWidth={1.25}
            fill="none"
            strokeLinecap="round"
          />
        )
      })}
      {mini.nodes.map((n, i) => (
        <circle
          key={i}
          cx={n.x}
          cy={n.y}
          r={3.5}
          fill="currentColor"
          stroke="hsl(var(--background))"
          strokeWidth={1}
        />
      ))}
    </svg>
  )
}

interface TemplateCardProps {
  template: StarterTemplate
  busy: boolean
  onPick: (t: StarterTemplate) => void
}

function TemplateCard({ template, busy, onPick }: TemplateCardProps) {
  return (
    <button
      type="button"
      disabled={busy}
      onClick={() => onPick(template)}
      className={cn(
        'group flex h-full flex-col items-stretch gap-3 rounded-lg border border-border bg-card p-4 text-left transition-all',
        'hover:-translate-y-0.5 hover:border-accent hover:bg-accent/30 hover:shadow-sm',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        'disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0',
      )}
    >
      <MiniDagPreview mini={template.miniDag} />
      <div className="space-y-1">
        <div className="text-sm font-medium text-foreground">
          {template.name}
        </div>
        <div className="text-xs leading-relaxed text-muted-foreground">
          {template.description}
        </div>
      </div>
      <div className="mt-auto text-xs text-muted-foreground/80">
        {template.def.nodes.length} node
        {template.def.nodes.length === 1 ? '' : 's'}
      </div>
    </button>
  )
}

interface ImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onImported: (result: SymphonySaveResult) => void
}

function ImportDialog({ open, onOpenChange, onImported }: ImportDialogProps) {
  const [source, setSource] = useState('')
  const [busy, setBusy] = useState(false)

  const handleImport = useCallback(async () => {
    if (!source.trim()) {
      toast.error('Paste a WORKFLOW.md document first.')
      return
    }
    setBusy(true)
    try {
      const result = await symphonyImportWorkflowMd(source)
      onImported(result)
      setSource('')
      onOpenChange(false)
    } catch (e) {
      console.error('[symphony] import failed:', e)
      toast.error(`Import failed: ${String(e)}`)
    } finally {
      setBusy(false)
    }
  }, [source, onImported, onOpenChange])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Import WORKFLOW.md</DialogTitle>
          <DialogDescription>
            Paste the YAML front matter + Markdown body. The id field
            inside the YAML becomes the workflow id.
          </DialogDescription>
        </DialogHeader>
        <Textarea
          value={source}
          onChange={(e) => setSource(e.target.value)}
          placeholder={'---\nid: my-workflow\nname: My workflow\nnodes:\n  - id: greet\n    label: Greet\n    kind: agent\n    deps: []\n---\n\n# My workflow\n'}
          rows={14}
          className="font-mono text-xs"
          disabled={busy}
        />
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            Cancel
          </Button>
          <Button onClick={handleImport} disabled={busy || !source.trim()}>
            {busy ? (
              <>
                <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                Importing…
              </>
            ) : (
              'Import workflow'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

const DOCS_URL =
  'https://github.com/novolei/uclaw-new/blob/main/docs/superpowers/specs/2026-05-17-symphony-runtime-design.md'

export function SymphonyEmptyState({ onCreated }: SymphonyEmptyStateProps) {
  const [busyTemplateId, setBusyTemplateId] = useState<string | null>(null)
  const [importOpen, setImportOpen] = useState(false)

  const createFromTemplate = useCallback(
    async (tpl: StarterTemplate) => {
      if (busyTemplateId) return
      setBusyTemplateId(tpl.id)
      try {
        const result = await symphonySaveWorkflow(tpl.def, tpl.definitionMd)
        onCreated({ workflowId: result.workflowId, name: tpl.def.name })
        toast.success(`Created "${tpl.def.name}"`)
      } catch (e) {
        console.error('[symphony] create failed:', e)
        toast.error(`Could not create workflow: ${String(e)}`)
      } finally {
        setBusyTemplateId(null)
      }
    },
    [busyTemplateId, onCreated],
  )

  const handleBlank = useCallback(() => {
    createFromTemplate(blankTemplate())
  }, [createFromTemplate])

  const handleImported = useCallback(
    (result: SymphonySaveResult) => {
      // Imported workflows use the id baked into the YAML; we let the
      // backend echo it back to us via the save result. Name resolution
      // happens lazily on the next symphonyListWorkflows refresh.
      onCreated({ workflowId: result.workflowId, name: 'Imported workflow' })
      toast.success('Workflow imported')
    },
    [onCreated],
  )

  const handleDocs = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-shell')
      await open(DOCS_URL)
    } catch (e) {
      console.error('[symphony] open docs failed:', e)
      // Fall back to opening in the in-app browser context.
      window.open(DOCS_URL, '_blank', 'noopener,noreferrer')
    }
  }, [])

  return (
    <div
      className="flex h-full w-full items-center justify-center bg-background p-8"
      data-testid="symphony-empty-state"
    >
      <div className="w-full max-w-3xl space-y-10">
        {/* Hero */}
        <div className="flex flex-col items-center text-center">
          <div
            className={cn(
              'flex h-14 w-14 items-center justify-center rounded-2xl',
              'bg-gradient-to-br from-primary/15 via-accent/30 to-primary/10',
              'text-primary shadow-sm ring-1 ring-border',
            )}
          >
            <Network className="h-7 w-7" strokeWidth={1.5} />
          </div>
          <h1 className="mt-5 text-2xl font-semibold tracking-tight text-foreground">
            Compose your first workflow
          </h1>
          <p className="mt-2 max-w-md text-sm text-muted-foreground">
            Symphony orchestrates agents as a DAG — each node is one
            agentic-loop run, edges describe handoffs, the whole graph is
            cost-bounded and recoverable.
          </p>
        </div>

        {/* Template gallery */}
        <div
          className="grid grid-cols-1 gap-4 sm:grid-cols-3"
          data-testid="symphony-template-grid"
        >
          {SYMPHONY_TEMPLATES.map((tpl) => (
            <TemplateCard
              key={tpl.id}
              template={tpl}
              busy={busyTemplateId !== null && busyTemplateId !== tpl.id}
              onPick={createFromTemplate}
            />
          ))}
        </div>

        {/* Quick actions */}
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button
            onClick={handleBlank}
            disabled={busyTemplateId !== null}
            size="sm"
          >
            {busyTemplateId === 'blank' ? (
              <>
                <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                Creating…
              </>
            ) : (
              <>
                <Plus className="mr-1.5 h-3.5 w-3.5" />
                Blank workflow
              </>
            )}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setImportOpen(true)}
            disabled={busyTemplateId !== null}
          >
            <FileUp className="mr-1.5 h-3.5 w-3.5" />
            Import .md
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleDocs}
            className="text-muted-foreground"
          >
            View docs
            <ExternalLink className="ml-1.5 h-3 w-3" />
          </Button>
        </div>
      </div>

      <ImportDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        onImported={handleImported}
      />
    </div>
  )
}
