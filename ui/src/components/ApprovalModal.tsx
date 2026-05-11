/**
 * ApprovalModal - 工具调用审批弹窗
 *
 * 当后端请求审批时弹出，展示工具名称、参数、风险等级，
 * 提供"批准"/"拒绝"/"本次会话允许"/"始终允许"操作。
 *
 * "始终允许" semantics differ by tool shape:
 *   - When `request.command` is present (e.g. bash) → create a V14 pattern
 *     rule with target = the exact command. Matcher uses prefix matching, so
 *     this allows the same command + longer ones starting with it but does
 *     NOT whitelist the whole tool. Avoids the "click 始终允许 on `bash ls`
 *     and now `bash rm -rf /` auto-passes too" footgun.
 *   - Without a command (read_file, write_file, etc.) → legacy
 *     `alwaysAllow=true` adds the whole tool to the global whitelist. These
 *     tools are uniform enough that "trust this tool" is the right concept.
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
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { ShieldAlert, ShieldCheck, ShieldOff, AlertTriangle, ChevronRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import { approveToolCall, onNeedApproval, createPermissionRule } from '@/lib/tauri-bridge'
import type { ApprovalRequest } from '@/lib/types'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'

/**
 * Risk-level visual config — uses semantic theme tokens (success / warning /
 * danger) instead of hardcoded Tailwind colors. CLAUDE.md flags hardcoded
 * `bg-yellow-500` / `text-red-500` patterns as the #1 source of theme
 * breakage; the success/warning/danger trio is defined per-theme in
 * globals.css and wired into tailwind.config.js so it stays consistent
 * under warm-paper / qingye / forest-* / etc.
 */
const RISK_CONFIG: Record<string, {
  label: string
  icon: React.ElementType
  /** Background tint for the hero icon + badge. */
  tint: string
  /** Foreground text color for the badge. */
  text: string
  /** Border accent for the request detail card. */
  border: string
  /** Vertical accent bar color at the start of the request card. */
  bar: string
}> = {
  low: {
    label: '低风险',
    icon: ShieldCheck,
    tint: 'bg-success-bg',
    text: 'text-success',
    border: 'border-success/30',
    bar: 'bg-success',
  },
  medium: {
    label: '中风险',
    icon: ShieldAlert,
    tint: 'bg-warning-bg',
    text: 'text-warning',
    border: 'border-warning/30',
    bar: 'bg-warning',
  },
  high: {
    label: '高风险',
    icon: AlertTriangle,
    tint: 'bg-danger-bg',
    text: 'text-danger',
    border: 'border-danger/30',
    bar: 'bg-danger',
  },
  critical: {
    label: '严重风险',
    icon: ShieldOff,
    tint: 'bg-danger-bg',
    text: 'text-danger',
    border: 'border-danger/50',
    bar: 'bg-danger',
  },
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
      // Same logic as 始终允许 (PR #43): if the call has a command (bash),
      // narrow the rule to that command prefix. Without target, a session
      // rule for `bash` would auto-pass every bash call in this session
      // including `rm -rf /` — same footgun as a global whitelist.
      const command = request.command?.trim()
      await createPermissionRule({
        scope: 'session',
        sessionId: activeSessionId,
        toolName: request.toolName,
        target: command || undefined,
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

  const respondPath = async (scope: 'once' | 'session' | 'deny'): Promise<void> => {
    if (!request || loading) return
    setLoading(true)
    try {
      await approveToolCall({
        sessionId: request.sessionId,
        toolId: request.toolId,
        approved: scope !== 'deny',
        pathScope: scope,
        paths: scope === 'session' ? request.paths : undefined,
      })
    } catch (err) {
      console.error('[ApprovalModal] path-approval failed:', err)
    } finally {
      setLoading(false)
      setRequest(null)
    }
  }

  /**
   * "始终允许" handler — chooses between pattern rule (when the call has a
   * command, like bash) and legacy whole-tool whitelist (everything else).
   */
  const respondAlwaysAllow = async (): Promise<void> => {
    if (!request || loading) return
    setLoading(true)
    try {
      const command = request.command?.trim()
      if (command) {
        // Granular: V14 pattern rule, target = exact command (prefix match
        // means future "<command> <more args>" also passes).
        await createPermissionRule({
          scope: 'pattern',
          toolName: request.toolName,
          target: command,
          mode: 'allow',
        })
        await approveToolCall({
          sessionId: request.sessionId,
          toolId: request.toolId,
          approved: true,
          alwaysAllow: false,
        })
      } else {
        // Legacy: whole-tool whitelist via SafetyManager.add_auto_approved.
        await approveToolCall({
          sessionId: request.sessionId,
          toolId: request.toolId,
          approved: true,
          alwaysAllow: true,
        })
      }
    } catch (err) {
      console.error('[ApprovalModal] always-allow failed:', err)
    } finally {
      setLoading(false)
      setRequest(null)
    }
  }

  const hasCommand = !!request?.command?.trim()

  const risk = getRiskConfig(request?.riskLevel)
  const RiskIcon = risk.icon

  const argsEntries = request?.arguments ? Object.entries(request.arguments) : []

  // Shared hero header — risk-tinted icon disc + title + supporting line.
  // Used by both the path-approval and tool-call variants.
  const Hero = ({ title, subtitle }: { title: string; subtitle: string }) => (
    <div className="flex items-start gap-3 mb-1">
      <div
        className={cn(
          'shrink-0 inline-flex items-center justify-center size-10 rounded-xl',
          risk.tint, risk.text,
        )}
        aria-hidden
      >
        <RiskIcon className="size-5" />
      </div>
      <div className="min-w-0 flex-1">
        <AlertDialogTitle className="text-base font-semibold leading-tight">
          {title}
        </AlertDialogTitle>
        <AlertDialogDescription className="text-[12.5px] mt-0.5 text-muted-foreground">
          {subtitle}
        </AlertDialogDescription>
      </div>
      <span
        className={cn(
          'shrink-0 inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium',
          'border', risk.border, risk.tint, risk.text,
        )}
      >
        {risk.label}
      </span>
    </div>
  )

  if (request && request.kind === 'path') {
    return (
      <AlertDialog open={!!request} onOpenChange={(open) => { if (!open) setRequest(null) }}>
        <AlertDialogContent className="sm:max-w-lg overflow-hidden p-0">
          {/* Risk accent bar — 2px stripe across the top of the dialog
              ties the chrome to the request's risk severity. */}
          <div className={cn('h-[2px] w-full', risk.bar)} aria-hidden />

          <div className="p-5 space-y-4">
            <AlertDialogHeader>
              <Hero
                title="外部路径访问请求"
                subtitle={request.reason ?? '工具请求访问工作区以外的路径。'}
              />
            </AlertDialogHeader>

            <div className="space-y-1.5">
              <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/80">
                请求的路径 · {(request.paths ?? []).length} 项
              </p>
              <ScrollArea className="max-h-40">
                <div className="rounded-md border border-border/60 bg-muted/40 p-2 space-y-0.5">
                  {(request.paths ?? []).map((p) => (
                    <div
                      key={p}
                      className="font-mono text-[12px] text-foreground/85 truncate px-1"
                      title={p}
                    >
                      {p}
                    </div>
                  ))}
                </div>
              </ScrollArea>
            </div>

            <AlertDialogFooter className="flex-row gap-2 flex-wrap sm:justify-end">
              <AlertDialogCancel asChild>
                <Button
                  variant="ghost"
                  onClick={() => respondPath('deny')}
                  disabled={loading}
                  className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                >
                  拒绝
                </Button>
              </AlertDialogCancel>
              <Button
                variant="outline"
                onClick={() => respondPath('session')}
                disabled={loading}
                title="本会话内,这些路径不再提示"
              >
                本会话允许
              </Button>
              <AlertDialogAction asChild>
                <Button onClick={() => respondPath('once')} disabled={loading}>
                  仅此一次
                  <ChevronRight className="size-3.5 -mr-1 opacity-70" />
                </Button>
              </AlertDialogAction>
            </AlertDialogFooter>
          </div>
        </AlertDialogContent>
      </AlertDialog>
    )
  }

  return (
    <AlertDialog open={!!request} onOpenChange={(open) => { if (!open) setRequest(null) }}>
      <AlertDialogContent className="sm:max-w-lg overflow-hidden p-0">
        {/* Risk accent bar — same treatment as the path variant. */}
        <div className={cn('h-[2px] w-full', risk.bar)} aria-hidden />

        <div className="p-5 space-y-4">
          <AlertDialogHeader>
            <Hero
              title="工具调用审批"
              subtitle="AI 请求执行以下工具，请确认是否允许。"
            />
          </AlertDialogHeader>

          {request && (
            <div className="space-y-3">
              {/* Request detail card — risk-tinted left bar + tool name + */}
              {/* optional command/parameters in a unified surface. */}
              <div className={cn(
                'relative rounded-lg border border-border/60 bg-muted/30',
                'overflow-hidden',
              )}>
                {/* Left accent bar tied to risk severity */}
                <div className={cn('absolute left-0 top-0 bottom-0 w-[3px]', risk.bar)} aria-hidden />

                <div className="pl-4 pr-3 py-3 space-y-3">
                  <div className="flex items-center gap-2">
                    <span className="text-[11px] uppercase tracking-wide text-muted-foreground/80 shrink-0">
                      工具
                    </span>
                    <code className="text-[13px] font-mono font-medium text-foreground bg-foreground/[0.06] px-1.5 py-0.5 rounded">
                      {request.toolName}
                    </code>
                  </div>

                  {request.command && (
                    <div className="space-y-1">
                      <p className="text-[11px] uppercase tracking-wide text-muted-foreground/80">
                        命令
                      </p>
                      <pre className="text-[12px] font-mono bg-foreground/[0.04] border border-border/40 rounded-md px-2.5 py-2 overflow-x-auto whitespace-pre-wrap break-all text-foreground/90">
                        {request.command}
                      </pre>
                    </div>
                  )}

                  {argsEntries.length > 0 && (
                    <div className="space-y-1">
                      <p className="text-[11px] uppercase tracking-wide text-muted-foreground/80">
                        参数 · {argsEntries.length} 项
                      </p>
                      <ScrollArea className="max-h-40">
                        <div className="space-y-1 font-mono text-[12px]">
                          {argsEntries.map(([key, val]) => (
                            <div key={key} className="flex gap-2 leading-relaxed">
                              <span className="text-muted-foreground/70 shrink-0 min-w-[5rem]">
                                {key}
                              </span>
                              <span className="text-foreground/90 break-all flex-1">
                                {typeof val === 'string' ? val : JSON.stringify(val)}
                              </span>
                            </div>
                          ))}
                        </div>
                      </ScrollArea>
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          <AlertDialogFooter className="flex-row gap-2 flex-wrap sm:justify-end">
            <AlertDialogCancel asChild>
              <Button
                variant="ghost"
                onClick={() => respond(false)}
                disabled={loading}
                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
              >
                拒绝
              </Button>
            </AlertDialogCancel>
            {activeSessionId && (
              <Button
                variant="outline"
                onClick={respondWithSessionRule}
                disabled={loading}
                title="为当前会话创建一条 allow 规则"
              >
                本次会话允许
              </Button>
            )}
            <Button
              variant="outline"
              onClick={respondAlwaysAllow}
              disabled={loading}
              title={hasCommand
                ? `为命令 "${request?.command}" 创建放行规则（匹配该命令前缀）`
                : '把工具加入全局白名单（所有调用自动通过）'}
            >
              {hasCommand ? '始终允许这条命令' : '始终允许'}
            </Button>
            <AlertDialogAction asChild>
              <Button onClick={() => respond(true)} disabled={loading}>
                批准
                <ChevronRight className="size-3.5 -mr-1 opacity-70" />
              </Button>
            </AlertDialogAction>
          </AlertDialogFooter>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  )
}
