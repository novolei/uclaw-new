import { describe, it, expect } from 'vitest'
import { parseAutomationChatIdentityKey } from './ChatThreadsTab'

describe('parseAutomationChatIdentityKey', () => {
  it('parses Halo-compatible app-scoped IM identities', () => {
    expect(parseAutomationChatIdentityKey('app-chat:spec-1:wechat_ilink:UIN_a')).toEqual({
      channelType: 'wechat_ilink',
      chatId: 'UIN_a',
      label: 'UIN_a',
    })
  })

  it('keeps legacy channel identities readable', () => {
    expect(parseAutomationChatIdentityKey('wechat_ilink:UIN_a')).toEqual({
      channelType: 'wechat_ilink',
      chatId: 'UIN_a',
      label: 'UIN_a',
    })
  })

  it('preserves chat ids that contain colons', () => {
    expect(parseAutomationChatIdentityKey('app-chat:spec-1:unknown:chat:with:colon')).toEqual({
      channelType: 'unknown',
      chatId: 'chat:with:colon',
      label: 'chat:with:colon',
    })
  })
})
