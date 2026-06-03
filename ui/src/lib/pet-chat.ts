export class PetModelNotReady extends Error {}

export interface PetChatMsg { role: 'user' | 'assistant' | 'system'; content: string }

/** Stream a completion from the LOCAL engine. Calls onDelta per chunk.
 *  Throws PetModelNotReady on 503 (model not downloaded/loaded). */
export async function streamPetChat(
  messages: PetChatMsg[],
  onDelta: (t: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const resp = await fetch('http://127.0.0.1:7337/v1/chat/completions', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ model: 'local/minicpm5-1b', messages, stream: true }),
    signal,
  })
  if (resp.status === 503) throw new PetModelNotReady('model not ready')
  if (!resp.ok || !resp.body) throw new Error(`pet chat HTTP ${resp.status}`)
  const reader = resp.body.getReader()
  const decoder = new TextDecoder()
  let buf = ''
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buf += decoder.decode(value, { stream: true })
    const lines = buf.split('\n')
    buf = lines.pop() ?? ''
    for (const line of lines) {
      const s = line.trim()
      if (!s.startsWith('data:')) continue
      const data = s.slice(5).trim()
      if (data === '[DONE]') return
      try {
        const json = JSON.parse(data)
        const delta = json?.choices?.[0]?.delta?.content
        if (typeof delta === 'string' && delta) onDelta(delta)
      } catch { /* ignore keep-alive / partial frames */ }
    }
  }
}
