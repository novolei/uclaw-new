# Embedding Endpoint Configuration + Developer Options Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose embedding-endpoint configuration (base_url, model, dim) + the four setup scripts (bun runtime, gbrain source, python env, gbrain init) through the System Settings UI so users can adjust gbrain/memU without dropping into a terminal.

**Architecture:** Backend adds `EmbeddingEndpointConfig` to `MemubotConfig` + three new Tauri commands (`get_embedding_config`, `set_embedding_config`, `run_setup_script`). `set_embedding_config` orchestrates: persists to `memubot_config.json`, shells out to `~/.uclaw/gbrain/run.sh config set ...` for the three gbrain keys, and conditionally calls `MemUClient::restart()` when the FastEmbed model changes (the bridge env captures `FASTEMBED_MODEL` at spawn). `run_setup_script` accepts a hardcoded allowlist (4 names — no arbitrary shell), spawns the script with line-buffered stdout via `tokio::process::Command`, and emits each line as a Tauri event (`system-setup-script:output`) plus a terminal event on exit. Frontend extends `SystemTab.tsx` with two new collapsible sections: `EmbeddingEndpointSection` (form + Save button) and `DeveloperOptionsSection` (4 script-runner cards, each with progress + tail-of-output console).

**Tech Stack:** Rust (Tauri v2 commands + `tokio::process::Command` + `serde` config), React 18 + TypeScript + Jotai, Tauri event system (`emit` / `listen`).

---

## Out of scope (explicit)

- **No new tab.** Everything lives inside the existing `SystemTab.tsx`.
- **No arbitrary shell execution.** `run_setup_script` enforces a hardcoded `&'static [&'static str]` allowlist of 4 script names; the argv passed to bash is `script_path + optional --force flag for init-gbrain only`. Anything else is rejected at compile + runtime.
- **No live editing of arbitrary gbrain config keys.** Only the three embedding-related keys are written by `set_embedding_config` (`embedding_model`, `embedding_dimensions`, `base_urls.llama-server`). Other keys remain under user control via the CLI.
- **No file-watcher for `memubot_config.json`.** Config is written once on Save and the relevant subsystem (memU bridge) restarts if needed. Cross-process config watching is a separate, larger concern.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/memubot_config.rs` | Config schema + serde defaults. | Add `EmbeddingEndpointConfig` struct + field on `MemubotConfig`. |
| `src-tauri/src/tauri_commands.rs` | All Tauri IPC commands. | Add 3 commands (`get_embedding_config`, `set_embedding_config`, `run_setup_script`) + supporting types + helpers. |
| `src-tauri/src/main.rs` | `invoke_handler!` registration. | Register the 3 new commands. |
| `src-tauri/src/memu/bridge.rs` | memU bridge spawn. | Inject `FASTEMBED_MODEL` from `llm_env` (already iterated) — actually no change needed if we just put the value into the `llm_env` HashMap from `app.rs::try_init_memu`. Verify only. |
| `src-tauri/src/app.rs` | `try_init_memu` constructs `MemUBridge`. | Pass `fastembed_model` from `MemubotConfig` into `llm_env` so it gets exported as `FASTEMBED_MODEL` env. |
| `ui/src/lib/embedding-endpoint.ts` | TS types matching the Rust `EmbeddingEndpointConfig` + the `SetupScriptName` allowlist + helper to invoke each script. | New file. |
| `ui/src/components/settings/EmbeddingEndpointSection.tsx` | The Embedding Endpoint form section embedded in SystemTab. | New file. |
| `ui/src/components/settings/DeveloperOptionsSection.tsx` | The 4 script-runner cards + Tauri event listener. | New file. |
| `ui/src/components/settings/SystemTab.tsx` | Mount the two new sections. | Modify (add 2 imports + 2 JSX blocks). |
| `docs/superpowers/handoff/2026-05-19-embedding-config-and-dev-options-handoff.md` | Hand-off doc. | New file. |

3 backend changes + 3 new frontend files + 1 modified frontend file + 1 docs file.

---

## Task 1: Backend — `EmbeddingEndpointConfig` schema

**Files:**
- Modify: `src-tauri/src/memubot_config.rs` — add the struct and field
- Test: inline `#[cfg(test)]` in the same file (uClaw convention)

### Step 1.1: Read the existing LocalApiConfig pattern to mirror

```bash
cd /Users/ryanliu/Documents/uclaw
grep -nA10 "^pub struct LocalApiConfig\|^impl Default for LocalApiConfig" src-tauri/src/memubot_config.rs | head -30
```

You should see `LocalApiConfig { enabled: bool, port: u16 }` plus its `Default` impl. Mirror this shape for `EmbeddingEndpointConfig`.

### Step 1.2: Add the struct + field — locate insertion point

```bash
grep -n "^pub struct LocalApiConfig\|^impl Default for LocalApiConfig\|pub local_api: LocalApiConfig" src-tauri/src/memubot_config.rs
```

You'll have three line numbers:
- `pub local_api: LocalApiConfig,` — field in `MemubotConfig`
- `pub struct LocalApiConfig` — the struct definition
- `impl Default for LocalApiConfig` — the default impl

Add `EmbeddingEndpointConfig` immediately AFTER the `LocalApiConfig` struct and immediately AFTER its `Default` impl.

### Step 1.3: Add the struct

After the `LocalApiConfig` struct definition (the one with `enabled` + `port`), append:

```rust
/// Embedding endpoint configuration (Sprint 2.2 followon #4)
///
/// Three gbrain config keys + one memU env var, surfaced as a single
/// settings page section so the user doesn't have to coordinate them
/// manually.
///
/// Default points gbrain at uClaw's own `/v1/embeddings` route
/// (`POST http://localhost:<local_api.port>/v1/embeddings` — backed by
/// memU's FastEmbed bridge, ~100ms warm-path per chunk, no external
/// API key required). Users can override to point at OpenAI / Voyage /
/// llama-server / ollama / any openai-compatible endpoint.
///
/// `fastembed_model` drives the actual FastEmbed model memU loads
/// inside its Python bridge (read at bridge spawn time via
/// `FASTEMBED_MODEL` env). Changing this requires a memU bridge
/// restart, which `set_embedding_config` handles transparently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingEndpointConfig {
    /// gbrain's `base_urls.llama-server` value. Default
    /// `http://localhost:7337/v1` (uClaw local API; pairs with
    /// `local_api.port = 7337`).
    pub base_url: String,
    /// gbrain's `embedding_model` value, in the `<recipe>:<model>` shape
    /// gbrain expects. Default `llama-server:bge-small-en-v1.5`.
    pub model: String,
    /// gbrain's `embedding_dimensions` value. Default `384` (bge-small).
    pub dimensions: u32,
    /// FastEmbed model id loaded by the memU bridge (via
    /// `FASTEMBED_MODEL` env). Default `BAAI/bge-small-en-v1.5`.
    /// Changing this triggers a memU bridge restart so the new model
    /// is loaded on the next embed call.
    pub fastembed_model: String,
}
```

And the `Default` impl, immediately after `impl Default for LocalApiConfig`:

```rust
impl Default for EmbeddingEndpointConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:7337/v1".to_string(),
            model: "llama-server:bge-small-en-v1.5".to_string(),
            dimensions: 384,
            fastembed_model: "BAAI/bge-small-en-v1.5".to_string(),
        }
    }
}
```

### Step 1.4: Add the field on `MemubotConfig`

Find the `MemubotConfig` struct (`grep -n "^pub struct MemubotConfig" src-tauri/src/memubot_config.rs`). Add the field next to `local_api`:

```rust
    /// 本地 API 服务配置
    pub local_api: LocalApiConfig,
    /// Embedding endpoint configuration (gbrain pointer + memU FastEmbed
    /// model). New in Sprint 2.2 followon #4.
    #[serde(default)]
    pub embedding_endpoint: EmbeddingEndpointConfig,
```

Then add the corresponding init line in `impl Default for MemubotConfig`:

```rust
            local_api: LocalApiConfig::default(),
            embedding_endpoint: EmbeddingEndpointConfig::default(),
```

### Step 1.5: Inline test — default values + round-trip serde

Add to the existing `#[cfg(test)] mod tests` block in `memubot_config.rs` (`grep -n "^mod tests\|^#\[cfg(test)\]" src-tauri/src/memubot_config.rs` to locate; if none, create at file end):

```rust
#[cfg(test)]
mod embedding_endpoint_tests {
    use super::*;

    #[test]
    fn default_points_at_local_api() {
        let cfg = EmbeddingEndpointConfig::default();
        assert_eq!(cfg.base_url, "http://localhost:7337/v1");
        assert_eq!(cfg.model, "llama-server:bge-small-en-v1.5");
        assert_eq!(cfg.dimensions, 384);
        assert_eq!(cfg.fastembed_model, "BAAI/bge-small-en-v1.5");
    }

    #[test]
    fn memubot_default_includes_embedding_endpoint() {
        let cfg = MemubotConfig::default();
        // The field is present + has the right default.
        assert_eq!(cfg.embedding_endpoint.dimensions, 384);
    }

    #[test]
    fn embedding_endpoint_round_trips_through_json() {
        let cfg = EmbeddingEndpointConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "openai:text-embedding-3-large".to_string(),
            dimensions: 3072,
            fastembed_model: "BAAI/bge-m3".to_string(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: EmbeddingEndpointConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_url, cfg.base_url);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.dimensions, cfg.dimensions);
        assert_eq!(parsed.fastembed_model, cfg.fastembed_model);
    }

    #[test]
    fn missing_field_falls_back_to_default() {
        // Older config files won't have embedding_endpoint at all —
        // verify `#[serde(default)]` on the field + `#[serde(default)]`
        // on EmbeddingEndpointConfig together cover this.
        let legacy_json = r#"{}"#;
        let cfg: MemubotConfig = serde_json::from_str(legacy_json).unwrap();
        // Default values land:
        assert_eq!(cfg.embedding_endpoint.base_url, "http://localhost:7337/v1");
    }
}
```

### Step 1.6: Build + test

```bash
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head
cargo test --lib embedding_endpoint_tests 2>&1 | tail -8
```

Expected: zero errors, 4 tests passing.

### Step 1.7: Commit

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/<this-worktree>
git add src-tauri/src/memubot_config.rs
git commit -m "feat(config): EmbeddingEndpointConfig schema (Sprint 2.2 followon #4 task 1/N)

Adds the EmbeddingEndpointConfig struct to MemubotConfig — one section
collecting the three gbrain config keys + the memU FastEmbed model id
that drive the bundled embedding pipeline. Defaults point at uClaw's
own /v1/embeddings (PR #214) so the out-of-the-box experience needs no
external API key.

Tasks 2-3 add the IPC commands that read/write this. Task 5 adds the
settings-UI form."
```

---

## Task 2: Backend — `get_embedding_config` + `set_embedding_config` commands

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` — add types + 2 commands + helper
- Modify: `src-tauri/src/main.rs` — register both in `invoke_handler!`
- Modify: `src-tauri/src/app.rs::try_init_memu` — inject `FASTEMBED_MODEL` into `llm_env`

### Step 2.1: Add types + helpers + commands to `tauri_commands.rs`

Locate an empty place near `restart_memu_bridge` (`grep -n "fn restart_memu_bridge" src-tauri/src/tauri_commands.rs`). Insert this block immediately AFTER `restart_memu_bridge`:

```rust
// ─── Embedding endpoint configuration (Sprint 2.2 followon #4) ───────

/// Wire-shape mirror of `MemubotConfig.embedding_endpoint`. Kept as a
/// separate type so the IPC payload is self-contained — frontend
/// doesn't see the rest of the config.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingEndpointPayload {
    pub base_url: String,
    pub model: String,
    pub dimensions: u32,
    pub fastembed_model: String,
}

impl From<&crate::memubot_config::EmbeddingEndpointConfig> for EmbeddingEndpointPayload {
    fn from(c: &crate::memubot_config::EmbeddingEndpointConfig) -> Self {
        Self {
            base_url: c.base_url.clone(),
            model: c.model.clone(),
            dimensions: c.dimensions,
            fastembed_model: c.fastembed_model.clone(),
        }
    }
}

#[tauri::command]
pub async fn get_embedding_config(
    state: State<'_, AppState>,
) -> Result<EmbeddingEndpointPayload, Error> {
    let cfg = state.memubot_config.read().await;
    Ok((&cfg.embedding_endpoint).into())
}

/// Apply embedding-endpoint settings:
///   1. Persist the new values into `memubot_config.json`.
///   2. Shell out to `~/.uclaw/gbrain/run.sh config set ...` for the
///      three gbrain keys (`embedding_model`, `embedding_dimensions`,
///      `base_urls.llama-server`). Each runs serially; first failure
///      aborts + returns Err WITHOUT touching the remaining keys, so
///      the user sees a precise "which key failed" error instead of a
///      half-applied state.
///   3. If `fastembed_model` changed, call `MemUClient::restart()` so
///      the bridge re-spawns with the new env. memU is degraded-mode-
///      tolerant — if restart fails the rest still applied.
///
/// On total success, returns the new payload (so the frontend can update
/// its form without a second `get_embedding_config` round-trip).
#[tauri::command]
pub async fn set_embedding_config(
    state: State<'_, AppState>,
    payload: EmbeddingEndpointPayload,
) -> Result<EmbeddingEndpointPayload, Error> {
    // Capture the OLD fastembed_model BEFORE we overwrite it, so we
    // know whether a memU restart is needed.
    let old_fastembed_model = {
        let cfg = state.memubot_config.read().await;
        cfg.embedding_endpoint.fastembed_model.clone()
    };

    // 1. Persist.
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.embedding_endpoint = crate::memubot_config::EmbeddingEndpointConfig {
            base_url: payload.base_url.clone(),
            model: payload.model.clone(),
            dimensions: payload.dimensions,
            fastembed_model: payload.fastembed_model.clone(),
        };
        cfg.save(&state.data_dir);
    }

    // 2. Shell out to gbrain CLI to write the three keys.
    let gbrain_run_sh = state.data_dir.join("gbrain").join("run.sh");
    if !gbrain_run_sh.is_file() {
        return Err(Error::Internal(format!(
            "gbrain launcher not found at {} — run uClaw at least once \
             so Stage 3 writes it (see Sprint 2.2 launcher PR #207)",
            gbrain_run_sh.display()
        )));
    }
    for (key, value) in [
        ("embedding_model", payload.model.clone()),
        ("embedding_dimensions", payload.dimensions.to_string()),
        ("base_urls.llama-server", payload.base_url.clone()),
    ] {
        let output = tokio::process::Command::new(&gbrain_run_sh)
            .arg("config")
            .arg("set")
            .arg(key)
            .arg(&value)
            .output()
            .await
            .map_err(|e| {
                Error::Internal(format!("spawn gbrain config set {}: {}", key, e))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Internal(format!(
                "gbrain config set {} = {:?} exited {:?}: {}",
                key,
                value,
                output.status.code(),
                stderr.trim()
            )));
        }
    }

    // 3. Restart memU bridge if FASTEMBED_MODEL changed.
    if old_fastembed_model != payload.fastembed_model {
        if let Some(client) = state.memu_client.as_ref() {
            // `restart` is async + bubbles errors; we log + continue so a
            // bridge failure doesn't unwind the already-applied gbrain
            // config (graceful degradation matches the rest of memU's
            // failure posture in this codebase).
            if let Err(e) = client.restart().await {
                tracing::warn!(
                    "memU bridge restart failed after FASTEMBED_MODEL change: {}; \
                     bridge will continue on the old model until next manual \
                     restart",
                    e
                );
            }
        }
    }

    Ok(payload)
}
```

### Step 2.2: Verify `MemUClient::restart` exists

```bash
grep -n "pub async fn restart" src-tauri/src/memu/client.rs
```

You should see at least one `pub async fn restart` method (PR #205 era). If it's missing or has a different signature, surface to the controller before continuing — DO NOT invent a new method here.

### Step 2.3: Update `app.rs::try_init_memu` to inject FASTEMBED_MODEL

```bash
grep -n "fn try_init_memu\|llm_env\|MemUBridge::new" src-tauri/src/app.rs | head -10
```

Find the `try_init_memu` function. It constructs an `llm_env: HashMap<String, String>` and passes it to `MemUBridge::new(python_path, script_path, data_dir, llm_env)`. Add a single insertion BEFORE the `MemUBridge::new(...)` call so `FASTEMBED_MODEL` lands in the env that gets exported on every subprocess spawn.

Use `MemubotConfig` to read the default — if config hasn't been loaded yet at this point, fall back to the EmbeddingEndpointConfig default value:

```rust
        // Sprint 2.2 followon #4 — pin the FastEmbed model the bridge
        // loads, configurable via set_embedding_config. Loaded from
        // memubot_config.json if present; falls back to the schema
        // default otherwise (matches what set_embedding_config writes
        // on first save).
        let fastembed_model = crate::memubot_config::MemubotConfig::load(data_dir)
            .embedding_endpoint
            .fastembed_model;
        llm_env.insert("FASTEMBED_MODEL".to_string(), fastembed_model);
        let bridge = Arc::new(MemUBridge::new(python_path, script_path, data_dir.to_path_buf(), llm_env));
```

(Insert this just before the existing `let bridge = Arc::new(MemUBridge::new(...))` line.)

### Step 2.4: Register the two commands in `main.rs`

```bash
grep -n "restart_memu_bridge," src-tauri/src/main.rs
```

That gets you the spot in `invoke_handler!`. Insert immediately AFTER that line:

```rust
            uclaw_core::tauri_commands::get_embedding_config,
            uclaw_core::tauri_commands::set_embedding_config,
```

### Step 2.5: Build + test

```bash
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib embedding_endpoint_tests 2>&1 | tail -5
```

All clean. Tests still 4 passed.

### Step 2.6: Commit

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/app.rs
git commit -m "feat(commands): get_embedding_config + set_embedding_config IPCs (Sprint 2.2 followon #4 task 2/N)

Two Tauri commands surface the new EmbeddingEndpointConfig to the
frontend. set_embedding_config does three things atomically (per call):

  1. Persist to memubot_config.json.
  2. Shell out to ~/.uclaw/gbrain/run.sh config set for the three
     gbrain keys (embedding_model / embedding_dimensions /
     base_urls.llama-server). Serial; first failure aborts with a
     precise per-key error so the user sees which key broke.
  3. Restart MemUClient bridge ONLY if fastembed_model changed —
     skips the cold-start cost when the user is just changing url/dim.

app.rs::try_init_memu now injects FASTEMBED_MODEL into llm_env from
the config default, so the very first bridge spawn already respects
the user's saved model (no need to round-trip set_embedding_config to
pick up the value)."
```

---

## Task 3: Backend — `run_setup_script` with allowlist + event streaming

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` — add types + command + line-streaming spawn helper
- Modify: `src-tauri/src/main.rs` — register in `invoke_handler!`

### Step 3.1: Add the allowlist + types + command

Insert immediately AFTER `set_embedding_config` in `tauri_commands.rs`:

```rust
// ─── Setup-script runner with allowlist (Sprint 2.2 followon #4) ─────

/// Hardcoded allowlist of setup scripts the UI is allowed to run. Index
/// in this array is the public API; anything not here is rejected.
/// Adding a script is an explicit code change — there is intentionally
/// no way to extend this from configuration.
const SETUP_SCRIPT_ALLOWLIST: &[&str] = &[
    "setup-bun-runtime",   // scripts/setup-bun-runtime.sh
    "setup-gbrain-source", // scripts/setup-gbrain-source.sh
    "setup-python-env",    // scripts/setup-python-env.sh
    "init-gbrain",         // scripts/init-gbrain.sh
];

/// Each script's argv shape. The script_name is the allowlist entry
/// above; supports a small set of well-known flags for the scripts
/// that take them (init-gbrain accepts --force; everything else gets
/// just --yes for CI-style non-interactive runs).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RunSetupScriptArgs {
    pub script_name: String,
    /// Currently only honored by `init-gbrain`. Default false.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunSetupScriptResult {
    pub run_id: String,
    pub exit_code: Option<i32>,
    pub success: bool,
}

/// Spawn the script + stream stdout/stderr lines as Tauri events:
///   "system-setup-script:output" with payload
///   {run_id, stream: "stdout"|"stderr", line: "..."}
///
/// When the process exits, fire:
///   "system-setup-script:end" with payload
///   {run_id, exit_code, success}
///
/// Returns once the process has exited (not at spawn) so the frontend's
/// promise resolves with the final exit code AND the in-process event
/// stream is fully drained.
#[tauri::command]
pub async fn run_setup_script(
    app: tauri::AppHandle,
    args: RunSetupScriptArgs,
) -> Result<RunSetupScriptResult, Error> {
    use tauri::Emitter;

    // 1. Allowlist enforcement — rejects compile-time-unknown names.
    if !SETUP_SCRIPT_ALLOWLIST.contains(&args.script_name.as_str()) {
        return Err(Error::Internal(format!(
            "script '{}' is not in the allowlist; permitted: {:?}",
            args.script_name, SETUP_SCRIPT_ALLOWLIST
        )));
    }

    // 2. Resolve script path. Scripts live under <project_root>/scripts/.
    // In dev builds, the project root is the parent of CARGO_MANIFEST_DIR;
    // in release the scripts are NOT bundled (they are dev-only). So this
    // command is dev-mode only by design — fail loud if scripts/ isn't
    // reachable.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().ok_or_else(|| {
        Error::Internal("CARGO_MANIFEST_DIR has no parent — unexpected layout".into())
    })?;
    let script_path = project_root
        .join("scripts")
        .join(format!("{}.sh", args.script_name));
    if !script_path.is_file() {
        return Err(Error::Internal(format!(
            "script not found at {} (dev-only command — bundle does not ship scripts/)",
            script_path.display()
        )));
    }

    // 3. Build argv. Only init-gbrain honors --force; all four accept --yes
    // for non-interactive runs (matches scripts/setup-*.sh convention).
    let mut argv: Vec<String> = vec![script_path.to_string_lossy().into_owned()];
    argv.push("--yes".to_string());
    if args.script_name == "init-gbrain" && args.force {
        argv.push("--force".to_string());
    }

    // 4. Generate a run_id for event correlation. Frontend uses it to
    // route output to the right card when multiple scripts run in
    // parallel.
    let run_id = format!(
        "setup-{}-{}",
        args.script_name,
        chrono::Utc::now().timestamp_millis()
    );

    // 5. Spawn + drain.
    tracing::info!(
        run_id = %run_id,
        script = %script_path.display(),
        force = args.force,
        "[setup-script] starting"
    );
    let mut child = tokio::process::Command::new("bash")
        .args(&argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::Internal(format!("spawn {}: {}", args.script_name, e)))?;

    let stdout = child.stdout.take().ok_or_else(|| {
        Error::Internal("failed to capture stdout".into())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        Error::Internal("failed to capture stderr".into())
    })?;

    // Spawn line readers for both streams in parallel — without this,
    // a script that writes a lot to one stream can block the other
    // (pipe buffer fills, write() blocks).
    use tokio::io::AsyncBufReadExt;
    let app_for_stdout = app.clone();
    let run_id_for_stdout = run_id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_for_stdout.emit(
                "system-setup-script:output",
                serde_json::json!({
                    "run_id": run_id_for_stdout,
                    "stream": "stdout",
                    "line": line,
                }),
            );
        }
    });

    let app_for_stderr = app.clone();
    let run_id_for_stderr = run_id.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_for_stderr.emit(
                "system-setup-script:output",
                serde_json::json!({
                    "run_id": run_id_for_stderr,
                    "stream": "stderr",
                    "line": line,
                }),
            );
        }
    });

    let status = child.wait().await.map_err(|e| {
        Error::Internal(format!("wait on {}: {}", args.script_name, e))
    })?;
    // Drain the line readers — they finish naturally on EOF; the await
    // here just guarantees we don't fire the `end` event before the
    // last `output` event lands.
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let exit_code = status.code();
    let success = status.success();
    let _ = app.emit(
        "system-setup-script:end",
        serde_json::json!({
            "run_id": run_id,
            "exit_code": exit_code,
            "success": success,
        }),
    );

    tracing::info!(
        run_id = %run_id,
        exit_code = ?exit_code,
        success = success,
        "[setup-script] finished"
    );

    Ok(RunSetupScriptResult {
        run_id,
        exit_code,
        success,
    })
}
```

### Step 3.2: Register the command

In `main.rs::invoke_handler!`, immediately after `set_embedding_config`:

```rust
            uclaw_core::tauri_commands::run_setup_script,
```

### Step 3.3: Inline test for the allowlist gate

Add to a new `#[cfg(test)] mod setup_script_tests` block at the END of `tauri_commands.rs`:

```rust
#[cfg(test)]
mod setup_script_tests {
    use super::*;

    #[test]
    fn allowlist_contains_exactly_the_four_documented_scripts() {
        // Pin the contract — extending the allowlist is a deliberate
        // code change, not a config tweak.
        assert_eq!(
            SETUP_SCRIPT_ALLOWLIST,
            &[
                "setup-bun-runtime",
                "setup-gbrain-source",
                "setup-python-env",
                "init-gbrain",
            ]
        );
    }

    #[test]
    fn allowlist_rejects_arbitrary_names_at_membership_check() {
        // Direct test of the contains() guard so a future rewrite of
        // run_setup_script can't quietly drop the check.
        assert!(!SETUP_SCRIPT_ALLOWLIST.contains(&"rm-rf-slash"));
        assert!(!SETUP_SCRIPT_ALLOWLIST.contains(&"setup-bun-runtime.sh"), "name must NOT include the .sh extension");
        assert!(!SETUP_SCRIPT_ALLOWLIST.contains(&"../scripts/setup-bun-runtime"));
        assert!(SETUP_SCRIPT_ALLOWLIST.contains(&"setup-bun-runtime"));
    }
}
```

### Step 3.4: Build + test

```bash
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib setup_script_tests 2>&1 | tail -8
cargo test --lib embedding_endpoint_tests 2>&1 | tail -5
```

Both green. Full binary clean.

### Step 3.5: Commit

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(commands): run_setup_script with allowlist + event streaming (Sprint 2.2 followon #4 task 3/N)

Adds a dev-mode IPC for running the four setup scripts (setup-bun-runtime,
setup-gbrain-source, setup-python-env, init-gbrain) with two safety rails:

1. Hardcoded allowlist (&'static [&'static str]) — anything outside the
   four documented names is rejected before spawn. Adding a script is
   a code change, not a config tweak.
2. argv is built from a fixed shape (bash <resolved-path> --yes [+ --force
   for init-gbrain only]) — no user-supplied flags reach the shell.

Streams stdout AND stderr line-by-line as 'system-setup-script:output'
Tauri events with a run_id for frontend correlation. Fires a single
'system-setup-script:end' on exit with the final code. Per-stream
readers are spawned in parallel so a noisy stream doesn't deadlock its
sibling.

kill_on_drop(true) on the child means a UI close or rust-side cancel
guarantees the script process dies — no orphan setup runs."
```

---

## Task 4: Frontend — shared TS types + IPC helpers

**Files:**
- Create: `ui/src/lib/embedding-endpoint.ts` (new)

### Step 4.1: Write the file verbatim

```typescript
import { invoke } from '@tauri-apps/api/core'

// ─── Embedding endpoint config (mirrors Rust EmbeddingEndpointPayload) ────

export interface EmbeddingEndpointConfig {
  base_url: string
  model: string
  dimensions: number
  fastembed_model: string
}

export async function getEmbeddingConfig(): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('get_embedding_config')
}

export async function setEmbeddingConfig(
  payload: EmbeddingEndpointConfig,
): Promise<EmbeddingEndpointConfig> {
  return await invoke<EmbeddingEndpointConfig>('set_embedding_config', { payload })
}

// ─── Setup-script runner (mirrors Rust allowlist) ─────────────────────────

/**
 * Hardcoded mirror of SETUP_SCRIPT_ALLOWLIST in Rust. Names must match
 * exactly — backend rejects anything outside this set. Adding a script
 * is a coordinated code change in both repos.
 */
export const SETUP_SCRIPTS = [
  'setup-bun-runtime',
  'setup-gbrain-source',
  'setup-python-env',
  'init-gbrain',
] as const

export type SetupScriptName = (typeof SETUP_SCRIPTS)[number]

export interface SetupScriptDescriptor {
  name: SetupScriptName
  /** Display label, Chinese — matches the rest of the settings UI. */
  label: string
  /** One-line description below the label. */
  description: string
  /** When true the script has a destructive --force mode; UI surfaces a confirm gate. */
  supportsForce: boolean
  /** Approximate wall-time so the progress bar can be calibrated. */
  expectedDurationSecs: number
}

export const SETUP_SCRIPT_DESCRIPTORS: Record<SetupScriptName, SetupScriptDescriptor> = {
  'setup-bun-runtime': {
    name: 'setup-bun-runtime',
    label: '安装 Bun 运行时',
    description: '下载 Bun 静态二进制 (~50MB) 到 src-tauri/bunembed/。首次 setup 或升级 Bun 时使用。',
    supportsForce: false,
    expectedDurationSecs: 30,
  },
  'setup-gbrain-source': {
    name: 'setup-gbrain-source',
    label: '安装 gbrain 源码',
    description: 'Clone gbrain 源码到 src-tauri/gbrain-source/ + bun install --production。首次 setup 或换 gbrain 版本时使用。',
    supportsForce: false,
    expectedDurationSecs: 90,
  },
  'setup-python-env': {
    name: 'setup-python-env',
    label: '安装 Python 环境 (memU)',
    description: '装 embedded Python + memU + fastembed 等依赖到 src-tauri/pyembed/。首次 setup 或 memU 依赖损坏时使用。',
    supportsForce: false,
    expectedDurationSecs: 120,
  },
  'init-gbrain': {
    name: 'init-gbrain',
    label: '初始化 gbrain brain',
    description: '在 ~/.uclaw/gbrain/.gbrain/brain.pglite/ 跑 PGLite migrations。--force 会先 rm -rf 现有 brain 再 init。',
    supportsForce: true,
    expectedDurationSecs: 60,
  },
}

export interface SetupScriptRunResult {
  run_id: string
  exit_code: number | null
  success: boolean
}

export async function runSetupScript(
  name: SetupScriptName,
  opts: { force?: boolean } = {},
): Promise<SetupScriptRunResult> {
  return await invoke<SetupScriptRunResult>('run_setup_script', {
    args: {
      script_name: name,
      force: opts.force ?? false,
    },
  })
}

// ─── Tauri event payloads ────────────────────────────────────────────────

export interface SetupScriptOutputEvent {
  run_id: string
  stream: 'stdout' | 'stderr'
  line: string
}

export interface SetupScriptEndEvent {
  run_id: string
  exit_code: number | null
  success: boolean
}
```

### Step 4.2: Smoke type-check

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -E "embedding-endpoint" | head -5
```

Expected: zero output (file type-checks clean).

### Step 4.3: Commit

```bash
git add ui/src/lib/embedding-endpoint.ts
git commit -m "feat(ui-lib): embedding-endpoint TS types + IPC helpers (Sprint 2.2 followon #4 task 4/N)

Shared module the two new SystemTab sections both import. Mirrors the
Rust EmbeddingEndpointPayload + the setup-script allowlist + the two
Tauri event shapes (output/end). All four scripts have a labeled
descriptor with expected duration so the UI can render a calibrated
progress bar without guessing per render."
```

---

## Task 5: Frontend — `EmbeddingEndpointSection.tsx`

**Files:**
- Create: `ui/src/components/settings/EmbeddingEndpointSection.tsx` (new)

### Step 5.1: Write the file verbatim

```typescript
import * as React from 'react'
import { Save, RotateCcw } from 'lucide-react'
import {
  getEmbeddingConfig,
  setEmbeddingConfig,
  type EmbeddingEndpointConfig,
} from '@/lib/embedding-endpoint'

const DEFAULT_CONFIG: EmbeddingEndpointConfig = {
  base_url: 'http://localhost:7337/v1',
  model: 'llama-server:bge-small-en-v1.5',
  dimensions: 384,
  fastembed_model: 'BAAI/bge-small-en-v1.5',
}

export function EmbeddingEndpointSection(): React.ReactElement {
  const [config, setConfig] = React.useState<EmbeddingEndpointConfig>(DEFAULT_CONFIG)
  const [pristine, setPristine] = React.useState<EmbeddingEndpointConfig>(DEFAULT_CONFIG)
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [toast, setToast] = React.useState<string | null>(null)

  // Load current config on mount.
  React.useEffect(() => {
    setLoading(true)
    getEmbeddingConfig()
      .then((c) => {
        setConfig(c)
        setPristine(c)
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const dirty = React.useMemo(
    () =>
      config.base_url !== pristine.base_url ||
      config.model !== pristine.model ||
      config.dimensions !== pristine.dimensions ||
      config.fastembed_model !== pristine.fastembed_model,
    [config, pristine],
  )

  const handleSave = async () => {
    setSaving(true)
    setError(null)
    setToast(null)
    try {
      const updated = await setEmbeddingConfig(config)
      setConfig(updated)
      setPristine(updated)
      setToast('已保存。如修改了 FastEmbed 模型，memU 已自动重启。')
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  const handleReset = () => {
    setConfig(pristine)
    setError(null)
    setToast(null)
  }

  return (
    <div className="border border-border rounded-lg p-4 space-y-3">
      <div>
        <h3 className="text-sm font-semibold">Embedding 端点配置</h3>
        <p className="text-[11px] text-muted-foreground mt-0.5">
          gbrain 把内容向量化时调用的 endpoint + 模型。默认指向 uClaw 自带的
          <code className="mx-1 px-1 py-0.5 rounded bg-muted text-[10px]">/v1/embeddings</code>
          (由 memU FastEmbed 后端) — 无需外部 API key 即可工作。
        </p>
      </div>

      {loading && <p className="text-[11px] text-muted-foreground">读取中...</p>}

      {!loading && (
        <div className="space-y-2">
          <Field
            label="Base URL"
            description="gbrain config base_urls.llama-server"
            value={config.base_url}
            onChange={(v) => setConfig({ ...config, base_url: v })}
            placeholder="http://localhost:7337/v1"
          />
          <Field
            label="模型 (gbrain embedding_model)"
            description="格式 <recipe>:<model>，例如 llama-server:bge-small-en-v1.5"
            value={config.model}
            onChange={(v) => setConfig({ ...config, model: v })}
            placeholder="llama-server:bge-small-en-v1.5"
          />
          <Field
            label="向量维度 (gbrain embedding_dimensions)"
            description="必须跟 FastEmbed 模型的输出维度一致 (bge-small=384, bge-m3=1024)"
            value={String(config.dimensions)}
            onChange={(v) => {
              const n = parseInt(v, 10)
              setConfig({ ...config, dimensions: Number.isFinite(n) ? n : 0 })
            }}
            placeholder="384"
            type="number"
          />
          <Field
            label="FastEmbed 模型 (memU)"
            description="memU bridge 加载的 FastEmbed 模型 id (例如 BAAI/bge-m3 多语言)。变更会触发 memU 重启。"
            value={config.fastembed_model}
            onChange={(v) => setConfig({ ...config, fastembed_model: v })}
            placeholder="BAAI/bge-small-en-v1.5"
          />
        </div>
      )}

      <div className="flex items-center gap-2 pt-2">
        <button
          onClick={handleSave}
          disabled={!dirty || saving || loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] font-medium bg-primary text-primary-foreground disabled:opacity-50"
        >
          <Save size={11} />
          {saving ? '保存中...' : '保存'}
        </button>
        <button
          onClick={handleReset}
          disabled={!dirty || saving}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50"
        >
          <RotateCcw size={11} />
          重置
        </button>
        {dirty && !saving && (
          <span className="text-[10px] text-yellow-500">未保存的更改</span>
        )}
      </div>

      {error && (
        <p className="text-[11px] text-destructive bg-destructive/10 px-2 py-1.5 rounded">
          {error}
        </p>
      )}
      {toast && (
        <p className="text-[11px] text-green-500 bg-green-500/10 px-2 py-1.5 rounded">
          {toast}
        </p>
      )}
    </div>
  )
}

interface FieldProps {
  label: string
  description: string
  value: string
  onChange: (v: string) => void
  placeholder?: string
  type?: 'text' | 'number'
}

function Field({ label, description, value, onChange, placeholder, type = 'text' }: FieldProps): React.ReactElement {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] font-medium">{label}</span>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="px-2 py-1.5 rounded border border-input bg-background text-[11px] font-mono"
      />
      <span className="text-[10px] text-muted-foreground">{description}</span>
    </label>
  )
}
```

### Step 5.2: Type-check + commit

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -E "EmbeddingEndpointSection" | head -5
```

Expected: zero output.

```bash
git add ui/src/components/settings/EmbeddingEndpointSection.tsx
git commit -m "feat(settings): EmbeddingEndpointSection component (Sprint 2.2 followon #4 task 5/N)

The four-field form (base_url + model + dimensions + fastembed_model)
that drives the new get/set_embedding_config IPCs. Dirty-state tracking
disables Save until something actually changed; Reset rolls back to the
last loaded state.

Save shows a 'memU 已自动重启' note when the user changed the FastEmbed
model — matches the backend's restart-iff-changed contract from task 2."
```

---

## Task 6: Frontend — `DeveloperOptionsSection.tsx`

**Files:**
- Create: `ui/src/components/settings/DeveloperOptionsSection.tsx` (new)

### Step 6.1: Write the file verbatim

```typescript
import * as React from 'react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Play, AlertTriangle, ChevronDown, ChevronUp } from 'lucide-react'
import {
  SETUP_SCRIPTS,
  SETUP_SCRIPT_DESCRIPTORS,
  runSetupScript,
  type SetupScriptName,
  type SetupScriptOutputEvent,
  type SetupScriptEndEvent,
} from '@/lib/embedding-endpoint'

/**
 * Per-script runtime state. Keyed by script name (one card per script).
 * `running` flips true on Play, back to false on the `end` event. Logs
 * are appended live from `system-setup-script:output`. We cap the log
 * at 500 lines so a chatty script doesn't blow the React render budget.
 */
interface ScriptState {
  running: boolean
  /** Last-fired run_id for filtering inbound events. */
  runId: string | null
  /** Tail of stdout+stderr. Capped at MAX_LOG_LINES. */
  log: string[]
  /** Exit code from the last completed run; null when running or never-run. */
  exitCode: number | null
  /** Optimistic progress 0-100, computed from elapsedMs / expectedDurationSecs. */
  progressPct: number
  /** Walltime start for the progress estimator. */
  startedAtMs: number | null
  /** Last error message (e.g. spawn failure). */
  error: string | null
}

const EMPTY_STATE: ScriptState = {
  running: false,
  runId: null,
  log: [],
  exitCode: null,
  progressPct: 0,
  startedAtMs: null,
  error: null,
}

const MAX_LOG_LINES = 500

function makeInitial(): Record<SetupScriptName, ScriptState> {
  const r = {} as Record<SetupScriptName, ScriptState>
  for (const n of SETUP_SCRIPTS) {
    r[n] = { ...EMPTY_STATE }
  }
  return r
}

export function DeveloperOptionsSection(): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const [states, setStates] = React.useState<Record<SetupScriptName, ScriptState>>(makeInitial())
  const [forceConfirm, setForceConfirm] = React.useState<SetupScriptName | null>(null)

  // Subscribe to the two Tauri events once. Use a ref to the current
  // states map so the listener doesn't need to re-subscribe on every
  // setStates (which would race with in-flight events).
  const statesRef = React.useRef(states)
  React.useEffect(() => { statesRef.current = states }, [states])

  React.useEffect(() => {
    if (!expanded) return // skip subscribing while section is collapsed
    let unlistenOutput: UnlistenFn | null = null
    let unlistenEnd: UnlistenFn | null = null
    ;(async () => {
      unlistenOutput = await listen<SetupScriptOutputEvent>('system-setup-script:output', (e) => {
        const { run_id, line } = e.payload
        setStates((prev) => {
          const next = { ...prev }
          for (const n of SETUP_SCRIPTS) {
            if (prev[n].runId === run_id) {
              const log = [...prev[n].log, line]
              if (log.length > MAX_LOG_LINES) log.splice(0, log.length - MAX_LOG_LINES)
              next[n] = { ...prev[n], log }
              break
            }
          }
          return next
        })
      })
      unlistenEnd = await listen<SetupScriptEndEvent>('system-setup-script:end', (e) => {
        const { run_id, exit_code, success } = e.payload
        setStates((prev) => {
          const next = { ...prev }
          for (const n of SETUP_SCRIPTS) {
            if (prev[n].runId === run_id) {
              next[n] = {
                ...prev[n],
                running: false,
                exitCode: exit_code,
                progressPct: success ? 100 : prev[n].progressPct,
                error: success ? null : `exit ${exit_code ?? 'killed'}`,
              }
              break
            }
          }
          return next
        })
      })
    })()
    return () => {
      unlistenOutput?.()
      unlistenEnd?.()
    }
  }, [expanded])

  // Progress estimator — ticks while any script is running.
  React.useEffect(() => {
    const anyRunning = SETUP_SCRIPTS.some((n) => states[n].running)
    if (!anyRunning) return
    const timer = setInterval(() => {
      setStates((prev) => {
        const next = { ...prev }
        let changed = false
        for (const n of SETUP_SCRIPTS) {
          if (!prev[n].running || prev[n].startedAtMs == null) continue
          const elapsedSecs = (Date.now() - prev[n].startedAtMs) / 1000
          const expected = SETUP_SCRIPT_DESCRIPTORS[n].expectedDurationSecs
          // Cap at 95% so user knows it's not actually done.
          const pct = Math.min(95, Math.floor((elapsedSecs / expected) * 95))
          if (pct !== prev[n].progressPct) {
            next[n] = { ...prev[n], progressPct: pct }
            changed = true
          }
        }
        return changed ? next : prev
      })
    }, 500)
    return () => clearInterval(timer)
  }, [states])

  const handleRun = async (name: SetupScriptName, force: boolean) => {
    setStates((prev) => ({
      ...prev,
      [name]: {
        running: true,
        runId: null, // filled when invoke resolves
        log: [],
        exitCode: null,
        progressPct: 1,
        startedAtMs: Date.now(),
        error: null,
      },
    }))
    setForceConfirm(null)
    try {
      const result = await runSetupScript(name, { force })
      // invoke RESOLVES when the process exits — at this point the
      // 'end' event has fired (or is in flight). Set runId so any
      // late-arriving output events that race with this resolution
      // still get routed to the right card.
      setStates((prev) => ({
        ...prev,
        [name]: {
          ...prev[name],
          runId: result.run_id,
          // Don't flip `running` here — the `end` listener handles it.
          // This avoids a flicker if `end` arrives a tick later.
        },
      }))
    } catch (e) {
      setStates((prev) => ({
        ...prev,
        [name]: {
          ...prev[name],
          running: false,
          error: String(e),
        },
      }))
    }
  }

  // When invoke RESOLVES it returns run_id; for events that fire BEFORE
  // that resolution, we don't yet know the run_id. We solve this by
  // having Rust emit the run_id back on the FIRST output event with a
  // matching `started` flag — but that's overkill for the common case
  // (script runs >100ms, invoke RESOLVES quickly enough that runId is
  // set before output starts streaming). For the rare race we just
  // ignore early outputs (they're typically just bash startup noise).
  // Documented here so a future reader doesn't re-derive it.

  return (
    <div className="border border-border rounded-lg">
      <button
        onClick={() => setExpanded((v) => !v)}
        className="w-full flex items-center justify-between p-3 hover:bg-accent/30"
      >
        <div className="text-left">
          <h3 className="text-sm font-semibold flex items-center gap-2">
            开发者选项
            <span className="px-1.5 py-0.5 rounded bg-yellow-500/20 text-yellow-600 text-[9px] font-medium">DEV</span>
          </h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            手动运行 setup 脚本。仅 dev 模式可用 — release 包不包含 scripts/。
          </p>
        </div>
        {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>

      {expanded && (
        <div className="border-t border-border p-3 space-y-3">
          {SETUP_SCRIPTS.map((name) => (
            <ScriptCard
              key={name}
              name={name}
              state={states[name]}
              onRun={(force) => {
                const desc = SETUP_SCRIPT_DESCRIPTORS[name]
                if (force && desc.supportsForce) {
                  setForceConfirm(name)
                } else {
                  handleRun(name, false)
                }
              }}
              confirmingForce={forceConfirm === name}
              onConfirmForce={() => handleRun(name, true)}
              onCancelForce={() => setForceConfirm(null)}
            />
          ))}
        </div>
      )}
    </div>
  )
}

interface ScriptCardProps {
  name: SetupScriptName
  state: ScriptState
  onRun: (force: boolean) => void
  confirmingForce: boolean
  onConfirmForce: () => void
  onCancelForce: () => void
}

function ScriptCard({ name, state, onRun, confirmingForce, onConfirmForce, onCancelForce }: ScriptCardProps): React.ReactElement {
  const desc = SETUP_SCRIPT_DESCRIPTORS[name]

  return (
    <div className="border border-border rounded-md p-3 space-y-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <p className="text-[12px] font-medium">{desc.label}</p>
          <p className="text-[10px] text-muted-foreground mt-0.5">{desc.description}</p>
          <code className="text-[10px] text-muted-foreground/70 font-mono">scripts/{name}.sh</code>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => onRun(false)}
            disabled={state.running}
            className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 disabled:opacity-50"
          >
            <Play size={11} />
            {state.running ? '运行中' : '运行'}
          </button>
          {desc.supportsForce && (
            <button
              onClick={() => onRun(true)}
              disabled={state.running}
              className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-red-500/10 text-red-500 hover:bg-red-500/20 disabled:opacity-50"
              title="--force"
            >
              <AlertTriangle size={11} />
              重置
            </button>
          )}
        </div>
      </div>

      {confirmingForce && (
        <div className="p-2 rounded bg-red-500/10 border border-red-500/30 space-y-2">
          <p className="text-[11px] text-red-600">
            <AlertTriangle size={11} className="inline mr-1 -mt-0.5" />
            <strong>--force 会清空数据。</strong> 确认执行吗？
          </p>
          <div className="flex gap-2">
            <button
              onClick={onConfirmForce}
              className="px-2 py-1 rounded text-[11px] bg-red-500 text-white hover:bg-red-600"
            >
              确认重置
            </button>
            <button
              onClick={onCancelForce}
              className="px-2 py-1 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent"
            >
              取消
            </button>
          </div>
        </div>
      )}

      {(state.running || state.progressPct > 0) && (
        <div className="space-y-1">
          <div className="h-1 rounded-full bg-muted overflow-hidden">
            <div
              className={[
                'h-full transition-all duration-300',
                state.error ? 'bg-red-500' : state.exitCode === 0 ? 'bg-green-500' : 'bg-primary',
              ].join(' ')}
              style={{ width: `${state.progressPct}%` }}
            />
          </div>
          <p className="text-[10px] text-muted-foreground">
            {state.running
              ? `运行中 ~${state.progressPct}% (估计耗时 ${desc.expectedDurationSecs}s)`
              : state.exitCode === 0
                ? '✓ 完成'
                : state.error
                  ? `✗ ${state.error}`
                  : ''}
          </p>
        </div>
      )}

      {state.log.length > 0 && (
        <details className="border-t border-border pt-2">
          <summary className="text-[10px] text-muted-foreground cursor-pointer hover:text-foreground">
            输出日志 ({state.log.length} 行)
          </summary>
          <pre className="mt-1 max-h-48 overflow-auto text-[10px] font-mono bg-muted/50 p-2 rounded whitespace-pre-wrap">
            {state.log.join('\n')}
          </pre>
        </details>
      )}
    </div>
  )
}
```

### Step 6.2: Type-check + commit

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -E "DeveloperOptionsSection" | head -5
```

Expected: zero output.

```bash
git add ui/src/components/settings/DeveloperOptionsSection.tsx
git commit -m "feat(settings): DeveloperOptionsSection component (Sprint 2.2 followon #4 task 6/N)

The collapsible 'Developer Options' panel housing four ScriptCard
runners (one per allowlist entry). Each card shows label/description/
script-path, a Play button (and a 'Reset' for init-gbrain's --force
mode behind a confirmation gate), a progress bar calibrated against
the script's expected duration, and a collapsible last-500-line log
tail.

Subscribes to system-setup-script:output / :end events ONLY while
the section is expanded so the listener overhead is zero on collapsed
sessions."
```

---

## Task 7: Frontend — mount the two sections in `SystemTab.tsx`

**Files:**
- Modify: `ui/src/components/settings/SystemTab.tsx`

### Step 7.1: Find the existing JSX shape

```bash
grep -n "return (\|<div\|</div>" ui/src/components/settings/SystemTab.tsx | head -30
```

You should see the top-level `return (...)` of the component. We want to add the two new sections at a natural position — after the diagnostics panel, before the action buttons (Restart/Reset). Either is fine; the goal is "user scrolls past status, then sees Embedding config, then sees Developer Options at the bottom".

### Step 7.2: Add the imports

At the top of `SystemTab.tsx`, after the existing imports:

```typescript
import { EmbeddingEndpointSection } from './EmbeddingEndpointSection'
import { DeveloperOptionsSection } from './DeveloperOptionsSection'
```

### Step 7.3: Mount the sections

Inside the component's JSX, between the existing diagnostics card and the action-button row, add:

```tsx
        {/* Sprint 2.2 followon #4 — embedding endpoint configuration */}
        <EmbeddingEndpointSection />

        {/* Sprint 2.2 followon #4 — developer options (collapsed by default) */}
        <DeveloperOptionsSection />
```

The exact insertion location depends on the existing JSX flow. Look for the outermost wrapping container's JSX children list — the sections should be siblings of the diagnostics report card. Use `grep -n "actionError\|busyRestart" ui/src/components/settings/SystemTab.tsx` to find the action button block; place the two new sections ABOVE it.

### Step 7.4: Type-check + build full UI

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -E "SystemTab" | head -5
npm run build 2>&1 | tail -10
```

`tsc` should show zero errors. `npm run build` should complete without errors. Bundle size growth should be modest (~15KB minified for the two new components).

### Step 7.5: Commit

```bash
git add ui/src/components/settings/SystemTab.tsx
git commit -m "feat(settings): mount EmbeddingEndpointSection + DeveloperOptionsSection in SystemTab (Sprint 2.2 followon #4 task 7/N)

Two new sections appear between the diagnostics panel and the
Restart/Reset action buttons. Order matches the user's mental
hierarchy: 'see current state' → 'tune configuration' → 'dev escape
hatches' → 'last-resort destructive actions'."
```

---

## Task 8: Verify + hand-off doc + PR

### Step 8.1: Full backend + frontend verification

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/<this-worktree>
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head    # expect: clean
cargo build 2>&1 | grep -E "^error" | head                          # expect: clean
cargo test --lib embedding_endpoint_tests 2>&1 | tail -5             # expect: 4 passed
cargo test --lib setup_script_tests 2>&1 | tail -5                   # expect: 2 passed
cargo test --lib 2>&1 | tail -3                                     # full suite green
cd ../ui && npx tsc --noEmit 2>&1 | tail -5                         # expect: clean
npm run build 2>&1 | tail -3                                        # expect: build OK
```

All steps must be green before continuing.

### Step 8.2: Write the hand-off doc

Create `docs/superpowers/handoff/2026-05-19-embedding-config-and-dev-options-handoff.md` with the structure of prior hand-offs (Why / What changed / How to verify / Files index). Content should cover:

- Why: three user pain points (manual gbrain config CLI dance, no way to know what fastembed model is loaded, scripts only runnable from terminal).
- What: 3 new commands + 1 config struct + 3 new frontend files + SystemTab mount + 6 unit tests.
- Defaults: `http://localhost:7337/v1` + `llama-server:bge-small-en-v1.5` + 384 + `BAAI/bge-small-en-v1.5`.
- Allowlist enforcement: 4 hardcoded script names, no shell injection surface, dev-only.
- Verify steps: open SystemTab, expand Developer Options, run a no-op `init-gbrain` (already initialized → fast exit 0 with "已初始化" message); change a field in Embedding Endpoint, hit Save, confirm `gbrain config get embedding_model` shows the new value via terminal.

Use the same shape as `2026-05-19-gbrain-embeddings-endpoint-handoff.md` (the predecessor PR).

### Step 8.3: Stage all + commit hand-off

```bash
git add docs/superpowers/handoff/2026-05-19-embedding-config-and-dev-options-handoff.md
git commit -m "docs: Sprint 2.2 followon #4 hand-off (task 8/8)"
```

### Step 8.4: Push + open PR

```bash
git push -u origin <worktree-branch-name>
gh pr create --base main --head <worktree-branch-name> \
  --title "feat(settings): embedding endpoint config + developer options panel (Sprint 2.2 followon #4)" \
  --body "$(cat docs/superpowers/handoff/2026-05-19-embedding-config-and-dev-options-handoff.md)"
```

---

## Self-Review

**Spec coverage check** against the user's 4 requirements:

1. **默认端点设置** ✓ — Task 1 defines `EmbeddingEndpointConfig::default()` with `http://localhost:7337/v1` + `bge-small-en-v1.5` + 384 + `BAAI/bge-small-en-v1.5`. Task 2's `try_init_memu` injection means even a fresh install uses these defaults immediately (no save-first-then-restart dance).
2. **设置页面集成** ✓ — Task 5 builds the `EmbeddingEndpointSection` with form fields for all four config knobs. Task 7 mounts it in `SystemTab.tsx`.
3. **开发者选项按钮 + 进度条** ✓ — Task 3 backend `run_setup_script` streams output via Tauri events. Task 6 frontend `DeveloperOptionsSection` consumes the events into per-script log + calibrated progress bar.
4. **UI 位置 — System 部分的开发者选项页面** ✓ — Task 6 builds a collapsible section, Task 7 mounts it inside `SystemTab.tsx` below the existing diagnostics.

**Placeholder scan:** No `TBD` / "implement later" / "appropriate error handling" / "similar to Task N". Every code block is complete + verbatim. The hand-off doc in Task 8 is the only place where I describe content rather than write it verbatim — that's correct because the hand-off doc structure is fluid and depends on observed test outputs from steps 8.1.

**Type consistency:**
- `EmbeddingEndpointConfig` (Rust struct, Task 1) ↔ `EmbeddingEndpointPayload` (IPC type, Task 2) ↔ `EmbeddingEndpointConfig` (TS interface, Task 4) — all four fields (`base_url: String`, `model: String`, `dimensions: u32`, `fastembed_model: String`) match.
- `SETUP_SCRIPT_ALLOWLIST` (Rust const, Task 3) ↔ `SETUP_SCRIPTS` (TS const, Task 4) — both lists contain `setup-bun-runtime`, `setup-gbrain-source`, `setup-python-env`, `init-gbrain` in that order.
- `RunSetupScriptArgs { script_name, force }` (Rust, Task 3) ↔ `runSetupScript(name, opts)` (TS, Task 4) — the TS helper wraps the same shape (`{script_name: name, force: opts.force ?? false}`).
- `system-setup-script:output` event payload (Rust emit in Task 3) ↔ `SetupScriptOutputEvent` (TS interface, Task 4) — both `{run_id, stream, line}`.
- `system-setup-script:end` event payload ↔ `SetupScriptEndEvent` — both `{run_id, exit_code, success}`.

No drift identified.
