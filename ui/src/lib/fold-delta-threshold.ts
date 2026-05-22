import { invoke } from '@tauri-apps/api/core'

// Bundle 17-B — settings-exposed threshold for `/compact` fold-delta
// path. Mirrors `get_fold_delta_threshold` / `set_fold_delta_threshold`
// Tauri commands registered in `src-tauri/src/main.rs`.
//
// Backend semantics:
// - Read-once-per-call by `tauri_commands.rs::/compact intercept`, so
//   changes apply on the next `/compact` without restart.
// - `set_fold_delta_threshold` clamps to
//   `[FOLD_DELTA_THRESHOLD_MIN, FOLD_DELTA_THRESHOLD_MAX]` = `[1, 50]`
//   server-side; the clamped value is what gets persisted.
// - Below 1 disables the delta path entirely (every compact re-renders).
// - Above 50 would let nearly-fresh folds slip through as deltas and
//   defeat the cache-stability benefit.

/** Mirrors the Rust default in
 *  `src-tauri/src/memubot_config.rs::default_fold_delta_threshold`. */
export const FOLD_DELTA_THRESHOLD_DEFAULT = 50

export const FOLD_DELTA_THRESHOLD_MIN = 1
export const FOLD_DELTA_THRESHOLD_MAX = 50

export async function getFoldDeltaThreshold(): Promise<number> {
  return await invoke<number>('get_fold_delta_threshold')
}

export async function setFoldDeltaThreshold(threshold: number): Promise<void> {
  await invoke<void>('set_fold_delta_threshold', { threshold })
}
