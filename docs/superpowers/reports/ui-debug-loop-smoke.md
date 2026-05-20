# UI Debug Loop Smoke Report

Status: implemented as a repeatable development workflow.

## Commands

```bash
bash -n scripts/ui_debug_smoke.sh
cd ui && npm test -- src/lib/dev-tauri-mock.test.ts
cd ui && npm test -- src/components/settings/SystemTab.test.tsx
cd ui && npm run build
```

## Desktop Path

Use:

```bash
UCLAW_UI_DEBUG_KEEP_ALIVE=1 ./scripts/ui_debug_smoke.sh
```

Then inspect `uClaw` with Computer Use and confirm:

- process path includes `target/debug/uclaw`;
- WebView URL is the expected dev URL or a classified mismatch;
- screenshot shows meaningful UI or a classified failure.

## Browser Mock Path

Use:

```bash
cd ui
npm run dev:mock-tauri
```

Then inspect `http://127.0.0.1:5173/` with Playwright or the in-app browser and confirm:

- no missing Tauri IPC errors;
- app shell renders;
- System Diagnostics mock commands return fixtures.

## Classification Labels

- `pass`
- `frontend-runtime-error`
- `tauri-devurl-mismatch`
- `ipc-injection-missing`
- `backend-boot-failure`
- `wrong-app-under-test`
- `inconclusive`
