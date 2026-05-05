import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

const SHORTCUTS = [
  { action: '新建对话', keys: '⌘ N' },
  { action: '打开设置', keys: '⌘ ,' },
  { action: '搜索', keys: '⌘ K' },
  { action: '关闭当前面板', keys: '⌘ W' },
  { action: '切换侧边栏', keys: '⌘ B' },
  { action: '发送消息', keys: 'Enter' },
  { action: '换行', keys: 'Shift + Enter' },
  { action: '重新生成', keys: '⌘ R' },
  { action: '停止生成', keys: 'Esc' },
  { action: '复制最后回复', keys: '⌘ Shift + C' },
]

export function ShortcutSettings() {
  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">快捷键</h2>

      <SettingsSection description="以下快捷键在应用内全局生效">
        <SettingsCard>
          <div className="divide-y divide-border">
            {SHORTCUTS.map((s) => (
              <div key={s.action} className="flex items-center justify-between py-2.5">
                <span className="text-sm text-foreground">{s.action}</span>
                <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border border-border">
                  {s.keys}
                </kbd>
              </div>
            ))}
          </div>
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
