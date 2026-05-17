import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { ImChannelInput, ImChannelRow } from '@/atoms/im-channel-atoms'

const CHANNEL_TYPES = [
  { value: 'wecom_bot',    label: '企业微信 Bot (WebSocket)' },
  { value: 'wechat_ilink', label: '微信个人 (iLink)' },
  { value: 'email',        label: '电子邮件 (SMTP)' },
  { value: 'dingtalk',     label: '钉钉 Webhook' },
  { value: 'feishu',       label: '飞书 Webhook' },
  { value: 'webhook',      label: '通用 Webhook' },
]

interface Props {
  spaces: { id: string; name: string }[]
  editing?: ImChannelRow
  onDone: () => void
}

export function ImChannelForm({ spaces, editing, onDone }: Props) {
  const [channelType, setChannelType] = useState(editing?.channelType ?? 'webhook')
  const [name, setName] = useState(editing?.name ?? '')
  const [spaceId, setSpaceId] = useState(editing?.spaceId ?? spaces[0]?.id ?? '')
  const [enabled, setEnabled] = useState(editing?.enabled ?? true)
  const [streaming, setStreaming] = useState(editing?.streaming ?? false)
  const [permissionEnabled, setPermissionEnabled] = useState(editing?.permissionEnabled ?? false)
  const [owners, setOwners] = useState(editing?.owners.join(', ') ?? '')
  const [mcpEnabled, setMcpEnabled] = useState(editing?.guestPolicy.mcp_enabled ?? false)
  // Channel-specific fields
  const [webhookUrl, setWebhookUrl] = useState((editing?.config.url as string) ?? '')
  const [smtpHost, setSmtpHost] = useState((editing?.config.smtp_host as string) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(editing?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((editing?.config.username as string) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState((editing?.config.to_addresses as string[])?.join(', ') ?? '')
  const [corpId, setCorpId] = useState((editing?.config.corp_id as string) ?? '')
  const [agentId, setAgentId] = useState((editing?.config.agent_id as string) ?? '')
  const [corpSecret, setCorpSecret] = useState('')
  const [wecomWsUrl, setWecomWsUrl] = useState((editing?.config.ws_url as string) ?? '')
  const [appId, setAppId] = useState((editing?.config.app_id as string) ?? '')
  const [apiKey, setApiKey] = useState('')
  const [signingSecret, setSigningSecret] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function buildInput(): ImChannelInput {
    let config: Record<string, unknown> = {}
    let credentials: Record<string, unknown> = {}

    switch (channelType) {
      case 'webhook':
        config = { url: webhookUrl }
        break
      case 'email':
        config = {
          smtp_host: smtpHost,
          smtp_port: Number(smtpPort),
          username: smtpUser,
          to_addresses: toAddresses.split(',').map(s => s.trim()).filter(Boolean),
        }
        credentials = { password: smtpPass }
        break
      case 'dingtalk':
      case 'feishu':
        config = { webhook_url: webhookUrl }
        credentials = { signing_secret: signingSecret }
        break
      case 'wecom_bot':
        config = { corp_id: corpId, agent_id: agentId, ...(wecomWsUrl ? { ws_url: wecomWsUrl } : {}) }
        credentials = { corp_secret: corpSecret }
        break
      case 'wechat_ilink':
        config = { app_id: appId }
        credentials = { api_key: apiKey }
        break
    }

    return {
      spaceId,
      channelType,
      name,
      config,
      credentials,
      enabled,
      streaming,
      replyScope: 'all',
      permissionEnabled,
      owners: owners.split(',').map(s => s.trim()).filter(Boolean),
      guestPolicy: { tool_allowlist: [], mcp_enabled: mcpEnabled },
    }
  }

  async function handleSave() {
    setSaving(true)
    setError(null)
    try {
      if (channelType === 'email') {
        const port = Number(smtpPort)
        if (!Number.isInteger(port) || port < 1 || port > 65535) {
          setError('端口号必须是 1–65535 之间的整数')
          setSaving(false)
          return
        }
      }
      const input = buildInput()
      if (editing) {
        await invoke('update_im_channel', { id: editing.id, input })
      } else {
        await invoke('create_im_channel', { input })
      }
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-4 p-4">
      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">渠道类型</label>
        <select
          value={channelType}
          onChange={e => setChannelType(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          disabled={!!editing}
        >
          {CHANNEL_TYPES.map(t => (
            <option key={t.value} value={t.value}>{t.label}</option>
          ))}
        </select>
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">名称</label>
        <input
          value={name}
          onChange={e => setName(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          placeholder="我的企微机器人"
        />
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">绑定 Space</label>
        <select
          value={spaceId}
          onChange={e => setSpaceId(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
        >
          {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </div>

      {channelType === 'webhook' && (
        <div className="space-y-1">
          <label className="text-xs text-muted-foreground">Webhook URL</label>
          <input value={webhookUrl} onChange={e => setWebhookUrl(e.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
            placeholder="https://example.com/hook" />
        </div>
      )}

      {(channelType === 'dingtalk' || channelType === 'feishu') && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Webhook URL</label>
            <input value={webhookUrl} onChange={e => setWebhookUrl(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">签名密钥（可选）</label>
            <input value={signingSecret} onChange={e => setSigningSecret(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
        </>
      )}

      {channelType === 'email' && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">SMTP Host</label>
              <input value={smtpHost} onChange={e => setSmtpHost(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="smtp.gmail.com" />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">端口</label>
              <input value={smtpPort} onChange={e => setSmtpPort(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="587" />
            </div>
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">用户名</label>
            <input value={smtpUser} onChange={e => setSmtpUser(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">密码</label>
            <input value={smtpPass} onChange={e => setSmtpPass(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">收件人（逗号分隔）</label>
            <input value={toAddresses} onChange={e => setToAddresses(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="a@example.com, b@example.com" />
          </div>
        </>
      )}

      {channelType === 'wecom_bot' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp ID</label>
            <input value={corpId} onChange={e => setCorpId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Agent ID</label>
            <input value={agentId} onChange={e => setAgentId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp Secret</label>
            <input value={corpSecret} onChange={e => setCorpSecret(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">
              WebSocket 服务器（可选，私有化部署时填写）
            </label>
            <input value={wecomWsUrl} onChange={e => setWecomWsUrl(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm font-mono"
              placeholder="wss://openws.work.weixin.qq.com" />
          </div>
        </>
      )}

      {channelType === 'wechat_ilink' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">App ID</label>
            <input value={appId} onChange={e => setAppId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">API Key</label>
            <input value={apiKey} onChange={e => setApiKey(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
        </>
      )}

      <div className="flex items-center gap-3">
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} />
          启用
        </label>
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={streaming} onChange={e => setStreaming(e.target.checked)} />
          流式回复
        </label>
      </div>

      <div className="space-y-2 rounded border border-border p-3">
        <label className="flex items-center gap-1.5 text-sm font-medium">
          <input type="checkbox" checked={permissionEnabled}
            onChange={e => setPermissionEnabled(e.target.checked)} />
          启用权限控制
        </label>
        {permissionEnabled && (
          <>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">
                Owners（chat_id 白名单，逗号分隔）
              </label>
              <input value={owners} onChange={e => setOwners(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="openid_1, openid_2" />
            </div>
            <label className="flex items-center gap-1.5 text-sm">
              <input type="checkbox" checked={mcpEnabled}
                onChange={e => setMcpEnabled(e.target.checked)} />
              Guest 允许 MCP 工具
            </label>
          </>
        )}
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex justify-end gap-2">
        <button onClick={onDone}
          className="rounded px-3 py-1.5 text-sm hover:bg-muted">
          取消
        </button>
        <button onClick={handleSave} disabled={saving || !name || !spaceId}
          className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50">
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </div>
  )
}
