/**
 * skill_marketplace_search 结果卡片：按热度排序的候选技能，每行一个「安装」按钮，
 * 下拉选择安装到全局 / 本工作区。安装成功后递增 skillsRefreshAtom，技能列表随之刷新。
 * 结果不是合法 JSON（或工具报错）时退回默认渲染，原始输出不丢。
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { AlertCircle, Check, ChevronDown, Download, FolderPlus, Globe, Loader2, PackageSearch } from 'lucide-react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { skillsRefreshAtom } from '@/atoms/agent-atoms'
import { installSkillFromMarketplace } from '@/lib/tauri-bridge'
import type { MarketplaceInstallScope, MarketplaceProvider } from '@/lib/types'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { DefaultResultRenderer } from './default-result'

interface SkillRow {
  id: string
  name: string
  source: string
  installs?: number
  installUrl?: string
  description?: string
}

interface SearchOutput {
  results?: SkillRow[]
  provider?: MarketplaceProvider
}

type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: MarketplaceInstallScope }
  | { kind: 'installed'; scope: MarketplaceInstallScope }
  | { kind: 'error'; message: string }

const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 })

function parse(result: string): SearchOutput | null {
  try {
    const v = JSON.parse(result) as unknown
    return v && typeof v === 'object' ? (v as SearchOutput) : null
  } catch {
    return null
  }
}

export function SkillMarketplaceSearchResult({
  result,
  isError,
}: {
  result: string
  isError: boolean
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const bumpSkills = useSetAtom(skillsRefreshAtom)
  const [states, setStates] = React.useState<Record<string, RowState>>({})
  const parsed = React.useMemo(() => parse(result), [result])

  if (isError || !parsed) {
    return <DefaultResultRenderer toolName="skill_marketplace_search" input={{}} result={result} isError={isError} />
  }

  const provider: MarketplaceProvider = parsed.provider ?? 'skills_sh'
  const rows = [...(parsed.results ?? [])].sort((a, b) => (b.installs ?? 0) - (a.installs ?? 0))
  if (rows.length === 0) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground">
        <PackageSearch className="size-3.5 shrink-0" aria-hidden />
        没有找到匹配的技能，换个关键词试试。
      </div>
    )
  }

  const install = async (rowKey: string, row: SkillRow, scope: MarketplaceInstallScope) => {
    setStates((s) => ({ ...s, [rowKey]: { kind: 'installing', scope } }))
    try {
      await installSkillFromMarketplace(
        row.id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
        provider,
        row.installUrl,
      )
      setStates((s) => ({ ...s, [rowKey]: { kind: 'installed', scope } }))
      bumpSkills((n) => n + 1)
    } catch (e) {
      setStates((s) => ({ ...s, [rowKey]: { kind: 'error', message: String(e) } }))
    }
  }

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row, i) => {
        const rowKey = `${row.id}__${i}`
        const st: RowState = states[rowKey] ?? { kind: 'idle' }
        const busy = st.kind === 'installing'
        return (
          <div
            key={rowKey}
            className="flex items-start gap-3 rounded-lg border border-border bg-card/40 px-3 py-2.5 transition-colors hover:bg-muted/40"
          >
            <span
              className={cn(
                'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md text-[11px] font-semibold tabular-nums',
                i === 0
                  ? 'bg-emerald-500/15 text-emerald-600 ring-1 ring-inset ring-emerald-500/30 dark:text-emerald-400'
                  : 'bg-muted text-muted-foreground',
              )}
              aria-label={`排名第 ${i + 1}`}
            >
              {i + 1}
            </span>
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium text-foreground" title={row.name} data-testid="marketplace-row-name">
                {row.name}
              </div>
              <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                <span className="truncate" title={row.source}>
                  {row.source}
                </span>
                {typeof row.installs === 'number' && row.installs > 0 && (
                  <span
                    className="inline-flex shrink-0 items-center gap-1 rounded bg-muted px-1.5 py-0.5 font-medium tabular-nums"
                    title={`${row.installs.toLocaleString()} ${provider === 'skillsmp' ? '星' : '次安装'}`}
                  >
                    <Download className="size-3" aria-hidden />
                    {compact.format(row.installs)}
                  </span>
                )}
              </div>
              {row.description && (
                <div className="mt-1 line-clamp-2 text-xs leading-relaxed text-muted-foreground/80">{row.description}</div>
              )}
              {st.kind === 'error' && (
                <div className="mt-1 flex items-start gap-1.5 break-words text-xs text-destructive">
                  <AlertCircle className="mt-px size-3 shrink-0" aria-hidden />
                  <span>{st.message}</span>
                </div>
              )}
            </div>
            {st.kind === 'installed' ? (
              <span className="mt-0.5 inline-flex shrink-0 items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                <Check className="size-3" aria-hidden />
                已安装（{st.scope === 'global' ? '全局' : '本工作区'}）
              </span>
            ) : (
              <DropdownMenu>
                <DropdownMenuTrigger
                  disabled={busy}
                  className={cn(
                    'mt-0.5 inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
                    'bg-emerald-500/90 text-white hover:bg-emerald-500',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/50',
                    'disabled:cursor-default disabled:opacity-60',
                  )}
                  aria-label={`安装 ${row.name}`}
                >
                  {busy ? <Loader2 className="size-3 animate-spin" aria-hidden /> : <Download className="size-3" aria-hidden />}
                  安装
                  {!busy && <ChevronDown className="size-3 opacity-80" aria-hidden />}
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-[10rem]">
                  <DropdownMenuItem onSelect={() => void install(rowKey, row, 'global')} className="cursor-pointer">
                    <Globe className="text-muted-foreground" aria-hidden />
                    安装到全局
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={!activeWorkspaceId}
                    onSelect={() => void install(rowKey, row, 'workspace')}
                    className="cursor-pointer"
                  >
                    <FolderPlus className="text-muted-foreground" aria-hidden />
                    <span className="flex flex-col">
                      安装到本工作区
                      {!activeWorkspaceId && <span className="text-[10px] text-muted-foreground/70">没有选中的工作区</span>}
                    </span>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>
        )
      })}
    </div>
  )
}
