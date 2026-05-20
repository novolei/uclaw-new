# Browser Parity Scorecard

Status: implemented in the planned PR-248 slice and merged via GitHub PR #282. The System Diagnostics frontend connector was added later in GitHub PR #285.

The browser parity harness now runs browser-use-aligned cases through a `BrowserParityExecutor`, records each case as a `HarnessRuntime` episode, and writes a `browser_parity_scorecard` JSON artifact for pass/fail review. `BrowserAgentLoop` implements the executor trait, so the same adapter can run against the real `browser_task` loop in targeted backend tests or future live smoke runs.

The System Diagnostics `Browser` button added in PR #285 intentionally uses `BrowserFixtureParityExecutor`, a deterministic fixture executor. That UI path validates browser-agent contract shape, scoring, checkpoint/intervention states, auth-profile ordering, multi-tab expectations, file-upload evidence, and recovery semantics without launching live external browsing or calling an LLM. Live browser autonomy remains verified through chat `browser_task`, the Browser panel, and Browser Task Monitor.

| Capability | Case | Primary Signals |
|---|---|---|
| Navigation | `browser.navigation.basic` | completed status, `browser_navigate`, observed final URL, action budget |
| Multi-tab planning | `browser.multi_tab.compare` | navigation, tab switch, distinct tab evidence, active page evidence |
| File upload | `browser.file_upload.local_file` | workspace-safe allowed file path, `browser_upload_file`, action budget |
| Auth profile restore | `browser.auth_profile.restore` | auth profile apply before navigation, protected URL observed |
| Boundary detection | `browser.boundary.login` | `needs_user_intervention`, boundary kind precision |
| Checkpoint resume | `browser.checkpoint.resume` | checkpoint saved, resume response, completed status |
| Long task recovery | `browser.long_task.recovery` | failed action before successful `recover`, completion |

Runtime fixture support:

- Built-in cases use `{{fixtureBaseUrl}}` and `{{workspaceFixtureFile}}` placeholders instead of hard-coded `127.0.0.1:0` or `/tmp` paths.
- `BrowserParityFixtureServer::spawn()` starts deterministic local fixture pages on a real ephemeral port.
- `run_builtin_suite()` materializes cases against that server and runs them through the supplied executor.
- `run_builtin_suite_with_context()` supports externally managed fixture servers for CI.
- `BrowserAgentLoopParityExecutor` can seed deterministic fake storageState into an injected auth-profile broker before the auth restore case runs.
- `BrowserFixtureParityExecutor` backs the System Diagnostics `Browser` harness button for stable local regression feedback.
- Executor errors are recorded as failed harness episodes with `execution_error` scorecards instead of leaving partial runs.
- URL checks only trust observed page-state phases, not `Decide` plans or raw action attempts.

Verification command:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::browser --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
```
