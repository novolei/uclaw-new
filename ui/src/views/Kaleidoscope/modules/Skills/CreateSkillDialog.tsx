/**
 * CreateSkillDialog — 自定义技能创建对话框。
 *
 * 表单字段：名称(kebab-case)、描述(必填)、类别(可选)、激活关键词(tags)。
 * 调用后端 create_user_skill 命令（预留），成功后刷新列表 + toast 通知。
 */
import * as React from 'react'
import { X, Plus } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { createUserSkill } from '@/lib/tauri-bridge'

const KEBAB_CASE_REGEX = /^[a-z0-9][a-z0-9-]*$/

const CATEGORY_OPTIONS = [
  { value: 'productivity', label: '生产力' },
  { value: 'engineering', label: '工程' },
  { value: 'writing', label: '写作' },
  { value: 'debugging', label: '调试' },
  { value: 'other', label: '其他' },
]

export interface CreateSkillDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated?: () => void
}

export function CreateSkillDialog({
  open,
  onOpenChange,
  onCreated,
}: CreateSkillDialogProps): React.ReactElement {
  const [name, setName] = React.useState('')
  const [description, setDescription] = React.useState('')
  const [category, setCategory] = React.useState('')
  const [keywordInput, setKeywordInput] = React.useState('')
  const [keywords, setKeywords] = React.useState<string[]>([])
  const [creating, setCreating] = React.useState(false)

  const nameValid = name.length === 0 || KEBAB_CASE_REGEX.test(name)
  const canSubmit = name.length > 0 && nameValid && description.trim().length > 0

  const resetForm = () => {
    setName('')
    setDescription('')
    setCategory('')
    setKeywordInput('')
    setKeywords([])
  }

  const handleKeywordKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault()
      const value = keywordInput.trim().replace(/,$/, '')
      if (value && !keywords.includes(value)) {
        setKeywords((prev) => [...prev, value])
      }
      setKeywordInput('')
    }
  }

  const removeKeyword = (kw: string) => {
    setKeywords((prev) => prev.filter((k) => k !== kw))
  }

  const handleCreate = async () => {
    if (!canSubmit) return
    setCreating(true)
    try {
      await createUserSkill({
        name,
        description: description.trim(),
        category: category || undefined,
        keywords: keywords.length > 0 ? keywords : undefined,
      })
      toast.success(`技能「${name}」已创建`)
      resetForm()
      onOpenChange(false)
      onCreated?.()
    } catch (err) {
      toast.error('创建技能失败', { description: String(err) })
    } finally {
      setCreating(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="text-[16px]">创建自定义技能</DialogTitle>
          <DialogDescription className="text-[12px]">
            定义一个新的自定义技能，Agent 将在适当时机自动调用。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          {/* 名称 */}
          <div className="space-y-1.5">
            <label className="text-[11px] font-medium text-foreground">
              技能名称 <span className="text-destructive">*</span>
            </label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="my-custom-skill"
              className={cn(
                'h-8 text-[12px]',
                name.length > 0 && !nameValid && 'border-destructive focus-visible:ring-destructive',
              )}
            />
            {name.length > 0 && !nameValid && (
              <p className="text-[10px] text-destructive">
                名称必须为 kebab-case 格式（小写字母、数字和连字符）
              </p>
            )}
          </div>

          {/* 描述 */}
          <div className="space-y-1.5">
            <label className="text-[11px] font-medium text-foreground">
              描述 <span className="text-destructive">*</span>
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="描述此技能的触发条件和核心行为…"
              rows={3}
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-[12px] text-foreground placeholder:text-muted-foreground resize-y focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1 transition-colors"
            />
          </div>

          {/* 类别 */}
          <div className="space-y-1.5">
            <label className="text-[11px] font-medium text-foreground">
              类别
            </label>
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full h-8 rounded-md border border-border bg-background px-3 text-[12px] text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1 transition-colors"
            >
              <option value="">未分类</option>
              {CATEGORY_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          {/* 关键词 */}
          <div className="space-y-1.5">
            <label className="text-[11px] font-medium text-foreground">
              激活关键词
            </label>
            <div className="flex flex-wrap gap-1.5 min-h-[32px] rounded-md border border-border bg-background px-2 py-1.5 focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-1 transition-colors">
              {keywords.map((kw) => (
                <span
                  key={kw}
                  className="inline-flex items-center gap-0.5 rounded-full bg-accent/20 border border-accent/40 px-2 py-0.5 text-[10px] text-foreground"
                >
                  {kw}
                  <button
                    type="button"
                    onClick={() => removeKeyword(kw)}
                    className="ml-0.5 rounded-full hover:bg-destructive/20 p-0.5 transition-colors"
                  >
                    <X className="size-2.5" />
                  </button>
                </span>
              ))}
              <input
                type="text"
                value={keywordInput}
                onChange={(e) => setKeywordInput(e.target.value)}
                onKeyDown={handleKeywordKeyDown}
                placeholder={keywords.length === 0 ? '输入后回车添加…' : ''}
                className="flex-1 min-w-[80px] bg-transparent text-[11px] outline-none placeholder:text-muted-foreground"
              />
            </div>
            <p className="text-[10px] text-muted-foreground">
              按 Enter 或逗号添加关键词，用于匹配触发条件
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
            disabled={creating}
            className="h-8 text-[12px]"
          >
            取消
          </Button>
          <Button
            size="sm"
            onClick={() => void handleCreate()}
            disabled={!canSubmit || creating}
            className="h-8 text-[12px] gap-1"
          >
            <Plus className="size-3.5" />
            {creating ? '创建中…' : '创建技能'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
