import * as React from 'react'
import { useSetAtom } from 'jotai'
import { HardDrive, Trash2, Sparkles, User, FolderOpen } from 'lucide-react'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import { activePetPersonaAtom } from '@/atoms/pet-persona-atoms'
import {
  localModelList,
  localModelDelete,
  petPersonaList,
  petPersonaGetActive,
  petPersonaSetActive,
  petPersonaImport,
  petPersonaDelete,
  openPersonaFileDialog,
  type LocalInstalledModel,
  type PetPersona,
} from '@/lib/tauri-bridge'

export function MiniCPMSettings(): React.ReactElement {
  const setWizard = useSetAtom(minicpmWizardAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const [models, setModels] = React.useState<LocalInstalledModel[]>([])
  const [busy, setBusy] = React.useState(false)

  // ── Pet persona state ───────────────────────────────────────────────
  const [personas, setPersonas] = React.useState<PetPersona[]>([])
  const [activeId, setActiveId] = React.useState<string | null>(null)
  const [importPath, setImportPath] = React.useState('')
  const [importError, setImportError] = React.useState<string | null>(null)
  const [importBusy, setImportBusy] = React.useState(false)
  const setActivePersona = useSetAtom(activePetPersonaAtom)

  const refresh = React.useCallback(async () => {
    try { setModels(await localModelList()) } catch { /* ignore */ }
  }, [])

  const refreshPersonas = React.useCallback(async () => {
    try {
      const [list, active] = await Promise.all([petPersonaList(), petPersonaGetActive()])
      setPersonas(list)
      setActiveId(active.id)
      setActivePersona(active)
    } catch { /* ignore — backend may not be ready yet */ }
  }, [setActivePersona])

  React.useEffect(() => { void refresh() }, [refresh])
  React.useEffect(() => { void refreshPersonas() }, [refreshPersonas])

  const startWizard = () => { setSettingsOpen(false); setWizard((s) => ({ ...s, step: 'intro', error: null })) }

  const remove = async () => {
    setBusy(true)
    try { await localModelDelete(); await refresh() } finally { setBusy(false) }
  }

  const choose = async (id: string) => {
    await petPersonaSetActive(id)
    await refreshPersonas()
  }

  const removePersona = async (id: string) => {
    await petPersonaDelete(id)
    await refreshPersonas()
  }

  const doImportByPath = async (path: string) => {
    const p = path.trim()
    if (!p) return
    setImportBusy(true)
    setImportError(null)
    try {
      await petPersonaImport(p)
      setImportPath('')
      await refreshPersonas()
    } catch (e) {
      setImportError(e instanceof Error ? e.message : String(e))
    } finally {
      setImportBusy(false)
    }
  }

  const doImportViaDialog = async () => {
    setImportError(null)
    const path = await openPersonaFileDialog()
    if (!path) return
    await doImportByPath(path)
  }

  const installed = models[0]?.installed ?? false
  const totalMb = Math.round((models[0]?.total_bytes ?? 0) / 1_000_000)

  return (
    <div className="p-4 space-y-4">
      {/* ── Model management block (Slice D) ── */}
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

      {/* ── 宠物角色 section (Slice F) ── */}
      <section data-testid="pet-persona-section" className="space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <User size={16} /> 宠物角色
        </div>
        <p className="text-xs text-muted-foreground">
          选择宠物的对话角色。内置角色随时可用，导入角色支持自定义系统提示词与 LoRA 适配器。
        </p>

        {/* Persona list */}
        <div className="rounded-md border border-border/50 divide-y divide-border/30">
          {personas.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">暂无角色数据，请检查后端服务。</div>
          )}
          {personas.map((p) => (
            <div
              key={p.id}
              data-testid={`pet-persona-row-${p.id}`}
              className="flex items-center justify-between gap-2 px-3 py-2 text-xs"
            >
              <div className="flex items-center gap-2 min-w-0">
                <span className="font-medium truncate">{p.name}</span>
                {p.source === 'imported' && (
                  <span className="text-muted-foreground shrink-0">[导入]</span>
                )}
              </div>
              <div className="flex items-center gap-1.5 shrink-0">
                {activeId === p.id ? (
                  <span className="rounded-full bg-primary/15 px-2 py-0.5 text-primary text-[10px] font-medium">
                    使用中
                  </span>
                ) : (
                  <button
                    type="button"
                    data-testid={`pet-persona-select-${p.id}`}
                    onClick={() => { void choose(p.id) }}
                    className="rounded-md border border-border px-2 py-0.5 hover:bg-muted"
                  >
                    选择
                  </button>
                )}
                {p.source === 'imported' && (
                  <button
                    type="button"
                    data-testid={`pet-persona-delete-${p.id}`}
                    onClick={() => { void removePersona(p.id) }}
                    className="rounded-md border border-destructive/50 px-2 py-0.5 text-destructive hover:bg-destructive/10"
                  >
                    <Trash2 size={12} />
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>

        {/* Import control — native file dialog (plugin-dialog available) */}
        <div className="flex flex-col gap-1.5">
          <div className="flex gap-2 items-center">
            <button
              type="button"
              data-testid="pet-persona-import-dialog-btn"
              onClick={() => { void doImportViaDialog() }}
              disabled={importBusy}
              className="inline-flex items-center gap-1 rounded-md border border-border px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
            >
              <FolderOpen size={13} /> 导入角色文件…
            </button>
            {/* Fallback: paste-path input for power users */}
            <input
              type="text"
              data-testid="pet-persona-import-path-input"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              placeholder="或粘贴 JSON 路径…"
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
            <button
              type="button"
              data-testid="pet-persona-import-path-btn"
              onClick={() => { void doImportByPath(importPath) }}
              disabled={importBusy || !importPath.trim()}
              className="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
            >
              导入
            </button>
          </div>
          {importError && (
            <p className="text-xs text-destructive" data-testid="pet-persona-import-error">
              导入失败：{importError}
            </p>
          )}
        </div>
      </section>
    </div>
  )
}
