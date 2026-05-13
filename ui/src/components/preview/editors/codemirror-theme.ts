/**
 * codemirror-theme — uClaw theme tokens → CodeMirror 6 EditorView.theme.
 *
 * CM6 themes are JSS-style objects. We pull colors from uClaw's CSS
 * custom properties (var(--popover), var(--foreground), etc.) so the
 * editor adapts to all 11 uClaw themes without per-theme builds.
 *
 * CM6 ships per-language `LanguageSupport` packages (lang-typescript,
 * lang-rust, etc.) with built-in syntax highlighting via Lezer.
 * For W4d we use CM6's native Lezer highlighting via uclawHighlightStyle
 * rather than bridging shiki — visually close enough since both render
 * the same tokens into the same color buckets.
 */

import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

/** Build the editor theme from uClaw CSS tokens. */
export const uclawCmTheme = EditorView.theme(
  {
    '&': {
      color: 'hsl(var(--foreground))',
      backgroundColor: 'hsl(var(--popover))',
      fontFamily:
        'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
      fontSize: '12px',
      height: '100%',
    },
    '.cm-content': {
      caretColor: 'hsl(var(--foreground))',
      padding: '8px 12px',
    },
    '.cm-cursor': {
      borderLeftColor: 'hsl(var(--foreground))',
    },
    '&.cm-focused .cm-selectionBackground, ::selection': {
      backgroundColor: 'hsl(var(--accent) / 0.4) !important',
    },
    '.cm-gutters': {
      backgroundColor: 'hsl(var(--popover))',
      color: 'hsl(var(--muted-foreground))',
      border: 'none',
      borderRight: '1px solid hsl(var(--border))',
    },
    '.cm-activeLineGutter, .cm-activeLine': {
      backgroundColor: 'hsl(var(--accent) / 0.08)',
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'hsl(var(--accent) / 0.2)',
      border: 'none',
      color: 'hsl(var(--muted-foreground))',
    },
    '.cm-tooltip': {
      backgroundColor: 'hsl(var(--popover))',
      color: 'hsl(var(--popover-foreground))',
      border: '1px solid hsl(var(--border))',
      borderRadius: '6px',
    },
  },
  { dark: false }, // theme is token-driven; uClaw handles dark via CSS vars
)

/** Lezer syntax highlight palette — uses CSS custom properties with
 *  GitHub-light fallbacks. uClaw can define `--syntax-*` overrides in
 *  globals.css per theme; the fallbacks ensure readability without them. */
export const uclawHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: 'var(--syntax-keyword, #d73a49)' },
  { tag: t.string, color: 'var(--syntax-string, #032f62)' },
  { tag: t.number, color: 'var(--syntax-number, #005cc5)' },
  { tag: t.comment, color: 'var(--syntax-comment, #6a737d)', fontStyle: 'italic' },
  { tag: t.function(t.variableName), color: 'var(--syntax-function, #6f42c1)' },
  { tag: t.typeName, color: 'var(--syntax-type, #6f42c1)' },
  { tag: t.variableName, color: 'hsl(var(--foreground))' },
  { tag: t.operator, color: 'var(--syntax-operator, #d73a49)' },
  { tag: t.punctuation, color: 'hsl(var(--muted-foreground))' },
])

export const uclawSyntaxHighlight = syntaxHighlighting(uclawHighlightStyle)
