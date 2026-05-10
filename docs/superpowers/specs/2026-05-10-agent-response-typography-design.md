# Agent Response Typography & Visual Hierarchy

**Date:** 2026-05-10
**Status:** Spec — pending implementation plan
**Scope:** Visual redesign of `MessageResponse` markdown rendering used by agent assistant messages.

## Problem

Agent response content currently renders via `react-markdown` + Tailwind Typography (`prose prose-sm`) with minimal customization in [`ui/src/components/ai-elements/message.tsx`](../../../ui/src/components/ai-elements/message.tsx). The output looks amateurish:

- Tables are flat HTML with only `bg-muted/40` on `<th>` — no card framing, weak row separation.
- Headings (`##`, `###`) blend into body text — no real hierarchy beyond default prose weights.
- Numbered lists with emoji prefixes (1️⃣ 2️⃣ 3️⃣) render as plain `<ol>` items — no visual chunking.
- Status emojis (✅ ⏳ ❌) appear inline as raw unicode — no semantic styling, looks like raw markdown.
- The whole composition reads like raw markdown rather than a designed document.

## Goals

1. Lift agent responses to feel like polished editorial content (Apple-doc quality).
2. Maintain compatibility with all 11 themes (`warm-paper`, `qingye`, `forest-*`, `ocean-*`, etc.) — no hardcoded colors that break under non-default themes.
3. Zero new dependencies. Stay within `react-markdown` + Tailwind + CSS variables.
4. Surgical change — touch only the markdown rendering surface, not message containers, tool blocks, or message headers.

## Non-Goals

- Redesigning `ChatToolBlock`, `NativeBlockRenderer`, `MessageHeader`, or `MessageActions`.
- Adding new chat features (reactions, pinning, etc.).
- Changing user message rendering (`UserMessageContent`) — only assistant prose.
- Changing the markdown source format the agent emits (server-side prompt unchanged).

## Design Decisions (validated via visual companion)

- **Visual structure:** "Structured Cards" direction — table-card containers, status badges, numbered chips for ordered lists.
- **Typography:** "Apple Editorial" stack — SF Pro Text + PingFang SC, negative letter-spacing on headings, ALLCAPS table headers with wide tracking.
- **Color approach:** Per-theme HSL semantic tokens (`--success`, `--warning`, `--danger`, plus `-bg` variants). Each theme tunes its own harmonious set.
- **Scope:** Only `MessageResponse` markdown rendering pipeline.

## Architecture

Two files change:

| File | Change |
|---|---|
| [`ui/src/components/ai-elements/message.tsx`](../../../ui/src/components/ai-elements/message.tsx) | Add custom markdown components for `table`, `thead`, `tbody`, `tr`, `th`, `td`, `h1`, `h2`, `h3`, `ol`, `li`, `blockquote`, `hr`. Update prose className composition. |
| [`ui/src/styles/globals.css`](../../../ui/src/styles/globals.css) | Add semantic color tokens to `:root`, `.dark`, and each `.theme-*` block. Adjust `.chat-content` font stack and feature-settings. Add `.chat-content`-scoped overrides where Tailwind utilities can't reach (e.g. table internal layout). |

No new files. No new components beyond inline definitions next to existing `MarkdownLink` / `MarkdownPre` / `MarkdownInlineCode`.

## Detailed Spec

### 1. Semantic Color Tokens

Add to every theme block (`:root`, `.dark`, and 9 named theme blocks already in `globals.css`):

```css
--success:    142 65% 35%;
--success-bg: 142 70% 94%;
--warning:    35 85% 38%;
--warning-bg: 40 95% 92%;
--danger:     0 70% 45%;
--danger-bg:  0 80% 96%;
```

Each theme tunes the H/S/L to match its overall palette. Concrete values:

| Theme | success H/S/L | warning H/S/L | danger H/S/L |
|---|---|---|---|
| `:root` (light default) | 142 65 35 / 142 70 94 | 35 85 38 / 40 95 92 | 0 70 45 / 0 80 96 |
| `.dark` | 142 50 60 / 142 35 18 | 40 80 65 / 35 40 18 | 0 65 65 / 0 40 18 |
| `theme-warm-paper` | 95 35 35 / 95 30 92 | 30 65 45 / 30 60 90 | 10 60 45 / 10 60 94 |
| `theme-qingye` (dark) | 165 40 55 / 165 30 18 | 35 65 60 / 35 35 18 | 5 60 60 / 5 35 18 |
| `theme-ocean-light` | 175 50 35 / 175 50 94 | 35 80 40 / 35 85 92 | 350 65 45 / 350 70 95 |
| `theme-ocean-dark` | 175 45 55 / 175 30 18 | 35 75 65 / 35 35 18 | 350 60 60 / 350 35 18 |
| `theme-forest-light` | 145 60 32 / 145 50 92 | 35 85 38 / 40 90 92 | 0 70 45 / 0 75 95 |
| `theme-forest-dark` | 150 50 60 / 150 30 18 | 40 75 60 / 40 35 18 | 0 60 60 / 0 35 18 |
| (other 3 themes) | tuned analogously | | |

Implementation note: the implementing engineer should sample each theme's `--primary`/`--accent` and shift `--success` toward analogous green, `--warning` toward analogous amber/orange, `--danger` toward analogous red, keeping perceptual lightness consistent within the theme. Final values picked during implementation, not at spec time.

### 2. `.chat-content` Base Typography

Update existing `.chat-content` block in `globals.css`:

```css
.chat-content {
  font-family:
    -apple-system, BlinkMacSystemFont, "SF Pro Text",
    "PingFang SC", "Helvetica Neue", "Microsoft YaHei", sans-serif;
  font-size: 15px;
  line-height: 1.65;
  letter-spacing: 0.005em;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  font-feature-settings: "tnum", "ss01";
  text-rendering: optimizeLegibility;
}
```

Existing `data-chat-font-size="sm|md|lg"` rules continue to apply. Existing `data-chat-serif="true"` toggle continues to swap to the serif stack — no change.

### 3. Heading Components (in `message.tsx`)

```tsx
const MarkdownH1 = ({ children }) => (
  <h1 className="flex items-center gap-2.5 text-[22px] font-semibold tracking-[-0.015em]
                 mt-7 mb-3.5 first:mt-0 leading-[1.3]">
    <span className="w-[3px] h-[18px] bg-foreground rounded-sm shrink-0" aria-hidden />
    <span>{children}</span>
  </h1>
)

const MarkdownH2 = ({ children }) => (
  <h2 className="flex items-center gap-2.5 text-[19px] font-semibold tracking-[-0.012em]
                 mt-[22px] mb-3 first:mt-0 leading-[1.35]">
    <span className="w-[3px] h-4 bg-foreground rounded-sm shrink-0" aria-hidden />
    <span>{children}</span>
  </h2>
)

const MarkdownH3 = ({ children }) => (
  <h3 className="text-[16px] font-semibold mt-[18px] mb-2 first:mt-0 leading-[1.4]
                 tracking-[-0.005em]">
    {children}
  </h3>
)
```

The `flex` wrapper on h1/h2 means children are inlined into the second `<span>`. We accept that `react-markdown` passes children as React nodes; this works for typical heading content (text + inline formatting). Anchor links inside headings render fine.

### 4. Table Components

```tsx
const MarkdownTable = ({ children }) => (
  <div className="my-3 rounded-[10px] border border-border overflow-hidden bg-card">
    <table className="w-full border-collapse">{children}</table>
  </div>
)

const MarkdownThead = ({ children }) => (
  <thead className="bg-muted/50">{children}</thead>
)

const MarkdownTh = ({ children }) => (
  <th className="text-left px-3.5 py-2.5 text-[11.5px] font-semibold uppercase
                 tracking-[0.06em] text-muted-foreground border-b border-border">
    {children}
  </th>
)

const MarkdownTr = ({ children }) => (
  <tr className="[&:not(:last-child)>td]:border-b [&>td]:border-border/40">{children}</tr>
)

const MarkdownTd = ({ children }) => {
  const text = extractText(children)
  const badge = detectStatusBadge(text)
  if (badge) return <td className="px-3.5 py-3 text-[14.5px]">{badge}</td>
  return <td className="px-3.5 py-3 text-[14.5px]">{children}</td>
}
```

`detectStatusBadge` uses regex on the cell's plain text:

| Pattern | Variant |
|---|---|
| `/✅\|✓\|已完成\|completed\|done/i` | `success` |
| `/⏳\|未完成\|in progress\|pending\|wip/i` | `warning` |
| `/❌\|✗\|未开始\|failed\|error\|尚未/i` | `danger` |

Returns:

```tsx
<span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[12px]
                 font-medium bg-[hsl(var(--success-bg))] text-[hsl(var(--success))]">
  {originalText}
</span>
```

(swap token names per variant). If no pattern matches, `td` renders `children` unchanged.

`extractText` walks the React children tree and collects text nodes — implemented as a small helper next to `MarkdownTd`.

### 5. Ordered List → Numbered Cards

```tsx
const MarkdownOl = ({ children }) => (
  <ol className="list-none pl-0 my-3 space-y-2 counter-reset-[ordered]">{children}</ol>
)

const MarkdownLi = ({ children, node }) => {
  const parent = node?.position?.parent
  const isOrdered = parent?.tagName === 'ol' // see implementation note
  if (!isOrdered) {
    return <li className="leading-[1.7]">{children}</li>
  }
  return (
    <li className="flex gap-3 items-start p-3.5 my-2 bg-muted/40 rounded-[10px]
                   counter-increment-[ordered]
                   before:content-[counter(ordered)] before:w-6 before:h-6 before:shrink-0
                   before:bg-foreground before:text-background before:rounded-md
                   before:flex before:items-center before:justify-center
                   before:text-[12.5px] before:font-semibold before:tabular-nums">
      <div className="flex-1 min-w-0 [&>p:first-child]:mt-0 [&>p:last-child]:mb-0">
        {children}
      </div>
    </li>
  )
}
```

**Implementation note:** `react-markdown` does not pass parent info via `node.position.parent`. The implementing engineer must use one of:

1. A React Context set by `MarkdownOl` that `MarkdownLi` reads to know it's inside an ordered list.
2. CSS-only solution: render all `<li>` with the card style only when inside `<ol>`, via a `.chat-content ol > li { ... }` rule scoped in `globals.css`. Bullet `<ul>` lists keep the simpler styling.

Recommendation: option 2 (CSS-only) — simpler, no Context plumbing. The `before:counter` reset is set on `<ol>`, `before:counter-increment` on `<li>`. Move the long Tailwind chain into a single `.chat-content ol > li` block in `globals.css` for readability.

Unordered lists (`<ul>`) keep close to default prose: `list-disc pl-5 space-y-1`.

### 6. Other Elements

```tsx
const MarkdownBlockquote = ({ children }) => (
  <blockquote className="my-3 pl-3 border-l-2 border-foreground/20 text-foreground/75
                         not-italic [&>p]:my-1">
    {children}
  </blockquote>
)

const MarkdownHr = () => (
  <hr className="my-6 border-0 border-t border-border/60" />
)
```

### 7. Updated `MessageResponse` Composition

```tsx
const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1, h2: MarkdownH2, h3: MarkdownH3,
  table: MarkdownTable, thead: MarkdownThead,
  tr: MarkdownTr, th: MarkdownTh, td: MarkdownTd,
  blockquote: MarkdownBlockquote,
  hr: MarkdownHr,
  // ol/li handled via CSS-only path in globals.css; no JS components needed
} as const
```

The outer `<div>` className simplifies — most prose-* utility classes are now redundant because the custom components carry their own styling. Keep `prose prose-sm dark:prose-invert max-w-none` only as a fallback for unhandled elements (em, strong, ul, p). Drop the table-/heading-/blockquote-specific prose classes that are now overridden.

### 8. Font-size Scale Coupling

The existing `data-chat-font-size` rules in `globals.css` already scale `p`, `li`, `td`. Extend the rules to scale the new typography:

```css
html[data-chat-font-size="sm"] .chat-content h1 { font-size: 19px; }
html[data-chat-font-size="sm"] .chat-content h2 { font-size: 17px; }
html[data-chat-font-size="sm"] .chat-content h3 { font-size: 14.5px; }
html[data-chat-font-size="sm"] .chat-content th { font-size: 10.5px; }
html[data-chat-font-size="sm"] .chat-content td { font-size: 13px; }
/* lg: 1.13x of base — h1 25px, h2 22px, h3 18px, th 13px, td 16.5px */
```

## Testing

New file: `ui/src/components/ai-elements/message.test.tsx` (Vitest + React Testing Library + jsdom).

Test cases:

1. Renders markdown table inside a `<div class="...rounded-[10px] border...">` wrapper.
2. `<th>` content gets `uppercase` class and `text-muted-foreground`.
3. Cell text `"✅ 已完成"` produces an element with class containing `bg-[hsl(var(--success-bg))]`.
4. Cell text `"⏳ 未完成"` produces a warning badge.
5. Cell text `"❌ 尚未开始"` produces a danger badge.
6. Cell text without a status pattern renders children verbatim.
7. Markdown `## Heading` renders an `<h2>` with the accent bar `<span>` and the expected text.
8. Markdown `1. First\n2. Second` renders an `<ol>` whose children pick up the numbered-card class via CSS (assert structural ol > li, not visual styles).
9. Existing tests for `MarkdownLink` external-URL handling and `MarkdownInlineCode` continue to pass.

Use `renderWithProviders` from `ui/src/test-utils/render.tsx`.

## Migration / Rollout

- Single-PR change. Per CLAUDE.md "one commit per plan task" — implementation plan should split into ~3 commits:
  1. Add semantic color tokens to all 11 theme blocks in `globals.css`.
  2. Add custom markdown components and CSS rules in `message.tsx` + `globals.css`.
  3. Tests + screenshot review.
- No data migration. No backend change.
- No feature flag — visual-only change, fully reversible by revert.

## Open Questions for Implementation

- Exact HSL values for the 11 themes' semantic tokens. The spec gives a starting set; implementer should preview each theme and adjust to taste.
- Whether to use `counter-reset` / `counter-increment` (CSS counters) vs `<li>` index from react-markdown's `node.index` prop. CSS counters chosen for simplicity and zero JS overhead.
- Whether to keep `prose` classes on the outer div or remove entirely. Recommend keeping minimal `prose prose-sm dark:prose-invert max-w-none` for fallback styling of `<em>`, `<strong>`, `<ul>`, `<p>` margins.

## Out of Scope (explicit)

- Tool block visuals (`ChatToolBlock`).
- User message styling.
- `MessageHeader` / model logo / timestamps.
- Streaming indicator dots.
- `MarkdownCodeBlock` (kept as-is — already custom and looks fine).
- Math rendering (KaTeX).

## Acceptance Criteria

1. Re-render the screenshot's "请求建议" content. Tables show as cards with rounded borders. Numbered list items render as gray-bg cards with black numeric chips. ✅/⏳/❌ in table cells render as colored pills.
2. Switch through all 11 themes — no hardcoded color leaks. Status badges remain readable in every theme.
3. `data-chat-font-size="sm|md|lg"` scales all typography (h1/h2/h3/th/td) proportionally.
4. `data-chat-serif="true"` swaps to serif stack — code stays mono.
5. Vitest suite passes.
6. No regression in `ChatToolBlock`, `NativeBlockRenderer`, or user message rendering.
