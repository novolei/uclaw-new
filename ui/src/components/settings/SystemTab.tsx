import * as React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { ChevronDown, ChevronUp, RefreshCw, RotateCcw, Power } from 'lucide-react'
import { cn } from '@/lib/utils'
import { EmbeddingEndpointSection } from './EmbeddingEndpointSection'
import { DeveloperOptionsSection } from './DeveloperOptionsSection'

// ── Types (mirror Rust structs) ──────────────────────────────────────

type ServiceStatus =
  | { status: 'Stopped' }
  | { status: 'Starting' }
  | { status: 'Running' }
  | { status: 'Stopping' }
  | { status: 'Failed'; reason: string }

interface ServiceHealth {
  name: string
  status: ServiceStatus
  uptime_secs: number | null
  last_error: string | null
  metrics: Record<string, unknown>
}

interface MemUBridgeStatus {
  running: boolean
  pid: number | null
}

interface GbrainStatus {
  connected: boolean
  tool_count: number
  pgdata_ready: boolean
  error: string | null
}

// Sprint 2.2.5b — mirror of Rust's `mcp::GbrainInitStatus`. Discriminated
// union via serde's `tag = "status"`. The frontend pattern-matches on
// the `status` field to pick the right label + remediation hint.
type GbrainInitStatus =
  | { status: 'not_attempted' }
  | { status: 'in_progress' }
  | { status: 'succeeded'; duration_ms: number; at_ms: number }
  | { status: 'skipped_already_initialized'; at_ms: number }
  | { status: 'failed'; error: string; stderr_tail: string | null; at_ms: number }
  | { status: 'bundle_missing' }

interface SystemDiagnosticsReport {
  app_version: string
  platform: string
  arch: string
  memory_used_mb: number
  memory_total_mb: number
  uptime_secs: number
  consecutive_failures: number
  recovery_attempts: number
  active_processes: number
  orphan_processes: number
  services: ServiceHealth[]
  memu: MemUBridgeStatus
  gbrain: GbrainStatus
  gbrain_init: GbrainInitStatus
}

// ── Helpers ──────────────────────────────────────────────────────────

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}

function formatMemory(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`
}

function serviceStatusLabel(s: ServiceStatus): string {
  const map: Record<string, string> = {
    Running: '运行中', Stopped: '未启动',
    Starting: '启动中', Stopping: '停止中',
  }
  if (s.status === 'Failed') return `失败: ${(s as { status: 'Failed'; reason: string }).reason.slice(0, 40)}`
  return map[s.status] ?? s.status
}

function serviceStatusDot(s: ServiceStatus): string {
  if (s.status === 'Running') return 'bg-green-500'
  if (s.status === 'Stopped' || s.status === 'Stopping') return 'bg-muted-foreground/40'
  if (s.status === 'Failed') return 'bg-red-500'
  return 'bg-yellow-400' // Starting
}

// ── Main component ───────────────────────────────────────────────────

export function SystemTab() {
  const [report, setReport] = React.useState<SystemDiagnosticsReport | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [lastChecked, setLastChecked] = React.useState<Date | null>(null)
  const [healthExpanded, setHealthExpanded] = React.useState(false)
  const [busyMemu, setBusyMemu] = React.useState(false)
  const [busyGbrain, setBusyGbrain] = React.useState(false)
  const [busyReset, setBusyReset] = React.useState(false)
  const [busyRestart, setBusyRestart] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)

  const runDiagnostics = React.useCallback(async () => {
    setLoading(true)
    setActionError(null)
    try {
      const r = await invoke<SystemDiagnosticsReport>('get_system_diagnostics')
      setReport(r)
      setLastChecked(new Date())
    } catch (e) {
      setActionError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  const isHealthy = report
    ? report.consecutive_failures === 0
      && !report.services.some(s => s.status.status === 'Failed')
      && report.memu.running
      && report.gbrain.connected
    : true

  const failedServices = report?.services.filter(s => s.status.status === 'Failed') ?? []

  async function handleBridgeAction(
    command: string,
    setBusy: (v: boolean) => void,
  ) {
    setBusy(true)
    setActionError(null)
    try {
      await invoke(command)
      await runDiagnostics()
    } catch (e) {
      setActionError(String(e))
    } finally {
      setBusy(false)
    }
  }

  function handleCopyReport() {
    if (!report) return
    navigator.clipboard.writeText(JSON.stringify(report, null, 2))
  }

  function handleExportReport() {
    if (!report) return
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `uclaw-diagnostics-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
  }

  return (
    <div className="flex flex-col gap-4 p-4 max-w-2xl">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h2 className="text-base font-semibold text-foreground">系统诊断</h2>
          <p className="text-xs text-muted-foreground mt-0.5">检查系统健康状态并修复问题</p>
        </div>
        <button
          onClick={runDiagnostics}
          disabled={loading}
          className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg bg-accent text-accent-foreground hover:bg-accent/80 disabled:opacity-50 transition-colors"
        >
          <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
          运行诊断
        </button>
      </div>

      {actionError && (
        <div className="text-xs text-red-400 bg-red-400/10 rounded-lg px-3 py-2">
          {actionError}
        </div>
      )}

      {/* 系统健康 collapsible card */}
      {report && (
        <div
          className={cn(
            'rounded-xl border px-4 py-3 cursor-pointer select-none',
            isHealthy
              ? 'bg-green-500/10 border-green-500/20'
              : 'bg-red-500/10 border-red-500/20',
          )}
          onClick={() => setHealthExpanded(v => !v)}
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={cn('text-sm font-medium', isHealthy ? 'text-green-400' : 'text-red-400')}>
                {isHealthy ? '✓ 系统健康' : '✗ 发现问题'}
              </span>
              {lastChecked && (
                <span className="text-xs text-muted-foreground">
                  上次检查: {lastChecked.toLocaleString('zh-CN')}
                </span>
              )}
            </div>
            {healthExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
          </div>
          {healthExpanded && (failedServices.length > 0 || !report.memu.running || !report.gbrain.connected) && (
            <ul className="mt-2 text-xs text-red-400 space-y-0.5">
              {failedServices.map(s => (
                <li key={s.name}>• {s.name}: {serviceStatusLabel(s.status)}</li>
              ))}
              {!report.memu.running && <li>• memU: Python Bridge 未运行或健康检查失败</li>}
              {!report.gbrain.connected && (
                <li>• gbrain: MCP 未连接{report.gbrain.error ? ` — ${report.gbrain.error.slice(0, 140)}` : ''}</li>
              )}
            </ul>
          )}
        </div>
      )}

      {report && (
        <>
          {/* 系统信息 */}
          <Section title="系统信息">
            <Grid4>
              <InfoCell label="版本" value={report.app_version} />
              <InfoCell label="平台" value={`${report.platform} (${report.arch})`} />
              <InfoCell label="内存" value={`${formatMemory(report.memory_used_mb)} / ${formatMemory(report.memory_total_mb)}`} />
              <InfoCell label="运行时间" value={formatUptime(report.uptime_secs)} />
            </Grid4>
          </Section>

          {/* 健康指标 */}
          <Section title="健康指标">
            <Grid4>
              <InfoCell label="连续失败次数" value={String(report.consecutive_failures)} />
              <InfoCell label="恢复尝试次数" value={String(report.recovery_attempts)} />
              <InfoCell label="活跃进程" value={String(report.active_processes)} />
              <InfoCell label="发现孤儿进程" value={String(report.orphan_processes)} />
            </Grid4>
          </Section>

          {/* 桥接服务 */}
          <Section title="桥接服务">
            <div className="flex flex-col gap-2">
              <BridgeCard
                name="memU"
                subtitle="Python Bridge"
                running={report.memu.running}
                detail={report.memu.running
                  ? (report.memu.pid ? `PID ${report.memu.pid}` : '运行中')
                  : '未运行'}
              />
              <BridgeCard
                name="gbrain"
                subtitle="Bun MCP"
                running={report.gbrain.connected}
                detail={report.gbrain.connected
                  ? `${report.gbrain.tool_count} 工具 · PGlite pgdata ${report.gbrain.pgdata_ready ? '已就绪' : '未就绪'}`
                  : (report.gbrain.error ? `未连接: ${report.gbrain.error.slice(0, 90)}` : '未连接')}
              />
            </div>
            {/* Sprint 2.2.5b — init status row.
                Only render when init was attempted (not_attempted = boot
                pre-Stage-3, no useful signal). Failed shows actionable
                hint pointing at scripts/init-gbrain.sh. */}
            {report.gbrain_init.status !== 'not_attempted' && (
              <GbrainInitRow status={report.gbrain_init} />
            )}
          </Section>

          {/* 服务状态 */}
          <Section title="服务状态">
            <div className="flex flex-col divide-y divide-border/50">
              {report.services.map(svc => (
                <div key={svc.name} className="flex items-center justify-between py-2">
                  <div className="flex items-center gap-2">
                    <span className={cn('size-2 rounded-full flex-shrink-0', serviceStatusDot(svc.status))} />
                    <span className="text-sm text-foreground">{svc.name}</span>
                  </div>
                  <span className="text-xs text-muted-foreground">{serviceStatusLabel(svc.status)}</span>
                </div>
              ))}
            </div>
          </Section>

          {/* 恢复操作 */}
          <Section title="恢复操作">
            <div className="flex flex-col gap-2">
              <div className="flex gap-2">
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重置 AI 引擎"
                  busy={busyReset}
                  variant="warm"
                  onClick={() => handleBridgeAction('reset_ai_engine', setBusyReset)}
                />
                <ActionButton
                  icon={<Power size={13} />}
                  label="重启应用"
                  busy={busyRestart}
                  variant="danger"
                  onClick={() => handleBridgeAction('restart_app', setBusyRestart)}
                />
              </div>
              <div className="flex gap-2">
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重启 memU"
                  busy={busyMemu}
                  variant="bridge"
                  onClick={() => handleBridgeAction('restart_memu_bridge', setBusyMemu)}
                />
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重启 gbrain"
                  busy={busyGbrain}
                  variant="bridge"
                  onClick={() => handleBridgeAction('restart_gbrain_mcp', setBusyGbrain)}
                />
              </div>
            </div>
          </Section>
        </>
      )}

      {/* Sprint 2.2 followon #4 — embedding endpoint configuration */}
      <EmbeddingEndpointSection />

      {/* Sprint 2.2 followon #4 — developer options (collapsed by default) */}
      <DeveloperOptionsSection />

      {/* Footer */}
      {report && (
        <div className="flex gap-4 pt-1 border-t border-border/50">
          <button
            onClick={handleCopyReport}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            复制报告
          </button>
          <button
            onClick={handleExportReport}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            导出报告
          </button>
        </div>
      )}

      {!report && !loading && (
        <p className="text-sm text-muted-foreground text-center py-8">
          点击「运行诊断」开始检查系统状态
        </p>
      )}
    </div>
  )
}

// ── Sub-components ───────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">{title}</p>
      {children}
    </div>
  )
}

function Grid4({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-x-8 gap-y-2">{children}</div>
}

function InfoCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1.5 border-b border-border/40">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs text-foreground font-mono">{value}</span>
    </div>
  )
}

function BridgeCard({ name, subtitle, running, detail }: {
  name: string; subtitle: string; running: boolean; detail: string
}) {
  return (
    <div className="rounded-lg bg-muted/40 px-3 py-2.5 flex items-center justify-between">
      <div className="flex items-center gap-2">
        <span className={cn('size-2 rounded-full flex-shrink-0', running ? 'bg-green-500' : 'bg-muted-foreground/40')} />
        <span className="text-sm font-medium text-foreground">{name}</span>
        <span className="text-xs text-muted-foreground">({subtitle})</span>
      </div>
      <span className={cn('text-xs', running ? 'text-green-400' : 'text-muted-foreground')}>{detail}</span>
    </div>
  )
}

// Sprint 2.2.5b — surface the gbrain init outcome with actionable copy.
// Each status branch picks an appropriate dot color + 1-line message +
// optional remediation hint.
function GbrainInitRow({ status }: { status:
  | { status: 'not_attempted' }
  | { status: 'in_progress' }
  | { status: 'succeeded'; duration_ms: number; at_ms: number }
  | { status: 'skipped_already_initialized'; at_ms: number }
  | { status: 'failed'; error: string; stderr_tail: string | null; at_ms: number }
  | { status: 'bundle_missing' }
}) {
  let dotClass = 'bg-muted-foreground/40'
  let label = '初始化未尝试'
  let detail = ''
  let hint: string | null = null

  switch (status.status) {
    case 'in_progress':
      dotClass = 'bg-yellow-400 animate-pulse'
      label = '初始化进行中'
      detail = '首次启动 — PGlite 正在跑 ~63 次迁移 (30-60s)'
      break
    case 'succeeded':
      dotClass = 'bg-green-500'
      label = '初始化成功'
      detail = `首次初始化耗时 ${(status.duration_ms / 1000).toFixed(1)}s`
      break
    case 'skipped_already_initialized':
      dotClass = 'bg-green-500'
      label = '已初始化'
      detail = 'PGlite 数据库已就绪'
      break
    case 'failed':
      dotClass = 'bg-red-500'
      label = '初始化失败'
      detail = status.error
      hint = '运行 scripts/init-gbrain.sh 或删除 ~/.uclaw/gbrain/ 后重启'
      break
    case 'bundle_missing':
      dotClass = 'bg-red-500'
      label = 'bundle 缺失'
      detail = 'bunembed/bun 或 gbrain-source 未找到'
      hint = '运行 scripts/setup-bun-runtime.sh + scripts/setup-gbrain-source.sh'
      break
    case 'not_attempted':
      // Caller filters this out, but TS demands exhaustive match.
      break
  }

  return (
    <div className="mt-2 rounded-lg bg-muted/30 px-3 py-2 text-xs">
      <div className="flex items-center gap-2">
        <span className={cn('size-2 rounded-full flex-shrink-0', dotClass)} />
        <span className="font-medium text-foreground">gbrain init</span>
        <span className="text-muted-foreground">— {label}</span>
      </div>
      {detail && (
        <div className="mt-1 pl-4 text-muted-foreground">{detail}</div>
      )}
      {hint && (
        <div className="mt-1 pl-4 text-amber-400">{hint}</div>
      )}
    </div>
  )
}

function ActionButton({ icon, label, busy, variant, onClick }: {
  icon: React.ReactNode; label: string; busy: boolean
  variant: 'warm' | 'danger' | 'bridge'; onClick: () => void
}) {
  const cls = {
    warm: 'bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 border border-amber-500/20',
    danger: 'bg-red-500/10 text-red-400 hover:bg-red-500/20 border border-red-500/20',
    bridge: 'bg-green-500/10 text-green-400 hover:bg-green-500/20 border border-green-500/20',
  }[variant]

  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={cn(
        'flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
        cls,
      )}
    >
      {busy ? <RefreshCw size={12} className="animate-spin" /> : icon}
      {label}
    </button>
  )
}
