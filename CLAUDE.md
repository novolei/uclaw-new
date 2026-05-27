# CLAUDE.md

Top-level entry file for Claude Code (Cowork, any IDE) working in uClaw.

The full multi-session behavior contract is in **`@BEHAVIOR.md`** — consult it
before non-trivial or policy-sensitive work. Detailed project reference
material is in **`@CONTEXT.md`**. The strategic baseline is
**`@docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`** (Pi-lightweight
product philosophy), which **supersedes the "Agent OS v2" heavyweight positioning** of
`@docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` (retained for history).

**Product philosophy (2026-05-28)**: uClaw is a **Pi-style lightweight, pluggable,
domain-blind agent kernel** serving **everyday/office users + vibe-coding users**.
Kernel stays pure (stateless loop + one `AgentApi` handle + Pi `AgentHarness` layer);
domains/heavy features (Teams, World Projection, Evolution Factory) live **above** the
kernel as optional layers, never as loop branches. Plugins: one handle; third-party code
plugins via subprocess/RPC (MCP generalized); domains as capability presets. Memory:
modernize via openhuman ideas behind one `MemoryAdapter` (detailed gbrain↔openhuman
architecture deferred to a dedicated effort). Borrow Pi (kernel/plugins), openhuman
(memory), hermes (coding edits) — no language migration.

**Agent framework direction**: Any work touching `src-tauri/src/agent/` should consult
the philosophy ADR above + the gap audit `docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md`
(current-state flaws + 5-phase remediation) and the Pi 8-axis design
`docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md`. Already-landed
Pi patterns (dual queues, iterative compaction + split-turn, FileOps) are real; the open
debts are: collapse 4 registries → one handle, one safety chokepoint, thread
`CancellationToken` to flight points, kill dead skeleton, rename eval `harness/`.

Other agents (Codex, Cursor, Copilot, …) get equivalent entry files
(`AGENTS.md`, `.cursorrules`, `.github/copilot-instructions.md`) that point
to the same `BEHAVIOR.md` so behavior stays uniform across sessions.

---

## Milestone Work

If the user mentions "推进主线", "continue main line", M2/M3/M4/M5+ work,
C1/C2/C3, Bundle wire-up, milestone closeout, next slice, or queue-next work,
load `uclaw-milestone-closed-loop` and follow
`docs/agents/milestone-closed-loop.md`.

---

# Part 1 — Working Style

## Surfaces to check before assuming

- **Migration version numbers.** New schema work picks the next free integer in `src-tauri/src/db/migrations.rs` AND must coordinate with any open PR that's claimed a number — see *Active migration registry* in `@CONTEXT.md`. Two PRs reusing the same V-number is the most common merge accident in this repo.
- **The agent loop is pure Rust.** No Claude Code SDK / Anthropic SDK in the agent path. Frontend code that looks SDK-shaped (`SDKMessage`, `useSDKRenderer`, etc.) is Proma-migration leftover — verify it actually executes before relying on it.
- **Pi convergence modules**: new agent work should land in focused modules: `agent/steering.rs` (dual queues), `agent/compaction.rs` (iterative + split-turn), `agent/file_ops.rs` (SessionFileOps), `agent/tools/bash.rs` (RollingTailBuffer). Do not add message-injection logic to `SoftInterruptQueue` — it is deprecated in favor of the dual-queue design.
- **Two storage tables per domain.** Chat lives in `messages`; agent lives in `agent_messages` (the visible conversation) **and** `agent_turns` (per-tool-call breakdown). Search/index/migration work must touch the right one — a typical dev DB has ≫ rows in `agent_messages` and `agent_turns`, often 0 in `messages`.

## Match the codebase shape

When extending a feature that already has a flat shape (e.g. the existing `search_conversations` UNION-of-branches pattern), add another branch in the same file rather than introducing a new abstraction layer. uClaw favors flat enumeration over generic dispatchers — match it.

## Adjacent edits that look like scope creep but aren't

- **New Tauri command** → define in `tauri_commands.rs` AND register in the `invoke_handler!` macro in `main.rs`. Forgetting the macro entry compiles fine but fails at runtime.
- **New background service** → register in the `[Stage 3]` block in `main.rs`.
- **New built-in agent tool** → register in `agent/dispatcher.rs` and, if destructive, in `SafetyManager`.
- **Chat-composer behavior change** → uClaw has **two parallel composers** that wrap the same `RichTextInput`: `ui/src/components/chat/ChatInput.tsx` (Chat mode) and `ui/src/components/agent/AgentView.tsx` (Agent mode). Each owns its own `handlePasteFiles` / `handleDrop` / send wiring. Any paste / drop / attachment / submit behavior change must be applied to **both** files. The shared `RichTextInput` is a [PLACEHOLDER] textarea today — a real TipTap port is scheduled for W4 of the Proma preview port.

Call these out in the commit body so they're not mistaken for scope creep.

## Verification commands

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — backend compile, errors only
- `cd src-tauri && cargo test --lib [filter]` — unit tests (inline `#[cfg(test)]`)
- `cd ui && npx tsc --noEmit 2>&1 | head -10` — TS check
- `cd ui && npm test -- --run 2>&1 | tail -10` — Vitest, jsdom

Bisectability: one logical change per commit. Match the plans in `docs/superpowers/plans/*.md`.

## Workflow

Use risk-scaled planning from `BEHAVIOR.md`: full
`superpowers:brainstorming` → `writing-plans` →
`subagent-driven-development` for high-blast-radius work, and a lightweight
inspect → edit → verify loop for small reversible docs, tests, and hotfixes.
When a full plan is needed, put it in
`docs/superpowers/plans/<feature>.md`.

PR shape: one branch per plan, one commit per plan task, one PR with a `## Commits (bisectable)` table.

### Skill entry points

Beyond the superpowers loop, reach for these at the matching stage:

- **Entering ideation** — `to-prd` (PRD on GitHub) or `grill-me` (stress-test a half-formed plan).
- **Aligning with domain** — `grill-with-docs` challenges a plan against `@CONTEXT.md` + `docs/adr/`.
- **Investigation** — `zoom-out` for system-level context on `automation/`, `memu/`, `proactive/`, `harness/`, `memory_graph/`. `prototype` for throwaway design validation.
- **Planning fan-out** — `to-issues` slices a plan into independently-grabbable GitHub issues.
- **Refactor pass** — `improve-codebase-architecture` hunts consolidation / testability wins.
- **Inbox** — `triage` walks incoming GitHub issues through a state machine.
- **Comms** — `handoff` compacts the current conversation; `caveman` switches to ultra-compressed style.

Overlaps: prefer `superpowers:test-driven-development` over `tdd`, `superpowers:systematic-debugging` over `diagnose`, `superpowers:writing-skills` over `write-a-skill` — unless the mattpocock variant's tighter ritual is clearly the better fit.

## Agent skills

### Issue tracker

GitHub Issues for `novolei/uclaw-new`. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the repo label vocabulary in `docs/agents/triage-labels.md`.

### Domain docs

Single-context repo: `CONTEXT.md`, `BEHAVIOR.md`, and `docs/adr/`. See
`docs/agents/domain.md`.

## Real bugs found mid-task

If you discover a bug outside the current task's scope with a confident root cause and a low-risk fix, spin it off as its own small PR — don't fold it in (scope creep + bisectability loss) and don't leave it for later (it'll get forgotten). If the root cause isn't clear, surface it in your status report rather than patching symptoms.

---

# Quick links

- **Behavior spec (canonical multi-session contract)** → `@BEHAVIOR.md`
- **Project reference (architecture, build, migration registry)** → `@CONTEXT.md`
- **Strategic baseline (Pi-lightweight product philosophy)** → `@docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md` (supersedes the Agent OS v2 North Star → `@docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`, retained for history)
- **Pi-convergence gap audit (flaws + 5-phase remediation)** → `@docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md`
- **License & derivation procedure** → `LICENSE`, `NOTICE`, `docs/THIRD_PARTY.md`
- **Pre-commit hooks (block memory_graph::write, dirs::home_dir for .uclaw, missing SPDX)** → `scripts/git-hooks/README.md`
- **Other IDE entry files** → `AGENTS.md` (Codex), `.cursorrules` (Cursor), `.github/copilot-instructions.md` (Copilot)

<!-- gitnexus:start -->
<!-- gitnexus:keep -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **uclaw-new** (38998 symbols, 64970 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/uclaw-new/context` | Codebase overview, check index freshness |
| `gitnexus://repo/uclaw-new/clusters` | All functional areas |
| `gitnexus://repo/uclaw-new/processes` | All execution flows |
| `gitnexus://repo/uclaw-new/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
