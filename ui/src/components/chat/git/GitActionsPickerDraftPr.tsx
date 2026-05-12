/**
 * GitActionsPickerDraftPr.tsx
 *
 * Verbatim port of if2Ai's GitActionsPicker.tsx (717 LOC monolith) split
 * across 3 files for uClaw's 400 LOC cap. This file: PrDraftView +
 * GhMissingBanner sub-components + shellAnsiCQuote ANSI-C escape helper.
 *
 * See sibling files for the other halves of the split.
 */

import * as React from 'react'
import {
  AlertTriangle,
  Check,
  Copy,
  ExternalLink,
  X,
} from 'lucide-react'

const GH_INSTALL_URL = 'https://cli.github.com/'

/** Inline 警告条：gh CLI 不可用时挂在 PR/Issue 表单顶部，给安装链接。 */
export function GhMissingBanner() {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-2.5 py-2 text-[11.5px] leading-5 text-amber-800">
      <AlertTriangle className="mt-[2px] h-3.5 w-3.5 shrink-0" strokeWidth={2} />
      <div className="min-w-0 flex-1">
        <div className="font-medium">未检测到 gh CLI</div>
        <div className="text-amber-800/80">
          将以草稿形式生成命令，复制到终端执行；
          <a
            href={GH_INSTALL_URL}
            target="_blank"
            rel="noreferrer"
            className="ml-1 inline-flex items-center gap-0.5 underline underline-offset-2 hover:text-amber-900"
          >
            安装 gh
            <ExternalLink className="h-2.5 w-2.5" />
          </a>
        </div>
      </div>
    </div>
  )
}

/**
 * gh 不可用时的草稿视图。展示用户填写的 title + body，并把可执行的
 * `gh pr create` 命令拼出来；点 Copy 复制整段命令到剪贴板，用户即可
 * 在装好 gh 后直接粘贴执行。
 *
 * Body 经过 shell-escape 嵌入 `--body $'...'`：避免单引号 / 反斜杠
 * 截断命令；用 `$''` ANSI-C 引用形式，转义换行 / 单引号。
 */
export function PrDraftView({
  title,
  body,
  onBack,
}: {
  title: string
  body: string
  onBack: () => void
}) {
  const [copied, setCopied] = React.useState(false)

  const command = React.useMemo(() => {
    const escapedTitle = shellAnsiCQuote(title)
    const escapedBody = shellAnsiCQuote(body || '(no body provided)')
    return `gh pr create --title ${escapedTitle} --body ${escapedBody}`
  }, [title, body])

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1600)
    } catch {
      /* clipboard unavailable; surface no error — Copy button is best-effort */
    }
  }

  return (
    <div className="px-3.5 py-2.5">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11.5px] font-medium text-muted-foreground">PR 草稿</span>
        <button
          type="button"
          onClick={onBack}
          className="flex size-5 items-center justify-center rounded-full text-muted-foreground hover:bg-accent hover:text-accent-foreground"
          aria-label="返回"
        >
          <X className="h-3 w-3" />
        </button>
      </div>
      <GhMissingBanner />
      <div className="mt-2 space-y-1.5">
        <div className="text-[11px] uppercase tracking-wider text-muted-foreground">命令</div>
        <pre className="m-0 max-h-[160px] overflow-auto whitespace-pre-wrap break-all rounded-lg border border-border/70 bg-muted px-2.5 py-2 font-mono text-[11.5px] leading-5 text-foreground/80">
          {command}
        </pre>
        <button
          type="button"
          onClick={onCopy}
          className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90"
        >
          {copied ? (
            <>
              <Check className="h-3.5 w-3.5" />
              已复制
            </>
          ) : (
            <>
              <Copy className="h-3.5 w-3.5" />
              复制命令
            </>
          )}
        </button>
      </div>
    </div>
  )
}

/**
 * 把任意字符串编码成 bash ANSI-C ($'...') 引用形式：
 * `\` `'` 都转义；换行变成 `\n`，回车 `\r`，制表 `\t`。
 * 这样 `gh pr create --body $'...'` 在用户终端里粘贴后能保留多行
 * body 不被截断。
 */
export function shellAnsiCQuote(value: string): string {
  const escaped = value
    .replace(/\\/g, '\\\\')
    .replace(/'/g, "\\'")
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t')
  return `$'${escaped}'`
}
