/**
 * GitActionsPicker.tsx
 *
 * Verbatim port of if2Ai's GitActionsPicker.tsx (717 LOC monolith) split
 * across 3 files for uClaw's 400 LOC cap. This file: outer chip trigger +
 * Popover shell + Mode state machine + menu sub-view + busy/success/error
 * rendered states + all dispatcher logic (runCommit, runCreateBranch,
 * runCreatePr, runInitRepo).
 *
 * See sibling files for the other halves of the split.
 */

import * as React from 'react'
import {
  ChevronDown,
  GitCommitHorizontal,
  Sparkles,
} from 'lucide-react'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import {
  ghAvailable,
  gitCommit,
  gitCommitPushPr,
  gitCreateBranch,
  gitInitRepo,
} from '@/modules/git/api'
import {
  BusyView,
  CommitForm,
  CreateBranchForm,
  ErrorView,
  MenuContent,
  PrForm,
  SuccessView,
} from './GitActionsPickerForms'
import { PrDraftView } from './GitActionsPickerDraftPr'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Props = {
  cwd: string | undefined
  /** Tri-state Git presence — same semantics as `BranchPicker.isGitRepo`. */
  isGitRepo?: boolean | null
  /** Fired after this picker successfully runs `git init`; parent should
   *  re-probe `gitIsRepo` so both pickers re-enable. */
  onGitRepoChanged?: () => void
  /** Notify parent that the current branch may have changed (after
   *  create-branch flow).  Hooked to the same `setBranchLabel` used by
   *  `BranchPicker`. */
  onBranchChange?: (newBranch: string) => void
  /** Optional callback to open the Git Workbench drawer (status / diff
   *  / branches view).  When provided, the menu surfaces a "查看 Git
   *  状态" entry; when omitted that entry is hidden so the picker stays
   *  usable in environments without a workbench (settings page, etc). */
  onOpenWorkbench?: () => void
  className?: string
  /** Visual placement context. Default `'composer'` preserves the original
   *  in-toolbar chip style. `'sidebar'` switches to the lighter, lower-
   *  emphasis style used in `LeftSidebar` (mirrors the MCP·Skills row
   *  aesthetic) and flips the popover to open to the right instead of
   *  above. See spec §3.2 for the per-aspect table. */
  variant?: 'composer' | 'sidebar'
}

type Mode =
  | { kind: 'menu' }
  | { kind: 'commit' }
  | { kind: 'createBranch' }
  | { kind: 'pr' }
  | { kind: 'busy'; label: string }
  | { kind: 'success'; message: string }
  | { kind: 'error'; message: string }
  /**
   * "gh 不可用" 兜底视图：拿到用户填的 PR title/body 后，不发 IPC，
   * 而是把可执行的 `gh pr create` 命令 + body 一起展示，让用户复制后
   * 自己粘贴到终端。配合 `installGuidance: true` 时还会显示安装链接。
   */
  | { kind: 'prDraft'; title: string; body: string }

// ---------------------------------------------------------------------------
// GitActionsPicker
// ---------------------------------------------------------------------------

export function GitActionsPicker({
  cwd,
  isGitRepo = null,
  onGitRepoChanged,
  onBranchChange,
  onOpenWorkbench,
  className,
  variant = 'composer',
}: Props) {
  const isSidebar = variant === 'sidebar'
  const [open, setOpen] = React.useState(false)
  const [mode, setMode] = React.useState<Mode>({ kind: 'menu' })
  const [commitMessage, setCommitMessage] = React.useState('')
  const [branchName, setBranchName] = React.useState('')
  const [prTitle, setPrTitle] = React.useState('')
  const [prBody, setPrBody] = React.useState('')
  // `gh` 探测结果：null = 还在探测；true/false = 已知。
  // 第一次 popover 打开时触发探测；探测期间 UI 走"假定可用"的乐观
  // 路径，请求失败再回到草稿模式（由 runCreatePr 的 catch 兜底）。
  const [ghOk, setGhOk] = React.useState<boolean | null>(null)

  const noCwd = !cwd || cwd.trim() === ''
  const noRepo = isGitRepo === false
  const disabled = noCwd

  React.useEffect(() => {
    if (!open) {
      setMode({ kind: 'menu' })
      setCommitMessage('')
      setBranchName('')
      setPrTitle('')
      setPrBody('')
      return
    }
    if (ghOk === null) {
      let cancelled = false
      void ghAvailable()
        .then((ok) => { if (!cancelled) setGhOk(ok) })
        .catch(() => { if (!cancelled) setGhOk(false) })
      return () => { cancelled = true }
    }
  }, [open, ghOk])

  const runCommit = async () => {
    if (!cwd || !commitMessage.trim()) return
    setMode({ kind: 'busy', label: '正在提交…' })
    try {
      const outcome = await gitCommit(cwd, commitMessage.trim())
      setMode({
        kind: 'success',
        message:
          outcome.status === 'created' ? '已提交' : '工作区干净，已跳过提交',
      })
    } catch (err) {
      setMode({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  const runCreateBranch = async () => {
    const name = branchName.trim()
    if (!cwd || !name) return
    setMode({ kind: 'busy', label: '正在创建分支…' })
    try {
      await gitCreateBranch(cwd, name)
      onBranchChange?.(name)
      setMode({ kind: 'success', message: `已切换到 ${name}` })
    } catch (err) {
      setMode({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  const runInitRepo = async () => {
    if (!cwd) return
    setMode({ kind: 'busy', label: '正在初始化 Git…' })
    try {
      await gitInitRepo(cwd)
      onGitRepoChanged?.()
      setMode({
        kind: 'success',
        message: '已在当前项目目录初始化 Git 仓库',
      })
    } catch (err) {
      setMode({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  const runCreatePr = async () => {
    if (!cwd || !prTitle.trim()) return
    // gh 已探明缺失 → 直接进入草稿模式，不发 IPC（backend 会
    // 立刻返回 MissingBinary，等价于多一次 round-trip）
    if (ghOk === false) {
      setMode({ kind: 'prDraft', title: prTitle.trim(), body: prBody })
      return
    }
    setMode({ kind: 'busy', label: '正在提交并创建 PR…' })
    try {
      const result = await gitCommitPushPr({
        cwd,
        title: prTitle.trim(),
        body: prBody,
      })
      setMode({ kind: 'success', message: result })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      // 兜底：探测期间用户已经按了提交，IPC 失败提示是 gh 相关 →
      // 转到草稿模式，避免给用户一段没头没尾的英文 stderr
      if (/gh\b|MissingBinary/i.test(message)) {
        setGhOk(false)
        setMode({ kind: 'prDraft', title: prTitle.trim(), body: prBody })
        return
      }
      setMode({ kind: 'error', message })
    }
  }

  const renderBody = () => {
    switch (mode.kind) {
      case 'menu':
        return (
          <MenuContent
            noRepo={noRepo}
            onOpenWorkbench={onOpenWorkbench}
            onInitRepo={runInitRepo}
            onSetCommitMode={() => setMode({ kind: 'commit' })}
            onSetPushError={() =>
              setMode({
                kind: 'error',
                message: '推送暂未单独支持，请使用「创建拉取请求」一键提交并推送。',
              })
            }
            onSetPrMode={() => setMode({ kind: 'pr' })}
            onSetCreateBranchMode={() => setMode({ kind: 'createBranch' })}
            onClose={() => setOpen(false)}
          />
        )

      case 'commit':
        return (
          <CommitForm
            commitMessage={commitMessage}
            setCommitMessage={setCommitMessage}
            onSubmit={runCommit}
            onCancel={() => setMode({ kind: 'menu' })}
          />
        )

      case 'createBranch':
        return (
          <CreateBranchForm
            branchName={branchName}
            setBranchName={setBranchName}
            onSubmit={runCreateBranch}
            onCancel={() => setMode({ kind: 'menu' })}
          />
        )

      case 'pr':
        return (
          <PrForm
            ghOk={ghOk}
            prTitle={prTitle}
            setPrTitle={setPrTitle}
            prBody={prBody}
            setPrBody={setPrBody}
            onSubmit={runCreatePr}
            onCancel={() => setMode({ kind: 'menu' })}
          />
        )

      case 'prDraft':
        return <PrDraftView title={mode.title} body={mode.body} onBack={() => setMode({ kind: 'pr' })} />

      case 'busy':
        return <BusyView label={mode.label} />

      case 'success':
        return <SuccessView message={mode.message} onClose={() => setOpen(false)} />

      case 'error':
        return <ErrorView message={mode.message} onBack={() => setMode({ kind: 'menu' })} />
    }
  }

  return (
    <Popover open={open} onOpenChange={(next) => !disabled && setOpen(next)}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          className={cn(
            'window-no-drag transition-colors disabled:cursor-not-allowed disabled:opacity-60',
            // Variant: composer uses the original chip style; sidebar
            // borrows BranchPicker's lighter aesthetic so the row sits
            // visually flush with MCP·Skills (per spec §3.2).
            isSidebar
              ? cn(
                  'flex items-center gap-1.5 rounded-[10px] px-3 py-2 text-[12px]',
                  noRepo
                    ? 'text-amber-600 hover:bg-amber-500/12 hover:text-amber-500'
                    : 'text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70',
                )
              : cn(
                  'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-[12px] font-medium',
                  noRepo
                    ? 'border-amber-200 bg-amber-50 text-amber-800 hover:border-amber-300 hover:bg-amber-100'
                    : 'border-border/70 text-muted-foreground hover:border-border hover:bg-accent hover:text-accent-foreground',
                ),
            className,
          )}
          data-window-no-drag="true"
          aria-label="Git 操作"
          title={noRepo ? '当前目录尚未初始化 Git — 点击查看初始化选项' : undefined}
        >
          {noRepo ? (
            <Sparkles className={cn(isSidebar ? 'h-[13px] w-[13px]' : 'h-3.5 w-3.5')} strokeWidth={1.75} />
          ) : (
            <GitCommitHorizontal className={cn(isSidebar ? 'h-[13px] w-[13px]' : 'h-3.5 w-3.5')} strokeWidth={1.75} />
          )}
          <span>{noRepo ? '初始化 Git' : '提交'}</span>
          <ChevronDown
            className={cn(
              isSidebar ? 'h-[11px] w-[11px] text-foreground/30' : 'h-3 w-3',
              !isSidebar && (noRepo ? 'text-amber-600' : 'text-muted-foreground'),
            )}
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        side={isSidebar ? 'right' : 'top'}
        align={isSidebar ? 'start' : 'center'}
        sideOffset={isSidebar ? 8 : 12}
        collisionPadding={16}
        className={cn(
          'w-[240px] overflow-hidden rounded-2xl border border-border/70 bg-popover/96 p-0 text-[13px] text-popover-foreground backdrop-blur-2xl backdrop-saturate-150',
          'shadow-[0_2px_4px_rgba(0,0,0,0.04),0_8px_20px_rgba(0,0,0,0.08),0_24px_56px_rgba(0,0,0,0.16),0_0_0_0.5px_rgba(0,0,0,0.04)]',
          'origin-[var(--radix-popover-content-transform-origin)] transition-all duration-200 ease-out',
          'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95',
          isSidebar ? 'data-[state=open]:slide-in-from-left-1' : 'data-[state=open]:slide-in-from-top-1',
          'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95',
        )}
      >
        {renderBody()}
      </PopoverContent>
    </Popover>
  )
}
