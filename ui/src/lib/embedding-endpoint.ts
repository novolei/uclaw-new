import { invoke } from '@tauri-apps/api/core'

// ─── Embedding endpoint config (mirrors Rust EmbeddingEndpointPayload) ────

export interface EmbeddingEndpointConfig {
  base_url: string
  model: string
  dimensions: number
}

export async function getEmbeddingConfig(): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('get_embedding_config')
}

export async function setEmbeddingConfig(
  payload: EmbeddingEndpointConfig,
): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('set_embedding_config', { payload })
}

/**
 * Sprint 2.2.5c — preview-only reachability check for the embedding
 * base_url. Same probe `set_embedding_config` runs implicitly before
 * persisting, exposed as its own IPC so the user can click "Test"
 * without committing the form.
 *
 * Resolves on a reachable URL (HTTP status < 500). Rejects with a
 * structured error message when connection refused, DNS fails, TLS
 * fails, server returns 5xx, or the 2s timeout elapses.
 */
export async function testEmbeddingEndpoint(baseUrl: string): Promise<void> {
  await invoke<void>('test_embedding_endpoint', { baseUrl })
}

