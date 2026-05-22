/**
 * Mock helpers for @tauri-apps/api so component tests don't need a running
 * Tauri runtime. Only the surface the chat layer actually uses.
 *
 * Apply at the top of a test file:
 *
 *   import { vi } from 'vitest'
 *   import { mockInvoke, mockListen } from '@/test-utils/mock-tauri'
 *
 *   vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }))
 *   vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }))
 *
 *   beforeEach(() => { mockInvoke.mockClear(); mockListen.mockClear() })
 *
 * Stub specific commands per test:
 *
 *   mockInvoke.mockImplementation((cmd, args) => {
 *     if (cmd === 'get_messages') return Promise.resolve([])
 *     return Promise.reject(new Error(`Unmocked cmd: ${cmd}`))
 *   })
 */

import { vi } from 'vitest'

/** Default invoke mock — every test should override per-command. */
export const mockInvoke = vi.fn(async (cmd: string, _args?: unknown): Promise<any> => {
  throw new Error(`mockInvoke: command ${cmd} not stubbed for this test`)
})

/** Default listen mock — returns a no-op unlisten. */
export const mockListen = vi.fn(async (_event: string, _handler: unknown) => {
  return () => {}
})

/** Reset both mocks. Call this in `beforeEach` if your test stubs them. */
export function resetTauriMocks(): void {
  mockInvoke.mockClear()
  mockListen.mockClear()
}
