import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { StartupSplash } from './StartupSplash'
import { deriveStartupDoctorViewModel, type StartupDoctorCheck } from '@/lib/startup/startup-doctor'

describe('StartupSplash', () => {
  it('renders a concise startup first frame by default', () => {
    renderWithProviders(<StartupSplash />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Startup doctor checks' })).not.toBeInTheDocument()
  })

  it('expands diagnostics on demand', async () => {
    const { user } = renderWithProviders(<StartupSplash />)

    await user.click(screen.getByRole('button', { name: /details/i }))

    expect(screen.getByRole('list', { name: 'Startup doctor checks' })).toBeInTheDocument()
    expect(screen.getByText('Browser runtime manifest')).toBeInTheDocument()
  })

  it('opens details automatically when the model recommends attention', () => {
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'browser-runtime-pack', label: 'Runtime pack path', status: 'failed', detail: 'Missing pack' },
    ]

    renderWithProviders(<StartupSplash viewModel={deriveStartupDoctorViewModel(checks)} />)

    expect(screen.getByText('Startup doctor needs attention')).toBeInTheDocument()
    expect(screen.getByRole('list', { name: 'Startup doctor checks' })).toBeInTheDocument()
    expect(screen.getByText('Missing pack')).toBeInTheDocument()
  })

  it('supports controlled details state', async () => {
    const onChange = vi.fn()
    const { user } = renderWithProviders(
      <StartupSplash detailsExpanded={false} onDetailsExpandedChange={onChange} />,
    )

    await user.click(screen.getByRole('button', { name: /details/i }))

    expect(onChange).toHaveBeenCalledWith(true)
    expect(screen.queryByRole('list', { name: 'Startup doctor checks' })).not.toBeInTheDocument()
  })
})
