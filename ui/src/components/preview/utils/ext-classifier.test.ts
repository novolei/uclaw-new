import { describe, it, expect } from 'vitest'
import {
  classifyExtension,
  getExtension,
  IMAGE_EXTS,
  CODE_EXTS,
  MD_EXTS,
} from './ext-classifier'

describe('ext-classifier', () => {
  describe('getExtension', () => {
    it('returns lowercased ext without dot', () => {
      expect(getExtension('foo.TS')).toBe('ts')
      expect(getExtension('FOO.Bar.JSX')).toBe('jsx')
    })

    it('returns empty for no-extension filenames', () => {
      expect(getExtension('Makefile')).toBe('')
      expect(getExtension('LICENSE')).toBe('')
    })

    it('handles dotfiles', () => {
      expect(getExtension('.gitignore')).toBe('gitignore')
      expect(getExtension('.env')).toBe('env')
    })
  })

  describe('classifyExtension', () => {
    it('routes images to image', () => {
      expect(classifyExtension('foo.png').kind).toBe('image')
      expect(classifyExtension('foo.JPEG').kind).toBe('image')
      expect(classifyExtension('foo.svg').kind).toBe('image')
    })

    it('routes markdown to markdown', () => {
      expect(classifyExtension('readme.md').kind).toBe('markdown')
      expect(classifyExtension('notes.MARKDOWN').kind).toBe('markdown')
    })

    it('routes code by extension', () => {
      const ts = classifyExtension('a.ts')
      expect(ts.kind).toBe('code')
      expect(ts.language).toBe('ts')

      const rs = classifyExtension('a.rs')
      expect(rs.kind).toBe('code')
      expect(rs.language).toBe('rs')

      const py = classifyExtension('a.py')
      expect(py.kind).toBe('code')
      expect(py.language).toBe('py')
    })

    it('routes text-like files to code with plaintext lang', () => {
      const txt = classifyExtension('a.txt')
      expect(txt.kind).toBe('code')
      expect(txt.language).toBe('text')
    })

    it('routes unknown extensions to binary', () => {
      expect(classifyExtension('a.unknownext').kind).toBe('binary')
      expect(classifyExtension('Makefile').kind).toBe('binary')
    })

    it('exports immutable sets', () => {
      expect(IMAGE_EXTS.has('png')).toBe(true)
      expect(CODE_EXTS.has('ts')).toBe(true)
      expect(MD_EXTS.has('md')).toBe(true)
    })
  })
})
