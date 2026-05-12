/**
 * Tests for useComposerMentionTrigger — the trigger detection state machine
 * that powers the composer's `/` and `@` autocomplete.
 *
 * Focuses on boundary behavior (the rules most likely to break silently):
 *   - Trigger fires only at start-of-string or after whitespace
 *   - Empty query is valid (user types just `/` or `@`)
 *   - Whitespace inside query closes the trigger
 *   - `commitReplacement` splices correctly
 */
import { describe, it, expect } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import * as React from 'react'
import { useComposerMentionTrigger } from './useComposerMentionTrigger'

/** Build a real textarea + a wrapper hook that proxies the controlled
 *  value. The trigger hook reads selectionStart imperatively so we need
 *  a real DOM node. jsdom is fine. */
function setup(initialValue = '') {
  const ta = document.createElement('textarea')
  ta.value = initialValue
  document.body.appendChild(ta)
  const ref = { current: ta }

  let externalValue = initialValue
  const setValue = (v: string) => {
    externalValue = v
    ta.value = v
  }

  const { result, rerender } = renderHook(({ value }) =>
    useComposerMentionTrigger({ textareaRef: ref, value }),
  { initialProps: { value: initialValue } })

  const moveCursor = (pos: number) => {
    ta.setSelectionRange(pos, pos)
    // Hook listens on `select`/`click`/`keyup` — synthesize one to
    // trigger the recompute path.
    ta.dispatchEvent(new Event('select'))
  }

  const cleanup = () => {
    document.body.removeChild(ta)
  }

  return { result, rerender, ta, ref, setValue, moveCursor, getValue: () => externalValue, cleanup }
}

describe('useComposerMentionTrigger', () => {
  it('fires when "/" is typed at start of string', () => {
    const env = setup('/')
    env.moveCursor(1)
    env.rerender({ value: '/' })
    expect(env.result.current.trigger).not.toBeNull()
    expect(env.result.current.trigger?.char).toBe('/')
    expect(env.result.current.trigger?.query).toBe('')
    env.cleanup()
  })

  it('fires when "@" is typed after whitespace', () => {
    const env = setup('hello @')
    env.moveCursor(7)
    env.rerender({ value: 'hello @' })
    expect(env.result.current.trigger?.char).toBe('@')
    expect(env.result.current.trigger?.triggerStart).toBe(6)
    env.cleanup()
  })

  it('does NOT fire when "@" is part of an email-like substring', () => {
    // "alice@" with no leading whitespace — the trigger char is preceded
    // by `e`, not whitespace → boundary check rejects.
    const env = setup('alice@')
    env.moveCursor(6)
    env.rerender({ value: 'alice@' })
    expect(env.result.current.trigger).toBeNull()
    env.cleanup()
  })

  it('does NOT fire for "/" inside a path like "src/foo"', () => {
    const env = setup('src/foo')
    env.moveCursor(7)
    env.rerender({ value: 'src/foo' })
    expect(env.result.current.trigger).toBeNull()
    env.cleanup()
  })

  it('updates query as user types after the trigger', () => {
    const env = setup('/')
    env.moveCursor(1)
    env.rerender({ value: '/' })
    expect(env.result.current.trigger?.query).toBe('')

    // setValue keeps the textarea node's `.value` in sync so cursor
    // positions past the original length aren't clamped — the hook
    // reads selectionStart via the ref, not via the React prop.
    env.setValue('/tdd')
    env.moveCursor(4)
    env.rerender({ value: '/tdd' })
    expect(env.result.current.trigger?.query).toBe('tdd')
    env.cleanup()
  })

  it('closes when whitespace is typed inside the query', () => {
    const env = setup('/tdd ')
    env.moveCursor(5)
    env.rerender({ value: '/tdd ' })
    // Whitespace at cursor-1 → recompute walks back, hits the space,
    // and stops without finding a trigger char.
    expect(env.result.current.trigger).toBeNull()
    env.cleanup()
  })

  it('commitReplacement splices the trigger span', () => {
    const env = setup('hi /tdd')
    env.moveCursor(7)
    env.rerender({ value: 'hi /tdd' })
    expect(env.result.current.trigger?.query).toBe('tdd')

    let commitResult: { newValue: string; newCursor: number } | null = null
    act(() => {
      commitResult = env.result.current.commitReplacement('/tdd-full-name')
    })
    expect(commitResult!.newValue).toBe('hi /tdd-full-name ')
    expect(commitResult!.newCursor).toBe('hi /tdd-full-name '.length)
    env.cleanup()
  })

  it('close() drops the trigger without modifying value', () => {
    const env = setup('/')
    env.moveCursor(1)
    env.rerender({ value: '/' })
    expect(env.result.current.trigger).not.toBeNull()

    act(() => {
      env.result.current.close()
    })
    expect(env.result.current.trigger).toBeNull()
    env.cleanup()
  })
})
