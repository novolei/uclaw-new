import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Plus, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import {
  listAlwaysAllowedPaths,
  addAlwaysAllowedPath,
  removeAlwaysAllowedPath,
  listSessionAllowedPaths,
  promoteSessionPathToGlobal,
  openFolderDialog,
} from '@/lib/tauri-bridge'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'

export function WorkspaceSandboxSettings(): React.ReactElement {
  const sessionId = useAtomValue(currentAgentSessionIdAtom)
  const [global, setGlobal] = React.useState<string[]>([])
  const [session, setSession] = React.useState<string[]>([])

  const refreshGlobal = React.useCallback(async () => {
    try { setGlobal(await listAlwaysAllowedPaths()) } catch (err) { console.error('[sandbox]', err) }
  }, [])

  const refreshSession = React.useCallback(async () => {
    if (!sessionId) { setSession([]); return }
    try { setSession(await listSessionAllowedPaths(sessionId)) } catch (err) { console.error('[sandbox]', err) }
  }, [sessionId])

  React.useEffect(() => { void refreshGlobal() }, [refreshGlobal])
  React.useEffect(() => { void refreshSession() }, [refreshSession])

  const handleAdd = async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      await addAlwaysAllowedPath(picked.path)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`添加失败: ${msg}`)
    }
  }

  const handleRemove = async (p: string) => {
    try {
      await removeAlwaysAllowedPath(p)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`删除失败: ${msg}`)
    }
  }

  const handlePromote = async (p: string) => {
    if (!sessionId) return
    try {
      await promoteSessionPathToGlobal(sessionId, p)
      await refreshGlobal()
      await refreshSession()
      toast.success('已升级为永久允许')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`升级失败: ${msg}`)
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">始终允许的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">Agent 在任何工作区都可以访问这些路径,无需提示。</p>
        <div className="rounded-md border bg-muted/30">
          {global.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">尚未添加任何路径。</div>
          )}
          {global.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handleRemove(p)}
                className={cn('shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10')}
                title="删除"
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={handleAdd}
          className="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md bg-primary/10 text-primary hover:bg-primary/20"
        >
          <Plus className="size-3.5" />
          添加路径
        </button>
      </section>

      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">本会话已临时授权的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">
          仅本会话有效,重启应用后清除。点"升级为永久"加入上面的列表。
        </p>
        <div className="rounded-md border bg-muted/30">
          {!sessionId && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">没有活动会话。</div>
          )}
          {sessionId && session.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">本会话没有触发过外部路径授权。</div>
          )}
          {sessionId && session.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handlePromote(p)}
                className="shrink-0 px-2 py-0.5 text-[11px] rounded text-primary hover:bg-primary/10"
              >
                升级为永久
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}
