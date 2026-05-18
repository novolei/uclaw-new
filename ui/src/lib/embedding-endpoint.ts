import { invoke } from '@tauri-apps/api/core'

// ─── Embedding endpoint config (mirrors Rust EmbeddingEndpointPayload) ────

export interface EmbeddingEndpointConfig {
  base_url: string
  model: string
  dimensions: number
  fastembed_model: string
}

export async function getEmbeddingConfig(): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('get_embedding_config')
}

export async function setEmbeddingConfig(
  payload: EmbeddingEndpointConfig,
): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('set_embedding_config', { payload })
}

// ─── Setup-script runner (mirrors Rust allowlist) ─────────────────────────

/**
 * Hardcoded mirror of SETUP_SCRIPT_ALLOWLIST in Rust. Names must match
 * exactly — backend rejects anything outside this set. Adding a script
 * is a coordinated code change in both repos.
 */
export const SETUP_SCRIPTS = [
  'setup-bun-runtime',
  'setup-gbrain-source',
  'setup-python-env',
  'init-gbrain',
] as const

export type SetupScriptName = (typeof SETUP_SCRIPTS)[number]

export interface SetupScriptDescriptor {
  name: SetupScriptName
  /** Display label, Chinese — matches the rest of the settings UI. */
  label: string
  /** One-line description below the label. */
  description: string
  /** When true the script has a destructive --force mode; UI surfaces a confirm gate. */
  supportsForce: boolean
  /** Approximate wall-time so the progress bar can be calibrated. */
  expectedDurationSecs: number
}

export const SETUP_SCRIPT_DESCRIPTORS: Record<SetupScriptName, SetupScriptDescriptor> = {
  'setup-bun-runtime': {
    name: 'setup-bun-runtime',
    label: '安装 Bun 运行时',
    description: '下载 Bun 静态二进制 (~50MB) 到 src-tauri/bunembed/。首次 setup 或升级 Bun 时使用。',
    supportsForce: false,
    expectedDurationSecs: 30,
  },
  'setup-gbrain-source': {
    name: 'setup-gbrain-source',
    label: '安装 gbrain 源码',
    description: 'Clone gbrain 源码到 src-tauri/gbrain-source/ + bun install --production。首次 setup 或换 gbrain 版本时使用。',
    supportsForce: false,
    expectedDurationSecs: 90,
  },
  'setup-python-env': {
    name: 'setup-python-env',
    label: '安装 Python 环境 (memU)',
    description: '装 embedded Python + memU + fastembed 等依赖到 src-tauri/pyembed/。首次 setup 或 memU 依赖损坏时使用。',
    supportsForce: false,
    expectedDurationSecs: 120,
  },
  'init-gbrain': {
    name: 'init-gbrain',
    label: '初始化 gbrain brain',
    description: '在 ~/.uclaw/gbrain/.gbrain/brain.pglite/ 跑 PGLite migrations。--force 会先 rm -rf 现有 brain 再 init。',
    supportsForce: true,
    expectedDurationSecs: 60,
  },
}

export interface SetupScriptRunResult {
  run_id: string
  exit_code: number | null
  success: boolean
}

export async function runSetupScript(
  name: SetupScriptName,
  opts: { force?: boolean } = {},
): Promise<SetupScriptRunResult> {
  return await invoke<SetupScriptRunResult>('run_setup_script', {
    args: {
      script_name: name,
      force: opts.force ?? false,
    },
  })
}

// ─── Tauri event payloads ────────────────────────────────────────────────

export interface SetupScriptOutputEvent {
  run_id: string
  stream: 'stdout' | 'stderr'
  line: string
}

export interface SetupScriptEndEvent {
  run_id: string
  exit_code: number | null
  success: boolean
}
