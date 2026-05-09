import { describe, it, expect } from 'vitest'
import { ChatToolBlock } from './ChatToolBlock'
import { renderWithProviders, screen } from '@/test-utils/render'

describe('ChatToolBlock', () => {
  const baseProps = {
    toolName: 'bash',
    input: { command: 'ls -a' },
    isCompleted: true,
    animate: false,
    index: 0,
  }

  describe('completed success', () => {
    it('renders a check icon when completed without error', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ok" isError={false} />,
      )
      // The Check icon is the first svg in the row; lucide-react renders it as
      // <svg class="lucide lucide-check ..."> — assert by class fragment.
      const svgs = container.querySelectorAll('svg.lucide')
      const hasCheck = Array.from(svgs).some((s) => s.classList.contains('lucide-check'))
      expect(hasCheck).toBe(true)
    })

    it('does not render AlertTriangle for successful row', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ok" isError={false} />,
      )
      const hasTriangle = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-triangle-alert'))
      expect(hasTriangle).toBe(false)
    })
  })

  describe('completed error', () => {
    it('renders an AlertTriangle icon and tints the row', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="error output" isError={true} />,
      )
      const hasTriangle = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-triangle-alert'))
      expect(hasTriangle).toBe(true)

      // Row container should carry the destructive tint class
      const button = screen.getByRole('button')
      expect(button.className).toMatch(/bg-destructive/)
    })
  })

  describe('running', () => {
    it('renders a Loader2 spinner when not yet completed', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} isCompleted={false} />,
      )
      const hasLoader = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-loader-circle'))
      expect(hasLoader).toBe(true)

      // Button should be disabled (no result to expand yet)
      expect(screen.getByRole('button')).toBeDisabled()
    })
  })

  describe('expansion', () => {
    it('expands result panel on click when result present', async () => {
      const { user } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ls output" isError={false} />,
      )
      // Result not visible initially (expanded panel not rendered)
      expect(screen.queryByText('ls output')).not.toBeInTheDocument()

      await user.click(screen.getByRole('button'))

      // Result panel renders the content (ToolResultRenderer may format it,
      // but the raw text should appear somewhere in the DOM)
      expect(screen.getByText(/ls output/)).toBeInTheDocument()
    })

    it('button is non-clickable when no result', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} isCompleted={true} result={undefined} />,
      )
      const button = container.querySelector('button')
      expect(button).toBeDisabled()
    })
  })
})
