import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimeProviderId,
  BrowserRuntimeProviderLane,
} from '@/lib/startup/startup-doctor'

export interface BrowserRuntimeControlCenterViewModel {
  routeSummary: {
    desiredLabel: string
    activeLabel: string
    reasonLabel: string
    primaryActionLabel: string
  }
  providerRows: BrowserRuntimeProviderRowViewModel[]
}

export interface BrowserRuntimeProviderRowViewModel {
  lane: BrowserRuntimeProviderLane
  statusLabel: string
  nextActionLabel: string
  configureMcpClickable: boolean
  canEnable: boolean
  canSetFirst: boolean
  isFirst: boolean
}

export function deriveBrowserRuntimeControlCenterViewModel(
  report?: BrowserRuntimeControlCenterReport,
): BrowserRuntimeControlCenterViewModel {
  if (!report) {
    return {
      routeSummary: {
        desiredLabel: '等待 Rust 状态',
        activeLabel: '未检查',
        reasonLabel: '等待 Browser Runtime Control Center 报告。',
        primaryActionLabel: '刷新状态',
      },
      providerRows: [],
    }
  }

  return {
    routeSummary: {
      desiredLabel: report.desiredProviderPriority.map(providerLabel).join(' > '),
      activeLabel: report.activeProviderRoute.displayName,
      reasonLabel: routeReason(report.providerLanes),
      primaryActionLabel: primaryAction(report.providerLanes),
    },
    providerRows: report.providerLanes.map((lane) => ({
      lane,
      statusLabel: laneStatusLabel(lane),
      nextActionLabel: nextActionLabel(lane.nextAction),
      configureMcpClickable:
        lane.providerId === 'browser.playwright_mcp' &&
        report.mcpIntegrationSummary.configureRouteReady,
      canEnable: lane.providerId !== 'browser.local_chromium' && !lane.enabled,
      canSetFirst: lane.providerId !== report.desiredProviderPriority[0],
      isFirst: lane.providerId === report.desiredProviderPriority[0],
    })),
  }
}

export function priorityWithProviderFirst(
  priority: BrowserRuntimeProviderId[],
  providerId: BrowserRuntimeProviderId,
): BrowserRuntimeProviderId[] {
  const next = [providerId, ...priority.filter((id) => id !== providerId)]
  for (const fallback of [
    'browser.playwright_cli',
    'browser.playwright_mcp',
    'browser.local_chromium',
  ] as BrowserRuntimeProviderId[]) {
    if (!next.includes(fallback)) next.push(fallback)
  }
  return next
}

function providerLabel(providerId: string): string {
  if (providerId === 'browser.playwright_cli') return 'Playwright CLI'
  if (providerId === 'browser.playwright_mcp') return 'Playwright MCP'
  if (providerId === 'browser.local_chromium') return 'Local Chromium'
  return providerId
}

function routeReason(lanes: BrowserRuntimeProviderLane[]): string {
  const skipped = lanes.filter(
    (lane) => lane.fallbackReason && lane.providerId !== 'browser.local_chromium',
  )
  if (skipped.length === 0) return '首选 provider 可用。'
  return skipped
    .map((lane) => `${lane.displayName}: ${fallbackLabel(lane.fallbackReason)}`)
    .join(' · ')
}

function laneStatusLabel(lane: BrowserRuntimeProviderLane): string {
  if (!lane.enabled) return 'Off'
  if (lane.routeRole === 'active') return 'Active'
  if (lane.fallbackReason === 'runtime_pack_not_ready') return 'Needs runtime pack'
  if (lane.fallbackReason === 'probe_not_passed') return 'Needs probe'
  if (!lane.routable) return 'Not routable'
  return 'Ready'
}

function fallbackLabel(reason?: string): string {
  if (reason === 'provider_disabled') return 'Off'
  if (reason === 'runtime_pack_not_ready') return 'Needs runtime pack'
  if (reason === 'probe_not_passed') return 'Needs probe'
  return reason ? reason : 'Ready'
}

function nextActionLabel(nextAction: string): string {
  if (nextAction === 'enable_mcp') return 'Enable MCP'
  if (nextAction === 'enable_provider') return 'Enable provider'
  if (nextAction === 'prepare_runtime_pack') return 'Prepare runtime pack'
  if (nextAction === 'run_probe') return 'Run probe'
  if (nextAction === 'view_details') return 'View details'
  return 'No action'
}

function primaryAction(lanes: BrowserRuntimeProviderLane[]): string {
  if (lanes.some((lane) => lane.nextAction === 'run_probe')) return 'Run probes'
  if (lanes.some((lane) => lane.nextAction === 'prepare_runtime_pack')) {
    return 'Prepare runtime pack'
  }
  return 'Refresh status'
}
