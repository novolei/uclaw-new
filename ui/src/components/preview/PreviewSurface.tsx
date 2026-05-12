import * as React from 'react'
import { useFileBytes } from '@/components/preview/hooks/useFileBytes'
import { usePreviewRouter } from '@/components/preview/hooks/usePreviewRouter'
import { PreviewEmpty } from './PreviewEmpty'
import { CodeRenderer } from './renderers/CodeRenderer'
import { MarkdownRenderer } from './renderers/MarkdownRenderer'
import { ImageRenderer } from './renderers/ImageRenderer'
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
    if (route.kind === 'code' || route.kind === 'markdown') {
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
  if (route.kind === 'markdown') {
    return <MarkdownRenderer text={text} />
  }
  if (route.kind === 'code') {
    return (
      <CodeRenderer
        code={text}
        language={route.language ?? 'text'}
        cacheScope={state.resolvedPath}
        refreshVersion={refreshVersion}
        truncated={state.truncated}
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
