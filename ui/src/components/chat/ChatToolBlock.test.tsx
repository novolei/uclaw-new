import { describe, it, expect, vi } from 'vitest'
import { ChatToolBlock } from './ChatToolBlock'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { LiveOutput } from '@/atoms/agent-atoms'

// BashStreamView calls invoke('read_bash_log') on demand — mock to prevent JSDOM errors.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

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

  describe('live bash streaming (Seam F regression)', () => {
    const liveOutput: LiveOutput = {
      segments: [
        { stream: 'stdout', text: 'compiling…\n' },
        { stream: 'stderr', text: 'warning: unused import\n' },
      ],
      bytes: 34,
      droppedHead: false,
    }

    it('renders BashStreamView with streamed text for a running bash with liveOutput', () => {
      renderWithProviders(
        <ChatToolBlock
          toolName="bash"
          input={{ command: 'cargo build' }}
          isCompleted={false}
          liveOutput={liveOutput}
        />,
      )
      // BashStreamView renders the command label and segment text
      expect(screen.getByText(/compiling/)).toBeInTheDocument()
      expect(screen.getByText(/warning: unused import/)).toBeInTheDocument()
    })

    it('does NOT render BashStreamView when tool is done (result panel takes over)', () => {
      renderWithProviders(
        <ChatToolBlock
          toolName="bash"
          input={{ command: 'cargo build' }}
          isCompleted={true}
          result="Finished"
          liveOutput={liveOutput}
        />,
      )
      // Stream text should NOT appear — completed tool shows result panel on click instead
      expect(screen.queryByText(/compiling/)).not.toBeInTheDocument()
    })

    it('does NOT render BashStreamView for a non-bash tool with liveOutput', () => {
      renderWithProviders(
        <ChatToolBlock
          toolName="read_file"
          input={{ path: '/tmp/foo' }}
          isCompleted={false}
          liveOutput={liveOutput}
        />,
      )
      // liveOutput ignored for non-bash tools
      expect(screen.queryByText(/compiling/)).not.toBeInTheDocument()
    })

    it('does NOT render BashStreamView when liveOutput has no segments', () => {
      const emptyLive: LiveOutput = { segments: [], bytes: 0, droppedHead: false }
      renderWithProviders(
        <ChatToolBlock
          toolName="bash"
          input={{ command: 'echo hi' }}
          isCompleted={false}
          liveOutput={emptyLive}
        />,
      )
      // Pre element should not appear at all (condition guards segments.length > 0)
      expect(screen.queryByText(/\$ echo hi/)).not.toBeInTheDocument()
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
