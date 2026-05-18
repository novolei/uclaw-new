import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  previewTabsAtom,
  activePreviewTabKeyAtom,
  closePreviewTabAction,
  previewTabKey,
} from '@/atoms/preview-panel-atoms'
import { PreviewTabItem } from './PreviewTabItem'

export function PreviewTabBar(): React.ReactElement | null {
  const tabs = useAtomValue(previewTabsAtom)
  const activeKey = useAtomValue(activePreviewTabKeyAtom)
  const setActive = useSetAtom(activePreviewTabKeyAtom)
  const closeTab = useSetAtom(closePreviewTabAction)

  if (tabs.length === 0) return null

  return (
    <div
      role="tablist"
      aria-label="预览文件标签页"
      className="flex items-stretch border-b border-border bg-card overflow-x-auto"
    >
      {tabs.map((tab) => {
        const key = previewTabKey(tab)
        return (
          <PreviewTabItem
            key={key}
            tab={tab}
            isActive={key === activeKey}
            onActivate={() => setActive(key)}
            onClose={() => closeTab(key)}
          />
        )
      })}
    </div>
  )
}
