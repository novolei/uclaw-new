import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'

vi.mock('@/lib/tauri-bridge', async () => {
  return {
    createWorkspace: vi.fn().mockResolvedValue({ id: 'new-id', name: 'x', icon: 'Folder' }),
    openFolderDialog: vi.fn().mockResolvedValue({ path: '/custom/picked', name: 'picked' }),
  }
})

describe('WorkspaceCreateDialog', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('preview path follows slugified name', () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'My Project!' } })
    expect(screen.getByText(/~\/Documents\/workground\/my-project/)).toBeInTheDocument()
  })

  it('"选择其他位置..." overrides the preview path', async () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'thing' } })
    expect(screen.getByText(/workground\/thing/)).toBeInTheDocument()
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => {
      expect(screen.getByText('/custom/picked')).toBeInTheDocument()
    })
  })

  it('"清除" reverts the preview to slug', async () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'thing' } })
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => expect(screen.getByText('/custom/picked')).toBeInTheDocument())
    fireEvent.click(screen.getByText('清除'))
    expect(screen.getByText(/workground\/thing/)).toBeInTheDocument()
  })

  it('Create call passes overridePath when set, undefined when not', async () => {
    const { createWorkspace } = await import('@/lib/tauri-bridge')
    // First: no override → passes undefined
    const { unmount } = renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    fireEvent.change(screen.getByPlaceholderText('Workspace name'), { target: { value: 'plain' } })
    fireEvent.click(screen.getByText('Create'))
    await waitFor(() => {
      expect(createWorkspace).toHaveBeenCalledWith('plain', undefined, 'Folder')
    })
    unmount()
    vi.clearAllMocks()
    // Second: with override → passes picked path
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    fireEvent.change(screen.getByPlaceholderText('Workspace name'), { target: { value: 'plain' } })
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => expect(screen.getByText('/custom/picked')).toBeInTheDocument())
    fireEvent.click(screen.getByText('Create'))
    await waitFor(() => {
      expect(createWorkspace).toHaveBeenCalledWith('plain', '/custom/picked', 'Folder')
    })
  })
})
