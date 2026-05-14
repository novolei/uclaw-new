/**
 * McpTemplateLibrary — 「+ 添加集成」弹出的模板库。
 *
 * 4 个模板(GitHub / Notion / Slack / Custom)。选中 → 关闭本弹层、把预填
 * McpServerInput 交给父级打开 McpEditorModal。模板定义是纯前端常量。
 */
import * as React from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { McpServerInput } from '@/lib/types'

export interface McpTemplate {
  key: string
  label: string
  description: string
  prefill: McpServerInput
}

export const MCP_TEMPLATES: McpTemplate[] = [
  {
    key: 'github',
    label: 'GitHub',
    description: '仓库 / PR / issue 操作',
    prefill: {
      name: 'github',
      description: 'GitHub 仓库 / PR / issue 操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-github'],
      env: { GITHUB_TOKEN: '' },
    },
  },
  {
    key: 'notion',
    label: 'Notion',
    description: 'Notion 页面 / 数据库',
    prefill: {
      name: 'notion',
      description: 'Notion 页面 / 数据库操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-notion'],
      env: { NOTION_API_KEY: '' },
    },
  },
  {
    key: 'slack',
    label: 'Slack',
    description: '消息 / 频道',
    prefill: {
      name: 'slack',
      description: 'Slack 消息 / 频道操作',
      transportType: 'stdio',
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-slack'],
      env: { SLACK_BOT_TOKEN: '' },
    },
  },
  {
    key: 'custom',
    label: 'Custom',
    description: '从空白表单开始',
    prefill: {
      name: '',
      description: '',
      transportType: 'stdio',
      command: '',
      args: [],
      env: {},
    },
  },
]

export interface McpTemplateLibraryProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onPick: (prefill: McpServerInput) => void
}

export function McpTemplateLibrary({ open, onOpenChange, onPick }: McpTemplateLibraryProps): React.ReactElement {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-popover">
        <DialogHeader>
          <DialogTitle>从模板新建</DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-2">
          {MCP_TEMPLATES.map((tpl) => (
            <button
              key={tpl.key}
              type="button"
              onClick={() => onPick(tpl.prefill)}
              className="rounded-lg border border-border bg-card p-3 text-left transition-colors hover:bg-muted/40"
            >
              <div className="text-[13px] font-semibold text-foreground">{tpl.label}</div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">{tpl.description}</div>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
