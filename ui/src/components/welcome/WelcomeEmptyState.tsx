/**
 * WelcomeEmptyState — 空对话欢迎页
 *
 * 在没有消息时显示欢迎信息和快捷操作建议。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MessageSquare, Sparkles, Code, FileText, Lightbulb } from 'lucide-react'
import { cn } from '@/lib/utils'
import { userProfileAtom } from '@/atoms/user-profile'
import { getAgentWelcomeMessage } from '@/lib/tips'

interface QuickAction {
  icon: React.ReactNode
  label: string
  description: string
  prompt: string
}

const QUICK_ACTIONS: QuickAction[] = [
  {
    icon: <Code className="size-4" />,
    label: '编写代码',
    description: '让 AI 帮你写一段代码',
    prompt: '帮我写一个',
  },
  {
    icon: <FileText className="size-4" />,
    label: '文本处理',
    description: '翻译、摘要、改写',
    prompt: '帮我翻译以下内容：',
  },
  {
    icon: <Lightbulb className="size-4" />,
    label: '头脑风暴',
    description: '讨论想法和方案',
    prompt: '帮我分析一下',
  },
  {
    icon: <Sparkles className="size-4" />,
    label: '自由对话',
    description: '随便聊聊',
    prompt: '',
  },
]

interface WelcomeEmptyStateProps {
  /** 点击快捷操作时回调 */
  onQuickAction?: (prompt: string) => void
  className?: string
}

export function WelcomeEmptyState({ onQuickAction, className }: WelcomeEmptyStateProps): React.ReactElement {
  const userProfile = useAtomValue(userProfileAtom)
  const welcomeMessage = React.useMemo(
    () => getAgentWelcomeMessage(userProfile.userName !== '用户' ? userProfile.userName : undefined),
    [userProfile.userName],
  )

  return (
    <div className={cn('flex-1 flex flex-col items-center justify-center gap-6 p-8 text-center', className)}>
      {/* Logo / Branding */}
      <div className="size-16 rounded-2xl bg-primary/10 flex items-center justify-center">
        <MessageSquare className="size-8 text-primary/60" />
      </div>

      {/* Welcome Text */}
      <div>
        <h3 className="text-lg font-medium text-foreground/80 mb-1">{welcomeMessage}</h3>
        <p className="text-sm text-muted-foreground max-w-sm">
          选择下方的快捷操作，或在输入框中直接输入你的问题。
        </p>
      </div>

      {/* Quick Actions Grid */}
      {onQuickAction && (
        <div className="grid grid-cols-2 gap-3 w-full max-w-md">
          {QUICK_ACTIONS.map((action) => (
            <button
              key={action.label}
              type="button"
              className="flex flex-col items-start gap-1.5 p-3 rounded-lg border border-border/50 bg-card/50 hover:bg-accent/50 hover:border-border transition-colors text-left group"
              onClick={() => onQuickAction(action.prompt)}
            >
              <div className="text-muted-foreground/60 group-hover:text-primary/60 transition-colors">
                {action.icon}
              </div>
              <div>
                <div className="text-sm font-medium text-foreground/80">{action.label}</div>
                <div className="text-xs text-muted-foreground">{action.description}</div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
