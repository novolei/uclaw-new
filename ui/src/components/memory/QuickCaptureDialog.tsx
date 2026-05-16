/**
 * QuickCaptureDialog — 文本快速记忆碎片录入对话框。
 *
 * 与 MemoryVoiceCapture（语音记忆浮层）互补——语音走独立浮层，文本走此对话框。
 * 支持键盘输入/粘贴、快速标签单选、剪贴板一键粘贴，Cmd+Enter 保存。
 * 由 quickCaptureOpenAtom 控制显示/隐藏。
 */
import * as React from 'react'
import { useAtom } from 'jotai'
import { Mic, ClipboardPaste, Loader2, Zap } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { quickCaptureOpenAtom } from '@/atoms/quick-capture-atoms'
import { memoryGraphQuickCapture } from '@/lib/tauri-bridge'

// ─── 标签定义 ────────────────────────────────────────────────────────────────
const FRAGMENT_SUBTYPES = [
  { id: 'daily', label: '日常', icon: '☀️' },
  { id: 'credential', label: '凭证', icon: '🔑' },
  { id: 'location', label: '位置', icon: '📍' },
  { id: 'reminder', label: '提醒', icon: '⏰' },
  { id: 'insight', label: '灵感', icon: '💡' },
  { id: 'bookmark', label: '书签', icon: '🔖' },
] as const

type FragmentSubtype = (typeof FRAGMENT_SUBTYPES)[number]['id']

export function QuickCaptureDialog(): React.ReactElement | null {
  const [open, setOpen] = useAtom(quickCaptureOpenAtom)
  const [content, setContent] = React.useState('')
  const [selectedTag, setSelectedTag] = React.useState<FragmentSubtype | null>(null)
  const [clipboardText, setClipboardText] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)
  const textareaRef = React.useRef<HTMLTextAreaElement>(null)

  // ── 打开时：自动聚焦 + 检测剪贴板 ──────────────────────────────────────────
  React.useEffect(() => {
    if (!open) return

    // 重置状态
    setContent('')
    setSelectedTag(null)
    setClipboardText(null)
    setSaving(false)

    // 自动聚焦 textarea
    requestAnimationFrame(() => {
      textareaRef.current?.focus()
    })

    // 检测剪贴板
    navigator.clipboard
      .readText()
      .then((text) => {
        if (text && text.trim().length > 0) {
          setClipboardText(text.trim())
        }
      })
      .catch(() => {
        // 剪贴板访问可能被拒绝，静默处理
      })
  }, [open])

  // ── Cmd+Enter / Ctrl+Enter 保存快捷键 ──────────────────────────────────────
  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        void handleSave()
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [content, selectedTag],
  )

  // ── 保存逻辑 ──────────────────────────────────────────────────────────────
  const handleSave = async () => {
    const trimmed = content.trim()
    if (!trimmed || saving) return

    setSaving(true)
    try {
      await memoryGraphQuickCapture({
        content: trimmed,
        source: 'manual',
        tags: selectedTag ? [selectedTag] : undefined,
      })
      toast.success('已记住 ✓')
      setOpen(false)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`保存失败: ${msg}`)
    } finally {
      setSaving(false)
    }
  }

  // ── 粘贴剪贴板内容 ────────────────────────────────────────────────────────
  const handlePasteClipboard = () => {
    if (!clipboardText) return
    setContent((prev) => (prev ? `${prev}\n${clipboardText}` : clipboardText))
    setClipboardText(null)
    textareaRef.current?.focus()
  }

  // ── 麦克风按钮：触发语音记忆浮层 ──────────────────────────────────────────
  const handleVoiceSwitch = () => {
    setOpen(false)
    // 延迟一帧确保对话框关闭后再触发语音浮层
    requestAnimationFrame(() => {
      window.dispatchEvent(new CustomEvent('uclaw:memory-voice-start'))
    })
  }

  // ── 标签切换 ──────────────────────────────────────────────────────────────
  const toggleTag = (id: FragmentSubtype) => {
    setSelectedTag((prev) => (prev === id ? null : id))
  }

  const isMac = /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)
  const saveShortcut = isMac ? '⌘↵ 保存' : 'Ctrl+↵ 保存'

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="max-w-[480px] gap-0 p-0 overflow-hidden"
        hideClose
      >
        {/* ── 标题栏 ─────────────────────────────────────────────── */}
        <DialogHeader className="px-5 pt-4 pb-3">
          <DialogTitle className="flex items-center gap-2 text-base">
            <Zap className="h-4 w-4 text-pink-500" />
            <span>记忆碎片</span>
          </DialogTitle>
          <DialogDescription className="sr-only">
            输入或粘贴你想记住的内容
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-5 space-y-4">
          {/* ── Textarea 区域 ──────────────────────────────────── */}
          <div className="relative">
            <textarea
              ref={textareaRef}
              value={content}
              onChange={(e) => setContent(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入或粘贴你想记住的内容…"
              rows={4}
              className={cn(
                'w-full resize-none rounded-lg border border-border/60 bg-muted/30 px-3.5 py-3 text-sm',
                'placeholder:text-muted-foreground/60',
                'focus:outline-none focus:ring-2 focus:ring-pink-500/40 focus:border-pink-500/50',
                'transition-colors',
              )}
            />
            {/* 右下角麦克风按钮 */}
            <button
              type="button"
              onClick={handleVoiceSwitch}
              title="切换到语音记忆"
              className={cn(
                'absolute right-2 bottom-2 p-1.5 rounded-md',
                'text-muted-foreground/60 hover:text-pink-500 hover:bg-pink-500/10',
                'transition-colors',
              )}
            >
              <Mic className="h-4 w-4" />
            </button>
          </div>

          {/* ── 快速标签 ──────────────────────────────────────── */}
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground/70 font-medium">快速标签</p>
            <div className="flex flex-wrap gap-1.5">
              {FRAGMENT_SUBTYPES.map((tag) => (
                <button
                  key={tag.id}
                  type="button"
                  onClick={() => toggleTag(tag.id)}
                  className={cn(
                    'inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-xs font-medium transition-all',
                    'border',
                    selectedTag === tag.id
                      ? 'border-pink-500/50 bg-pink-500/15 text-pink-600 dark:text-pink-400'
                      : 'border-border/50 bg-muted/40 text-muted-foreground hover:border-border hover:bg-muted/70',
                  )}
                >
                  <span>{tag.icon}</span>
                  <span>{tag.label}</span>
                </button>
              ))}
            </div>
          </div>

          {/* ── 粘贴剪贴板按钮 ────────────────────────────────── */}
          {clipboardText && (
            <button
              type="button"
              onClick={handlePasteClipboard}
              className={cn(
                'flex items-center gap-2 w-full rounded-lg px-3 py-2 text-xs text-left',
                'border border-dashed border-border/60 bg-muted/20',
                'text-muted-foreground hover:bg-muted/40 hover:border-border',
                'transition-colors',
              )}
            >
              <ClipboardPaste className="h-3.5 w-3.5 flex-shrink-0 text-pink-500/70" />
              <span className="truncate">
                粘贴剪贴板内容：{clipboardText.length > 60 ? `${clipboardText.slice(0, 60)}…` : clipboardText}
              </span>
            </button>
          )}

          {/* ── 保存按钮 ──────────────────────────────────────── */}
          <div className="flex justify-end">
            <Button
              onClick={() => void handleSave()}
              disabled={!content.trim() || saving}
              size="sm"
              className={cn(
                'bg-pink-500 text-white hover:bg-pink-600',
                'disabled:bg-pink-500/40 disabled:text-white/60',
              )}
            >
              {saving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <span className="text-xs">{saveShortcut}</span>
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
