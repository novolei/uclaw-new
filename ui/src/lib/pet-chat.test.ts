import { describe, it, expect, vi, afterEach } from 'vitest'
import { streamPetChat, PetModelNotReady } from './pet-chat'

afterEach(() => {
  vi.restoreAllMocks()
})

describe('streamPetChat', () => {
  it('streams SSE deltas and calls onDelta for each chunk', async () => {
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(c) {
        c.enqueue(encoder.encode('data: {"choices":[{"delta":{"content":"你"}}]}\n'))
        c.enqueue(encoder.encode('data: {"choices":[{"delta":{"content":"好"}}]}\n'))
        c.enqueue(encoder.encode('data: [DONE]\n'))
        c.close()
      },
    })
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, status: 200, body: stream })
    global.fetch = fetchMock

    const deltas: string[] = []
    await streamPetChat([{ role: 'user', content: 'hi' }], (d) => deltas.push(d))

    expect(deltas).toEqual(['你', '好'])
  })

  it('throws PetModelNotReady on 503', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ status: 503, ok: false, body: null })
    global.fetch = fetchMock

    await expect(
      streamPetChat([{ role: 'user', content: 'hi' }], () => {}),
    ).rejects.toBeInstanceOf(PetModelNotReady)
  })
})
