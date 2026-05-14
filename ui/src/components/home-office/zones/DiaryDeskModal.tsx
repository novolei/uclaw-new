import { useAtom } from 'jotai'
import { openZoneAtom, diaryEntriesAtom } from '@/atoms/home-office-atoms'

export function DiaryDeskModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const [entries] = useAtom(diaryEntriesAtom)

  if (openZone !== 'diary') return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[440px] max-w-[560px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-base font-semibold">✍️ Agent Diary</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <p className="text-xs text-muted-foreground mb-3">暂存，重启丢失（Phase 4 持久化 + 按 session 归档）</p>
        <div className="max-h-[360px] overflow-y-auto space-y-3">
          {entries.length === 0 && (
            <div className="text-sm text-muted-foreground italic">Agent 还没有写过日记</div>
          )}
          {entries.map(entry => (
            <div key={entry.id} className="p-3 bg-secondary/40 rounded-md">
              <div className="text-xs text-muted-foreground mb-1">
                {new Date(entry.at).toLocaleString()} · session {entry.sessionId.slice(0, 8)}
              </div>
              <div className="text-sm whitespace-pre-wrap">{entry.text}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
