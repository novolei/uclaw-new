import { useCallback, useEffect, useMemo, useState } from 'react'
import { Check, RefreshCw, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  listProviders,
  listConfiguredProviders,
  getProviderConfig,
  configureProviderWithModels,
  removeProviderConfig,
  testProviderConnection,
  listProviderModels,
  getConfiguredModels,
  getAllConfiguredModels,
} from '@/lib/tauri-bridge'
import type { ProviderInfo, ModelInfo } from '@/lib/types'
import { SettingsSecretInput } from './primitives/SettingsSecretInput'

const API_TYPE_OPTIONS = [
  { value: 'openai-completions', label: 'OpenAI Compatible' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'openai-responses', label: 'OpenAI Responses' },
]

const CATEGORY_ORDER: { key: string; label: string }[] = [
  { key: 'OAuth', label: 'OAUTH' },
  { key: 'CodingPlan', label: 'CODING PLAN' },
  { key: 'Api', label: 'API' },
]

export function ChannelSettings() {
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [configuredIds, setConfiguredIds] = useState<Set<string>>(new Set())
  const [modelCounts, setModelCounts] = useState<Map<string, number>>(new Map())

  const refreshData = useCallback(async () => {
    const [allProviders, ids, allModels] = await Promise.all([
      listProviders(),
      listConfiguredProviders(),
      getAllConfiguredModels(),
    ])
    setProviders(allProviders)
    setConfiguredIds(new Set(ids))
    const counts = new Map<string, number>()
    allModels.forEach(([pid, mids]) => counts.set(pid, mids.length))
    setModelCounts(counts)
  }, [])

  useEffect(() => {
    void refreshData()
  }, [refreshData])

  const selected = useMemo(
    () => providers.find((p) => p.id === selectedId) ?? null,
    [providers, selectedId],
  )

  const grouped = useMemo(() => {
    const map = new Map<string, ProviderInfo[]>()
    for (const p of providers) {
      const cat = p.serviceCategory || 'Api'
      if (!map.has(cat)) map.set(cat, [])
      map.get(cat)!.push(p)
    }
    return map
  }, [providers])

  return (
    <div className="flex-1 min-h-0 grid grid-cols-[220px_1fr] grid-rows-1 overflow-hidden">
      {/* Left: grouped provider list */}
      <div className="overflow-y-auto border-r border-border bg-muted/20">
        {CATEGORY_ORDER.map(({ key, label }) => {
          const items = grouped.get(key) ?? []
          if (items.length === 0) return null
          return (
            <div key={key} className="py-1.5">
              <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/50">
                {label}
              </div>
              {items.map((p) => {
                const isConfigured = configuredIds.has(p.id)
                const isSelected = selectedId === p.id
                const count = modelCounts.get(p.id) ?? 0
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => setSelectedId(p.id)}
                    className={cn(
                      'flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-accent/50',
                      isSelected && 'bg-accent text-accent-foreground',
                    )}
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        className={cn(
                          'h-1.5 w-1.5 shrink-0 rounded-full',
                          isConfigured ? 'bg-green-500' : 'bg-muted-foreground/25',
                        )}
                        aria-hidden
                      />
                      <span className="truncate">{p.displayName}</span>
                    </div>
                    <span className="shrink-0 text-[10.5px] text-muted-foreground/50">
                      {count > 0 ? count : ''}
                    </span>
                  </button>
                )
              })}
            </div>
          )
        })}
      </div>

      {/* Right: detail panel */}
      <div className="overflow-y-auto px-6 py-5">
        {selected ? (
          <ProviderDetail
            provider={selected}
            isConfigured={configuredIds.has(selected.id)}
            onSaved={() => void refreshData()}
          />
        ) : (
          <ProviderEmptyState />
        )}
      </div>
    </div>
  )
}

function ProviderEmptyState() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-[12px] text-muted-foreground">
      <span>从左侧选择一个服务商以配置 API Key、Base URL 与可用模型。</span>
      <span className="text-[10.5px] text-muted-foreground/60">
        三个分组：OAuth · Coding Plan（订阅制）· API（标准 Key 服务）
      </span>
    </div>
  )
}

interface ProviderDetailProps {
  provider: ProviderInfo
  isConfigured: boolean
  onSaved: () => void
}

export function ProviderDetail({ provider, isConfigured, onSaved }: ProviderDetailProps) {
  const [apiKey, setApiKey] = useState('')
  const [hasApiKey, setHasApiKey] = useState(false)
  const [maskedKey, setMaskedKey] = useState<string | null>(null)
  const [baseUrl, setBaseUrl] = useState(provider.defaultBaseUrl)
  const [apiType, setApiType] = useState(provider.defaultApi || 'openai-completions')
  const [availableModels, setAvailableModels] = useState<ModelInfo[]>([])
  const [selectedModelIds, setSelectedModelIds] = useState<Set<string>>(new Set())
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    setBaseUrl(provider.defaultBaseUrl)
    setApiType(provider.defaultApi || 'openai-completions')
    setApiKey('')
    setHasApiKey(false)
    setMaskedKey(null)
    setAvailableModels([])
    setSelectedModelIds(new Set())

    void (async () => {
      const [cfg, savedModelIds] = await Promise.all([
        getProviderConfig(provider.id),
        getConfiguredModels(provider.id),
      ])
      if (cfg) {
        setBaseUrl(cfg.baseUrl ?? provider.defaultBaseUrl)
        if (cfg.api) setApiType(cfg.api)
        setHasApiKey(cfg.hasApiKey)
        setMaskedKey(cfg.maskedKey ?? null)
      }
      if (savedModelIds.length > 0) {
        setAvailableModels(
          savedModelIds.map((id) => ({
            id,
            name: id,
            modality: 'Text',
            reasoning: false,
            supportsReasoningEffort: false,
          })),
        )
        setSelectedModelIds(new Set(savedModelIds))
      }
    })()
  }, [provider.id, provider.defaultBaseUrl, provider.defaultApi])

  const handleLoadModels = useCallback(async () => {
    setBusy(true)
    try {
      const models = await listProviderModels({
        providerId: provider.id,
        baseUrl: baseUrl || provider.defaultBaseUrl,
        apiKey: apiKey || null,
      })
      setAvailableModels(models)
      if (models.length === 0) {
        toast.warning('未拉取到模型，请确认 Base URL / API Key 正确。')
      }
    } catch (e) {
      toast.error(`读取模型失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider.id, provider.defaultBaseUrl, baseUrl, apiKey])

  const toggleModel = useCallback((id: string) => {
    setSelectedModelIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])

  const handleTest = useCallback(async () => {
    if (provider.authType === 'apikey' && !apiKey) {
      toast.warning('请先填写 API Key。')
      return
    }
    setBusy(true)
    try {
      const result = await testProviderConnection({
        providerId: provider.id,
        baseUrl: baseUrl || provider.defaultBaseUrl,
        apiKey: apiKey || null,
      })
      if (result.success) {
        toast.success(`连接成功${result.latencyMs ? ` (${result.latencyMs}ms)` : ''}`)
      } else {
        toast.error(`连接失败: ${result.message}`)
      }
    } catch (e) {
      toast.error(`连接失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider, apiKey, baseUrl])

  const handleSave = useCallback(async () => {
    if (selectedModelIds.size === 0 && availableModels.length > 0) {
      toast.warning('请至少选择一个模型。')
      return
    }
    setBusy(true)
    try {
      await configureProviderWithModels({
        providerId: provider.id,
        displayName: provider.displayName,
        apiKey: apiKey || null,
        baseUrl: baseUrl || null,
        api: apiType,
        modelIds: Array.from(selectedModelIds),
      })
      toast.success('已保存')
      onSaved()
    } catch (e) {
      toast.error(`保存失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider, apiKey, baseUrl, apiType, availableModels, selectedModelIds, onSaved])

  const handleDelete = useCallback(async () => {
    setBusy(true)
    try {
      await removeProviderConfig(provider.id)
      toast.success('已删除')
      onSaved()
    } catch (e) {
      toast.error(`删除失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider.id, onSaved])

  return (
    <div className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-[15px] font-medium">{provider.displayName}</h3>
          <p className="text-[11px] text-muted-foreground">
            {provider.id} · {provider.serviceCategory}
          </p>
        </div>
        {isConfigured && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="gap-1 text-destructive hover:text-destructive"
            disabled={busy}
            onClick={() => void handleDelete()}
          >
            <Trash2 className="h-3.5 w-3.5" />
            删除
          </Button>
        )}
      </div>

      {/* Credentials grid */}
      <div className="grid grid-cols-[80px_1fr] items-center gap-x-3 gap-y-2 text-[12px]">
        <label className="text-muted-foreground">API Key</label>
        {provider.authType === 'oauth' || provider.authType === 'OAuth' ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled
            className="justify-self-start text-[11px]"
          >
            通过 OAuth 连接（即将上线）
          </Button>
        ) : (
          <SettingsSecretInput
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            disabled={provider.authType === 'none' || provider.authType === 'None'}
            placeholder={
              provider.authType === 'none' || provider.authType === 'None'
                ? '无需 API Key'
                : hasApiKey && !apiKey
                  ? `已配置 ••••${maskedKey ?? ''}（输入以更新）`
                  : 'sk-…'
            }
            className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-[12px] outline-none placeholder:text-muted-foreground/50 focus:ring-1 focus:ring-ring disabled:opacity-50"
          />
        )}

        <label className="text-muted-foreground">Base URL</label>
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          placeholder={provider.defaultBaseUrl}
          className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-[12px] outline-none placeholder:text-muted-foreground/50 focus:ring-1 focus:ring-ring"
        />

        <label className="text-muted-foreground">API 类型</label>
        <select
          value={apiType}
          onChange={(e) => setApiType(e.target.value)}
          className="rounded-md border border-input bg-background px-2 py-1.5 text-[12px] outline-none focus:ring-1 focus:ring-ring"
        >
          {API_TYPE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Models section */}
      <div className="border-t border-border pt-3">
        <div className="mb-2 flex items-center justify-between">
          <div className="text-[12px] text-muted-foreground">
            已添加的模型{' '}
            <span className="text-muted-foreground/50">{selectedModelIds.size}</span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="gap-1"
              disabled={busy || provider.authType === 'oauth' || provider.authType === 'OAuth'}
              onClick={() => void handleLoadModels()}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', busy && 'animate-spin')} />
              读取模型
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => void handleTest()}
            >
              测试连接
            </Button>
          </div>
        </div>

        {availableModels.length === 0 ? (
          <p className="rounded-md border border-dashed border-border bg-muted/30 px-3 py-4 text-center text-[11px] text-muted-foreground">
            暂无已保存模型。点击「读取模型」从供应商加载可用模型。
          </p>
        ) : (
          <ul className="divide-y divide-border rounded-md border border-border">
            {availableModels.map((model) => {
              const checked = selectedModelIds.has(model.id)
              return (
                <li key={model.id}>
                  <button
                    type="button"
                    onClick={() => toggleModel(model.id)}
                    className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-[12px] hover:bg-accent/30"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        className={cn(
                          'flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border',
                          checked
                            ? 'border-primary bg-primary text-primary-foreground'
                            : 'border-muted-foreground/30 bg-background',
                        )}
                      >
                        {checked ? <Check className="h-2.5 w-2.5" /> : null}
                      </span>
                      <span className="truncate font-medium">{model.name}</span>
                      {model.id !== model.name && (
                        <span className="truncate text-[10.5px] text-muted-foreground/50">
                          {model.id}
                        </span>
                      )}
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {model.reasoning && (
                        <span className="rounded bg-primary/10 px-1.5 text-[9.5px] text-primary">
                          thinking
                        </span>
                      )}
                      {model.contextWindow ? (
                        <span className="rounded bg-muted px-1.5 text-[9.5px] text-muted-foreground">
                          {(model.contextWindow / 1000).toFixed(0)}K
                        </span>
                      ) : null}
                    </div>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <div className="flex justify-end pt-2">
        <Button type="button" size="sm" disabled={busy} onClick={() => void handleSave()}>
          保存
        </Button>
      </div>
    </div>
  )
}
