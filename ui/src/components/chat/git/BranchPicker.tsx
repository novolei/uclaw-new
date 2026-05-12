/**
 * BranchPicker — composer footer's "current git branch" chip + dropdown.
 *
 * Verbatim port of if2Ai's BranchPicker (Tailwind, JSX, Chinese labels,
 * ARIA all preserved). Three adaptations vs the if2Ai source:
 *
 * 1. State + handlers extracted into `./useBranchPicker.ts` so this
 *    file stays under uClaw's 400 LOC hard cap.
 * 2. `the-finals-selected-menu-item` class dropped from the
 *    current-branch row — it's a theme-scoped hook for if2Ai's
 *    "The Finals" theme, no-op in uClaw.
 * 3. Init-repo flow surfaces a `sonner` confirmation toast before
 *    running `gitInitRepo` (per W6 spec §4.2 — workspace is a long-
 *    lived user dir; surprise repo creation is bad UX).
 */

import * as React from 'react'
import { Check, GitBranch, Loader2, Plus, Search, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useBranchPicker, type UseBranchPickerArgs } from './useBranchPicker'

type Props = UseBranchPickerArgs & {
  className?: string
}

export function BranchPicker(props: Props) {
  const {
    open, setOpen, state, query, setQuery,
    creating, setCreating, busyBranch, createName, setCreateName,
    pendingCheckout, setPendingCheckout, initing,
    filtered, noRepo, popoverDisabled, triggerDisabled,
    handleInit, handleCheckout, handleCreate, runCheckout,
  } = useBranchPicker(props)

  const { currentBranch, className } = props

  // uClaw adaptation: confirmation toast before git init (W6 spec §4.2).
  // Click on amber "无 Git 仓库" trigger → toast offers "初始化" CTA.
  // User must confirm; passive dismiss does nothing.
  const handleInitWithConfirm = React.useCallback(() => {
    toast(
      `您将在当前工作区初始化 Git 仓库吗？`,
      {
        duration: 5000,
        action: {
          label: '初始化',
          onClick: () => void handleInit(),
        },
      },
    )
  }, [handleInit])

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (popoverDisabled) return
        setOpen(next)
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={triggerDisabled}
          onClick={
            noRepo
              ? (e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  handleInitWithConfirm()
                }
              : undefined
          }
          title={noRepo ? '当前目录不是 Git 仓库 — 点击执行 git init' : undefined}
          className={cn(
            'flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] transition-colors disabled:cursor-not-allowed disabled:opacity-60',
            noRepo
              ? 'text-amber-600 hover:bg-amber-500/12 hover:text-amber-500'
              : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
            className,
          )}
          aria-label={noRepo ? '初始化 Git 仓库' : '切换 git 分支'}
        >
          <GitBranch className="h-[11px] w-[11px]" />
          {noRepo ? (
            <span className="inline-flex items-center gap-1">
              {initing ? (
                <Loader2 className="h-[10px] w-[10px] animate-spin" />
              ) : (
                <Sparkles className="h-[10px] w-[10px]" />
              )}
              <span>无 Git 仓库 · 点击初始化</span>
            </span>
          ) : (
            <span className="max-w-[160px] truncate">
              {currentBranch || '—'}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="center"
        sideOffset={12}
        collisionPadding={16}
        className={cn(
          'w-[260px] overflow-hidden rounded-2xl border border-border/70 bg-popover/96 p-0 text-[13px] text-popover-foreground backdrop-blur-2xl backdrop-saturate-150',
          'shadow-[0_2px_4px_rgba(0,0,0,0.04),0_8px_20px_rgba(0,0,0,0.08),0_24px_56px_rgba(0,0,0,0.16),0_0_0_0.5px_rgba(0,0,0,0.04)]',
          'origin-[var(--radix-popover-content-transform-origin)] transition-all duration-200 ease-out',
          'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=open]:slide-in-from-top-1',
          'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95',
        )}
      >
        {pendingCheckout && state.kind === 'ready' && (
          <div className="border-b border-amber-200/70 bg-amber-50/70 px-3.5 py-3">
            <div className="text-[12px] leading-5 text-amber-900">
              工作区有 <span className="font-semibold">{state.uncommittedCount}</span> 个未提交的文件。切到{' '}
              <span className="font-mono text-[12px] text-amber-900">{pendingCheckout}</span>{' '}
              可能会覆盖或保留改动，git 视情况决定 —— 建议先提交或暂存。
            </div>
            <div className="mt-2 flex items-center justify-end gap-1.5">
              <button
                type="button"
                onClick={() => setPendingCheckout(null)}
                className="rounded-md px-2.5 py-1 text-[11.5px] text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground"
              >
                返回
              </button>
              <button
                type="button"
                onClick={() => void runCheckout(pendingCheckout)}
                className="rounded-md bg-amber-600 px-2.5 py-1 text-[11.5px] font-medium text-white outline-none transition-opacity hover:opacity-90 focus-visible:opacity-90"
              >
                仍要切换
              </button>
            </div>
          </div>
        )}

        {/* Search */}
        <div className="flex items-center gap-2 px-3.5 pt-3 pb-2.5">
          <Search className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <input
            type="text"
            autoFocus
            placeholder="搜索分支"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 bg-transparent text-[11.5px] leading-6 text-popover-foreground outline-none placeholder:text-muted-foreground"
          />
        </div>

        {/* List */}
        <div className="max-h-[280px] overflow-y-auto pb-1.5">
          {state.kind === 'loading' && (
            <div className="flex items-center justify-center gap-2 py-7 text-[13px] text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>加载中…</span>
            </div>
          )}
          {state.kind === 'error' && (
            <div className="px-3.5 py-3 text-[11.5px] leading-5 text-rose-600/80">
              {state.message}
            </div>
          )}
          {state.kind === 'ready' && (
            <>
              <div className="px-3.5 pb-1 pt-1 text-[11.5px] text-muted-foreground">
                分支
              </div>
              {filtered.length === 0 && (
                <div className="px-3.5 py-5 text-center text-[13px] text-muted-foreground">
                  无匹配分支
                </div>
              )}
              {filtered.map((b) => {
                const isCurrent = b.isCurrent || b.name === currentBranch
                const isBusy = busyBranch === b.name
                return (
                  <button
                    key={b.name}
                    type="button"
                    disabled={isBusy}
                    aria-selected={isCurrent}
                    onClick={() => handleCheckout(b.name)}
                    className={cn(
                      'flex w-full items-start gap-2.5 px-3.5 py-1.5 text-left outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground',
                      // NOTE: 'the-finals-selected-menu-item' from if2Ai dropped per W6 PR B Task 4 adaptation
                      isBusy && 'opacity-60',
                    )}
                  >
                    <GitBranch
                      className={cn(
                        'mt-[3px] h-[14px] w-[14px] shrink-0',
                        isCurrent ? 'text-primary' : 'text-muted-foreground',
                      )}
                      strokeWidth={1.75}
                    />
                    <div className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] leading-6 text-popover-foreground">
                        {b.name}
                      </span>
                      {isCurrent && state.uncommittedCount > 0 && (
                        <div className="text-[11.5px] leading-5 text-muted-foreground">
                          未提交的更改：{state.uncommittedCount} 个文件
                        </div>
                      )}
                    </div>
                    {isCurrent && !isBusy && (
                      <Check
                        className="mt-[5px] h-[13px] w-[13px] shrink-0 text-primary"
                        strokeWidth={2}
                      />
                    )}
                    {isBusy && (
                      <Loader2 className="mt-[5px] h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
                    )}
                  </button>
                )
              })}
            </>
          )}
        </div>

        {/* Create new branch */}
        <div className="border-t border-border/65">
          {!creating ? (
            <button
              type="button"
              onClick={() => setCreating(true)}
              disabled={state.kind !== 'ready'}
              className="flex w-full items-center gap-2.5 px-3.5 py-2.5 text-left text-[11.5px] leading-6 text-popover-foreground/80 outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground disabled:cursor-not-allowed disabled:opacity-60"
            >
              <Plus className="h-3.5 w-3.5 text-muted-foreground" strokeWidth={2} />
              创建并检出新分支…
            </button>
          ) : (
            <div className="flex items-center gap-2 px-3.5 py-2.5">
              <input
                autoFocus
                type="text"
                placeholder="新分支名"
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    void handleCreate()
                  } else if (e.key === 'Escape') {
                    setCreating(false)
                    setCreateName('')
                  }
                }}
                className="flex-1 rounded-lg border border-border/70 bg-muted px-2.5 py-1.5 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
              />
              <button
                type="button"
                onClick={() => void handleCreate()}
                disabled={!createName.trim() || busyBranch !== null}
                className="rounded-lg bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                创建
              </button>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
