import { describe, it, expect } from 'vitest'
import { spaceIdForMount } from './files-rail-helpers'
import type { MountKind } from '@/atoms/files-rail-atoms'

const mk = (id: string, kind: MountKind = 'workspace') => ({ id, kind })

describe('spaceIdForMount', () => {
  it('extracts space_id from workspace mount', () => {
    expect(spaceIdForMount(mk('workspace:abc123'), null)).toBe('abc123')
  })

  it('extracts space_id from workspace-attached mount (with hash suffix)', () => {
    expect(spaceIdForMount(mk('workspace-attached:abc123:deadbeef0123', 'attached_dir'), null)).toBe('abc123')
  })

  it('falls back to currentWorkspaceId for session mount', () => {
    expect(spaceIdForMount(mk('session:sess-xyz', 'session'), 'fallback-id')).toBe('fallback-id')
  })

  it('falls back to currentWorkspaceId for session-attached mount', () => {
    expect(spaceIdForMount(mk('attached:sess-xyz:cafebabe', 'attached_dir'), 'fallback-id')).toBe('fallback-id')
  })

  it('returns null for session mount when no fallback', () => {
    expect(spaceIdForMount(mk('session:sess-xyz', 'session'), null)).toBeNull()
  })

  it('returns null for malformed id (unknown prefix)', () => {
    expect(spaceIdForMount(mk('totally:bogus'), 'fallback')).toBeNull()
  })

  it('returns null for empty workspace id', () => {
    expect(spaceIdForMount(mk('workspace:'), null)).toBeNull()
  })

  it('returns null for workspace-attached missing colon segment', () => {
    expect(spaceIdForMount(mk('workspace-attached:onlyspace', 'attached_dir'), null)).toBeNull()
  })
})
