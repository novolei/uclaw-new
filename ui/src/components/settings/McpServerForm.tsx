import { useState } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsInput } from './primitives/SettingsInput'
import { Button } from '@/components/ui/button'
import { addMcpServer } from '@/lib/tauri-bridge'

interface McpServerFormProps {
  onClose: () => void
  onAdded: () => void
}

export function McpServerForm({ onClose, onAdded }: McpServerFormProps) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [command, setCommand] = useState('')
  const [args, setArgs] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    try {
      await addMcpServer({
        name,
        description,
        command,
        args: args ? args.split(' ') : [],
      })
      onAdded()
    } catch (err) {
      console.error('Failed to add MCP server:', err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-xl p-6 w-[480px] max-w-[90vw] space-y-4">
        <h3 className="text-base font-semibold">添加 MCP 服务器</h3>

        <form onSubmit={handleSubmit} className="space-y-4">
          <SettingsSection>
            <SettingsInput
              label="名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My MCP Server"
              required
            />
            <SettingsInput
              label="描述"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="服务器用途说明"
            />
            <SettingsInput
              label="启动命令"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
              required
            />
            <SettingsInput
              label="参数（空格分隔）"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="-y @modelcontextprotocol/server-xxx"
            />
          </SettingsSection>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={submitting}>
              {submitting ? '添加中...' : '添加'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}
