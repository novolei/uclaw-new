import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'

const bridge = vi.hoisted(() => ({ getMemoryIndexStatus: vi.fn() }))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { MemoryIndexStatusLine } from './MemoryIndexStatusLine'

beforeEach(() => bridge.getMemoryIndexStatus.mockReset())

describe('MemoryIndexStatusLine', () => {
  it('memory_index_status_line_renders', async () => {
    bridge.getMemoryIndexStatus.mockResolvedValue({
      docs: 1234,
      lastSync: { atMs: Date.now() - 30_000, stats: { upserted: 3, deleted: 0, unchanged: 1231 } },
    })
    render(<MemoryIndexStatusLine />)
    expect(await screen.findByText(/关键词索引：1,234 条/)).toBeInTheDocument()
    expect(screen.getByText(/上次同步/)).toBeInTheDocument()
  })

  it('shows building before the first sync', async () => {
    bridge.getMemoryIndexStatus.mockResolvedValue({ docs: 0, lastSync: null })
    render(<MemoryIndexStatusLine />)
    expect(await screen.findByText(/关键词索引：正在建立/)).toBeInTheDocument()
  })
})
