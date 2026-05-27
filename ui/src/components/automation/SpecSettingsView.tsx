import { useState, useEffect } from 'react'
import { useSetAtom, useAtomValue } from 'jotai'
import {
  setAutomationEnabled,
  setAutomationPermission,
  updateAutomationUserConfig,
  listSpecChannelBindings,
  updateSpecChannelBindings,
  updateSpecImSettings,
  listenAutomationBrowserLoginCompleted,
} from '@/lib/tauri-bridge'
import type { HumaneSpecRow, SpecChannelBinding } from '@/lib/tauri-bridge'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { openAutomationLoginWindow } from '@/lib/automation-login-window'
import { userLocaleAtom } from '@/atoms/marketplace'
import { localizeConfig, localizeOption } from '@/lib/marketplace-i18n'
import type { SpecI18n } from '@/lib/marketplace-i18n'

interface Props {
  spec: HumaneSpecRow
  onSpecChange: (updated: HumaneSpecRow) => void
}

const AUTOMATION_PERMISSION_IDS = ['ai_browser', 'notification', 'filesystem', 'network', 'shell'] as const

type PermissionState = 'granted' | 'denied' | 'default'

interface ConfigSchemaEntry {
  key: string
  label: string
  description?: string
  placeholder?: string
  type?: string
  options?: Array<{ label: string; value: unknown } | string | number | boolean>
  required?: boolean
  default?: unknown
}

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

function readBrowserLoginProfiles(spec: HumaneSpecRow): Record<string, { status?: string; profileId?: string; completedAt?: number }> {
  const raw = parseJsonRecord(spec.userConfigValues)
  return parseJsonRecord(raw.browser_login_profiles) as Record<string, { status?: string; profileId?: string; completedAt?: number }>
}

function errorMessage(err: unknown, fallback: string): string {
  if (typeof err === 'string' && err.trim()) return err
  if (err instanceof Error && err.message.trim()) return err.message
  if (err && typeof err === 'object' && 'message' in err) {
    const message = String((err as { message?: unknown }).message ?? '').trim()
    if (message) return message
  }
  return fallback
}

export function SpecSettingsView({ spec, onSpecChange }: Props) {
  const [view, setView] = useState<'settings' | 'yaml'>('settings')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [savingPermission, setSavingPermission] = useState<string | null>(null)
  const [savingLiveConfig, setSavingLiveConfig] = useState(false)

  async function handleToggleEnabled() {
    setSaving(true)
    setError(null)
    try {
      await setAutomationEnabled(spec.id, !spec.enabled)
      onSpecChange({ ...spec, enabled: !spec.enabled })
    } catch (err: unknown) {
      setError(errorMessage(err, '操作失败'))
    } finally {
      setSaving(false)
    }
  }

  const permissionsGranted = parseJsonArray(spec.permissionsGranted)
  const permissionsDenied = parseJsonArray(spec.permissionsDenied)
  const browserLogins = readBrowserLogins(spec)
  const browserLoginProfiles = readBrowserLoginProfiles(spec)

  const raw = parseSpecJson(spec)
  const configSchema = (Array.isArray(raw.config_schema) ? raw.config_schema : []) as ConfigSchemaEntry[]
  const specI18n = raw.i18n as SpecI18n | undefined
  const locale = useAtomValue(userLocaleAtom)

  const [draftConfig, setDraftConfig] = useState<Record<string, unknown>>(() => {
    const values: Record<string, unknown> = {}
    configSchema.forEach((entry) => {
      values[entry.key] = entry.default ?? ''
    })
    const saved = parseJsonRecord(spec.userConfigValues)
    return { ...values, ...saved }
  })

  useEffect(() => {
    const values: Record<string, unknown> = {}
    configSchema.forEach((entry) => {
      values[entry.key] = entry.default ?? ''
    })
    const saved = parseJsonRecord(spec.userConfigValues)
    setDraftConfig({ ...values, ...saved })
  }, [spec.userConfigValues, spec.specJson])

  useEffect(() => {
    let unlisten: (() => void) | null = null
    listenAutomationBrowserLoginCompleted((payload) => {
      if (payload.specId !== spec.id) return
      const current = parseJsonRecord(spec.userConfigValues)
      const profiles = parseJsonRecord(current.browser_login_profiles)
      const nextValues = {
        ...current,
        browser_login_profiles: {
          ...profiles,
          [payload.url]: {
            status: payload.status,
            profileId: payload.profileId,
            label: payload.label,
            completedAt: payload.completedAt,
          },
        },
      }
      onSpecChange({
        ...spec,
        userConfigValues: JSON.stringify(nextValues),
      })
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [onSpecChange, spec])

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
      setError(errorMessage(err, '权限更新失败'))
    } finally {
      setSavingPermission(null)
    }
  }

  const hasConfigChanges = () => {
    const saved = parseJsonRecord(spec.userConfigValues)
    return configSchema.some((entry) => {
      const savedVal = saved[entry.key] !== undefined ? saved[entry.key] : (entry.default ?? '')
      const draftVal = draftConfig[entry.key] !== undefined ? draftConfig[entry.key] : (entry.default ?? '')
      return String(savedVal) !== String(draftVal)
    })
  }

  async function handleSaveConfig() {
    setSavingLiveConfig(true)
    setError(null)
    try {
      const current = parseJsonRecord(spec.userConfigValues)
      const nextValues = { ...current }
      configSchema.forEach((entry) => {
        let val = draftConfig[entry.key]
        if (val === undefined) {
          val = entry.default ?? ''
        }
        if (entry.type === 'number') {
          const num = Number(val)
          nextValues[entry.key] = Number.isFinite(num) ? num : (entry.default ?? 0)
        } else if (entry.type === 'boolean') {
          nextValues[entry.key] = !!val
        } else {
          nextValues[entry.key] = typeof val === 'string' ? val.trim() : val
        }
      })
      await updateAutomationUserConfig(spec.id, nextValues)
      onSpecChange({
        ...spec,
        userConfigValues: JSON.stringify(nextValues),
      })
    } catch (err: unknown) {
      setError(errorMessage(err, '保存配置失败'))
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
                  <div key={`${entry.label}:${entry.url}`} className="flex flex-col gap-1">
                  <button
                    type="button"
                    onClick={() => {
                      openAutomationLoginWindow({
                        specId: spec.id,
                        label: entry.label,
                        url: entry.url,
                      }).catch((err) => {
                        setError(errorMessage(err, '打开登录窗口失败'))
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
                  {browserLoginProfiles[entry.url]?.status === 'live' && (
                    <span className="px-1 text-[11px] text-emerald-600">
                      已登录 · auth profile {browserLoginProfiles[entry.url]?.profileId}
                    </span>
                  )}
                  </div>
                ))}
              </div>
            </Section>
          )}

          {configSchema.length > 0 && (
            <Section title="配置">
              {configSchema.map((entry) => {
                const label = localizeConfig(entry.key, 'label', entry.label, specI18n, locale) || entry.key
                const description = localizeConfig(entry.key, 'description', entry.description, specI18n, locale)
                const placeholder = localizeConfig(entry.key, 'placeholder', entry.placeholder, specI18n, locale)

                if (entry.type === 'boolean') {
                  return (
                    <Row key={entry.key} label={label} description={description}>
                      <Toggle
                        checked={!!draftConfig[entry.key]}
                        disabled={savingLiveConfig}
                        onChange={() => setDraftConfig((prev) => ({ ...prev, [entry.key]: !prev[entry.key] }))}
                      />
                    </Row>
                  )
                }

                if (entry.type === 'select') {
                  return (
                    <Row key={entry.key} label={label} description={description}>
                      <select
                        aria-label={entry.key}
                        value={String(draftConfig[entry.key] ?? '')}
                        disabled={savingLiveConfig}
                        onChange={(e) => setDraftConfig((prev) => ({ ...prev, [entry.key]: e.target.value }))}
                        className="titlebar-no-drag w-52 rounded border border-border bg-muted/40 px-2 py-1 text-xs"
                      >
                        <option value="">请选择...</option>
                        {(entry.options ?? []).map((opt) => {
                          const val = typeof opt === 'object' && opt !== null && 'value' in opt ? (opt as any).value : opt;
                          const lbl = typeof opt === 'object' && opt !== null && 'label' in opt ? (opt as any).label : String(opt);
                          const optionLabel = localizeOption(entry.key, String(val), String(lbl), specI18n, locale)
                          return (
                            <option key={String(val)} value={String(val)}>
                              {optionLabel}
                            </option>
                          )
                        })}
                      </select>
                    </Row>
                  )
                }

                if (entry.type === 'text') {
                  return (
                    <div key={entry.key} className="flex flex-col gap-1">
                      <div className="text-sm">{label}</div>
                      {description && <div className="text-xs text-muted-foreground">{description}</div>}
                      <textarea
                        aria-label={entry.key}
                        placeholder={placeholder}
                        disabled={savingLiveConfig}
                        value={String(draftConfig[entry.key] ?? '')}
                        onChange={(e) => setDraftConfig((prev) => ({ ...prev, [entry.key]: e.target.value }))}
                        className="titlebar-no-drag mt-1 text-xs bg-muted/40 border border-border rounded px-2 py-1 font-mono resize-y min-h-[80px]"
                      />
                    </div>
                  )
                }

                return (
                  <Row key={entry.key} label={label} description={description}>
                    <input
                      aria-label={entry.key}
                      type={entry.type === 'number' ? 'number' : 'text'}
                      placeholder={placeholder}
                      disabled={savingLiveConfig}
                      value={draftConfig[entry.key] === undefined || draftConfig[entry.key] === null ? '' : String(draftConfig[entry.key])}
                      onChange={(e) => {
                        const val = entry.type === 'number'
                          ? (e.target.value === '' ? '' : Number(e.target.value))
                          : e.target.value
                        setDraftConfig((prev) => ({ ...prev, [entry.key]: val }))
                      }}
                      className="titlebar-no-drag w-52 rounded border border-border bg-muted/40 px-2 py-1 text-xs font-mono"
                    />
                  </Row>
                )
              })}

              <div className="flex items-center justify-between mt-1">
                {hasConfigChanges() ? (
                  <span className="text-[11px] text-amber-500 font-medium">配置已修改，尚未保存</span>
                ) : (
                  <div />
                )}
                <button
                  type="button"
                  disabled={savingLiveConfig || !hasConfigChanges()}
                  onClick={handleSaveConfig}
                  className="titlebar-no-drag rounded bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-40"
                >
                  {savingLiveConfig ? '保存中…' : '保存配置'}
                </button>
              </div>
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
