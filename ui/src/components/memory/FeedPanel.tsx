import React from 'react'
import { getPathForFile } from '@/lib/tauri-bridge'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { ingestFiles, ingestUrl, type IngestionJob } from '@/lib/ingestion'

export function FeedPanel({ onClose: _onClose }: { onClose: () => void }): React.ReactElement {
  const [url, setUrl] = React.useState('')
  const [dragOver, setDragOver] = React.useState(false)
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

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    const files = Array.from(e.dataTransfer.files)
    const paths: string[] = []
    for (const f of files) {
      try { const p = getPathForFile(f); if (p) paths.push(p) } catch { /* skip */ }
    }
    if (paths.length === 0) { toast.error('无法获取文件路径'); return }
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
      <div
        onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
        className={`rounded-lg border-2 border-dashed p-6 text-center text-sm ${
          dragOver ? 'border-accent bg-accent/10' : 'border-border text-muted-foreground'
        }`}
      >
        拖放 PDF / md / 音视频文件到这里
      </div>
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
