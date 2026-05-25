import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, within } from '@/test-utils/render'
import { StartupSplash } from './StartupSplash'
import {
  deriveStartupDoctorViewModel,
  deriveStartupDoctorViewModelFromRuntimePackStatus,
  type StartupDoctorCheck,
  type StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor'

function blockedRuntimeReport(): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: false,
    canRunBrowserTasks: false,
    primaryAction: 'repair',
    eventNames: ['browser.runtime.doctor.failed'],
    doctor: {
      status: 'needs_repair',
      ready: false,
      issue: 'missing_browser_binary',
      remediation: 'Browser runtime pack is incomplete.',
      actions: ['repair'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'blocked',
      summary: 'Repair is blocked until the runtime pack is available.',
      eventNames: ['browser.runtime.repair.blocked'],
    },
  }
}

describe('StartupSplash', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('renders a concise startup first frame by default', () => {
    renderWithProviders(<StartupSplash />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Startup doctor checks' })).not.toBeInTheDocument()
  })

  it('keeps the default startup model without a parent runtime status projection', () => {
    renderWithProviders(<StartupSplash />)

    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Startup doctor checks' })).not.toBeInTheDocument()
  })

  it('renders parent-provided Rust browser runtime status into Startup Doctor checks', () => {
    renderWithProviders(
      <StartupSplash
        viewModel={deriveStartupDoctorViewModelFromRuntimePackStatus(blockedRuntimeReport())}
        onOpenBrowserRuntimeSettings={vi.fn()}
      />,
    )

    expect(screen.getByText('Startup doctor needs attention')).toBeInTheDocument()
    expect(screen.getByRole('list', { name: 'Startup doctor checks' })).toBeInTheDocument()
    expect(screen.getByText('Browser runtime pack is incomplete.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Browser Runtime Settings' })).toBeInTheDocument()
  })

  it('renders a preview view model without owning live status reads', () => {
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'browser-runtime-pack', label: 'Runtime pack path', status: 'failed', detail: 'Missing pack' },
    ]

    renderWithProviders(<StartupSplash viewModel={deriveStartupDoctorViewModel(checks)} />)

    expect(screen.getByText('Missing pack')).toBeInTheDocument()
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

  it('shows branded recovery guidance for failed startup states', () => {
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      {
        id: 'browser-runtime-pack',
        label: 'Runtime pack path',
        status: 'failed',
        detail: 'Rollback is blocked because no previous runtime pack exists.',
      },
    ]

    renderWithProviders(<StartupSplash viewModel={deriveStartupDoctorViewModel(checks)} />)

    const recovery = screen.getByRole('status', { name: 'Startup recovery' })
    expect(recovery).toBeInTheDocument()
    expect(within(recovery).getByText('Recovery needed')).toBeInTheDocument()
    expect(within(recovery).getByText(/Rollback is blocked/)).toBeInTheDocument()
  })

  it('lets recovery guidance reveal diagnostics when details are controlled closed', async () => {
    const onChange = vi.fn()
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'network', label: 'Network availability', status: 'warning', detail: 'Waiting for network access.' },
    ]

    const { user } = renderWithProviders(
      <StartupSplash
        viewModel={deriveStartupDoctorViewModel(checks)}
        detailsExpanded={false}
        onDetailsExpandedChange={onChange}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'View diagnostics' }))

    expect(onChange).toHaveBeenCalledWith(true)
  })

  it('links browser runtime doctor attention to Browser Runtime settings', async () => {
    const onOpenBrowserRuntimeSettings = vi.fn()
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'browser-runtime-pack', label: 'Runtime pack path', status: 'failed', detail: 'Missing pack' },
    ]

    const { user } = renderWithProviders(
      <StartupSplash
        viewModel={deriveStartupDoctorViewModel(checks)}
        onOpenBrowserRuntimeSettings={onOpenBrowserRuntimeSettings}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Browser Runtime Settings' }))

    expect(onOpenBrowserRuntimeSettings).toHaveBeenCalledTimes(1)
  })

  it('hides the Browser Runtime settings link for unrelated doctor attention', () => {
    const checks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'network', label: 'Network availability', status: 'warning', detail: 'Waiting for network access.' },
    ]

    renderWithProviders(
      <StartupSplash
        viewModel={deriveStartupDoctorViewModel(checks)}
        onOpenBrowserRuntimeSettings={vi.fn()}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Browser Runtime Settings' })).not.toBeInTheDocument()
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
