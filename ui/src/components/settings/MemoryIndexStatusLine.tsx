/**
 * MemoryIndexStatusLine — 设置 → 记忆：关键词索引（jieba 分词 + BM25）的状态。
 * 索引在后台每 60 秒同步一次；第一次同步完成前显示「正在建立」。
 */
import * as React from 'react'
import { getMemoryIndexStatus, type MemoryIndexStatus } from '@/lib/tauri-bridge'

function ago(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (s < 60) return `${s} 秒前`
  if (s < 3600) return `${Math.round(s / 60)} 分钟前`
  return new Date(ms).toLocaleString()
}

export function MemoryIndexStatusLine(): React.ReactElement | null {
  const [status, setStatus] = React.useState<MemoryIndexStatus | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    getMemoryIndexStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)))
  }, [])

  if (error) return <p className="text-xs text-muted-foreground">关键词索引：读取失败（{error}）</p>
  if (!status) return null
  if (!status.lastSync && status.docs === 0) {
    return <p className="text-xs text-muted-foreground">关键词索引：正在建立（后台每 60 秒同步一次）</p>
  }
  return (
    <p className="text-xs text-muted-foreground">
      关键词索引：{status.docs.toLocaleString('en-US')} 条记忆
      {status.lastSync && ` · 上次同步 ${ago(status.lastSync.atMs)}`}
      <span className="block text-[10px] mt-0.5">
        用中文分词索引你的记忆事实和记忆片段，每轮对话会先按关键词找出最相关的几条。
      </span>
    </p>
  )
}
