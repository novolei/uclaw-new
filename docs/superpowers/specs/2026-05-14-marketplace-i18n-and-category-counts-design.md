# Marketplace i18n + Category-Counts Fix — Design

**Status:** Draft → ready to plan
**Author:** Claude (worktree `marketplace-i18n-and-counts`)
**Date:** 2026-05-14
**Scope:** Phase 3a polish (post-merge of #158)

---

## 1. Problem

### 1.1 Locale mismatch in store rendering

DHP specs encode authoring-language strings at the top level and provide
locale overlays under `i18n.<locale>`. For example,
`xiaohongshu-keyword-monitor/spec.yaml`:

```yaml
name: Xiaohongshu Keyword Monitor
description: Passively monitors Xiaohongshu ...
config_schema:
  - key: keywords
    label: Search Keywords
    description: The keyword or brand name to monitor ...
i18n:
  zh-CN:
    name: 小红书关键词监控
    description: 被动监控小红书上提及品牌...
    config_schema:
      keywords:
        label: 监控关键词
        description: 在小红书上搜索的品牌名、产品名或关键词
```

Halo (the reference store) detects `zh-CN` and renders the overlay. uClaw
renders the top-level English even though the entire app UI is Chinese.

Two layers are involved:

- **Registry index** (`index.json`): per-entry `i18n.<locale>.{name,description}`.
  Authors can put `name` in either language; the `locale` field declares the
  default. Some humans (e.g. `ai-daily-news`) ship Chinese `name` +
  `i18n.en-US` English overlay; others (e.g. `xiaohongshu-keyword-monitor`)
  ship English `name` + `i18n.zh-CN` Chinese overlay.

- **spec.yaml** (per human): same shape under top-level `i18n`, but also
  covers `config_schema.<key>.{label,description,placeholder,options}` — a
  deep overlay that Halo applies recursively.

Today's behaviour:

- Backend `cache.rs` extracts only `i18n.en-US.{name,description}` from the
  registry index (other locales are dropped) and exposes them as
  `MarketplaceItem.i18nName` / `i18nDescription`.
- Backend `I18nLocaleBlock` (in `humane_v1.rs`) only deserialises
  `{name, description, system_prompt}`. The `config_schema` overlay is
  parsed away into the wildcard `serde` consumer and never reaches the UI.
- Frontend uses `item.i18nName ?? item.name` — which always prefers the
  cached English even for zh-CN users.

### 1.2 Category-count inconsistency bug

`StoreView.tsx` derives chip counts from the current paged query result:

```ts
const cats: Record<string, number> = {}
for (const it of result.items) {
  cats[it.category] = (cats[it.category] ?? 0) + 1
}
setCounts((prev) => ({ ...prev, ...cats }))
```

When the user picks `dev-tools`, the backend returns only dev-tools rows;
`cats['dev-tools']` becomes 8 (the true count). When viewing "全部" with
PAGE_SIZE=20, only the first 20 items get counted, so `dev-tools` shows 2.
Because chips are sorted by count descending, the chip order also changes
when the user clicks. Both numbers are "true within the rendered page" but
neither answers the question chips actually represent: *"how many items
would I see if I clicked this chip right now (under current type/search
filters, ignoring the category filter)?"*

---

## 2. Goals

- Authoring-language and per-locale strings both surface end-to-end: index
  `name`/`description`, spec `name`/`description`/`system_prompt`, AND
  spec `config_schema.<key>.{label,description,placeholder,options}`.
- Locale resolution order: user setting → `navigator.language` →
  authoring-language (`item.locale` or top-level field).
- Category chip counts always reflect "items matching `itemType` + `search`,
  grouped by category" — independent of the active category filter, so the
  numbers and ordering are stable across clicks.

## 3. Non-Goals

- A general translation runtime for UI chrome strings (already
  hard-coded zh in the UI; out of scope).
- Authoring-time validation that translations are complete (a separate
  concern for the DHP repo itself).
- Persisting the user's locale override across sessions in this PR — we'll
  use `atomWithStorage` so it persists, but we won't add a settings-UI
  surface for it; that goes in a follow-up.

## 4. Design

### 4.1 Backend — full i18n surface

**`HumaneAutomationSpec.i18n: HashMap<String, I18nLocaleBlock>`** stays as
the carrier; `I18nLocaleBlock` gains an optional `config_schema` field.

Approach: keep `config_schema` as `serde_json::Value` (HashMap<String, Value>)
rather than typed — DHP's overlay shape per key is `{label?, description?,
placeholder?, options?}` where `options` is `{value: label}` map. Typing
this strictly buys nothing because the frontend treats it as opaque
key-lookup. `serde_json::Value` keeps the parser lenient and survives
future overlay additions.

```rust
pub struct I18nLocaleBlock {
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub system_prompt: Option<String>,
    #[serde(default)] pub config_schema: Option<serde_json::Value>,
}
```

**`MarketplaceItem`** drops the en-US-only fields and gains a single
`i18n: HashMap<String, EntryI18n>` map carrying every locale the registry
provided. Old fields are removed (clean break — no consumers outside the
store UI).

```rust
pub struct MarketplaceItem {
    // existing fields ...
    pub locale: Option<String>,
    pub i18n: HashMap<String, EntryI18n>, // was: i18n_name, i18n_description
}
```

`MarketplaceDetail.parsedSpecJson` is already `serde_json::Value` and
already carries `i18n.<locale>.config_schema` once the parser change above
lands — no extra plumbing.

### 4.2 Backend — category counts command

New Tauri command:

```rust
#[tauri::command]
pub async fn marketplace_category_counts(
    state: tauri::State<'_, AppState>,
    item_type: Option<String>,
    search: Option<String>,
) -> Result<HashMap<String, i64>, Error>
```

Implementation in `marketplace/cache.rs`:

```sql
SELECT category, COUNT(*) FROM marketplace_items i
[JOIN automation_marketplace_fts ON ... AND MATCH ?]   -- if search
WHERE 1=1
  [AND type = ?]                                       -- if item_type
GROUP BY category
```

Same JOIN/MATCH machinery as `query_marketplace_cached` but no category
filter, no pagination. Returns `{category: count}` for the entire
post-type-and-search corpus.

### 4.3 Frontend — locale infra

```ts
// atoms/marketplace.ts
type Locale = string // e.g. 'zh-CN', 'en-US'

function detectInitialLocale(): Locale {
  if (typeof navigator !== 'undefined' && navigator.language) {
    return navigator.language
  }
  return 'en-US'
}

export const userLocaleAtom = atomWithStorage<Locale>(
  'uclaw.marketplace.locale',
  detectInitialLocale(),
)
```

`localize` helper for both index-level and spec-level overlays:

```ts
// lib/marketplace-i18n.ts
type I18nMap = Record<string, { name?: string; description?: string; system_prompt?: string; config_schema?: Record<string, ConfigOverlay> }>

interface ConfigOverlay {
  label?: string
  description?: string
  placeholder?: string
  options?: Record<string, string>
}

/** Pick `i18n[locale].field` → fall back to `fallback`. Region-tolerant: zh matches zh-CN if exact key is missing. */
export function pickLocale<T>(
  i18n: Record<string, T> | undefined,
  locale: string,
): T | undefined {
  if (!i18n) return undefined
  if (i18n[locale]) return i18n[locale]
  const base = locale.split('-')[0]
  const match = Object.keys(i18n).find((k) => k.split('-')[0] === base)
  return match ? i18n[match] : undefined
}

export function localizeText(
  field: 'name' | 'description' | 'system_prompt',
  base: string | null | undefined,
  i18n: I18nMap | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.[field] ?? base ?? ''
}

export function localizeConfig(
  key: string,
  field: 'label' | 'description' | 'placeholder',
  base: string | null | undefined,
  i18n: I18nMap | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.config_schema?.[key]?.[field] ?? base ?? ''
}

export function localizeOption(
  inputKey: string,
  optionValue: string,
  baseLabel: string,
  i18n: I18nMap | undefined,
  locale: string,
): string {
  return pickLocale(i18n, locale)?.config_schema?.[inputKey]?.options?.[optionValue] ?? baseLabel
}
```

### 4.4 Frontend — applying it

Touchpoints:

- `StoreCard.tsx`, `StoreFeaturedRow.tsx`: replace `item.i18nName ?? item.name`
  with `localizeText('name', item.name, item.i18n, locale)`.
- `StoreDetail.tsx`: same for the title/description, plus the config-schema
  preview block — pull `parsedSpec.i18n` and run each field/option through
  the helpers.

### 4.5 Frontend — category counts wiring

Replace the per-page derivation in `StoreView.tsx` with a separate effect
that calls `marketplaceCategoryCounts(itemType, search)` whenever
`filters.itemType` or `filters.search` changes (NOT `filters.category` —
that's the whole point). Result replaces the atom (not merges).

```ts
React.useEffect(() => {
  marketplaceCategoryCounts(
    filters.itemType === 'all' ? undefined : filters.itemType,
    filters.search || undefined,
  ).then(setCounts).catch(...)
}, [filters.itemType, filters.search])
```

## 5. Compatibility

- DB schema unchanged. The cache already stores the full `i18n_json`; we
  just stop dropping locales when constructing `MarketplaceItem`.
- `MarketplaceItem.i18nName/i18nDescription` are removed — only the store
  UI consumes them and we touch every consumer in this PR. Verified no
  external readers via `rg`.
- spec.yaml backward compatibility: specs without `i18n.<locale>.config_schema`
  fall through to the top-level English. No spec changes required.

## 6. Tests

- Rust: extend `I18nLocaleBlock` parse test to assert `config_schema`
  round-trip with a synthetic spec.
- Rust: `marketplace_category_counts` test against a seeded cache —
  assert independence from the category filter, dependence on type + search.
- TS: `marketplace-i18n.test.ts` table-driven test of `localizeText`
  / `localizeConfig` / `localizeOption` with zh-CN, en-US, missing-locale,
  region-tolerant `zh` → `zh-CN` cases.

## 7. Out of scope / follow-ups

- Settings UI to change locale (key in `atomWithStorage` is enough for
  now; persisting works, just no toggle).
- Translating uClaw UI chrome (the rest of the app is hard-Chinese).
- Localising tags and category names — tags are user-facing free text;
  category labels already have a Chinese override in `StoreHeader.tsx`.
