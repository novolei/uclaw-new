# Phase 7E - MCP Stdio Action Boundary

## Context

ADR 2026-05-23 Phase 7 adds Playwright MCP as a second provider lane behind a
feature flag. Phase 7D merged the app-managed sidecar runner: Rust now starts
`current_pack_dir/node/bin/node` with
`current_pack_dir/node_modules/@playwright/mcp/cli.js`, controlled output/profile
directories, and no global npm production path.

This slice moves out of the dry-run lane by adding a real stdio JSON-RPC
boundary against a supervised child process. It remains narrow: uClaw actions
are translated to a fixed internal MCP `tools/call` allowlist, and no raw MCP
tool catalog is exposed to the model.

## ADR Section 18 Questions

1. **What user or agent intent does this serve?**
   - Let a browser task ask for MCP-specific exploratory actions such as
     accessibility snapshots, locator discovery, trace capture, navigate, click,
     and type through a Rust-owned supervisor boundary.

2. **What autonomy level can it run at?**
   - This slice is provider-internal and test-only. It prepares
     supervised/local-first execution but does not route live tasks or raise
     autonomy.

3. **What is the truth source?**
   - `PlaywrightMcpRequestEnvelope` remains the uClaw request contract.
     `PlaywrightMcpSidecarHandle` owns the managed child process, stdin/stdout,
     and launch summary.

4. **What TaskEvents does it emit?**
   - None in Phase 7E. Future Phase 7 slices will route MCP action/artifact
     results through supervisor TaskEvents.

5. **What context does it read, and how is it cited?**
   - It reads only the request envelope, sidecar launch summary, and JSON-RPC
     response from the child process. Model-visible observations are not emitted
     in this slice; result DTOs preserve request id, action kind, tool name, and
     artifact refs for later citation.

6. **What capability cards does it add or consume?**
   - It consumes the existing Playwright MCP capability card and implements the
     backend action boundary for that card. It does not change provider
     selection.

7. **What policy hooks can block it?**
   - Existing envelope gates still block disabled MCP flags, runtime-not-ready
     state, and raw tool exposure. This slice does not add external posting,
     credentials, file upload/download, or hosted egress.

8. **What world projection does the UI render?**
   - None yet. The result DTO carries artifact metadata for a later projection
     slice.

9. **What harness cases prove it works?**
   - Focused Rust tests use a fake app-managed sidecar process to prove
     initialize, initialized notification, fixed `tools/call` translation,
     snapshot/click action mapping, JSON-RPC error mapping, and no `tools/list`
     or generic raw-tool surface.

10. **What is the rollback or disable path?**
    - Revert this PR. Because no task routing, IPC, provider promotion, or
      persistent migrations are added, rollback returns Phase 7 to the Phase 7D
      sidecar runner.

11. **What does it deliberately not own?**
    - It does not promote MCP as a default provider, call real websites, expose
      raw MCP tools to the model, add UI/IPC/DB migrations, emit TaskEvents,
      implement provider parity routing, or modify `agentic_loop.rs` /
      `tauri_commands.rs`.

## Allowed Files

- `src-tauri/src/browser/playwright_mcp_sidecar.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7e-mcp-stdio-action-boundary.md`

## Non-Goals

- No provider promotion or BrowserProvider routing.
- No UI, Tauri IPC, DB migration, or TaskEvent emission.
- No global npm, `npx`, or user-installed Playwright production path.
- No generic MCP client exposed to agent/model code.
- No edits to `agentic_loop.rs` or `tauri_commands.rs` in this slice.

## Impact Targets

- `start_playwright_mcp_sidecar`: LOW impact; direct callers are focused
  sidecar tests, with 0 affected execution flows.
- `PlaywrightMcpSidecarHandle`: LOW impact; only the runner and focused tests
  depend on the struct.
- `PlaywrightMcpAction`: LOW impact; mapping consumes the existing enum without
  changing its public variants.

## Rollback

Revert the single Phase 7E commit. The MCP sidecar can still be launched and
terminated as in Phase 7D, but no stdio action execution boundary remains.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp_sidecar.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit
