/**
 * McpEditorModal — MCP server 可视化编辑器(居中模态框,变体 A)。
 *
 * 表单字段对齐后端 McpServerConfig:名称 / 描述 / 传输方式(stdio↔http)/
 * stdio 字段(命令 / 参数 chips / 环境变量 key-value)或 http 字段(URL)/
 * 自动批准开关。「测试连接并保存」= 先 add/update 落库,再 connect 读状态。
 */
import * as React from 'react'
import { toast } from 'sonner'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { addMcpServer, updateMcpServer, connectMcpServer } from '@/lib/tauri-bridge'
import type { McpServerInfo, McpServerInput, McpTransportType } from '@/lib/types'

let envRowId = 0

/** 编辑器模式:新建(带预填)或编辑现有 server。 */
export type McpEditorTarget =
  | { mode: 'add'; prefill: McpServerInput }
  | { mode: 'edit'; server: McpServerInfo }

export interface McpEditorModalProps {
  target: McpEditorTarget | null
  onOpenChange: (open: boolean) => void
  /** 保存 + 连接成功后回调,父级用来刷新列表。 */
  onSaved: () => void
}

interface FormState {
  name: string
  description: string
  transportType: McpTransportType
  command: string
  args: string[]
  env: Array<{ id: number; key: string; value: string }>
  url: string
  autoApprove: boolean
}

function emptyForm(): FormState {
  return { name: '', description: '', transportType: 'stdio', command: '', args: [], env: [], url: '', autoApprove: false }
}

function fromInput(input: McpServerInput): FormState {
  return {
    name: input.name,
    description: input.description,
    transportType: input.transportType ?? 'stdio',
    command: input.command,
    args: input.args ?? [],
    env: Object.entries(input.env ?? {}).map(([key, value]) => ({ id: envRowId++, key, value })),
    url: input.url ?? '',
    autoApprove: input.autoApprove ?? false,
  }
}

function fromServer(server: McpServerInfo): FormState {
  return {
    name: server.name,
    description: server.description,
    transportType: server.transportType,
    command: server.command,
    args: server.args,
    env: Object.entries(server.env ?? {}).map(([key, value]) => ({ id: envRowId++, key, value })),
    url: server.url ?? '',
    autoApprove: server.autoApprove,
  }
}

function toInput(form: FormState): McpServerInput {
  return {
    name: form.name.trim(),
    description: form.description.trim(),
    transportType: form.transportType,
    command: form.command.trim(),
    args: form.args,
    env: Object.fromEntries(form.env.filter((e) => e.key.trim()).map((e) => [e.key.trim(), e.value])),
    url: form.transportType === 'http' ? form.url.trim() : null,
    autoApprove: form.autoApprove,
  }
}

export function McpEditorModal({ target, onOpenChange, onSaved }: McpEditorModalProps): React.ReactElement {
  const [form, setForm] = React.useState<FormState>(emptyForm)
  const [submitting, setSubmitting] = React.useState(false)
  const [connectError, setConnectError] = React.useState<string | null>(null)
  const [newArg, setNewArg] = React.useState('')
  // 已落库的 server —— connect 失败后留在表单里重试时,用它走 update 而不是再 add 一次。
  const savedRef = React.useRef<McpServerInfo | null>(null)

  // 打开/切换 target 时重置表单。
  React.useEffect(() => {
    if (!target) return
    setForm(target.mode === 'add' ? fromInput(target.prefill) : fromServer(target.server))
    setConnectError(null)
    setNewArg('')
    savedRef.current = null
  }, [target])

  const valid =
    form.name.trim().length > 0 &&
    (form.transportType === 'stdio' ? form.command.trim().length > 0 : form.url.trim().length > 0)

  const handleSave = async () => {
    if (!target || !valid) return
    setSubmitting(true)
    setConnectError(null)
    const input = toInput(form)
    try {
      // 已落库过(上次 connect 失败)→ 走 update,既避免重复 add 又能带上重试前的编辑。
      const saved = savedRef.current
        ? await updateMcpServer(savedRef.current.id, input)
        : target.mode === 'add'
          ? await addMcpServer(input)
          : await updateMcpServer(target.server.id, input)
      savedRef.current = saved
      try {
        await connectMcpServer(saved.id)
        savedRef.current = null
        toast.success(`「${saved.name}」已连接`)
        onSaved()
        onOpenChange(false)
      } catch (connErr) {
        // 已落库但连不上 —— modal 不关,内联显示错误。savedRef 留着,重试走 update。
        setConnectError(String(connErr))
        onSaved()
      }
    } catch (saveErr) {
      toast.error('保存失败', { description: String(saveErr) })
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onOpenChange(false)}>
      <DialogContent className="bg-popover max-w-[440px]">
        <DialogHeader>
          <DialogTitle>{target?.mode === 'edit' ? `编辑集成 · ${target.server.name}` : '新建集成'}</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <Field label="名称">
            <Input
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="github"
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="描述">
            <Input
              value={form.description}
              onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
              placeholder="服务器用途说明"
              className="h-8 text-[12px]"
            />
          </Field>

          <Field label="传输方式">
            <div className="flex gap-1.5">
              {(['stdio', 'http'] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => setForm((f) => ({ ...f, transportType: t }))}
                  className={cn(
                    'flex-1 rounded-md border px-2 py-1 text-[11.5px] transition-colors',
                    form.transportType === t
                      ? 'border-accent/35 bg-accent/15 text-accent-foreground'
                      : 'border-border text-muted-foreground hover:bg-muted/40',
                  )}
                >
                  {t === 'stdio' ? 'stdio（子进程）' : 'http'}
                </button>
              ))}
            </div>
          </Field>

          {form.transportType === 'stdio' ? (
            <>
              <Field label="命令">
                <Input
                  value={form.command}
                  onChange={(e) => setForm((f) => ({ ...f, command: e.target.value }))}
                  placeholder="npx"
                  className="h-8 font-mono text-[11.5px]"
                />
              </Field>
              <Field label="参数">
                <div className="flex flex-wrap gap-1.5">
                  {form.args.map((arg, i) => (
                    <span
                      key={`${arg}-${i}`}
                      className="flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]"
                    >
                      {arg}
                      <button
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, args: f.args.filter((_, j) => j !== i) }))}
                        className="text-muted-foreground hover:text-destructive"
                        aria-label={`移除参数 ${arg}`}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <Input
                    value={newArg}
                    onChange={(e) => setNewArg(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && newArg.trim()) {
                        e.preventDefault()
                        setForm((f) => ({ ...f, args: [...f.args, newArg.trim()] }))
                        setNewArg('')
                      }
                    }}
                    placeholder="+ 参数后回车"
                    className="h-6 w-32 font-mono text-[10px]"
                  />
                </div>
              </Field>
              <Field label="环境变量">
                <div className="space-y-1">
                  {form.env.map((row, i) => (
                    <div key={row.id} className="flex gap-1.5">
                      <Input
                        value={row.key}
                        onChange={(e) =>
                          setForm((f) => ({
                            ...f,
                            env: f.env.map((r, j) => (j === i ? { ...r, key: e.target.value } : r)),
                          }))
                        }
                        placeholder="KEY"
                        className="h-7 flex-[0_0_130px] font-mono text-[10px]"
                      />
                      <Input
                        value={row.value}
                        onChange={(e) =>
                          setForm((f) => ({
                            ...f,
                            env: f.env.map((r, j) => (j === i ? { ...r, value: e.target.value } : r)),
                          }))
                        }
                        placeholder="value"
                        className="h-7 flex-1 font-mono text-[10px]"
                      />
                      <button
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, env: f.env.filter((_, j) => j !== i) }))}
                        className="text-[11px] text-muted-foreground hover:text-destructive"
                        aria-label="移除环境变量"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() => setForm((f) => ({ ...f, env: [...f.env, { id: envRowId++, key: '', value: '' }] }))}
                    className="w-full rounded border border-dashed border-border py-1 text-[10px] text-muted-foreground hover:bg-muted/40"
                  >
                    + 添加环境变量
                  </button>
                </div>
              </Field>
            </>
          ) : (
            <Field label="URL">
              <Input
                value={form.url}
                onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
                placeholder="https://example.com/mcp"
                className="h-8 font-mono text-[11.5px]"
              />
            </Field>
          )}

          <div className="flex items-center justify-between rounded-md bg-muted/40 px-3 py-2">
            <div>
              <div className="text-[11.5px] font-medium text-foreground">自动批准工具调用</div>
              <div className="text-[10px] text-muted-foreground">Agent 调用此 server 的工具时不再逐次确认</div>
            </div>
            <Switch
              checked={form.autoApprove}
              onCheckedChange={(next) => setForm((f) => ({ ...f, autoApprove: next }))}
              aria-label="自动批准工具调用"
            />
          </div>

          {connectError && (
            <pre className="whitespace-pre-wrap break-words rounded-md bg-destructive/10 p-2 text-[11px] text-destructive">
              已保存，但连接失败：{connectError}
            </pre>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={submitting}>
            取消
          </Button>
          <Button onClick={() => void handleSave()} disabled={!valid || submitting}>
            {submitting ? '保存中…' : '测试连接并保存'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div>
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      {children}
    </div>
  )
}
