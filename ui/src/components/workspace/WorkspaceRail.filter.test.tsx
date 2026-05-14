import { describe, it, expect } from 'vitest'
import { isAutomationSession } from './WorkspaceRail'

describe('isAutomationSession', () => {
  it('detects an automation-origin session from metadataJson', () => {
    expect(isAutomationSession({ metadataJson: '{"origin":"automation:schedule"}' })).toBe(true)
  })
  it('returns false for a human session', () => {
    expect(isAutomationSession({ metadataJson: '{"origin":"human"}' })).toBe(false)
  })
  it('returns false when metadataJson is missing or unparseable', () => {
    expect(isAutomationSession({ metadataJson: null })).toBe(false)
    expect(isAutomationSession({ metadataJson: 'not json' })).toBe(false)
    expect(isAutomationSession({})).toBe(false)
  })
})
