/**
 * WorkspaceSkillTagsEditor — chip input for per-workspace skill scoping (V19+).
 *
 * Empty tag set = no filter (every enabled skill stays in the manifest).
 * Non-empty enables the intersection rule:
 *   - skills without tags stay global (always included)
 *   - skills with tags need at least one match with the workspace's tags
 *
 * Tag normalization happens server-side (trim + lowercase + dedup), and
 * the `setWorkspaceSkillTags` IPC returns the normalized list so this
 * component echoes back what's actually stored.
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { activeWorkspaceIdAtom, workspacesAtom } from '@/atoms/workspace'
import { getWorkspaceSkillTags, setWorkspaceSkillTags } from '@/lib/tauri-bridge'
import { Button } from '@/components/ui/button'
import { toast } from 'sonner'
import { X } from 'lucide-react'

export function WorkspaceSkillTagsEditor(): React.ReactElement | null {
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspace = React.useMemo(
    () => workspaces.find((w) => w.id === activeId),
    [workspaces, activeId],
  )

  const [tags, setTags] = React.useState<string[]>([])
  const [draft, setDraft] = React.useState('')
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    if (!activeId) {
      setTags([])
      return
    }
    setLoading(true)
    getWorkspaceSkillTags(activeId)
      .then(setTags)
      .catch((e) => toast.error('读取标签失败', { description: String(e) }))
      .finally(() => setLoading(false))
  }, [activeId])

  const persist = React.useCallback(
    async (next: string[]) => {
      if (!activeId) return
      setSaving(true)
      try {
        const normalized = await setWorkspaceSkillTags(activeId, next)
        setTags(normalized)
      } catch (e) {
        toast.error('保存失败', { description: String(e) })
      } finally {
        setSaving(false)
      }
    },
    [activeId],
  )

  const addTag = React.useCallback(() => {
    const trimmed = draft.trim()
    if (!trimmed) return
    if (tags.includes(trimmed.toLowerCase())) {
      setDraft('')
      return
    }
    void persist([...tags, trimmed])
    setDraft('')
  }, [draft, tags, persist])

  const removeTag = React.useCallback(
    (tag: string) => {
      void persist(tags.filter((t) => t !== tag))
    },
    [tags, persist],
  )

  if (!activeId) {
    return (
      <div className="text-xs text-muted-foreground py-2">
        请先选择一个工作区。
      </div>
    )
  }

  return (
    <div className="space-y-3">
      <div className="text-xs text-muted-foreground">
        当前工作区：<span className="font-medium text-foreground/80">
          {activeWorkspace?.name ?? activeId}
        </span>
        {tags.length === 0 && (
          <span className="ml-2 text-muted-foreground/60">
            (未设标签 = 所有 Skill 都可见，默认)
          </span>
        )}
      </div>

      <div className="flex flex-wrap gap-1.5 items-center min-h-[28px]">
        {tags.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded-full border bg-primary/10 text-primary border-primary/20"
          >
            {tag}
            <button
              type="button"
              onClick={() => removeTag(tag)}
              disabled={saving}
              className="hover:text-primary/70 disabled:opacity-50"
              aria-label={`移除标签 ${tag}`}
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        {loading && (
          <span className="text-[10px] text-muted-foreground/60">加载中…</span>
        )}
      </div>

      <div className="flex items-center gap-2">
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              addTag()
            } else if (e.key === ',') {
              e.preventDefault()
              addTag()
            }
          }}
          placeholder="输入标签，回车或逗号添加"
          disabled={saving}
          className="flex-1 text-xs px-2 py-1 rounded border border-border bg-background focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
        />
        <Button size="sm" variant="outline" onClick={addTag} disabled={saving || !draft.trim()}>
          添加
        </Button>
      </div>
    </div>
  )
}
