/**
 * WorkspacePanelHeader — top row of the files-rail workspace panel.
 *
 * [FolderHeart] 工作区文件 [Info ⓘ]            ··· [↻ refresh-all] [↗ Finder]
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { FolderHeart, Info, RotateCw, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  bumpFilesRailRefreshAtom,
  mountRootsAtomFamily,
  fileTreeAtomFamily,
} from '@/atoms/files-rail-atoms'

interface Props {
  sessionId: string | null
  workspaceRootPath: string | null
}

export function WorkspacePanelHeader({ sessionId, workspaceRootPath }: Props): React.ReactElement {
  const bumpRefresh = useSetAtom(bumpFilesRailRefreshAtom)
  const mounts = useAtomValue(mountRootsAtomFamily(sessionId))

  // Spinning while any mount is currently loading.
  // We can't useAtomValue inside a map (rules-of-hooks), so we render a
  // child component per mount and aggregate via React state.
  const [loadingCount, setLoadingCount] = React.useState(0)
  const anyLoading = loadingCount > 0

  const handleRefresh = React.useCallback(() => {
    bumpRefresh()
  }, [bumpRefresh])

  const handleReveal = React.useCallback(async () => {
    if (!workspaceRootPath) return
    try {
      await invoke('reveal_path_in_file_manager', { path: workspaceRootPath })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [workspaceRootPath])

  return (
    <header
      className={cn(
        'flex items-center gap-1.5 flex-shrink-0',
        'h-[36px] px-3',
        'border-b border-border bg-popover',
      )}
    >
      <FolderHeart className="size-3.5 text-muted-foreground shrink-0" />
      <span className="text-[12px] font-medium text-foreground/85">工作区文件</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label="工作区文件说明"
            className="inline-flex items-center justify-center size-4 text-muted-foreground/60 hover:text-muted-foreground transition-colors"
          >
            <Info className="size-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="max-w-[240px]">
          <p className="text-[11px]">工作区内所有会话可访问的文件和文件夹，每个新对话都可以自动读取</p>
        </TooltipContent>
      </Tooltip>
      <div className="flex-1" />
      {mounts.map((m) => (
        <MountLoadProbe
          key={m.id}
          mountId={m.id}
          onLoadingChange={(loading) =>
            setLoadingCount((prev) => prev + (loading ? 1 : -1))
          }
        />
      ))}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={handleRefresh}
            aria-label="刷新文件列表"
            className="inline-flex items-center justify-center size-6 rounded text-muted-foreground/70 hover:text-foreground hover:bg-foreground/[0.06] transition-colors"
          >
            <RotateCw className={cn('size-3.5', anyLoading && 'animate-spin')} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p className="text-[11px]">刷新所有挂载点</p>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={handleReveal}
            disabled={!workspaceRootPath}
            aria-label="在文件管理器中显示工作区"
            className={cn(
              'inline-flex items-center justify-center size-6 rounded transition-colors',
              workspaceRootPath
                ? 'text-muted-foreground/70 hover:text-foreground hover:bg-foreground/[0.06]'
                : 'text-foreground/25 cursor-not-allowed',
            )}
          >
            <ExternalLink className="size-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p className="text-[11px]">在文件管理器中显示工作区目录</p>
        </TooltipContent>
      </Tooltip>
    </header>
  )
}

/**
 * Subscribes to one mount's load state and bubbles changes upward via callback.
 * Avoids the hooks-rule violation of calling useAtomValue inside a map.
 */
function MountLoadProbe({
  mountId,
  onLoadingChange,
}: {
  mountId: string
  onLoadingChange: (loading: boolean) => void
}): null {
  const [tree] = useAtom(fileTreeAtomFamily(mountId))
  const loading = tree.status === 'loading'
  const prev = React.useRef(loading)
  React.useEffect(() => {
    if (prev.current !== loading) {
      onLoadingChange(loading)
      prev.current = loading
    }
  }, [loading, onLoadingChange])
  // Drain on unmount so the counter doesn't leak when sessionId changes.
  React.useEffect(() => {
    return () => {
      if (prev.current) onLoadingChange(false)
    }
  }, [onLoadingChange])
  return null
}
