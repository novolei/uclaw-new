import { describe, it, expect } from 'vitest'
import {
  localizeEntry,
  localizeConfig,
  localizeOption,
  pickLocale,
  type SpecI18n,
} from './marketplace-i18n'

const specI18n: SpecI18n = {
  'zh-CN': {
    name: '小红书',
    description: '中文描述',
    config_schema: {
      keywords: {
        label: '监控关键词',
        options: { time_descending: '最新' },
      },
    },
  },
}

const entryI18n = {
  'zh-CN': { name: '小红书', description: '中文描述' },
}

describe('marketplace-i18n', () => {
  it('localizeEntry picks the locale name', () => {
    expect(localizeEntry('name', 'English', entryI18n, 'zh-CN')).toBe('小红书')
  })

  it('localizeEntry falls back to base when locale missing', () => {
    expect(localizeEntry('name', 'English', entryI18n, 'fr-FR')).toBe('English')
  })

  it('localizeEntry returns empty string when both missing', () => {
    expect(localizeEntry('name', undefined, {}, 'zh-CN')).toBe('')
  })

  it('pickLocale is region-tolerant — zh → zh-CN', () => {
    expect(pickLocale(specI18n, 'zh')).toEqual(specI18n['zh-CN'])
  })

  it('pickLocale handles undefined input', () => {
    expect(pickLocale(undefined, 'zh-CN')).toBeUndefined()
  })

  it('localizeConfig finds nested label', () => {
    expect(localizeConfig('keywords', 'label', 'Search', specI18n, 'zh-CN')).toBe(
      '监控关键词',
    )
  })

  it('localizeConfig falls back to base label when key unknown', () => {
    expect(localizeConfig('unknown', 'label', 'Default', specI18n, 'zh-CN')).toBe(
      'Default',
    )
  })

  it('localizeOption resolves overlay value', () => {
    expect(
      localizeOption('keywords', 'time_descending', 'Latest', specI18n, 'zh-CN'),
    ).toBe('最新')
  })

  it('localizeOption falls back to base option label', () => {
    expect(
      localizeOption('keywords', 'unknown_value', 'Most Likes', specI18n, 'zh-CN'),
    ).toBe('Most Likes')
  })
})
