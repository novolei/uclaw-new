import * as React from 'react'
import { useSetAtom } from 'jotai'
import { HardDrive, Trash2, Sparkles } from 'lucide-react'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'
import { localModelList, localModelDelete, type LocalInstalledModel } from '@/lib/tauri-bridge'

export function MiniCPMSettings(): React.ReactElement {
  const setWizard = useSetAtom(minicpmWizardAtom)
  const [models, setModels] = React.useState<LocalInstalledModel[]>([])
  const [busy, setBusy] = React.useState(false)

  const refresh = React.useCallback(async () => {
    try { setModels(await localModelList()) } catch { /* ignore */ }
  }, [])

  React.useEffect(() => { void refresh() }, [refresh])

  const startWizard = () => setWizard((s) => ({ ...s, step: 'intro', error: null }))

  const remove = async () => {
    setBusy(true)
    try { await localModelDelete(); await refresh() } finally { setBusy(false) }
  }

  const installed = models[0]?.installed ?? false
  const totalMb = Math.round((models[0]?.total_bytes ?? 0) / 1_000_000)

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-2 text-sm font-medium">
        <HardDrive size={16} /> 本地模型 (MiniCPM5-1B)
      </div>
      <p className="text-xs text-muted-foreground">
        本地运行的轻量模型，用于「轻工具」与「记忆摘要」场景，省 token、保护隐私、可离线。
      </p>
      <div className="rounded-md border border-border/50 p-3 text-xs space-y-2">
        <div>状态：{installed ? `已安装（${totalMb} MB）` : '未安装'}</div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={startWizard}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1.5 text-primary-foreground"
          >
            <Sparkles size={14} /> {installed ? '重新运行向导' : '开始设置'}
          </button>
          {installed && (
            <button
              type="button"
              onClick={remove}
              disabled={busy}
              className="inline-flex items-center gap-1 rounded-md border border-border px-3 py-1.5 disabled:opacity-50"
            >
              <Trash2 size={14} /> 删除模型
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
