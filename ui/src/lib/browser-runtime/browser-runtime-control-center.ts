import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimeProviderId,
  BrowserRuntimeProviderLane,
} from '@/lib/startup/startup-doctor'

const ALLOWED_STATUS_LABELS = [
  'Off',
  'Enabled',
  'Needs setup',
  'Needs probe',
  'Probe failed',
  'Ready',
  'Active',
  'Fallback active',
  'Advanced',
  'Not routable',
] as const

type BrowserRuntimeProductStatusLabel = (typeof ALLOWED_STATUS_LABELS)[number]

export interface BrowserRuntimeControlCenterViewModel {
  routeSummary: {
    desiredLabel: string
    activeLabel: string
    reasonLabel: string
    primaryActionLabel: string
  }
  setupSummary: {
    statusLabel: string
    detailLabel: string
    needsNode: boolean
    canAutoSetup: boolean
  }
  providerRows: BrowserRuntimeProviderRowViewModel[]
}

export function rawControlCenterJson(report?: BrowserRuntimeControlCenterReport): string {
  if (!report) return '{}'
  return JSON.stringify(report, null, 2)
}

export function artifactLabel(artifactId?: string): string {
  return artifactId ? artifactId : 'No artifact yet'
}

export interface BrowserRuntimeProviderRowViewModel {
  lane: BrowserRuntimeProviderLane
  statusLabel: string
  nextActionLabel: string
  routeHintLabel: string | null
  configureMcpClickable: boolean
  canEnable: boolean
  canSetFirst: boolean
  canRunPlaywrightSetup: boolean
  canRunProbe: boolean
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
      setupSummary: {
        statusLabel: 'Unknown',
        detailLabel: 'Waiting for Rust Browser Automation status.',
        needsNode: false,
        canAutoSetup: false,
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
    setupSummary: setupSummary(report.providerLanes),
    providerRows: report.providerLanes.map((lane) => ({
      lane,
      statusLabel: laneStatusLabel(lane),
      nextActionLabel: nextActionLabel(lane.nextAction),
      routeHintLabel: routeHintLabel(lane),
      configureMcpClickable:
        lane.providerId === 'browser.playwright_mcp' &&
        report.mcpIntegrationSummary.configureRouteReady,
      canEnable: lane.providerId !== 'browser.local_chromium' && !lane.enabled,
      canSetFirst: lane.providerId !== report.desiredProviderPriority[0],
      canRunPlaywrightSetup:
        lane.enabled &&
        lane.nextAction === 'run_playwright_setup',
      canRunProbe:
        lane.enabled &&
        lane.nextAction === 'run_probe' &&
        (lane.providerId === 'browser.playwright_cli' ||
          lane.providerId === 'browser.playwright_mcp'),
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

function laneStatusLabel(lane: BrowserRuntimeProviderLane): BrowserRuntimeProductStatusLabel {
  if (!lane.enabled) return 'Off'
  if (lane.routeRole === 'active') return 'Active'
  if (lane.fallbackReason === 'playwright_setup_not_ready') return 'Needs setup'
  if (lane.fallbackReason === 'probe_not_passed') return 'Needs probe'
  if (lane.fallbackReason === 'probe_failed') return 'Probe failed'
  if (!lane.routable) return 'Not routable'
  return 'Ready'
}

function fallbackLabel(reason?: string): BrowserRuntimeProductStatusLabel {
  if (reason === 'provider_disabled') return 'Off'
  if (reason === 'playwright_setup_not_ready') return 'Needs setup'
  if (reason === 'node_required' || reason === 'npm_required' || reason === 'npx_required') {
    return 'Needs setup'
  }
  if (reason === 'probe_not_passed') return 'Needs probe'
  if (reason === 'probe_failed') return 'Probe failed'
  if (reason) return 'Not routable'
  return 'Ready'
}

function nextActionLabel(nextAction: string): string {
  if (nextAction === 'enable_mcp') return 'Enable MCP'
  if (nextAction === 'enable_provider') return 'Enable provider'
  if (nextAction === 'run_playwright_setup') return 'Set up'
  if (nextAction === 'run_probe') return 'Run probe'
  if (nextAction === 'view_details') return 'View details'
  return 'No action'
}

function routeHintLabel(lane: BrowserRuntimeProviderLane): string | null {
  if (lane.nextAction !== 'run_probe') return null
  if (lane.providerId === 'browser.playwright_cli') {
    return 'Official Playwright CLI is installed; run the Rust adapter probe before routing browser actions.'
  }
  if (lane.providerId === 'browser.playwright_mcp') {
    return 'Built-in Playwright MCP is configured as backup; run the guarded Rust adapter probe before routing.'
  }
  return 'Probe gates require a passing Rust provider probe before routing.'
}

function primaryAction(lanes: BrowserRuntimeProviderLane[]): string {
  if (lanes.some((lane) => lane.nextAction === 'run_probe')) return 'Run probes'
  if (lanes.some((lane) => lane.nextAction === 'run_playwright_setup')) {
    return 'Set up Playwright'
  }
  return 'Refresh status'
}

function setupSummary(lanes: BrowserRuntimeProviderLane[]): BrowserRuntimeControlCenterViewModel['setupSummary'] {
  const playwrightLanes = lanes.filter((lane) =>
    lane.providerId === 'browser.playwright_cli' || lane.providerId === 'browser.playwright_mcp',
  )
  const needsNode = playwrightLanes.some((lane) =>
    lane.fallbackReason === 'node_required' ||
    lane.fallbackReason === 'npm_required' ||
    lane.fallbackReason === 'npx_required',
  )
  const needsSetup = playwrightLanes.some((lane) =>
    lane.fallbackReason === 'playwright_setup_not_ready' ||
    lane.nextAction === 'run_playwright_setup',
  )
  if (needsNode) {
    return {
      statusLabel: 'Node.js required',
      detailLabel: 'Install Node.js/npm/npx in Terminal, then run setup again.',
      needsNode: true,
      canAutoSetup: false,
    }
  }
  if (needsSetup) {
    return {
      statusLabel: 'Needs setup',
      detailLabel: 'Install official Playwright CLI globally, refresh built-in skills, and probe the built-in MCP server.',
      needsNode: false,
      canAutoSetup: true,
    }
  }
  return {
    statusLabel: 'Ready',
    detailLabel: 'Official Playwright tooling is available; browser actions route through the Rust adapter.',
    needsNode: false,
    canAutoSetup: false,
  }
}
