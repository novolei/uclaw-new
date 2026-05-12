/**
 * file-type-colors — Per-extension chip color tokens.
 *
 * Single source of truth (per master spec §2.2). Theme-token classes only —
 * no raw hex — so chips adapt to every uClaw theme.
 *
 * `icon` is the inline-SVG color class; `bg` is the chip background;
 * `border` is the chip border. The chip uses bg-foreground/[0.04]
 * by default and only the icon color shifts per ext, so we keep this
 * deliberately small.
 */

export interface ChipColors {
  /** Tailwind class applied to the leading icon (color only). */
  icon: string
}

const TYPE_COLOR_MAP: Record<string, ChipColors> = {
  // typescript / javascript
  ts:   { icon: 'text-sky-600 dark:text-sky-300' },
  tsx:  { icon: 'text-sky-600 dark:text-sky-300' },
  js:   { icon: 'text-amber-600 dark:text-amber-300' },
  jsx:  { icon: 'text-amber-600 dark:text-amber-300' },
  mjs:  { icon: 'text-amber-600 dark:text-amber-300' },
  // systems
  rs:   { icon: 'text-orange-700 dark:text-orange-300' },
  go:   { icon: 'text-cyan-600 dark:text-cyan-300' },
  py:   { icon: 'text-emerald-600 dark:text-emerald-300' },
  // web
  html: { icon: 'text-orange-600 dark:text-orange-300' },
  css:  { icon: 'text-blue-600 dark:text-blue-300' },
  scss: { icon: 'text-pink-600 dark:text-pink-300' },
  // data / markup
  json: { icon: 'text-yellow-600 dark:text-yellow-300' },
  yaml: { icon: 'text-violet-600 dark:text-violet-300' },
  yml:  { icon: 'text-violet-600 dark:text-violet-300' },
  toml: { icon: 'text-violet-600 dark:text-violet-300' },
  md:   { icon: 'text-slate-600 dark:text-slate-300' },
  // images
  png:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  jpg:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  jpeg: { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  gif:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  svg:  { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  webp: { icon: 'text-fuchsia-600 dark:text-fuchsia-300' },
  // documents
  pdf:  { icon: 'text-rose-600 dark:text-rose-300' },
  docx: { icon: 'text-blue-700 dark:text-blue-300' },
  xlsx: { icon: 'text-emerald-700 dark:text-emerald-300' },
  pptx: { icon: 'text-orange-700 dark:text-orange-300' },
}

const FALLBACK: ChipColors = { icon: 'text-foreground/55' }

export function getChipColors(ext: string): ChipColors {
  return TYPE_COLOR_MAP[ext.toLowerCase()] ?? FALLBACK
}
