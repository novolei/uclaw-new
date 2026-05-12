# Skill Recall Closed-Loop — Design

**Date:** 2026-05-12
**Status:** Draft (pending writing-plans)
**Author:** ryan + claude (brainstorming session)

## Background

uClaw has **two skill systems** living side-by-side:

| System | Storage | Source | Provenance |
|---|---|---|---|
| `SkillsRegistry` ([skills.rs](../../src-tauri/src/skills.rs)) | `*/SKILL.md` files under `~/.uclaw/skills/` + project `skills/` | hand-authored Markdown manifests | **builtin** |
| `MemoryGraphStore` learned procedures ([memory_graph/store.rs](../../src-tauri/src/memory_graph/store.rs)) | `memory_nodes` table, `kind=Procedure`, `metadata.skill_type='learned'`; body in `memory_versions.content`; keywords in `memory_keywords` | proactive `skill_extraction` scenario | **learned** |

The PR #58–#71 series built ranking + citation + dedup + keyword indexing on
the **memory_graph** side: `list_top_learned_skills` orders by
`cited_count DESC, usage_count DESC, updated_at DESC`; `bump_skill_usage`
exists; `record_skill_cited` IPC ([tauri_commands.rs:3166](../../src-tauri/src/tauri_commands.rs#L3166))
already increments cited_count via `find_learned_skill_by_normalized_title`;
`SkillCitationChips` UI surfaces citations.

**The gap (this spec closes it):** the agent loop never reaches either
system. `match_skills` / `build_skill_prompt` exist in skills.rs but are
called only from a Tauri command for the Settings UI ([tauri_commands.rs:2211](../../src-tauri/src/tauri_commands.rs#L2211)).
The full memory_graph ranking infrastructure has no consumer in the LLM
prompt path. `grep skill src-tauri/src/agent/dispatcher.rs` returns only
`skills_tokens: 0` — a reserved field that was never populated.

Symptom: a skill extracted at the end of one stock-research session never
influences the next stock-research session, even when keywords match.

## Goals

1. **Make both builtin and learned skills reach the LLM** in a way the
   agent can actually act on, without violating uClaw's probe-first
   philosophy (no auto-injection of full bodies into every turn).
2. **Surface recall in the UI** so the user can audit when and why a skill
   is being used.
3. **Reuse the existing memory_graph ranking + counter infrastructure** —
   no new schema, no parallel counter table.

## Non-goals

- Replacing the existing `SkillCitationChips` flow (PR #60) — kept as-is.
- Auto-creating new skills from successful turns (already handled by the
  proactive scenario; out of scope).
- Migrating builtin (SKILL.md) skills to memory_graph or vice versa —
  the two systems coexist; this spec just exposes both to the agent.
- Semantic vector search. GenericAgent's `skill_search` uses embeddings;
  we use `memory_keywords` LIKE matching for v1 (the existing index
  PR #71 backfilled).

## Reference: GenericAgent's pattern

[ga.py:416-437](file:///Users/ryanliu/Documents/GenericAgent/ga.py) treats
SOPs (`memory/*_sop.md`) as files the agent **discovers and reads via tool
call** when relevant. The framework injects a `[SYSTEM TIPS]` reminder when
the agent reads any path containing `sop` or `memory`, prompting it to
extract key points into working memory; provides `memory/skill_search/` for
semantic search across 105K skill cards.

Philosophy: **agent-driven discovery, system-driven nudges**. This spec
adopts the same shape: a lightweight manifest in the system prompt tells
the agent *what's available*, dedicated tools let it *fetch when needed*,
the UI surfaces *what happened* for the user.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Boot                                                            │
│    SkillsRegistry::discover()         ← unchanged (builtin)     │
│    MemoryGraphStore::list_top_learned ← unchanged (learned)     │
│                                                                  │
│  Per LLM call (ChatDelegate::call_llm in dispatcher.rs)          │
│    system_prompt = base                                          │
│                 + memory_context                                 │
│                 + mode_prompts                                   │
│                 + skills_manifest_block          ← NEW           │
│        where manifest_block = union(                             │
│          SkillsRegistry.list()           [builtin],              │
│          MemoryGraphStore.list_top_learned_skills() [learned]    │
│        )                                                         │
│                                                                  │
│  Agent decides to use a skill:                                  │
│    1. tool skill_search(query, top_k)                           │
│         → searches both systems, merges + scores                │
│         → returns [{name, summary, score, provenance, ...}]     │
│         → backend emits agent:skill-recalled (kind: search)     │
│         → calls bump_skill_usage on each learned-skill hit      │
│         → frontend renders SkillRecallChips                      │
│    2. tool load_skill(name, reason)                             │
│         → returns full prompt body + parameters                  │
│         → backend emits agent:skill-recalled (kind: load)        │
│         → calls bump_skill_usage on the learned-skill node      │
│         → frontend renders SkillRecallChips                      │
│    3. Agent applies, writes "> 应用技能：X — Y" in response     │
│         → existing SkillCitationChips renders (unchanged)        │
│         → recordSkillCited IPC bumps cited_count (unchanged)    │
└──────────────────────────────────────────────────────────────────┘
```

**No new schema.** All counter writes go through existing
`bump_skill_usage` / `record_skill_cited`. Builtin skills don't have
counters in memory_graph (they live on disk) — they always rank first in
the manifest by convention.

## Component design

### 1. Manifest block (system prompt addition)

Appended to `effective_system_prompt()` in
[agent/dispatcher.rs:128](../../src-tauri/src/agent/dispatcher.rs#L128).

```markdown
---

## 你已学习到的技能 (Learned Skills)

下面是你过往会话中沉淀的技能清单。当遇到相关问题时，先用 `skill_search`
查询，再用 `load_skill` 加载完整内容。**不强制使用**——只有当技能确实匹配
当前任务时再调用。

- **writing-assistant** [builtin] — Help refine prose: tone, clarity, structure
- **stock-research-multi-source** [learned · cited 7] — 跨 Yahoo/Macrotrends/StockAnalysis 校验股票财报，HTTP 403 时切换源
- **api-key-blacklisting** [learned · cited 3] — API key 在某端点失败后整域+key 拉黑
- ... (up to 30 entries)

**使用流程**：
1. `skill_search(query: "...", top_k: 3)` → 看摘要决定
2. `load_skill(name: "...", reason: "...")` → 拿完整指引
3. 应用后，在回复末尾用 `> 应用技能：name — 简短原因` 标注（供未来检索）

---
```

**Entry format:** `- **{name}** [{provenance}{cite_segment}] — {summary}`

- `provenance` is literal `learned` or `builtin`.
- `cite_segment` is ` · cited {n}` only when `cited_count > 0`, else absent
  (`cited_count` always 0 for builtin → never shown).
- `summary` is `manifest.description` (builtin) or `metadata.summary`
  (learned, falling back to `title` if summary is absent), truncated to
  100 chars on a word boundary.

**Caps:** top 30 entries OR 1500 manifest tokens, whichever hits first.

If no skills exist in either system, the entire block is omitted (no
empty header in the prompt).

### 2. Ordering

**Builtin skills always emit first**, sorted alphabetically by name.
They're hand-curated and small in number; they always belong at the top
so the agent learns the canonical "always available" set without having
to disambiguate from learned skills.

**Learned skills** follow, using the existing `list_top_learned_skills`
order: `cited_count DESC, usage_count DESC, updated_at DESC`.

Manifest content cached per session for 60s; invalidated on `record_skill_cited` /
`bump_skill_usage` / `SkillsRegistry::reload`.

### 3. Tool: `skill_search`

New built-in agent tool (`src-tauri/src/agent/tools/builtin/skill_search.rs`).
Registered in [agent/dispatcher.rs](../../src-tauri/src/agent/dispatcher.rs)
alongside file/web/shell tools.

```json
{
  "name": "skill_search",
  "description": "Search learned skills by keywords. Returns top-N matches with one-line summaries. Use this when facing a problem similar to one you've solved before — load the full skill content via load_skill afterward if a match looks promising.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Keywords describing the current task / problem (English works better than Chinese)."
      },
      "top_k": {
        "type": "integer",
        "description": "Number of skills to return (default 3, max 10).",
        "default": 3
      }
    },
    "required": ["query"]
  }
}
```

**Implementation strategy:**

1. **Builtin pass:** call `SkillsRegistry::match_skills(query)` (already
   implemented; uses `score_skill` keyword scoring). Take top results.
2. **Learned pass:** tokenize query → for each token call
   `MemoryGraphStore::search_by_keyword(space_id, token)` → union node
   IDs → fetch full nodes → score by (keyword-hit count × 1.0)
   + (cited_count × 0.5) + (usage_count × 0.2) + recency bonus.
3. Merge both lists, sort by score, take top_k.
4. For each learned-skill hit, call `bump_skill_usage(node_ids)`.

**Returns** (JSON array, may be empty):

```json
[
  {
    "name": "stock-research-multi-source",
    "summary": "Cross-validate stock financials across Yahoo/Macrotrends...",
    "score": 0.87,
    "provenance": "learned",
    "cited_count": 7,
    "node_id": "abc-123"
  },
  {
    "name": "writing-assistant",
    "summary": "Help refine prose: tone, clarity, structure",
    "score": 0.42,
    "provenance": "builtin"
  }
]
```

`node_id` is included for learned skills only; `load_skill` uses it for
efficient lookup. Builtin skills are looked up by `name` against the
registry.

**Side effects:**
- `bump_skill_usage` called for each learned-skill hit.
- `agent:skill-recalled` event emitted with `kind='search'`.

### 4. Tool: `load_skill`

New built-in tool (`src-tauri/src/agent/tools/builtin/load_skill.rs`).

```json
{
  "name": "load_skill",
  "description": "Load the full content of a skill. Use after skill_search identifies a promising match. The returned content is the skill's full prompt body — read it, then apply it to the current task.",
  "parameters": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Exact skill name." },
      "reason": {
        "type": "string",
        "description": "One sentence: why you're loading this skill in the current context. Surfaces as a chip in the UI; helps the user audit your reasoning."
      }
    },
    "required": ["name", "reason"]
  }
}
```

**Implementation:**

1. First check `SkillsRegistry::get_loaded(name)` → builtin hit.
2. Otherwise call `find_learned_skill_by_normalized_title(space_id, name)`
   → learned hit; fetch active version via `get_active_version(node.id)`.
3. Errors with `ToolError::Execution("Skill 'X' not found")` if neither
   yields a match.

**Returns:**

```json
{
  "name": "stock-research-multi-source",
  "version": "1.0.0",
  "content": "...full prompt body (Markdown)...",
  "parameters": [
    { "name": "ticker", "type": "string", "required": true, "description": "..." }
  ],
  "provenance": "learned"
}
```

For builtin, `version` comes from `manifest.version` and `parameters`
from `manifest.parameters`. For learned, `version` is the
`memory_versions.id` of the active version and `parameters` is an empty
array (learned skills don't have structured params today).

**Side effects:**
- For learned hits: `bump_skill_usage` called on the node (`load` reinforces
  the same signal as `search` — no new counter field needed in v1).
- `agent:skill-recalled` event emitted with `kind='load'`.

### 5. Event protocol: `agent:skill-recalled`

```typescript
{
  conversationId: string
  toolCallId: string                            // dedup key
  kind: 'search' | 'load'
  timestamp: string                             // RFC 3339
  // search path
  query?: string
  results?: Array<{
    name: string
    summary: string
    score: number
    provenance: 'learned' | 'builtin'
    cited_count?: number
  }>
  // load path
  name?: string
  reason?: string
  provenance?: 'learned' | 'builtin'
}
```

Emitted from inside the tool's `execute()` body, after the search/load
operation completes successfully. The existing `chat:stream-tool-activity`
event still fires for these tools (tools always emit those) — the new
event is **in addition**, carrying the structured payload the UI chip
needs.

Frontend listener writes into a new atom
`skillRecallsMapAtom: Map<sessionId, SkillRecall[]>`. The `SkillRecallChips`
component reads the current session's recalls and renders them below the
most-recent assistant message.

Dedup: `(sessionId, toolCallId)` — tool retry doesn't multi-render.

Lifecycle: in-memory only. Session switch keeps state (consistent with
`liveMessagesMap`); page refresh clears them. The data isn't persisted
because the toolCallId trail in `agent_turns` already serves as the
durable history if we ever want to reconstruct chip UI from DB.

### 6. UI: `SkillRecallChips` component

New file: `ui/src/components/agent/SkillRecallChips.tsx`.

Style parity with [SkillCitationChips.tsx](../../ui/src/components/agent/SkillCitationChips.tsx)
but distinguishable:

| Property | CitationChips (existing) | RecallChips (new) |
|---|---|---|
| When | agent wrote `> 应用技能：X — Y` | agent invoked `skill_search` / `load_skill` |
| Meaning | "I just applied X" | "I'm looking up X" |
| Color | `bg-primary/10 text-primary` | `bg-secondary/15 text-secondary-foreground` |
| Icon | `Sparkles` | `Search` (kind=search) / `BookOpen` (kind=load) |
| Tooltip | LLM's `reason` | search: `query` + count; load: `reason` + provenance |
| Click | opens Settings → 已学技能 | same |
| Dedup key | `(messageKey, citation.title)` | `(sessionId, toolCallId)` |

Mount point in [AgentMessages.tsx](../../ui/src/components/agent/AgentMessages.tsx)
— same `pl-[46px] mt-2` row beneath the assistant message body, sharing
the visual lane with citation chips.

### 7. Counter wiring summary

| Action | Counter | Mechanism |
|---|---|---|
| `skill_search` returns learned skill | `usage_count++` | existing `bump_skill_usage` |
| `load_skill` returns learned skill | `usage_count++` | existing `bump_skill_usage` (same call) |
| Agent writes `> 应用技能：X — Y` | `cited_count++` | existing `recordSkillCited` IPC + memory_graph mutation |

Builtin skills don't have counters — they live on disk and always rank
first regardless of usage. If counter-driven ranking for builtin
becomes important later, that's a separate change (would need a
sidecar table or registry mutation).

## Failure modes

| Scenario | Behavior |
|---|---|
| `skill_search` returns zero matches | Empty array. Chip shows "🔍 搜索 X → 0 命中". Agent decides next move. |
| `load_skill` with unknown name | `ToolError::Execution("Skill 'X' not found")`. Agent sees error, can retry. |
| `bump_skill_usage` failure | Logged via tracing; swallowed (counter is soft signal, never fail the tool). |
| Learned skill exists but active version is missing | `load_skill` returns content from `node.title` + a synthetic body explaining the corruption; this should never happen given the version invariant but the code defends against it. |
| Builtin SKILL.md file deleted between discover and use | `load_skill` returns NotFound. Next `discover()` cleans up registry. |
| Disabled learned skill (`metadata.enabled=false`) | Excluded from `list_top_learned_skills` (existing filter). Manifest skips it. `load_skill` still loads it if the agent passes the exact name (escape hatch). |
| Manifest exceeds 1500 tokens | Score-descending fill stops at budget. Cut-off skills accessible via `skill_search` only. |
| Concurrent `skill_search` / `load_skill` in same turn | Layer 2 spawn boundary already serializes per-tool-call; SQLite WAL handles concurrent counter writes. |

## Philosophy guardrails

**Won't do:**
- Auto-inject full skill bodies into every turn's system prompt
  (violates probe-first; bloats tokens; defeats the purpose of `load_skill`).
- Auto-call `skill_search` on every user message (defeats agent autonomy;
  GenericAgent explicitly nudges via system tips, doesn't force).
- Hide skill failures from the agent (e.g. retrying `load_skill` invisibly).
  Let the agent see the error and decide.
- Add new counter fields (`loaded_count`, etc.) beyond what memory_graph
  already tracks. `usage_count` is enough signal for v1.

## Test coverage

**Rust unit tests:**

- `build_manifest_block` with: empty registry / mixed builtin+learned /
  cite-count present and absent / token cap hit / 30-entry cap hit /
  zero-skill case (no block emitted).
- Manifest ordering: builtin alphabetical → learned by E3 rank.
- `skill_search` tool: keyword hit on builtin / on learned / on both;
  empty query → empty result; disabled learned skill excluded;
  `bump_skill_usage` called on learned hits only.
- `load_skill` tool: builtin success / learned success / unknown name →
  ToolError; deprecated-only version → synthetic body.
- `agent:skill-recalled` event payload shape for both kinds.

**Vitest** (`ui/src/components/agent/SkillRecallChips.test.tsx`):

- Renders one chip per recall.
- `kind='search'` shows query in tooltip; `kind='load'` shows reason.
- Dedup: two events with same `toolCallId` render one chip.
- Empty results don't render.

## Out-of-scope, noted for later

- **Semantic search** (embedding-based) for `skill_search` — current keyword
  scoring works for v1 but won't scale past ~100 learned skills well.
- **Cross-skill conflict detection** — if two skills give contradictory
  advice on the same task, no resolution mechanism exists.
- **Decay** — `cited_count` accumulates forever; eventually older patterns
  drown newer ones. Add half-life decay if/when ranking quality degrades.
- **Builtin skill usage counters** — if builtin skills need ranking too,
  add a sidecar table (deferred).
- **User-facing audit log** — memory_graph already has
  `list_top_learned_skills` exposed to Settings; a "Skill activity" panel
  showing recent recalls would close the observability loop fully.

## Migration impact

- **No schema changes.** All required tables (memory_nodes, memory_versions,
  memory_keywords) exist as of V4.
- Existing `SkillCitationChips` continues to work; no IPC changes.
- No breaking changes to existing tool surface or LLM payload shape.
- The reserved `skills_tokens: 0` field in dispatcher's context-stats
  ([agent/dispatcher.rs:367](../../src-tauri/src/agent/dispatcher.rs#L367))
  becomes actually populated.

## Estimated LOC

| Area | LOC |
|---|---|
| `skills.rs` (manifest builder) | ~90 |
| `memory_graph/store.rs` (optional helper for joined search scoring) | ~40 |
| `agent/tools/builtin/skill_search.rs` (new) | ~160 |
| `agent/tools/builtin/load_skill.rs` (new) | ~110 |
| `agent/dispatcher.rs` (manifest splice, tool registration, skills_tokens) | ~40 |
| Rust tests | ~200 |
| `ui/SkillRecallChips.tsx` + atom + types | ~140 |
| `ui/AgentMessages.tsx` mount | ~10 |
| `ui/useGlobalAgentListeners.ts` listener | ~25 |
| `ui/lib/tauri-bridge.ts` event type | ~15 |
| Vitest | ~100 |

**Total:** ~930 LOC across ~10 files, one PR with ~5 bisectable commits:

1. `feat(skills): manifest builder unifies builtin + learned`
2. `feat(agent): skill_search built-in tool`
3. `feat(agent): load_skill built-in tool`
4. `feat(ui): SkillRecallChips component + atom + listener`
5. `feat(agent): inject skill manifest into system prompt + wire tools`
