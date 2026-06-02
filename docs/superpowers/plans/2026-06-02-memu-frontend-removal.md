# memU Frontend Removal + Backend Stub Retirement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Remove the frontend memU surface (atoms, dock indicator, settings fields, the broken "install Python (memU)" button, consolidation events, env-check items) left behind by the backend teardown (PR #642), then retire the now-dead backend stub commands. After this, no memU UI, no broken affordance, and the stubbed `get_memu_status`/`restart_memu_bridge`/`MemUBridgeStatus` are gone.

**Architecture:** Pure deletion across `ui/src` (TS/React + vitest) + a small `src-tauri` coda. `tsc --noEmit` + vitest + `cargo build` are the guards.

**Context:** Backend memU deleted in PR #642 (zero Python, user-confirmed). The 3 frontend-facing commands were *stubbed* there for IPC stability; this slice removes the FE callers THEN the stubs.

**Decision:** Python is fully gone, so `EnvironmentCheckDialog`'s "Python Runtime (memU)" + "memU 服务" checks are **removed** (not renamed) — there's no Python/memU runtime to check.

**Verification commands:** `cd ui && npx tsc --noEmit 2>&1 | head -20`; `cd ui && npm test -- --run 2>&1 | tail -15`; `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` + `cargo clippy --lib`.

**Recon map (file:line) — all paths under `ui/src/`:**
- `atoms/dock-atoms.ts`: `memuOnlineAtom` (:19 + comment :15-18), `memuConsolidatingAtom` (:207 + comment :199-206). `atoms/dock-atoms.test.ts`: import :7, test :31-34, import :275, describe :277-290.
- `components/dock/ConnectionIndicator.tsx`: import :12, type `'memu'` :18, `useAtomValue` :50, `memuState` :58-64, bar entry :82-93. `.test.tsx`: import :6, `memu` param/calls, `bars.length` 3→2, memu assertions.
- `components/dock/useConnectionStatus.ts`: import :8, `setMemu` :15, `invoke('get_memu_status')` block :34-39, dep :50. `.test.ts`: import :8, mocks :45/139/152/164, asserts :125/189, describe :149-170.
- `hooks/useMemuConsolidation.ts` + `.test.tsx`: **DELETE both files.** `components/app-shell/AppShell.tsx`: import :66, call :86.
- `hooks/useDockLiveness.ts`: import :2, `useAtomValue` :29, `'mode-memory'` :42-44 → `OFF`. `.test.tsx`: import :6, tests :53-72.
- `components/settings/EmbeddingEndpointSection.tsx`: `fastembed_model` default :14, dirty-calc :37-44, toast :54, Field :122-128. `lib/embedding-endpoint.ts`: interface field :9, `SETUP_SCRIPTS` `'setup-python-env'` :46, descriptor :79-85.
- `components/settings/SystemTab.tsx`: `MemUBridgeStatus` iface :27-34, `memu` field :78, `busyMemu` :183, `report.memu.running` :213, health block :352-353, BridgeCard :391-403, restart button :531-534. `.test.tsx`: mock `memu` :21.
- `components/settings/DeveloperOptionsSection.tsx`: no change (loops `SETUP_SCRIPTS`; the entry disappears with the constant).
- `lib/dev-tauri-mock.ts`: `memu` fixture :36-42, `restart_memu_bridge` case :342.
- `components/environment/EnvironmentCheckDialog.tsx`: comment :4, "Python Runtime (memU)" CheckItem :96-107, "memU 服务" CheckItem :109-114 → **remove both checks**.
- `components/settings/MemoryRecallSettings.tsx`: description :388 (drop "当 memU 向量引擎不可用时，").
- Comment-only (update if trivial): `lib/fold-delta-threshold.ts:18`, `lib/stream-skill-thresholds.ts:29` (reference `memubot_config.rs` — still exists, leave).

---

## Task 1: FE dock + connection + consolidation events

**Files:** `atoms/dock-atoms.ts`(+test), `components/dock/ConnectionIndicator.tsx`(+test), `components/dock/useConnectionStatus.ts`(+test), `hooks/useMemuConsolidation.ts`(+test, DELETE), `components/app-shell/AppShell.tsx`, `hooks/useDockLiveness.ts`(+test).

- [ ] **Step 1:** Per the recon map, delete the 2 atoms + comments; remove the `'memu'` channel + bar + `useAtomValue`/`memuState` from `ConnectionIndicator` (now 2 bars); remove the `get_memu_status` poll + `setMemu` from `useConnectionStatus`; `git rm` both `useMemuConsolidation.*` files + remove its import/call in `AppShell`; remove `memuConsolidatingAtom` use in `useDockLiveness` (`'mode-memory'` → `OFF`/drop key).
- [ ] **Step 2:** Update the matching tests: `dock-atoms.test.ts` (drop the 2 tests + imports), `ConnectionIndicator.test.tsx` (drop `memu` param + assertions, `bars.length` 3→2), `useConnectionStatus.test.ts` (drop the 3 memu tests + mocks + asserts), `useDockLiveness.test.tsx` (drop the 2 memuConsolidating tests + import). `git rm useMemuConsolidation.test.tsx`.
- [ ] **Step 3:** `cd ui && npx tsc --noEmit 2>&1 | head -20` (no new errors) + `npm test -- --run 2>&1 | tail -15` (green; the deleted-test files gone).
- [ ] **Step 4: Commit** — `refactor(ui): remove memU dock atoms + connection bar + consolidation events`

---

## Task 2: FE settings (embedding form, system tab, env check, install button)

**Files:** `components/settings/EmbeddingEndpointSection.tsx`, `lib/embedding-endpoint.ts`, `components/settings/SystemTab.tsx`(+test), `lib/dev-tauri-mock.ts`, `components/environment/EnvironmentCheckDialog.tsx`, `components/settings/MemoryRecallSettings.tsx`.

- [ ] **Step 1:** `EmbeddingEndpointSection` — remove the `fastembed_model` default + dirty-check + the FastEmbed Field (:122-128); simplify the save toast (drop the memU-restart line). `embedding-endpoint.ts` — remove `fastembed_model` from the interface, remove `'setup-python-env'` from `SETUP_SCRIPTS` + its descriptor (this removes the broken install button via the `DeveloperOptionsSection` loop).
- [ ] **Step 2:** `SystemTab.tsx` — delete the `MemUBridgeStatus` interface, the `memu` field on `SystemDiagnosticsReport`, `busyMemu`, the `report.memu.running` health condition, the memU health block, the memU `BridgeCard`, and the "重启 memU" button. `SystemTab.test.tsx` — drop the `memu` mock fixture field. `dev-tauri-mock.ts` — drop the `memu` diagnostics fixture + the `restart_memu_bridge` mock case.
- [ ] **Step 3:** `EnvironmentCheckDialog.tsx` — **remove** the "Python Runtime (memU)" CheckItem (:96-107) and the "memU 服务" CheckItem (:109-114) (Python is fully gone); update the file's top comment. `MemoryRecallSettings.tsx:388` — drop the "当 memU 向量引擎不可用时，" clause from the FTS-fallback description.
- [ ] **Step 4:** `cd ui && npx tsc --noEmit 2>&1 | head -20` + `npm test -- --run 2>&1 | tail -15` (green).
- [ ] **Step 5: Commit** — `refactor(ui): remove memU settings fields, system-tab card, env checks + broken install-python button`

---

## Task 3: Backend coda — retire the now-dead stub commands

**Files:** `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs`.

- [ ] **Step 1:** Confirm no remaining FE/Rust caller of the stubs after Tasks 1-2: `grep -rn "get_memu_status\|restart_memu_bridge" ui/src` → empty; `grep -rn "memu_embed_text" ui/src` → if empty, it's also dead (delete it too); if a caller exists, keep it.
- [ ] **Step 2:** Delete `get_memu_status` + `restart_memu_bridge` (+ `memu_embed_text` if no caller) fns from `tauri_commands.rs` + their `invoke_handler!` macro entries in `main.rs`. Delete the `MemUBridgeStatus` struct + the `memu` field on `SystemDiagnosticsReport` + the offline-stub construction in `get_system_diagnostics` (the `memu` variable + its assignment).
- [ ] **Step 3:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` (none) + `cargo clippy --lib 2>&1 | grep -E "^error"` (none). The SystemTab.tsx (Task 2) already dropped `report.memu`, so the report-struct change is FE-consistent.
- [ ] **Step 4: Commit** — `refactor(memu): retire now-dead stub commands (get_memu_status/restart_memu_bridge) + MemUBridgeStatus`

---

## Task 4: Whole-slice verification + ship

- [ ] **Step 1:** `cd ui && npx tsc --noEmit 2>&1 | head -20` (clean) + `npm test -- --run 2>&1 | tail -15` (green) + `cd src-tauri && cargo build` + `cargo clippy --lib` (clean).
- [ ] **Step 2: Gates:** `grep -rni "memu" ui/src | grep -v "memubot_config"` → empty (or only unrelated comment refs to memubot_config.rs in fold-delta/stream-skill libs); `grep -rn "get_memu_status\|restart_memu_bridge\|MemUBridgeStatus" src-tauri/src ui/src` → empty.
- [ ] **Step 3: Ship** — push → PR (Commits table T1-T3) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 4: Post-merge (manual):** open Settings → System tab shows only the gbrain bridge (no memU card/button); embedding-endpoint form has no FastEmbed field; Developer Options has no "install Python (memU)" button; dock connection indicator shows 2 bars; no console errors.

---

## Self-Review

- **Coverage:** recon's 18-file map → T1 (dock/connection/events), T2 (settings/env/install-button), T3 (backend stubs). ✓
- **No placeholders:** every target is file:line-keyed from the recon. ✓
- **Layout concerns** (from recon) handled: ConnectionIndicator 2 bars (fine), SystemTab gbrain-only card (fine), embedding form 3 fields (fine).
- **Finish:** removes the live-broken install button + the stubs; after this, zero memU in FE + backend (only `memu_tools.rs` filename + `map_memu_type_to_kind` remain, both memU-free). ✓
