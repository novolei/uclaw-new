/**
 * WorkspaceShell — 任务流 surface。
 *
 * 由 MainArea.tsx move-only 提取（2026-05-14 Kaleidoscope Phase 1）。
 * 承载 chat / agent / automation / homeOffice / preview 全部分支与
 * 相关 hooks。MainArea 现在只负责顶层 surface 切换 + Panel 外壳 +
 * SettingsDialog。
 *
 * W4a-followup: when the preview panel is open, the body is split
 * horizontally (chat ↔ preview) with a draggable resize handle between
 * them. The split ratio is persisted via `previewPanelSplitRatioAtom`.
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { visibleTabsAtom, activeTabIdAtom } from '@/atoms/tab-atoms'
import {
  previewPanelOpenAtom,
  previewPanelSplitRatioAtom,
  selectedPreviewFileAtom,
} from '@/atoms/preview-panel-atoms'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import { PreviewPanel } from '@/components/preview/PreviewPanel'
import WelcomeView from '@/views/WelcomeView'
import { TabBar } from '@/components/tabs/TabBar'
import { TabContent } from '@/components/tabs/TabContent'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeView } from '@/components/home-office/HomeOfficeView'
import { MemoryRecallPanel } from '@/components/workspace/MemoryRecallPanel'

const MIN_CHAT_RATIO = 0.30
const MAX_CHAT_RATIO = 0.80

export function WorkspaceShell(): React.ReactElement {
  const tabs = useAtomValue(visibleTabsAtom)
  const activeTabId = useAtomValue(activeTabIdAtom)
  const setActiveTabId = useSetAtom(activeTabIdAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const setPreviewOpen = useSetAtom(previewPanelOpenAtom)
  const setSelectedPreviewFile = useSetAtom(selectedPreviewFileAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const [splitRatio, setSplitRatio] = useAtom(previewPanelSplitRatioAtom)
  const [homeOfficeOpen, setHomeOfficeOpen] = useAtom(homeOfficePanelOpenAtom)
  const draggingRef = React.useRef(false)

  // Reset the preview panel when the active workspace changes. The previously
  // selected file lived in the OLD workspace's mount tree — keeping the panel
  // open after switch surfaces a stale file (or a noisy "loading" state for a
  // path that the new workspace can't resolve). Future opens (via openPreview
  // from the rail) re-mount the panel with the new file.
  const prevWorkspaceRef = React.useRef(currentWorkspaceId)
  React.useEffect(() => {
    if (prevWorkspaceRef.current === currentWorkspaceId) return
    prevWorkspaceRef.current = currentWorkspaceId
    setPreviewOpen(false)
    setSelectedPreviewFile(null)
  }, [currentWorkspaceId, setPreviewOpen, setSelectedPreviewFile])

  // [FLASH-DEBUG] 监控 tabs 变化，如果 tabs.length 变为 0 说明所有标签被卸载
  React.useEffect(() => {
    if (tabs.length === 0) {
      console.warn('[FLASH-DEBUG] WorkspaceShell: tabs.length === 0, showing WelcomeView!', new Error().stack)
    }
  }, [tabs.length])

  // 兜底：tabs 存在但 activeTabId 为空时，自动激活第一个标签。
  // 正常路径（openTab/closeTab/持久化恢复）都会维护 activeTabId，此分支只为防御
  // 异常状态（如外部原子被误清空），避免渲染 WelcomeView 触发重复 openTab 循环。
  React.useEffect(() => {
    if (tabs.length > 0 && !activeTabId) {
      setActiveTabId(tabs[0]!.id)
    }
  }, [tabs, activeTabId, setActiveTabId])

  const onResizeStart = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      draggingRef.current = true
      const startX = e.clientX
      const startRatio = splitRatio
      const containerEl = (e.currentTarget as HTMLElement).closest(
        '[data-preview-split]',
      ) as HTMLElement | null
      const containerWidth = containerEl?.clientWidth ?? 1
      let rafId = 0

      document.body.style.userSelect = 'none'
      document.body.style.cursor = 'col-resize'
      // Lock iframes during the drag so they don't swallow mouse events.
      document.querySelectorAll('iframe').forEach((f) => {
        ;(f as HTMLElement).style.pointerEvents = 'none'
      })

      const onMove = (ev: MouseEvent) => {
        if (!draggingRef.current) return
        if (rafId) return
        rafId = requestAnimationFrame(() => {
          rafId = 0
          const delta = ev.clientX - startX
          const next = Math.max(
            MIN_CHAT_RATIO,
            Math.min(MAX_CHAT_RATIO, startRatio + delta / containerWidth),
          )
          setSplitRatio(next)
        })
      }
      const onUp = () => {
        draggingRef.current = false
        if (rafId) cancelAnimationFrame(rafId)
        document.body.style.userSelect = ''
        document.body.style.cursor = ''
        document.querySelectorAll('iframe').forEach((f) => {
          ;(f as HTMLElement).style.pointerEvents = ''
        })
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
      }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [splitRatio, setSplitRatio],
  )

  const chatBody = (
    <>
      <TabBar />
      {/* Both body branches sit inside their own titlebar-no-drag so
          clicks land on chat/agent/welcome UI; the TabBar above stays
          in the drag region. We removed the previous broad
          titlebar-no-drag wrapper at AppShell-level because WKWebView
          won't subtract a child drag-region from a parent no-drag. */}
      {tabs.length === 0 ? (
        <div className="flex-1 min-h-0 titlebar-no-drag">
          <WelcomeView />
        </div>
      ) : activeTabId ? (
        <div className="flex-1 min-h-0 titlebar-no-drag">
          <TabContent tabId={activeTabId} />
        </div>
      ) : null}
      <MemoryRecallPanel />
    </>
  )

  React.useEffect(() => {
    if (!homeOfficeOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setHomeOfficeOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [homeOfficeOpen, setHomeOfficeOpen])

  if (homeOfficeOpen) return <HomeOfficeView />
  if (previewOpen) {
    return (
      <div className="flex flex-1 min-h-0" data-preview-split>
        {/* Left: chat (TabBar + TabContent) */}
        <div
          className="flex flex-col min-w-0 h-full"
          style={{ flex: `0 0 calc(${splitRatio * 100}% - 4px)` }}
        >
          {chatBody}
        </div>

        {/* Drag handle */}
        <button
          type="button"
          onMouseDown={onResizeStart}
          aria-label="拖动调整预览面板宽度"
          title="拖动调整宽度"
          className="w-[8px] cursor-col-resize flex-shrink-0 self-stretch bg-border/40 hover:bg-foreground/20 active:bg-foreground/30 transition-colors"
        />

        {/* Right: preview */}
        <div className="flex-1 min-w-0 h-full overflow-hidden">
          <PreviewPanel />
        </div>
      </div>
    )
  }
  return chatBody
}
