/**
 * Phase 2b cluster A — per-spec chat threads tab.
 *
 * Lists every (spec, identity) chat session sourced from
 * `automation_chat_sessions`. Identities are either "local" (the spec
 * owner) or "{channelType}:{chatId}" (one per IM user that has triggered
 * the spec). The leading icon swaps to the channel logo for IM rows,
 * mirroring the WorkspaceRail / TabBar convention from PR #189.
 *
 * Read-only for now — opening a thread as a tab is a follow-up PR (the
 * tab-open helper signature differs by surface). The list itself proves
 * the (spec, identity) → session data plumbing is complete.
 */

import { useEffect } from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { chatSessionsBySpecAtom, refreshChatSessionsAtom } from '@/atoms/automation'
import { imChannelDisplay } from '@/lib/im-channel-display'

interface Props {
  specId: string
}

export function ChatThreadsTab({ specId }: Props): React.ReactElement {
  const map = useAtomValue(chatSessionsBySpecAtom)
  const refresh = useSetAtom(refreshChatSessionsAtom)
  const sessions = map[specId] ?? []

  useEffect(() => {
    void refresh(specId)
  }, [specId, refresh])

  if (sessions.length === 0) {
    return (
      <div className="text-sm text-muted-foreground p-4">
        暂无 chat 线。spec owner 手动触发，或绑定的 IM 用户首次发出 trigger phrase 时，
        会在这里自动出现一条线。
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-0.5 p-2 overflow-y-auto">
      {sessions.map((s) => {
        // identity_key: "local" | "{channelType}:{chatId}"
        const colonIdx = s.identityKey.indexOf(':')
        const [channelType, chatId] =
          s.identityKey === 'local' || colonIdx < 0
            ? [null, null]
            : [s.identityKey.slice(0, colonIdx), s.identityKey.slice(colonIdx + 1)]
        const channel = channelType ? imChannelDisplay(channelType) : null

        return (
          <div
            key={s.agentSessionId}
            className="flex items-center gap-2 w-full px-2 py-1.5 rounded text-left"
            title={`agent_session: ${s.agentSessionId}`}
          >
            <span className="shrink-0 w-4 h-4 inline-flex items-center justify-center">
              {channel?.logoSrc ? (
                <img
                  src={channel.logoSrc}
                  alt={channel.label}
                  className="w-3.5 h-3.5 object-contain rounded-sm"
                  draggable={false}
                />
              ) : channel ? (
                <span className="text-[12px] leading-none">{channel.emoji}</span>
              ) : (
                <span className="text-[12px] leading-none">💬</span>
              )}
            </span>
            <span className="flex-1 truncate text-sm">
              {s.identityKey === 'local' ? '本地 owner' : (chatId ?? s.identityKey)}
            </span>
            <span className="text-xs text-muted-foreground shrink-0">
              {s.messageCount} 条
            </span>
          </div>
        )
      })}
    </div>
  )
}
