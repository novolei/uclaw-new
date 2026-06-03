import * as React from 'react'
import { useAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Loader2, Check, AlertCircle, X } from 'lucide-react'
import { listen } from '@tauri-apps/api/event'
import {
  localModelEnvCheck, localModelProbeSources, localModelDownload, localModelCancel,
  localModelWarmup, localModelSmokeTest, setRoleModel, setOnboardingState,
  type MiniCpmDownloadProgress,
} from '@/lib/tauri-bridge'
import { minicpmWizardAtom, INITIAL_WIZARD } from '@/atoms/minicpm-wizard'

export function MiniCPMWizard(): React.ReactElement | null {
  const [s, set] = useAtom(minicpmWizardAtom)

  // Subscribe to download progress while on the download step.
  // Cancelled-flag guards against the component unmounting before listen() resolves.
  React.useEffect(() => {
    if (s.step !== 'download') return
    let cancelled = false
    let unlisten: (() => void) | undefined
    listen<MiniCpmDownloadProgress>('minicpm://download-progress', (e) => {
      set((p) => ({ ...p, progress: e.payload }))
    }).then((fn) => { if (cancelled) fn(); else unlisten = fn })
    return () => { cancelled = true; unlisten?.() }
  }, [s.step, set])

  if (s.step === null) return null

  const close = () => set(INITIAL_WIZARD)
  const fail = (msg: string) => set((p) => ({ ...p, step: 'error', error: msg }))

  const runEnvCheck = async () => {
    set((p) => ({ ...p, step: 'envcheck' }))
    try {
      const env = await localModelEnvCheck()
      set((p) => ({ ...p, env }))
    } catch (e) { fail(String(e)) }
  }

  const runProbe = async () => {
    set((p) => ({ ...p, step: 'source' }))
    try {
      const sources = await localModelProbeSources()
      set((p) => ({ ...p, sources }))
    } catch (e) { fail(String(e)) }
  }

  const runDownload = async () => {
    set((p) => ({ ...p, step: 'download', progress: null }))
    try {
      await localModelDownload({ source: s.chosenSource ?? undefined })
      await runWarmup()
    } catch (e) { fail(String(e)) }
  }

  const runWarmup = async () => {
    set((p) => ({ ...p, step: 'warmup' }))
    try { await localModelWarmup(); await runSmoke() }
    catch (e) { fail(String(e)) }
  }

  const runSmoke = async () => {
    set((p) => ({ ...p, step: 'smoketest' }))
    try {
      const out = await localModelSmokeTest('你好')
      set((p) => ({ ...p, smokeOutput: out, step: 'done' }))
      await finish()
    } catch (e) { fail(String(e)) }
  }

  // done: auto-wire roles + persist completion.
  // Role wiring is best-effort per-call — user can re-pick in 模型分配 if it fails.
  // setOnboardingState MUST run regardless, else the wizard re-prompts every launch.
  const finish = async () => {
    try { await setRoleModel('utility', 'local/minicpm5-1b') } catch { /* surfaced in 模型分配 */ }
    try { await setRoleModel('summarizer', 'local/minicpm5-1b') } catch { /* surfaced in 模型分配 */ }
    try { await setOnboardingState('completed') } catch { /* non-fatal; gate re-prompts next launch */ }
  }

  const cancel = async () => { try { await localModelCancel() } catch { /* ignore */ }; close() }

  return (
    <AnimatePresence>
      <motion.div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
        data-testid="minicpm-wizard"
      >
        <div className="w-[480px] rounded-lg bg-background p-6 shadow-xl space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-base font-semibold">本地模型设置</h2>
            <button type="button" onClick={close} aria-label="关闭"><X size={16} /></button>
          </div>

          {s.step === 'intro' && (
            <div className="space-y-3 text-sm">
              <p>安装本地 MiniCPM 模型可在「轻工具 / 记忆摘要」场景省 token、保护隐私、离线可用。约 688 MB。</p>
              <div className="flex gap-2">
                <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runEnvCheck}>现在设置</button>
                <button className="rounded-md border px-3 py-1.5" onClick={async () => { await setOnboardingState('deferred'); close() }}>稍后</button>
                <button className="rounded-md border px-3 py-1.5 text-muted-foreground" onClick={async () => { await setOnboardingState('skipped'); close() }}>不再提示</button>
              </div>
            </div>
          )}

          {s.step === 'envcheck' && (
            <div className="space-y-3 text-sm">
              {!s.env ? <Loader2 className="animate-spin" size={18} /> : (
                <>
                  <div className="text-xs space-y-1">
                    <div>系统：{s.env.os} / {s.env.arch} · {s.env.cpu_cores} 核 · Metal {s.env.metal_available ? '可用' : '不可用'}</div>
                    <div>内存：{Math.round(s.env.total_ram / 1e9)} GB · 磁盘剩余：{Math.round(s.env.free_disk / 1e9)} GB</div>
                    <div>推荐量化：{s.env.recommended_quant}</div>
                  </div>
                  {s.env.warnings.map((w, i) => (
                    <div key={i} className="flex items-center gap-1 text-amber-600 text-xs"><AlertCircle size={12} /> {w}</div>
                  ))}
                  <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runProbe}>继续 →</button>
                </>
              )}
            </div>
          )}

          {s.step === 'source' && (
            <div className="space-y-3 text-sm">
              {!s.sources ? <Loader2 className="animate-spin" size={18} /> : (
                <>
                  <div className="text-xs">下载源（已按延迟排序）：</div>
                  {s.sources.map((src) => (
                    <label key={src.host} className="flex items-center gap-2 text-xs">
                      <input type="radio" name="src" checked={s.chosenSource === src.host}
                        onChange={() => set((p) => ({ ...p, chosenSource: src.host }))} />
                      {src.host} · {src.reachable ? `${src.latency_ms ?? '?'} ms` : '不可达'}
                    </label>
                  ))}
                  <label className="flex items-center gap-2 text-xs">
                    <input type="radio" name="src" checked={s.chosenSource === null}
                      onChange={() => set((p) => ({ ...p, chosenSource: null }))} />
                    自动（最快）
                  </label>
                  <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runDownload}>下载 →</button>
                </>
              )}
            </div>
          )}

          {s.step === 'download' && (
            <div className="space-y-3 text-sm">
              <div className="text-xs">{s.progress ? `${s.progress.file} · ${s.progress.source} · ${Math.round((s.progress.downloaded) / 1e6)} MB${s.progress.total ? ` / ${Math.round(s.progress.total / 1e6)} MB` : ''} · ${s.progress.phase}` : '准备下载…'}</div>
              <div className="h-2 w-full overflow-hidden rounded bg-muted">
                <div className="h-full bg-primary transition-all"
                  style={{ width: s.progress?.total ? `${Math.min(100, (s.progress.downloaded / s.progress.total) * 100)}%` : '15%' }} />
              </div>
              <button className="rounded-md border px-3 py-1.5" onClick={cancel}>取消</button>
            </div>
          )}

          {s.step === 'warmup' && <div className="flex items-center gap-2 text-sm"><Loader2 className="animate-spin" size={16} /> 正在预热模型…</div>}

          {s.step === 'smoketest' && <div className="flex items-center gap-2 text-sm"><Loader2 className="animate-spin" size={16} /> 正在测试生成…</div>}

          {s.step === 'done' && (
            <div className="space-y-3 text-sm">
              <div className="flex items-center gap-2 text-green-600"><Check size={16} /> 完成！已把「轻工具 / 记忆摘要」接到本地模型。</div>
              {s.smokeOutput && <div className="rounded bg-muted p-2 text-xs">模型输出：{s.smokeOutput}</div>}
              <div className="text-xs text-muted-foreground">可随时在「设置 → 智能 → 模型分配」中更改。</div>
              <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={close}>知道了</button>
            </div>
          )}

          {s.step === 'error' && (
            <div className="space-y-3 text-sm">
              <div className="flex items-center gap-2 text-red-600"><AlertCircle size={16} /> 出错了</div>
              <div className="rounded bg-muted p-2 text-xs">{s.error}</div>
              <div className="flex gap-2">
                <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runEnvCheck}>重试</button>
                <button className="rounded-md border px-3 py-1.5" onClick={close}>关闭（稍后再试，云端继续可用）</button>
              </div>
            </div>
          )}
        </div>
      </motion.div>
    </AnimatePresence>
  )
}
