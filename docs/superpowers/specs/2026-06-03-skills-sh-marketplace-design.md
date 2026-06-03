# skills.sh-Driven Skill Marketplace Search + Install Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** Agent skill discovery. Replaces the dead GitHub code-search marketplace path
(`skill_marketplace_search`) with the skills.sh registry, which requires no authentication.

## Problem

`skill_marketplace_search` ([`skill_marketplace.rs:150-282`](../../../src-tauri/src/agent/tools/builtin/skill_marketplace.rs)) is a **dead path** — it has never succeeded. It calls GitHub's Code Search API:

```rust
let gh_query = format!("{} filename:SKILL.md", query);
let url = format!("https://api.github.com/search/code?q={}&per_page={}", ...);
let resp = client.get(&url).header("Accept", "application/vnd.github+json").send().await?;
```

with **no `Authorization` header**. GitHub's `/search/code` endpoint **mandates authentication by design** (unlike the anonymous-60/hr REST endpoints), so it always returns `401 Requires authentication`. A code comment at `skill_marketplace.rs:165-167` incorrectly assumes "Public, no auth required for low-rate access". The error path ([`skill_marketplace.rs:187-210`](../../../src-tauri/src/agent/tools/builtin/skill_marketplace.rs)) only explicitly classifies `429`/`403`, so `401` falls through to `UpstreamError` — surfaced to the agent as `[UpstreamError] github search returned 401`.

Two secondary facts:
- The tool's prose advertises a "skills.sh / GitHub" ecosystem, but **there is no real skills.sh integration** — only description/comment strings (`skill_marketplace.rs:4,13,73`). The only live external call is the 401-ing GitHub search.
- **There is no GitHub token anywhere in the codebase** (verified: only redaction-related comments in `mcp/mod.rs`), so a token-based fix would require new secret-storage plumbing.

## Decision (approved 2026-06-03)

- **skills.sh `/api/search` becomes the marketplace search backend** (verified live, no auth). Delete the GitHub code-search path entirely — no token, no fallback.
- **Install stays native Rust, in-process, in `~/.uclaw/`** — reuse the existing `skill_install_from_marketplace` GitHub-fetch logic (which works anonymously today, because git-trees + raw download do NOT require auth — only code-search does). No shelling out to `npx skills add`; aligns with the "zero external runtimes" direction.
- **Local `skill_search` and marketplace tools stay two independent tools** — no `find_skill` façade in this slice.
- **Path resolution lives in its own module** — skills.sh returns `source` + `skillId` but not the in-repo SKILL.md path; resolving it is a distinct concern and gets its own file (per the no-god-files convention).

## Verified facts about skills.sh

**Search** — `GET https://skills.sh/api/search?q=<query>&limit=<n>` (base overridable via `SEARCH_API_BASE`, defaults `https://skills.sh`). No auth, no headers required. Semantic search, results sorted by `installs` descending. Live response shape:

```json
{
  "query": "excel spreadsheet",
  "searchType": "semantic",
  "skills": [
    {
      "id": "skillcreatorai/ai-agent-skills/xlsx",
      "skillId": "xlsx",
      "name": "xlsx",
      "installs": 172,
      "source": "skillcreatorai/ai-agent-skills",
      "isDuplicate": true
    }
  ],
  "count": 1,
  "duration_ms": 349
}
```

Per-skill fields: `id` (`source` + `/` + `skillId`), `skillId`, `name`, `installs` (number — community-acceptance signal), `source` (`owner/repo`), `isDuplicate` (bool — same skill indexed from multiple repos). **No** description / stars / repo-path / install-command fields are returned by `/api/search`.

**Install** — the official CLI uses `npx skills add <source>`, resolving `owner/repo` (or a full URL / in-repo path) to files on GitHub and writing to agent dirs like `~/.claude/skills/<name>/SKILL.md`. We do NOT shell out to it; we replicate the resolve + fetch natively.

**Not used** — the proposed `/api/skills` list endpoint + `/browse` UI ([Issue #426](https://github.com/vercel-labs/skills/issues/426)) is still a proposal; out of scope.

## Architecture — data flow

```
User: "is there a skill for making Excel files?"
      │
      ▼
skill_marketplace_search(query, limit)
      │  GET https://skills.sh/api/search?q=…&limit=N   (no auth, semantic, sorted by installs)
      ▼
candidates [{ skillId, name, source(owner/repo), installs, isDuplicate }]   (deduped)
      │  agent presents candidates + install counts; user picks one
      ▼
skill_install_from_marketplace(source="owner/repo", skill_id="xlsx")   (user-approved, destructive)
      │  ① resolve:  GitHub git-trees API → find ".../xlsx/SKILL.md" dir   (anonymous)
      │  ② fetch:    raw.githubusercontent → download that dir file-by-file (existing logic, anonymous)
      │  ③ write:    ~/.uclaw/skills/_marketplace/<owner>__<repo>__xlsx/
      │  ④ register: re-register with skills registry
      ▼
skill available → local skill_search hits it on subsequent queries
```

Local `skill_search` (learned-skill recall) is untouched and runs in parallel as a separate tool.

## Components

### A. `skill_marketplace_search` — rewrite `query_marketplace`
File: [`skill_marketplace.rs:150-282`](../../../src-tauri/src/agent/tools/builtin/skill_marketplace.rs)

- Endpoint: `api.github.com/search/code` → `{SEARCH_API_BASE}/api/search?q={q}&limit={n}` (`SEARCH_API_BASE` const, default `https://skills.sh`).
- Delete the `filename:SKILL.md` query construction and the entire GitHub code-search request/parse block.
- Deserialize the skills.sh envelope into a typed struct (`SearchResponse { skills: Vec<SearchSkill> }`, `SearchSkill { skill_id, name, source, installs, is_duplicate }` via serde rename).
- Dedup: when `is_duplicate == true`, collapse to the highest-`installs` entry per `skill_id`.
- Return to the agent: `{ skillId, name, source, installs }` per candidate, plus an `install` hint object carrying `source` + `skillId` so the model can call the install tool without re-deriving them.
- Update the tool `description()` to honestly describe skills.sh semantic search ranked by install count; drop the "GitHub" claim.
- Keep `User-Agent: uClaw/0.1`. No auth header.

### B. Path resolution — NEW module `skill_marketplace_resolve.rs`
New file: `src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs`

- `async fn resolve_skill_path(owner, repo, skill_id) -> Result<String, ToolError>`.
- Resolve default branch, then `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` (anonymous; git-trees does not require auth).
- Match a tree entry whose path ends in `/{skill_id}/SKILL.md` (or top-level `{skill_id}/SKILL.md`); return the directory path (everything before `/SKILL.md`).
- 0 matches → `ToolErrorKind::NotFound` with a message telling the agent to try another candidate.
- >1 match → pick the shallowest path (fewest segments); log the ambiguity.
- Unit-testable in isolation against a fixture tree JSON (no network in tests).

### C. `skill_install_from_marketplace` — accept `(source, skill_id)`
File: [`skill_marketplace.rs:296-659`](../../../src-tauri/src/agent/tools/builtin/skill_marketplace.rs)

- Input schema: accept `source="owner/repo"` + optional `skill_id`. When `skill_id` is present, call `resolve_skill_path` (B) to obtain the in-repo path, then proceed through the existing contents-listing → raw-fetch → write → register pipeline unchanged.
- Backward compatibility: still accept a fully-qualified `source="owner/repo/<path>"` with no `skill_id` (existing behavior), so the tool can install a path the user names directly.
- Remains destructive → still gated through `SafetyManager` / user approval (unchanged).
- Install target unchanged: `~/.uclaw/skills/_marketplace/<owner>__<repo>__<tail>/`.

### Registration
No new tool registrations needed — `skill_marketplace_search` and `skill_install_from_marketplace` are already registered ([`registry_build.rs:296-298`](../../../src-tauri/src/agent/tools/builtin/registry_build.rs)). The new `resolve` module is an internal helper, not a registered tool. (If the install tool's input schema gains `skill_id`, confirm the dispatcher picks up the new parameter — no `invoke_handler!`/macro change, these are agent tools not Tauri commands.)

## Error handling

| Condition | Classification | Agent-facing message |
|---|---|---|
| skills.sh unreachable / timeout | `NetworkError` | "marketplace temporarily unavailable" — **no 401 possible** (no auth path) |
| search 0 results | empty result (not an error) | agent falls back to building directly |
| resolve finds no `SKILL.md` | `NotFound` | "could not locate that skill in the repo; try another candidate" |
| git-trees / raw hits anonymous 60/hr cap | `RateLimited` (explicit) | "GitHub rate-limited; retry shortly" — fixes the old bug of misclassifying status codes |
| install dir already exists | existing overwrite/skip logic | unchanged |

## Testing

- **Unit (fixtures, no network):**
  - `query_marketplace` deserialization of the skills.sh envelope (incl. `is_duplicate` dedup).
  - `resolve_skill_path` path-matching against a sample recursive-tree JSON (top-level, nested, 0-match, multi-match shallowest-wins).
- **Manual e2e:** `cargo build`, then real run: search "excel" → pick a candidate → install → confirm `~/.uclaw/skills/_marketplace/…/SKILL.md` written and `skill_search` hits it.

## Out of scope (YAGNI)

- No `find_skill` façade (local + marketplace stay two tools).
- No GitHub code-search fallback (deleted, not retained).
- No GitHub token / secret-storage plumbing (anonymous paths suffice).
- No skills.sh `/api/skills` list endpoint (still a proposal — Issue #426).
- No change to local `skill_search`.

## File-touch summary

| File | Change |
|---|---|
| `src-tauri/src/agent/tools/builtin/skill_marketplace.rs` | rewrite `query_marketplace` (search), extend install input to `(source, skill_id)`, fix error classification, update description |
| `src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs` | **new** — `resolve_skill_path` via git-trees |
| `src-tauri/src/agent/tools/builtin/mod.rs` | declare `pub mod skill_marketplace_resolve;` (next to `pub mod skill_marketplace;` at line 16) |

## References

- skills.sh `/api/search` live response: `GET https://skills.sh/api/search?q=excel%20spreadsheet&limit=5`
- find/search implementation: https://deepwiki.com/vercel-labs/skills/4.4-find-search
- CLI: https://github.com/vercel-labs/skills
- Proposed `/api/skills` endpoint: https://github.com/vercel-labs/skills/issues/426
