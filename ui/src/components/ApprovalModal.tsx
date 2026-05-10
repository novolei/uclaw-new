/**
 * ApprovalModal - 工具调用审批弹窗
 *
 * 当后端请求审批时弹出，展示工具名称、参数、风险等级，
 * 提供"批准"/"拒绝"/"始终允许"操作。
 * 使用 shadcn AlertDialog。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { ShieldAlert, ShieldCheck, ShieldOff, Terminal, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { approveToolCall, onNeedApproval, createPermissionRule } from '@/lib/tauri-bridge'
import type { ApprovalRequest } from '@/lib/types'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'

const RISK_CONFIG: Record<string, { label: string; icon: React.ElementType; color: string; bgClass: string }> = {
  low: { label: '低风险', icon: ShieldCheck, color: '#22c55e', bgClass: 'bg-green-500/10 text-green-500 border-green-500/20' },
  medium: { label: '中风险', icon: ShieldAlert, color: '#eab308', bgClass: 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20' },
  high: { label: '高风险', icon: AlertTriangle, color: '#ef4444', bgClass: 'bg-red-500/10 text-red-500 border-red-500/20' },
  critical: { label: '严重风险', icon: ShieldOff, color: '#dc2626', bgClass: 'bg-red-600/10 text-red-600 border-red-600/20' },
}

function getRiskConfig(level?: string) {
  return RISK_CONFIG[level ?? 'medium'] ?? RISK_CONFIG.medium
}

export function ApprovalModal(): React.ReactElement {
  const [request, setRequest] = React.useState<ApprovalRequest | null>(null)
  const [loading, setLoading] = React.useState(false)

  const appMode = useAtomValue(appModeAtom)
  const currentAgentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const currentConversationId = useAtomValue(currentConversationIdAtom)
  const activeSessionId = appMode === 'agent' ? currentAgentSessionId : currentConversationId

  // 监听后端审批事件
  React.useEffect(() => {
    let unlisten: (() => void) | undefined
    onNeedApproval((payload) => {
      setRequest(payload)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  const respondWithSessionRule = async (): Promise<void> => {
    if (!request || loading || !activeSessionId) return
    setLoading(true)
    try {
      await createPermissionRule({
        scope: 'session',
        sessionId: activeSessionId,
        toolName: request.toolName,
        mode: 'allow',
      })
      await approveToolCall({
        sessionId: request.sessionId,
        toolId: request.toolId,
        approved: true,
        alwaysAllow: false,
      })
    } catch (err) {
      console.error('[ApprovalModal] session-allow failed:', err)
    } finally {
      setLoading(false)
      setRequest(null)
    }
  }

  const respond = async (approved: boolean, alwaysAllow?: boolean): Promise<void> => {
    if (!request || loading) return
    setLoading(true)
    try {
      await approveToolCall({
        sessionId: request.sessionId,
        toolId: request.toolId,
        approved,
        alwaysAllow,
      })
    } catch (err) {
      console.error('[ApprovalModal] 审批失败:', err)
    } finally {
      setLoading(false)
      setRequest(null)
    }
  }

  const risk = getRiskConfig(request?.riskLevel)
  const RiskIcon = risk.icon

  const argsEntries = request?.arguments ? Object.entries(request.arguments) : []

  return (
    <AlertDialog open={!!request} onOpenChange={(open) => { if (!open) setRequest(null) }}>
      <AlertDialogContent className="sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <Terminal className="size-5 text-muted-foreground" />
            工具调用审批
          </AlertDialogTitle>
          <AlertDialogDescription>
            AI 请求执行以下工具，请确认是否允许。
          </AlertDialogDescription>
        </AlertDialogHeader>

        {request && (
          <div className="space-y-3 my-2">
            {/* 工具名 + 风险等级 */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <code className="text-sm font-mono font-medium bg-muted px-2 py-0.5 rounded">
                  {request.toolName}
                </code>
              </div>
              <Badge variant="outline" className={cn('gap-1', risk.bgClass)}>
                <RiskIcon className="size-3" />
                {risk.label}
              </Badge>
            </div>

            {/* 命令（如果有） */}
            {request.command && (
              <div className="space-y-1">
                <p className="text-xs font-medium text-muted-foreground">命令</p>
                <pre className="text-xs font-mono bg-muted/60 border border-border/50 rounded-md p-2 overflow-x-auto whitespace-pre-wrap break-all">
                  {request.command}
                </pre>
              </div>
            )}

            {/* 参数 */}
            {argsEntries.length > 0 && (
              <div className="space-y-1">
                <p className="text-xs font-medium text-muted-foreground">参数</p>
                <ScrollArea className="max-h-40">
                  <div className="space-y-1">
                    {argsEntries.map(([key, val]) => (
                      <div key={key} className="flex gap-2 text-xs">
                        <span className="font-mono text-muted-foreground shrink-0">{key}:</span>
                        <span className="font-mono break-all">
                          {typeof val === 'string' ? val : JSON.stringify(val)}
                        </span>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              </div>
            )}
          </div>
        )}

        <AlertDialogFooter className="flex-col sm:flex-row gap-2 flex-wrap">
          <AlertDialogCancel asChild>
            <Button
              variant="outline"
              onClick={() => respond(false)}
              disabled={loading}
              className="border-red-500/30 text-red-500 hover:bg-red-500/10"
            >
              拒绝
            </Button>
          </AlertDialogCancel>
          {activeSessionId && (
            <Button
              variant="outline"
              onClick={respondWithSessionRule}
              disabled={loading}
              className="border-muted-foreground/30"
              title="为当前会话创建一条 allow 规则"
            >
              本次会话允许
            </Button>
          )}
          <Button
            variant="outline"
            onClick={() => respond(true, true)}
            disabled={loading}
            className="border-muted-foreground/30"
            title="把工具加入全局白名单"
          >
            始终允许
          </Button>
          <AlertDialogAction asChild>
            <Button onClick={() => respond(true)} disabled={loading}>
              批准
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
