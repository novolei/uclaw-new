import { describe, it, expect, beforeEach } from 'vitest'
import * as React from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { ContextUsageBadge } from './ContextUsageBadge'

function renderBadge(props: Partial<React.ComponentProps<typeof ContextUsageBadge>> = {}) {
  return render(
    <ContextUsageBadge
      inputTokens={110_000}
      contextWindow={1_000_000}
      isCompacting={false}
      isProcessing={false}
      onCompact={() => {}}
      {...props}
    />,
  )
}

describe('ContextUsageBadge', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('uses backend compaction threshold when provided', () => {
    renderBadge({ compactionThresholdTokens: 128_000 })
    // 110K / 128K ≈ 86% ≥ 80% warning ratio → warning (amber) state.
    expect(screen.getByRole('button').className).toContain('text-amber-600')
  })

  it('falls back to ratio when absent', () => {
    renderBadge()
    // 110K / floor(1M × 0.775) ≈ 14% → no warning.
    expect(screen.getByRole('button').className).not.toContain('text-amber-600')
  })

  it('shows remaining tokens until auto-compaction in the details', () => {
    renderBadge({ compactionThresholdTokens: 128_000 })
    fireEvent.mouseEnter(screen.getByRole('button'))
    const label = screen.getByText('距离自动压缩')
    // 128K − 110K = 18K remaining.
    expect(label.parentElement?.textContent).toContain('18.0k')
  })
})
