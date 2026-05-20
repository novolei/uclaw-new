import React from 'react'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { ingestFiles, ingestUrl, type IngestionJob } from '@/lib/ingestion'

export function FeedPanel({ onClose: _onClose }: { onClose: () => void }): React.ReactElement {
  const [url, setUrl] = React.useState('')
  const [jobs, setJobs] = React.useState<Record<string, IngestionJob>>({})

  React.useEffect(() => {
    const un = listen<IngestionJob>('ingestion:progress', (e) => {
      const job = e.payload
      setJobs((prev) => ({ ...prev, [job.id]: job }))
      if (job.status === 'done' || job.status === 'partial') {
        toast.success(`从 ${job.source_label} 写入 ${job.pages_written.length} 页`)
      } else if (job.status === 'failed') {
        toast.error(`摄入失败: ${job.source_label}${job.error ? ` (${job.error})` : ''}`)
      }
    })
    return () => { un.then((f) => f()) }
  }, [])

  const pickFiles = async () => {
    const selected = await openFileDialog({
      multiple: true,
      title: '选择要摄入的文件',
      filters: [{ name: '文档/音视频', extensions: ['md', 'markdown', 'txt', 'pdf', 'mp3', 'wav', 'm4a', 'flac', 'ogg', 'mp4', 'mov', 'webm'] }],
    })
    if (selected == null) return
    const paths = Array.isArray(selected) ? selected : [selected]
    if (paths.length === 0) return
    await ingestFiles(paths)
    toast.message(`已开始摄入 ${paths.length} 个文件`)
  }

  const submitUrl = async () => {
    const u = url.trim()
    if (!u) return
    await ingestUrl(u)
    toast.message(`已开始摄入 ${u}`)
    setUrl('')
  }

  const active = Object.values(jobs)

  return (
    <div className="flex flex-col gap-3">
      <button
        type="button"
        onClick={() => void pickFiles()}
        className="rounded-lg border-2 border-dashed border-border p-6 text-center text-sm text-muted-foreground hover:border-accent hover:bg-accent/10"
      >
        点击选择 PDF / md / 音视频文件
      </button>
      <div className="flex gap-2">
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void submitUrl() }}
          placeholder="或粘贴一个 URL"
          className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-sm"
        />
        <button type="button" onClick={() => void submitUrl()} className="rounded-md bg-accent px-3 py-1 text-sm text-accent-foreground">
          摄入
        </button>
      </div>
      {active.length > 0 && (
        <ul className="max-h-40 overflow-auto text-xs text-muted-foreground">
          {active.map((j) => (
            <li key={j.id} className="flex justify-between py-0.5">
              <span className="truncate">{j.source_label}</span>
              <span>{j.status === 'extracting' || j.status === 'writing'
                ? `${j.progress.stage} ${j.progress.done}/${j.progress.total}`
                : j.status}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
