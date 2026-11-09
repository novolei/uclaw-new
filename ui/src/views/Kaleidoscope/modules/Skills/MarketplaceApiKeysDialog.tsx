/**
 * MarketplaceApiKeysDialog — 技能市场 API Key 设置。
 * 只显示「已设置 / 未设置」，从不回显 Key；输入框只写不读，保存后清空。
 * skills.sh：Key 解锁热门榜、详情和审计；skillsmp.com：Key 提高搜索额度。
 */
import * as React from 'react'
import { getMarketplaceApiKeyStatus, setMarketplaceApiKey } from '@/lib/tauri-bridge'
import type { MarketplaceProvider } from '@/lib/types'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type Status = { skillsSh: boolean; skillsmp: boolean }

const PROVIDERS: { value: MarketplaceProvider; label: string; field: keyof Status; hint: string }[] = [
  { value: 'skills_sh', label: 'skills.sh', field: 'skillsSh', hint: '解锁热门榜、技能详情和安全审计；搜索不需要 Key。' },
  { value: 'skillsmp', label: 'skillsmp.com', field: 'skillsmp', hint: '可选；匿名每天 50 次搜索，有 Key 提高到 500 次。' },
]

export function MarketplaceApiKeysDialog({
  open,
  onOpenChange,
  onSaved,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved?: () => void
}): React.ReactElement {
  const [status, setStatus] = React.useState<Status | null>(null)
  const [drafts, setDrafts] = React.useState<Record<string, string>>({})
  const [busy, setBusy] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  const refresh = React.useCallback(() => {
    getMarketplaceApiKeyStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)))
  }, [])

  React.useEffect(() => {
    if (open) refresh()
  }, [open, refresh])

  const save = async (provider: MarketplaceProvider, key: string) => {
    setBusy(provider)
    setError(null)
    try {
      await setMarketplaceApiKey(provider, key)
      setDrafts((d) => ({ ...d, [provider]: '' }))
      refresh()
      onSaved?.()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>技能市场 API Key</DialogTitle>
          <DialogDescription>Key 只保存在本机，界面不会再显示它。</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          {PROVIDERS.map((p) => {
            const isSet = status?.[p.field] ?? false
            const draft = drafts[p.value] ?? ''
            const inputId = `marketplace-key-${p.value}`
            return (
              <div key={p.value} className="flex flex-col gap-1.5">
                <div className="flex items-center gap-2">
                  <label htmlFor={inputId} className="text-sm font-medium">
                    {p.label}
                  </label>
                  {status && (
                    <span
                      className={cn(
                        'text-[10.5px] px-2 py-0.5 rounded-full border',
                        isSet
                          ? 'bg-emerald-500/15 text-emerald-700 border-emerald-500/30 dark:text-emerald-400'
                          : 'bg-muted text-muted-foreground border-border',
                      )}
                    >
                      {isSet ? '已设置' : '未设置'}
                    </span>
                  )}
                </div>
                <div className="text-xs text-muted-foreground">{p.hint}</div>
                <div className="flex items-center gap-1.5">
                  <Input
                    id={inputId}
                    aria-label={`${p.label} API Key`}
                    type="password"
                    autoComplete="off"
                    value={draft}
                    onChange={(e) => setDrafts((d) => ({ ...d, [p.value]: e.target.value }))}
                    placeholder={isSet ? '粘贴新的 API Key 以替换' : '粘贴新的 API Key'}
                    className="h-8 text-[12px]"
                  />
                  <Button
                    size="sm"
                    className="h-8"
                    disabled={!draft.trim() || busy !== null}
                    onClick={() => void save(p.value, draft)}
                    aria-label={`保存 ${p.label}`}
                  >
                    保存
                  </Button>
                  {isSet && (
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-8"
                      disabled={busy !== null}
                      onClick={() => void save(p.value, '')}
                      aria-label={`清除 ${p.label}`}
                    >
                      清除
                    </Button>
                  )}
                </div>
              </div>
            )
          })}
          {error && <div className="text-xs text-red-500/90">{error}</div>}
        </div>
      </DialogContent>
    </Dialog>
  )
}
