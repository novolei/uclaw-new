/**
 * PromptSettings — 系统提示词管理组件
 *
 * 功能：
 *  - 列出所有系统提示词
 *  - 选择当前使用的提示词（设为默认）
 *  - 创建 / 编辑 / 删除自定义提示词
 *  - 内置提示词（"默认"）不可删除或编辑
 */

import * as React from 'react'
import { Plus, Trash2, Check, Pencil, Star, StarOff, Loader2, History, ChevronDown, ChevronUp } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  getSystemPromptConfig,
  createSystemPrompt,
  updateSystemPrompt,
  deleteSystemPrompt,
  setDefaultPrompt,
  getSystemPromptVersions,
} from '@/lib/tauri-bridge'
import type { SystemPrompt, SystemPromptConfig, SystemPromptVersion } from '@/lib/chat-types'
import { BUILTIN_DEFAULT_ID } from '@/lib/chat-types'

export function PromptSettings(): React.ReactElement {
  const [config, setConfig] = React.useState<SystemPromptConfig | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [expandedId, setExpandedId] = React.useState<string | null>(null)
  const [editingId, setEditingId] = React.useState<string | null>(null)
  const [editName, setEditName] = React.useState('')
  const [editContent, setEditContent] = React.useState('')
  const [showNewForm, setShowNewForm] = React.useState(false)
  const [newName, setNewName] = React.useState('')
  const [newContent, setNewContent] = React.useState('')
  const [saving, setSaving] = React.useState(false)
  const [showVersionsId, setShowVersionsId] = React.useState<string | null>(null)
  const [versions, setVersions] = React.useState<SystemPromptVersion[]>([])
  const [loadingVersions, setLoadingVersions] = React.useState(false)

  const loadConfig = React.useCallback(async () => {
    try {
      const cfg = await getSystemPromptConfig()
      setConfig(cfg as SystemPromptConfig)
    } catch (e) {
      console.error('[PromptSettings] load failed:', e)
      toast.error('加载提示词配置失败')
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => { loadConfig() }, [loadConfig])

  const defaultPromptId = config?.defaultPromptId ?? BUILTIN_DEFAULT_ID

  const handleSetDefault = async (id: string) => {
    try {
      await setDefaultPrompt(id)
      setConfig((prev) => prev ? { ...prev, defaultPromptId: id } : prev)
      toast.success('已设为默认提示词')
    } catch (e) {
      console.error('[PromptSettings] setDefault failed:', e)
      toast.error('设置默认提示词失败')
    }
  }

  const handleDelete = async (id: string, name: string) => {
    if (!confirm(`确定要删除提示词「${name}」吗？`)) return
    try {
      await deleteSystemPrompt(id)
      setConfig((prev) => prev ? {
        ...prev,
        prompts: prev.prompts.filter((p) => p.id !== id),
        defaultPromptId: prev.defaultPromptId === id ? BUILTIN_DEFAULT_ID : prev.defaultPromptId,
      } : prev)
      toast.success(`已删除「${name}」`)
    } catch (e) {
      console.error('[PromptSettings] delete failed:', e)
      toast.error('删除失败')
    }
  }

  const handleStartEdit = (p: SystemPrompt) => {
    setEditingId(p.id)
    setEditName(p.name)
    setEditContent(p.content)
  }

  const handleCancelEdit = () => {
    setEditingId(null)
    setEditName('')
    setEditContent('')
  }

  const handleSaveEdit = async () => {
    if (!editingId || !editName.trim()) return
    setSaving(true)
    try {
      const updated = await updateSystemPrompt(editingId, { name: editName.trim(), content: editContent })
      setConfig((prev) => prev ? {
        ...prev,
        prompts: prev.prompts.map((p) => p.id === editingId ? { ...p, name: updated.name ?? editName, content: updated.content ?? editContent } : p),
      } : prev)
      setEditingId(null)
      toast.success('提示词已更新')
    } catch (e) {
      console.error('[PromptSettings] update failed:', e)
      toast.error('更新失败')
    } finally {
      setSaving(false)
    }
  }

  const handleCreate = async () => {
    if (!newName.trim()) return
    setSaving(true)
    try {
      const created = await createSystemPrompt({ name: newName.trim(), content: newContent })
      setConfig((prev) => prev ? {
        ...prev,
        prompts: [...prev.prompts, created],
      } : prev)
      setShowNewForm(false)
      setNewName('')
      setNewContent('')
      toast.success('提示词已创建')
    } catch (e) {
      console.error('[PromptSettings] create failed:', e)
      toast.error('创建失败')
    } finally {
      setSaving(false)
    }
  }

  const toggleVersions = async (promptId: string) => {
    if (showVersionsId === promptId) {
      setShowVersionsId(null)
      setVersions([])
      return
    }
    setShowVersionsId(promptId)
    setLoadingVersions(true)
    try {
      const v = await getSystemPromptVersions(promptId)
      setVersions(v as SystemPromptVersion[])
    } catch (e) {
      console.error('[PromptSettings] load versions failed:', e)
      toast.error('加载版本历史失败')
    } finally {
      setLoadingVersions(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const prompts = config?.prompts ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-foreground">系统提示词</h3>
        <Button
          size="sm"
          variant="outline"
          onClick={() => setShowNewForm((v) => !v)}
          disabled={showNewForm}
        >
          <Plus className="size-3.5 mr-1" />
          新建
        </Button>
      </div>

      {/* Create new form */}
      {showNewForm && (
        <div className="space-y-2 rounded border border-border/50 bg-muted/30 p-3">
          <input
            type="text"
            placeholder="提示词名称"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs outline-none focus:border-border"
          />
          <textarea
            placeholder="提示词内容…"
            value={newContent}
            onChange={(e) => setNewContent(e.target.value)}
            rows={4}
            className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs font-mono outline-none focus:border-border resize-y"
          />
          <div className="flex items-center gap-2">
            <Button size="sm" onClick={handleCreate} disabled={saving || !newName.trim()}>
              {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : null}
              创建
            </Button>
            <Button size="sm" variant="ghost" onClick={() => { setShowNewForm(false); setNewName(''); setNewContent('') }}>
              取消
            </Button>
          </div>
        </div>
      )}

      {/* Prompt list */}
      <div className="space-y-1.5">
        {prompts.map((p) => {
          const isDefault = p.id === defaultPromptId
          const isBuiltin = p.id === BUILTIN_DEFAULT_ID
          const isExpanded = expandedId === p.id
          const isEditing = editingId === p.id

          return (
            <div
              key={p.id}
              className={cn(
                'rounded border transition-colors',
                isDefault ? 'border-primary/30 bg-primary/5' : 'border-border/50 bg-background',
              )}
            >
              {/* Header row */}
              <div
                className="flex items-center gap-2 px-3 py-2 cursor-pointer"
                onClick={() => setExpandedId(isExpanded ? null : p.id)}
              >
                <span className={cn(
                  'flex-1 text-xs font-medium truncate',
                  isDefault ? 'text-primary' : 'text-foreground',
                )}>
                  {p.name}
                  {isBuiltin && <span className="ml-1.5 text-[10px] text-muted-foreground">(内置)</span>}
                </span>
                <span className="text-[10px] text-muted-foreground whitespace-nowrap">
                  {p.content.length} 字符
                </span>
                {isDefault && (
                  <span className="text-[10px] text-primary font-medium whitespace-nowrap">
                    当前使用
                  </span>
                )}
              </div>

              {/* Expanded content / edit form */}
              {isExpanded && (
                <div className="px-3 pb-3 pt-0 border-t border-border/30">
                  {isEditing ? (
                    <div className="space-y-2 mt-2">
                      <input
                        type="text"
                        value={editName}
                        onChange={(e) => setEditName(e.target.value)}
                        className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs outline-none focus:border-border"
                      />
                      <textarea
                        value={editContent}
                        onChange={(e) => setEditContent(e.target.value)}
                        rows={6}
                        className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs font-mono outline-none focus:border-border resize-y"
                      />
                      <div className="flex items-center gap-2">
                        <Button size="sm" onClick={handleSaveEdit} disabled={saving || !editName.trim()}>
                          {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : <Check className="size-3 mr-1" />}
                          保存
                        </Button>
                        <Button size="sm" variant="ghost" onClick={handleCancelEdit}>取消</Button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <pre className="mt-2 text-[11px] font-mono text-muted-foreground whitespace-pre-wrap max-h-40 overflow-y-auto">
                        {p.content}
                      </pre>
                      <div className="mt-2 flex items-center gap-1.5 flex-wrap">
                        {!isDefault && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={(e) => { e.stopPropagation(); handleSetDefault(p.id) }}
                            title="设为默认"
                          >
                            <StarOff className="size-3" />
                          </Button>
                        )}
                        {isDefault && (
                          <span className="px-2 py-1 text-[11px] text-primary inline-flex items-center gap-1">
                            <Star className="size-3" />
                            当前默认
                          </span>
                        )}
                        {!isBuiltin && (
                          <>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={(e) => { e.stopPropagation(); handleStartEdit(p) }}
                            >
                              <Pencil className="size-3" />
                            </Button>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={(e) => { e.stopPropagation(); handleDelete(p.id, p.name) }}
                              className="text-destructive hover:text-destructive"
                            >
                              <Trash2 className="size-3" />
                            </Button>
                          </>
                        )}
                        <div className="flex-1" />
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={(e) => { e.stopPropagation(); toggleVersions(p.id) }}
                          className="text-[10px] text-muted-foreground hover:text-foreground"
                        >
                          <History className="size-3 mr-1" />
                          版本历史
                          {showVersionsId === p.id ? <ChevronUp className="size-3 ml-0.5" /> : <ChevronDown className="size-3 ml-0.5" />}
                        </Button>
                      </div>

                      {/* Version history list */}
                      {showVersionsId === p.id && (
                        <div className="mt-2 pt-2 border-t border-border/30">
                          {loadingVersions ? (
                            <div className="flex items-center justify-center py-2">
                              <Loader2 className="size-3 animate-spin text-muted-foreground" />
                            </div>
                          ) : versions.length === 0 ? (
                            <p className="text-[11px] text-muted-foreground text-center py-2">暂无版本记录</p>
                          ) : (
                            <div className="space-y-1.5 max-h-48 overflow-y-auto">
                              {versions.map((v, idx) => (
                                <div key={v.id} className="rounded border border-border/30 bg-muted/20 p-2">
                                  <div className="flex items-center justify-between mb-1">
                                    <span className="text-[10px] font-medium text-foreground">
                                      {idx === 0 ? '当前版本' : `版本 ${versions.length - idx}`}
                                    </span>
                                    <span className="text-[9px] text-muted-foreground">
                                      {new Date(v.createdAt).toLocaleString('zh-CN', {
                                        month: '2-digit',
                                        day: '2-digit',
                                        hour: '2-digit',
                                        minute: '2-digit',
                                      })}
                                    </span>
                                  </div>
                                  <pre className="text-[10px] font-mono text-muted-foreground whitespace-pre-wrap max-h-20 overflow-y-auto">
                                    {v.content}
                                  </pre>
                                </div>
                              ))}
                            </div>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              )}
            </div>
          )
        })}
      </div>

      {prompts.length === 0 && (
        <p className="text-xs text-muted-foreground py-4 text-center">
          暂无提示词
        </p>
      )}
    </div>
  )
}
