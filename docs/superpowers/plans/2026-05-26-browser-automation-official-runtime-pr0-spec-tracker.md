# Browser Automation Official Runtime PR0 Spec And Tracker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the official Playwright runtime design, update the Browser Runtime tracker, and mark the old runtime-pack-first strategy as superseded.

**Architecture:** This PR is docs-only. It records the strategic change from app-managed runtime pack as default truth to official Playwright CLI/MCP integration managed by uClaw discovery/setup/adapters.

**Tech Stack:** Markdown docs, Browser Runtime ADR/tracker docs.

---

## File Structure

- Modify: `docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md`
- Modify: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Modify: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

## Task 1: Finalize The Design Spec

**Files:**
- Modify: `docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md`

- [ ] **Step 1: Read the spec**

Run:

```bash
sed -n '1,260p' docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md
```

Expected: the spec includes official CLI/MCP targets, Node bootstrap policy, runtime-pack removal, MCP manager reuse, Playwright built-in skills, and ADR Section 18 answers.

- [ ] **Step 2: Search for placeholders**

Run:

```bash
rg -n "TBD|TODO|FIXME|placeholder|later maybe" docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md
```

Expected: no matches.

- [ ] **Step 3: Verify ADR Section 18 coverage**

Run:

```bash
rg -n "What user intent|What autonomy level|canonical truth source|TaskEvent|capability cards|policy hooks|world projection|harness cases|rollback|deliberately not own" docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md
```

Expected: matches for all eleven ADR Section 18 questions.

## Task 2: Update The Browser Runtime Tracker

**Files:**
- Modify: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`

- [ ] **Step 1: Add a Quick View row**

Add a row after `Control Center PR4`:

```markdown
| Official Runtime Simplification | Official Playwright CLI/MCP runtime | Spec planned | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-automation-official-runtime-spec` / `codex/browser-automation-official-runtime-spec` | Replace runtime-pack-first readiness with official Playwright CLI/MCP discovery, setup, and adapter routing. |
```

- [ ] **Step 2: Add a section**

Append this section before `Post-Completion Real-State Correction`:

```markdown
---

## Official Runtime Simplification - Official Playwright CLI/MCP Runtime

- Entry criteria: the architecture review at
  `/private/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/browser-runtime-architecture-review-20260526-103310.html`
  found that the Browser Runtime implementation had two unnecessary truths:
  app-managed runtime pack readiness and a custom Playwright MCP sidecar client.
- Design:
  `docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md`.
- Scope: make official `@playwright/cli@latest` plus
  `playwright-cli install --skills` the CLI lane, make official
  `npx @playwright/mcp@latest` a built-in uClaw MCP server through the existing
  `McpManager`, and remove runtime-pack readiness as the default provider gate.
- Explicit non-scope: no silent sudo, no Homebrew installation, no Linux/Windows
  Node bootstrap, no hosted provider changes, no browser identity redesign, and
  no raw Playwright MCP tools in the ordinary Agent tool pool.
- Planned PRs:
  1. PR0 spec and tracker alignment.
  2. PR1 Playwright system discovery and setup.
  3. PR2 runtime-pack product truth removal.
  4. PR3 Playwright MCP via existing `McpManager`.
  5. PR4 Browser Runtime Adapter routing and sidecar deletion.
  6. PR5 Control Center and built-in Playwright skills integration.
```

- [ ] **Step 3: Verify tracker wording**

Run:

```bash
rg -n "Official Runtime Simplification|runtime-pack readiness|McpManager|playwright-cli install --skills" docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
```

Expected: all new tracker phrases are present.

## Task 3: Update The Old ADR Status

**Files:**
- Modify: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

- [ ] **Step 1: Update status line**

Replace:

```markdown
- **Status:** Accepted as strategy direction; implementation not started.
```

with:

```markdown
- **Status:** Partially implemented, then superseded for Playwright runtime distribution by `docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md`.
```

- [ ] **Step 2: Add supersession note**

Add after the metadata block:

```markdown
> **Supersession note, 2026-05-26:** The Browser Runtime Supervisor, provider
> policy, identity, artifact, and routing principles remain active. The default
> Playwright runtime distribution strategy changed: uClaw now targets official
> `@playwright/cli@latest` plus `playwright-cli install --skills` for the CLI
> lane, and official `npx @playwright/mcp@latest` through the existing uClaw
> `McpManager` for MCP. The app-managed runtime pack is no longer the default
> CLI/MCP readiness truth.
```

- [ ] **Step 3: Verify ADR update**

Run:

```bash
sed -n '1,35p' docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md
```

Expected: status line and supersession note are visible.

## Task 4: Verify And Commit

**Files:**
- Modify: all PR0 docs.

- [ ] **Step 1: Run doc checks**

Run:

```bash
git diff --check -- docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md
```

Expected: no output.

- [ ] **Step 2: Review diff**

Run:

```bash
git diff -- docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md
```

Expected: docs-only diff, no code changes.

- [ ] **Step 3: Commit**

Run:

```bash
git add docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md
git commit -m "docs(browser-runtime): adopt official Playwright runtime design"
```

Expected: commit succeeds.
