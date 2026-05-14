import { useState } from 'react'
import { useAtom } from 'jotai'
import { openZoneAtom, stickyNotesAtom, type StickyNote } from '@/atoms/home-office-atoms'

function newId() {
  return Math.random().toString(36).slice(2, 10)
}

export function StickyNoteModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const [notes, setNotes] = useAtom(stickyNotesAtom)
  const [draft, setDraft] = useState('')

  if (openZone !== 'sticky') return null

  const add = () => {
    if (!draft.trim()) return
    const note: StickyNote = { id: newId(), text: draft.trim(), at: Date.now() }
    setNotes([note, ...notes])
    setDraft('')
  }

  const remove = (id: string) => setNotes(notes.filter(n => n.id !== id))

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[440px] max-w-[560px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-base font-semibold">📌 Sticky Notes</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <p className="text-xs text-muted-foreground mb-3">暂存，重启丢失（Phase 4 持久化）</p>

        <div className="flex gap-2 mb-4">
          <input
            value={draft}
            onChange={e => setDraft(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && add()}
            placeholder="写一条便签…"
            className="flex-1 px-3 py-1.5 rounded-md bg-input text-foreground border border-border text-sm"
          />
          <button
            onClick={add}
            disabled={!draft.trim()}
            className="px-3 py-1.5 bg-accent text-accent-foreground rounded-md text-sm hover:bg-accent/80 disabled:opacity-40"
          >
            Add
          </button>
        </div>

        <div className="max-h-[300px] overflow-y-auto space-y-2">
          {notes.length === 0 && (
            <div className="text-sm text-muted-foreground italic">还没有便签</div>
          )}
          {notes.map(note => (
            <div key={note.id}
                 className="flex items-start justify-between gap-2 p-2 bg-secondary/40 rounded-md">
              <div className="text-sm flex-1">{note.text}</div>
              <button
                onClick={() => remove(note.id)}
                className="text-muted-foreground hover:text-foreground text-xs"
                title="删除"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
