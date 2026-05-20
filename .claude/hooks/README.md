# `.claude/hooks/` — In-Session Policy Hooks

These scripts run inside Claude Code at the moment a tool call is about to
execute (`PreToolUse`). They are the **fast feedback layer** that mirrors
`scripts/git-hooks/checks/` — the git layer is the safety net; the in-session
layer is the conversation correction.

## Why two layers

| Layer | When it runs | What it does | Bypass |
|---|---|---|---|
| `.claude/hooks/*.sh` | Before `Edit/Write/MultiEdit` executes | Block bad edits before they hit disk | Stop using Claude (no `--no-verify` equivalent in-session) |
| `scripts/git-hooks/checks/*.sh` | Before `git commit` | Block bad code from being committed | `git commit --no-verify` (emergency only) |

Both layers exist because the in-session one is faster (the model sees the
block immediately and can self-correct) but only catches what passes through
Claude Code. The git layer catches edits made by any other tool (Codex, Cursor,
manual edits).

## Hooks in this directory

| Script | Tool match | Blocks (exit 2) / Warns (exit 0) | Mirror of |
|---|---|---|---|
| `check-memory-graph.sh` | `Edit\|Write\|MultiEdit` | Blocks new `memory_graph::write/insert/update/delete` calls in `src-tauri/src/*.rs` (exempt: `src-tauri/src/memory_graph/`) | `scripts/git-hooks/checks/check-memory-graph-freeze.sh` |
| `check-uclaw-home.sh` | `Edit\|Write\|MultiEdit` | Blocks new `dirs::home_dir().*".uclaw"` patterns (exempt: `crates/uclaw-utils-home/`) | `scripts/git-hooks/checks/check-dirs-home-dir-uclaw.sh` |
| `check-codex-spdx.sh` | `Write` only | Blocks new files under `crates/uclaw-utils-*/src/` without an SPDX header | `scripts/git-hooks/checks/check-codex-derived-spdx.sh` |
| `warn-dmz-edit.sh` | `Edit\|Write\|MultiEdit` | **Non-blocking advisory** on DMZ files (`agentic_loop.rs`, `tauri_commands.rs`, `db/migrations.rs`, root `Cargo.toml`, `CLAUDE.md`, `BEHAVIOR.md`) | (no git equivalent — DMZ is review discipline, not lint) |

## Hook contract

Each script receives JSON on stdin from Claude Code:

```json
{
  "session_id": "...",
  "transcript_path": "...",
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/abs/path",
    "old_string": "...",
    "new_string": "..."
  }
}
```

Exit codes:

| Exit | Meaning |
|---|---|
| `0` | Allow the tool call. `stderr` is shown as advisory text. |
| `2` | **Block** the tool call. `stderr` is shown as the block reason and Claude must self-correct. |
| any other non-zero | Hook error (treated as advisory — does not block). |

## How to add a hook

1. Drop a `<name>.sh` here, `chmod +x`, follow the contract above.
2. Register it in `.claude/settings.json` under `hooks.PreToolUse[]`.
3. If it's blocking, mirror it in `scripts/git-hooks/checks/` so non-Claude
   tools (Codex, Cursor, manual edits) are also covered.
4. Document the policy it enforces (ADR section, BEHAVIOR.md rule, etc.) in
   the script header — drive-by readers should know *why*, not just *what*.

## Smoke testing a hook

```bash
echo '{"tool_name":"Edit","tool_input":{"file_path":"/path/to/file.rs","new_string":"memory_graph::write_node()"}}' \
  | .claude/hooks/check-memory-graph.sh
echo "exit: $?"
```

A correct blocking hook prints the block reason to stderr and exits 2.

## Disabling locally (not recommended)

If a hook produces a false positive and you need to ship before fixing the
hook, override in `.claude/settings.local.json` (gitignored). Add `null` for
the matcher to remove team config:

```json
{
  "hooks": {
    "PreToolUse": []
  }
}
```

Tell the DRI in the PR description so the hook can be tightened.

## See also

- `BEHAVIOR.md` §9 — Hooks Enforce What Spec Says
- `scripts/git-hooks/README.md` — git-layer mirror
- `docs/THIRD_PARTY.md` — SPDX header policy
- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` §11.2 — memory_graph freeze
