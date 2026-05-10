/**
 * PermissionsSettings — Settings → 工具权限 tab.
 *
 * Three sections:
 *   1. Global allow / block lists — legacy whole-tool whitelist + blocklist
 *      from `safety_policy.json`. Most users should NOT add to allow here:
 *      whitelisting `bash` means every `rm -rf` auto-passes. Use pattern
 *      rules (section 2) for granular trust. This section exists so the
 *      whitelist that previously drove "始终允许" is finally visible.
 *   2. Permission rules — V14 session + pattern rules (the granular tier).
 *   3. Audit log — most-recent decisions across all sessions.
 *
 * Live update: re-fetch on mount; manual refresh button.
 */

import * as React from 'react'
import { Trash2, Plus, RefreshCw, ShieldCheck, ShieldOff } from 'lucide-react'
import {
  listPermissionRules,
  createPermissionRule,
  deletePermissionRule,
  listPermissionAudit,
  getSafetyPolicy,
  removeAutoApprovedTool,
  unblockTool,
} from '@/lib/tauri-bridge'
import type {
  PermissionRule,
  PermissionAuditEntry,
  CreatePermissionRuleInput,
  SafetyPolicyResponse,
} from '@/lib/types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const MODE_BADGE: Record<string, { label: string; className: string }> = {
  allow: { label: '允许', className: 'bg-green-500/15 text-green-600 dark:text-green-400 border-green-500/30' },
  block: { label: '阻止', className: 'bg-red-500/15 text-red-600 dark:text-red-400 border-red-500/30' },
  ask:   { label: '询问', className: 'bg-yellow-500/15 text-yellow-600 dark:text-yellow-400 border-yellow-500/30' },
}

const DECISION_BADGE: Record<string, { label: string; className: string }> = {
  auto_approve:  { label: '自动允许', className: 'bg-green-500/10 text-green-600 dark:text-green-400' },
  user_approve:  { label: '用户允许', className: 'bg-blue-500/10 text-blue-600 dark:text-blue-400' },
  user_deny:     { label: '用户拒绝', className: 'bg-orange-500/10 text-orange-600 dark:text-orange-400' },
  blocked:       { label: '已阻止',   className: 'bg-red-500/10 text-red-600 dark:text-red-400' },
}

function formatTime(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

export function PermissionsSettings(): React.ReactElement {
  const [rules, setRules] = React.useState<PermissionRule[]>([])
  const [audit, setAudit] = React.useState<PermissionAuditEntry[]>([])
  const [policy, setPolicy] = React.useState<SafetyPolicyResponse | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [draft, setDraft] = React.useState<CreatePermissionRuleInput>({
    scope: 'pattern', toolName: '', target: '', mode: 'allow',
  })

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [r, a, p] = await Promise.all([
        listPermissionRules(),
        listPermissionAudit(undefined, 100),
        getSafetyPolicy(),
      ])
      setRules(r)
      setAudit(a)
      setPolicy(p)
    } finally {
      setLoading(false)
    }
  }, [])
  React.useEffect(() => { void refetch() }, [refetch])

  const onRemoveAllow = async (toolName: string) => {
    await removeAutoApprovedTool({ toolName })
    await refetch()
  }
  const onUnblock = async (toolName: string) => {
    await unblockTool({ toolName })
    await refetch()
  }

  const onAddRule = async () => {
    if (!draft.toolName.trim()) return
    await createPermissionRule({
      scope: draft.scope,
      sessionId: draft.scope === 'session' ? draft.sessionId : undefined,
      toolName: draft.toolName.trim(),
      target: draft.scope === 'pattern' ? (draft.target?.trim() || undefined) : undefined,
      mode: draft.mode,
    })
    setDraft({ scope: 'pattern', toolName: '', target: '', mode: 'allow' })
    await refetch()
  }

  const onDelete = async (id: string) => {
    await deletePermissionRule(id)
    await refetch()
  }

  const allowList = policy?.autoApprovedTools ?? []
  const blockList = policy?.blockedTools ?? []

  return (
    <div className="space-y-6 pb-8">
      {/* Global tier — legacy whole-tool whitelist + blocklist */}
      <section>
        <div className="mb-2.5 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            全局放行 / 阻止
          </h3>
          <Button size="sm" variant="ghost" onClick={() => void refetch()} disabled={loading} title="刷新">
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
        <p className="mb-2 text-[11.5px] text-muted-foreground/70 leading-relaxed">
          全局放行 = 该工具的<b>所有</b>调用自动通过（包括 <code className="px-1 rounded bg-muted/60">bash rm -rf</code> 这种）。粒度过粗，建议改用下方"权限规则"针对命令前缀放行。
        </p>
        <div className="grid grid-cols-2 gap-3">
          {/* Allow list */}
          <div className="rounded-lg border border-green-500/30 bg-green-500/5 p-2">
            <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-green-700 dark:text-green-400">
              <ShieldCheck className="size-3.5" />
              全局放行（auto-approve）
            </div>
            {allowList.length === 0 ? (
              <div className="text-[11.5px] text-muted-foreground/60 px-1 py-2">空</div>
            ) : (
              <ul className="space-y-1">
                {allowList.map((tool) => (
                  <li key={tool} className="flex items-center justify-between gap-2 px-1.5 py-1 rounded hover:bg-green-500/10 group">
                    <code className="font-mono text-[12px]">{tool}</code>
                    <Button
                      size="sm" variant="ghost"
                      onClick={() => void onRemoveAllow(tool)}
                      className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
                      title="移除"
                    >
                      <Trash2 className="size-3 text-muted-foreground/70" />
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </div>
          {/* Block list */}
          <div className="rounded-lg border border-red-500/30 bg-red-500/5 p-2">
            <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-red-700 dark:text-red-400">
              <ShieldOff className="size-3.5" />
              全局阻止（block）
            </div>
            {blockList.length === 0 ? (
              <div className="text-[11.5px] text-muted-foreground/60 px-1 py-2">空</div>
            ) : (
              <ul className="space-y-1">
                {blockList.map((tool) => (
                  <li key={tool} className="flex items-center justify-between gap-2 px-1.5 py-1 rounded hover:bg-red-500/10 group">
                    <code className="font-mono text-[12px]">{tool}</code>
                    <Button
                      size="sm" variant="ghost"
                      onClick={() => void onUnblock(tool)}
                      className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
                      title="解除"
                    >
                      <Trash2 className="size-3 text-muted-foreground/70" />
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </section>

      {/* Rules section */}
      <section>
        <div className="mb-2.5 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            权限规则
          </h3>
        </div>

        {/* Rule editor */}
        <div className="mb-3 rounded-lg border border-border/50 bg-muted/20 p-3 space-y-2">
          <div className="grid grid-cols-12 gap-2 text-[12px]">
            <select
              value={draft.scope}
              onChange={(e) => setDraft((d) => ({ ...d, scope: e.target.value as 'session' | 'pattern' }))}
              className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
            >
              <option value="pattern">模式</option>
              <option value="session">会话</option>
            </select>
            <input
              placeholder="tool_name (例如 bash)"
              value={draft.toolName}
              onChange={(e) => setDraft((d) => ({ ...d, toolName: e.target.value }))}
              className="col-span-3 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
            />
            <input
              placeholder={draft.scope === 'pattern' ? '目标前缀 (例如 git status)' : 'session_id'}
              value={draft.scope === 'pattern' ? (draft.target ?? '') : (draft.sessionId ?? '')}
              onChange={(e) => setDraft((d) => draft.scope === 'pattern'
                ? { ...d, target: e.target.value }
                : { ...d, sessionId: e.target.value })}
              className="col-span-4 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
            />
            <select
              value={draft.mode}
              onChange={(e) => setDraft((d) => ({ ...d, mode: e.target.value as 'allow' | 'block' | 'ask' }))}
              className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
            >
              <option value="allow">允许</option>
              <option value="block">阻止</option>
              <option value="ask">询问</option>
            </select>
            <Button size="sm" onClick={onAddRule} className="col-span-1" disabled={!draft.toolName.trim()}>
              <Plus className="size-3.5" />
            </Button>
          </div>
        </div>

        {/* Rules table */}
        <div className="rounded-lg border border-border/50 bg-muted/20 max-h-64 overflow-y-auto">
          {rules.length === 0 ? (
            <div className="p-6 text-center text-[12px] text-muted-foreground/60">
              {loading ? '加载中…' : '暂无规则'}
            </div>
          ) : (
            <table className="w-full text-[12px]">
              <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                <tr className="text-left text-muted-foreground/70">
                  <th className="px-3 py-2 font-normal">范围</th>
                  <th className="px-3 py-2 font-normal">工具</th>
                  <th className="px-3 py-2 font-normal">目标 / 会话</th>
                  <th className="px-3 py-2 font-normal">模式</th>
                  <th className="px-3 py-2 font-normal w-8" />
                </tr>
              </thead>
              <tbody>
                {rules.map((r) => {
                  const mb = MODE_BADGE[r.mode] ?? MODE_BADGE.ask
                  return (
                    <tr key={r.id} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5">{r.scope === 'session' ? '会话' : '模式'}</td>
                      <td className="px-3 py-1.5 font-mono">{r.toolName}</td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/85 truncate max-w-[200px]">
                        {r.target ?? r.sessionId ?? ''}
                      </td>
                      <td className="px-3 py-1.5">
                        <span className={cn('inline-flex items-center rounded border px-1.5 py-0.5 text-[10.5px]', mb.className)}>
                          {mb.label}
                        </span>
                      </td>
                      <td className="px-3 py-1.5">
                        <Button size="sm" variant="ghost" onClick={() => void onDelete(r.id)} className="h-6 w-6 p-0">
                          <Trash2 className="size-3 text-muted-foreground/70" />
                        </Button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </div>
      </section>

      {/* Audit log */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          审计日志（最近 100 条）
        </h3>
        <div className="rounded-lg border border-border/50 bg-muted/20 max-h-72 overflow-y-auto">
          {audit.length === 0 ? (
            <div className="p-6 text-center text-[12px] text-muted-foreground/60">
              {loading ? '加载中…' : '暂无审计记录'}
            </div>
          ) : (
            <table className="w-full text-[12px]">
              <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                <tr className="text-left text-muted-foreground/70">
                  <th className="px-3 py-2 font-normal">时间</th>
                  <th className="px-3 py-2 font-normal">工具</th>
                  <th className="px-3 py-2 font-normal">会话</th>
                  <th className="px-3 py-2 font-normal">参数 hash</th>
                  <th className="px-3 py-2 font-normal">决定</th>
                </tr>
              </thead>
              <tbody>
                {audit.map((a) => {
                  const db = DECISION_BADGE[a.decision] ?? { label: a.decision, className: 'bg-muted text-muted-foreground' }
                  return (
                    <tr key={a.id} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5 text-muted-foreground/70 tabular-nums">{formatTime(a.createdAt)}</td>
                      <td className="px-3 py-1.5 font-mono">{a.toolName}</td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/70 truncate max-w-[100px]">
                        {a.sessionId.slice(0, 8)}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground/70">{a.argsHash}</td>
                      <td className="px-3 py-1.5">
                        <span className={cn('inline-flex items-center rounded px-1.5 py-0.5 text-[10.5px]', db.className)}>
                          {db.label}
                        </span>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </div>
      </section>
    </div>
  )
}
