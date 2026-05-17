import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '@/test-utils/render'
import { SpecList } from './SpecList'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

const makeSpec = (overrides: Partial<HumaneSpecRow> = {}): HumaneSpecRow => ({
  id: 'spec-1',
  name: 'Daily Report',
  version: '1.0',
  author: 'test',
  description: 'desc',
  systemPrompt: '',
  specFormat: 'humane-yaml-v1',
  specYaml: '',
  specJson: '{}',
  userConfigValues: '{}',
  permissionsGranted: '[]',
  permissionsDenied: '[]',
  status: 'active',
  enabled: true,
  spaceId: null,
  source: 'local',
  sourceRef: null,
  sourceVersion: null,
  createdAt: 0,
  updatedAt: 0,
  lastRunAt: null,
  lastRunOutcome: null,
  triggerPhrase: '',
  systemPromptOverride: '',
  ...overrides,
})

describe('SpecList', () => {
  it('renders spec names', () => {
    const specs = [makeSpec({ id: 's1', name: 'Daily Report' }), makeSpec({ id: 's2', name: 'Weekly Summary' })]
    renderWithProviders(<SpecList specs={specs} />)
    expect(screen.getByText('Daily Report')).toBeInTheDocument()
    expect(screen.getByText('Weekly Summary')).toBeInTheDocument()
  })

  it('calls onSelect when a spec is clicked', async () => {
    const onSelect = vi.fn()
    const specs = [makeSpec({ id: 'spec-1', name: 'Daily Report' })]
    renderWithProviders(<SpecList specs={specs} onSelect={onSelect} />)
    await userEvent.click(screen.getByText('Daily Report'))
    expect(onSelect).toHaveBeenCalledWith('spec-1')
  })

  it('highlights the selected spec', () => {
    const specs = [makeSpec({ id: 'spec-1', name: 'Daily Report' })]
    renderWithProviders(<SpecList specs={specs} selectedSpecId="spec-1" />)
    // The selected item should have a highlighted border
    const item = screen.getByRole('button', { name: /Daily Report/i })
    expect(item.className).toMatch(/border-primary|border-blue/)
  })

  it('shows empty state when no specs', () => {
    renderWithProviders(<SpecList specs={[]} />)
    expect(screen.getByText(/没有数字人/i)).toBeInTheDocument()
  })
})
