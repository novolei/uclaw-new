import * as React from 'react'
import { useAtom } from 'jotai'
import { settingsTabAtom, settingsOpenAtom, type SettingsTab } from '@/atoms/settings-tab'
import { hasUpdateAtom } from '@/atoms/updater'
import { ScrollArea } from '@/components/ui/scroll-area'
import { GeneralSettings } from './GeneralSettings'
import { ToolSettings } from './ToolSettings'
import { ShortcutSettings } from './ShortcutSettings'
import { ChannelSettings } from './ChannelSettings'
import { ModelSettings } from './ModelSettings'
import { ProxySetting } from './ProxySetting'
import { AboutSettings } from './AboutSettings'
import { PetSettings } from './PetSettings'
import { SettingsNav } from './SettingsNav'
import { SttSettings } from './SttSettings'
import { SettingsBreadcrumb } from './SettingsBreadcrumb'


function SettingsContent({ tab }: { tab: SettingsTab }) {
  switch (tab) {
    case 'connectivity':
      // Placeholder — ConnectivityTab wrapper lands in Task 5
      return <ChannelSettings />
    case 'intelligence':
      // Placeholder — IntelligenceTab wrapper lands in Task 5
      return <ModelSettings />
    case 'tools':
      // Placeholder — ToolsTab wrapper lands in Task 6
      return <ToolSettings />
    case 'general':
      // Placeholder — GeneralTab wrapper lands in Task 6
      return <GeneralSettings />
    case 'stt':
      return <SttSettings />
    case 'shortcuts':
      return <ShortcutSettings />
    case 'pet':
      return <PetSettings />
    case 'proxy':
      return <ProxySetting />
    case 'about':
      return <AboutSettings />
    default:
      return <ChannelSettings />
  }
}

export default function SettingsPanel() {
  const [activeTab, setActiveTab] = useAtom(settingsTabAtom)
  const [, setOpen] = useAtom(settingsOpenAtom)
  const [hasUpdate] = useAtom(hasUpdateAtom)

  const TAB_LABEL: Record<SettingsTab, string> = {
    connectivity: '服务商与用量',
    intelligence: '智能',
    tools: '工具与能力',
    general: '通用与外观',
    stt: '输入（语音）',
    shortcuts: '快捷键',
    pet: '桌面宠物',
    proxy: '代理',
    about: '关于',
  }
  const activeLabel = TAB_LABEL[activeTab] ?? '设置'
  const scrollRef = React.useRef<HTMLDivElement | null>(null)

  return (
    <div className="flex flex-col h-full">
      <SettingsBreadcrumb
        tabLabel={activeLabel}
        scrollContainerRef={scrollRef as React.MutableRefObject<HTMLElement | null>}
        onClose={() => setOpen(false)}
      />

      {/* Body: left nav + right content */}
      <div className="flex flex-1 min-h-0">
        <SettingsNav
          active={activeTab}
          onChange={setActiveTab}
          hasUpdate={hasUpdate}
          sttNeedsDownload={false /* Task 7 wires this from modelStatusAtom */}
        />

        {/* Right content */}
        <ScrollArea className="flex-1">
          <div ref={scrollRef} className="max-w-[800px] mx-auto px-6 py-5">
            <SettingsContent tab={activeTab} />
          </div>
        </ScrollArea>
      </div>
    </div>
  )
}
