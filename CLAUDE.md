# CLAUDE.md

Top-level entry file for Claude Code (Cowork, any IDE) working in uClaw.

The full multi-session behavior contract is in **`@BEHAVIOR.md`** — read it
once at the start of every non-trivial session. Detailed project reference
material is in **`@CONTEXT.md`**. The strategic baseline is in
**`@docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`**.

Other agents (Codex, Cursor, Copilot, …) get equivalent entry files
(`AGENTS.md`, `.cursorrules`, `.github/copilot-instructions.md`) that point
to the same `BEHAVIOR.md` so behavior stays uniform across sessions.

---

## ⚠️ Milestone work — must follow closed-loop discipline

If the user mentions **"推进主线" / "continue main line" / "M2/M3/M4/M5+
work" / "C1/C2/C3" / "Bundle wire-up" / "milestone closeout" / "next
slice"**, you MUST:

1. **Read SSoT first**: `docs/superpowers/MILESTONE_STATUS.md` (live
   M0-M9 state) — before any code edits
2. **Run drift check**: `./scripts/milestone-drift-check.sh
   --since "1 week ago"` — flag RED/YELLOW alarm in your first reply
3. **Load skill `uclaw-milestone-closed-loop`** — it encodes the rules
4. **Tag every PR** with one of `[M<N>-T<X>]` / `[M<N>-T<X> wire-up]` /
   `[Bundle <N>]` / `[Phase 0.5-T<X>]` / `[Backlog]` — no untagged PRs
5. **Update SSoT after merge**: 1-line edit to MILESTONE_STATUS.md is
   part of the PR, not a follow-up

Spec-first for wire-up: look in `docs/superpowers/specs/` for an
existing spec; if absent, write one (see
`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md` as
template) BEFORE opening a `prep/` branch.

Strategy doc with full reasoning + cutoff criteria:
`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`.

Current state (2026-05-22): C1 = M2 closeout in progress; C2 = M3
wire-up next; C3 = M4 wire-up after. **Strict order — do not start C2
before C1 closes.**

---

# Part 1 — Working Style

## Surfaces to check before assuming

- **Migration version numbers.** New schema work picks the next free integer in `src-tauri/src/db/migrations.rs` AND must coordinate with any open PR that's claimed a number — see *Active migration registry* in `@CONTEXT.md`. Two PRs reusing the same V-number is the most common merge accident in this repo.
- **The agent loop is pure Rust.** No Claude Code SDK / Anthropic SDK in the agent path. Frontend code that looks SDK-shaped (`SDKMessage`, `useSDKRenderer`, etc.) is Proma-migration leftover — verify it actually executes before relying on it.
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

Non-trivial work goes through `superpowers:brainstorming` → `writing-plans` → `subagent-driven-development`, producing a spec in `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` and a plan in `docs/superpowers/plans/<feature>.md`. Skip only for typos, single-line fixes, doc-only changes, or hotfixes with an obvious root cause and a ≤ 1-file fix.

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

## Real bugs found mid-task

If you discover a bug outside the current task's scope with a confident root cause and a low-risk fix, spin it off as its own small PR — don't fold it in (scope creep + bisectability loss) and don't leave it for later (it'll get forgotten). If the root cause isn't clear, surface it in your status report rather than patching symptoms.

---

# Quick links

- **Behavior spec (the canonical 10-practice contract)** → `@BEHAVIOR.md`
- **Project reference (architecture, build, migration registry)** → `@CONTEXT.md`
- **Strategic baseline (Agent OS v2 North Star)** → `@docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- **License & derivation procedure** → `LICENSE`, `NOTICE`, `docs/THIRD_PARTY.md`
- **Pre-commit hooks (block memory_graph::write, dirs::home_dir for .uclaw, missing SPDX)** → `scripts/git-hooks/README.md`
- **Other IDE entry files** → `AGENTS.md` (Codex), `.cursorrules` (Cursor), `.github/copilot-instructions.md` (Copilot)

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **uclaw-new** (34143 symbols, 56576 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

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
