// Bundle 27-A — Live agent heartbeat + stall banner + interrupted-reply
// recovery banner. One component owns all three states because they
// share the same event source and the same "show only for THIS
// session" guard.
//
// Three event sources from the backend (see
// src-tauri/src/agent/heartbeat.rs + recovery.rs):
//
//   - `agent:heartbeat`       — every 5s while a run is active.
//   - `agent:stalled`         — fires once when no activity for ≥30s.
//   - `agent:stall-recovered` — fires when activity resumes after a stall.
//   - `agent:interrupted-recovered` — fires once at boot if Bundle
//     27-C detected an unclean shutdown and Bundle 27-A's flight
//     recorder has buffered text from the dead run.
//
// Visuals: minimal, status-bar-style. Heartbeat lives as a tiny chip
// that only renders during a live run; stall is a yellow banner that
// blocks nothing but offers [中断并保存] / [继续等待]; recovery is a
// neutral banner with the recovered text inline.

import * as React from 'react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

interface HeartbeatPayload {
  conversationId: string
  iteration: number
  stage: string
  lastActivityMsAgo: number
  partialChars: number
  timestamp: number
}

interface StalledPayload {
  conversationId: string
  iteration: number
  stage: string
  stalledForMs: number
  partialChars: number
  timestamp: number
}

interface RecoveryPayload {
  conversationId: string
  spaceId: string
  iteration: number
  stage: string
  startedAt: number
  lastActivityAt: number
  partialText: string
  partialChars: number
  deadPid: number
}

type Beat = HeartbeatPayload | null
type StallState =
  | { kind: 'none' }
  | { kind: 'stalled'; data: StalledPayload }

interface AgentHeartbeatBannerProps {
  sessionId: string
}

// Translate stage labels into a brief human-readable hint so the user
// understands WHERE the agent is currently working. Falls through to
// raw stage for anything not pre-mapped.
function stageHint(stage: string): string {
  if (stage.startsWith('tool_call:')) {
    const tool = stage.slice('tool_call:'.length)
    return `正在调用工具 ${tool}`
  }
  switch (stage) {
    case 'starting':
      return '准备中'
    case 'llm_call':
      return '正在请求 LLM'
    case 'llm_stream':
      return '正在接收 LLM 流式响应'
    case 'thinking':
      return '正在推理'
    case 'tool_call':
      return '正在调用工具'
    case 'done':
      return '已完成'
    default:
      return stage
  }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const s = Math.floor(ms / 1000)
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  return `${m}m${s % 60}s`
}

export function AgentHeartbeatBanner({ sessionId }: AgentHeartbeatBannerProps) {
  const [beat, setBeat] = React.useState<Beat>(null)
  const [stall, setStall] = React.useState<StallState>({ kind: 'none' })
  const [recovery, setRecovery] = React.useState<RecoveryPayload | null>(null)
  // Whether the recovery banner is dismissed for this session.
  const [recoveryDismissed, setRecoveryDismissed] = React.useState(false)
  // Pending action — disables buttons while invoking the backend.
  const [interrupting, setInterrupting] = React.useState(false)

  // Listen to all four events. Each useEffect returns the unlisten fn
  // so React's cleanup handles tear-down on session change / unmount.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    listen<HeartbeatPayload>('agent:heartbeat', (e) => {
      if (e.payload.conversationId !== sessionId) return
      setBeat(e.payload)
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    listen<StalledPayload>('agent:stalled', (e) => {
      if (e.payload.conversationId !== sessionId) return
      setStall({ kind: 'stalled', data: e.payload })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    listen<{ conversationId: string }>('agent:stall-recovered', (e) => {
      if (e.payload.conversationId !== sessionId) return
      setStall({ kind: 'none' })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Listen to recovery events globally, then match on conversationId.
  // Combined push (event) + pull (invoke on mount) — see Bundle 27-A2.
  // The event-only push was unreliable because boot-time emit (500ms
  // after Tauri setup) can race with React mount in dev mode. The
  // pull-on-mount path queries backend AppState directly, so it
  // works even if the banner mounts AFTER the event fired.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    listen<RecoveryPayload>('agent:interrupted-recovered', (e) => {
      if (e.payload.conversationId !== sessionId) return
      setRecovery(e.payload)
      setRecoveryDismissed(false)
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Bundle 27-A2 — pull-model recovery. On mount (or when sessionId
  // changes), ask the backend "is there a pending recovery for THIS
  // session?". If yes, render the banner. The backend's consume_*
  // command is one-shot — first caller with the matching session_id
  // wins.
  React.useEffect(() => {
    let cancelled = false
    invoke('consume_pending_recovery', { sessionId })
      .then((payload) => {
        if (cancelled || !payload) return
        // payload shape matches RecoveryPayload (camelCase from JSON).
        setRecovery(payload as RecoveryPayload)
        setRecoveryDismissed(false)
      })
      .catch((err) => {
        // eslint-disable-next-line no-console
        console.error('[Bundle 27-A2] consume_pending_recovery failed', err)
      })
    return () => {
      cancelled = true
    }
  }, [sessionId])

  // Listen for stream completion — clear the heartbeat indicator
  // immediately when the agent run ends, rather than waiting for the
  // 15s stale-fade timer. Bundle 27-A initial draft only had the
  // stale-fade; users found the 15s lag confusing because the
  // streaming text was already done. `chat:stream-complete` is the
  // canonical "run finished" signal from dispatcher::emit_done.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    listen<{ conversationId: string }>('chat:stream-complete', (e) => {
      if (e.payload.conversationId !== sessionId) return
      setBeat(null)
      setStall({ kind: 'none' })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Heartbeat auto-fades when stale (no event in > 15s = run probably
  // ended; backend's `agent:stream-complete` would normally clear it
  // by emitting `done` stage first, but we belt-and-suspenders here).
  React.useEffect(() => {
    if (!beat) return
    const handle = setTimeout(() => setBeat(null), 15_000)
    return () => clearTimeout(handle)
  }, [beat])

  const handleInterrupt = React.useCallback(async () => {
    if (interrupting) return
    setInterrupting(true)
    try {
      // Backend cancels the run + returns the recovered partial text
      // payload. We treat it the same as an `agent:interrupted-
      // recovered` event so the UI converges.
      const payload = (await invoke('interrupt_current_agent_run', {
        sessionId,
      })) as {
        partialText: string
        iteration: number
        stage: string
        stalledForMs: number
        startedAt: number
      }
      if (payload?.partialText) {
        setRecovery({
          conversationId: sessionId,
          spaceId: '',
          iteration: payload.iteration,
          stage: payload.stage,
          startedAt: payload.startedAt,
          lastActivityAt: 0,
          partialText: payload.partialText,
          partialChars: payload.partialText.length,
          deadPid: 0,
        })
        setRecoveryDismissed(false)
      }
      setStall({ kind: 'none' })
    } catch (err) {
      // Surface to console — Tauri command errors are rare and we
      // don't want to block the user; the run will eventually
      // terminate via the stop_agent path even without this.
      // eslint-disable-next-line no-console
      console.error('[Bundle 27-A] interrupt_current_agent_run failed', err)
    } finally {
      setInterrupting(false)
    }
  }, [interrupting, sessionId])

  const handleKeepWaiting = React.useCallback(() => {
    setStall({ kind: 'none' })
  }, [])

  // ── Render ──────────────────────────────────────────────────────────

  return (
    <>
      {/* Boot-time recovery banner — neutral, dismissible */}
      {recovery && !recoveryDismissed && (
        <div
          role="status"
          style={{
            margin: '8px 12px',
            padding: '10px 12px',
            background: 'var(--color-surface-2, #f3f4f6)',
            border: '1px solid var(--color-border, #e5e7eb)',
            borderLeft: '3px solid #6b7280',
            borderRadius: 6,
            fontSize: 13,
            lineHeight: 1.5,
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'flex-start',
              gap: 8,
            }}
          >
            <strong>上一轮被异常中断 — 已恢复部分回复</strong>
            <button
              type="button"
              onClick={() => {
                setRecoveryDismissed(true)
                // Bundle 27-A2 — also tell backend to drop the payload
                // so it doesn't reappear on next mount.
                invoke('dismiss_pending_recovery').catch((err) => {
                  // eslint-disable-next-line no-console
                  console.error('[Bundle 27-A2] dismiss_pending_recovery failed', err)
                })
              }}
              style={{
                background: 'transparent',
                border: 'none',
                cursor: 'pointer',
                fontSize: 18,
                lineHeight: 1,
                opacity: 0.6,
              }}
              aria-label="关闭恢复提示"
            >
              ×
            </button>
          </div>
          <div style={{ marginTop: 4, opacity: 0.7, fontSize: 11 }}>
            iter={recovery.iteration} · stage={recovery.stage} · pid=
            {recovery.deadPid} · {recovery.partialChars} chars
          </div>
          <pre
            style={{
              marginTop: 8,
              padding: 8,
              background: 'var(--color-surface, #fafafa)',
              border: '1px solid var(--color-border, #e5e7eb)',
              borderRadius: 4,
              fontSize: 12,
              maxHeight: 160,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              fontFamily:
                'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
            }}
          >
            {recovery.partialText}
          </pre>
        </div>
      )}

      {/* Stall banner — yellow, actionable */}
      {stall.kind === 'stalled' && (
        <div
          role="alert"
          style={{
            margin: '8px 12px',
            padding: '10px 12px',
            background: '#fffbeb',
            border: '1px solid #fde68a',
            borderLeft: '3px solid #f59e0b',
            borderRadius: 6,
            fontSize: 13,
            lineHeight: 1.5,
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            gap: 12,
          }}
        >
          <div>
            <strong>Agent 似乎卡住了</strong>
            <span style={{ marginLeft: 6, opacity: 0.85 }}>
              ({stageHint(stall.data.stage)} · 已 {formatDuration(stall.data.stalledForMs)} 无活动
              {stall.data.partialChars > 0
                ? ` · 已收到 ${stall.data.partialChars} chars`
                : ''}
              )
            </span>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              type="button"
              onClick={handleInterrupt}
              disabled={interrupting}
              style={{
                padding: '4px 10px',
                fontSize: 12,
                background: '#f59e0b',
                color: '#fff',
                border: 'none',
                borderRadius: 4,
                cursor: interrupting ? 'wait' : 'pointer',
                opacity: interrupting ? 0.6 : 1,
              }}
            >
              {interrupting ? '中断中…' : '中断并保存'}
            </button>
            <button
              type="button"
              onClick={handleKeepWaiting}
              style={{
                padding: '4px 10px',
                fontSize: 12,
                background: 'transparent',
                color: '#92400e',
                border: '1px solid #fbbf24',
                borderRadius: 4,
                cursor: 'pointer',
              }}
            >
              继续等待
            </button>
          </div>
        </div>
      )}

      {/* Live heartbeat indicator — small, only when a run is active */}
      {beat && stall.kind === 'none' && (
        <div
          aria-live="polite"
          style={{
            margin: '4px 12px',
            padding: '4px 10px',
            background: 'transparent',
            color: 'var(--color-text-2, #6b7280)',
            fontSize: 11,
            lineHeight: 1.4,
            display: 'flex',
            alignItems: 'center',
            gap: 6,
          }}
        >
          <span
            aria-hidden="true"
            style={{
              display: 'inline-block',
              width: 6,
              height: 6,
              borderRadius: '50%',
              background:
                beat.lastActivityMsAgo > 10_000 ? '#f59e0b' : '#10b981',
              animation:
                beat.lastActivityMsAgo > 10_000
                  ? 'none'
                  : 'uclaw-heartbeat-pulse 1.6s ease-in-out infinite',
            }}
          />
          <span>
            iter {beat.iteration} · {stageHint(beat.stage)}
            {beat.lastActivityMsAgo > 2_000
              ? ` · ${formatDuration(beat.lastActivityMsAgo)} ago`
              : ''}
            {beat.partialChars > 0 ? ` · ${beat.partialChars} chars` : ''}
          </span>
        </div>
      )}

      {/* Inline keyframes so we don't depend on global CSS for the dot
          pulse animation. Scoped per-instance, harmless if duplicated. */}
      <style>{`
        @keyframes uclaw-heartbeat-pulse {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.4; transform: scale(0.85); }
        }
      `}</style>
    </>
  )
}

export default AgentHeartbeatBanner
