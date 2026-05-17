import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider } from 'jotai'

import { SymphonyEmptyState } from './EmptyState'
import { SYMPHONY_TEMPLATES } from './templates'

const symphonySaveWorkflow = vi.fn()
const symphonyImportWorkflowMd = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  symphonySaveWorkflow: (def: unknown, md: unknown) =>
    symphonySaveWorkflow(def, md),
  symphonyImportWorkflowMd: (source: unknown) =>
    symphonyImportWorkflowMd(source),
}))

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}))

function renderEmptyState(onCreated = vi.fn()) {
  return render(
    <Provider>
      <SymphonyEmptyState onCreated={onCreated} />
    </Provider>,
  )
}

describe('SymphonyEmptyState', () => {
  beforeEach(() => {
    symphonySaveWorkflow.mockReset()
    symphonyImportWorkflowMd.mockReset()
  })

  it('renders the hero, 3 template cards, and the 3 quick-action buttons', () => {
    renderEmptyState()
    expect(screen.getByText(/Compose your first workflow/i)).toBeTruthy()

    // Three template cards.
    for (const tpl of SYMPHONY_TEMPLATES) {
      expect(screen.getByText(tpl.name)).toBeTruthy()
    }

    // Quick action row.
    expect(screen.getByRole('button', { name: /blank workflow/i })).toBeTruthy()
    expect(screen.getByRole('button', { name: /import \.md/i })).toBeTruthy()
    expect(screen.getByRole('button', { name: /view docs/i })).toBeTruthy()
  })

  it('clicking a template invokes symphonySaveWorkflow then onCreated', async () => {
    const onCreated = vi.fn()
    symphonySaveWorkflow.mockResolvedValue({
      workflowId: 'tmpl-linear-chain',
      version: 1,
    })
    renderEmptyState(onCreated)

    fireEvent.click(screen.getByText('Linear chain'))

    await waitFor(() => {
      expect(symphonySaveWorkflow).toHaveBeenCalledTimes(1)
    })
    const [def] = symphonySaveWorkflow.mock.calls[0]!
    expect((def as { id: string }).id).toBe('tmpl-linear-chain')

    await waitFor(() => {
      expect(onCreated).toHaveBeenCalledWith({
        workflowId: 'tmpl-linear-chain',
        name: 'Linear chain',
      })
    })
  })

  it('Blank workflow creates a workflow with zero nodes', async () => {
    const onCreated = vi.fn()
    symphonySaveWorkflow.mockResolvedValue({
      workflowId: 'wf-blank',
      version: 1,
    })
    renderEmptyState(onCreated)

    fireEvent.click(screen.getByRole('button', { name: /blank workflow/i }))

    await waitFor(() => {
      expect(symphonySaveWorkflow).toHaveBeenCalledTimes(1)
    })
    const [def] = symphonySaveWorkflow.mock.calls[0]!
    expect((def as { nodes: unknown[] }).nodes).toHaveLength(0)
  })

  it('Import .md opens the import dialog with a textarea', () => {
    renderEmptyState()
    fireEvent.click(screen.getByRole('button', { name: /import \.md/i }))

    // Dialog mounts; textarea is keyed off the WORKFLOW.md placeholder text.
    const textarea = screen.getByPlaceholderText(/id: my-workflow/i)
    expect(textarea).toBeTruthy()
    expect((textarea as HTMLTextAreaElement).tagName).toBe('TEXTAREA')
  })
})
