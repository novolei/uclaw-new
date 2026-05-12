/**
 * GitActionsPickerForms.tsx
 *
 * Verbatim port of if2Ai's GitActionsPicker.tsx (717 LOC monolith) split
 * across 3 files for uClaw's 400 LOC cap. This file: ActionItem menu-row
 * primitive, MenuContent (init-only + full menu views), CommitForm,
 * CreateBranchForm, and PrForm sub-components
 * (presentational; receive state + dispatchers as props from the picker
 * shell), plus the shared FormShell and PrimaryButton primitives.
 *
 * See sibling files for the other halves of the split.
 */

import * as React from 'react'
import {
  GitBranch,
  GitCommitHorizontal,
  GitPullRequestArrow,
  Loader2,
  PanelTopOpen,
  Sparkles,
  UploadCloud,
  X,
} from 'lucide-react'
import { GhMissingBanner } from './GitActionsPickerDraftPr'

// ---------------------------------------------------------------------------
// ActionItem — menu row primitive
// ---------------------------------------------------------------------------

export function ActionItem({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      // `outline-none` 抑制 WebKit `:focus-visible` 默认蓝色描边；
      // 用 `focus-visible:bg-...` 给键盘用户保留可见的焦点反馈，
      // 同时不会被 PopoverContent 的 overflow-hidden 切成横线。
      className="flex w-full items-center gap-2.5 px-3.5 py-1.5 text-left text-[11.5px] leading-6 text-popover-foreground outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground"
    >
      <span className="text-muted-foreground">{icon}</span>
      {label}
    </button>
  )
}

// ---------------------------------------------------------------------------
// MenuContent — the mode==='menu' sub-view (init-only or full menu)
// ---------------------------------------------------------------------------

export function MenuContent({
  noRepo,
  onOpenWorkbench,
  onInitRepo,
  onSetCommitMode,
  onSetPushError,
  onSetPrMode,
  onSetCreateBranchMode,
  onClose,
}: {
  noRepo: boolean
  onOpenWorkbench?: () => void
  onInitRepo: () => void
  onSetCommitMode: () => void
  onSetPushError: () => void
  onSetPrMode: () => void
  onSetCreateBranchMode: () => void
  onClose: () => void
}) {
  if (noRepo) {
    return (
      <>
        <div className="px-3.5 pb-1 pt-2.5 text-[11.5px] text-muted-foreground">
          Git 操作
        </div>
        <div className="px-3.5 pb-2 text-[11.5px] leading-5 text-muted-foreground">
          当前项目目录还不是 Git 仓库。初始化后即可使用提交、分支、PR 等功能。
        </div>
        <ActionItem
          icon={<Sparkles className="h-[14px] w-[14px]" strokeWidth={1.75} />}
          label="在此目录初始化 Git 仓库"
          onClick={onInitRepo}
        />
        <div className="h-1.5" />
      </>
    )
  }
  return (
    <>
      <div className="px-3.5 pb-1 pt-2.5 text-[11.5px] text-muted-foreground">
        Git 操作
      </div>
      {onOpenWorkbench && (
        <ActionItem
          icon={<PanelTopOpen className="h-[14px] w-[14px]" strokeWidth={1.75} />}
          label="查看 Git 状态…"
          onClick={() => {
            onClose()
            onOpenWorkbench()
          }}
        />
      )}
      <ActionItem
        icon={<GitCommitHorizontal className="h-[14px] w-[14px]" strokeWidth={1.75} />}
        label="提交"
        onClick={onSetCommitMode}
      />
      <ActionItem
        icon={<UploadCloud className="h-[14px] w-[14px]" strokeWidth={1.75} />}
        label="推送"
        onClick={onSetPushError}
      />
      <ActionItem
        icon={<GitPullRequestArrow className="h-[14px] w-[14px]" strokeWidth={1.75} />}
        label="创建拉取请求"
        onClick={onSetPrMode}
      />
      <ActionItem
        icon={<GitBranch className="h-[14px] w-[14px]" strokeWidth={1.75} />}
        label="创建分支"
        onClick={onSetCreateBranchMode}
      />
      <div className="h-1.5" />
    </>
  )
}

// ---------------------------------------------------------------------------
// Shared primitives
// ---------------------------------------------------------------------------

export function FormShell({
  title,
  onCancel,
  children,
}: {
  title: string
  onCancel: () => void
  children: React.ReactNode
}) {
  return (
    <div className="px-3.5 py-2.5">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11.5px] font-medium text-muted-foreground">{title}</span>
        <button
          type="button"
          onClick={onCancel}
          className="flex size-5 items-center justify-center rounded-full text-muted-foreground hover:bg-accent hover:text-accent-foreground"
          aria-label="取消"
        >
          <X className="h-3 w-3" />
        </button>
      </div>
      <div className="flex flex-col gap-2">{children}</div>
    </div>
  )
}

export function PrimaryButton({
  disabled,
  onClick,
  children,
}: {
  disabled?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="rounded-lg bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
    >
      {children}
    </button>
  )
}

// ---------------------------------------------------------------------------
// CommitForm
// ---------------------------------------------------------------------------

export function CommitForm({
  commitMessage,
  setCommitMessage,
  onSubmit,
  onCancel,
}: {
  commitMessage: string
  setCommitMessage: (v: string) => void
  onSubmit: () => void
  onCancel: () => void
}) {
  return (
    <FormShell title="提交" onCancel={onCancel}>
      <textarea
        autoFocus
        value={commitMessage}
        onChange={(e) => setCommitMessage(e.target.value)}
        placeholder="Commit message"
        rows={3}
        className="w-full resize-none rounded-lg border border-border/70 bg-muted px-3 py-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
      />
      <PrimaryButton disabled={!commitMessage.trim()} onClick={onSubmit}>
        提交
      </PrimaryButton>
    </FormShell>
  )
}

// ---------------------------------------------------------------------------
// CreateBranchForm
// ---------------------------------------------------------------------------

export function CreateBranchForm({
  branchName,
  setBranchName,
  onSubmit,
  onCancel,
}: {
  branchName: string
  setBranchName: (v: string) => void
  onSubmit: () => void
  onCancel: () => void
}) {
  return (
    <FormShell title="创建分支" onCancel={onCancel}>
      <input
        autoFocus
        value={branchName}
        onChange={(e) => setBranchName(e.target.value)}
        placeholder="新分支名"
        className="w-full rounded-lg border border-border/70 bg-muted px-3 py-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
      />
      <PrimaryButton disabled={!branchName.trim()} onClick={onSubmit}>
        创建并检出
      </PrimaryButton>
    </FormShell>
  )
}

// ---------------------------------------------------------------------------
// PrForm
// ---------------------------------------------------------------------------

export function PrForm({
  ghOk,
  prTitle,
  setPrTitle,
  prBody,
  setPrBody,
  onSubmit,
  onCancel,
}: {
  ghOk: boolean | null
  prTitle: string
  setPrTitle: (v: string) => void
  prBody: string
  setPrBody: (v: string) => void
  onSubmit: () => void
  onCancel: () => void
}) {
  return (
    <FormShell title="创建拉取请求" onCancel={onCancel}>
      {ghOk === false && <GhMissingBanner />}
      <input
        autoFocus
        value={prTitle}
        onChange={(e) => setPrTitle(e.target.value)}
        placeholder="PR 标题"
        className="w-full rounded-lg border border-border/70 bg-muted px-3 py-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
      />
      <textarea
        value={prBody}
        onChange={(e) => setPrBody(e.target.value)}
        placeholder="PR 描述（可选）"
        rows={3}
        className="w-full resize-none rounded-lg border border-border/70 bg-muted px-3 py-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
      />
      <PrimaryButton disabled={!prTitle.trim()} onClick={onSubmit}>
        {ghOk === false ? '生成草稿' : '提交并创建'}
      </PrimaryButton>
    </FormShell>
  )
}

// ---------------------------------------------------------------------------
// Transient-state views (busy / success / error)
// ---------------------------------------------------------------------------

export function BusyView({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-7 text-[13px] leading-6 text-muted-foreground">
      <Loader2 className="h-4 w-4 animate-spin" />
      {label}
    </div>
  )
}

export function SuccessView({ message, onClose }: { message: string; onClose: () => void }) {
  return (
    <div className="px-3.5 py-3.5">
      <div className="text-[13px] leading-6 text-emerald-700">{message}</div>
      <button
        type="button"
        onClick={onClose}
        className="mt-2.5 w-full rounded-lg bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90"
      >
        完成
      </button>
    </div>
  )
}

export function ErrorView({ message, onBack }: { message: string; onBack: () => void }) {
  return (
    <div className="px-3.5 py-3.5">
      <div className="text-[11.5px] leading-5 text-rose-600/85">
        {message}
      </div>
      <button
        type="button"
        onClick={onBack}
        className="mt-2.5 w-full rounded-lg border border-border/70 bg-muted px-3 py-1.5 text-[12px] font-medium text-foreground hover:bg-accent hover:text-accent-foreground"
      >
        返回
      </button>
    </div>
  )
}
