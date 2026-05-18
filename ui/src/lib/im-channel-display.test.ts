import { describe, expect, it } from 'vitest'
import { imChannelDisplay } from './im-channel-display'

describe('imChannelDisplay', () => {
  it('returns null for undefined/null/empty input', () => {
    expect(imChannelDisplay(undefined)).toBeNull()
    expect(imChannelDisplay(null)).toBeNull()
    expect(imChannelDisplay('')).toBeNull()
  })

  it('supplies a logoSrc for channels with bundled assets', () => {
    const wechat = imChannelDisplay('wechat_ilink')
    expect(wechat?.label).toBe('微信')
    expect(typeof wechat?.logoSrc).toBe('string')
    expect(wechat?.logoSrc).toBeTruthy()

    const feishu = imChannelDisplay('feishu')
    expect(feishu?.label).toBe('飞书')
    expect(typeof feishu?.logoSrc).toBe('string')
  })

  it('disambiguates wecom_bot from wechat_ilink by label', () => {
    const wecom = imChannelDisplay('wecom_bot')
    const wechat = imChannelDisplay('wechat_ilink')
    expect(wecom?.label).toBe('企业微信')
    expect(wechat?.label).toBe('微信')
    // Today both share the WeChat logo until a dedicated wecom asset exists,
    // but tooltips/labels keep them distinct.
    expect(wecom?.label).not.toBe(wechat?.label)
  })

  it('falls back to emoji-only display for channels without a logo asset', () => {
    const dingtalk = imChannelDisplay('dingtalk')
    expect(dingtalk?.label).toBe('钉钉')
    expect(dingtalk?.emoji).toBe('🔔')
    expect(dingtalk?.logoSrc).toBeUndefined()

    const email = imChannelDisplay('email')
    expect(email?.logoSrc).toBeUndefined()
    expect(email?.emoji).toBe('✉️')
  })

  it('falls back to a generic IM marker for unknown channels', () => {
    const unknown = imChannelDisplay('some_future_channel')
    expect(unknown).toEqual({ emoji: '📩', label: 'IM' })
    expect(unknown?.logoSrc).toBeUndefined()
  })
})
