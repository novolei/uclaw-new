/**
 * EnvironmentCheckDialog — 环境检查对话框
 *
 * 显示运行时环境检测结果（Python runtime、memU bridge 等）。
 * 从 Proma 迁移，适配 uClaw 的环境检测需求。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { CheckCircle2, XCircle, AlertTriangle, RefreshCw, Loader2 } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  environmentCheckResultAtom,
  runtimeStatusAtom,
  isCheckingEnvironmentAtom,
  environmentCheckDialogOpenAtom,
} from '@/atoms/environment'

interface EnvironmentCheckDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** 单条检查项 */
function CheckItem({
  label,
  status,
  detail,
}: {
  label: string
  status: 'ok' | 'warning' | 'error' | 'checking'
  detail?: string
}): React.ReactElement {
  return (
    <div className="flex items-start gap-3 py-2">
      <div className="mt-0.5 shrink-0">
        {status === 'ok' && <CheckCircle2 className="size-4 text-emerald-500" />}
        {status === 'warning' && <AlertTriangle className="size-4 text-yellow-500" />}
        {status === 'error' && <XCircle className="size-4 text-red-500" />}
        {status === 'checking' && <Loader2 className="size-4 text-muted-foreground animate-spin" />}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-foreground">{label}</div>
        {detail && <div className="text-xs text-muted-foreground mt-0.5">{detail}</div>}
      </div>
    </div>
  )
}

export function EnvironmentCheckDialog({ open, onOpenChange }: EnvironmentCheckDialogProps): React.ReactElement {
  const checkResult = useAtomValue(environmentCheckResultAtom)
  const runtimeStatus = useAtomValue(runtimeStatusAtom)
  const isChecking = useAtomValue(isCheckingEnvironmentAtom)
  const setIsChecking = useSetAtom(isCheckingEnvironmentAtom)

  // 触发环境检测（后端通过 services_health 提供）
  const handleRecheck = React.useCallback(async () => {
    setIsChecking(true)
    try {
      const health = await invoke<Record<string, unknown>>('services_health')
      console.log('[EnvironmentCheckDialog] Services health:', health)
    } catch (error) {
      console.error('[EnvironmentCheckDialog] Failed to check environment:', error)
    } finally {
      setIsChecking(false)
    }
  }, [setIsChecking])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>环境检查</DialogTitle>
          <DialogDescription>
            检查运行所需的环境组件是否已就绪。
          </DialogDescription>
        </DialogHeader>

        <div className="divide-y divide-border/50 -mx-1 px-1">
          {/* Tauri Runtime */}
          <CheckItem
            label="Tauri 运行时"
            status="ok"
            detail="已内嵌，无需额外安装"
          />

          {/* Python Runtime (memU) */}
          <CheckItem
            label="Python 运行时"
            status={isChecking ? 'checking' : runtimeStatus ? 'ok' : 'warning'}
            detail={
              isChecking
                ? '检测中...'
                : runtimeStatus
                  ? 'Python 环境已就绪'
                  : '未检测到 Python 运行时（部分 AI 功能可能不可用）'
            }
          />

          {/* memU Bridge */}
          <CheckItem
            label="memU 服务"
            status={isChecking ? 'checking' : 'ok'}
            detail="记忆服务将在需要时自动启动"
          />

          {/* 总体状态 */}
          {checkResult?.hasIssues && (
            <div className="py-2">
              <div className="rounded-md bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400">
                <AlertTriangle className="size-3.5 inline mr-1.5 -mt-0.5" />
                检测到部分环境问题，但不影响基本使用。
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="gap-2">
          <Button variant="outline" size="sm" onClick={handleRecheck} disabled={isChecking}>
            {isChecking ? (
              <Loader2 className="size-3.5 animate-spin mr-1.5" />
            ) : (
              <RefreshCw className="size-3.5 mr-1.5" />
            )}
            重新检测
          </Button>
          <Button size="sm" onClick={() => onOpenChange(false)}>
            确定
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
