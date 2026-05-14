import { describe, it, expect } from 'vitest'
import { diffBundledSkills } from './skill-diff'

describe('diffBundledSkills', () => {
  it('classifies added / removed / kept', () => {
    expect(diffBundledSkills(['a', 'b'], ['b', 'c'])).toEqual({
      added: ['c'],
      removed: ['a'],
      kept: ['b'],
    })
  })

  it('all added when nothing installed', () => {
    expect(diffBundledSkills([], ['a', 'b'])).toEqual({
      added: ['a', 'b'],
      removed: [],
      kept: [],
    })
  })

  it('all removed when new version bundles nothing', () => {
    expect(diffBundledSkills(['a', 'b'], [])).toEqual({
      added: [],
      removed: ['a', 'b'],
      kept: [],
    })
  })

  it('all kept when identical', () => {
    expect(diffBundledSkills(['a', 'b'], ['a', 'b'])).toEqual({
      added: [],
      removed: [],
      kept: ['a', 'b'],
    })
  })

  it('empty both', () => {
    expect(diffBundledSkills([], [])).toEqual({ added: [], removed: [], kept: [] })
  })
})
