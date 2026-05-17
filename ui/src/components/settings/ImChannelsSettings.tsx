import { useAtom, useSetAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { imChannelsAtom, fetchImChannelsAtom, type ImChannelRow } from '@/atoms/im-channel-atoms'
import { ImChannelForm } from './ImChannelForm'
import type { SpaceSummary } from '@/lib/types'

const CHANNEL_TYPE_LABELS: Record<string, string> = {
  wecom_bot:    '企业微信 Bot',
  wechat_ilink: '微信 iLink',
  email:        '电子邮件',
  dingtalk:     '钉钉',
  feishu:       '飞书',
  webhook:      'Webhook',
}

export function ImChannelsSettings() {
  const [channels] = useAtom(imChannelsAtom)
  const fetchChannels = useSetAtom(fetchImChannelsAtom)
  const [spaces, setSpaces] = useState<{ id: string; name: string }[]>([])
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<ImChannelRow | undefined>()

  useEffect(() => {
    fetchChannels()
    invoke<SpaceSummary[]>('list_spaces')
      .then(rows => setSpaces(rows.map(s => ({ id: s.id, name: s.name }))))
      .catch(() => {})
  }, [fetchChannels])

  async function handleToggle(id: string, enabled: boolean) {
    try {
      await invoke('toggle_im_channel', { id, enabled })
      fetchChannels()
    } catch (e) {
      console.error('toggle_im_channel failed:', e)
      alert('操作失败，请查看控制台了解详情')
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此渠道实例？')) return
    try {
      await invoke('delete_im_channel', { id })
      fetchChannels()
    } catch (e) {
      console.error('delete_im_channel failed:', e)
      alert('删除失败，请查看控制台了解详情')
    }
  }

  function handleEdit(ch: ImChannelRow) {
    setEditing(ch)
    setShowForm(true)
  }

  function handleDone() {
    setShowForm(false)
    setEditing(undefined)
    fetchChannels()
  }

  if (showForm) {
    return (
      <div className="max-w-lg">
        <div className="mb-3 flex items-center gap-2">
          <button onClick={() => { setShowForm(false); setEditing(undefined) }}
            className="text-sm text-muted-foreground hover:text-foreground">
            ← 返回
          </button>
          <span className="text-sm font-medium">{editing ? '编辑渠道' : '新增渠道'}</span>
        </div>
        <ImChannelForm spaces={spaces} editing={editing} onDone={handleDone} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">IM 渠道</h3>
          <p className="text-xs text-muted-foreground mt-0.5">
            配置通知渠道和双向 IM 机器人，绑定到工作空间
          </p>
        </div>
        <button
          onClick={() => setShowForm(true)}
          className="rounded bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
        >
          + 新增渠道
        </button>
      </div>

      {channels.length === 0 ? (
        <div className="rounded border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
          还没有配置任何渠道。点击「新增渠道」开始。
        </div>
      ) : (
        <div className="space-y-2">
          {channels.map(ch => {
            const space = spaces.find(s => s.id === ch.spaceId)
            return (
              <div key={ch.id}
                className="flex items-center gap-3 rounded border border-border bg-card px-3 py-2.5">
                <div
                  className={`h-2 w-2 rounded-full flex-shrink-0 ${
                    ch.enabled ? 'bg-success' : 'bg-muted-foreground'
                  }`}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium truncate">{ch.name}</span>
                    <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                      {CHANNEL_TYPE_LABELS[ch.channelType] ?? ch.channelType}
                    </span>
                    {space && (
                      <span className="rounded bg-accent/20 px-1.5 py-0.5 text-xs text-accent-foreground">
                        {space.name}
                      </span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 flex-shrink-0">
                  <button
                    onClick={() => handleToggle(ch.id, !ch.enabled)}
                    className="rounded px-2 py-1 text-xs hover:bg-muted"
                  >
                    {ch.enabled ? '停用' : '启用'}
                  </button>
                  <button
                    onClick={() => handleEdit(ch)}
                    className="rounded px-2 py-1 text-xs hover:bg-muted"
                  >
                    编辑
                  </button>
                  <button
                    onClick={() => handleDelete(ch.id)}
                    className="rounded px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                  >
                    删除
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
