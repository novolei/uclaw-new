import {
  DEFAULT_STARTUP_DOCTOR_CHECKS,
  deriveStartupDoctorViewModel,
  deriveStartupDoctorViewModelFromRuntimePackStatus,
  type StartupDoctorCheck,
  type StartupDoctorViewModel,
  type StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor'

export type StartupSplashScenarioId = 'first-frame' | 'details' | 'ready' | 'deferred' | 'failed'

export type StartupSplashPreviewTheme =
  | 'light'
  | 'dark'
  | 'warm-paper'
  | 'qingye'
  | 'forest-light'
  | 'forest-dark'

export interface StartupSplashScenario {
  id: StartupSplashScenarioId
  viewModel: StartupDoctorViewModel
  detailsExpanded: boolean
}

export interface StartupSplashPreviewOptions {
  scenario: StartupSplashScenario
  theme: StartupSplashPreviewTheme
  reducedMotion: boolean
}

export const STARTUP_SPLASH_SCENARIO_IDS: StartupSplashScenarioId[] = [
  'first-frame',
  'details',
  'ready',
  'deferred',
  'failed',
]

export const STARTUP_SPLASH_PREVIEW_THEMES: StartupSplashPreviewTheme[] = [
  'light',
  'dark',
  'warm-paper',
  'qingye',
  'forest-light',
  'forest-dark',
]

export function resolveStartupSplashPreviewOptions(search: string): StartupSplashPreviewOptions {
  const params = new URLSearchParams(search)
  const scenarioId = parseScenarioId(params.get('scenario'))
  const theme = parseTheme(params.get('theme'))
  const motion = params.get('motion')

  return {
    scenario: getStartupSplashScenario(scenarioId),
    theme,
    reducedMotion: motion === 'reduced',
  }
}

export function getStartupSplashScenario(id: StartupSplashScenarioId): StartupSplashScenario {
  if (id === 'details') {
    return {
      id,
      viewModel: deriveStartupDoctorViewModel(),
      detailsExpanded: true,
    }
  }

  if (id === 'ready') {
    return {
      id,
      viewModel: deriveStartupDoctorViewModelFromRuntimePackStatus(readyRuntimeReport(), passedBaseChecks()),
      detailsExpanded: false,
    }
  }

  if (id === 'deferred') {
    return {
      id,
      viewModel: deriveStartupDoctorViewModelFromRuntimePackStatus(deferredRuntimeReport(), passedBaseChecks()),
      detailsExpanded: true,
    }
  }

  if (id === 'failed') {
    return {
      id,
      viewModel: deriveStartupDoctorViewModelFromRuntimePackStatus(failedRuntimeReport(), passedBaseChecks()),
      detailsExpanded: true,
    }
  }

  return {
    id,
    viewModel: deriveStartupDoctorViewModel(),
    detailsExpanded: false,
  }
}

function parseScenarioId(value: string | null): StartupSplashScenarioId {
  return STARTUP_SPLASH_SCENARIO_IDS.includes(value as StartupSplashScenarioId)
    ? (value as StartupSplashScenarioId)
    : 'first-frame'
}

function parseTheme(value: string | null): StartupSplashPreviewTheme {
  return STARTUP_SPLASH_PREVIEW_THEMES.includes(value as StartupSplashPreviewTheme)
    ? (value as StartupSplashPreviewTheme)
    : 'light'
}

function passedBaseChecks(): StartupDoctorCheck[] {
  return DEFAULT_STARTUP_DOCTOR_CHECKS.map((check) => {
    if (
      check.id === 'network' ||
      check.id === 'browser-runtime-manifest' ||
      check.id === 'browser-runtime-pack' ||
      check.id === 'last-runtime-status'
    ) {
      return { ...check }
    }

    return { ...check, status: 'passed' }
  })
}

function readyRuntimeReport(): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: [
      'browser.runtime.manifest.checked',
      'browser.runtime.filesystem.probed',
      'browser.runtime.doctor.completed',
    ],
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Runtime pack is ready.',
      eventNames: ['browser.runtime.keep_current.ready'],
    },
  }
}

function deferredRuntimeReport(): StartupRuntimePackStatusReport {
  return {
    ...readyRuntimeReport(),
    ready: false,
    canRunBrowserTasks: false,
    primaryAction: 'retry_when_online',
    eventNames: ['browser.runtime.manifest.checked', 'browser.runtime.prepare.deferred'],
    doctor: {
      status: 'deferred',
      ready: false,
      issue: 'offline_download',
      remediation: 'Browser runtime preparation is waiting for network access.',
      actions: ['retry_when_online', 'defer'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'deferred',
      summary: 'Runtime preparation is deferred until network is available.',
      eventNames: ['browser.runtime.prepare.deferred'],
    },
  }
}

function failedRuntimeReport(): StartupRuntimePackStatusReport {
  return {
    ...readyRuntimeReport(),
    ready: false,
    canRunBrowserTasks: false,
    primaryAction: 'rollback',
    eventNames: ['browser.runtime.manifest.checked', 'browser.runtime.rollback.blocked'],
    doctor: {
      status: 'needs_repair',
      ready: false,
      issue: 'worker_startup_failure',
      remediation: 'Browser runtime worker failed to start.',
      actions: ['rollback', 'reinstall'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'blocked',
      summary: 'Rollback is blocked because no previous runtime pack exists.',
      eventNames: ['browser.runtime.rollback.blocked'],
    },
  }
}
