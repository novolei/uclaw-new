import { useAtom } from 'jotai'
import { settingsTabAtom, type SettingsTab } from '@/atoms/settings-tab'
import { cn } from '@/lib/utils'
import { GeneralSettings } from './GeneralSettings'
import { AppearanceSettings } from './AppearanceSettings'
import { AgentSettings } from './AgentSettings'
import { ToolSettings } from './ToolSettings'
import { PromptSettings } from './PromptSettings'
import { ShortcutSettings } from './ShortcutSettings'
import { ChannelSettings } from './ChannelSettings'
import { BotDefaultSettings } from './BotDefaultSettings'
import { ProxySetting } from './ProxySetting'
import { AboutSettings } from './AboutSettings'
import { ScrollArea } from '@/components/ui/scroll-area'

const TABS: { id: SettingsTab; label: string }[] = [
  { id: 'channels', label: '渠道' },
  { id: 'general', label: '通用' },
  { id: 'appearance', label: '外观' },
  { id: 'agent', label: 'Agent' },
  { id: 'tools', label: '工具' },
  { id: 'prompts', label: '提示词' },
  { id: 'bots', label: 'Bot' },
  { id: 'shortcuts', label: '快捷键' },
  { id: 'proxy', label: '代理' },
  { id: 'about', label: '关于' },
]

function SettingsContent({ tab }: { tab: SettingsTab }) {
  switch (tab) {
    case 'general':
      return <GeneralSettings />
    case 'channels':
      return <ChannelSettings />
    case 'appearance':
      return <AppearanceSettings />
    case 'agent':
      return <AgentSettings />
    case 'tools':
      return <ToolSettings />
    case 'prompts':
      return <PromptSettings />
    case 'bots':
      return <BotDefaultSettings />
    case 'shortcuts':
      return <ShortcutSettings />
    case 'proxy':
      return <ProxySetting />
    case 'about':
      return <AboutSettings />
    default:
      return <GeneralSettings />
  }
}

export default function SettingsPanel() {
  const [activeTab, setActiveTab] = useAtom(settingsTabAtom)

  return (
    <div className="flex h-full">
      {/* Sidebar */}
      <div className="w-[200px] border-r border-border p-3 flex flex-col gap-0.5">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={cn(
              'w-full text-left px-3 py-1.5 rounded-md text-sm transition-colors',
              activeTab === tab.id
                ? 'bg-accent text-accent-foreground font-medium'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'
            )}
            onClick={() => setActiveTab(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {/* Content */}
      <ScrollArea className="flex-1">
        <div className="max-w-[640px] mx-auto p-6">
          <SettingsContent tab={activeTab} />
        </div>
      </ScrollArea>
    </div>
  )
}
