/**
 * Context continuity P3 — a muted inline notice in the agent message stream,
 * shown after the user message of a turn whose `agent:prompt_frame` event
 * reports a visible change: the session's tool set changed (`<tools_update>`),
 * the permission mode switched (`<mode_update>`), or the frozen system prompt
 * frame was rebuilt (template / workspace / app version — 'new' is silent).
 *
 * The context meter (ContextUsageBadge) shows the CURRENT frame's rebuild
 * reason as status; this notice marks WHEN the change happened in the
 * conversation. Runtime-only (see promptFrameEventsAtom).
 */
import * as React from 'react'
import { Info } from 'lucide-react'
import type { PromptFrameEvent } from '@/lib/types'
import type { AgentMessage } from '@/lib/agent-types'
import type { PromptFrameNoticeEntry } from '@/atoms/agent-atoms'

/** Backend snake_case mode names (`session_frame::mode_name`) → 中文名 */
const MODE_TEXT: Record<string, string> = {
  ask: '询问权限',
  accept_edits: '自动接受编辑',
  plan: '计划模式',
  supervised: '自动模式',
  yolo: '跳过权限确认',
}

const REBUILD_REASON_TEXT: Record<string, string> = {
  prompt_changed: '提示词模板变化',
  workspace_changed: '工作区变化',
  version_changed: '应用升级',
}

const TOOL_NAMES_SHOWN = 3
const CACHE_SUFFIX = '—— 本轮缓存重建一次'

function toolSegment(verb: string, names: string[]): string {
  const shown = names.slice(0, TOOL_NAMES_SHOWN).join('、')
  const more = names.length > TOOL_NAMES_SHOWN ? '…' : ''
  return `${verb} ${names.length} 个（${shown}${more}）`
}

/** The notice lines for one event, in display order (empty → nothing to show). */
export function promptFrameNoticeLines(event: PromptFrameEvent): string[] {
  const lines: string[] = []
  const added = event.toolsAdded ?? []
  const removed = event.toolsRemoved ?? []
  if (added.length > 0 || removed.length > 0) {
    const parts: string[] = []
    if (added.length > 0) parts.push(toolSegment('新增', added))
    if (removed.length > 0) parts.push(toolSegment('移除', removed))
    lines.push(`工具集已变化：${parts.join('· ')}${CACHE_SUFFIX}`)
  }
  if (event.modeChanged) {
    const to = event.modeChanged.to
    lines.push(`权限模式已切换为「${MODE_TEXT[to] ?? to}」，已告知模型`)
  }
  if (event.rebuilt && event.rebuildReason && event.rebuildReason !== 'new') {
    const reason = REBUILD_REASON_TEXT[event.rebuildReason] ?? event.rebuildReason
    lines.push(`系统提示词已重建（${reason}）${CACHE_SUFFIX}`)
  }
  return lines
}

export function PromptFrameNotice({ event }: { event: PromptFrameEvent }): React.ReactElement | null {
  const lines = promptFrameNoticeLines(event)
  if (lines.length === 0) return null
  return (
    <div role="note" className="flex flex-col gap-0.5 my-2 pl-[46px] text-[12px] text-muted-foreground">
      {lines.map((line) => (
        <div key={line} className="flex items-center gap-1.5">
          <Info className="size-3 shrink-0" />
          <span>{line}</span>
        </div>
      ))}
    </div>
  )
}

/**
 * Map user-message id → the notices of the turn it started: each notice goes
 * after the last user message created at or before the notice arrived (the
 * backend writes the user row before emitting `agent:prompt_frame`). Notices
 * older than every user message are dropped.
 */
export function anchorPromptFrameNotices(
  messages: readonly AgentMessage[],
  entries: readonly PromptFrameNoticeEntry[],
): Map<string, PromptFrameNoticeEntry[]> {
  const out = new Map<string, PromptFrameNoticeEntry[]>()
  if (entries.length === 0) return out
  const users = messages.filter((m) => m.role === 'user')
  for (const e of entries) {
    let anchor: AgentMessage | undefined
    for (const u of users) {
      if (u.createdAt <= e.receivedAt && (!anchor || u.createdAt >= anchor.createdAt)) anchor = u
    }
    if (!anchor) continue
    const list = out.get(anchor.id) ?? []
    list.push(e)
    out.set(anchor.id, list)
  }
  return out
}
