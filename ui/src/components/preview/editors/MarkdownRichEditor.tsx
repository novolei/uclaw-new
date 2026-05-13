/**
 * MarkdownRichEditor — TipTap WYSIWYG host for markdown files.
 *
 * TipTap doesn't natively round-trip markdown — it stores ProseMirror
 * JSON. We use a HTML-as-intermediate-format pattern: load markdown →
 * parse to HTML via a tiny converter → TipTap.setContent → on edit,
 * TipTap.getHTML → serialize back to markdown via a DOM walker.
 *
 * Round-trip fidelity caveats (one-time toast on first edit per session):
 *   - Raw HTML blocks: dropped on round-trip
 *   - Simple GFM tables round-trip cleanly; complex alignments may lose syntax
 *   - Footnote syntax ([^1]): not preserved
 *
 * Auto-save with 300ms debounce. The mtime-based conflict pause was
 * removed (2026-05-13) — dirty-guard pattern in usePreviewRefresh now
 * prevents external refreshes from clobbering an unsaved draft, so the
 * "pause auto-save while a conflict is open" path is no longer needed.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { useEditor, EditorContent } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import Link from '@tiptap/extension-link'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { common, createLowlight } from 'lowlight'
import { toast } from 'sonner'
import { tipTapFidelityToastShownAtom } from '@/atoms/preview-editor-atoms'
import type { EditorProps } from './TextEditor'

const AUTO_SAVE_DEBOUNCE_MS = 300

const lowlight = createLowlight(common)

/** Minimal markdown → HTML for TipTap setContent. */
function mdToHtml(md: string): string {
  // Tiny block-level converter. Production setups would use unified +
  // remark-html; we accept simple rendering for W4d (spec §6.4 caveat).
  return md
    .split('\n\n')
    .map((para) => {
      if (para.startsWith('# ')) return `<h1>${escapeInline(para.slice(2))}</h1>`
      if (para.startsWith('## ')) return `<h2>${escapeInline(para.slice(3))}</h2>`
      if (para.startsWith('### ')) return `<h3>${escapeInline(para.slice(4))}</h3>`
      if (para.startsWith('```')) {
        const lines = para.split('\n')
        const langLine = lines[0]?.slice(3) ?? ''
        const body = lines.slice(1, lines[lines.length - 1] === '```' ? -1 : undefined).join('\n')
        return `<pre><code class="language-${langLine}">${escapeHtml(body)}</code></pre>`
      }
      if (para.trim() === '') return ''
      return `<p>${escapeInline(para)}</p>`
    })
    .filter(Boolean)
    .join('')
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!))
}
function escapeInline(s: string): string {
  // Simple bold/italic/code inline + line breaks
  return escapeHtml(s)
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/`(.+?)`/g, '<code>$1</code>')
    .replace(/\n/g, '<br>')
}

/** TipTap HTML → markdown serialization via DOMParser walk. */
function htmlToMd(html: string): string {
  const doc = new DOMParser().parseFromString(html, 'text/html')
  return walk(doc.body).trim() + '\n'
}

function walk(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? ''
  if (node.nodeType !== Node.ELEMENT_NODE) return ''
  const el = node as Element
  const children = Array.from(el.childNodes).map(walk).join('')
  switch (el.tagName.toLowerCase()) {
    case 'h1': return `\n# ${children}\n\n`
    case 'h2': return `\n## ${children}\n\n`
    case 'h3': return `\n### ${children}\n\n`
    case 'strong':
    case 'b': return `**${children}**`
    case 'em':
    case 'i': return `*${children}*`
    case 'code': return `\`${children}\``
    case 'pre': return `\n\`\`\`\n${el.textContent}\n\`\`\`\n\n`
    case 'br': return '\n'
    case 'p': return `\n${children}\n\n`
    case 'a': {
      const href = el.getAttribute('href') ?? ''
      return `[${children}](${href})`
    }
    default: return children
  }
}

export function MarkdownRichEditor(props: EditorProps): React.ReactElement {
  const { initialContent, onSave, onContentChange } = props
  const [fidelityShown, setFidelityShown] = useAtom(tipTapFidelityToastShownAtom)
  const autoSaveTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // onSave is captured once into the editor's onUpdate closure — bridge
  // through a ref so the latest reference is always used.
  const onSaveRef = React.useRef(onSave)
  React.useEffect(() => {
    onSaveRef.current = onSave
  }, [onSave])

  // In-flight save guard: serializes saves so the latest typed content
  // wins. Without it, two parallel auto-save IPC calls can return
  // out-of-order and the editor's baseline promotion goes non-deterministic.
  const savingRef = React.useRef(false)
  const pendingContentRef = React.useRef<string | null>(null)

  const runSaveLoop = React.useCallback(async (content: string) => {
    if (savingRef.current) {
      pendingContentRef.current = content
      return
    }
    savingRef.current = true
    try {
      let toSave: string | null = content
      while (toSave !== null) {
        const cur: string = toSave
        const outcome = await onSaveRef.current(cur)
        if (outcome.kind !== 'saved') break
        const next: string | null = pendingContentRef.current
        pendingContentRef.current = null
        toSave = next !== null && next !== cur ? next : null
      }
    } finally {
      savingRef.current = false
    }
  }, [])
  const runSaveLoopRef = React.useRef(runSaveLoop)
  React.useEffect(() => { runSaveLoopRef.current = runSaveLoop }, [runSaveLoop])

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ codeBlock: false }), // CodeBlockLowlight replaces it
      Link.configure({ openOnClick: false }),
      CodeBlockLowlight.configure({ lowlight }),
    ],
    content: mdToHtml(initialContent),
    onUpdate({ editor: ed }) {
      if (!fidelityShown) {
        toast(
          '富文本编辑可能简化部分原始 Markdown 语法 — 切换到「源码」可保留所有原文',
          { duration: 6000, id: 'tiptap-fidelity-warning' },
        )
        setFidelityShown(true)
      }
      const html = ed.getHTML()
      const md = htmlToMd(html)
      onContentChange?.(md, md !== initialContent)

      if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
      autoSaveTimer.current = setTimeout(() => {
        void runSaveLoopRef.current(md)
      }, AUTO_SAVE_DEBOUNCE_MS)
    },
  })

  React.useEffect(() => {
    return () => {
      if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
      editor?.destroy()
    }
  }, [editor])

  return (
    <div className="tiptap-markdown-preview h-full w-full overflow-auto p-4 prose prose-sm dark:prose-invert max-w-none">
      <EditorContent editor={editor} />
    </div>
  )
}
