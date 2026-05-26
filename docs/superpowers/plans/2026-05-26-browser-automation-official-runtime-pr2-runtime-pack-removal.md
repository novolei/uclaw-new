# Browser Automation Official Runtime PR2 Runtime Pack Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the runtime-pack product concept from default Browser Automation readiness, IPC, and Settings UI while preserving Local Chromium fallback.

**Architecture:** Runtime-pack files are no longer the default truth for Playwright CLI/MCP readiness. Provider readiness consumes official Playwright discovery/setup status. Any reusable execution-report primitive moves under Playwright setup modules in later code, not under `runtime_pack`.

**Tech Stack:** Rust Browser Runtime status/control-center modules, React settings UI, TypeScript bridge tests.

---

## File Structure

- Modify: `src-tauri/src/browser/runtime_contracts.rs`
- Modify: `src-tauri/src/browser/runtime_status.rs`
- Modify: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: `src-tauri/src/browser/runtime_provider_probe.rs`
- Modify: `src-tauri/src/browser/runtime_pack_ipc.rs`
- Modify: `ui/src/lib/startup/startup-doctor.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Test: existing Rust/UI tests around these modules.

## Task 1: Remove Runtime Pack Requirement From Provider Cards

**Files:**
- Modify: `src-tauri/src/browser/runtime_contracts.rs`
- Test: `src-tauri/src/browser/runtime_contracts_tests.rs`

- [ ] **Step 1: Write failing test expectation**

Update `src-tauri/src/browser/runtime_contracts_tests.rs` so CLI and MCP no longer require runtime pack:

```rust
let cli = browser_provider_capability_card("browser.playwright_cli").unwrap();
assert!(!cli.requires_runtime_pack);
assert_eq!(cli.data_boundary_policy, "official_playwright_cli");
assert_eq!(cli.cost_policy, "system_npm_global_cli");

let mcp = browser_provider_capability_card("browser.playwright_mcp").unwrap();
assert!(!mcp.requires_runtime_pack);
assert_eq!(mcp.data_boundary_policy, "official_playwright_mcp");
assert_eq!(mcp.cost_policy, "system_npx_mcp");
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
```

Expected: FAIL because current cards still require runtime pack.

- [ ] **Step 3: Update cards**

In `src-tauri/src/browser/runtime_contracts.rs`, change Playwright CLI and MCP cards:

```rust
requires_runtime_pack: false,
data_boundary_policy: "official_playwright_cli",
cost_policy: "system_npm_global_cli",
policy_tags: &["official_playwright_cli", "short_lived_worker", "declarative_actions"],
```

and:

```rust
requires_runtime_pack: false,
data_boundary_policy: "official_playwright_mcp",
cost_policy: "system_npx_mcp",
policy_tags: &["official_playwright_mcp", "mcp_manager", "no_raw_mcp_tools"],
```

- [ ] **Step 4: Run test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
```

Expected: PASS.

## Task 2: Replace Runtime Pack Readiness In Status Composition

**Files:**
- Modify: `src-tauri/src/browser/runtime_status.rs`
- Modify: `src-tauri/src/browser/playwright_cli.rs`
- Modify: `src-tauri/src/browser/playwright_mcp.rs`

- [ ] **Step 1: Add failing tests**

In `src-tauri/src/browser/runtime_status.rs` tests, replace runtime-pack-ready setup checks with discovery/setup checks:

```rust
#[test]
fn enabled_cli_does_not_require_runtime_pack_readiness() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), false);
    let mut config = BrowserRuntimeProviderConfig::default();
    config.playwright_cli_enabled = true;

    let report = compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config);

    assert!(report.control_center.feature_flags.playwright_cli);
    assert_ne!(
        report.control_center.provider_lanes[0].fallback_reason.as_deref(),
        Some("runtime_pack_not_ready")
    );
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
```

Expected: FAIL because current control center still uses runtime pack readiness.

- [ ] **Step 3: Change status composition**

In `runtime_status.rs`, stop passing `runtime_pack.ready && runtime_pack.can_run_browser_tasks` into provider readiness and control center as the CLI/MCP gate. Use a temporary setup-ready boolean from PR1 discovery until the full setup status is wired:

```rust
let official_runtime_ready = true;
```

Then use:

```rust
playwright_cli_provider_status(flags, official_runtime_ready)
playwright_mcp_provider_status(flags, official_runtime_ready)
build_control_center_report(provider_config, official_runtime_ready, &provider_statuses)
```

Update `playwright_cli_provider_status` to accept `official_runtime_ready: bool`
instead of `Option<&BrowserRuntimePackStatusReport>`. Update setup check ids:

```rust
"official_playwright_cli_ready"
"Official Playwright CLI"
"Install or repair official Playwright CLI before enabling this provider."
```

Update MCP setup check ids:

```rust
"official_playwright_mcp_ready"
"Official Playwright MCP"
"Install or repair official Playwright MCP before enabling this provider."
```

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status browser::playwright_cli browser::playwright_mcp
```

Expected: PASS.

## Task 3: Remove Runtime Pack Lane Copy From Control Center

**Files:**
- Modify: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: UI Browser Runtime helpers/components.

- [ ] **Step 1: Write expected Rust behavior**

Update control center tests:

```rust
assert_ne!(
    cli.fallback_reason.as_deref(),
    Some("runtime_pack_not_ready")
);
assert_ne!(cli.next_action, "prepare_runtime_pack");
```

- [ ] **Step 2: Update backend fallback reasons**

Replace runtime pack reason logic:

```rust
} else if requires_pack && !runtime_pack_ready {
    Some("runtime_pack_not_ready".to_string())
}
```

with setup readiness reason:

```rust
} else if requires_setup && !official_runtime_ready {
    Some("playwright_setup_not_ready".to_string())
}
```

Replace next action:

```rust
Some("playwright_setup_not_ready") => "run_playwright_setup",
```

- [ ] **Step 3: Update frontend labels**

In `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`, map:

```ts
playwright_setup_not_ready -> "Needs setup"
run_playwright_setup -> "Set up"
```

Remove labels that display:

```ts
"Needs runtime pack"
"Prepare runtime pack"
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

## Task 4: Deprecate Runtime Pack IPC From Normal Settings Path

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_ipc.rs`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`

- [ ] **Step 1: Replace Settings action calls**

Remove or hide calls from the normal Control Center flow:

```ts
dryRunBrowserRuntimeAction("prepare")
executeBrowserRuntimeAction("prepare", ...)
```

Replace with:

```ts
runPlaywrightSetupAction("auto_setup")
```

until PR1/PR5 expose the final command name.

- [ ] **Step 2: Keep command compatibility temporarily**

In `runtime_pack_ipc.rs`, keep existing commands compiling for old callers, but mark returned action as deprecated:

```rust
report.event_names.push("browser.runtime_pack.deprecated".to_string());
```

Do not show those actions in new UI.

- [ ] **Step 3: Verify no primary copy remains**

Run:

```bash
rg -n "Needs runtime pack|Prepare runtime pack|runtime_pack_not_ready|prepare_runtime_pack" ui/src src-tauri/src/browser
```

Expected: matches only in legacy compatibility tests or explicitly deprecated runtime-pack files, not in new Control Center labels or provider readiness.

## Task 5: Verify And Commit

- [ ] **Step 1: Run focused backend tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts browser::runtime_status browser::runtime_control_center browser::runtime_provider_probe
```

Expected: PASS.

- [ ] **Step 2: Run focused frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 3: Commit**

Run:

```bash
git add src-tauri/src/browser ui/src
git commit -m "refactor(browser-runtime): remove runtime pack as provider readiness truth"
```

Expected: commit succeeds.
