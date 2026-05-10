/**
 * Single source of truth for the global SafetyMode in the UI.
 *
 * Hydrated on mount by `PermissionModeSelector` from
 * `get_safety_policy`; mutated via `set_safety_mode`. Persisted on the
 * backend in `~/.uclaw/safety_policy.json`.
 *
 * For per-session / per-pattern rules + audit log see `safety/permissions.rs`
 * (P6) and the Settings → 工具权限 tab.
 */

import { atom } from 'jotai'
import type { SafetyModeWire } from '@/lib/tauri-bridge'

export const safetyModeAtom = atom<SafetyModeWire>('supervised')
