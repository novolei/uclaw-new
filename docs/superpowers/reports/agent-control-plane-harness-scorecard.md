# Agent Loop Control-Plane Harness Scorecard

Status: implemented for PR #250 as an app-native control-plane trace harness.

The control-plane harness scores agent loop behavior through a normalized trace instead of tying the grader to one concrete UI or model path. The first suite validates the failure modes that have caused real regressions: tool calls without results, missing permission boundaries, missing long-task checkpoints, and runs that remain visually or logically stuck in `running`.

| Capability | Case | Primary Signals |
|---|---|---|
| Tool/result pairing | `agent_loop.tool_result_pairing` | required tool call, matching tool result, closed final status |
| Permission boundary | `agent_loop.permission_boundary` | guarded tool call, recorded permission request, blocked final status |
| Checkpoint resume | `agent_loop.checkpoint_resume` | browser task result, checkpoint event, completed final status |
| Tool failure closure | `agent_loop.tool_failure_completes` | failed tool result, non-running final status |

App entrypoint:

- `run_agent_control_plane_harness`: runs deterministic fixture traces through `AgentLoopControlPlaneHarnessAdapter`, records harness episodes, and writes `agent_control_plane_trace` plus `agent_control_plane_scorecard` artifacts.

Verification command:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::agent_loop --lib
cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw
git diff --check
```
