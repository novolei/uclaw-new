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
import {
  ShieldAlert, ShieldCheck, ShieldOff, AlertTriangle, ChevronRight,
  Wrench, Terminal, Sliders, Globe, FileText, FileCog, Cpu,
} from 'lucide-react'
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

/**
 * Tool category — used for the OVERVIEW stat cards at the bottom of the
 * modal. Pure-presentation classification by tool name; no backend
 * dependency. Falls back to "工具" / Wrench for unknown tools.
 */
function getToolCategory(toolName: string): { label: string; icon: React.ElementType } {
  const n = toolName.toLowerCase()
  if (n.includes('bash') || n.includes('shell') || n === 'exec') {
    return { label: 'Shell', icon: Terminal }
  }
  if (n.includes('web') || n.includes('fetch') || n.includes('http')) {
    return { label: '网络请求', icon: Globe }
  }
  if (n.includes('write') || n.includes('edit') || n.includes('create')) {
    return { label: '文件写入', icon: FileCog }
  }
  if (n.includes('read') || n.includes('grep') || n.includes('glob') || n.includes('list')) {
    return { label: '文件读取', icon: FileText }
  }
  if (n.includes('python') || n.includes('eval') || n.includes('run')) {
    return { label: '代码执行', icon: Cpu }
  }
  return { label: '工具调用', icon: Wrench }
}

/**
 * Effect summary — derived presentation hint for the third OVERVIEW card.
 * Surfaces whether this call reads, mutates, or reaches outside the box.
 */
function getEffectSummary(toolName: string, hasCommand: boolean): string {
  const n = toolName.toLowerCase()
  if (n.includes('write') || n.includes('edit') || hasCommand) return '可变副作用'
  if (n.includes('web') || n.includes('fetch') || n.includes('http')) return '外部资源'
  return '只读操作'
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
  // Pattern: bold left, status pill right (matches the "Delivery timeline"
  // reference's header bar).
  const Hero = ({ title, subtitle }: { title: string; subtitle: string }) => (
    <div className="flex items-start gap-3">
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
        <AlertDialogTitle className="text-base font-semibold leading-tight text-foreground">
          {title}
        </AlertDialogTitle>
        <AlertDialogDescription className="text-[12.5px] mt-0.5 text-muted-foreground">
          {subtitle}
        </AlertDialogDescription>
      </div>
      <span
        className={cn(
          'shrink-0 inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10.5px] font-semibold uppercase tracking-wide',
          risk.tint, risk.text,
        )}
      >
        <span className={cn('size-1.5 rounded-full', risk.bar)} aria-hidden />
        {risk.label}
      </span>
    </div>
  )

  /**
   * Body row — left bullet + label + tiny status pill + value content.
   * Style adapted from the reference timeline's per-step rhythm: small
   * colored circle on the left, bold label, uppercase status pill, then
   * the data on the next line.
   */
  const Row = ({
    bullet, label, pill, pillTint, pillText, children,
  }: {
    bullet: 'risk' | 'neutral'
    label: string
    pill?: string
    pillTint?: string
    pillText?: string
    children: React.ReactNode
  }) => (
    <div className="flex items-start gap-3">
      <div className="relative shrink-0 pt-1">
        <span
          className={cn(
            'block size-2.5 rounded-full',
            bullet === 'risk' ? risk.bar : 'bg-muted-foreground/40',
          )}
          aria-hidden
        />
      </div>
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[12px] font-semibold text-foreground">{label}</span>
          {pill && (
            <span className={cn(
              'inline-flex items-center rounded-full px-1.5 py-0.5',
              'text-[10px] font-semibold uppercase tracking-wide',
              pillTint ?? 'bg-muted',
              pillText ?? 'text-muted-foreground',
            )}>
              {pill}
            </span>
          )}
        </div>
        <div className="text-[12.5px] text-foreground/85">{children}</div>
      </div>
    </div>
  )

  /**
   * Overview stat card — borrowed from the reference's bottom-summary
   * cards. Three of them in a row summarize: risk class / tool category
   * / effect. Each card is color-tinted only when it carries semantic
   * meaning (risk uses the risk palette; others stay neutral so they
   * don't compete).
   */
  const StatCard = ({
    label, value, icon: Icon, tone,
  }: {
    label: string
    value: string
    icon: React.ElementType
    tone: 'risk' | 'neutral'
  }) => (
    <div className={cn(
      'flex-1 rounded-lg border px-3 py-2.5',
      tone === 'risk'
        ? cn(risk.tint, risk.border)
        : 'bg-muted/40 border-border/60',
    )}>
      <div className="flex items-center gap-1.5">
        <Icon className={cn('size-3', tone === 'risk' ? risk.text : 'text-muted-foreground')} />
        <span className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
          {label}
        </span>
      </div>
      <div className={cn(
        'mt-1 text-[13px] font-semibold',
        tone === 'risk' ? risk.text : 'text-foreground',
      )}>
        {value}
      </div>
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

            <div className="space-y-3">
              <Row
                bullet="risk"
                label="请求的路径"
                pill={`${(request.paths ?? []).length} 项`}
                pillTint={risk.tint}
                pillText={risk.text}
              >
                <ScrollArea className="max-h-40">
                  <div className="rounded-md border border-border/40 bg-foreground/[0.04] p-2 space-y-0.5">
                    {(request.paths ?? []).map((p) => (
                      <div
                        key={p}
                        className="font-mono text-[12px] text-foreground/90 truncate px-1"
                        title={p}
                      >
                        {p}
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              </Row>
            </div>

            {/* OVERVIEW for path-approval — only 2 cards since there's no
                tool/command context, just the scope of the request. */}
            <div className="pt-2 border-t border-border/40">
              <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70 mb-2">
                概览
              </p>
              <div className="flex gap-2">
                <StatCard
                  label="风险等级"
                  value={risk.label}
                  icon={RiskIcon}
                  tone="risk"
                />
                <StatCard
                  label="类型"
                  value="路径访问"
                  icon={FileText}
                  tone="neutral"
                />
                <StatCard
                  label="影响"
                  value={`${(request.paths ?? []).length} 条路径`}
                  icon={Sliders}
                  tone="neutral"
                />
              </div>
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

          {request && (() => {
            const toolCat = getToolCategory(request.toolName)
            const effect = getEffectSummary(request.toolName, !!request.command)
            return (
              <>
                {/* ===== Detail rows ===== */}
                <div className="space-y-3">
                  <Row
                    bullet="risk"
                    label="工具"
                    pill={toolCat.label}
                    pillTint={risk.tint}
                    pillText={risk.text}
                  >
                    <code className="font-mono font-medium text-[13px] text-foreground">
                      {request.toolName}
                    </code>
                  </Row>

                  {request.command && (
                    <Row
                      bullet="neutral"
                      label="命令"
                      pill="Shell"
                    >
                      <pre className="font-mono text-[12px] bg-foreground/[0.04] border border-border/40 rounded-md px-2.5 py-2 overflow-x-auto whitespace-pre-wrap break-all text-foreground/90">
                        {request.command}
                      </pre>
                    </Row>
                  )}

                  {argsEntries.length > 0 && (
                    <Row
                      bullet="neutral"
                      label="参数"
                      pill={`${argsEntries.length} 项`}
                    >
                      <ScrollArea className="max-h-40">
                        <div className="font-mono text-[12px] space-y-0.5 pr-2">
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
                    </Row>
                  )}
                </div>

                {/* ===== OVERVIEW stat row ===== */}
                <div className="pt-2 border-t border-border/40">
                  <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70 mb-2">
                    概览
                  </p>
                  <div className="flex gap-2">
                    <StatCard
                      label="风险等级"
                      value={risk.label}
                      icon={RiskIcon}
                      tone="risk"
                    />
                    <StatCard
                      label="类型"
                      value={toolCat.label}
                      icon={toolCat.icon}
                      tone="neutral"
                    />
                    <StatCard
                      label="影响"
                      value={effect}
                      icon={Sliders}
                      tone="neutral"
                    />
                  </div>
                </div>
              </>
            )
          })()}

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
