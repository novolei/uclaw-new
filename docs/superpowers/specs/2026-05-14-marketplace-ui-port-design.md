# Marketplace UI Port — Phase 3a Design

**Date**: 2026-05-14
**Status**: Approved (user picked option 1 on 2026-05-14)
**Phase**: 3a (newly carved out of original Phase 3; see [humane-automation-design.md § 10](2026-05-13-humane-automation-design.md#10-phasing--migration-plan))
**Reference**: hello-halo at `/Users/ryanliu/Documents/hello-halo`

---

## 1. Goal

Port hello-halo's marketplace UI (StoreView / StoreCard / StoreGrid / StoreDetail / StoreInstallDialog / AppTypeBadge + 3-tab AppsPage navigation) to uClaw. Match the screenshots the user shared on 2026-05-14:

- **Tab bar at top**: 我的数字人 / 我的应用 / 应用商店
- **Browse view**: search bar + type tabs (全部 / 数字人 / 技能 / MCP) + category chips (Shopping / News / 内容 / Dev Tools / Productivity / Data / Social / Other) + card grid
- **Detail page**: full-page detail with config-schema preview, dependencies (MCP + skills), required logins, collapsible system prompt
- **Install dialog**: dynamic config form + space scope selector + install-progress bar

Phase 3a is **UI port + minimal backend** to support it. Multi-registry / proxy adapters / workspace scan stay in Phase 3b.

## 2. Non-goals (deferred to Phase 3b / 4)

- Multiple registry sources (5 built-ins like hello-halo): single DHP registry only
- Smithery / MCP Registry / SkillHub proxy adapters: not used
- Skill / MCP install (only `type='automation'` is installable; the UI shows skills/MCPs as un-installable previews)
- Local hello-halo workspace scan (`scan_humane_workspace`)
- Full-text search (Phase 4 lands FTS over `automation_specs`)
- Per-spec rating / reviews
- Marketplace publish (uClaw → DHP push)

## 3. Module layout

```
src-tauri/src/automation/marketplace/
├── mod.rs                  # existing — Phase 1 list_humans + install_human; keep
├── halo_adapter.rs         # existing — fetch_index + fetch_spec_yaml with Gitee fallback; keep
├── types.rs                # existing — extend with MarketplaceQuery, MarketplaceDetail
├── cache.rs                # NEW — SQLite sync + query layer (replaces every-call HTTP fetch)
└── update_check.rs         # NEW — version comparison for installed specs

ui/src/
├── views/
│   └── AppsPage.tsx        # NEW — 3-tab container (我的数字人 / 我的应用 / 应用商店)
├── components/automation/
│   ├── AutomationHub.tsx           # existing — becomes the "我的数字人" tab body
│   ├── AppTypeBadge.tsx            # NEW — 4-color type pill + hover tooltip
│   ├── StoreHeader.tsx             # NEW — search input + type tabs + category chips
│   ├── StoreCard.tsx               # NEW — replaces MarketplaceCard
│   ├── StoreGrid.tsx               # NEW — replaces MarketplaceModal grid
│   ├── StoreDetail.tsx             # NEW — full-page detail replacement
│   ├── StoreInstallDialog.tsx      # NEW — dynamic config form + scope + progress
│   ├── MarketplaceCard.tsx         # DELETE (replaced by StoreCard)
│   └── MarketplaceModal.tsx        # DELETE (replaced by StoreView via AppsPage)
├── atoms/
│   └── marketplace.ts              # NEW — storeApps / storeFilters / storeDetail / installProgress
└── lib/
    └── tauri-bridge.ts             # add 4 new commands + types
```

## 4. Database schema (V23 partial)

V23 reserved in Phase 1's spec. This phase claims **part** of it:

```sql
-- V23: marketplace cache (Phase 3a)
-- Phase 3b adds automation_registries for multi-source support.

CREATE TABLE automation_marketplace_items (
    registry_id     TEXT NOT NULL,             -- 'halo' (Phase 3a hard-coded)
    slug            TEXT NOT NULL,
    name            TEXT NOT NULL,
    version         TEXT NOT NULL,
    author          TEXT NOT NULL,
    description     TEXT NOT NULL,
    item_type       TEXT NOT NULL,             -- 'automation' | 'skill' | 'mcp' | 'extension'
    category        TEXT NOT NULL DEFAULT 'other',
    icon            TEXT,
    tags_json       TEXT NOT NULL DEFAULT '[]',
    locale          TEXT,
    min_app_version TEXT,
    size_bytes      INTEGER,
    checksum        TEXT,
    requires_json   TEXT NOT NULL DEFAULT '{}',  -- {mcps: [], skills: []}
    i18n_json       TEXT NOT NULL DEFAULT '{}',
    spec_yaml       TEXT,                       -- cached full YAML (lazy — populated on first detail view)
    updated_at_index TEXT,                      -- ISO from registry's updated_at
    cached_at       INTEGER NOT NULL,           -- our local cache time (epoch ms)
    PRIMARY KEY (registry_id, slug)
);

CREATE INDEX idx_marketplace_type     ON automation_marketplace_items(item_type);
CREATE INDEX idx_marketplace_category ON automation_marketplace_items(category);

-- FTS5 virtual table for search across name + description + tags
CREATE VIRTUAL TABLE automation_marketplace_fts USING fts5(
    slug UNINDEXED,
    registry_id UNINDEXED,
    name,
    description,
    author,
    tags,
    tokenize = 'trigram'
);

CREATE TABLE automation_registry_sync (
    registry_id        TEXT PRIMARY KEY,
    last_synced_at     INTEGER,                 -- epoch ms
    last_etag          TEXT,
    last_modified      TEXT,
    last_error         TEXT,                    -- last sync error msg, NULL when OK
    item_count         INTEGER NOT NULL DEFAULT 0
);
```

FTS5 trigram tokenizer matches Phase 1's `messages_fts` choice for consistency.

## 5. Backend API surface

### 5.1 New Tauri commands

```rust
// query_marketplace — paginated browse with filters
#[tauri::command]
pub async fn query_marketplace(
    state: State<'_, AppState>,
    search: Option<String>,             // FTS query, optional
    item_type: Option<String>,          // 'automation' | 'skill' | 'mcp' | 'extension'
    category: Option<String>,
    page: u32,                          // 0-indexed
    page_size: u32,                     // default 20
) -> Result<MarketplaceQueryResult, Error>;

#[derive(Serialize)]
pub struct MarketplaceQueryResult {
    items: Vec<MarketplaceItem>,        // existing trim-down type, ~15 fields
    total: u32,
    has_more: bool,
}

// get_marketplace_detail — full record incl. cached spec.yaml
#[tauri::command]
pub async fn get_marketplace_detail(
    state: State<'_, AppState>,
    slug: String,
) -> Result<MarketplaceDetail, Error>;

#[derive(Serialize)]
pub struct MarketplaceDetail {
    item: MarketplaceItem,              // base metadata
    spec_yaml: String,                  // fetched on demand
    parsed_spec: Option<HumaneAutomationSpec>,  // None if parse fails (status='needs_review')
    requires_mcps: Vec<String>,
    requires_skills: Vec<String>,
    required_logins: Vec<BrowserLoginEntry>,
    config_schema: Vec<InputDef>,
    system_prompt: String,
    installed_version: Option<String>,  // current uClaw-installed version, None if not installed
}

// install_marketplace_human — UPGRADED Phase 1 command
// Phase 1: install_marketplace_human(registry_url, slug) -> HumaneSpecRow
// Phase 3a: install_marketplace_human(slug, space_id?, user_config?, progress_channel?) -> HumaneSpecRow
//   - registry_url removed (single registry until 3b)
//   - space_id for scoping
//   - user_config = JSON object from StoreInstallDialog's filled form
//   - progress_channel = Tauri event channel name; backend emits {phase, downloaded, total}
#[tauri::command]
pub async fn install_marketplace_human(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    slug: String,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    progress_channel: Option<String>,
) -> Result<HumaneSpecRow, Error>;

// check_marketplace_updates — compare installed versions vs registry
#[tauri::command]
pub async fn check_marketplace_updates(
    state: State<'_, AppState>,
) -> Result<Vec<MarketplaceUpdate>, Error>;

#[derive(Serialize)]
pub struct MarketplaceUpdate {
    slug: String,
    installed_version: String,
    latest_version: String,
}

// refresh_marketplace — force re-sync from registry (bypass cache)
#[tauri::command]
pub async fn refresh_marketplace(state: State<'_, AppState>) -> Result<u32, Error>; // returns item count
```

Phase 1's `list_marketplace_humans` is **deprecated** but kept as a thin wrapper over `query_marketplace(None, Some("automation"), None, 0, 100)` for backward compat. Removed in Phase 3b.

### 5.2 Sync logic

```rust
// marketplace/cache.rs

pub async fn sync_registry(
    conn: &Connection,
    source: &RegistrySource,
    force: bool,
) -> Result<u32> {
    // 1. Load registry_sync state for source.id
    // 2. If !force and last_synced_at < TTL_MS ago, return cached count
    // 3. HTTP GET source.url/index.json with If-None-Match / If-Modified-Since
    // 4. If 304 Not Modified, update last_synced_at + return cached count
    // 5. If 200, parse, upsert each entry into automation_marketplace_items,
    //    delete rows not in new index (handle removals), update FTS,
    //    update registry_sync (etag, last_modified, item_count)
    // 6. Return new item count
}

const TTL_MS: i64 = 60 * 60 * 1000;  // 1 hour

pub fn query_items(
    conn: &Connection,
    search: Option<&str>,
    item_type: Option<&str>,
    category: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<MarketplaceQueryResult> {
    // FTS5 if search present, otherwise plain SELECT with WHERE + LIMIT/OFFSET
    // Sort: featured/popularity first (Phase 3a uses raw cached order),
    //       fallback to updated_at_index DESC
}
```

Sync is **lazy** — triggered by `query_marketplace` when `last_synced_at` is stale, or explicitly by `refresh_marketplace`. No background timer in Phase 3a (Phase 4 may add it).

## 6. Frontend state

```typescript
// atoms/marketplace.ts

interface MarketplaceFilters {
  search: string                                  // debounced 300ms
  itemType: 'all' | 'automation' | 'skill' | 'mcp'
  category: string | null                         // null = all
}

interface MarketplaceState {
  items: MarketplaceItem[]                        // paged accumulator
  page: number                                    // 0-indexed
  hasMore: boolean
  loading: boolean
  loadError: string | null
  filters: MarketplaceFilters
  detail: MarketplaceDetail | null                // current detail view (null = grid view)
  detailLoading: boolean
  availableUpdates: MarketplaceUpdate[]           // from check_marketplace_updates
  installInProgress: { slug: string; phase: string; pct: number } | null
}

// AppsPage tab state (separate atom — used by both Hub and Store)
type AppsTab = 'my-humans' | 'my-apps' | 'store'
export const appsTabAtom = atomWithStorage<AppsTab>('uclaw-apps-tab', 'my-humans')
```

`atomWithStorage` for tab persistence so users return to where they left off.

## 7. Component contracts

### AppsPage.tsx
- 3 tabs at top, content area below
- Tab 1 "我的数字人" → existing `AutomationHub` rendered as a tab body
- Tab 2 "我的应用" → Phase 3a stub: "MCP / 技能 / 扩展 lands in Phase 3b" (or hide tab entirely until 3b)
- Tab 3 "应用商店" → `StoreView` (which internally switches between grid + detail)
- Replaces the LeftSidebar Automations button → MainArea swap pattern (the page itself owns the swap)

### StoreView.tsx
- If `detail != null` → render `StoreDetail`
- Else → `StoreHeader` + `StoreGrid`
- Triggers `loadStore()` on mount + on filter change (debounced)

### StoreHeader.tsx
Three rows:
- Row 1: `<input>` with 300ms debounce → updates `filters.search`
- Row 2: 4 type tabs (All / 数字人 / 技能 / MCP) → updates `filters.itemType`
- Row 3: Horizontal scroll of category chips (Shopping / News / 内容 / Dev Tools / Productivity / Data / Social / Other) → updates `filters.category`

### StoreCard.tsx
Per `MarketplaceItem`:
- Icon string (Phase 3a renders as text, Phase 4 maps to lucide icons)
- Name + version + `AppTypeBadge`
- Author line
- Description (line-clamp-2)
- Up to 3 tags + overflow `+N`
- Click → set `detail.slug` to load detail

### StoreDetail.tsx
8 sections (from hello-halo StoreDetail.tsx):
1. Back button (clears `detail`)
2. Header: icon + name + version + author + `AppTypeBadge` + category
3. Install CTA — disabled for non-automation in Phase 3a
4. Description
5. Config schema preview (read-only field list — labels, types, required indicators)
6. Dependencies — MCP names + skill names (Phase 3a just lists; Phase 3b deep-links)
7. Required logins — amber-tinted row with URL
8. Collapsible system prompt (`<details>` or chevron toggle)
9. Tags
10. Metadata footer (format, min_app_version, updated_at, license, repository link)

### StoreInstallDialog.tsx
Modal triggered by Install CTA from StoreDetail:
- Space scope selector (defaults to currentSpace; `__global__` sentinel for skill/MCP types — but Phase 3a only allows automation, so always space-scoped)
- Dynamic form per `config_schema`:
  - `string` / `text` → `<input>` / `<textarea>`
  - `number` → `<input type=number>`
  - `boolean` → `<input type=checkbox>`
  - `select` → `<select>` with `options` (Phase 3a accepts both `[label, value]` shape from new specs and `{[key]: label}` map shape from older specs)
- Required-field validation before submit
- Submit → `invoke('install_marketplace_human', { slug, space_id, user_config, progress_channel })`
- Subscribe to `progress_channel` events → progress bar
- Success → toast + close + refresh installed list

### AppTypeBadge.tsx
4 color variants matching hello-halo's:
- `automation` → primary (blue)
- `mcp` → blue-500
- `skill` → emerald-500
- `extension` → amber-500

Hover tooltip = one-sentence description. Tooltip direction prop (`up` for in-card, `down` for in-detail-header).

## 8. UX flow

```
User opens app
  → LeftSidebar shows "Automations" button (Phase 1 wiring, unchanged)
  → Click → MainArea swaps to AppsPage (was: only AutomationHub)
  → AppsPage opens to whichever tab user saw last (atomWithStorage)

User clicks "应用商店" tab
  → StoreView mounts
  → If cache stale (>1h): sync_registry() in background, show loading
  → Render StoreGrid with paged items
  → User searches/filters → debounced query_marketplace → grid updates
  → User clicks a card → get_marketplace_detail(slug) → StoreDetail mounts
  → User clicks install → StoreInstallDialog opens
  → User fills config, picks scope, hits Install → install_marketplace_human
  → Progress bar streams via Tauri event channel
  → Done → toast "已安装" → close dialog → installed list refreshes

User clicks "我的数字人" tab
  → Existing AutomationHub renders (Phase 1 behavior unchanged)

User clicks "我的应用" tab (Phase 3a)
  → Empty state: "MCP / 技能 / 扩展 安装支持在 Phase 3b 开放"
```

## 9. Migration impact on Phase 1 code

- `MarketplaceModal.tsx` deleted; `automationPanelOpenAtom` keeps controlling AppsPage visibility
- `MarketplaceCard.tsx` deleted (StoreCard takes over); tests carry over with field updates
- `list_marketplace_humans` Tauri command kept as deprecated wrapper
- `install_marketplace_human` signature changes — frontend has to update too (handled atomically in the same PR)
- `LeftSidebar.tsx` Automation button continues to set `automationPanelOpenAtom`; `MainArea` now renders `AppsPage` instead of `AutomationHub` directly when atom is true
- The `automation_marketplace_items` cache means `list_humans()` is now a SQLite query, not an HTTP call — faster + works offline

## 10. Risk + mitigation

| Risk | Mitigation |
|---|---|
| `automation_marketplace_items` schema diverges from what registries publish | Phase 3a stores extras in `tags_json` / `requires_json` / `i18n_json` raw blobs — schema only types fields hello-halo's StoreCard/Detail need |
| StoreInstallDialog config form fails on un-modeled `InputDef` types | Phase 3a accepts string/number/boolean/select/text; falls back to `<input type="text">` with warning for unknown types |
| ai-daily-news + 32 other live specs have schema drift we haven't surveyed | Last-mile: write a test that installs each of 33 specs from `~/.uclaw/automation/spec_dump/` (gitignored) and asserts `status != 'error'` |
| FTS5 trigram tokenizer rejects Chinese well | Same as Phase 1 messages_fts: trigram works for mixed Chinese/English search at 90%+ recall. Phase 4 may switch to a CJK-aware tokenizer if recall is too low |
| Schema V23 collides with Phase 3b's `automation_registries` table | Phase 3a creates only `automation_marketplace_items` + `automation_marketplace_fts` + `automation_registry_sync`. Phase 3b adds `automation_registries` separately under V23 — no overlap |
| Phase 2 hardening lands after 3a, may touch the same files | Phase 2 is mostly backend (AppRuntimeService timeouts, configurable concurrency). UI port doesn't touch those; merge conflicts unlikely |

## 11. Done criteria (UAT)

- [ ] User clicks LeftSidebar Automations → 3-tab AppsPage opens
- [ ] "应用商店" tab loads 33+ DHP specs visible in grid (with current Gitee fallback working under GFW)
- [ ] Search "新闻" filters to ai-daily-news + relevant entries
- [ ] Type tab "数字人" shows only automation type
- [ ] Category chip "Social" shows only `category=social` entries
- [ ] Click ai-daily-news card → StoreDetail with all 8 sections rendered
- [ ] Config schema preview shows news_topics / max_news_count / news_style / include_commentary / output_language with required-field markers
- [ ] Required-logins section shows the xiaohongshu login row (for the社交类 specs)
- [ ] Install CTA opens dialog with config form
- [ ] Filling form + clicking Install streams progress, ends with success toast
- [ ] Installed spec appears in "我的数字人" tab
- [ ] `check_marketplace_updates` shows update available when registry has newer version
- [ ] Esc closes detail view back to grid (not back to chat)
- [ ] Forward-back ⌘← / ⌘→ navigation works between grid and detail

## 12. Out of scope (Phase 3b / 4)

- Multi-registry management UI (add/remove/toggle/configure)
- Proxy adapters (Smithery / MCP Registry / SkillHub)
- Skill / MCP install
- Local hello-halo workspace scan
- Background sync timer (Phase 3a is lazy-on-query)
- FTS over installed `automation_specs` (Phase 4)
- Avatar / icon rendering as lucide icon (Phase 4)
- Apps "我的应用" tab content (Phase 3b — needs MCP/Skill registries)
- Cross-registry deduplication (multi-registry feature)

---

## 13. uClaw Design DNA — design specifics (added 2026-05-14)

The user explicitly asked: "前端 Markthub 的 UI 可以参考 Hello-halo，但是也需要有 uClaw 的自己的特色，需要遵循整个 app 的 UI 风格并对 UI UX 做进一步的创新和优化".

The bare hello-halo port (sections 5-9 above) would feel pasted-in. This section overlays uClaw design DNA + concrete innovations.

### 13.1 Visual identity contract (non-negotiable)

Following the design audit completed 2026-05-14:

| Rule | What | Why |
|---|---|---|
| **Theme tokens only** | `bg-content-area`, `bg-card`, `text-foreground`, `text-muted-foreground`, `border-border/50`, `text-success / text-warning / text-danger` and their `-bg` variants | uClaw ships 11 themes (warm-paper, qingye, forest-*, etc.). Hardcoded `bg-zinc-X` / `text-gray-X` / `text-green-500` etc. break under 4 of them. Even semantic colors must use tokens — see `--success / --warning / --danger` in globals.css |
| **Radius hierarchy** | Main panel `rounded-2xl`, content cards `rounded-xl`, buttons/pills `rounded-md` or `rounded-full` | Phase 1 AutomationHub used `rounded-lg` (button-sized) on cards — looks slightly off. Match SettingsCard's `rounded-xl` |
| **Shadow restraint** | `shadow-xl` only on the topmost panel; cards use `border-border/50` for separation, NO shadow | Stacking shadows creates depth confusion. Layer hierarchy comes from the `p-2` gap between sidebar+content, not nested shadows |
| **Motion: ≤ 150ms, no bounce** | `transition-colors duration-100` for hover, `motion/react` with `duration: 0.22, ease: [0.32, 0.72, 0, 1]` for dialog/state transitions | Matches the SettingsDialog signature feel — fast, decellerating, no overshoot. Spring physics reserved for spatial workspace switching only |
| **Type scale: literal px** | `text-[28px]` hero, `text-[14px]` body, `text-[13px]` rows, `text-[11px]` group headers, `text-[10px]` meta | uClaw doesn't use Tailwind's semantic text sizes — everything is `text-[Npx]`. Headings `font-semibold`, labels `font-medium`, body unweighted |
| **Padding rhythm** | Container `px-6 py-5`, card rows `px-4 py-3.5`, sidebar items `px-3 py-2`, dense list `px-3 py-1.5` | These exact values come from SettingsCard / SettingsNav patterns |
| **Hover fills** | `hover:bg-muted/60` or `hover:bg-accent/30`, never `hover:bg-gray-100` | `/60` alpha works on every theme surface |
| **Active item pattern** | 2px primary bar at left edge: `absolute left-0 top-1.5 bottom-1.5 w-[2px] bg-primary rounded-r` + `bg-muted text-foreground font-medium` on the item itself | This is SettingsNav's active state — it's our canonical "selected" indicator |

### 13.2 Architectural differences vs. hello-halo

uClaw's marketplace IS NOT a sibling tab to "my apps" with split-pane layout (hello-halo's `AppsPage` pattern). Instead it lives inside the existing **MainArea view-replacement** mechanism (the same one Phase 3a's AutomationHub already uses via `automationPanelOpenAtom`).

| hello-halo | uClaw Phase 3a equivalent |
|---|---|
| `AppsPage.tsx` with top tab bar (我的数字人 / 我的应用 / 应用商店) | Single top sub-nav strip inside the existing MainArea view, integrated where AutomationHub's current header bar sits. No new top-level page. |
| Three full-screen tabs | Three sub-views within the Automation MainArea view, switched by an internal atom (`automationSubviewAtom: 'humans' \| 'apps' \| 'store'`) |
| Store detail = full-page replacement of grid | Store detail = SAME MainArea view, sub-view atom switches `store` → `store-detail` |
| `StoreInstallDialog` = modal | uClaw 3-step **Install Wizard** — replaces detail body in place (each step is a state of the same sub-view), with progress dots and Esc-back navigation |

This means the user always remains anchored to the Automation tab in LeftSidebar — no separate top-of-app navigation. Matches how Settings is structured (one settings dialog, internal nav, no app-level tabs).

### 13.3 uClaw-specific innovations (v1 — included in Phase 3a)

**A. Three-step Install Wizard, not a dialog.** 30% of hello-halo's StoreInstallDialog complexity comes from cramming scope + config form + progress into one modal that resizes. uClaw splits into 3 chronological steps:

```
[1 ●─2─3]  选择空间          [继续 →]
[1─2 ●─3]  填写配置          [← 返回] [继续 →]
[1─2─3 ●]  确认 + 安装        [← 返回] [安装]
```

Step transitions use the standard 0.22s `[0.32, 0.72, 0, 1]` ease. Progress dots are `w-2 h-2 rounded-full` (filled `bg-primary` for current, `bg-muted` for upcoming, `bg-primary/40` for completed). Esc = back, Enter = continue.

**B. Try-install sandbox (the standout feature).** Each store detail page has TWO CTAs: 「正式安装」(commit install) and 「试装到沙盒」(try in sandbox). Try-install creates an ephemeral workspace `试用-{slug}-{timestamp}`, installs the spec there with auto-generated config defaults, and surfaces a banner: "试用中 · 5 分钟后自动清理 · [保留并选择正式空间] [立即丢弃]". This is genuinely novel — none of hello-halo / standard app stores offer it. Implementation: leverages existing workspace creation + AutomationHub manual trigger + auto-cleanup task.

**C. "Featured" row above search.** A 3-card horizontal scrolling row at the top of the store, showing curated picks. Phase 3a hard-codes the featured list (3-5 slugs chosen from the official DHP registry); Phase 4 may make it remote-driven. Each featured card is `w-[320px] h-[180px]` — larger than grid cards, with the icon area more prominent. Marks something special is happening above the routine grid scroll.

**D. Smart filter chips with counts.** Category chips show item count: `Social · 12`, `Productivity · 8`. Counts come from the `query_marketplace` result aggregation. Active chip uses `bg-primary/10 text-primary border border-primary/30`, inactive uses `bg-muted text-muted-foreground border border-border/50`.

**E. Pet awareness on install success.** When install_marketplace_human succeeds, fire a one-shot `chat:pet-celebrate` Tauri event. PetWidget already listens for stream events; add a "celebrate" frame (or reuse existing success animation). This connects the marketplace to uClaw's emotional identity without being intrusive.

**F. Sticky CTA bar on detail page.** The install button stays visible at the top of the detail view as user scrolls through the 8 sections. `sticky top-0 z-10 backdrop-blur-md bg-content-area/95 border-b border-border/50`. Mirrors how the settings dialog's section header stays sticky.

**G. View-tabs at the top of detail page (not all sections stacked).** Instead of hello-halo's 8-section vertical scroll, the detail page has 4 sub-tabs: `概览 / 配置 / 依赖 / 提示词`. Information density per-screen is higher, scrolling is shorter, and the System Prompt (often huge) doesn't dominate the page. Each tab transitions with 0.22s opacity fade.

**H. Empty state copy mirrors WelcomeView's tone.** Not "No apps found", but `市场里还没有匹配的数字员工 — 试试别的关键词，或浏览全部分类`. Warm but action-oriented — the WelcomeView signature.

### 13.4 Deferred to v2 polish (not Phase 3a)

- **Auto-uninstall sandbox after N minutes** — Phase 3a relies on user "保留 / 丢弃" choice; Phase 3b adds the timer
- **Featured row as remote config** — Phase 3a hardcodes
- **In-card dependency tooltip** — Phase 3a shows raw count, hover tooltip in Phase 4
- **Compare mode (2 specs side-by-side)** — Phase 4 polish
- **Recent searches / install history** — Phase 4
- **CJK-aware FTS** — Phase 4 (trigram is good enough for Phase 3a)

### 13.5 Component-level styling reference

Quick-look table for the implementer:

```tsx
// Container (matches MainArea convention)
<div className="bg-content-area rounded-2xl shadow-xl overflow-hidden flex flex-col h-full">

// Sub-nav strip (Humans / Apps / Store)
<div className="flex items-center gap-1 px-6 py-3 border-b border-border/50">
  {tabs.map(t => (
    <button className={cn(
      "relative px-3 py-1.5 text-[13px] rounded-md transition-colors",
      active === t.id
        ? "bg-muted text-foreground font-medium"
        : "text-muted-foreground hover:text-foreground hover:bg-accent/30"
    )}>
      {active === t.id && <span className="absolute left-0 top-2 bottom-2 w-[2px] bg-primary rounded-r" />}
      {t.label}
    </button>
  ))}
</div>

// Featured row (hero band)
<div className="px-6 pt-4 pb-2">
  <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
    今日推荐
  </div>
  <div className="flex gap-3 overflow-x-auto pb-1 -mx-6 px-6">
    {featured.map(item => <FeaturedCard ... />)}
  </div>
</div>

// Card (StoreCard)
<button className={cn(
  "w-full text-left p-4",
  "rounded-xl border border-border/50 bg-card",
  "hover:border-primary/40 hover:bg-secondary/50",
  "transition-colors"
)}>
  <div className="flex items-start justify-between gap-2 mb-1">
    <div className="flex items-center gap-2 min-w-0">
      <div className="w-7 h-7 rounded-md bg-primary/10 flex items-center justify-center text-[11px]">
        {item.icon || '🤖'}
      </div>
      <span className="text-[13px] font-medium truncate">{item.name}</span>
      <AppTypeBadge type={item.appType} />
    </div>
    <span className="text-[10px] text-muted-foreground tabular-nums">v{item.version}</span>
  </div>
  <p className="text-[11px] text-muted-foreground">by {item.author}</p>
  <p className="text-[12px] text-muted-foreground mt-2 line-clamp-2">{item.description}</p>
  {item.tags.length > 0 && (
    <div className="flex flex-wrap gap-1 mt-3">
      {item.tags.slice(0, 3).map(tag => (
        <span className="text-[10px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
          {tag}
        </span>
      ))}
    </div>
  )}
</button>

// Detail sub-nav
<div className="sticky top-0 z-10 backdrop-blur-md bg-content-area/95 border-b border-border/50">
  <div className="flex items-center gap-1 px-6 py-2">
    {['概览', '配置', '依赖', '提示词'].map(tab => ...)}
  </div>
</div>

// Install Wizard step dots
<div className="flex items-center gap-2">
  {[1, 2, 3].map(i => (
    <div className={cn(
      "w-2 h-2 rounded-full transition-colors",
      i === step ? "bg-primary" : i < step ? "bg-primary/40" : "bg-muted"
    )} />
  ))}
</div>
```

### 13.6 Tracking: what to flag during implementation

Each commit's spec-reviewer pass should explicitly check:

1. **No hardcoded color classes.** `grep -nE "bg-(zinc|gray|slate|stone|neutral|green|red|amber|blue|yellow)-[0-9]" ui/src/components/automation/<new file>` returns empty.
2. **Card radius is `rounded-xl`.** Not `rounded-lg`.
3. **No `transition-all` outside the search-input focus animation.**
4. **Theme switch test.** A reviewer should swap to warm-paper + qingye + ocean-light and visually verify nothing breaks.
5. **Motion uses `motion/react` for state transitions, not pure CSS.**
6. **Empty states use the WelcomeView pattern** (centered, dimmed icon, action-link, no PetWidget).
