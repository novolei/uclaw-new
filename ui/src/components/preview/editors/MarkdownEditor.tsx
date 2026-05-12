/**
 * MarkdownEditor — routes to TipTap rich or CM6 raw based on the
 * persisted markdownEditorModeAtom toggle.
 *
 * Both modes use auto-save (saveMode='auto' passed down). The toggle
 * lives in EditorToolbar (Task 15).
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { markdownEditorModeAtom } from '@/atoms/preview-editor-atoms'
import { TextEditor, type EditorProps } from './TextEditor'
import { MarkdownRichEditor } from './MarkdownRichEditor'

export function MarkdownEditor(props: EditorProps): React.ReactElement {
  const mode = useAtomValue(markdownEditorModeAtom)
  // Both modes auto-save (per W4d spec §4 hybrid model).
  const propsWithAutoSave: EditorProps = { ...props, saveMode: 'auto' }
  return mode === 'rich' ? (
    <MarkdownRichEditor {...propsWithAutoSave} />
  ) : (
    <TextEditor {...propsWithAutoSave} language="markdown" />
  )
}
