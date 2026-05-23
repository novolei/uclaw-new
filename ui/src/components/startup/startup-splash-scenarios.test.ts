import { describe, expect, it } from 'vitest'
import {
  STARTUP_SPLASH_PREVIEW_THEMES,
  STARTUP_SPLASH_SCENARIO_IDS,
  getStartupSplashScenario,
  resolveStartupSplashPreviewOptions,
} from './startup-splash-scenarios'

describe('startup splash preview scenarios', () => {
  it('defaults to first-frame light preview without reduced motion override', () => {
    const options = resolveStartupSplashPreviewOptions('')

    expect(options.scenario.id).toBe('first-frame')
    expect(options.scenario.detailsExpanded).toBe(false)
    expect(options.scenario.viewModel.statusLine).toBe('Preparing uClaw')
    expect(options.theme).toBe('light')
    expect(options.reducedMotion).toBe(false)
  })

  it('accepts every declared scenario id and theme', () => {
    for (const scenarioId of STARTUP_SPLASH_SCENARIO_IDS) {
      const scenario = getStartupSplashScenario(scenarioId)
      expect(scenario.id).toBe(scenarioId)
      expect(scenario.viewModel.checks.length).toBeGreaterThan(0)
    }

    for (const theme of STARTUP_SPLASH_PREVIEW_THEMES) {
      expect(resolveStartupSplashPreviewOptions(`?theme=${theme}`).theme).toBe(theme)
    }
  })

  it('resolves details and reduced-motion query flags for screenshot checks', () => {
    const options = resolveStartupSplashPreviewOptions('?scenario=details&theme=qingye&motion=reduced')

    expect(options.scenario.id).toBe('details')
    expect(options.scenario.detailsExpanded).toBe(true)
    expect(options.theme).toBe('qingye')
    expect(options.reducedMotion).toBe(true)
  })

  it('models ready, deferred, and failed runtime-pack states distinctly', () => {
    expect(getStartupSplashScenario('ready').viewModel).toMatchObject({
      phase: 'ready',
      statusLine: 'uClaw is ready',
      detailsRecommended: false,
    })
    expect(getStartupSplashScenario('deferred').viewModel).toMatchObject({
      phase: 'degraded',
      detailsRecommended: true,
    })
    expect(getStartupSplashScenario('failed').viewModel).toMatchObject({
      phase: 'failed',
      detailsRecommended: true,
    })
  })
})
