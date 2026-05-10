import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { PromptsSettings } from './PromptsSettings'
import { renderWithProviders, screen, waitFor, fireEvent } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  readWorkspaceUclawMd: vi.fn(async () => '# my project\nuse rust 2021'),
  writeWorkspaceUclawMd: vi.fn(async () => {}),
  readDefaultPrompts: vi.fn(async () => ({
    baseline: 'BASELINE_TEXT',
    modeAsk: 'ASK_TEXT',
    modeAcceptEdits: 'ACCEPT_EDITS_TEXT',
    modePlan: 'PLAN_TEXT',
    modeBypass: 'BYPASS_TEXT',
  })),
}))

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

describe('PromptsSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('loads existing uclaw.md into the textarea', async () => {
    renderWithProviders(<PromptsSettings />)
    await waitFor(() => {
      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement
      expect(textarea.value).toContain('# my project')
    })
  })

  it('Save button calls writeWorkspaceUclawMd with edited content', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    const { user } = renderWithProviders(<PromptsSettings />)
    const textarea = await waitFor(() => {
      const el = screen.getByRole('textbox') as HTMLTextAreaElement
      if (!el.value.includes('# my project')) throw new Error('not loaded')
      return el
    })
    // Replace content (fireEvent.change avoids the placeholder fallback issue
    // that happens when user.clear sets content to '' before user.type runs)
    fireEvent.change(textarea, { target: { value: '# edited content' } })
    const save = screen.getByRole('button', { name: /保存/ })
    await user.click(save)
    await waitFor(() => {
      expect(bridge.writeWorkspaceUclawMd).toHaveBeenCalledWith('# edited content')
    })
  })

  it('expanding 内置行为护栏 shows baseline + mode prompt', async () => {
    const { user } = renderWithProviders(<PromptsSettings />)
    await waitFor(() => screen.getByText(/内置行为护栏/i))
    const toggle = screen.getByText(/内置行为护栏/i)
    await user.click(toggle)
    await waitFor(() => {
      expect(screen.getByText('BASELINE_TEXT')).toBeInTheDocument()
    })
  })
})
