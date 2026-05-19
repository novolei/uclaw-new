import * as React from 'react'
import { BottomDock } from './BottomDock'

const REVEAL_HIDE_DELAY_MS = 180
const TRIGGER_WIDTH_PX = 440 // bottom centered "wake" strip width when collapsed
const TRIGGER_HEIGHT_PX = 6 //  ~macOS Dock corner hot strip
const REVEAL_PAD_X_PX = 24 // invisible horizontal hover buffer around dock when revealed
const REVEAL_PAD_TOP_PX = 12 // invisible vertical hover buffer above dock when revealed

/**
 * 一个容器同时承载「未展开时的窄触发条」和「展开后包裹 dock 的缓冲区」，
 * 共用一套 mouseEnter/mouseLeave 状态机 —— 杜绝触发条 ↔ dock 之间的「无人区」
 * 导致 dock 卡住不收回的 bug。
 *
 * BottomDock 始终挂载（保留滑入动画状态），通过 props.revealed 控制可见性。
 * 容器自身的 pointer-events 仅落在可见的命中区域，不会拦截其他页面交互。
 */
export function BottomDockHoverRegion(): React.ReactElement {
  const [revealed, setRevealed] = React.useState(false)
  const hideTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  const cancelHide = React.useCallback(() => {
    if (hideTimerRef.current !== null) {
      clearTimeout(hideTimerRef.current)
      hideTimerRef.current = null
    }
  }, [])

  const scheduleHide = React.useCallback(() => {
    cancelHide()
    hideTimerRef.current = setTimeout(() => setRevealed(false), REVEAL_HIDE_DELAY_MS)
  }, [cancelHide])

  const handleEnter = React.useCallback(() => {
    cancelHide()
    setRevealed(true)
  }, [cancelHide])

  React.useEffect(() => () => cancelHide(), [cancelHide])

  return (
    <div
      className="fixed bottom-0 left-1/2 z-[70] flex justify-center pointer-events-auto"
      style={{
        transform: 'translateX(-50%)',
        // 折叠时只占 ~440x6 居中底条；展开后扩张到 dock+缓冲（auto width，给 padding 留量）
        width: revealed ? 'auto' : TRIGGER_WIDTH_PX,
        height: revealed ? 'auto' : TRIGGER_HEIGHT_PX,
        paddingLeft: revealed ? REVEAL_PAD_X_PX : 0,
        paddingRight: revealed ? REVEAL_PAD_X_PX : 0,
        paddingTop: revealed ? REVEAL_PAD_TOP_PX : 0,
      }}
      onMouseEnter={handleEnter}
      onMouseLeave={scheduleHide}
      data-revealed={revealed}
    >
      <BottomDock revealed={revealed} />
    </div>
  )
}
