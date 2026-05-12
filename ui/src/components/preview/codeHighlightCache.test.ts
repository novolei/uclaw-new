import { describe, it, expect, beforeEach } from 'vitest'
import {
  cacheGet,
  cacheSet,
  cacheKey,
  shouldSkipHighlight,
  __resetCacheForTests,
  MAX_HIGHLIGHT_CHARS,
  CACHE_MAX,
} from './codeHighlightCache'

describe('codeHighlightCache', () => {
  beforeEach(() => __resetCacheForTests())

  describe('cacheKey', () => {
    it('joins gitRoot, filePath, refreshVersion with separator', () => {
      expect(cacheKey({ gitRoot: '/repo', filePath: 'a.ts', refreshVersion: 3 }))
        .toBe('/repo\0a.ts\0v3')
    })

    it('treats null gitRoot as empty string', () => {
      expect(cacheKey({ gitRoot: null, filePath: 'a.ts', refreshVersion: 0 }))
        .toBe('\0a.ts\0v0')
    })
  })

  describe('cacheGet / cacheSet', () => {
    it('returns undefined for missing key', () => {
      expect(cacheGet('missing')).toBeUndefined()
    })

    it('returns stored entry', () => {
      cacheSet('k1', { oldContent: 'a', newContent: 'b' })
      expect(cacheGet('k1')).toEqual({ oldContent: 'a', newContent: 'b' })
    })

    it('promotes entry to MRU on access', () => {
      cacheSet('k1', { oldContent: '1', newContent: '1' })
      cacheSet('k2', { oldContent: '2', newContent: '2' })
      cacheGet('k1') // promote k1
      // fill cache to evict LRU (k2)
      for (let i = 0; i < CACHE_MAX - 1; i++) {
        cacheSet(`fill-${i}`, { oldContent: '', newContent: '' })
      }
      expect(cacheGet('k1')).toBeDefined()
      expect(cacheGet('k2')).toBeUndefined()
    })

    it('evicts oldest entry when over CACHE_MAX', () => {
      for (let i = 0; i < CACHE_MAX + 5; i++) {
        cacheSet(`k-${i}`, { oldContent: String(i), newContent: '' })
      }
      // first 5 should have been evicted
      expect(cacheGet('k-0')).toBeUndefined()
      expect(cacheGet('k-4')).toBeUndefined()
      expect(cacheGet(`k-${CACHE_MAX + 4}`)).toBeDefined()
    })

    it('stores optional highlighted html / lang / theme', () => {
      cacheSet('k1', {
        oldContent: 'x',
        newContent: 'x',
        highlightedHtml: '<pre>x</pre>',
        highlightedLanguage: 'ts',
        highlightedTheme: 'github-dark',
      })
      const e = cacheGet('k1')
      expect(e?.highlightedHtml).toBe('<pre>x</pre>')
      expect(e?.highlightedLanguage).toBe('ts')
      expect(e?.highlightedTheme).toBe('github-dark')
    })
  })

  describe('shouldSkipHighlight', () => {
    it('returns false for short content', () => {
      expect(shouldSkipHighlight('a'.repeat(1000))).toBe(false)
    })

    it('returns true at threshold boundary', () => {
      expect(shouldSkipHighlight('a'.repeat(MAX_HIGHLIGHT_CHARS))).toBe(false)
      expect(shouldSkipHighlight('a'.repeat(MAX_HIGHLIGHT_CHARS + 1))).toBe(true)
    })
  })
})
