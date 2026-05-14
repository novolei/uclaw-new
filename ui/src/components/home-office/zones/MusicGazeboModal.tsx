import { useEffect, useRef, useState } from 'react'
import { useAtom } from 'jotai'
import { openZoneAtom } from '@/atoms/home-office-atoms'

const TRACKS = [
  { id: 'placeholder-1', title: 'Lofi Placeholder', src: '/home-office/audio/lofi-placeholder.mp3' },
]

export function MusicGazeboModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const audioRef = useRef<HTMLAudioElement | null>(null)
  const [playing, setPlaying] = useState(false)
  const [trackIndex, setTrackIndex] = useState(0)

  useEffect(() => {
    if (openZone !== 'music') {
      audioRef.current?.pause()
      setPlaying(false)
    }
  }, [openZone])

  if (openZone !== 'music') return null

  const track = TRACKS[trackIndex]
  const togglePlay = () => {
    const a = audioRef.current
    if (!a) return
    if (playing) {
      a.pause()
      setPlaying(false)
    } else {
      a.play().then(() => setPlaying(true)).catch(() => setPlaying(false))
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[360px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-base font-semibold">🎵 Music Gazebo</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <div className="text-sm mb-3">
          <div className="font-medium">{track.title}</div>
          <div className="text-muted-foreground text-xs">Track {trackIndex + 1} / {TRACKS.length}</div>
        </div>
        <audio
          ref={audioRef}
          src={track.src}
          onEnded={() => setPlaying(false)}
        />
        <div className="flex gap-2">
          <button
            onClick={togglePlay}
            className="px-3 py-1.5 bg-accent text-accent-foreground rounded-md text-sm hover:bg-accent/80"
          >
            {playing ? 'Pause' : 'Play'}
          </button>
          <button
            onClick={() => setTrackIndex(i => (i + 1) % TRACKS.length)}
            disabled={TRACKS.length <= 1}
            className="px-3 py-1.5 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 disabled:opacity-40"
          >
            Next
          </button>
        </div>
      </div>
    </div>
  )
}
