import { describe, it, expect } from 'vitest'
import { visibleTabs } from './RightSidePanel'

describe('visibleTabs', () => {
  it('shows all five tabs for a human session', () => {
    expect(visibleTabs(false)).toEqual(['files', 'teams', 'plan', 'trajectory', 'browser'])
  })
  it('hides teams + browser for an automation run session', () => {
    expect(visibleTabs(true)).toEqual(['files', 'plan', 'trajectory'])
  })
})
