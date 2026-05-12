import * as React from 'react'
import { useAtom } from 'jotai'
import { settingsTabAtom, settingsOpenAtom, type SettingsTab } from '@/atoms/settings-tab'
import { hasUpdateAtom } from '@/atoms/updater'
import { cn } from '@/lib/utils'
import {
  Settings,
  Radio,
  Palette,
  Info,
  Plug,
  Globe,
  Wrench,
  Bot,
  Keyboard,
  X,
  Cpu,
  BarChart3,
  ShieldCheck,
  FileCode2,
  Sparkles,
} from 'lucide-react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { GeneralSettings } from './GeneralSettings'
import { AppearanceSettings } from './AppearanceSettings'
import { UsageSettings } from './UsageSettings'
import { AgentSettings } from './AgentSettings'
import { ToolSettings } from './ToolSettings'
import { PermissionsSettings } from './PermissionsSettings'
import { PromptsSettings } from './PromptsSettings'
import { SkillsSettings } from './SkillsSettings'
import { ShortcutSettings } from './ShortcutSettings'
import { ChannelSettings } from './ChannelSettings'
import { ModelSettings } from './ModelSettings'
import { BotDefaultSettings } from './BotDefaultSettings'
import { ProxySetting } from './ProxySetting'
import { AboutSettings } from './AboutSettings'

interface TabItem {
  id: SettingsTab
  label: string
  icon: React.ReactNode
}

const TABS: TabItem[] = [
  { id: 'channels', label: '服务商', icon: <Radio size={15} /> },
  { id: 'models', label: '模型配置', icon: <Cpu size={15} /> },
  { id: 'general', label: '通用', icon: <Settings size={15} /> },
  { id: 'appearance', label: '外观', icon: <Palette size={15} /> },
  { id: 'usage', label: '用量与预算', icon: <BarChart3 size={15} /> },
  { id: 'agent', label: 'Agent', icon: <Plug size={15} /> },
  { id: 'tools', label: '工具', icon: <Wrench size={15} /> },
  { id: 'permissions', label: '工具权限', icon: <ShieldCheck size={15} /> },
  { id: 'prompts', label: '提示词', icon: <FileCode2 size={15} /> },
  { id: 'skills', label: '已学技能', icon: <Sparkles size={15} /> },
  { id: 'bots', label: 'Bot', icon: <Bot size={15} /> },
  { id: 'shortcuts', label: '快捷键', icon: <Keyboard size={15} /> },
  { id: 'proxy', label: '代理', icon: <Globe size={15} /> },
  { id: 'about', label: '关于', icon: <Info size={15} /> },
]

function SettingsContent({ tab }: { tab: SettingsTab }) {
  switch (tab) {
    case 'general':
      return <GeneralSettings />
    case 'channels':
      return <ChannelSettings />
    case 'models':
      return <ModelSettings />
    case 'appearance':
      return <AppearanceSettings />
    case 'usage':
      return <UsageSettings />
    case 'agent':
      return <AgentSettings />
    case 'tools':
      return <ToolSettings />
    case 'permissions':
      return <PermissionsSettings />
    case 'prompts':
      return <PromptsSettings />
    case 'skills':
      return <SkillsSettings />
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
  const [, setOpen] = useAtom(settingsOpenAtom)
  const [hasUpdate] = useAtom(hasUpdateAtom)

  const activeLabel = TABS.find((t) => t.id === activeTab)?.label ?? '设置'

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="h-12 flex items-center justify-between px-5 border-b border-border/50 flex-shrink-0">
        <h2 className="text-sm font-medium text-foreground">{activeLabel}</h2>
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="rounded-md p-1.5 text-muted-foreground/60 hover:text-foreground hover:bg-muted transition-colors"
        >
          <X size={16} />
        </button>
      </div>

      {/* Body: left nav + right content */}
      <div className="flex flex-1 min-h-0">
        {/* Left nav */}
        <div className="w-[160px] border-r border-border/50 pt-3 px-2 flex-shrink-0">
          <nav className="flex flex-col gap-0.5">
            {TABS.map((tab) => (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  'flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
                  activeTab === tab.id
                    ? 'bg-muted text-foreground font-medium'
                    : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
                )}
              >
                {tab.icon}
                <span>{tab.label}</span>
                {tab.id === 'about' && hasUpdate && (
                  <span className="w-2 h-2 rounded-full bg-red-500 ml-auto" />
                )}
              </button>
            ))}
          </nav>
        </div>

        {/* Right content */}
        {activeTab === 'channels' ? (
          <ChannelSettings />
        ) : (
          <ScrollArea className="flex-1">
            <div className="max-w-[640px] mx-auto px-6 py-5">
              <SettingsContent tab={activeTab} />
            </div>
          </ScrollArea>
        )}
      </div>
    </div>
  )
}
