/**
 * Provider / Model logo resolution.
 *
 * Static map built at bundle time from `src/assets/provider-logos/provider_logo_*.png`
 * (21 PNG files at last count). All assistant avatars / model chips funnel
 * through `getModelLogo()` so a single source of truth controls the visual.
 *
 * Resolution chain:
 *   1. Explicit `provider` argument (caller already knows — most accurate)
 *   2. `inferProviderFromModelId()` — regex on the model id
 *   3. Empty string fallback (caller renders a Bot icon)
 *
 * Channel logo path goes through `inferProviderFromBaseUrl()` which matches
 * common API host substrings.
 */

import type { Channel } from '@/lib/chat-types'

// Vite eager glob — every PNG under provider-logos/ becomes a hashed URL at
// build time. We keep the naming convention `provider_logo_<slug>.png` so
// the map is self-documenting; the legacy Deepseek-logo-icon.svg.png stays
// in the folder for design history but isn't picked up by the glob.
const logoModules = import.meta.glob<string>(
  '@/assets/provider-logos/provider_logo_*.png',
  { eager: true, import: 'default' },
)

/** provider slug → bundled image URL */
const providerLogoMap: Record<string, string> = (() => {
  const out: Record<string, string> = {}
  for (const [path, url] of Object.entries(logoModules)) {
    const match = path.match(/provider_logo_([a-z0-9]+)\.png$/i)
    if (match && match[1]) out[match[1].toLowerCase()] = url
  }
  return out
})()

/**
 * Infer provider slug from a model id by matching common prefix patterns.
 * Returns `undefined` if the id doesn't match any known family — caller
 * decides whether to render a fallback icon.
 */
export function inferProviderFromModelId(modelId: string): string | undefined {
  if (!modelId) return undefined
  const id = modelId.toLowerCase()

  // Anthropic — `claude-*`, `claude_*`
  if (id.startsWith('claude')) return 'anthropic'

  // OpenAI — `gpt-*`, `o1`/`o3`/`o4` reasoning, `chatgpt-*`, embeddings
  if (
    id.startsWith('gpt-') ||
    id.startsWith('gpt4') ||
    id.startsWith('gpt3') ||
    /^o[134](-|$)/.test(id) ||
    id.startsWith('chatgpt') ||
    id.startsWith('text-embedding') ||
    id.startsWith('dall-e') ||
    id.startsWith('whisper') ||
    id.startsWith('tts-')
  ) return 'openai'

  // Google — Gemini / PaLM
  if (id.startsWith('gemini') || id.startsWith('palm') || id.startsWith('bison')) return 'google'

  // DeepSeek
  if (id.startsWith('deepseek')) return 'deepseek'

  // Alibaba Cloud — Qwen family
  if (id.startsWith('qwen') || id.startsWith('aliyun') || id.startsWith('tongyi')) return 'aliyun'

  // Mistral
  if (id.startsWith('mistral') || id.startsWith('mixtral') || id.startsWith('codestral') || id.startsWith('ministral')) {
    return 'mistral'
  }

  // xAI — Grok
  if (id.startsWith('grok')) return 'xai'

  // Moonshot — Kimi
  if (id.startsWith('moonshot') || id.startsWith('kimi')) return 'moonshot'

  // Zhipu / Z.ai — GLM / ChatGLM / CogView
  if (id.startsWith('glm') || id.startsWith('chatglm') || id.startsWith('cogview') || id.startsWith('cogvideo')) {
    return 'zai'
  }

  // MiniMax — abab series + named MiniMax-* SKUs
  if (id.startsWith('minimax') || id.startsWith('abab')) return 'minimax'

  // ByteDance — Doubao on Volcengine / ByteDance Ark
  if (id.startsWith('doubao')) return 'volcengine'
  if (id.startsWith('byteplus') || id.startsWith('skylark')) return 'byteplus'

  // Baidu — Ernie / Wenxin
  if (id.startsWith('ernie') || id.startsWith('wenxin')) return 'baidu'

  // Xiaomi — Mi-* models
  if (id.startsWith('mi-') || id.startsWith('xiaomi') || id.startsWith('mimo')) return 'xiaomi'

  // OpenRouter — explicit prefix or vendor/model slug
  if (id.startsWith('openrouter') || id.startsWith('or-') || id.includes('/')) {
    // `vendor/model` slugs are an OpenRouter convention — peel off and recurse
    const vendor = id.split('/')[0]
    if (vendor && vendor !== id) {
      const nested = inferProviderFromModelId(vendor)
      if (nested) return nested
    }
    return 'openrouter'
  }

  // Local / OSS — Llama, Phi, Gemma fall back to ollama (most common host)
  if (
    id.startsWith('llama') ||
    id.startsWith('phi') ||
    id.startsWith('gemma') ||
    id.startsWith('codellama') ||
    id.startsWith('ollama')
  ) return 'ollama'

  // HuggingFace generic prefix
  if (id.startsWith('hf-') || id.startsWith('huggingface')) return 'huggingface'

  return undefined
}

/**
 * Infer provider slug from a channel `baseUrl`. Substring match against
 * common API hosts — order matters when one host can mask another.
 */
export function inferProviderFromBaseUrl(baseUrl: string): string | undefined {
  if (!baseUrl) return undefined
  const url = baseUrl.toLowerCase()

  if (url.includes('anthropic.com')) return 'anthropic'
  if (url.includes('openai.com') || url.includes('openai.azure.com')) return 'openai'
  if (url.includes('googleapis.com') || url.includes('generativelanguage') || url.includes('vertex')) return 'google'
  if (url.includes('deepseek')) return 'deepseek'
  if (url.includes('dashscope') || url.includes('aliyun') || url.includes('aliyuncs')) return 'aliyun'
  if (url.includes('mistral')) return 'mistral'
  if (url.includes('x.ai') || url.includes('grok')) return 'xai'
  if (url.includes('moonshot')) return 'moonshot'
  if (url.includes('bigmodel') || url.includes('zhipuai') || url.includes('z.ai')) return 'zai'
  if (url.includes('minimax')) return 'minimax'
  if (url.includes('volc') || url.includes('ark.cn-')) return 'volcengine'
  if (url.includes('byteplus')) return 'byteplus'
  if (url.includes('baidu') || url.includes('baidubce')) return 'baidu'
  if (url.includes('xiaomi')) return 'xiaomi'
  if (url.includes('openrouter')) return 'openrouter'
  if (url.includes('huggingface') || url.includes('hf.co')) return 'huggingface'
  if (url.includes('cloudflare') || url.includes('workers.dev')) return 'cloudflare'
  if (url.includes('vercel')) return 'vercel'
  if (url.includes('ollama') || url.includes('localhost') || url.includes('127.0.0.1')) return 'ollama'
  if (url.includes('opencode')) return 'opencode'

  return undefined
}

/**
 * Resolve the bundled PNG URL for an assistant / channel logo.
 *
 * @param modelId  Raw model id ("claude-sonnet-4-6", "deepseek-v4-pro", …)
 * @param provider Optional provider hint from the channel record — wins over inference.
 * @returns Absolute URL when a logo is found, empty string otherwise.
 */
export function getModelLogo(modelId: string, provider?: string): string {
  const slug = (provider || inferProviderFromModelId(modelId) || '').toLowerCase()
  return providerLogoMap[slug] ?? ''
}

/**
 * Resolve the channel-row logo. Channels always carry an explicit `provider`
 * slug in the registry; pass that as the second arg when available — base
 * URL inference is the fallback for self-hosted endpoints.
 */
export function getChannelLogo(baseUrl: string, provider?: string): string {
  const slug = (provider || inferProviderFromBaseUrl(baseUrl) || '').toLowerCase()
  return providerLogoMap[slug] ?? ''
}

/**
 * Display-name resolution. Looks at the channel registry first (each
 * `ChannelModel` carries `{id, name}` — `name` is the friendly label set
 * in the channel settings), then falls back to the raw model id.
 */
export function resolveModelDisplayName(modelId: string, channels?: unknown[]): string {
  if (!modelId) return modelId
  const list = (channels ?? []) as Channel[]
  for (const ch of list) {
    if (!Array.isArray(ch?.models)) continue
    for (const m of ch.models) {
      if (m?.id === modelId) {
        const name = m.name
        if (name && name.trim()) return name
      }
    }
  }
  return modelId
}
