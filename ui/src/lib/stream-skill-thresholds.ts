import { invoke } from '@tauri-apps/api/core'

// Bundle 26-B / 26-D / 27-B — settings-exposed thresholds for the
// LLM-stream idle timeout + the skill distillation / promotion
// passes. Mirrors the Rust commands in
// `src-tauri/src/tauri_commands.rs` (registered in main.rs's
// `invoke_handler!`).
//
// Backend semantics:
// - `set_stream_idle_timeout_secs` is read-once-per-call by the
//   dispatcher / headless `call_llm`, so changes apply to the very
//   next message without restarting anything.
// - `set_skill_prune_min_unused_days` and
//   `set_skill_promote_min_returned_count` mutate the JSON config
//   then trigger a silent `restart_service("proactive")` so the new
//   threshold lands in the proactive tick's MemoryOsRuntimeConfig
//   snapshot on the next tick.
// - All three setters server-side clamp to a sane range; the
//   request value is logged so the user can see the applied value
//   in tracing.

export interface StreamSkillThresholds {
  stream_idle_timeout_secs: number
  skill_prune_min_unused_days: number
  skill_promote_min_returned_count: number
}

/** Documented defaults — keep in sync with the `default_*` fns in
 * `src-tauri/src/memubot_config.rs`. */
export const STREAM_SKILL_DEFAULTS: StreamSkillThresholds = {
  stream_idle_timeout_secs: 90,
  skill_prune_min_unused_days: 30,
  skill_promote_min_returned_count: 3,
}

export async function getStreamIdleTimeoutSecs(): Promise<number> {
  return await invoke<number>('get_stream_idle_timeout_secs')
}

export async function setStreamIdleTimeoutSecs(secs: number): Promise<void> {
  await invoke<void>('set_stream_idle_timeout_secs', { secs })
}

export async function getSkillPruneMinUnusedDays(): Promise<number> {
  return await invoke<number>('get_skill_prune_min_unused_days')
}

export async function setSkillPruneMinUnusedDays(days: number): Promise<void> {
  await invoke<void>('set_skill_prune_min_unused_days', { days })
}

export async function getSkillPromoteMinReturnedCount(): Promise<number> {
  return await invoke<number>('get_skill_promote_min_returned_count')
}

export async function setSkillPromoteMinReturnedCount(count: number): Promise<void> {
  await invoke<void>('set_skill_promote_min_returned_count', { count })
}

/** One-shot read of all three. Used by the settings section on mount. */
export async function getStreamSkillThresholds(): Promise<StreamSkillThresholds> {
  const [stream_idle_timeout_secs, skill_prune_min_unused_days, skill_promote_min_returned_count] =
    await Promise.all([
      getStreamIdleTimeoutSecs(),
      getSkillPruneMinUnusedDays(),
      getSkillPromoteMinReturnedCount(),
    ])
  return {
    stream_idle_timeout_secs,
    skill_prune_min_unused_days,
    skill_promote_min_returned_count,
  }
}
