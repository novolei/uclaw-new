// Locale resolvers for DHP marketplace overlays.
//
// Two overlay shapes:
//   - Entry-level (registry index): { name?, description? } per locale, used in cards.
//   - Spec-level (spec.yaml `i18n.<locale>`): also carries
//     `config_schema.<key>.{label?, description?, placeholder?, options?}`.
//
// pickLocale is region-tolerant: if `zh-CN` isn't present but `zh-TW` is, the
// caller still gets `zh-TW` rather than the English fallback — better than
// nothing for a zh-* user.

export interface EntryI18n {
  name?: string | null
  description?: string | null
  system_prompt?: string | null
}

export interface ConfigOverlay {
  label?: string
  description?: string
  placeholder?: string
  options?: Record<string, string>
}

export interface SpecI18nBlock extends EntryI18n {
  config_schema?: Record<string, ConfigOverlay>
}

export type SpecI18n = Record<string, SpecI18nBlock>

export function pickLocale<T>(
  i18n: Record<string, T> | undefined | null,
  locale: string,
): T | undefined {
  if (!i18n) return undefined
  if (i18n[locale]) return i18n[locale]
  const base = locale.split('-')[0]
  const matchKey = Object.keys(i18n).find((k) => k.split('-')[0] === base)
  return matchKey ? i18n[matchKey] : undefined
}

export function localizeEntry(
  field: 'name' | 'description',
  base: string | null | undefined,
  i18n: Record<string, EntryI18n> | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.[field] ?? base ?? ''
}

export function localizeSpec(
  field: 'name' | 'description' | 'system_prompt',
  base: string | null | undefined,
  i18n: SpecI18n | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.[field] ?? base ?? ''
}

export function localizeConfig(
  key: string,
  field: 'label' | 'description' | 'placeholder',
  base: string | null | undefined,
  i18n: SpecI18n | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.config_schema?.[key]?.[field] ?? base ?? ''
}

export function localizeOption(
  inputKey: string,
  optionValue: string,
  baseLabel: string,
  i18n: SpecI18n | undefined,
  locale: string,
): string {
  return (
    pickLocale(i18n, locale)?.config_schema?.[inputKey]?.options?.[optionValue] ??
    baseLabel
  )
}
