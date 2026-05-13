/**
 * GitWorkbenchDialog — 状态 / 差异 / 分支 三屏视图。
 *
 * 2026-05-13 redesign: ports the visual language from `ApprovalModal` —
 * theme-token color system (no more `text-black/82`, which broke under
 * warm-paper / qingye / forest-*), hero header with branch disc + path
 * subtitle + change-count pill, OVERVIEW stat cards on the status tab,
 * parsed status rows with semantic badges (vs raw `<pre>`), and shiki
 * syntax highlighting on the full-patch diff view.
 *
 * State + reload still live in `useGitWorkbench` (extracted in W6 PR B
 * Task 5) so this file stays focused on presentation.
 */

import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import {
  GitBranch,
  GitCommit,
  Loader2,
  RefreshCw,
  FileEdit,
  FilePlus,
  FileMinus,
  FileWarning,
  HelpCircle,
  Check,
  CornerDownRight,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  gitCheckoutBranch,
  type BranchListItem,
} from '@/modules/git/api'
import { useShikiHighlight } from '@/components/preview/hooks/useShikiHighlight'
import { toast } from 'sonner'
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

  // Parsed status — drives the change-count pill in the hero AND the
  // OVERVIEW stat cards. Re-derives on every status data change.
  const statusParsed = React.useMemo(
    () => wb.statusState.kind === 'ready'
      ? parseStatus(wb.statusState.data)
      : { rows: [], counts: { staged: 0, unstaged: 0, untracked: 0, conflicts: 0 } },
    [wb.statusState],
  )
  const totalChanges
    = statusParsed.counts.staged
    + statusParsed.counts.unstaged
    + statusParsed.counts.untracked
    + statusParsed.counts.conflicts

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-2xl overflow-hidden p-0 rounded-2xl sm:rounded-2xl [&>*]:min-w-0"
      >
        <DialogTitle className="sr-only">Git 工作台</DialogTitle>
        <DialogDescription className="sr-only">
          查看当前仓库的状态、差异与分支列表
        </DialogDescription>

        <Hero
          currentBranch={currentBranch}
          cwd={cwd}
          totalChanges={totalChanges}
          onReload={() => void wb.reload(wb.tab)}
        />

        {/* Tabs */}
        <div role="tablist" className="flex border-b border-border/60 bg-muted/20 px-2">
          {(Object.keys(TAB_LABEL) as Tab[]).map((t) => {
            const active = wb.tab === t
            return (
              <button
                key={t}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => wb.setTab(t)}
                className={cn(
                  'relative px-3.5 py-2 text-[13px] leading-6 outline-none transition-colors',
                  active
                    ? 'text-foreground'
                    : 'text-muted-foreground hover:text-foreground/85',
                )}
              >
                {TAB_LABEL[t]}
                {active && (
                  <span className="absolute inset-x-2 bottom-0 h-[1.5px] rounded-full bg-primary" aria-hidden />
                )}
              </button>
            )
          })}
        </div>

        {/* Body */}
        <div className="max-h-[60vh] min-h-[260px] overflow-y-auto bg-background">
          {wb.tab === 'status' && (
            <StatusView
              state={wb.statusState}
              parsed={statusParsed}
              cwdMissing={!cwd}
            />
          )}
          {wb.tab === 'diff' && (
            <DiffView
              state={wb.diffState}
              diffFull={wb.diffFull}
              onToggleFull={() => wb.setDiffFull(!wb.diffFull)}
              cwdMissing={!cwd}
              cwd={cwd}
            />
          )}
          {wb.tab === 'branches' && (
            <BranchListView
              state={wb.branchesState}
              currentBranch={currentBranch}
              cwd={cwd}
              cwdMissing={!cwd}
              onCheckedOut={() => void wb.reload('branches')}
            />
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ─── Hero ──────────────────────────────────────────────────────────────

function Hero({
  currentBranch,
  cwd,
  totalChanges,
  onReload,
}: {
  currentBranch?: string
  cwd?: string
  totalChanges: number
  onReload: () => void
}) {
  const folderHint = cwd
    ? cwd.split('/').filter(Boolean).slice(-2).join('/')
    : null

  const cleanState = totalChanges === 0
  return (
    <div className="flex items-start gap-3 px-5 pt-5 pb-4 pr-14 border-b border-border/60">
      <div
        className={cn(
          'shrink-0 inline-flex items-center justify-center size-10 rounded-xl',
          // Theme-tokenized — primary palette for "this is the active branch" feel.
          'bg-primary/10 text-primary',
        )}
        aria-hidden
      >
        <GitBranch className="size-5" strokeWidth={1.75} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-base font-semibold leading-tight text-foreground truncate">
            {currentBranch || '—'}
          </span>
        </div>
        {folderHint && (
          <div className="text-[12.5px] mt-0.5 text-muted-foreground truncate">
            {folderHint}
          </div>
        )}
      </div>
      <span
        className={cn(
          'shrink-0 inline-flex items-center gap-1.5 rounded-full px-2.5 py-1',
          'text-[10.5px] font-semibold uppercase tracking-wide',
          cleanState
            ? 'bg-success-bg text-success'
            : 'bg-warning-bg text-warning',
        )}
        title={cleanState ? '工作树干净' : `${totalChanges} 处变更`}
      >
        <span
          className={cn('size-1.5 rounded-full', cleanState ? 'bg-success' : 'bg-warning')}
          aria-hidden
        />
        {cleanState ? '干净' : `${totalChanges} 处变更`}
      </span>
      <button
        type="button"
        onClick={onReload}
        className={cn(
          'shrink-0 inline-flex items-center justify-center h-7 w-7 rounded-lg',
          'text-muted-foreground hover:bg-muted hover:text-foreground transition-colors',
        )}
        aria-label="刷新"
        title="刷新当前 Tab"
      >
        <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.75} />
      </button>
    </div>
  )
}

// ─── Status: parsed view ───────────────────────────────────────────────

interface StatusRow {
  /** Two-letter porcelain code: ` M`, `M `, `MM`, `??`, `A `, etc. */
  code: string
  /** Trimmed file path / spec. Includes rename arrows when present. */
  path: string
}

interface StatusParsed {
  rows: StatusRow[]
  counts: { staged: number; unstaged: number; untracked: number; conflicts: number }
}

function parseStatus(raw: string): StatusParsed {
  const rows: StatusRow[] = []
  const counts = { staged: 0, unstaged: 0, untracked: 0, conflicts: 0 }
  for (const line of raw.split('\n')) {
    if (!line) continue
    if (line.startsWith('##')) continue  // branch header — already in hero
    if (line.length < 3) continue
    const code = line.slice(0, 2)
    const path = line.slice(3).trim()
    if (!path) continue
    rows.push({ code, path })
    // Classify for the OVERVIEW counts.
    const x = code[0]
    const y = code[1]
    if (code === '??') {
      counts.untracked++
    } else if (x === 'U' || y === 'U' || code === 'AA' || code === 'DD') {
      counts.conflicts++
    } else {
      if (x && x !== ' ' && x !== '?') counts.staged++
      if (y && y !== ' ') counts.unstaged++
    }
  }
  return { rows, counts }
}

/** Visual classification of one porcelain row → (icon, tone label, color tone). */
function classifyStatusRow(code: string): {
  icon: React.ElementType
  label: string
  tone: 'staged' | 'unstaged' | 'untracked' | 'conflict' | 'renamed'
} {
  if (code === '??') return { icon: HelpCircle, label: '未跟踪', tone: 'untracked' }
  if (code[0] === 'U' || code[1] === 'U' || code === 'AA' || code === 'DD') {
    return { icon: FileWarning, label: '冲突', tone: 'conflict' }
  }
  if (code[0] === 'R' || code[1] === 'R') return { icon: CornerDownRight, label: '重命名', tone: 'renamed' }
  if (code[0] === 'A' || code[1] === 'A') return { icon: FilePlus, label: '新增', tone: 'staged' }
  if (code[0] === 'D' || code[1] === 'D') return { icon: FileMinus, label: '删除', tone: 'unstaged' }
  // Modified (`M `, ` M`, `MM`)
  if (code[0] === 'M' && code[1] !== ' ') {
    return { icon: FileEdit, label: '已暂存 · 又修改', tone: 'unstaged' }
  }
  if (code[0] === 'M') return { icon: FileEdit, label: '已暂存', tone: 'staged' }
  return { icon: FileEdit, label: '已修改', tone: 'unstaged' }
}

const STATUS_TONE: Record<string, { tint: string; text: string; pillTint: string; pillText: string }> = {
  staged:    { tint: 'bg-success/10',  text: 'text-success',  pillTint: 'bg-success-bg',  pillText: 'text-success' },
  unstaged:  { tint: 'bg-warning/10',  text: 'text-warning',  pillTint: 'bg-warning-bg',  pillText: 'text-warning' },
  untracked: { tint: 'bg-muted',       text: 'text-muted-foreground', pillTint: 'bg-muted', pillText: 'text-muted-foreground' },
  conflict:  { tint: 'bg-danger/10',   text: 'text-danger',   pillTint: 'bg-danger-bg',   pillText: 'text-danger' },
  renamed:   { tint: 'bg-primary/10',  text: 'text-primary',  pillTint: 'bg-primary/15',  pillText: 'text-primary' },
}

function StatusView({
  state,
  parsed,
  cwdMissing,
}: {
  state: ViewState<string>
  parsed: StatusParsed
  cwdMissing: boolean
}) {
  if (cwdMissing) return <CenteredHint>未选择项目，无法读取 Git 状态。</CenteredHint>
  if (state.kind === 'loading' || state.kind === 'idle') return <LoadingView />
  if (state.kind === 'error') return <ErrorBlock message={state.message} />
  if (state.kind === 'empty' || parsed.rows.length === 0) {
    return (
      <div className="px-5 py-10 text-center">
        <div className="inline-flex items-center justify-center size-12 rounded-2xl bg-success-bg text-success mb-3" aria-hidden>
          <Check className="size-6" />
        </div>
        <div className="text-[13.5px] text-foreground/85 font-medium">工作树干净</div>
        <div className="text-[12px] text-muted-foreground mt-0.5">没有未提交的变更。</div>
      </div>
    )
  }

  return (
    <div>
      {/* OVERVIEW row — small stat cards, mirrors ApprovalModal's pattern.
          Drop the 4th column to "conflicts" only when relevant; otherwise
          render an empty cell to preserve grid rhythm. */}
      <div className="grid grid-cols-4 gap-2 p-4 pb-3">
        <StatCard label="已暂存" value={parsed.counts.staged} tone="staged" />
        <StatCard label="未暂存" value={parsed.counts.unstaged} tone="unstaged" />
        <StatCard label="未跟踪" value={parsed.counts.untracked} tone="untracked" />
        {parsed.counts.conflicts > 0
          ? <StatCard label="冲突" value={parsed.counts.conflicts} tone="conflict" />
          : <div />}
      </div>
      <ul className="divide-y divide-border/40 px-2 pb-2">
        {parsed.rows.map((r, i) => {
          const klass = classifyStatusRow(r.code)
          const tone = STATUS_TONE[klass.tone]!
          const Icon = klass.icon
          return (
            <li
              key={`${r.code}-${r.path}-${i}`}
              className="flex items-center gap-2.5 px-3 py-1.5 rounded-md hover:bg-muted/40 transition-colors"
            >
              <span className={cn('shrink-0 inline-flex items-center justify-center size-6 rounded-md', tone.tint, tone.text)} aria-hidden>
                <Icon className="size-3.5" strokeWidth={1.75} />
              </span>
              <span
                className={cn(
                  'shrink-0 inline-flex items-center rounded px-1.5 py-0.5 font-mono',
                  'text-[10.5px] font-semibold',
                  tone.pillTint, tone.pillText,
                )}
                title={`porcelain: ${r.code === '??' ? '??' : r.code.replace(/ /g, '·')}`}
              >
                {klass.label}
              </span>
              <span className="truncate text-[12.5px] font-mono text-foreground/85">
                {r.path}
              </span>
            </li>
          )
        })}
      </ul>
    </div>
  )
}

function StatCard({
  label, value, tone,
}: { label: string; value: number; tone: 'staged' | 'unstaged' | 'untracked' | 'conflict' }) {
  const t = STATUS_TONE[tone]!
  return (
    <div className={cn('rounded-lg border px-3 py-2', t.tint, 'border-border/60')}>
      <div className={cn('text-[10px] font-semibold uppercase tracking-wide', t.text)}>{label}</div>
      <div className="text-[18px] font-semibold tabular-nums text-foreground mt-0.5">{value}</div>
    </div>
  )
}

// ─── Diff: shiki-highlighted patch ─────────────────────────────────────

function DiffView({
  state,
  diffFull,
  onToggleFull,
  cwdMissing,
  cwd,
}: {
  state: ViewState<string>
  diffFull: boolean
  onToggleFull: () => void
  cwdMissing: boolean
  cwd?: string
}) {
  if (cwdMissing) return <CenteredHint>未选择项目，无法读取 Git 状态。</CenteredHint>
  return (
    <div>
      <div className="flex items-center justify-between gap-2 border-b border-border/40 bg-muted/30 px-4 py-1.5 text-[11.5px] text-muted-foreground">
        <span>
          当前模式：
          <span className="ml-1 font-medium text-foreground/85">
            {diffFull ? '完整 patch' : '摘要 (--stat)'}
          </span>
        </span>
        <button
          type="button"
          onClick={onToggleFull}
          className={cn(
            'rounded-md border border-border/60 bg-background px-2 py-0.5',
            'text-[11px] font-medium text-foreground/70 transition-colors',
            'hover:bg-muted hover:text-foreground',
          )}
        >
          {diffFull ? '切回摘要' : '查看完整 patch'}
        </button>
      </div>
      {/* Stat mode: keep plain monospace (lines like ` foo.ts | 12 ++-- `).
          Full patch mode: shiki-highlight with `diff` language. */}
      {diffFull
        ? <DiffPatchView state={state} cwd={cwd ?? ''} />
        : <PlainTextView state={state} emptyHint="没有暂存或未暂存的差异。" />}
    </div>
  )
}

function DiffPatchView({ state, cwd }: { state: ViewState<string>; cwd: string }) {
  // Stable-shaped code for the highlight hook even when loading / empty,
  // so the hook always runs and React doesn't see a different hook list
  // across renders.
  const code = state.kind === 'ready' ? state.data : ''
  const { html, loading: shikiLoading } = useShikiHighlight({
    code,
    language: 'diff',
    cacheScope: `git-workbench-diff:${cwd}`,
    refreshVersion: 0,
  })

  if (state.kind === 'loading' || state.kind === 'idle') return <LoadingView />
  if (state.kind === 'error') return <ErrorBlock message={state.message} />
  if (state.kind === 'empty') return <CenteredHint>没有暂存或未暂存的差异。</CenteredHint>

  return (
    <div className="px-4 py-3">
      {html
        ? (
          <div
            className="shiki-render text-[12px] leading-5 font-mono [&_pre]:!bg-transparent [&_pre]:m-0 [&_pre]:p-0"
            // shiki output is sanitized HTML — already escaped by shiki itself.
            // eslint-disable-next-line react/no-danger
            dangerouslySetInnerHTML={{ __html: html }}
          />
        )
        : (
          <pre className={cn(
            'm-0 whitespace-pre-wrap break-words font-mono text-[12px] leading-5 text-foreground/85',
            shikiLoading && 'opacity-70',
          )}>
            {code}
          </pre>
        )}
    </div>
  )
}

// Plain-text view kept for the stat-mode diff + as a generic fallback.
function PlainTextView({
  state,
  emptyHint,
}: {
  state: ViewState<string>
  emptyHint: string
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
  if (state.kind === 'loading' || state.kind === 'idle') return <LoadingView />
  if (state.kind === 'error') return <ErrorBlock message={state.message} />
  if (state.kind === 'empty') return <CenteredHint>{emptyHint}</CenteredHint>

  const lines = state.data.split('\n')
  const total = lines.length
  const showAll = visibleLines >= total
  const slice = showAll ? lines : lines.slice(0, visibleLines)
  const remaining = total - slice.length

  return (
    <div>
      <pre className="m-0 whitespace-pre-wrap break-words px-4 py-3 font-mono text-[12px] leading-5 text-foreground/85">
        {slice.join('\n')}
      </pre>
      {!showAll && (
        <div className={cn(
          'sticky bottom-0 flex items-center justify-between gap-2 border-t border-border/40',
          'bg-background/85 backdrop-blur px-4 py-2 text-[11.5px] text-muted-foreground',
        )}>
          <span>
            已显示 <span className="font-medium text-foreground/85">{slice.length.toLocaleString()}</span> /
            {' '}{total.toLocaleString()} 行
          </span>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => setVisibleLines((c) => Math.min(c + LINE_PAGE_SIZE, total))}
              className="rounded-md border border-border/60 bg-background px-2 py-0.5 text-[11px] font-medium text-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
            >
              加载下 {Math.min(LINE_PAGE_SIZE, remaining).toLocaleString()} 行
            </button>
            <button
              type="button"
              onClick={() => {
                if (
                  total > FULL_EXPAND_WARN_THRESHOLD
                  && typeof window !== 'undefined'
                  && !window.confirm(`共 ${total.toLocaleString()} 行，一次渲染可能会卡顿。仍要全部展开吗？`)
                ) return
                setVisibleLines(total)
              }}
              className="rounded-md border border-border/60 bg-background px-2 py-0.5 text-[11px] font-medium text-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
            >
              全部展开
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

// ─── Branches: row + checkout action ───────────────────────────────────

function BranchListView({
  state,
  currentBranch,
  cwd,
  cwdMissing,
  onCheckedOut,
}: {
  state: ViewState<BranchListItem[]>
  currentBranch?: string
  cwd?: string
  cwdMissing: boolean
  onCheckedOut?: (branch: string) => void
}) {
  if (cwdMissing) return <CenteredHint>未选择项目，无法读取分支列表。</CenteredHint>
  if (state.kind === 'loading' || state.kind === 'idle') return <LoadingView />
  if (state.kind === 'error') return <ErrorBlock message={state.message} />
  if (state.kind === 'empty') return <CenteredHint>没有本地分支。</CenteredHint>

  return (
    <ul className="divide-y divide-border/40 p-2">
      {state.data.map((b) => {
        const isCurrent = b.isCurrent || b.name === currentBranch
        return (
          <BranchRow
            key={b.name}
            branch={b}
            isCurrent={isCurrent}
            cwd={cwd}
            onCheckedOut={onCheckedOut}
          />
        )
      })}
    </ul>
  )
}

function BranchRow({
  branch,
  isCurrent,
  cwd,
  onCheckedOut,
}: {
  branch: BranchListItem
  isCurrent: boolean
  cwd?: string
  onCheckedOut?: (branch: string) => void
}) {
  const [busy, setBusy] = React.useState(false)

  const handleCheckout = async () => {
    if (!cwd || busy || isCurrent) return
    setBusy(true)
    try {
      await gitCheckoutBranch(cwd, branch.name)
      toast.success(`已切换到 ${branch.name}`)
      onCheckedOut?.(branch.name)
    } catch (e) {
      toast.error('切换分支失败', { description: String(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <li
      className={cn(
        'flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors',
        isCurrent ? 'bg-primary/[0.06]' : 'hover:bg-muted/40',
      )}
    >
      <span
        className={cn(
          'shrink-0 inline-flex items-center justify-center size-6 rounded-md',
          isCurrent ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground',
        )}
        aria-hidden
      >
        <GitCommit className="size-3.5" strokeWidth={1.75} />
      </span>
      <span
        className={cn(
          'truncate text-[13px] font-mono',
          isCurrent ? 'font-semibold text-foreground' : 'text-foreground/80',
        )}
      >
        {branch.name}
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        {isCurrent
          ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-success-bg px-2 py-0.5 text-[10.5px] font-semibold uppercase tracking-wide text-success">
              <span className="size-1.5 rounded-full bg-success" aria-hidden />
              当前
            </span>
          )
          : (
            <button
              type="button"
              disabled={busy || !cwd}
              onClick={handleCheckout}
              className={cn(
                'inline-flex items-center gap-1 rounded-md border border-border/60 bg-background px-2 py-0.5',
                'text-[11px] font-medium text-foreground/70 transition-colors',
                'hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60',
              )}
            >
              {busy && <Loader2 className="size-3 animate-spin" />}
              切换
            </button>
          )}
      </div>
    </li>
  )
}

// ─── Shared ───────────────────────────────────────────────────────────

function LoadingView() {
  return (
    <div className="flex items-center justify-center gap-2 py-12 text-[13px] text-muted-foreground">
      <Loader2 className="h-3.5 w-3.5 animate-spin" />
      加载中…
    </div>
  )
}

function CenteredHint({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-center px-6 py-12 text-center text-[13px] text-muted-foreground">
      {children}
    </div>
  )
}

function ErrorBlock({ message }: { message: string }) {
  return (
    <div className="m-4 rounded-lg border border-danger/30 bg-danger-bg px-3 py-2 text-[12.5px] leading-5 text-danger">
      {message}
    </div>
  )
}
