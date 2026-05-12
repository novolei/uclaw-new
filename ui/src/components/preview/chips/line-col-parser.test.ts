import { describe, it, expect } from 'vitest'
import { parseLineCol } from './line-col-parser'

describe('parseLineCol', () => {
  it('returns input unchanged when no :line:col suffix', () => {
    expect(parseLineCol('src/main.rs')).toEqual({ path: 'src/main.rs' })
    expect(parseLineCol('foo.ts')).toEqual({ path: 'foo.ts' })
  })

  it('strips :line', () => {
    expect(parseLineCol('src/main.rs:42')).toEqual({ path: 'src/main.rs', line: 42 })
  })

  it('strips :line:col', () => {
    expect(parseLineCol('src/main.rs:42:15')).toEqual({
      path: 'src/main.rs',
      line: 42,
      col: 15,
    })
  })

  it('rejects bogus :suffixes (non-numeric)', () => {
    expect(parseLineCol('src/main.rs:foo')).toEqual({ path: 'src/main.rs:foo' })
    expect(parseLineCol('http://example.com')).toEqual({ path: 'http://example.com' })
  })

  it('rejects negative or zero line/col', () => {
    expect(parseLineCol('src/main.rs:0')).toEqual({ path: 'src/main.rs:0' })
    expect(parseLineCol('src/main.rs:-1')).toEqual({ path: 'src/main.rs:-1' })
  })

  it('handles Windows-style paths conservatively (treats colon after drive letter as non-line)', () => {
    expect(parseLineCol('C:foo')).toEqual({ path: 'C:foo' })
  })
})
