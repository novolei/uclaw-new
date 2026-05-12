/**
 * GitWorkbenchDialog — 状态 / 差异 / 分支 三屏只读视图。
 *
 * Verbatim port of if2Ai's GitWorkbenchDialog JSX (lines 138-419) + local
 * sub-components. State + reload extracted into useGitWorkbench hook (Task 5)
 * to keep this file under uClaw's 400 LOC hard cap.
 */

import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { GitBranch, Loader2, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { type BranchListItem } from '@/modules/git/api'
import { useGitWorkbench, type Tab, type ViewState } from './useGitWorkbench'

type Props = {
  open: boolean
  onOpenChange: (next: boolean) => void
  cwd: string | undefined
  currentBranch?: string
}

const TAB_LABEL: Record<Tab, string> = {
  status: '状态',
  diff: '差异',
  branches: '分支',
}

const LINE_PAGE_SIZE = 500
const FULL_EXPAND_WARN_THRESHOLD = 5000

export function GitWorkbenchDialog({ open, onOpenChange, cwd, currentBranch }: Props) {
  const wb = useGitWorkbench({ open, cwd })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-[min(92vw,42rem)] gap-0 p-0"
      >
        <DialogTitle className="sr-only">Git 工作台</DialogTitle>
        <DialogDescription className="sr-only">
          查看当前仓库的状态、差异与分支列表
        </DialogDescription>

        {/* Header — `pr-12` 给 DialogContent 自带的右上角关闭按钮留位，
            否则 RefreshCw 会被 X 盖住（参见 dialog.tsx 的 absolute X 按钮）。 */}
        <div className="flex items-center justify-between gap-3 border-b border-black/[0.06] py-3 pl-4 pr-12">
          <div className="flex min-w-0 items-center gap-2">
            <GitBranch className="h-4 w-4 shrink-0 text-black/55" strokeWidth={1.75} />
            <span className="truncate text-[13px] font-medium text-black/82">
              {currentBranch || '—'}
            </span>
            {cwd ? (
              <span className="truncate text-[11.5px] text-black/35">
                {cwd.split('/').filter(Boolean).slice(-2).join('/')}
              </span>
            ) : null}
          </div>
          <button
            type="button"
            onClick={() => void wb.reload(wb.tab)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-black/40 transition-colors hover:bg-black/[0.04] hover:text-black/70"
            aria-label="刷新"
            title="刷新当前 Tab"
          >
            <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.75} />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-black/[0.06] px-2">
          {(Object.keys(TAB_LABEL) as Tab[]).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => wb.setTab(t)}
              className={cn(
                'relative px-3 py-2 text-[13px] leading-6 transition-colors',
                wb.tab === t
                  ? 'text-black/85'
                  : 'text-black/45 hover:text-black/70',
              )}
            >
              {TAB_LABEL[t]}
              {wb.tab === t && (
                <span className="absolute bottom-0 left-2 right-2 h-[1.5px] rounded-full bg-black/75" />
              )}
            </button>
          ))}
        </div>

        {/* Body */}
        <div className="max-h-[60vh] min-h-[260px] overflow-y-auto">
          {wb.tab === 'status' && (
            <PlainTextView
              state={wb.statusState}
              emptyHint="工作树干净。"
              cwdMissing={!cwd}
            />
          )}
          {wb.tab === 'diff' && (
            <div>
              {/* stat ⇄ full 切换条；放在 body 顶部，跟随滚动以节省垂直空间 */}
              <div className="flex items-center justify-between gap-2 border-b border-black/[0.05] bg-white/60 px-4 py-1.5 text-[11.5px] text-black/55">
                <span>
                  当前模式：
                  <span className="ml-1 font-medium text-black/70">
                    {wb.diffFull ? '完整 patch' : '摘要 (--stat)'}
                  </span>
                </span>
                <button
                  type="button"
                  onClick={() => wb.setDiffFull(!wb.diffFull)}
                  className="rounded-md border border-black/10 bg-white px-2 py-0.5 text-[11px] font-medium text-black/65 transition-colors hover:bg-black/[0.04] hover:text-black/82"
                >
                  {wb.diffFull ? '切回摘要' : '查看完整 patch'}
                </button>
              </div>
              <PlainTextView
                state={wb.diffState}
                emptyHint="没有暂存或未暂存的差异。"
                cwdMissing={!cwd}
              />
            </div>
          )}
          {wb.tab === 'branches' && (
            <BranchListView
              state={wb.branchesState}
              currentBranch={currentBranch}
              cwdMissing={!cwd}
            />
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function PlainTextView({
  state,
  emptyHint,
  cwdMissing,
}: {
  state: ViewState<string>
  emptyHint: string
  cwdMissing: boolean
}) {
  const [visibleLines, setVisibleLines] = React.useState(LINE_PAGE_SIZE)
  const lastDataRef = React.useRef<string | null>(null)
  const data = state.kind === 'ready' ? state.data : null
  React.useEffect(() => {
    if (data !== lastDataRef.current) {
      lastDataRef.current = data
      setVisibleLines(LINE_PAGE_SIZE)
    }
  }, [data])

  if (cwdMissing) {
    return (
      <CenteredHint>未选择项目，无法读取 Git 状态。</CenteredHint>
    )
  }
  if (state.kind === 'loading' || state.kind === 'idle') {
    return (
      <div className="flex items-center justify-center gap-2 py-12 text-[13px] text-black/40">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        加载中…
      </div>
    )
  }
  if (state.kind === 'error') {
    return <ErrorBlock message={state.message} />
  }
  if (state.kind === 'empty') {
    return <CenteredHint>{emptyHint}</CenteredHint>
  }
  // ready —— chunk 渲染
  const lines = state.data.split('\n')
  const total = lines.length
  const showAll = visibleLines >= total
  const slice = showAll ? lines : lines.slice(0, visibleLines)
  const remaining = total - slice.length

  const onLoadMore = () => {
    setVisibleLines((current) => Math.min(current + LINE_PAGE_SIZE, total))
  }
  const onLoadAll = () => {
    if (
      total > FULL_EXPAND_WARN_THRESHOLD &&
      typeof window !== 'undefined' &&
      !window.confirm(
        `共 ${total.toLocaleString()} 行，一次渲染可能会卡顿。仍要全部展开吗？`,
      )
    ) {
      return
    }
    setVisibleLines(total)
  }

  return (
    <div>
      <pre className="m-0 whitespace-pre-wrap break-words px-4 py-3 font-mono text-[12px] leading-5 text-black/82">
        {slice.join('\n')}
      </pre>
      {!showAll && (
        <div className="sticky bottom-0 flex items-center justify-between gap-2 border-t border-black/[0.05] bg-white/85 px-4 py-2 text-[11.5px] text-black/55 backdrop-blur">
          <span>
            已显示 <span className="font-medium text-black/70">{slice.length.toLocaleString()}</span> /
            {' '}{total.toLocaleString()} 行
          </span>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={onLoadMore}
              className="rounded-md border border-black/10 bg-white px-2 py-0.5 text-[11px] font-medium text-black/65 outline-none transition-colors hover:bg-black/[0.04] hover:text-black/82 focus-visible:bg-black/[0.04]"
            >
              加载下 {Math.min(LINE_PAGE_SIZE, remaining).toLocaleString()} 行
            </button>
            <button
              type="button"
              onClick={onLoadAll}
              className="rounded-md border border-black/10 bg-white px-2 py-0.5 text-[11px] font-medium text-black/65 outline-none transition-colors hover:bg-black/[0.04] hover:text-black/82 focus-visible:bg-black/[0.04]"
            >
              全部展开
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

function BranchListView({
  state,
  currentBranch,
  cwdMissing,
}: {
  state: ViewState<BranchListItem[]>
  currentBranch?: string
  cwdMissing: boolean
}) {
  if (cwdMissing) {
    return <CenteredHint>未选择项目，无法读取分支列表。</CenteredHint>
  }
  if (state.kind === 'loading' || state.kind === 'idle') {
    return (
      <div className="flex items-center justify-center gap-2 py-12 text-[13px] text-black/40">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        加载中…
      </div>
    )
  }
  if (state.kind === 'error') {
    return <ErrorBlock message={state.message} />
  }
  if (state.kind === 'empty') {
    return <CenteredHint>没有本地分支。</CenteredHint>
  }
  return (
    <ul className="divide-y divide-black/[0.05]">
      {state.data.map((b) => {
        const isCurrent = b.isCurrent || b.name === currentBranch
        return (
          <li
            key={b.name}
            className="flex items-center gap-2.5 px-4 py-2 text-[13px] leading-6"
          >
            <GitBranch
              className={cn(
                'h-[14px] w-[14px] shrink-0',
                isCurrent ? 'text-black/70' : 'text-black/40',
              )}
              strokeWidth={1.75}
            />
            <span className={cn('truncate', isCurrent ? 'font-medium text-black/85' : 'text-black/72')}>
              {b.name}
            </span>
            {isCurrent && (
              <span className="ml-auto rounded-full bg-emerald-100 px-1.5 py-0.5 text-[11px] font-medium text-emerald-700">
                当前
              </span>
            )}
          </li>
        )
      })}
    </ul>
  )
}

function CenteredHint({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-center px-6 py-12 text-center text-[13px] text-black/45">
      {children}
    </div>
  )
}

function ErrorBlock({ message }: { message: string }) {
  return (
    <div className="px-4 py-3 text-[12.5px] leading-5 text-rose-600/85">
      {message}
    </div>
  )
}
