import { describe, expect, it } from 'vitest'
import {
  artifactLabel,
  deriveBrowserRuntimeControlCenterViewModel,
  priorityWithProviderFirst,
  rawControlCenterJson,
} from './browser-runtime-control-center'
import type { BrowserRuntimeControlCenterReport } from '@/lib/startup/startup-doctor'

function report(): BrowserRuntimeControlCenterReport {
  return {
    featureFlags: {
      playwrightCli: true,
      playwrightMcp: false,
      hostedBrowser: false,
      forceLegacyLocalChromium: false,
    },
    desiredProviderPriority: [
      'browser.playwright_cli',
      'browser.playwright_mcp',
      'browser.local_chromium',
    ],
    activeProviderRoute: {
      providerId: 'browser.local_chromium',
      displayName: 'Local Chromium',
    },
    providerLanes: [
      {
        providerId: 'browser.playwright_cli',
        displayName: 'Playwright CLI',
        enabled: true,
        priorityRank: 1,
        readiness: 'needs_setup',
        routable: false,
        routeRole: 'desired_first',
        probeState: 'not_run',
        fallbackReason: 'probe_not_passed',
        nextAction: 'run_probe',
      },
      {
        providerId: 'browser.playwright_mcp',
        displayName: 'Playwright MCP',
        enabled: false,
        priorityRank: 2,
        readiness: 'unavailable',
        routable: false,
        routeRole: 'desired',
        probeState: 'not_run',
        fallbackReason: 'provider_disabled',
        nextAction: 'enable_mcp',
      },
      {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
        enabled: true,
        priorityRank: 3,
        readiness: 'ready',
        routable: true,
        routeRole: 'active',
        probeState: 'passed',
        nextAction: 'none',
      },
    ],
    mcpIntegrationSummary: {
      builtIn: true,
      enabled: false,
      rawToolsExposed: false,
      configureRouteReady: false,
    },
    updatedAtMs: 1,
  }
}

describe('browser runtime control center view model', () => {
  it('separates desired provider priority from active route', () => {
    const model = deriveBrowserRuntimeControlCenterViewModel(report())

    expect(model.routeSummary.desiredLabel).toBe(
      'Playwright CLI > Playwright MCP > Local Chromium',
    )
    expect(model.routeSummary.activeLabel).toBe('Local Chromium')
    expect(model.routeSummary.reasonLabel).toContain('Playwright CLI')
    expect(model.providerRows[1].configureMcpClickable).toBe(false)
    expect(model.providerRows[1].canEnable).toBe(true)
  })

  it('moves a provider to the front without dropping fallback providers', () => {
    expect(
      priorityWithProviderFirst(['browser.playwright_cli'], 'browser.playwright_mcp'),
    ).toEqual([
      'browser.playwright_mcp',
      'browser.playwright_cli',
      'browser.local_chromium',
    ])
  })

  it('uses product status vocabulary instead of raw unavailable/setup copy', () => {
    const model = deriveBrowserRuntimeControlCenterViewModel(report())

    expect(model.providerRows.map((row) => row.statusLabel)).toContain('Off')
    expect(JSON.stringify(model)).not.toContain('feature flag disabled')
    expect(JSON.stringify(model)).not.toContain('setup 未完成')
  })

  it('serializes raw diagnostics and labels missing artifacts explicitly', () => {
    const json = rawControlCenterJson(report())

    expect(json).toContain('"desiredProviderPriority"')
    expect(artifactLabel(undefined)).toBe('No artifact yet')
    expect(artifactLabel('browser-runtime-provider-probe-passed')).toBe(
      'browser-runtime-provider-probe-passed',
    )
  })
})
