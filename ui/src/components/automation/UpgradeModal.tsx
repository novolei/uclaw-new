import * as React from 'react'
import { ArrowRight, Plus, Minus, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { getMarketplaceDetail, installMarketplaceHuman } from '@/lib/tauri-bridge'
import { diffBundledSkills, type SkillDiff } from '@/lib/skill-diff'

interface Props {
  slug: string
  name: string
  currentVersion: string
  installedSkillIds: string[]
  onClose: () => void
  onUpgraded: () => void
}

/** Extract bundled-skill ids from a parsed Humane spec's requires.skills[]. */
function bundledSkillIds(parsedSpecJson: unknown): string[] {
  const requires = (parsedSpecJson as { requires?: { skills?: unknown } } | null)?.requires
  const skills = Array.isArray(requires?.skills) ? requires.skills : []
  return skills
    .filter(
      (s): s is { id: string; bundled?: boolean } =>
        typeof s === 'object' && s !== null && typeof (s as { id?: unknown }).id === 'string',
    )
    .filter((s) => s.bundled === true)
    .map((s) => s.id)
}

export function UpgradeModal({
  slug,
  name,
  currentVersion,
  installedSkillIds,
  onClose,
  onUpgraded,
}: Props): React.ReactElement {
  const [newVersion, setNewVersion] = React.useState<string | null>(null)
  const [diff, setDiff] = React.useState<SkillDiff | null>(null)
  const [loadError, setLoadError] = React.useState<string | null>(null)
  const [upgrading, setUpgrading] = React.useState(false)

  React.useEffect(() => {
    let cancelled = false
    getMarketplaceDetail(slug)
      .then((detail) => {
        if (cancelled) return
        setNewVersion(detail.item.version)
        setDiff(diffBundledSkills(installedSkillIds, bundledSkillIds(detail.parsedSpecJson)))
      })
      .catch((err) => {
        if (!cancelled) setLoadError(String(err))
      })
    return () => {
      cancelled = true
    }
  }, [slug, installedSkillIds])

  const handleUpgrade = async () => {
    setUpgrading(true)
    try {
      await installMarketplaceHuman(slug)
      toast.success(`已升级 ${name} 到 v${newVersion}`)
      onUpgraded()
      onClose()
    } catch (err) {
      toast.error(`升级失败：${String(err)}`)
      setUpgrading(false)
    }
  }

  return (
    <Dialog open onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="text-[15px]">升级 {name}</DialogTitle>
          {/* Version bump line: show only currentVersion here; newVersion appears only in the confirm button */}
          <DialogDescription className="flex items-center gap-1.5 text-[12px] tabular-nums">
            <span>v{currentVersion}</span>
            <ArrowRight size={12} className="text-muted-foreground" />
            <span className="text-muted-foreground">{newVersion ? '新版本可用' : '…'}</span>
          </DialogDescription>
        </DialogHeader>

        {loadError && (
          <div className="text-[12px] text-danger py-2">加载新版本信息失败：{loadError}</div>
        )}

        {diff && (
          <div className="flex flex-col gap-2 py-1">
            {diff.added.length === 0 && diff.removed.length === 0 ? (
              <div className="text-[12px] text-muted-foreground">本次升级不改变 bundled skill 集合。</div>
            ) : (
              <ul className="flex flex-col gap-1 text-[12px]">
                {diff.added.map((id) => (
                  <li key={`a-${id}`} className="flex items-center gap-1.5 text-success">
                    <Plus size={12} />
                    {id}
                  </li>
                ))}
                {diff.removed.map((id) => (
                  <li key={`r-${id}`} className="flex items-center gap-1.5 text-muted-foreground line-through">
                    <Minus size={12} />
                    {id}
                  </li>
                ))}
              </ul>
            )}
            {diff.kept.length > 0 && (
              <div className="text-[11px] text-muted-foreground/70">
                保留 {diff.kept.length} 个 skill：{diff.kept.join('、')}
              </div>
            )}
          </div>
        )}

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onClose} disabled={upgrading}>
            取消
          </Button>
          <Button
            onClick={handleUpgrade}
            disabled={upgrading || newVersion === null || loadError !== null}
          >
            {upgrading && <Loader2 size={13} className="animate-spin mr-1" />}
            升级到 v{newVersion ?? '…'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
