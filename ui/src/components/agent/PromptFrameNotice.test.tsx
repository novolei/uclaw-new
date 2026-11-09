import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import type { PromptFrameEvent } from '@/lib/types'
import type { AgentMessage } from '@/lib/agent-types'
import {
  appendPromptFrameEvent,
  prunePromptFrameEventsAfter,
  type PromptFrameNoticeEntry,
} from '@/atoms/agent-atoms'
import { PromptFrameNotice, anchorPromptFrameNotices, promptFrameNoticeLines } from './PromptFrameNotice'

function ev(over: Partial<PromptFrameEvent> = {}): PromptFrameEvent {
  return {
    conversationId: 's1',
    frozen: true,
    rebuilt: false,
    rebuildReason: null,
    systemPromptTokens: 4000,
    toolCount: 30,
    modeChanged: null,
    toolsAdded: [],
    toolsRemoved: [],
    memory: 'delta',
    memoryTokens: 0,
    turnContextTokens: 120,
    ...over,
  }
}

const names = (n: number, prefix = 'tool_') => Array.from({ length: n }, (_, i) => `${prefix}${i + 1}`)

describe('PromptFrameNotice', () => {
  it('summarises a large tool-set change: count + only the first 3 names', () => {
    render(<PromptFrameNotice event={ev({ toolsAdded: names(25) })} />)
    const text = screen.getByRole('note').textContent ?? ''
    expect(text).toContain('工具集已变化：新增 25 个（tool_1、tool_2、tool_3…）')
    expect(text).not.toContain('tool_4')
    expect(text).toContain('本轮缓存重建一次')
    // No removed segment when nothing was removed.
    expect(text).not.toContain('移除')
  })

  it('lists both added and removed tools', () => {
    const [line] = promptFrameNoticeLines(ev({ toolsAdded: ['a'], toolsRemoved: names(2, 'mcp_') }))
    expect(line).toBe('工具集已变化：新增 1 个（a）· 移除 2 个（mcp_1、mcp_2）—— 本轮缓存重建一次')
  })

  it('announces a permission-mode switch with its Chinese name', () => {
    render(<PromptFrameNotice event={ev({ modeChanged: { from: 'ask', to: 'plan' } })} />)
    expect(screen.getByText('权限模式已切换为「计划模式」，已告知模型')).toBeTruthy()
  })

  it("renders nothing for a first-time frame ('new') without other changes", () => {
    const { container } = render(<PromptFrameNotice event={ev({ rebuilt: true, rebuildReason: 'new' })} />)
    expect(container.innerHTML).toBe('')
  })

  it('renders nothing when nothing changed', () => {
    const { container } = render(<PromptFrameNotice event={ev()} />)
    expect(container.innerHTML).toBe('')
  })

  it('explains a rebuild caused by a prompt template change', () => {
    render(<PromptFrameNotice event={ev({ rebuilt: true, rebuildReason: 'prompt_changed' })} />)
    expect(screen.getByText('系统提示词已重建（提示词模板变化）—— 本轮缓存重建一次')).toBeTruthy()
  })

  it('ignores a stale rebuildReason when rebuilt is false', () => {
    expect(promptFrameNoticeLines(ev({ rebuilt: false, rebuildReason: 'workspace_changed' }))).toEqual([])
  })
})

function msg(id: string, role: AgentMessage['role'], createdAt: number): AgentMessage {
  return { id, role, content: id, createdAt }
}

function entry(receivedAt: number, over: Partial<PromptFrameEvent> = {}): PromptFrameNoticeEntry {
  return { ...ev({ modeChanged: { from: 'ask', to: 'yolo' }, ...over }), receivedAt }
}

describe('anchorPromptFrameNotices', () => {
  it('attaches each notice to the last user message sent before it arrived', () => {
    const messages = [msg('u1', 'user', 100), msg('a1', 'assistant', 200), msg('u2', 'user', 300), msg('a2', 'assistant', 400)]
    const e1 = entry(150)
    const e2 = entry(310)
    const anchored = anchorPromptFrameNotices(messages, [e1, e2])
    expect(anchored.get('u1')).toEqual([e1])
    expect(anchored.get('u2')).toEqual([e2])
    expect(anchored.has('a1')).toBe(false)
  })

  it('drops notices that precede every user message', () => {
    expect(anchorPromptFrameNotices([msg('u1', 'user', 500)], [entry(100)]).size).toBe(0)
  })
})

describe('promptFrameEventsAtom helpers', () => {
  it('keeps only events with a visible change, per session', () => {
    let state: Record<string, PromptFrameNoticeEntry[]> = {}
    state = appendPromptFrameEvent(state, ev({ conversationId: 's1' }), 1) // nothing visible
    state = appendPromptFrameEvent(state, ev({ conversationId: 's1', rebuilt: true, rebuildReason: 'new' }), 2)
    expect(state).toEqual({})
    state = appendPromptFrameEvent(state, ev({ conversationId: 's1', toolsAdded: ['x'] }), 3)
    state = appendPromptFrameEvent(state, ev({ conversationId: 's2', toolsRemoved: ['y'] }), 4)
    expect(state.s1.map((e) => e.receivedAt)).toEqual([3])
    expect(state.s2.map((e) => e.receivedAt)).toEqual([4])
  })

  it('prunes a session\'s notices newer than a rewind point, leaving other sessions', () => {
    const state = {
      s1: [entry(100), entry(300)],
      s2: [entry(400)],
    }
    const next = prunePromptFrameEventsAfter(state, 's1', 200)
    expect(next.s1.map((e) => e.receivedAt)).toEqual([100])
    expect(next.s2).toBe(state.s2)
    // No-op returns the same object (no re-render).
    expect(prunePromptFrameEventsAfter(next, 's1', 200)).toBe(next)
    expect(prunePromptFrameEventsAfter(next, 'missing', 0)).toBe(next)
  })
})
