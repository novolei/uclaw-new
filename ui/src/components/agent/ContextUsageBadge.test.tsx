import { describe, it, expect, beforeEach } from 'vitest'
import * as React from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { ContextUsageBadge } from './ContextUsageBadge'
import { carryContextMeterFields } from '@/atoms/agent-atoms'
import type { AgentStreamState } from '@/atoms/agent-atoms'

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

describe('ContextUsageBadge — 提示词缓存', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  function rowValue(label: string): string {
    const el = screen.getByText(label)
    return el.parentElement?.textContent ?? ''
  }

  const frameProps = {
    frozenSystemPrompt: true,
    systemPromptTokens: 15_400,
    toolDefTokens: 11_800,
    toolCount: 59,
    memoryInjection: 'delta' as const,
    turnContextTokens: 900,
    sessionCacheHitRatio: 0.82,
  }

  it('renders the prompt-cache section in transcript mode', () => {
    renderBadge(frameProps)
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(screen.getByText('提示词缓存')).toBeInTheDocument()
    expect(rowValue('系统提示词')).toContain('15.4k · 已冻结')
    expect(rowValue('工具定义')).toContain('59 个 · 11.8k')
    expect(rowValue('本轮动态块')).toContain('记忆：增量')
    expect(rowValue('本会话命中率')).toContain('82%')
  })

  it('explains a frame rebuild under the system prompt row', () => {
    renderBadge({ ...frameProps, frameRebuildReason: 'workspace_changed' })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(screen.getByText('本会话已重建：工作区变化')).toBeInTheDocument()
  })

  it('does not explain a fresh frame', () => {
    renderBadge({ ...frameProps, frameRebuildReason: 'new' })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(screen.queryByText(/本会话已重建/)).toBeNull()
  })

  it('shows 未上报 when the provider never reports cache reads', () => {
    renderBadge({ ...frameProps, cacheHitRatio: 0, sessionCacheHitRatio: 0, sessionCacheUnreported: true })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(rowValue('本会话命中率')).toContain('未上报')
    expect(rowValue('本会话命中率')).not.toContain('0%')
    expect(rowValue('本次调用命中')).toContain('未上报')
  })

  it('omits the section outside transcript mode', () => {
    renderBadge({ cacheReadTokens: 50_000 })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(screen.queryByText('提示词缓存')).toBeNull()
    expect(screen.queryByText('系统提示词')).toBeNull()
  })

  it('uses the backend per-call cacheHitRatio for the hit badge', () => {
    // OpenAI-compatible wire: inputTokens already includes the 50k cache
    // reads. The old frontend formula (read / (input + read + creation))
    // would show 31%; the backend ratio is 45%.
    renderBadge({ cacheReadTokens: 50_000, cacheHitRatio: 0.4545 })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(screen.getByText('45% 命中')).toBeInTheDocument()
    expect(screen.queryByText(/31%/)).toBeNull()
  })

  it('treats a cache write as reported on a cold call (no 未上报)', () => {
    renderBadge({
      ...frameProps,
      cacheHitRatio: 0,
      cacheCreationTokens: 20_000,
      sessionCacheHitRatio: 0,
      sessionCacheUnreported: true,
    })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(rowValue('本次调用命中')).toContain('缓存已写入')
    expect(screen.queryByText('未上报')).toBeNull()
  })

  it('keeps the section across a turn start (state rebuilt with carried fields)', () => {
    const { sessionCacheHitRatio: _session, ...frameState } = frameProps
    const prev: AgentStreamState = {
      running: false, content: '', toolActivities: [], teammates: [],
      inputTokens: 110_000, contextWindow: 1_000_000,
      ...frameState, cacheHitRatio: 0.5, cacheReadTokens: 40_000,
    }
    const next = { running: true, content: '', toolActivities: [], ...carryContextMeterFields(prev) }
    renderBadge({ ...next, sessionCacheHitRatio: 0.82 })
    fireEvent.mouseEnter(screen.getByRole('button'))
    expect(rowValue('系统提示词')).toContain('15.4k · 已冻结')
    expect(rowValue('工具定义')).toContain('59 个 · 11.8k')
    expect(screen.getByText('50% 命中')).toBeInTheDocument()
  })
})
