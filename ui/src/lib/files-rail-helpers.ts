/**
 * files-rail-helpers — pure utilities shared by files-rail components.
 *
 * `spaceIdForMount` derives the workspace `space_id` from a MountRoot.
 * Encoded in the mount.id for workspace + workspace-attached kinds;
 * falls back to the active workspace atom for session-scoped mounts.
 */

import type { MountKind } from '@/atoms/files-rail-atoms'

export function spaceIdForMount(
  mount: { id: string; kind: MountKind },
  currentWorkspaceId: string | null,
): string | null {
  // workspace:<sid>
  if (mount.id.startsWith('workspace:')) {
    const sid = mount.id.slice('workspace:'.length)
    return sid.length > 0 ? sid : null
  }
  // workspace-attached:<sid>:<hash>
  if (mount.id.startsWith('workspace-attached:')) {
    const rest = mount.id.slice('workspace-attached:'.length)
    const colon = rest.indexOf(':')
    if (colon < 0) return null
    const sid = rest.slice(0, colon)
    return sid.length > 0 ? sid : null
  }
  // session:<sid> or attached:<sid>:<hash> — sessions live in their workspace
  if (mount.id.startsWith('session:') || mount.id.startsWith('attached:')) {
    return currentWorkspaceId
  }
  return null
}
