import React from 'react'
import { cn, safeParseDate } from '@/lib/utils'
import { listDailySummaries } from '@/lib/tauri-bridge'
import type { DailySummaryItem } from '@/lib/tauri-bridge'
import { Calendar, FileText } from 'lucide-react'

export function DailySummaryView() {
  const [summaries, setSummaries] = React.useState<DailySummaryItem[]>([])
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    async function load() {
      try {
        const result = await listDailySummaries(30)
        setSummaries(result)
      } catch (e) {
        console.error('Failed to load daily summaries:', e)
      } finally {
        setLoading(false)
      }
    }
    load()
  }, [])

  if (loading) {
    return <div className="p-4 text-center text-[12px] text-muted-foreground">加载中...</div>
  }

  if (summaries.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 p-6">
        <Calendar className="size-8 text-muted-foreground/40" />
        <p className="text-[14px] text-muted-foreground">暂无日记摘要</p>
        <p className="text-[12px] text-muted-foreground/60 text-center">
          每天早晨将自动生成昨日记忆摘要
        </p>
      </div>
    )
  }

  return (
    <div className="p-3 overflow-y-auto h-full">
      {/* 时间线布局 */}
      <div className="relative pl-6">
        {/* 时间线轴 */}
        <div className="absolute left-[9px] top-2 bottom-2 w-px bg-border" />
        
        <div className="space-y-4">
          {summaries.map((summary) => (
            <div key={summary.id} className="relative">
              {/* 时间线圆点 */}
              <div className="absolute left-[-18px] top-3 size-2.5 rounded-full bg-primary ring-2 ring-background" />
              
              {/* 摘要卡片 */}
              <div className="rounded-lg border border-border/60 p-3.5 space-y-2">
                {/* 日期头 */}
                <div className="flex items-center justify-between">
                  <h4 className="text-[14px] font-semibold">{formatDate(summary.summaryDate)}</h4>
                  <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
                    <FileText className="size-3" />
                    {summary.fragmentCount} 条碎片
                  </span>
                </div>
                
                {/* 摘要内容 */}
                <p className="text-[14px] text-foreground/80 leading-relaxed">
                  {summary.content}
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function formatDate(dateStr: string): string {
  const date = safeParseDate(dateStr)
  if (!date) return dateStr
  const today = new Date()
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)
  
  if (dateStr === today.toISOString().slice(0, 10)) return '今天'
  if (dateStr === yesterday.toISOString().slice(0, 10)) return '昨天'
  
  return date.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' })
}
