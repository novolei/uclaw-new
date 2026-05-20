import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { FeedPanel } from './FeedPanel'

const openFileDialog = vi.fn().mockResolvedValue(['/tmp/a.pdf', '/tmp/b.md'])
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (opts: unknown) => openFileDialog(opts) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
const ingestUrl = vi.fn().mockResolvedValue('job-1')
const ingestFiles = vi.fn().mockResolvedValue(['job-2'])
vi.mock('@/lib/ingestion', () => ({ ingestUrl: (u: string) => ingestUrl(u), ingestFiles: (p: string[]) => ingestFiles(p) }))
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), message: vi.fn() } }))

describe('FeedPanel', () => {
  beforeEach(() => { ingestUrl.mockClear(); ingestFiles.mockClear(); openFileDialog.mockClear() })

  it('submits a URL', async () => {
    render(<FeedPanel onClose={() => {}} />)
    fireEvent.change(screen.getByPlaceholderText('或粘贴一个 URL'), { target: { value: 'https://x.com' } })
    fireEvent.click(screen.getByText('摄入'))
    await waitFor(() => expect(ingestUrl).toHaveBeenCalledWith('https://x.com'))
  })

  it('picks files and ingests their paths', async () => {
    render(<FeedPanel onClose={() => {}} />)
    fireEvent.click(screen.getByText(/点击选择/))
    await waitFor(() => expect(ingestFiles).toHaveBeenCalledWith(['/tmp/a.pdf', '/tmp/b.md']))
  })

  it('renders the picker affordance', () => {
    render(<FeedPanel onClose={() => {}} />)
    expect(screen.getByText(/点击选择/)).toBeInTheDocument()
  })
})
