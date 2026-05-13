import * as React from 'react'
import { Loader2, AlertCircle } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  listMarketplaceHumans,
  installMarketplaceHuman,
  type MarketplaceItem,
  type HumaneSpecRow,
} from '@/lib/tauri-bridge'
import { MarketplaceCard } from './MarketplaceCard'

interface Props {
  open: boolean
  onClose: () => void
  onInstalled: (row: HumaneSpecRow) => void
}

export function MarketplaceModal({ open, onClose, onInstalled }: Props): React.ReactElement {
  const [items, setItems] = React.useState<MarketplaceItem[] | null>(null)
  const [loadError, setLoadError] = React.useState<string | null>(null)
  const [installingSlug, setInstallingSlug] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!open) return
    setItems(null)
    setLoadError(null)
    let cancelled = false
    listMarketplaceHumans()
      .then((list) => {
        if (cancelled) return
        // Sort by category then name for stable UI
        list.sort(
          (a, b) =>
            a.category.localeCompare(b.category) || a.name.localeCompare(b.name),
        )
        setItems(list)
      })
      .catch((err) => {
        if (!cancelled) setLoadError(String(err))
      })
    return () => {
      cancelled = true
    }
  }, [open])

  const handleInstall = async (slug: string) => {
    setInstallingSlug(slug)
    try {
      const row = await installMarketplaceHuman(slug)
      toast.success(`已安装：${row.name}`)
      onInstalled(row)
      onClose()
    } catch (err) {
      toast.error(`安装失败：${String(err)}`)
    } finally {
      setInstallingSlug(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-w-3xl max-h-[80vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>数字人市场</DialogTitle>
          <DialogDescription>
            从 Digital Human Protocol 注册表浏览并安装数字人（来源：openkursar/digital-human-protocol）
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto -mx-6 px-6">
          {/* Loading state */}
          {items === null && !loadError && (
            <div className="flex items-center justify-center gap-2 py-12 text-muted-foreground">
              <Loader2 size={16} className="animate-spin" />
              <span>正在加载注册表…</span>
            </div>
          )}

          {/* Error state */}
          {loadError && (
            <div className="flex flex-col items-center gap-3 py-12 text-muted-foreground">
              <AlertCircle size={24} className="text-red-500" />
              <div className="text-center">
                <p className="text-sm font-medium">无法加载注册表</p>
                <p className="text-xs mt-1 max-w-md">{loadError}</p>
              </div>
              <Button variant="outline" size="sm" onClick={onClose}>
                关闭
              </Button>
            </div>
          )}

          {/* Empty state */}
          {items !== null && items.length === 0 && (
            <div className="flex flex-col items-center gap-2 py-12 text-muted-foreground">
              <p className="text-sm">注册表为空</p>
            </div>
          )}

          {/* Grid */}
          {items && items.length > 0 && (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pb-4">
              {items.map((item) => (
                <MarketplaceCard
                  key={item.slug}
                  item={item}
                  installing={installingSlug === item.slug}
                  onInstall={handleInstall}
                />
              ))}
            </div>
          )}
        </div>

        {items && (
          <div className="text-xs text-muted-foreground text-right pt-2 border-t border-border">
            共 {items.length} 个数字人
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
