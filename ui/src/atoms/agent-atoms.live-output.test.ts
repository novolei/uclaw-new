import { describe, it, expect } from 'vitest'
import { appendLiveOutput, type LiveOutput } from './agent-atoms'

describe('appendLiveOutput', () => {
  it('creates segments on first chunk', () => {
    const out = appendLiveOutput(undefined, 'stdout', 'hello')
    expect(out.segments).toEqual([{ stream: 'stdout', text: 'hello' }])
    expect(out.bytes).toBe(5)
    expect(out.droppedHead).toBe(false)
  })

  it('merges consecutive same-stream chunks into one segment', () => {
    let out = appendLiveOutput(undefined, 'stdout', 'foo')
    out = appendLiveOutput(out, 'stdout', 'bar')
    expect(out.segments).toEqual([{ stream: 'stdout', text: 'foobar' }])
  })

  it('starts a new segment when the stream switches', () => {
    let out = appendLiveOutput(undefined, 'stdout', 'out')
    out = appendLiveOutput(out, 'stderr', 'err')
    expect(out.segments).toEqual([
      { stream: 'stdout', text: 'out' },
      { stream: 'stderr', text: 'err' },
    ])
  })

  it('drops head and sets droppedHead when exceeding 256KB', () => {
    let out: LiveOutput | undefined = undefined
    const big = 'x'.repeat(100 * 1024)
    out = appendLiveOutput(out, 'stdout', big)
    out = appendLiveOutput(out, 'stdout', big)
    out = appendLiveOutput(out, 'stdout', big) // 300KB total > 256KB
    expect(out.droppedHead).toBe(true)
    expect(out.bytes).toBeLessThanOrEqual(256 * 1024)
  })
})
