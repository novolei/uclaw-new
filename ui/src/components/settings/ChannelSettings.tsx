import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

import { Button } from '@/components/ui/button'
import {
  listProviders,
  listConfiguredProviders,
  removeProviderConfig,
  testProviderConnection,
} from '@/lib/tauri-bridge'
import type { ProviderInfo } from '@/lib/types'
import { ChannelForm } from './ChannelForm'

export function ChannelSettings() {
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [configuredIds, setConfiguredIds] = useState<string[]>([])
  const [showForm, setShowForm] = useState(false)
  const [editingProvider, setEditingProvider] = useState<string | null>(null)

  useEffect(() => {
    loadData()
  }, [])

  const loadData = async () => {
    const [allProviders, configured] = await Promise.all([
      listProviders(),
      listConfiguredProviders(),
    ])
    setProviders(allProviders)
    setConfiguredIds(configured)
  }

  const handleRemove = async (providerId: string) => {
    await removeProviderConfig(providerId)
    await loadData()
  }

  const handleTest = async (providerId: string) => {
    try {
      const provider = providers.find((p) => p.id === providerId)
      const result = await testProviderConnection({
        providerId,
        baseUrl: provider?.defaultBaseUrl || '',
      })
      alert(result.success ? '连接成功！' : `连接失败：${result.message}`)
    } catch (err) {
      alert(`测试失败: ${err}`)
    }
  }

  const configuredProviders = providers.filter((p) => configuredIds.includes(p.id))
  const availableProviders = providers.filter((p) => !configuredIds.includes(p.id))

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">渠道管理</h2>

      <SettingsSection title="已配置的 Provider" description="管理你的 AI 模型供应商连接">
        {configuredProviders.length > 0 ? (
          <div className="space-y-2">
            {configuredProviders.map((provider) => (
              <SettingsCard key={provider.id}>
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-sm font-medium">{provider.displayName}</div>
                    <div className="text-xs text-muted-foreground">{provider.id}</div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleTest(provider.id)}
                    >
                      测试
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setEditingProvider(provider.id)}
                    >
                      编辑
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="text-destructive"
                      onClick={() => handleRemove(provider.id)}
                    >
                      删除
                    </Button>
                  </div>
                </div>
              </SettingsCard>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground py-4 text-center">
            暂无已配置的 Provider，请添加一个以开始使用
          </div>
        )}
      </SettingsSection>

      {availableProviders.length > 0 && (
        <SettingsSection title="可用 Provider">
          <div className="grid grid-cols-2 gap-2">
            {availableProviders.map((provider) => (
              <Button
                key={provider.id}
                variant="outline"
                size="sm"
                className="justify-start"
                onClick={() => {
                  setEditingProvider(provider.id)
                  setShowForm(true)
                }}
              >
                + {provider.displayName}
              </Button>
            ))}
          </div>
        </SettingsSection>
      )}

      {(showForm || editingProvider) && (
        <ChannelForm
          providerId={editingProvider}
          onClose={() => {
            setShowForm(false)
            setEditingProvider(null)
          }}
          onSaved={() => {
            setShowForm(false)
            setEditingProvider(null)
            loadData()
          }}
        />
      )}
    </div>
  )
}
