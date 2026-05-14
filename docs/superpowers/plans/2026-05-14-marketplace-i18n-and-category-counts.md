# Marketplace i18n + Category-Counts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply per-locale overlays (entry- and spec-level) end-to-end, and fix the category-chip count/order drift caused by deriving counts from paginated results.

**Architecture:** Backend keeps the full `i18n` map alive (currently it's dropped to en-US-only at the DTO boundary), and adds a separate `marketplace_category_counts` Tauri command. Frontend gets a `userLocaleAtom` + `localizeText` / `localizeConfig` / `localizeOption` helpers, applied at `StoreCard` / `StoreFeaturedRow` / `StoreDetail`. Category counts are fetched independently from the active category filter so chip ordering is stable.

**Tech Stack:** Rust (rusqlite, serde, garde), React 18 + TypeScript + Jotai (`atomWithStorage`).

**Spec:** `docs/superpowers/specs/2026-05-14-marketplace-i18n-and-category-counts-design.md`

---

### Task 1: Backend — extend `I18nLocaleBlock` with `config_schema` overlay

**Files:**
- Modify: `src-tauri/src/automation/protocol/humane_v1.rs:404-415`
- Test: `src-tauri/src/automation/protocol/humane_v1.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module at the bottom of `humane_v1.rs`:

```rust
#[test]
fn parses_i18n_with_config_schema_overlay() {
    let yaml = r#"
spec_version: "1"
name: Test
version: 1.0.0
author: test
description: test
type: automation
icon: other
system_prompt: irrelevant
config_schema:
  - key: keywords
    label: Search Keywords
    type: string
    required: true
    description: en desc
i18n:
  zh-CN:
    name: 中文名
    description: 中文描述
    config_schema:
      keywords:
        label: 监控关键词
        description: 中文描述
        placeholder: 关键词
        options:
          opt_a: 选项A
"#;
    let spec: HumaneAutomationSpec = serde_yml::from_str(yaml).expect("parses");
    spec.validate().expect("validates");
    let zh = spec.i18n.get("zh-CN").expect("zh-CN present");
    assert_eq!(zh.name.as_deref(), Some("中文名"));
    let cs = zh.config_schema.as_ref().expect("config_schema present");
    let keywords = cs.get("keywords").expect("keywords overlay");
    assert_eq!(keywords["label"].as_str(), Some("监控关键词"));
    assert_eq!(keywords["options"]["opt_a"].as_str(), Some("选项A"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib parses_i18n_with_config_schema_overlay`
Expected: FAIL (no `config_schema` field on `I18nLocaleBlock`)

- [ ] **Step 3: Add `config_schema` to `I18nLocaleBlock`**

Edit `src-tauri/src/automation/protocol/humane_v1.rs` around line 404:

```rust
pub struct I18nLocaleBlock {
    #[garde(skip)]
    #[serde(default)]
    pub name: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub description: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Per-locale overlay for config_schema. Shape: { key: { label?, description?, placeholder?, options? } }.
    /// Kept as raw JSON because per-input overlay shape varies (esp. `options` which is a value→label map).
    /// The frontend looks up entries by key — strict typing buys nothing here.
    #[garde(skip)]
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib parses_i18n_with_config_schema_overlay`
Expected: PASS

- [ ] **Step 5: Run the full crate test suite to make sure nothing else regressed**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/protocol/humane_v1.rs
git commit -m "feat(marketplace): I18nLocaleBlock carries config_schema overlay

Phase 3a polish: DHP specs nest config_schema label/description/
placeholder/options translations under i18n.<locale>.config_schema.
Parser was silently dropping them; UI sees the full overlay now."
```

---

### Task 2: Backend — expose full `i18n` map on `MarketplaceItem`

**Files:**
- Modify: `src-tauri/src/automation/marketplace/types.rs:120-148`
- Modify: `src-tauri/src/automation/marketplace/cache.rs:310-335,430-455`
- Modify: `src-tauri/src/automation/marketplace/mod.rs:67-105,250-265` (any place constructing `MarketplaceItem`)
- Test: `src-tauri/src/automation/marketplace/mod.rs` (inline tests)

- [ ] **Step 1: Update the type**

Edit `types.rs` — replace the en-US-only fields with a full map:

```rust
pub struct MarketplaceItem {
    // ... existing fields up to `pub locale: Option<String>,`
    pub locale: Option<String>,
    /// Full per-locale overlay map carried from the registry index entry.
    /// Keys are locale codes (e.g. "zh-CN", "en-US"); values carry name + description.
    pub i18n: std::collections::HashMap<String, EntryI18n>,
}
```

Remove `i18n_name` and `i18n_description` fields.

Update the `From<&RegistryEntry> for MarketplaceItem` impl to copy `e.i18n.clone()` straight through:

```rust
impl From<&RegistryEntry> for MarketplaceItem {
    fn from(e: &RegistryEntry) -> Self {
        Self {
            // ... existing fields
            locale: e.locale.clone(),
            i18n: e.i18n.clone(),
        }
    }
}
```

- [ ] **Step 2: Update both cache row readers in `cache.rs`**

There are two places that build `MarketplaceItem` from a row: in `query_marketplace_cached` (~line 315) and in `get_marketplace_detail_cached` (~line 434). Both currently do:

```rust
let i18n_en = i18n_map.get("en-US");
// ...
i18n_name: i18n_en.and_then(|x| x.name.clone()),
i18n_description: i18n_en.and_then(|x| x.description.clone()),
```

Replace both with:

```rust
i18n: i18n_map,
```

(no need for `i18n_en`; the map is the field).

- [ ] **Step 3: Update `mod.rs` construction sites**

`marketplace/mod.rs` builds `MarketplaceItem` directly in `query_marketplace_cached`'s fallback path and inside `install_marketplace_human`. Both must use the new field name. Search for `i18n_name` / `i18n_description` and convert.

- [ ] **Step 4: Frontend TS type sync**

Edit `ui/src/lib/tauri-bridge.ts:1320-1342`:

```ts
export interface EntryI18n {
  name?: string | null
  description?: string | null
}

export interface MarketplaceItem {
  // ... existing fields
  locale: string | null
  i18n: Record<string, EntryI18n>
}
```

Remove `i18nName` and `i18nDescription` fields.

- [ ] **Step 5: Update existing UI consumers temporarily (compile-only)**

`StoreCard.tsx` and `StoreFeaturedRow.tsx` currently read `item.i18nName ?? item.name`. Until Task 4 wires the locale helper in, swap to a placeholder so the build doesn't break:

```tsx
const displayName = item.name  // TODO Task 4: localize via locale + item.i18n
const displayDesc = item.description
```

This is intentional: Task 4 supplies the proper resolver.

- [ ] **Step 6: Update Rust tests**

Adjust `marketplace_item_resolves_en_us_i18n` and `marketplace_item_handles_missing_i18n` in `mod.rs` to assert on the new shape:

```rust
#[test]
fn marketplace_item_carries_full_i18n_map() {
    let entry: RegistryEntry = serde_json::from_value(seed_entry_with_en_us()).unwrap();
    let item = MarketplaceItem::from(&entry);
    assert_eq!(item.i18n.get("en-US").and_then(|x| x.name.as_deref()), Some("AI Daily News Digest"));
}

#[test]
fn marketplace_item_handles_missing_i18n() {
    let entry: RegistryEntry = serde_json::from_value(seed_entry_no_i18n()).unwrap();
    let item = MarketplaceItem::from(&entry);
    assert!(item.i18n.is_empty());
}
```

- [ ] **Step 7: Build + test**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib marketplace 2>&1 | tail -20
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: zero compile errors, all tests pass, zero TS errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/automation/marketplace/ ui/src/lib/tauri-bridge.ts ui/src/components/automation/StoreCard.tsx ui/src/components/automation/StoreFeaturedRow.tsx
git commit -m "feat(marketplace): MarketplaceItem carries full i18n map

Drop the en-US-only fields and pass the entire i18n HashMap to the
frontend. The locale resolver in the next commit picks the right
overlay; for this commit, UI temporarily falls back to top-level
fields to keep the build green."
```

---

### Task 3: Backend — `marketplace_category_counts` command

**Files:**
- Modify: `src-tauri/src/automation/marketplace/cache.rs` (add function)
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (re-export)
- Modify: `src-tauri/src/tauri_commands.rs` (command + invoke_handler)
- Modify: `src-tauri/src/main.rs` (`invoke_handler!`)
- Modify: `ui/src/lib/tauri-bridge.ts` (bridge function)
- Test: `src-tauri/src/automation/marketplace/cache.rs` inline tests

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `cache.rs`:

```rust
#[test]
fn category_counts_ignore_category_filter_and_respect_type() {
    let conn = setup_test_cache_with_items(&[
        ("social-1", "social", "automation"),
        ("social-2", "social", "automation"),
        ("dev-1", "dev-tools", "skill"),
        ("dev-2", "dev-tools", "skill"),
        ("dev-3", "dev-tools", "automation"),
    ]);

    // all types, no search
    let counts = category_counts_cached(&conn, None, None).unwrap();
    assert_eq!(counts.get("social").copied(), Some(2));
    assert_eq!(counts.get("dev-tools").copied(), Some(3));

    // skill only
    let counts = category_counts_cached(&conn, Some("skill"), None).unwrap();
    assert_eq!(counts.get("dev-tools").copied(), Some(2));
    assert!(counts.get("social").is_none());
}
```

(`setup_test_cache_with_items` may already exist in the test module — if not, add a minimal helper that creates an in-memory DB, runs `init_cache_schema`, inserts rows.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib category_counts_ignore`
Expected: FAIL (function doesn't exist)

- [ ] **Step 3: Implement `category_counts_cached`**

Add to `cache.rs`:

```rust
pub fn category_counts_cached(
    conn: &Connection,
    item_type: Option<&str>,
    search: Option<&str>,
) -> Result<HashMap<String, i64>> {
    let mut sql = String::from("SELECT i.category, COUNT(*) FROM marketplace_items i");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(q) = search {
        sql.push_str(
            " JOIN automation_marketplace_fts \
                ON automation_marketplace_fts.slug = i.slug \
               AND automation_marketplace_fts.registry_id = i.registry_id",
        );
        sql.push_str(" WHERE automation_marketplace_fts MATCH ?");
        params.push(Box::new(q.to_string()));
    } else {
        sql.push_str(" WHERE 1=1");
    }

    if let Some(t) = item_type {
        sql.push_str(" AND i.type = ?");
        params.push(Box::new(t.to_string()));
    }

    sql.push_str(" GROUP BY i.category");

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (cat, n) = row?;
        out.insert(cat, n);
    }
    Ok(out)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib category_counts_ignore`
Expected: PASS.

- [ ] **Step 5: Re-export in `mod.rs`**

Add to `marketplace/mod.rs` (next to the existing re-exports):

```rust
pub use cache::category_counts_cached;
```

- [ ] **Step 6: Add Tauri command**

In `tauri_commands.rs` (near `query_marketplace`):

```rust
#[tauri::command]
pub async fn marketplace_category_counts(
    state: tauri::State<'_, AppState>,
    item_type: Option<String>,
    search: Option<String>,
) -> Result<std::collections::HashMap<String, i64>, Error> {
    let conn = state.db.lock().unwrap();
    crate::automation::marketplace::category_counts_cached(
        &conn,
        item_type.as_deref(),
        search.as_deref(),
    )
    .map_err(|e| Error::Internal(e.to_string()))
}
```

Register in `main.rs` `invoke_handler!` next to `query_marketplace` and `get_marketplace_detail`.

- [ ] **Step 7: TS bridge**

Edit `ui/src/lib/tauri-bridge.ts`:

```ts
export const marketplaceCategoryCounts = (
  itemType?: string,
  search?: string,
): Promise<Record<string, number>> =>
  invoke<Record<string, number>>('marketplace_category_counts', { itemType, search })
```

- [ ] **Step 8: Verify**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head
```

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/automation/marketplace/cache.rs src-tauri/src/automation/marketplace/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "feat(marketplace): marketplace_category_counts command

Backend-side category aggregation that's independent of the active
category filter. Used in the next commit to fix chip count/order
drift when a category chip is clicked."
```

---

### Task 4: Frontend — locale atom + localize helpers

**Files:**
- Create: `ui/src/lib/marketplace-i18n.ts`
- Create: `ui/src/lib/marketplace-i18n.test.ts`
- Modify: `ui/src/atoms/marketplace.ts`

- [ ] **Step 1: Add atom + helper**

Create `ui/src/lib/marketplace-i18n.ts`:

```ts
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

/** Region-tolerant locale picker: zh-CN matches zh; en matches en-US. */
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
```

- [ ] **Step 2: Tests**

Create `ui/src/lib/marketplace-i18n.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { localizeEntry, localizeConfig, localizeOption, pickLocale } from './marketplace-i18n'

describe('marketplace-i18n', () => {
  const i18n = {
    'zh-CN': {
      name: '小红书',
      description: '中文描述',
      config_schema: {
        keywords: {
          label: '监控关键词',
          options: { time_descending: '最新' },
        },
      },
    },
  }

  it('localizeEntry picks the locale name', () => {
    expect(localizeEntry('name', 'English', i18n, 'zh-CN')).toBe('小红书')
  })

  it('localizeEntry falls back to base when locale missing', () => {
    expect(localizeEntry('name', 'English', i18n, 'fr-FR')).toBe('English')
  })

  it('pickLocale is region-tolerant — zh → zh-CN', () => {
    expect(pickLocale(i18n, 'zh')).toEqual(i18n['zh-CN'])
  })

  it('localizeConfig finds nested label', () => {
    expect(localizeConfig('keywords', 'label', 'Search', i18n, 'zh-CN')).toBe('监控关键词')
  })

  it('localizeConfig falls back to base label', () => {
    expect(localizeConfig('unknown', 'label', 'Default', i18n, 'zh-CN')).toBe('Default')
  })

  it('localizeOption resolves overlay value', () => {
    expect(localizeOption('keywords', 'time_descending', 'Latest', i18n, 'zh-CN')).toBe('最新')
  })

  it('localizeOption falls back to base label', () => {
    expect(localizeOption('keywords', 'unknown', 'Most Likes', i18n, 'zh-CN')).toBe('Most Likes')
  })
})
```

- [ ] **Step 3: Add `userLocaleAtom`**

Edit `ui/src/atoms/marketplace.ts`. Add at top of file (after imports):

```ts
import { atomWithStorage } from 'jotai/utils'

function detectInitialLocale(): string {
  if (typeof navigator !== 'undefined' && navigator.language) {
    return navigator.language
  }
  return 'en-US'
}

export const userLocaleAtom = atomWithStorage<string>(
  'uclaw.marketplace.locale',
  detectInitialLocale(),
)
```

(If `atomWithStorage` is already imported from another atom file, reuse it; check for duplicates.)

- [ ] **Step 4: Run tests**

```bash
cd ui && npm test -- --run marketplace-i18n 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/marketplace-i18n.ts ui/src/lib/marketplace-i18n.test.ts ui/src/atoms/marketplace.ts
git commit -m "feat(marketplace): locale atom + localize helpers

userLocaleAtom defaults to navigator.language and persists via
atomWithStorage. localizeEntry/localizeSpec/localizeConfig/
localizeOption resolve DHP-style i18n overlays with region-tolerant
fallback (zh → zh-CN)."
```

---

### Task 5: Frontend — apply localize across the store UI

**Files:**
- Modify: `ui/src/components/automation/StoreCard.tsx`
- Modify: `ui/src/components/automation/StoreFeaturedRow.tsx`
- Modify: `ui/src/components/automation/StoreDetail.tsx`

- [ ] **Step 1: StoreCard**

Replace the temporary fallback with locale-aware resolution:

```tsx
import { useAtomValue } from 'jotai'
import { userLocaleAtom } from '@/atoms/marketplace'
import { localizeEntry } from '@/lib/marketplace-i18n'

// ... inside StoreCard:
const locale = useAtomValue(userLocaleAtom)
const displayName = localizeEntry('name', item.name, item.i18n, locale)
const displayDesc = localizeEntry('description', item.description, item.i18n, locale)
```

- [ ] **Step 2: StoreFeaturedRow**

Same pattern: read `userLocaleAtom`, replace `item.i18nName ?? item.name` and
`item.i18nDescription ?? item.description` with the localize helpers.

- [ ] **Step 3: StoreDetail — title/description**

```tsx
import { useAtomValue } from 'jotai'
import { userLocaleAtom } from '@/atoms/marketplace'
import { localizeEntry, localizeSpec, localizeConfig, localizeOption } from '@/lib/marketplace-i18n'

// ... inside StoreDetail:
const locale = useAtomValue(userLocaleAtom)
const item = detail.item
const spec = detail.parsedSpecJson as any  // parsed HumaneAutomationSpec or null
const specI18n = spec?.i18n as Record<string, any> | undefined

const displayName = localizeSpec('name', item.name, specI18n, locale) || localizeEntry('name', item.name, item.i18n, locale)
const displayDesc = localizeSpec('description', item.description, specI18n, locale) || localizeEntry('description', item.description, item.i18n, locale)
```

(spec-level overlay wins when present; entry-level is the fallback for the brief description shown in lists.)

- [ ] **Step 4: StoreDetail — config_schema preview**

Wherever the detail page renders each `config_schema` entry's label / description / placeholder, route through:

```tsx
const inputLabel = localizeConfig(input.key, 'label', input.label, specI18n, locale)
const inputDesc = localizeConfig(input.key, 'description', input.description, specI18n, locale)
const inputPlaceholder = localizeConfig(input.key, 'placeholder', input.placeholder, specI18n, locale)
```

For `select` inputs, when listing options:

```tsx
const optLabel = localizeOption(input.key, opt.value, opt.label, specI18n, locale)
```

- [ ] **Step 5: Visual check**

Run `cargo tauri dev` (or trust manual verification). With the system in zh-CN, opening Xiaohongshu Keyword Monitor should show:
- Title: 小红书关键词监控
- Description: 被动监控小红书上提及品牌...
- Config "Search Keywords" → "监控关键词"
- Config "Sort Order" options → "最新", "最多点赞", ...

Items without `i18n.zh-CN` keep their English (graceful fallback).

- [ ] **Step 6: Build check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: zero TS errors; same 88 passed / 1 ConnectivityTab pre-existing failure baseline.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/automation/StoreCard.tsx ui/src/components/automation/StoreFeaturedRow.tsx ui/src/components/automation/StoreDetail.tsx
git commit -m "feat(marketplace): localized title/description/config across store UI

StoreCard + StoreFeaturedRow apply entry-level overlays (registry index
i18n). StoreDetail applies spec-level overlays — name, description,
config_schema labels/descriptions/placeholders, and select option
labels — all routed through the region-tolerant locale resolver."
```

---

### Task 6: Frontend — backend-driven category counts

**Files:**
- Modify: `ui/src/components/automation/StoreView.tsx`

- [ ] **Step 1: Replace per-page derivation**

Remove the loop that builds `cats` from `result.items` and the `setCounts((prev) => ({ ...prev, ...cats }))` call. Add a separate effect that depends on `filters.itemType` and `filters.search` (NOT `filters.category`):

```tsx
import { marketplaceCategoryCounts } from '@/lib/tauri-bridge'

// ... inside StoreView, after the existing effects:
React.useEffect(() => {
  marketplaceCategoryCounts(
    filters.itemType === 'all' ? undefined : filters.itemType,
    filters.search || undefined,
  )
    .then(setCounts)
    .catch((err) => console.warn('[StoreView] category counts failed:', err))
}, [filters.itemType, filters.search, setCounts])
```

Also delete the now-unused `cats` block in `loadPage`. `setCounts` should NOT be called from `loadPage` any more.

- [ ] **Step 2: Visual verification**

In dev mode:
1. Open marketplace with no category filter — chip counts and order stable.
2. Click "dev-tools" — count next to dev-tools stays the same; chips don't reorder.
3. Type a search term — counts update; the active filter still works.

- [ ] **Step 3: Build check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head
cd ui && npm test -- --run 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/StoreView.tsx
git commit -m "fix(marketplace): stable chip counts via backend aggregation

Chip counts were derived from the paginated query result, so:
1. \"全部\" showed counts truncated to PAGE_SIZE (dev-tools=2 instead of 8)
2. Clicking a category overwrote that category's count with the
   post-filter count
3. Sort-by-count reordered chips on each click

Switch to marketplace_category_counts which aggregates the entire
post-itemType+post-search corpus. Numbers and order are now stable
across category clicks."
```

---

### Task 7: PR

- [ ] **Step 1: Push and open PR**

```bash
git push -u origin worktree-marketplace-i18n-and-counts
gh pr create --title "feat(marketplace): per-locale overlays + stable category counts" --body "$(cat <<'EOF'
## Summary
- DHP specs encode authoring-language strings at the top level and locale overlays under `i18n.<locale>`. Previously the cache dropped everything except `en-US.{name,description}` and the UI showed English to zh-CN users.
- Category chip counts were derived from the paginated query result, so they were truncated for \"全部\" and overwritten when a chip was clicked — also reordering chips.

## What changes
- Parser: `I18nLocaleBlock` now carries `config_schema` (per-input label / description / placeholder / options overlay).
- DTO: `MarketplaceItem` exposes the full `i18n` map; en-US-only fields removed.
- Frontend: `userLocaleAtom` (atomWithStorage, defaults to `navigator.language`) + `localizeEntry` / `localizeSpec` / `localizeConfig` / `localizeOption` helpers with region-tolerant fallback.
- Store UI: `StoreCard`, `StoreFeaturedRow`, `StoreDetail` route every user-facing string through the resolver.
- New `marketplace_category_counts` Tauri command + frontend wiring; chip counts stable across category clicks.

## Commits (bisectable)
| # | Commit | Scope |
|---|--------|-------|
| 1 | feat(marketplace): I18nLocaleBlock carries config_schema overlay | parser |
| 2 | feat(marketplace): MarketplaceItem carries full i18n map | DTO |
| 3 | feat(marketplace): marketplace_category_counts command | backend |
| 4 | feat(marketplace): locale atom + localize helpers | frontend infra |
| 5 | feat(marketplace): localized title/description/config across store UI | UI |
| 6 | fix(marketplace): stable chip counts via backend aggregation | UI |

## Test plan
- [ ] `cargo test --lib parses_i18n_with_config_schema_overlay` passes
- [ ] `cargo test --lib category_counts_ignore_category_filter_and_respect_type` passes
- [ ] `cargo test --lib marketplace` all green
- [ ] `npm test -- --run marketplace-i18n` 7 cases pass
- [ ] Manually: Xiaohongshu Keyword Monitor detail page renders in Chinese on zh-CN
- [ ] Manually: dev-tools chip count is the same whether \"全部\" or \"dev-tools\" is active; chip order doesn't shift
EOF
)"
```

---

## Self-review

- **Spec coverage:** every spec section maps to a task — i18n parser (Task 1), DTO surface (Task 2), backend counts (Task 3), locale infra (Task 4), UI application (Task 5), counts wiring (Task 6).
- **Placeholders:** none. Each task ships complete code, exact files, exact commands.
- **Type consistency:** `EntryI18n` is the same name across Rust DTO export and TS bridge; `SpecI18nBlock` lives in `marketplace-i18n.ts` and matches the `i18n.<locale>` shape from `humane_v1.rs`; helper names (`localizeEntry`, `localizeSpec`, `localizeConfig`, `localizeOption`) consistent across plan and tasks.
- **Bisectability:** each commit compiles and is independently revertable. Task 2 introduces a temporary plain-`item.name` fallback so the UI builds; Task 5 replaces it. If Task 5 is reverted alone, Task 2 still ships a working build.
