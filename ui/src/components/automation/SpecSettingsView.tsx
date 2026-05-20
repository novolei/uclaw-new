import { useState, useEffect } from 'react'
import { useSetAtom } from 'jotai'
import {
  setAutomationEnabled,
  setAutomationPermission,
  updateAutomationUserConfig,
  listSpecChannelBindings,
  updateSpecChannelBindings,
  updateSpecImSettings,
} from '@/lib/tauri-bridge'
import type { HumaneSpecRow, SpecChannelBinding } from '@/lib/tauri-bridge'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { openBrowserTabAction } from '@/atoms/preview-panel-atoms'

interface Props {
  spec: HumaneSpecRow
  onSpecChange: (updated: HumaneSpecRow) => void
}

const AUTOMATION_PERMISSION_IDS = ['ai_browser', 'notification', 'filesystem', 'network', 'shell'] as const
const LIVE_CONFIG_FIELDS = ['platform', 'room_id', 'live_url', 'action_mode', 'poll_interval_seconds'] as const

type PermissionState = 'granted' | 'denied' | 'default'
type LiveConfigDraft = Record<(typeof LIVE_CONFIG_FIELDS)[number], string>

function parseJsonArray(value: unknown): string[] {
  if (Array.isArray(value)) return value.filter((v): v is string => typeof v === 'string')
  if (typeof value !== 'string') return []
  try {
    const parsed = JSON.parse(value)
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === 'string') : []
  } catch {
    return []
  }
}

function parseJsonRecord(value: unknown): Record<string, unknown> {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>
  if (typeof value !== 'string' || value.trim() === '') return {}
  try {
    const parsed = JSON.parse(value)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : {}
  } catch {
    return {}
  }
}

function parseSpecJson(spec: HumaneSpecRow): Record<string, unknown> {
  return parseJsonRecord(spec.specJson)
}

function readLiveConfig(spec: HumaneSpecRow) {
  const raw = parseSpecJson(spec)
  if (parseJsonRecord(raw.x_uclaw_runtime).kind !== 'live_room_moderator') return null
  const config = parseJsonRecord(raw.config)
  const runtime = parseJsonRecord(raw.x_uclaw_runtime)
  const overrides = parseJsonRecord(spec.userConfigValues)
  const read = (snake: string, camel?: string, fallback = ''): string => {
    const value = overrides[snake] ?? (camel ? overrides[camel] : undefined) ?? config[snake] ?? (camel ? config[camel] : undefined) ?? fallback
    return value == null ? '' : String(value)
  }
  return {
    platform: read('platform', undefined, 'douyin'),
    room_id: read('room_id', 'roomId'),
    live_url: read('live_url', 'liveUrl'),
    action_mode: read('action_mode', 'actionMode', String(runtime.action_mode_default ?? 'real')),
    poll_interval_seconds: read(
      'poll_interval_seconds',
      'pollIntervalSeconds',
      String(runtime.poll_interval_seconds ?? 30),
    ),
    knowledgeScope: read('knowledge_scope', 'knowledgeScope', 'room_only'),
  }
}

function readBrowserLogins(spec: HumaneSpecRow): Array<{ url: string; label: string }> {
  const raw = parseSpecJson(spec)
  const browserLogin = raw.browser_login
  const entries = Array.isArray(browserLogin) ? browserLogin : browserLogin ? [browserLogin] : []
  return entries
    .map((entry) => parseJsonRecord(entry))
    .map((entry) => ({
      url: String(entry.url ?? ''),
      label: String(entry.label ?? entry.url ?? 'Browser login'),
    }))
    .filter((entry) => entry.url || entry.label)
}

export function SpecSettingsView({ spec, onSpecChange }: Props) {
  const [view, setView] = useState<'settings' | 'yaml'>('settings')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [savingPermission, setSavingPermission] = useState<string | null>(null)
  const [savingLiveConfig, setSavingLiveConfig] = useState(false)
  const openBrowserTab = useSetAtom(openBrowserTabAction)

  async function handleToggleEnabled() {
    setSaving(true)
    setError(null)
    try {
      await setAutomationEnabled(spec.id, !spec.enabled)
      onSpecChange({ ...spec, enabled: !spec.enabled })
    } catch (err: unknown) {
      setError((err as { message?: string })?.message ?? '操作失败')
    } finally {
      setSaving(false)
    }
  }

  const permissionsGranted = parseJsonArray(spec.permissionsGranted)
  const permissionsDenied = parseJsonArray(spec.permissionsDenied)
  const liveConfig = readLiveConfig(spec)
  const browserLogins = readBrowserLogins(spec)
  const [liveDraft, setLiveDraft] = useState<LiveConfigDraft>(() => ({
    platform: liveConfig?.platform ?? 'douyin',
    room_id: liveConfig?.room_id ?? '',
    live_url: liveConfig?.live_url ?? '',
    action_mode: liveConfig?.action_mode ?? 'real',
    poll_interval_seconds: liveConfig?.poll_interval_seconds ?? '30',
  }))

  useEffect(() => {
    setLiveDraft({
      platform: liveConfig?.platform ?? 'douyin',
      room_id: liveConfig?.room_id ?? '',
      live_url: liveConfig?.live_url ?? '',
      action_mode: liveConfig?.action_mode ?? 'real',
      poll_interval_seconds: liveConfig?.poll_interval_seconds ?? '30',
    })
  }, [
    liveConfig?.platform,
    liveConfig?.room_id,
    liveConfig?.live_url,
    liveConfig?.action_mode,
    liveConfig?.poll_interval_seconds,
  ])

  async function handleSetPermission(permission: string, granted: boolean) {
    setSavingPermission(permission)
    setError(null)
    try {
      await setAutomationPermission(spec.id, permission, granted)
      const nextGranted = new Set(permissionsGranted)
      const nextDenied = new Set(permissionsDenied)
      if (granted) {
        nextGranted.add(permission)
        nextDenied.delete(permission)
      } else {
        nextDenied.add(permission)
        nextGranted.delete(permission)
      }
      onSpecChange({
        ...spec,
        permissionsGranted: JSON.stringify([...nextGranted]),
        permissionsDenied: JSON.stringify([...nextDenied]),
      })
    } catch (err: unknown) {
      setError((err as { message?: string })?.message ?? '权限更新失败')
    } finally {
      setSavingPermission(null)
    }
  }

  async function handleSaveLiveConfig() {
    if (!liveConfig) return
    setSavingLiveConfig(true)
    setError(null)
    try {
      const current = parseJsonRecord(spec.userConfigValues)
      const poll = Number.parseInt(liveDraft.poll_interval_seconds, 10)
      const nextValues = {
        ...current,
        platform: liveDraft.platform.trim() || 'douyin',
        room_id: liveDraft.room_id.trim(),
        live_url: liveDraft.live_url.trim(),
        action_mode: liveDraft.action_mode.trim() || 'real',
        poll_interval_seconds: Number.isFinite(poll) && poll > 0 ? poll : 30,
      }
      await updateAutomationUserConfig(spec.id, nextValues)
      onSpecChange({
        ...spec,
        userConfigValues: JSON.stringify(nextValues),
      })
    } catch (err: unknown) {
      setError((err as { message?: string })?.message ?? '直播间配置保存失败')
    } finally {
      setSavingLiveConfig(false)
    }
  }

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      {/* header */}
      <div className="flex items-center gap-2 p-4 border-b border-border/50">
        <div className="flex-1">
          <div className="font-semibold text-sm">{spec.name}</div>
          <div className="text-xs text-muted-foreground">
            v{spec.version} · {spec.author}
          </div>
        </div>
        {/* view toggle */}
        <div className="flex rounded-lg border border-border overflow-hidden text-xs">
          {(['settings', 'yaml'] as const).map((v) => (
            <button
              key={v}
              onClick={() => setView(v)}
              className={[
                'titlebar-no-drag px-3 py-1',
                view === v ? 'bg-muted text-foreground' : 'text-muted-foreground hover:bg-muted/50',
              ].join(' ')}
            >
              {v === 'settings' ? '⚙ 设置' : '<> YAML'}
            </button>
          ))}
        </div>
      </div>

      {error && (
        <div className="mx-4 mt-3 text-xs text-destructive bg-destructive/10 rounded px-3 py-2">
          {error}
        </div>
      )}

      {view === 'yaml' ? (
        <pre className="flex-1 p-4 text-xs font-mono overflow-auto whitespace-pre-wrap text-muted-foreground">
          {spec.specYaml}
        </pre>
      ) : (
        <div className="flex flex-col gap-6 p-4">
          {/* enabled */}
          <Section title="状态">
            <Row label="启用" description="允许定时任务自动触发">
              <Toggle
                checked={spec.enabled}
                disabled={saving}
                onChange={handleToggleEnabled}
              />
            </Row>
          </Section>

          {/* permissions */}
          <Section title="权限">
            {AUTOMATION_PERMISSION_IDS.map((p) => (
              <PermissionRow
                key={p}
                permission={p}
                state={
                  permissionsGranted.includes(p)
                    ? 'granted'
                    : permissionsDenied.includes(p)
                      ? 'denied'
                      : 'default'
                }
                disabled={savingPermission === p}
                onSet={(granted) => handleSetPermission(p, granted)}
              />
            ))}
          </Section>

          {browserLogins.length > 0 && (
            <Section title="浏览器登录">
              <div className="flex flex-col gap-2">
                {browserLogins.map((entry) => (
                  <button
                    key={`${entry.label}:${entry.url}`}
                    type="button"
                    onClick={() => {
                      openBrowserTab({
                        agentSessionId: `automation-login:${spec.id}`,
                        initialUrl: entry.url,
                      })
                    }}
                    className="titlebar-no-drag flex items-center justify-between gap-3 rounded-md border border-border bg-muted/30 px-3 py-2 text-left hover:bg-muted/60"
                    aria-label={`在 AI Browser 登录 ${entry.label}`}
                  >
                    <span>
                      <span className="block text-sm">{entry.label}</span>
                      <span className="block text-xs text-muted-foreground">使用 AI Browser 登录页完成账号/验证码，不保存凭据</span>
                    </span>
                    <span className="text-xs text-primary shrink-0">AI Browser 登录</span>
                  </button>
                ))}
              </div>
            </Section>
          )}

          {liveConfig && (
            <Section title="直播间">
              <LiveConfigInput label="platform" value={liveDraft.platform} onChange={(v) => setLiveDraft((prev) => ({ ...prev, platform: v }))} />
              <LiveConfigInput label="room_id" value={liveDraft.room_id} onChange={(v) => setLiveDraft((prev) => ({ ...prev, room_id: v }))} />
              <LiveConfigInput label="live_url" value={liveDraft.live_url} onChange={(v) => setLiveDraft((prev) => ({ ...prev, live_url: v }))} />
              <Row label="action_mode" description="">
                <select
                  aria-label="action_mode"
                  value={liveDraft.action_mode}
                  onChange={(e) => setLiveDraft((prev) => ({ ...prev, action_mode: e.target.value }))}
                  className="titlebar-no-drag w-36 rounded border border-border bg-muted/40 px-2 py-1 text-xs"
                >
                  <option value="real">real</option>
                  <option value="dry_run">dry_run</option>
                  <option value="ask">ask</option>
                </select>
              </Row>
              <LiveConfigInput
                label="poll_interval_seconds"
                type="number"
                value={liveDraft.poll_interval_seconds}
                onChange={(v) => setLiveDraft((prev) => ({ ...prev, poll_interval_seconds: v }))}
              />
              <Row label="知识库" description="仅使用直播间隔离命名空间">
                <span className="text-xs text-muted-foreground">{liveConfig.knowledgeScope}</span>
              </Row>
              <button
                type="button"
                disabled={savingLiveConfig}
                onClick={handleSaveLiveConfig}
                className="titlebar-no-drag self-end rounded-md bg-primary px-3 py-1 text-xs text-primary-foreground disabled:opacity-50"
              >
                {savingLiveConfig ? '保存中…' : '保存直播间配置'}
              </button>
            </Section>
          )}

          {/* info */}
          <Section title="关于">
            <p className="text-xs text-muted-foreground">{spec.description}</p>
            <p className="text-xs text-muted-foreground mt-1">来源：{spec.source}</p>
          </Section>

          {/* 消息通道 — IM Channel Bindings */}
          <ImChannelBindingsSection specId={spec.id} />

          {/* IM触发 */}
          <Section title="IM 触发">
            <ImTriggerRow
              specId={spec.id}
              initialTriggerPhrase={spec.triggerPhrase ?? ''}
            />
          </Section>

          {/* 开发者 */}
          <Section title="开发者">
            <SystemPromptRow
              specId={spec.id}
              initialValue={spec.systemPromptOverride ?? ''}
            />
          </Section>
        </div>
      )}
    </div>
  )
}

function PermissionRow({
  permission,
  state,
  disabled,
  onSet,
}: {
  permission: string
  state: PermissionState
  disabled: boolean
  onSet: (granted: boolean) => void
}) {
  const stateLabel: Record<PermissionState, string> = {
    granted: 'granted',
    denied: 'denied',
    default: 'default',
  }
  return (
    <Row label={permission} description="">
      <div className="flex items-center gap-2">
        <span
          className={[
            'rounded px-2 py-0.5 text-[11px] font-mono',
            state === 'granted'
              ? 'bg-emerald-500/10 text-emerald-600'
              : state === 'denied'
                ? 'bg-destructive/10 text-destructive'
                : 'bg-muted text-muted-foreground',
          ].join(' ')}
        >
          {stateLabel[state]}
        </span>
        <div className="flex overflow-hidden rounded-md border border-border text-xs">
          <button
            type="button"
            disabled={disabled || state === 'granted'}
            onClick={() => onSet(true)}
            className="titlebar-no-drag px-2 py-1 hover:bg-muted disabled:opacity-45"
            aria-label={`允许 ${permission}`}
          >
            允许
          </button>
          <button
            type="button"
            disabled={disabled || state === 'denied'}
            onClick={() => onSet(false)}
            className="titlebar-no-drag border-l border-border px-2 py-1 hover:bg-muted disabled:opacity-45"
            aria-label={`拒绝 ${permission}`}
          >
            拒绝
          </button>
        </div>
      </div>
    </Row>
  )
}

function LiveConfigInput({
  label,
  value,
  type = 'text',
  onChange,
}: {
  label: string
  value: string
  type?: string
  onChange: (value: string) => void
}) {
  return (
    <Row label={label} description="">
      <input
        aria-label={label}
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="titlebar-no-drag w-52 rounded border border-border bg-muted/40 px-2 py-1 text-xs font-mono"
      />
    </Row>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">{title}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </div>
  )
}

function Row({ label, description, children }: { label: string; description: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-sm">{label}</div>
        {description && <div className="text-xs text-muted-foreground">{description}</div>}
      </div>
      {children}
    </div>
  )
}

function Toggle({ checked, disabled, onChange }: { checked: boolean; disabled: boolean; onChange: () => void }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={onChange}
      className={[
        'titlebar-no-drag relative w-10 h-5 rounded-full transition-colors',
        checked ? 'bg-primary' : 'bg-muted',
        disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer',
      ].join(' ')}
    >
      <span
        className={[
          'absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-background shadow transition-transform',
          checked ? 'translate-x-5' : 'translate-x-0',
        ].join(' ')}
      />
    </button>
  )
}

function ImChannelBindingsSection({ specId }: { specId: string }) {
  const [bindings, setBindings] = useState<SpecChannelBinding[]>([])
  const [loading, setLoading] = useState(true)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  useEffect(() => {
    listSpecChannelBindings(specId)
      .then(setBindings)
      .catch(() => setBindings([]))
      .finally(() => setLoading(false))
  }, [specId])

  async function handleToggle(channelInstanceId: string, enabled: boolean) {
    const updated = bindings.map((b) =>
      b.channelInstanceId === channelInstanceId ? { ...b, enabled } : b
    )
    setBindings(updated)
    await updateSpecChannelBindings(specId, updated).catch(() => {})
  }

  if (loading) return null

  return (
    <Section title="消息通道">
      <p className="text-xs text-muted-foreground mb-2">
        AI 驱动：数字人决定何时以及通过配置的渠道通知什么内容。
      </p>
      {bindings.length === 0 ? (
        <p className="text-xs text-muted-foreground">暂无渠道。请先在设置中配置 IM 渠道。</p>
      ) : (
        bindings.map((b) => (
          <Row key={b.channelInstanceId} label={b.channelName ?? b.channelInstanceId} description={b.channelType ?? ''}>
            <Toggle checked={b.enabled} disabled={false} onChange={() => handleToggle(b.channelInstanceId, !b.enabled)} />
          </Row>
        ))
      )}
      <button
        className="titlebar-no-drag text-xs text-primary mt-1 hover:underline"
        onClick={() => { setSettingsTab('imChannels'); setSettingsOpen(true) }}
      >
        在设置中配置渠道 ↗
      </button>
    </Section>
  )
}

function ImTriggerRow({ specId, initialTriggerPhrase }: { specId: string; initialTriggerPhrase: string }) {
  const [value, setValue] = useState(initialTriggerPhrase)
  const [saved, setSaved] = useState(initialTriggerPhrase)
  const [saving, setSaving] = useState(false)
  const dirty = value !== saved

  async function handleSave() {
    if (!dirty) return
    setSaving(true)
    await updateSpecImSettings(specId, value || null, null).catch(() => {})
    setSaved(value)
    setSaving(false)
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1.5">
        <span className="text-sm">触发关键词</span>
        {dirty && <span className="text-xs text-amber-500">未保存</span>}
      </div>
      <div className="text-xs text-muted-foreground">IM 消息以此关键词开头时触发本 automation</div>
      <div className="flex gap-2 mt-1">
        <input
          className="flex-1 text-xs bg-muted/50 border border-border rounded px-2 py-1 font-mono"
          placeholder="/daily-report"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onBlur={handleSave}
        />
        <button
          disabled={saving || !dirty}
          onClick={handleSave}
          className="titlebar-no-drag text-xs px-3 py-1 bg-primary text-primary-foreground rounded disabled:opacity-50"
        >
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </div>
  )
}

function SystemPromptRow({ specId, initialValue }: { specId: string; initialValue: string }) {
  const [value, setValue] = useState(initialValue)
  const [saved, setSaved] = useState(initialValue)
  const [saving, setSaving] = useState(false)
  const dirty = value !== saved

  async function handleSave() {
    if (!dirty) return
    setSaving(true)
    await updateSpecImSettings(specId, null, value || null).catch(() => {})
    setSaved(value)
    setSaving(false)
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1.5">
        <span className="text-sm">系统提示词</span>
        {dirty && <span className="text-xs text-amber-500">未保存</span>}
      </div>
      <div className="text-xs text-muted-foreground">覆盖 Space 级默认 prompt（可选）</div>
      <textarea
        className="mt-1 text-xs bg-muted/50 border border-border rounded px-2 py-1 font-mono resize-y min-h-[80px]"
        placeholder="（留空则使用 Space 默认提示词）"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={handleSave}
      />
      <button
        disabled={saving || !dirty}
        onClick={handleSave}
        className="titlebar-no-drag self-end text-xs px-3 py-1 bg-primary text-primary-foreground rounded disabled:opacity-50 mt-1"
      >
        {saving ? '保存中…' : '保存'}
      </button>
    </div>
  )
}
