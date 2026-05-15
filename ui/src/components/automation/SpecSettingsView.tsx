import { useState } from 'react'
import { setAutomationEnabled } from '@/lib/tauri-bridge'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  spec: HumaneSpecRow
  onSpecChange: (updated: HumaneSpecRow) => void
}

export function SpecSettingsView({ spec, onSpecChange }: Props) {
  const [view, setView] = useState<'settings' | 'yaml'>('settings')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

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

  // permissionsGranted is a JSON string — normalize to array
  const permissions: string[] = (() => {
    if (Array.isArray(spec.permissionsGranted)) return spec.permissionsGranted as unknown as string[]
    try { return JSON.parse(spec.permissionsGranted as string) } catch { return [] }
  })()

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
            {(['AI 浏览器', '电子邮件', 'IM 推送'] as const).map((p) => (
              <Row key={p} label={p} description="">
                <span className="text-xs text-muted-foreground">
                  {permissions.includes(p) ? '已授权' : '未授权'}
                </span>
              </Row>
            ))}
          </Section>

          {/* info */}
          <Section title="关于">
            <p className="text-xs text-muted-foreground">{spec.description}</p>
            <p className="text-xs text-muted-foreground mt-1">来源：{spec.source}</p>
          </Section>
        </div>
      )}
    </div>
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
