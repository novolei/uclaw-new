# gbrain Sprint 2.2.5 + 2.3 + 2.4 — execution hand-off

**Branch:** `claude/sprint-gbrain-agent-loop` (sandbox worktree;
files staged on the main `/Users/ryanliu/Documents/uclaw/` working
tree because the sandbox worktree path isn't visible to file tools).

**Plan doc:** `docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md`

## What's in this hand-off

**Three commits ready** (1, 2, 4 from the plan):

| Commit | Task | Status |
|---|---|---|
| 1 | Sprint 2.2.5a — gbrain init 120s timeout | ✅ Code + test in place |
| 2 | Sprint 2.2.5b — init status enum + diagnostics tab row | ✅ Code in place |
| 4 | Sprint 2.3 — agent system prompt gbrain section | ✅ Code + 3 tests in place |

**Two deferred** to a follow-on PR (3, 5+6):

| Commit | Task | Why deferred |
|---|---|---|
| 3 | Sprint 2.2.5c — embedding base_url health check | Tiny; bundle with 2.4 PR |
| 5+6 | Sprint 2.4 — chat extractor → gbrain put_page | Should validate Sprint 2.3 first |

Rationale for the split: **Sprint 2.3 is the make-or-break commit**.
If agent doesn't actually call `mcp__gbrain__put_page` after this
PR, then Sprint 2.4 (auto-extractor) is layered on a non-functional
foundation. Better to ship 2.2.5a + 2.2.5b + 2.3 first, validate
post-merge that gbrain tools fire from real chat, **then** open the
auto-extractor PR.

## Commit-by-commit file list (Mac side `git add` plan)

### Commit 1 — Sprint 2.2.5a (init timeout)

```bash
git add src-tauri/src/mcp.rs src-tauri/src/main.rs
git commit -F docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_A.txt
```

Touches:
- `mcp.rs`: new `pub const GBRAIN_INIT_TIMEOUT_SECS: u64 = 120`,
  `ensure_bundled_gbrain_initialized` converted to `pub async fn`
  with `tokio::process::Command` + `kill_on_drop(true)` +
  `tokio::time::timeout`. Plus updated unit tests (was sync, now
  `#[tokio::test]`) + new `timeout_pattern_kills_hung_process` test.
- `main.rs`: Stage 3 call site adds `.await`. Comment text edits.

### Commit 2 — Sprint 2.2.5b (init status diagnostics)

```bash
git add src-tauri/src/mcp.rs src-tauri/src/app.rs \
        src-tauri/src/main.rs src-tauri/src/tauri_commands.rs \
        ui/src/components/settings/SystemTab.tsx
git commit -F docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_B.txt
```

Touches:
- `mcp.rs`: new `GbrainInitStatus` enum + `Default` impl
- `app.rs`: new `pub gbrain_init_status: Arc<Mutex<GbrainInitStatus>>`
  field on `AppState` + init in `AppState::new`
- `main.rs`: Stage 3 captures `gbrain_init_status_slot`, writes
  status at each branch (InProgress → Succeeded / Skipped /
  Failed), separate `BundleMissing` write when paths missing
- `tauri_commands.rs`: `SystemDiagnosticsReport.gbrain_init` field
  populated in `get_system_diagnostics`
- `SystemTab.tsx`: new `GbrainInitStatus` type (discriminated
  union), `GbrainInitRow` component, conditional render under the
  gbrain BridgeCard

### Commit 3 — (deferred) Sprint 2.2.5c — embedding URL health check

Not yet implemented. Plan in
`docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md` §1.

### Commit 4 — Sprint 2.3 (agent system prompt section)

```bash
git add src-tauri/src/agent/gbrain_prompt.rs \
        src-tauri/src/agent/mod.rs \
        src-tauri/src/agent/dispatcher.rs \
        src-tauri/src/tauri_commands.rs
git commit -F docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_3.txt
```

Touches:
- `agent/gbrain_prompt.rs` (new file, ~195 lines): module +
  `GbrainKnowledgeSection::render` + 3 unit tests
- `agent/mod.rs`: `pub mod gbrain_prompt;`
- `agent/dispatcher.rs`: new `gbrain_knowledge_block: String` field,
  `set_gbrain_knowledge_block` setter, append in
  `effective_system_prompt` after `learned_profile_block`
- `tauri_commands.rs`: 3 callsites — `send_message` (direct
  render), `send_agent_message` (pre-render before spawn),
  `start_agent_teams` (snapshot into factory closure)

### Commit 5 — Docs

```bash
git add docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md \
        docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_A.txt \
        docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_B.txt \
        docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_3.txt \
        docs/superpowers/handoff/2026-05-18-gbrain-agent-loop-handoff.md
git commit -m "docs(gbrain): Sprint 2.2.5+2.3 plan + commit bodies + handoff"
```

## Pre-flight (current sandbox state)

Important: **the sandbox is on a different branch** than the
`claude/sprint-gbrain-agent-loop` worktree it tried to create. Mac
side needs to first split files cleanly:

```bash
cd ~/Documents/uclaw
git status
# Expected: lots of modified files (some are from earlier codex
# browser work that's unrelated). Tease them apart:
git diff --stat
```

If you see browser-related files modified that you don't recognize,
stash them or commit them to the codex branch first:

```bash
git stash push -m "codex browser work before gbrain agent loop" \
  src-tauri/src/browser/ \
  ui/src/atoms/browser-atoms.ts \
  ui/src/components/agent/BrowserPreviewOverlay.tsx
```

Then create the gbrain branch fresh:

```bash
git checkout main && git pull
git checkout -b claude/sprint-gbrain-agent-loop
```

The gbrain-related modifications from this sandbox session are:

```
src-tauri/src/mcp.rs           # commits 1, 2
src-tauri/src/app.rs           # commit 2
src-tauri/src/main.rs          # commits 1, 2
src-tauri/src/tauri_commands.rs  # commits 2, 4
src-tauri/src/agent/mod.rs     # commit 4
src-tauri/src/agent/dispatcher.rs  # commit 4
src-tauri/src/agent/gbrain_prompt.rs  # commit 4 (NEW)
ui/src/components/settings/SystemTab.tsx  # commit 2
docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md   # commit 5
docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_A.txt               # commit 5
docs/superpowers/handoff/COMMIT_GBRAIN_2_2_5_B.txt               # commit 5
docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_3.txt            # commit 5
docs/superpowers/handoff/2026-05-18-gbrain-agent-loop-handoff.md # commit 5
```

## Verification at each commit

```bash
# After commit 1:
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib mcp::gbrain_init_tests 2>&1 | tail

# After commit 2:
cargo build 2>&1 | grep -E "^error" | head
cd ../ui && npx tsc --noEmit 2>&1 | head

# After commit 4:
cd ../src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::gbrain_prompt 2>&1 | tail
```

## Post-merge validation (the real test)

This is the part Sprint 2.4 depends on. Run AFTER all 5 commits are
in main:

```
1. Restart uClaw. Settings → 系统 → confirm "gbrain init — 已初始化"
   row appears with green dot.
2. Start fresh chat. Say (in Chinese or English):
     "OpenAI released GPT-5 on May 18 2026. Major upgrade over GPT-4."
3. Watch the agent's tool calls in the chat scroll. Within 1-2
   turns, expect to see `mcp__gbrain__put_page` with arguments
   containing slug "openai-gpt-5-release" (or similar) + YAML-
   frontmatter content.
4. Restart uClaw (proves persistence).
5. New chat: "Do you remember GPT-5?"
6. Agent should call `mcp__gbrain__recall` and quote the saved
   page in its answer.
```

**If step 3 doesn't fire**, the gbrain instruction block isn't
landing in the prompt. Debug by:
- Check Settings → 系统 → gbrain row shows tool_count > 0
- Check log around `effective_system_prompt` — temporarily add
  `tracing::info!(prompt = %prompt, "system prompt assembled");`
- Verify `mcp__gbrain__*` actually appears in the LLM's tool table
  (curl to provider API or anthropic CLI dump)

**If step 6 doesn't fire**, prompt is right but the LLM picks
recall less reliably. Tune `GBRAIN_INSTRUCTIONS` const in
`agent/gbrain_prompt.rs` — strengthen the recall trigger phrases
or add more examples.

## What Sprint 2.4 will need (next PR)

Only proceed to Sprint 2.4 after the post-merge validation above
succeeds. The 2.4 design assumes agent reliably calls put_page when
prompted; without that the auto-extractor amplifies a broken path.

Sprint 2.4 design (already in `2026-05-18-sprint-2-3-2-2-5-2-4-plan.md`):
- New module `src/gbrain/chat_extractor.rs` with Haiku-cheap
  extraction returning `Vec<GbrainPageProposal>`
- `ChatDelegate::before_llm_call` (Sprint 2.0 hook) gains a
  parallel gbrain extraction pass next to user-profile extractor
- `cost_records.model LIKE 'gbrain_extract%'` daily budget gating
  (mirrors Sprint 2.1b)
- New `memubot_config.memory_os.gbrain_extractor_enabled` flag,
  **default OFF** until validated
- 3 ChatDelegate::new() callsites wire it (same pattern as Sprint 2.0)

Estimated ~400 LOC + 5 tests. Should be 1-2 days of focused work
once Sprint 2.3 ships and validates.

## Sprint 2.2.5c stays small (~40 LOC)

`tauri_commands.rs::set_embedding_config` adds a 2s HEAD probe to
`<base_url>/v1/models` before persisting. Errors propagate as a
structured error the UI displays. New companion IPC
`test_embedding_endpoint(url)` so the frontend can offer a "Test"
button before save. Could ship with Sprint 2.4 or standalone.

## File index for the whole hand-off

- This doc: `docs/superpowers/handoff/2026-05-18-gbrain-agent-loop-handoff.md`
- Plan: `docs/superpowers/plans/2026-05-18-sprint-2-3-2-2-5-2-4-plan.md`
- Commit bodies (3):
  - `COMMIT_GBRAIN_2_2_5_A.txt`
  - `COMMIT_GBRAIN_2_2_5_B.txt`
  - `COMMIT_GBRAIN_SPRINT_2_3.txt`
