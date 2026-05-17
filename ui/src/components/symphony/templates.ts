import type { SymphonyWorkflowDef } from '@/lib/tauri-bridge'

/**
 * A 2D node position + edge list small enough to render as a 100×50 SVG
 * preview inside a template card. Coordinates use the viewBox `0 0 100 50`.
 */
export interface MiniDag {
  width: number
  height: number
  nodes: { x: number; y: number }[]
  /** Indices into `nodes`. */
  edges: { from: number; to: number }[]
}

export interface StarterTemplate {
  id: string
  name: string
  /** One-sentence pitch shown under the card title. */
  description: string
  /** Data the canvas + run system want — the shape of an actual workflow. */
  def: SymphonyWorkflowDef
  /** Raw WORKFLOW.md (YAML front matter + body) persisted alongside `def`. */
  definitionMd: string
  /** Geometry for the SVG preview embedded in the card. */
  miniDag: MiniDag
}

const DEFAULT_RETRY = { max_attempts: 1, max_backoff_ms: null }

function defOf(
  id: string,
  name: string,
  description: string,
  nodes: SymphonyWorkflowDef['nodes'],
  edges: SymphonyWorkflowDef['edges'],
): SymphonyWorkflowDef {
  return {
    id,
    name,
    description,
    space_id: null,
    default_model: null,
    per_run_cost_cap_usd: null,
    max_concurrent_nodes: null,
    failure_mode: 'abort',
    nodes,
    edges,
  }
}

// ─── Linear chain ────────────────────────────────────────────────────────

const LINEAR_DEF = defOf(
  'tmpl-linear-chain',
  'Linear chain',
  'Three agents in series — each one builds on the previous one\'s output.',
  [
    {
      id: 'fetch',
      label: 'Fetch',
      kind: 'agent',
      prompt:
        'Gather the raw material the user described.\n\nReturn a concise summary in markdown.',
      deps: [],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'process',
      label: 'Process',
      kind: 'agent',
      prompt:
        'Process the output from the fetch step.\n\nUpstream:\n{{ upstream.fetch.output }}',
      deps: ['fetch'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'report',
      label: 'Report',
      kind: 'agent',
      prompt:
        'Write the final report based on the processed material.\n\nUpstream:\n{{ upstream.process.output }}',
      deps: ['process'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
  ],
  [],
)

const LINEAR_MD = `---
id: tmpl-linear-chain
name: Linear chain
description: Three agents in series.
failure_mode: abort
nodes:
  - id: fetch
    label: Fetch
    kind: agent
    deps: []
  - id: process
    label: Process
    kind: agent
    deps: [fetch]
  - id: report
    label: Report
    kind: agent
    deps: [process]
---

# Linear chain

A starter workflow: each node passes its output to the next.
Edit prompts, costs, and retry policy in the Design view.
`

// ─── Diamond fan-out ────────────────────────────────────────────────────

const DIAMOND_DEF = defOf(
  'tmpl-diamond-fan-out',
  'Diamond fan-out',
  'One gather step splits into two parallel analyses, then merges into a synthesis.',
  [
    {
      id: 'gather',
      label: 'Gather',
      kind: 'agent',
      prompt: 'Collect the input set.',
      deps: [],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'analyze_a',
      label: 'Analyze (A)',
      kind: 'agent',
      prompt:
        'Run analysis lens A.\n\nUpstream:\n{{ upstream.gather.output }}',
      deps: ['gather'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'analyze_b',
      label: 'Analyze (B)',
      kind: 'agent',
      prompt:
        'Run analysis lens B.\n\nUpstream:\n{{ upstream.gather.output }}',
      deps: ['gather'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'synthesize',
      label: 'Synthesize',
      kind: 'agent',
      prompt:
        'Combine the two analyses into a single recommendation.\n\nA:\n{{ upstream.analyze_a.output }}\n\nB:\n{{ upstream.analyze_b.output }}',
      deps: ['analyze_a', 'analyze_b'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
  ],
  [],
)

const DIAMOND_MD = `---
id: tmpl-diamond-fan-out
name: Diamond fan-out
description: Gather, two parallel analyses, synthesize.
failure_mode: abort
nodes:
  - id: gather
    label: Gather
    kind: agent
    deps: []
  - id: analyze_a
    label: Analyze (A)
    kind: agent
    deps: [gather]
  - id: analyze_b
    label: Analyze (B)
    kind: agent
    deps: [gather]
  - id: synthesize
    label: Synthesize
    kind: agent
    deps: [analyze_a, analyze_b]
---

# Diamond fan-out

The synthesize step receives both analyses and reconciles them.
`

// ─── Research → Draft → Review ──────────────────────────────────────────

const RESEARCH_DEF = defOf(
  'tmpl-research-draft-review',
  'Research → Draft → Review',
  'Content pipeline: research the topic, draft the artifact, critique-and-revise.',
  [
    {
      id: 'research',
      label: 'Research',
      kind: 'agent',
      prompt:
        'Research the topic the user wants written about. Pull sources, key claims, counterpoints.',
      deps: [],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'draft',
      label: 'Draft',
      kind: 'agent',
      prompt:
        'Write a draft using the research notes.\n\nResearch:\n{{ upstream.research.output }}',
      deps: ['research'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
    {
      id: 'review',
      label: 'Review',
      kind: 'agent',
      prompt:
        'Critique the draft (clarity, accuracy, structure) and emit the revised final version.\n\nDraft:\n{{ upstream.draft.output }}',
      deps: ['draft'],
      cost_cap_usd: null,
      max_iterations: null,
      retry: DEFAULT_RETRY,
      after_create_command: null,
      after_run_command: null,
      model: null,
    },
  ],
  [],
)

const RESEARCH_MD = `---
id: tmpl-research-draft-review
name: Research → Draft → Review
description: Content pipeline.
failure_mode: abort
nodes:
  - id: research
    label: Research
    kind: agent
    deps: []
  - id: draft
    label: Draft
    kind: agent
    deps: [research]
  - id: review
    label: Review
    kind: agent
    deps: [draft]
---

# Research → Draft → Review

A three-step authoring loop. The review step doubles as a self-critique
+ final revision in one pass.
`

// ─── Blank ──────────────────────────────────────────────────────────────

export function blankTemplate(): StarterTemplate {
  const id = `wf-${crypto.randomUUID().slice(0, 8)}`
  return {
    id: 'blank',
    name: 'Blank workflow',
    description: 'Start from scratch.',
    def: defOf(id, 'Untitled workflow', '', [], []),
    definitionMd: `---\nid: ${id}\nname: Untitled workflow\nfailure_mode: abort\nnodes: []\n---\n\n# Untitled workflow\n`,
    miniDag: { width: 100, height: 50, nodes: [{ x: 50, y: 25 }], edges: [] },
  }
}

// ─── Exports ────────────────────────────────────────────────────────────

export const SYMPHONY_TEMPLATES: readonly StarterTemplate[] = [
  {
    id: 'linear-chain',
    name: 'Linear chain',
    description: LINEAR_DEF.description ?? '',
    def: LINEAR_DEF,
    definitionMd: LINEAR_MD,
    miniDag: {
      width: 100,
      height: 50,
      nodes: [
        { x: 14, y: 25 },
        { x: 50, y: 25 },
        { x: 86, y: 25 },
      ],
      edges: [
        { from: 0, to: 1 },
        { from: 1, to: 2 },
      ],
    },
  },
  {
    id: 'diamond-fan-out',
    name: 'Diamond fan-out',
    description: DIAMOND_DEF.description ?? '',
    def: DIAMOND_DEF,
    definitionMd: DIAMOND_MD,
    miniDag: {
      width: 100,
      height: 50,
      nodes: [
        { x: 14, y: 25 },
        { x: 50, y: 10 },
        { x: 50, y: 40 },
        { x: 86, y: 25 },
      ],
      edges: [
        { from: 0, to: 1 },
        { from: 0, to: 2 },
        { from: 1, to: 3 },
        { from: 2, to: 3 },
      ],
    },
  },
  {
    id: 'research-draft-review',
    name: 'Research → Draft → Review',
    description: RESEARCH_DEF.description ?? '',
    def: RESEARCH_DEF,
    definitionMd: RESEARCH_MD,
    miniDag: {
      width: 100,
      height: 50,
      nodes: [
        { x: 14, y: 25 },
        { x: 50, y: 25 },
        { x: 86, y: 25 },
      ],
      edges: [
        { from: 0, to: 1 },
        { from: 1, to: 2 },
      ],
    },
  },
]
