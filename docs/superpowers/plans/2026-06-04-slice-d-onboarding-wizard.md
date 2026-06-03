# Slice D — First-Run Onboarding Wizard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A non-blocking first-launch wizard (env-check → source → download → warmup → smoke-test → done) that, on completion, **auto-wires** the `utility` + `summarizer` roles to `local/minicpm5-1b` — the "zero manual config" payoff that closes the A→D loop. This is also the first time the full chain (download → load → generate) runs against real weights.

**Architecture:** 4 new backend Tauri commands (`local_model_env_check`, `local_model_warmup`, `local_model_smoke_test`, `get_onboarding_state`/`set_onboarding_state`) reusing Slice C's `local_model_probe_sources`/`download`/`cancel` and Slice B's `:7337` engine. `warmup`/`smoke_test` drive the **already-running** LocalApiService engine over HTTP-to-self (so they warm the actual serving instance, not a second 688 MB copy). The frontend adds a `minicpm` Settings tab + a `MiniCPMWizard` overlay (atom-driven step machine mirroring the existing `InstallWizard`), an app-start gating hook, and the `done`-step role auto-wire via Slice A's `setRoleModel` bridge.

**Tech Stack:** Rust (`reqwest` HTTP-to-self, `sysinfo` system+disk already present, candle Metal probe), Tauri commands + events, React + jotai atoms + `motion/react` + `@tauri-apps/api/event` `listen`, Vitest/jsdom with a mocked `tauri-bridge`.

---

## Boundary with adjacent slices (read first)

- **Slice A (merged):** `setRoleModel(role, modelRef)` → `invoke('set_role_model', {role, modelRef})` (`tauri-bridge.ts:699`). The wizard's `done` step calls it for `utility` + `summarizer` with `local/minicpm5-1b`. Role resolution + cloud fallback already work.
- **Slice B (merged):** the `:7337/v1/chat/completions` engine (lazy-load, 503 `model_not_ready`). `warmup`/`smoke_test` POST here. Provider `local/minicpm5-1b` registered.
- **Slice C (merged):** `local_model_probe_sources`/`download`/`cancel`/`list`/`delete` commands + `minicpm://download-progress` event (`{model_id,file,downloaded,total,source,phase}`) + TS bridge (`localModelProbeSources`, `localModelDownload`, etc., `MiniCpmDownloadProgress`). The wizard's `source`/`download` steps reuse these — **do NOT reimplement them.**
- **No DB migration.** Onboarding state persists to a small JSON file under the data dir.

## Verified facts (recon)

- `AppState.memubot_config: Arc<tokio::sync::RwLock<MemubotConfig>>` (`app.rs:296`) → `.read().await.local_api.port` (default 7337) for HTTP-to-self. `AppState.data_dir: PathBuf`.
- `SettingsTab` union: `ui/src/atoms/settings-tab.ts`. `SettingsNav` GROUPS: `ui/src/components/settings/SettingsNav.tsx`. Tab body switch + `TAB_LABEL`: `ui/src/components/settings/SettingsPanel.tsx` (`SettingsContent` switches on tab). `sttNeedsDownload` dot is the precedent for a "needs download" indicator.
- Wizard pattern: `ui/src/components/automation/InstallWizard.tsx` — jotai atom `{step, ...}`, `listen<Payload>(channel, cb)` in a `useEffect`, returns `null` when `step === null`, `motion`/`AnimatePresence` overlay. Test: `InstallWizard.test.tsx`.
- Slice C bridge wrappers + types already exported from `tauri-bridge.ts`: `localModelProbeSources()`, `localModelDownload({quant,source})`, `localModelCancel()`, `localModelList()`, `localModelDelete()`, `ProbedSource`, `MiniCpmDownloadProgress`, `LocalInstalledModel`.
- Two-edit rule (uclaw-tauri-commands): every command in BOTH `tauri_commands.rs` and `main.rs`'s `generate_handler!` (~line 1333 where Slice C's `local_model_*` are).
- `candle_core::Device::new_metal(0)` is available (Slice B dep); reuse for the Metal probe.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/local_llm/env_check.rs` | `EnvReport` + `recommended_quant` (pure) + `collect_env_report` |
| `src-tauri/src/local_llm/onboarding.rs` | onboarding-state JSON store (`OnboardingState`, read/write) |
| `src-tauri/src/local_llm/mod.rs` | `pub mod env_check; pub mod onboarding;` |
| `src-tauri/src/tauri_commands.rs` | 4 commands: `local_model_env_check`, `local_model_warmup`, `local_model_smoke_test`, `get_onboarding_state`/`set_onboarding_state` |
| `src-tauri/src/main.rs` | register the 5 commands in `invoke_handler!` |
| `ui/src/lib/tauri-bridge.ts` | wrappers + `EnvReport`/`OnboardingState` types |
| `ui/src/atoms/settings-tab.ts` | add `'minicpm'` to `SettingsTab` |
| `ui/src/atoms/minicpm-wizard.ts` | `minicpmWizardAtom` + step types |
| `ui/src/components/settings/SettingsNav.tsx` | add the `minicpm` nav item |
| `ui/src/components/settings/SettingsPanel.tsx` | `case 'minicpm'` + `TAB_LABEL` |
| `ui/src/components/settings/MiniCPMSettings.tsx` | Settings tab body (launch/re-run wizard + list/delete management) |
| `ui/src/components/onboarding/MiniCPMWizard.tsx` | the multi-step overlay wizard |
| `ui/src/App.tsx` | mount `<MiniCPMWizard />` + app-start gating hook |

All new `.rs` files start with `// SPDX-License-Identifier: Apache-2.0`.

---

## Task 1: `env_check` — EnvReport + recommended_quant (pure) + command

**Files:**
- Create: `src-tauri/src/local_llm/env_check.rs`
- Modify: `src-tauri/src/local_llm/mod.rs` (add `pub mod env_check;`)
- Modify: `src-tauri/src/tauri_commands.rs` (command) + `src-tauri/src/main.rs` (register)

- [ ] **Step 1: Write `env_check.rs` with the pure `recommended_quant` + report builder + tests**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Hardware environment check for the local-model onboarding wizard:
//! OS/arch/RAM/disk/Metal/cpu-cores → a recommended quant + warnings.

use serde::Serialize;

/// Recommended quant string, chosen by available RAM. Pure + unit-tested.
/// Q4_K_M needs ~1.5 GB resident; Q8_0 ~2 GB; F16 ~3 GB. We recommend
/// conservatively so first-run defaults never OOM a small machine.
pub fn recommended_quant(total_ram_bytes: u64) -> &'static str {
    const GB: u64 = 1_000_000_000;
    if total_ram_bytes >= 32 * GB {
        "Q8_0"
    } else {
        "Q4_K_M"
    }
}

/// Per-resource hardware report surfaced to the wizard's env-check step.
#[derive(Debug, Clone, Serialize)]
pub struct EnvReport {
    pub os: String,
    pub arch: String,
    pub total_ram: u64,
    pub free_disk: u64,
    pub metal_available: bool,
    pub cpu_cores: usize,
    pub recommended_quant: String,
    pub warnings: Vec<String>,
}

/// True if a candle Metal device initialises (macOS GPU acceleration).
pub fn metal_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        candle_core::Device::new_metal(0).is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Build warnings from the gathered numbers (pure — unit-tested).
/// Flags low disk (<1.2 GB headroom for the default quant) and low RAM (<8 GB).
pub fn build_warnings(total_ram: u64, free_disk: u64) -> Vec<String> {
    const GB: u64 = 1_000_000_000;
    let mut w = Vec::new();
    if free_disk < 1_200_000_000 {
        w.push(format!(
            "磁盘空间不足：剩余 {} MB，建议至少 1200 MB",
            free_disk / 1_000_000
        ));
    }
    if total_ram < 8 * GB {
        w.push(format!(
            "内存较小：{} GB，本地模型可能与其他应用争用内存",
            total_ram / GB
        ));
    }
    w
}

/// Collect the full report (uses sysinfo + the Metal probe + a data dir for
/// the disk figure). `free_disk_bytes` is Slice C's helper.
pub fn collect_env_report(data_dir: &std::path::Path) -> EnvReport {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram = sys.total_memory(); // bytes in sysinfo 0.31
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let free_disk = crate::local_llm::model_manager::free_disk_bytes(data_dir).unwrap_or(0);
    EnvReport {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        total_ram,
        free_disk,
        metal_available: metal_available(),
        cpu_cores,
        recommended_quant: recommended_quant(total_ram).to_string(),
        warnings: build_warnings(total_ram, free_disk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_q4_for_small_ram() {
        assert_eq!(recommended_quant(8_000_000_000), "Q4_K_M");
        assert_eq!(recommended_quant(16_000_000_000), "Q4_K_M");
    }

    #[test]
    fn recommends_q8_for_large_ram() {
        assert_eq!(recommended_quant(64_000_000_000), "Q8_0");
    }

    #[test]
    fn warns_on_low_disk() {
        let w = build_warnings(16_000_000_000, 500_000_000);
        assert!(w.iter().any(|s| s.contains("磁盘")));
    }

    #[test]
    fn warns_on_low_ram() {
        let w = build_warnings(4_000_000_000, 50_000_000_000);
        assert!(w.iter().any(|s| s.contains("内存")));
    }

    #[test]
    fn no_warnings_on_healthy_box() {
        let w = build_warnings(16_000_000_000, 50_000_000_000);
        assert!(w.is_empty());
    }
}
```

> **Implementer note:** confirm `sysinfo::System::total_memory()` returns **bytes** in 0.31 (older versions returned KB). Read `~/.cargo/registry/src/*/sysinfo-0.31*/src/` if unsure; adjust the GB math if it's KB. Also confirm `System::new()` + `refresh_memory()` is the right 0.31 call (vs `new_all`).

- [ ] **Step 2: Wire `pub mod env_check;` into `local_llm/mod.rs`.**

- [ ] **Step 3: Add the command to `tauri_commands.rs`** (near the Slice C `local_model_*` group):

```rust
/// Hardware env check for the onboarding wizard.
#[tauri::command]
pub async fn local_model_env_check(
    state: tauri::State<'_, AppState>,
) -> Result<crate::local_llm::env_check::EnvReport, String> {
    Ok(crate::local_llm::env_check::collect_env_report(&state.data_dir))
}
```

- [ ] **Step 4: Register in `main.rs`** `generate_handler!`: `uclaw_core::tauri_commands::local_model_env_check,`

- [ ] **Step 5: Build + test + two-edit audit**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib local_llm::env_check 2>&1 | tail -20` → `test result: ok. 5 passed`.
Run: `grep -c local_model_env_check src/tauri_commands.rs src/main.rs` → both ≥1.

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(local_llm): env_check command + recommended_quant + EnvReport

Slice D Task 1. Pure recommended_quant (by RAM) + build_warnings (low disk/RAM),
unit-tested; collect_env_report uses sysinfo + candle Metal probe + Slice C's
free_disk_bytes. Registered two-edit."
```

---

## Task 2: `warmup` + `smoke_test` commands (HTTP-to-self to the running engine)

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (2 commands + a tiny URL helper)
- Modify: `src-tauri/src/main.rs` (register 2)

- [ ] **Step 1: Add the commands to `tauri_commands.rs`**

```rust
/// Base URL of the in-process model server (reads the configured port).
async fn local_chat_url(state: &tauri::State<'_, AppState>) -> String {
    let port = state.memubot_config.read().await.local_api.port;
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

/// POST a chat-completions request to the running engine and return the
/// assistant text. Surfaces a 503 model_not_ready as an Err so the wizard can
/// guide the user back to download.
async fn call_local_chat(url: &str, prompt: &str, max_tokens: u32) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let resp = client
        .post(url)
        .json(&serde_json::json!({
            "model": "local/minicpm5-1b",
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
            "max_tokens": max_tokens,
            "temperature": 0.0
        }))
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return Err("model not ready".to_string());
    }
    let resp = resp.error_for_status().map_err(|e| format!("status: {e}"))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("decode: {e}"))?;
    Ok(body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// Warm the engine: trigger lazy-load + a 1-token forward (JITs Metal kernels).
#[tauri::command]
pub async fn local_model_warmup(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let url = local_chat_url(&state).await;
    call_local_chat(&url, "hi", 1).await.map(|_| ())
}

/// Run a real prompt through the local model and return its output (wizard proof).
#[tauri::command]
pub async fn local_model_smoke_test(
    state: tauri::State<'_, AppState>,
    prompt: Option<String>,
) -> Result<String, String> {
    let url = local_chat_url(&state).await;
    let p = prompt.unwrap_or_else(|| "你好".to_string());
    call_local_chat(&url, &p, 64).await
}
```

- [ ] **Step 2: Register both in `main.rs`** `generate_handler!`:
```rust
            uclaw_core::tauri_commands::local_model_warmup,
            uclaw_core::tauri_commands::local_model_smoke_test,
```

- [ ] **Step 3: Build + two-edit audit**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `for c in local_model_warmup local_model_smoke_test; do echo -n "$c "; echo "t=$(grep -c $c src/tauri_commands.rs) m=$(grep -c $c src/main.rs)"; done` → both in both.
Run: `cargo clippy --lib 2>&1 | grep tauri_commands | head` → no new warnings.

> No unit test here (it requires a running server). The wizard's gated end-to-end run is the validation; the URL helper is trivial. If you want, add a tiny pure test that `local_chat_url` formats the port correctly by factoring the format into a `fn chat_url(port: u16) -> String` and testing that.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(local_llm): warmup + smoke_test commands (HTTP-to-self)

Slice D Task 2. Both drive the already-running :7337 engine (warms the actual
serving instance, no second 688MB load) and surface 503 model_not_ready as Err.
Registered two-edit."
```

---

## Task 3: Onboarding-state store + get/set commands

**Files:**
- Create: `src-tauri/src/local_llm/onboarding.rs`
- Modify: `src-tauri/src/local_llm/mod.rs` (`pub mod onboarding;`)
- Modify: `src-tauri/src/tauri_commands.rs` (2 commands) + `main.rs` (register)

- [ ] **Step 1: Write `onboarding.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Persisted first-run onboarding state for the local model, stored as a small
//! JSON file under the data dir (no DB migration). Tri-state per the spec:
//! pending → (completed | deferred | skipped). `deferred` is re-promptable;
//! `skipped` is permanent ("不再提示").

use std::path::{Path, PathBuf};

const FILENAME: &str = "local_model_onboarding.json";

/// Valid onboarding states. Unknown strings are rejected at the command layer.
pub const VALID_STATES: &[&str] = &["pending", "completed", "deferred", "skipped"];

pub fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILENAME)
}

/// Read the stored state; defaults to "pending" if the file is absent/garbage.
pub fn read_state(data_dir: &Path) -> String {
    let path = state_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            v.get("minicpm")
                .and_then(|x| x.as_str())
                .filter(|s| VALID_STATES.contains(s))
                .unwrap_or("pending")
                .to_string()
        }
        Err(_) => "pending".to_string(),
    }
}

/// Persist the state. Rejects unknown values.
pub fn write_state(data_dir: &Path, state: &str) -> Result<(), String> {
    if !VALID_STATES.contains(&state) {
        return Err(format!("invalid onboarding state: {state}"));
    }
    std::fs::create_dir_all(data_dir).map_err(|e| format!("mkdir: {e}"))?;
    let body = serde_json::json!({ "minicpm": state });
    std::fs::write(state_path(data_dir), body.to_string()).map_err(|e| format!("write: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_pending_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read_state(tmp.path()), "pending");
    }

    #[test]
    fn write_then_read_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        write_state(tmp.path(), "completed").unwrap();
        assert_eq!(read_state(tmp.path()), "completed");
        write_state(tmp.path(), "deferred").unwrap();
        assert_eq!(read_state(tmp.path()), "deferred");
    }

    #[test]
    fn rejects_invalid_state() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(write_state(tmp.path(), "bogus").is_err());
    }

    #[test]
    fn garbage_file_reads_as_pending() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(state_path(tmp.path()), b"not json").unwrap();
        assert_eq!(read_state(tmp.path()), "pending");
    }
}
```

- [ ] **Step 2: `pub mod onboarding;` in `local_llm/mod.rs`.**

- [ ] **Step 3: Commands in `tauri_commands.rs`:**

```rust
/// Read the local-model onboarding state ("pending"/"completed"/"deferred"/"skipped").
#[tauri::command]
pub async fn get_onboarding_state(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(crate::local_llm::onboarding::read_state(&state.data_dir))
}

/// Persist the local-model onboarding state.
#[tauri::command]
pub async fn set_onboarding_state(
    state: tauri::State<'_, AppState>,
    value: String,
) -> Result<(), String> {
    crate::local_llm::onboarding::write_state(&state.data_dir, &value)
}
```

- [ ] **Step 4: Register both in `main.rs`.**

- [ ] **Step 5: Build + test + two-edit audit**

Run: `cd src-tauri && cargo test --lib local_llm::onboarding 2>&1 | tail -20` → 4 passed.
Run: `for c in get_onboarding_state set_onboarding_state; do echo -n "$c "; echo "t=$(grep -c $c src/tauri_commands.rs) m=$(grep -c $c src/main.rs)"; done` → both in both.

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(local_llm): onboarding-state store + get/set commands

Slice D Task 3. Tri-state (pending/completed/deferred/skipped) JSON store under
the data dir; read defaults to pending on absent/garbage; write validates.
Registered two-edit. No DB migration."
```

---

## Task 4: TS bridge + Settings tab plumbing (union + nav + panel + tab body)

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts` (wrappers + types)
- Modify: `ui/src/atoms/settings-tab.ts` (add `'minicpm'`)
- Modify: `ui/src/components/settings/SettingsNav.tsx` (nav item)
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (`case 'minicpm'` + `TAB_LABEL`)
- Create: `ui/src/components/settings/MiniCPMSettings.tsx` (tab body)
- Create: `ui/src/components/settings/MiniCPMSettings.test.tsx`

- [ ] **Step 1: Bridge wrappers + types in `tauri-bridge.ts`** (match the file's `invoke` import + existing Slice C block):

```typescript
// ── Local model onboarding (Slice D) ──────────────────────────────────

export interface EnvReport {
  os: string;
  arch: string;
  total_ram: number;
  free_disk: number;
  metal_available: boolean;
  cpu_cores: number;
  recommended_quant: string;
  warnings: string[];
}

export type OnboardingState = "pending" | "completed" | "deferred" | "skipped";

export function localModelEnvCheck(): Promise<EnvReport> {
  return invoke("local_model_env_check");
}
export function localModelWarmup(): Promise<void> {
  return invoke("local_model_warmup");
}
export function localModelSmokeTest(prompt?: string): Promise<string> {
  return invoke("local_model_smoke_test", { prompt: prompt ?? null });
}
export function getOnboardingState(): Promise<OnboardingState> {
  return invoke("get_onboarding_state") as Promise<OnboardingState>;
}
export function setOnboardingState(value: OnboardingState): Promise<void> {
  return invoke("set_onboarding_state", { value });
}
```

- [ ] **Step 2: Add `'minicpm'` to the `SettingsTab` union** in `ui/src/atoms/settings-tab.ts`:
```typescript
  | 'minicpm'        // 本地模型（MiniCPM 下载 + 向导）
```

- [ ] **Step 3: Add the nav item** in `SettingsNav.tsx`. Import an icon (e.g. `Cpu` is taken by intelligence; use `HardDrive` — already imported — or `Bot`). Add to the 偏好 group (near pet), and add a `minicpmNeedsDownload` dot prop mirroring `sttNeedsDownload`:
```tsx
      { id: 'minicpm', label: '本地模型', icon: <HardDrive size={16} /> },
```
(Add `minicpmNeedsDownload: boolean` to `SettingsNavProps` and render a dot like the `stt` one if true. The caller in SettingsPanel can pass `false` for now / wire it in Task 6 — keep the prop optional with a default to avoid breaking the existing call.)

- [ ] **Step 4: Wire the tab body** in `SettingsPanel.tsx`: import `MiniCPMSettings`, add `case 'minicpm': return <MiniCPMSettings />` to `SettingsContent`, and add `minicpm: '本地模型'` to `TAB_LABEL`.

- [ ] **Step 5: Create `MiniCPMSettings.tsx`** (tab body — launches the wizard + lists/deletes installed model):

```tsx
import * as React from 'react'
import { useSetAtom } from 'jotai'
import { HardDrive, Trash2, Sparkles } from 'lucide-react'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'
import { localModelList, localModelDelete, type LocalInstalledModel } from '@/lib/tauri-bridge'

export function MiniCPMSettings(): React.ReactElement {
  const setWizard = useSetAtom(minicpmWizardAtom)
  const [models, setModels] = React.useState<LocalInstalledModel[]>([])
  const [busy, setBusy] = React.useState(false)

  const refresh = React.useCallback(async () => {
    try { setModels(await localModelList()) } catch { /* ignore */ }
  }, [])

  React.useEffect(() => { void refresh() }, [refresh])

  const startWizard = () => setWizard((s) => ({ ...s, step: 'intro', error: null }))

  const remove = async () => {
    setBusy(true)
    try { await localModelDelete(); await refresh() } finally { setBusy(false) }
  }

  const installed = models[0]?.installed ?? false
  const totalMb = Math.round((models[0]?.total_bytes ?? 0) / 1_000_000)

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-2 text-sm font-medium">
        <HardDrive size={16} /> 本地模型 (MiniCPM5-1B)
      </div>
      <p className="text-xs text-muted-foreground">
        本地运行的轻量模型，用于「轻工具」与「记忆摘要」场景，省 token、保护隐私、可离线。
      </p>
      <div className="rounded-md border border-border/50 p-3 text-xs space-y-2">
        <div>状态：{installed ? `已安装（${totalMb} MB）` : '未安装'}</div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={startWizard}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1.5 text-primary-foreground"
          >
            <Sparkles size={14} /> {installed ? '重新运行向导' : '开始设置'}
          </button>
          {installed && (
            <button
              type="button"
              onClick={remove}
              disabled={busy}
              className="inline-flex items-center gap-1 rounded-md border border-border px-3 py-1.5 disabled:opacity-50"
            >
              <Trash2 size={14} /> 删除模型
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
```

> `minicpmWizardAtom` is created in Task 5. To keep THIS task compiling, create a minimal `ui/src/atoms/minicpm-wizard.ts` stub now with just the atom + step type (Task 5 fills the rest). Concretely, in this task create:
> ```typescript
> import { atom } from 'jotai'
> export type MiniCpmWizardStep =
>   | 'intro' | 'envcheck' | 'source' | 'download' | 'warmup' | 'smoketest' | 'done' | 'error' | null
> export interface MiniCpmWizardState { step: MiniCpmWizardStep; error: string | null }
> export const minicpmWizardAtom = atom<MiniCpmWizardState>({ step: null, error: null })
> ```
> Task 5 expands `MiniCpmWizardState` with the extra fields.

- [ ] **Step 6: Test `MiniCPMSettings.test.tsx`** (mock the bridge; assert it renders + launches the wizard):

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider } from 'jotai'

vi.mock('@/lib/tauri-bridge', () => ({
  localModelList: vi.fn().mockResolvedValue([
    { model_id: 'minicpm5-1b', installed: false, files: [], total_bytes: 0 },
  ]),
  localModelDelete: vi.fn().mockResolvedValue(undefined),
}))

import { MiniCPMSettings } from './MiniCPMSettings'

describe('MiniCPMSettings', () => {
  it('renders the not-installed state and a start button', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByText(/未安装/)).toBeInTheDocument())
    expect(screen.getByText(/开始设置/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 7: Build TS + run the test**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` → clean (re the new files).
Run: `cd ui && npm test -- --run MiniCPMSettings 2>&1 | tail -15` → pass.

- [ ] **Step 8: Commit**

```bash
git commit -am "feat(ui): MiniCPM settings tab + onboarding bridge wrappers

Slice D Task 4. SettingsTab 'minicpm' + nav item + panel case + MiniCPMSettings
body (launch wizard / list / delete). Bridge wrappers for env_check/warmup/
smoke_test/get+set_onboarding_state + EnvReport/OnboardingState types."
```

---

## Task 5: `MiniCPMWizard` overlay — step machine + progress + done→setRoleModel

**Files:**
- Modify: `ui/src/atoms/minicpm-wizard.ts` (expand state)
- Create: `ui/src/components/onboarding/MiniCPMWizard.tsx`
- Create: `ui/src/components/onboarding/MiniCPMWizard.test.tsx`

- [ ] **Step 1: Expand the wizard atom** (`ui/src/atoms/minicpm-wizard.ts`):

```typescript
import { atom } from 'jotai'
import type { EnvReport, ProbedSource, MiniCpmDownloadProgress } from '@/lib/tauri-bridge'

export type MiniCpmWizardStep =
  | 'intro' | 'envcheck' | 'source' | 'download' | 'warmup' | 'smoketest' | 'done' | 'error' | null

export interface MiniCpmWizardState {
  step: MiniCpmWizardStep
  env: EnvReport | null
  sources: ProbedSource[] | null
  chosenSource: string | null      // host id, or null = auto (probe-ranked)
  progress: MiniCpmDownloadProgress | null
  smokeOutput: string | null
  error: string | null
}

export const INITIAL_WIZARD: MiniCpmWizardState = {
  step: null, env: null, sources: null, chosenSource: null,
  progress: null, smokeOutput: null, error: null,
}

export const minicpmWizardAtom = atom<MiniCpmWizardState>(INITIAL_WIZARD)
```

- [ ] **Step 2: Write `MiniCPMWizard.tsx`** (overlay, mirrors `InstallWizard`):

```tsx
import * as React from 'react'
import { useAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Loader2, Check, AlertCircle, X } from 'lucide-react'
import { listen } from '@tauri-apps/api/event'
import {
  localModelEnvCheck, localModelProbeSources, localModelDownload, localModelCancel,
  localModelWarmup, localModelSmokeTest, setRoleModel, setOnboardingState,
  type MiniCpmDownloadProgress,
} from '@/lib/tauri-bridge'
import { minicpmWizardAtom, INITIAL_WIZARD } from '@/atoms/minicpm-wizard'

export function MiniCPMWizard(): React.ReactElement | null {
  const [s, set] = useAtom(minicpmWizardAtom)

  // Subscribe to download progress while on the download step.
  React.useEffect(() => {
    if (s.step !== 'download') return
    let unlisten: (() => void) | undefined
    listen<MiniCpmDownloadProgress>('minicpm://download-progress', (e) => {
      set((p) => ({ ...p, progress: e.payload }))
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [s.step, set])

  if (s.step === null) return null

  const close = () => set(INITIAL_WIZARD)
  const fail = (msg: string) => set((p) => ({ ...p, step: 'error', error: msg }))

  const runEnvCheck = async () => {
    set((p) => ({ ...p, step: 'envcheck' }))
    try { set((p) => ({ ...p, env: await localModelEnvCheck() })) }
    catch (e) { fail(String(e)) }
  }

  const runProbe = async () => {
    set((p) => ({ ...p, step: 'source' }))
    try { set((p) => ({ ...p, sources: await localModelProbeSources() })) }
    catch (e) { fail(String(e)) }
  }

  const runDownload = async () => {
    set((p) => ({ ...p, step: 'download', progress: null }))
    try {
      await localModelDownload({ source: s.chosenSource ?? undefined })
      await runWarmup()
    } catch (e) { fail(String(e)) }
  }

  const runWarmup = async () => {
    set((p) => ({ ...p, step: 'warmup' }))
    try { await localModelWarmup(); await runSmoke() }
    catch (e) { fail(String(e)) }
  }

  const runSmoke = async () => {
    set((p) => ({ ...p, step: 'smoketest' }))
    try {
      const out = await localModelSmokeTest('你好')
      set((p) => ({ ...p, smokeOutput: out, step: 'done' }))
      await finish()
    } catch (e) { fail(String(e)) }
  }

  // done: auto-wire roles + persist completion.
  const finish = async () => {
    try {
      await setRoleModel('utility', 'local/minicpm5-1b')
      await setRoleModel('summarizer', 'local/minicpm5-1b')
      await setOnboardingState('completed')
    } catch { /* role wiring best-effort; surfaced in 模型分配 */ }
  }

  const cancel = async () => { try { await localModelCancel() } catch { /* ignore */ } ; close() }

  return (
    <AnimatePresence>
      <motion.div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
        data-testid="minicpm-wizard"
      >
        <div className="w-[480px] rounded-lg bg-background p-6 shadow-xl space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-base font-semibold">本地模型设置</h2>
            <button type="button" onClick={close} aria-label="关闭"><X size={16} /></button>
          </div>

          {s.step === 'intro' && (
            <div className="space-y-3 text-sm">
              <p>安装本地 MiniCPM 模型可在「轻工具 / 记忆摘要」场景省 token、保护隐私、离线可用。约 688 MB。</p>
              <div className="flex gap-2">
                <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runEnvCheck}>现在设置</button>
                <button className="rounded-md border px-3 py-1.5" onClick={async () => { await setOnboardingState('deferred'); close() }}>稍后</button>
                <button className="rounded-md border px-3 py-1.5 text-muted-foreground" onClick={async () => { await setOnboardingState('skipped'); close() }}>不再提示</button>
              </div>
            </div>
          )}

          {s.step === 'envcheck' && (
            <div className="space-y-3 text-sm">
              {!s.env ? <Loader2 className="animate-spin" size={18} /> : (
                <>
                  <div className="text-xs space-y-1">
                    <div>系统：{s.env.os} / {s.env.arch} · {s.env.cpu_cores} 核 · Metal {s.env.metal_available ? '可用' : '不可用'}</div>
                    <div>内存：{Math.round(s.env.total_ram / 1e9)} GB · 磁盘剩余：{Math.round(s.env.free_disk / 1e9)} GB</div>
                    <div>推荐量化：{s.env.recommended_quant}</div>
                  </div>
                  {s.env.warnings.map((w, i) => (
                    <div key={i} className="flex items-center gap-1 text-amber-600 text-xs"><AlertCircle size={12} /> {w}</div>
                  ))}
                  <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runProbe}>继续 →</button>
                </>
              )}
            </div>
          )}

          {s.step === 'source' && (
            <div className="space-y-3 text-sm">
              {!s.sources ? <Loader2 className="animate-spin" size={18} /> : (
                <>
                  <div className="text-xs">下载源（已按延迟排序）：</div>
                  {s.sources.map((src) => (
                    <label key={src.host} className="flex items-center gap-2 text-xs">
                      <input type="radio" name="src" checked={s.chosenSource === src.host}
                        onChange={() => set((p) => ({ ...p, chosenSource: src.host }))} />
                      {src.host} · {src.reachable ? `${src.latency_ms ?? '?'} ms` : '不可达'}
                    </label>
                  ))}
                  <label className="flex items-center gap-2 text-xs">
                    <input type="radio" name="src" checked={s.chosenSource === null}
                      onChange={() => set((p) => ({ ...p, chosenSource: null }))} />
                    自动（最快）
                  </label>
                  <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runDownload}>下载 →</button>
                </>
              )}
            </div>
          )}

          {s.step === 'download' && (
            <div className="space-y-3 text-sm">
              <div className="text-xs">{s.progress ? `${s.progress.file} · ${s.progress.source} · ${Math.round((s.progress.downloaded) / 1e6)} MB${s.progress.total ? ` / ${Math.round(s.progress.total / 1e6)} MB` : ''} · ${s.progress.phase}` : '准备下载…'}</div>
              <div className="h-2 w-full overflow-hidden rounded bg-muted">
                <div className="h-full bg-primary transition-all"
                  style={{ width: s.progress?.total ? `${Math.min(100, (s.progress.downloaded / s.progress.total) * 100)}%` : '15%' }} />
              </div>
              <button className="rounded-md border px-3 py-1.5" onClick={cancel}>取消</button>
            </div>
          )}

          {s.step === 'warmup' && <div className="flex items-center gap-2 text-sm"><Loader2 className="animate-spin" size={16} /> 正在预热模型…</div>}

          {s.step === 'smoketest' && <div className="flex items-center gap-2 text-sm"><Loader2 className="animate-spin" size={16} /> 正在测试生成…</div>}

          {s.step === 'done' && (
            <div className="space-y-3 text-sm">
              <div className="flex items-center gap-2 text-green-600"><Check size={16} /> 完成！已把「轻工具 / 记忆摘要」接到本地模型。</div>
              {s.smokeOutput && <div className="rounded bg-muted p-2 text-xs">模型输出：{s.smokeOutput}</div>}
              <div className="text-xs text-muted-foreground">可随时在「设置 → 智能 → 模型分配」中更改。</div>
              <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={close}>知道了</button>
            </div>
          )}

          {s.step === 'error' && (
            <div className="space-y-3 text-sm">
              <div className="flex items-center gap-2 text-red-600"><AlertCircle size={16} /> 出错了</div>
              <div className="rounded bg-muted p-2 text-xs">{s.error}</div>
              <div className="flex gap-2">
                <button className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground" onClick={runEnvCheck}>重试</button>
                <button className="rounded-md border px-3 py-1.5" onClick={close}>关闭（稍后再试，云端继续可用）</button>
              </div>
            </div>
          )}
        </div>
      </motion.div>
    </AnimatePresence>
  )
}
```

- [ ] **Step 3: Write `MiniCPMWizard.test.tsx`** — the load-bearing test is "done step wires roles":

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

const setRoleModel = vi.fn().mockResolvedValue(undefined)
const setOnboardingState = vi.fn().mockResolvedValue(undefined)

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
vi.mock('@/lib/tauri-bridge', () => ({
  localModelEnvCheck: vi.fn().mockResolvedValue({
    os: 'macos', arch: 'aarch64', total_ram: 16e9, free_disk: 50e9,
    metal_available: true, cpu_cores: 8, recommended_quant: 'Q4_K_M', warnings: [],
  }),
  localModelProbeSources: vi.fn().mockResolvedValue([{ host: 'huggingface', reachable: true, latency_ms: 120 }]),
  localModelDownload: vi.fn().mockResolvedValue(undefined),
  localModelCancel: vi.fn().mockResolvedValue(undefined),
  localModelWarmup: vi.fn().mockResolvedValue(undefined),
  localModelSmokeTest: vi.fn().mockResolvedValue('你好！'),
  setRoleModel,
  setOnboardingState,
}))

import { MiniCPMWizard } from './MiniCPMWizard'

beforeEach(() => { setRoleModel.mockClear(); setOnboardingState.mockClear() })

function renderAtStep(step: string) {
  const store = createStore()
  store.set(minicpmWizardAtom, {
    step: step as never, env: null, sources: null, chosenSource: null,
    progress: null, smokeOutput: null, error: null,
  })
  return render(<Provider store={store}><MiniCPMWizard /></Provider>)
}

describe('MiniCPMWizard', () => {
  it('returns null when step is null', () => {
    const store = createStore()
    const { container } = render(<Provider store={store}><MiniCPMWizard /></Provider>)
    expect(container.querySelector('[data-testid="minicpm-wizard"]')).toBeNull()
  })

  it('intro → 不再提示 persists skipped', async () => {
    renderAtStep('intro')
    fireEvent.click(screen.getByText('不再提示'))
    await waitFor(() => expect(setOnboardingState).toHaveBeenCalledWith('skipped'))
  })

  it('full happy path from source wires both roles + completion', async () => {
    renderAtStep('source')
    // sources null initially → component shows spinner; set sources via re-render:
    // simplest: drive from 'source' with sources already present
    // (re-render with sources)
    // Instead, click 下载 after we inject sources through the atom:
    // For determinism, start at 'source' and immediately trigger download path.
    await waitFor(() => screen.getByText(/自动/))
    fireEvent.click(screen.getByText('下载 →'))
    await waitFor(() => expect(setRoleModel).toHaveBeenCalledWith('utility', 'local/minicpm5-1b'))
    expect(setRoleModel).toHaveBeenCalledWith('summarizer', 'local/minicpm5-1b')
    await waitFor(() => expect(setOnboardingState).toHaveBeenCalledWith('completed'))
  })
})
```

> **Implementer note:** the third test needs `sources` populated to render the 下载 button. Since `renderAtStep('source')` sets `sources: null`, either (a) seed `sources` in the store in `renderAtStep` for that test, or (b) start at `intro`, click 现在设置, and let the mocked `localModelEnvCheck`/`localModelProbeSources` resolve through to the source step. Prefer (a) for determinism: add a `sources` param to `renderAtStep`. Adjust so the test deterministically reaches the download→warmup→smoke→done chain and asserts both `setRoleModel` calls + `setOnboardingState('completed')`. The ASSERTIONS (both roles wired, completion persisted) are the contract — keep them; adapt the setup to make them fire deterministically.

- [ ] **Step 4: Build TS + run tests**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` → clean.
Run: `cd ui && npm test -- --run MiniCPMWizard 2>&1 | tail -20` → pass (null-render, skipped-persist, happy-path role-wiring).

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(ui): MiniCPMWizard overlay (env→source→download→warmup→smoke→done)

Slice D Task 5. Atom-driven step machine mirroring InstallWizard; subscribes to
minicpm://download-progress; done step auto-wires utility+summarizer to
local/minicpm5-1b (Slice A setRoleModel) + persists onboarding 'completed'.
Vitest: null-render, skip-persist, happy-path role-wiring."
```

---

## Task 6: App-start gating hook + mount overlay + needs-download dot

**Files:**
- Modify: `ui/src/App.tsx` (mount `<MiniCPMWizard />` + gating effect)
- Modify: `ui/src/components/settings/SettingsPanel.tsx` (pass `minicpmNeedsDownload` to nav)
- Create: `ui/src/components/onboarding/useOnboardingGate.ts` (the gating hook) + test

- [ ] **Step 1: Create the gating hook `ui/src/components/onboarding/useOnboardingGate.ts`**

```typescript
import * as React from 'react'
import { useSetAtom } from 'jotai'
import { getOnboardingState } from '@/lib/tauri-bridge'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

/** On mount, open the wizard at `intro` iff onboarding is neither completed
 * nor skipped (i.e. pending or deferred). Non-blocking: failures are swallowed. */
export function useOnboardingGate(): void {
  const setWizard = useSetAtom(minicpmWizardAtom)
  React.useEffect(() => {
    let cancelled = false
    getOnboardingState()
      .then((state) => {
        if (cancelled) return
        if (state !== 'completed' && state !== 'skipped') {
          setWizard((s) => ({ ...s, step: 'intro' }))
        }
      })
      .catch(() => { /* non-blocking */ })
    return () => { cancelled = true }
  }, [setWizard])
}
```

> **Decision:** opening at start for BOTH `pending` and `deferred` matches "稍后 = re-promptable". If you prefer 稍后 to suppress until next launch only-once, that's a future refinement — the spec says deferred is re-runnable, and re-prompting next launch is acceptable and simplest. Note this in the PR body.

- [ ] **Step 2: Mount in `App.tsx`** — call `useOnboardingGate()` in the root component and render `<MiniCPMWizard />` near the other top-level overlays (find where `InstallWizard`/dialogs mount; add alongside). Import both.

- [ ] **Step 3: Wire the needs-download dot** — in `SettingsPanel.tsx`, compute `minicpmNeedsDownload` (e.g. from a `localModelList()` call or reuse the existing model-status atom if cheap) and pass it to `<SettingsNav minicpmNeedsDownload={...} />`. If wiring a live value is heavy, pass `false` and leave a `// TODO Slice D+: live needs-download dot` — the dot is cosmetic. Keep it simple; do not block.

- [ ] **Step 4: Test the gate `useOnboardingGate.test.tsx`**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { Provider, createStore, useAtomValue } from 'jotai'
import * as React from 'react'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

const getOnboardingState = vi.fn()
vi.mock('@/lib/tauri-bridge', () => ({ getOnboardingState: () => getOnboardingState() }))

import { useOnboardingGate } from './useOnboardingGate'

function setup() {
  const store = createStore()
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    <Provider store={store}>{children}</Provider>
  const r = renderHook(() => { useOnboardingGate(); return useAtomValue(minicpmWizardAtom) }, { wrapper })
  return { store, result: r.result }
}

beforeEach(() => getOnboardingState.mockReset())

describe('useOnboardingGate', () => {
  it('opens wizard when pending', async () => {
    getOnboardingState.mockResolvedValue('pending')
    const { result } = setup()
    await waitFor(() => expect(result.current.step).toBe('intro'))
  })
  it('opens wizard when deferred', async () => {
    getOnboardingState.mockResolvedValue('deferred')
    const { result } = setup()
    await waitFor(() => expect(result.current.step).toBe('intro'))
  })
  it('does NOT open when completed', async () => {
    getOnboardingState.mockResolvedValue('completed')
    const { result } = setup()
    await new Promise((r) => setTimeout(r, 30))
    expect(result.current.step).toBeNull()
  })
  it('does NOT open when skipped', async () => {
    getOnboardingState.mockResolvedValue('skipped')
    const { result } = setup()
    await new Promise((r) => setTimeout(r, 30))
    expect(result.current.step).toBeNull()
  })
})
```

- [ ] **Step 5: Build TS + run tests + full UI test sweep**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` → clean.
Run: `cd ui && npm test -- --run useOnboardingGate 2>&1 | tail -15` → 4 pass.
Run: `cd ui && npm test -- --run 2>&1 | tail -12` → no NEW failures vs baseline (note any pre-existing failures unrelated to this slice).

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(ui): app-start onboarding gate + mount MiniCPMWizard overlay

Slice D Task 6. useOnboardingGate opens the wizard at intro when onboarding is
pending/deferred (not completed/skipped); mounted non-blocking in App root.
Vitest covers all 4 states. Closes the A→D zero-config loop."
```

---

## Final verification (before PR)

- [ ] **Backend:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` empty; `cargo test --lib local_llm::env_check local_llm::onboarding 2>&1 | tail` all pass; `cargo clippy --lib 2>&1 | grep -iE "env_check|onboarding|tauri_commands" | head` clean.
- [ ] **Two-edit audit:** all 5 new commands (`local_model_env_check`, `local_model_warmup`, `local_model_smoke_test`, `get_onboarding_state`, `set_onboarding_state`) in BOTH `tauri_commands.rs` and `main.rs`.
- [ ] **Frontend:** `cd ui && npx tsc --noEmit` clean; `npm test -- --run` — new MiniCPM tests pass, no new regressions.
- [ ] **GitNexus:** `gitnexus_detect_changes()`.
- [ ] **Manual end-to-end (the payoff — real weights):** launch app fresh (or with onboarding `pending`) → wizard appears → env-check shows hardware → pick source → download ~688 MB with live progress → warmup → smoke-test shows a real Chinese reply → done auto-wires `utility`+`summarizer` → confirm 模型分配 shows `local/minicpm5-1b` for both. **This is the first full download→load→generate run against real weights — the B↔C↔D loop closed.**

## PR body must call out
- **5 new Tauri commands** (two-edit; DMZ file `tauri_commands.rs` touched → second-session review).
- **warmup/smoke_test drive the running engine over HTTP-to-self** (no second 688 MB load).
- **DMZ note** + no migration.
- **Known gaps:** deferred re-prompts every launch (simplest interpretation; refine later); needs-download dot may be stubbed `false`; the manual real-weights E2E is the only validation of warmup/smoke_test (no unit test for HTTP-to-self); env-check RAM thresholds are heuristic.
- **Commits (bisectable):** one row per Task 1–6.

## Self-review notes (plan-authoring time)
- **Spec coverage:** env-check (os/arch/ram/disk/metal/cores/recommended_quant/warnings) ✓ T1; warmup + smoke_test ✓ T2; onboarding tri-state + get/set ✓ T3; Settings MiniCPM tab + bridge ✓ T4; wizard step machine (intro→envcheck→source→download→warmup→smoketest→done + error) + done auto-wires roles ✓ T5; app-start non-blocking gate + 稍后/不再提示 tri-state ✓ T6. Reuses Slice C probe/download/cancel + Slice B engine + Slice A setRoleModel ✓.
- **Type consistency:** `EnvReport`/`OnboardingState`/`MiniCpmWizardState`/`MiniCpmWizardStep` consistent across Rust + TS + atom + components; `MiniCpmDownloadProgress`/`ProbedSource`/`LocalInstalledModel` reused from Slice C.
- **Open implementer confirmations (flagged inline):** sysinfo 0.31 `total_memory()` bytes-vs-KB + `System::new()`/`refresh_memory` (T1); the wizard happy-path test setup needs `sources` seeded for determinism (T5); App.tsx exact overlay mount site + needs-download-dot value source (T6); deferred re-prompt policy (T6).
</content>
</invoke>
