import { useSetAtom } from 'jotai'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeScene } from './scene/HomeOfficeScene'
import { MusicGazeboModal } from './zones/MusicGazeboModal'
import { StickyNoteModal } from './zones/StickyNoteModal'
import { DiaryDeskModal } from './zones/DiaryDeskModal'
import { useHomeOfficeAgentSync } from '@/hooks/useHomeOfficeAgentSync'
import { useCharacterPath } from '@/hooks/useCharacterPath'

export function HomeOfficeView() {
  const setOpen = useSetAtom(homeOfficePanelOpenAtom)
  useHomeOfficeAgentSync()
  useCharacterPath()

  return (
    <div className="flex flex-col w-full h-full">
      <div className="flex items-center justify-between px-4 h-[34px] flex-shrink-0 border-b border-border/40 titlebar-no-drag">
        <span className="text-[13px] font-semibold flex items-center gap-1.5">
          <span>🏝️ Home Office</span>
        </span>
        <button
          onClick={() => setOpen(false)}
          className="text-muted-foreground hover:text-foreground text-[18px] leading-none w-6 h-6 flex items-center justify-center rounded-md hover:bg-accent"
          title="返回 (Esc)"
        >
          ×
        </button>
      </div>
      <div className="flex-1 min-h-0 relative titlebar-no-drag">
        <HomeOfficeScene />
        <MusicGazeboModal />
        <StickyNoteModal />
        <DiaryDeskModal />
      </div>
    </div>
  )
}
