import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { emit } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'
import { petHistoryAtom, petBubbleAtom } from '@/atoms/pet-chat-atoms'
import { streamPetChat, PetModelNotReady, type PetChatMsg } from '@/lib/pet-chat'
import { petPersonaGetActive, type PetPersona } from '@/lib/tauri-bridge'

export function PetChat({ onClose }: { onClose: () => void }): React.ReactElement {
  const [history, setHistory] = useAtom(petHistoryAtom)
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const setBubble = useSetAtom(petBubbleAtom)
  const [input, setInput] = React.useState('')
  const [streaming, setStreaming] = React.useState(false)
  const [persona, setPersona] = React.useState<PetPersona | null>(null)

  React.useEffect(() => {
    let cancelled = false
    const load = () => { petPersonaGetActive().then((p) => { if (!cancelled) setPersona(p) }).catch(() => {}) }
    load()
    let un: (() => void) | undefined
    listen('pet://persona-changed', load).then((fn) => { if (cancelled) fn(); else un = fn })
    return () => { cancelled = true; un?.() }
  }, [])

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
    const sys = persona?.system_prompt?.trim()
      ? [{ role: 'system' as const, content: persona.system_prompt }]
      : []
    const msgs: PetChatMsg[] = [...sys, ...base.map((m) => ({ role: m.role, content: m.content }))]
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

  // Derive last assistant reply from history (accumulates during streaming)
  const lastReply = React.useMemo(() => {
    for (let i = history.length - 1; i >= 0; i--) {
      if (history[i].role === 'assistant') return history[i].content
    }
    return ''
  }, [history])

  return (
    <div className="pet-chat" data-testid="pet-chat">
      {lastReply
        ? <div className={`pet-reply${streaming ? ' streaming' : ''}`} data-testid="pet-reply">{lastReply}</div>
        : streaming
          ? <div className="pet-thinking"><span className="pet-spinner" />思考中…</div>
          : null}
      <textarea
        className="pet-ask"
        data-testid="pet-chat-input"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void send() }
          else if (e.key === 'Escape') onClose()
        }}
        placeholder="和我聊聊…"
        disabled={streaming}
        rows={1}
        autoFocus
      />
    </div>
  )
}
