# Harness UI Reporting Scorecard

Status: implemented for PR #251 as a compact system-diagnostics reporting surface.

The first UI reporting slice lives in the System tab because the harness is currently a developer/runtime verification tool tied to service health. It exposes two explicit runs:

- `Memory`: calls `run_memory_gbrain_eval_harness`.
- `Agent`: calls `run_agent_control_plane_harness`.

Results render as scorecard summaries with case count, run count, average score, pass/fail state, and failed check IDs/messages. The UI intentionally stays dense and operational rather than marketing-like: this is for repeated debugging and regression verification.

Verification command:

```bash
npm test -- --run src/components/settings/SystemTab.test.tsx
npm run build
```
