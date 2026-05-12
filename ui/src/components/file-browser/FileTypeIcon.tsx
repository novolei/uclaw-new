/**
 * FileTypeIcon — VS-Code-style file & folder glyphs.
 *
 * Backed by `@react-symbols/icons` (Miguel Solorio's Symbols set — the
 * same family VS Code ships). The library's `FileIcon` with `autoAssign`
 * picks the right glyph from the filename, matching the per-language
 * colour cues users expect (orange JS, red HTML, purple CSS, etc.) —
 * a fidelity we couldn't reach with lucide outlines and a hand-rolled
 * colour map. Reference: Proma's apps/electron/.../FileTypeIcon.tsx.
 */

import * as React from 'react'
import { FileIcon, FolderIcon } from '@react-symbols/icons/utils'

interface FileTypeIconProps {
  name: string
  isDirectory: boolean
  isOpen?: boolean
  size?: number
  className?: string
}

export const FileTypeIcon = React.memo(function FileTypeIcon({
  name,
  isDirectory,
  isOpen = false,
  size = 16,
  className,
}: FileTypeIconProps): React.ReactElement {
  const wrapStyle: React.CSSProperties = {
    width: size,
    height: size,
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  }
  // @react-symbols/icons' FolderIcon doesn't have an open/closed variant —
  // the `isOpen` prop on this component is preserved for callers that still
  // pass it but doesn't change the glyph. Tree expand/collapse is signalled
  // by the rotating ChevronRight chevron, not the folder shape, which keeps
  // the icon matrix stable.
  void isOpen
  if (isDirectory) {
    return (
      <span className={className} style={wrapStyle}>
        <FolderIcon folderName={name} width={size} height={size} />
      </span>
    )
  }
  return (
    <span className={className} style={wrapStyle}>
      <FileIcon fileName={name} autoAssign width={size} height={size} />
    </span>
  )
})
