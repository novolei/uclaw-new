# Harness UI Reporting Scorecard

Status: implemented in the planned PR-251 slice and expanded in GitHub PR #285 as the unified System Diagnostics harness surface.

The UI reporting surface lives in the System tab because the harness is currently a developer/runtime verification tool tied to service health. As of PR #285 it exposes five explicit controls:

- `All`: runs the four suites sequentially from the frontend.
- `Browser`: calls `run_browser_parity_harness`.
- `Memory`: calls `run_memory_gbrain_eval_harness`.
- `Agent`: calls `run_agent_control_plane_harness`.
- `Self`: calls `run_self_improvement_gate_harness`.

Results render as scorecard summaries with case count, run count, average score, pass/fail state, and failed check IDs/messages. The UI intentionally stays dense and operational rather than marketing-like: this is for repeated debugging and regression verification.

Important boundary: the `Browser` control uses the deterministic browser parity fixture executor. It is a stable contract/regression check, not a live browser-agent run against arbitrary websites. Live browser autonomy is still validated through chat `browser_task`, the Browser panel, and Browser Task Monitor.

Verification command:

```bash
npm test -- --run src/components/settings/SystemTab.test.tsx
npm run build
```
