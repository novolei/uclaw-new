import { describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  completeBrowserIdentityAuthorizationFromTab,
  completeBrowserIdentityAuthorizationFromWebview,
  listBrowserIdentities,
  revokeBrowserIdentity,
} from './tauri-bridge'
import type {
  BrowserIdentityAuthorizationReport,
  BrowserIdentityRevocationReport,
  BrowserIdentityStatusReport,
} from './tauri-bridge'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}))

describe('browser identity tauri bridge', () => {
  it('queries the dedicated browser identity status command', async () => {
    const report: BrowserIdentityStatusReport = {
      profiles: [
        {
          id: 'auth-example',
          label: 'Example',
          originPattern: 'https://*.example.com',
          kind: 'storage_state',
          provider: 'playwright',
          scope: 'global',
          createdAtMs: 1,
          lastUsedAtMs: null,
          lastVerifiedAtMs: null,
          expiresAtMs: null,
          revokedAtMs: null,
          status: 'unknown',
          revoked: false,
        },
      ],
      authorizedCount: 1,
      revokedCount: 0,
      activeTaskCount: 1,
      activeTasks: [
        {
          profileId: 'auth-example',
          runId: 'run-active',
          sessionId: 'session-active',
          task: 'Use an authorized dashboard',
          status: 'running',
          startedAtMs: 1,
          updatedAtMs: 2,
          drainDeadlineMs: null,
        },
      ],
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)

    await expect(listBrowserIdentities()).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('list_browser_identities')
  })

  it('requests browser identity revocation by profile id', async () => {
    const report: BrowserIdentityRevocationReport = {
      profile: {
        id: 'auth-example',
        label: 'Example',
        originPattern: 'https://*.example.com',
        kind: 'storage_state',
        provider: 'playwright',
        scope: 'global',
        createdAtMs: 1,
        lastUsedAtMs: null,
        lastVerifiedAtMs: null,
        expiresAtMs: null,
        revokedAtMs: 2,
        status: 'revoked',
        revoked: true,
      },
      revoked: true,
      activeTaskCount: 1,
      activeTasks: [
        {
          profileId: 'auth-example',
          runId: 'run-active',
          sessionId: 'session-active',
          task: 'Use an authorized dashboard',
          status: 'running',
          startedAtMs: 1,
          updatedAtMs: 2,
          drainDeadlineMs: 3,
        },
      ],
      drainDeadlineMs: 3,
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)

    await expect(revokeBrowserIdentity('auth-example')).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('revoke_browser_identity', {
      profileId: 'auth-example',
    })
  })

  it('completes browser identity authorization from a managed tab', async () => {
    const report: BrowserIdentityAuthorizationReport = {
      completed: true,
      profile: {
        id: 'auth-example',
        label: 'Example',
        originPattern: 'https://app.example.com',
        kind: 'storage_state',
        provider: 'playwright',
        scope: 'global',
        createdAtMs: 1,
        lastUsedAtMs: null,
        lastVerifiedAtMs: null,
        expiresAtMs: null,
        revokedAtMs: null,
        status: 'unknown',
        revoked: false,
      },
      profileId: 'auth-example',
      originPattern: 'https://app.example.com',
      capturedCookieCount: 1,
      capturedOriginCount: 1,
      message: null,
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)
    const input = {
      sessionId: 'identity-auth:example',
      tabId: 'tab-1',
      label: 'Example',
      url: 'https://app.example.com/login',
      scope: 'global' as const,
    }

    await expect(completeBrowserIdentityAuthorizationFromTab(input)).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('complete_browser_identity_authorization_from_tab', {
      input,
    })
  })

  it('completes browser identity authorization from a webview', async () => {
    const report: BrowserIdentityAuthorizationReport = {
      completed: false,
      profile: null,
      profileId: null,
      originPattern: null,
      capturedCookieCount: 0,
      capturedOriginCount: 0,
      message: 'Waiting for the site to write authenticated browser state.',
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)
    const input = {
      webviewLabel: 'identity-auth-example',
      label: 'Example',
      url: 'https://app.example.com/login',
    }

    await expect(completeBrowserIdentityAuthorizationFromWebview(input)).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('complete_browser_identity_authorization_from_webview', {
      input,
    })
  })
})
