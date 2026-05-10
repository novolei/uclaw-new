/**
 * PromptsSettings — Settings → 提示词 tab.
 *
 * Three sections:
 *   1. Global system prompt (link to existing 通用 tab — don't duplicate)
 *   2. uclaw.md (workspace-level, editable textarea + 保存 + 外部编辑器)
 *   3. uClaw 内置行为护栏 (read-only collapsible: Karpathy baseline +
 *      current mode addition for transparency)
 */

import * as React from 'react'
import { Save, ExternalLink, FileCode2, ChevronDown, ChevronRight } from 'lucide-react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  readWorkspaceUclawMd,
  writeWorkspaceUclawMd,
  readDefaultPrompts,
  openWorkspaceUclawMdExternally,
} from '@/lib/tauri-bridge'
import type { DefaultPromptsResponse } from '@/lib/types'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { settingsTabAtom } from '@/atoms/settings-tab'

const PLACEHOLDER_TEMPLATE = `# uClaw — <project name>

<!-- 这个文件描述当前项目的上下文。uClaw agent 在每次对话时都会
     读取它，作为 "项目说明" 注入到系统提示词。
     文件位置：<workspace>/uclaw.md
     编辑后保存即生效。 -->

## 项目约定

-

## Do

-

## Don't

-

## 常用命令 / 路径

-
`

export function PromptsSettings(): React.ReactElement {
  const [content, setContent] = React.useState('')
  const [pristine, setPristine] = React.useState('')
  const [defaults, setDefaults] = React.useState<DefaultPromptsResponse | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)
  const [showGuardrails, setShowGuardrails] = React.useState(false)
  const mode = useAtomValue(safetyModeAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  React.useEffect(() => {
    Promise.all([readWorkspaceUclawMd(), readDefaultPrompts()])
      .then(([md, p]) => {
        setContent(md)
        setPristine(md)
        setDefaults(p)
      })
      .catch((e) => {
        console.error('[PromptsSettings] load failed:', e)
        toast.error('加载提示词失败')
      })
      .finally(() => setLoading(false))
  }, [])

  const dirty = content !== pristine

  const onSave = async () => {
    setSaving(true)
    try {
      await writeWorkspaceUclawMd(content)
      setPristine(content)
      toast.success('uclaw.md 已保存')
    } catch (e) {
      console.error('[PromptsSettings] save failed:', e)
      toast.error('保存失败')
    } finally {
      setSaving(false)
    }
  }

  const currentModeAddition = React.useMemo(() => {
    if (!defaults) return ''
    switch (mode) {
      case 'ask': return defaults.modeAsk
      case 'acceptedits': return defaults.modeAcceptEdits
      case 'plan': return defaults.modePlan
      case 'yolo': return defaults.modeBypass
      default: return '(Auto mode — no mode-specific addition)'
    }
  }, [mode, defaults])

  return (
    <div className="space-y-6 pb-8">
      {/* Section 1: link to existing global system prompt tab */}
      <section>
        <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          全局系统提示词
        </h3>
        <Button variant="outline" size="sm" onClick={() => setSettingsTab('general')}>
          跳到 通用 tab 编辑
        </Button>
      </section>

      {/* Section 2: uclaw.md textarea */}
      <section>
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            项目说明 (uclaw.md)
          </h3>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost" size="sm"
              onClick={async () => {
                try {
                  await openWorkspaceUclawMdExternally()
                } catch (e) {
                  console.error('[PromptsSettings] open external failed:', e)
                  toast.error('打开外部编辑器失败')
                }
              }}
            >
              <ExternalLink className="size-3.5 mr-1" />
              在外部编辑器打开
            </Button>
            <Button
              size="sm"
              onClick={() => void onSave()}
              disabled={!dirty || saving}
            >
              <Save className="size-3.5 mr-1" />
              {saving ? '保存中…' : '保存'}
            </Button>
          </div>
        </div>
        <textarea
          value={loading ? '加载中…' : content}
          placeholder={loading ? '' : PLACEHOLDER_TEMPLATE}
          onChange={(e) => setContent(e.target.value)}
          disabled={loading}
          spellCheck={false}
          className={cn(
            'w-full min-h-[280px] font-mono text-[12.5px] p-3',
            'bg-background border border-border/50 rounded',
            'focus:outline-none focus:border-border',
          )}
        />
        <p className="mt-1 text-[11px] text-muted-foreground/60">
          路径：<code className="font-mono">&lt;workspace&gt;/uclaw.md</code>
          {dirty && <span className="ml-2 text-amber-600">• 未保存</span>}
        </p>
      </section>

      {/* Section 3: read-only guardrails preview */}
      <section>
        <button
          type="button"
          onClick={() => setShowGuardrails((v) => !v)}
          className="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70 hover:text-foreground"
        >
          {showGuardrails ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          uClaw 内置行为护栏 (只读)
        </button>
        {showGuardrails && defaults && (
          <div className="mt-2 space-y-3">
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> baseline.md (Karpathy guardrails)
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {defaults.baseline}
              </pre>
            </div>
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> 当前模式 ({mode}) 的特化提示词
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {currentModeAddition || '(empty)'}
              </pre>
            </div>
          </div>
        )}
      </section>
    </div>
  )
}
