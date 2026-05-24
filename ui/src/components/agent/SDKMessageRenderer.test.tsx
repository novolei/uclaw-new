import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import {
  MessageGroupRenderer,
  SDKMessageRenderer,
  type MessageGroup,
  type SDKMessage,
} from './SDKMessageRenderer'

type AssistantErrorMessage = Extract<MessageGroup, { type: 'assistant-turn' }>['assistantMessages'][number]

vi.mock('@/lib/tauri-bridge', () => ({
  openExternal: vi.fn(),
  readAttachment: vi.fn(),
  saveImageAs: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

function browserRuntimeErrorMessage(
  action = 'open_browser_runtime_settings',
  label = 'Open Browser Runtime Settings',
): SDKMessage {
  return {
    type: 'assistant',
    uuid: 'browser-runtime-error',
    error: { message: 'Browser runtime is unavailable' },
    message: {
      content: [
        {
          type: 'text',
          text: 'Browser runtime is unavailable. Open settings to inspect repair options.',
        },
      ],
    },
    _errorTitle: 'Browser runtime unavailable',
    _errorActions: [
      {
        action,
        label,
      },
    ],
  }
}

describe('SDKMessageRenderer browser runtime recovery actions', () => {
  it('opens Browser Runtime settings from direct assistant error recovery actions', async () => {
    const message = browserRuntimeErrorMessage()
    const { store, user } = renderWithProviders(
      <SDKMessageRenderer message={message} allMessages={[message]} />,
    )

    await user.click(screen.getByRole('button', { name: 'Open Browser Runtime Settings' }))

    expect(store.get(settingsTabAtom)).toBe('browserRuntime')
    expect(store.get(settingsOpenAtom)).toBe(true)
  })

  it('opens Browser Runtime settings from grouped assistant-turn error recovery actions', async () => {
    const message = browserRuntimeErrorMessage()
    const group: MessageGroup = {
      type: 'assistant-turn',
      assistantMessages: [message as AssistantErrorMessage],
      turnMessages: [message],
      model: 'test-model',
      createdAt: 1,
    }

    const { store, user } = renderWithProviders(
      <MessageGroupRenderer group={group} allMessages={[message]} />,
    )

    await user.click(screen.getByRole('button', { name: 'Open Browser Runtime Settings' }))

    expect(store.get(settingsTabAtom)).toBe('browserRuntime')
    expect(store.get(settingsOpenAtom)).toBe(true)
  })

  it('keeps generic settings recovery actions on the current settings tab', async () => {
    const message = browserRuntimeErrorMessage('settings', 'Open Settings')
    const { store, user } = renderWithProviders(
      <SDKMessageRenderer message={message} allMessages={[message]} />,
    )

    await user.click(screen.getByRole('button', { name: 'Open Settings' }))

    expect(store.get(settingsTabAtom)).toBe('connectivity')
    expect(store.get(settingsOpenAtom)).toBe(true)
  })
})
