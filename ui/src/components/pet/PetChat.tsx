import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { emit } from '@tauri-apps/api/event'
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'
import { petHistoryAtom, petBubbleAtom } from '@/atoms/pet-chat-atoms'
import { streamPetChat, PetModelNotReady, type PetChatMsg } from '@/lib/pet-chat'

export function PetChat({ onClose }: { onClose: () => void }): React.ReactElement {
  const [history, setHistory] = useAtom(petHistoryAtom)
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const setBubble = useSetAtom(petBubbleAtom)
  const [input, setInput] = React.useState('')
  const [streaming, setStreaming] = React.useState(false)

  const send = async () => {
    const text = input.trim()
    if (!text || streaming) return
    setInput('')
    const base = [...history, { role: 'user' as const, content: text }]
    setHistory(base)
    setStreaming(true)
    setPrimary('thinking')
    let acc = ''
    setHistory([...base, { role: 'assistant', content: '' }])
    const msgs: PetChatMsg[] = base.map((m) => ({ role: m.role, content: m.content }))
    try {
      await streamPetChat(msgs, (d) => {
        acc += d
        setPrimary('typing')
        setHistory([...base, { role: 'assistant', content: acc }])
      })
      setPrimary('success')
      window.setTimeout(() => setPrimary('idle'), 1200)
    } catch (e) {
      if (e instanceof PetModelNotReady) {
        setBubble('我还在热身,去把模型装好呀~')
        void emit('pet://open-wizard')
        setHistory(base)               // roll back the empty assistant placeholder
        setPrimary('idle')
      } else {
        setPrimary('error')
        window.setTimeout(() => setPrimary('idle'), 1500)
      }
    } finally {
      setStreaming(false)
    }
  }

  return (
    <div className="pet-chat" data-testid="pet-chat">
      <div className="pet-chat-msgs">
        {history.map((m, i) => (
          <div key={i} className={`pet-msg pet-msg-${m.role}`}>{m.content}</div>
        ))}
      </div>
      <div className="pet-chat-input">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void send() }}
          placeholder="和我聊聊…"
          disabled={streaming}
          data-testid="pet-chat-input"
        />
        <button type="button" onClick={() => void send()} disabled={streaming}>发送</button>
        <button type="button" onClick={onClose} aria-label="收起">×</button>
      </div>
    </div>
  )
}
