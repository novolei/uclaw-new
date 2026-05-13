import * as React from 'react'
import { useFileBytes } from '@/components/preview/hooks/useFileBytes'
import { usePreviewRouter } from '@/components/preview/hooks/usePreviewRouter'
import { PreviewEmpty } from './PreviewEmpty'
import { CodeRenderer } from './renderers/CodeRenderer'
import { MarkdownRenderer } from './renderers/MarkdownRenderer'
import { ImageRenderer } from './renderers/ImageRenderer'
import { VideoRenderer } from './renderers/VideoRenderer'
import { EditorSurface } from './editors/EditorSurface'
import { WriteApprovalDialog } from './editors/WriteApprovalDialog'
import { DiffRenderer } from './renderers/diff/DiffRenderer'
import { BinaryFallback } from './renderers/BinaryFallback'
import { PdfRenderer } from './renderers/PdfRenderer'
import { DocxRenderer } from './renderers/DocxRenderer'
import { XlsxRenderer } from './renderers/XlsxRenderer'
import { PptxRenderer } from './renderers/PptxRenderer'
import { LegacyOfficeHint } from './renderers/LegacyOfficeHint'
import { usePreviewRefresh } from '@/hooks/usePreviewRefresh'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

interface PreviewSurfaceProps {
  target: PreviewFileTarget | null
}

function decodeUtf8(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
  } catch {
    return ''
  }
}

export function PreviewSurface({ target }: PreviewSurfaceProps): React.ReactElement {
  const route = usePreviewRouter(target)
  const state = useFileBytes(target)
  const resolvedPath = state.status === 'ready' ? state.resolvedPath : null
  const refreshVersion = usePreviewRefresh(resolvedPath)

  // Decode bytes lazily; only when we know we need text (code / markdown).
  const text = React.useMemo(() => {
    if (state.status !== 'ready') return ''
    if (!route) return ''
    if (route.kind === 'code' || route.kind === 'markdown' || route.kind === 'diff') {
      return decodeUtf8(state.bytes)
    }
    return ''
  }, [state, route])

  if (!target) return <PreviewEmpty status="idle" />
  if (state.status === 'loading' || state.status === 'idle') return <PreviewEmpty status="loading" />
  if (state.status === 'error') return <PreviewEmpty status="error" message={state.message} />

  if (!route) return <PreviewEmpty status="idle" />

  if (route.kind === 'image') {
    return <ImageRenderer resolvedPath={state.resolvedPath} name={target.name} />
  }
  if (route.kind === 'video') {
    return <VideoRenderer resolvedPath={state.resolvedPath} name={target.name} />
  }
  // Force a fresh EditorSurface per file so internal baseline/content/mtime
  // state cannot leak across switches. The polish commit that decoupled the
  // sync-from-props effect from initialContent changes made the editor sticky
  // to whatever it mounted with — a `key` on the target makes that intentional.
  const surfaceKey = `${target.mountId}::${target.relPath}`
  if (route.kind === 'markdown') {
    return (
      <>
        <EditorSurface
          key={surfaceKey}
          target={target}
          initialContent={text}
          mtimeMs={state.mtimeMs}
          isMarkdown={true}
        />
        <WriteApprovalDialog />
      </>
    )
  }
  if (route.kind === 'code') {
    return (
      <>
        <EditorSurface
          key={surfaceKey}
          target={target}
          initialContent={text}
          mtimeMs={state.mtimeMs}
          isMarkdown={false}
          language={route.language ?? 'text'}
        />
        <WriteApprovalDialog />
      </>
    )
  }
  if (route.kind === 'diff') {
    return (
      <DiffRenderer
        left={{ content: '', label: 'before' }}
        right={{ content: text, label: target.name }}
        language="diff"
      />
    )
  }
  if (route.kind === 'pdf') {
    return <PdfRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'docx') {
    return <DocxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'xlsx') {
    return <XlsxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'pptx') {
    return <PptxRenderer bytes={state.bytes} name={target.name} />
  }
  if (route.kind === 'legacyOffice') {
    return <LegacyOfficeHint name={target.name} ext={route.ext} />
  }
  return <BinaryFallback name={target.name} size={state.size} ext={route.ext} />
}
