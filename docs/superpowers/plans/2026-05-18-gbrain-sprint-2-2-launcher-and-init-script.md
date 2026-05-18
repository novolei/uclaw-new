# gbrain Sprint 2.2 — launcher script + paths.json + init recovery script

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the bundled gbrain install self-describing (so other Cowork sessions / debug scripts / CLI users can discover the spawn command without grepping uClaw internals) and ship a power-user manual init/reset script for the bundled brain.

**Architecture:** On every Stage 3 boot, before `ensure_bundled_gbrain_initialized` (which already exists from PR #205), write two files to `~/.uclaw/gbrain/`: an executable POSIX `run.sh` launcher that bakes in absolute paths + GBRAIN_HOME, and a machine-readable `paths.json` manifest. Both are overwritten every boot so paths always reflect the current install (dev vs release bundle). Separately, ship `scripts/init-gbrain.sh` mirroring the existing `setup-bun-runtime.sh` style — gives the user a manual `--force` reset path that doesn't require restarting uClaw to trigger init.

**Tech Stack:** Rust (Tauri v2 runtime, `std::fs`, `serde_json`, `chrono`), Bash 4+ (POSIX-ish, matches existing `scripts/setup-*.sh` style).

---

## What's NOT in scope

Per the stale handoff vs reality reconciliation in chat:

- **Sprint 2.2.0 init fix** — already shipped in PR #205 (`ensure_bundled_gbrain_initialized` is the load-bearing function; it has defense-in-depth post-spawn `PG_VERSION` re-check + extracted pure-function probe + last-20-stderr-lines error format).
- **V41 schema migration / `gbrain_migrator.rs` / migration IPC** — out of scope; local `memory_nodes` has 0 EntityPage rows so there's nothing to migrate yet. Sprint 2.2.3+.
- **EntityPage substrate switch (uClaw `create_entity_page` → gbrain)** — out of scope; Sprint 2.3+.
- **Frontend changes** — pure backend + scripts.
- **`setup-bun-runtime.sh` / `setup-gbrain-source.sh` edits** — those are already shipped (PR #203) and tested. We only ADD `init-gbrain.sh`.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/app.rs` | `AppState` (the central DI container). Already houses `try_init_memu`, `find_bun_path`, `find_gbrain_entry` — the gbrain-bundled discovery surface. | Add `pub fn write_gbrain_launcher_files(data_dir, bun_path, entry_path) -> std::io::Result<()>` + private `shell_quote_path` helper. Inline `#[cfg(test)]` tests. |
| `src-tauri/src/main.rs` | Boot orchestration. Stage 3 already wires gbrain init + seed. | Insert `AppState::write_gbrain_launcher_files(...)` call inside the existing `if let (Some(bun), Some(entry))` arm, BEFORE the `ensure_bundled_gbrain_initialized` call. Best-effort: failures log warn but don't abort the gbrain seed. |
| `scripts/init-gbrain.sh` | New CLI tool. Manual init / reset path for the bundled brain. | New file. Mirrors `scripts/setup-bun-runtime.sh` color/flag style (`--force`, `--yes`, `--help`). |
| `docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md` | Hand-off doc for the PR. | New file. |
| `docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt` | PR squash/merge body. | New file. |

3 production-meaningful tasks, 3 commits. (Task 3 packages the plan + hand-off + commit body; user said "2 commit" meaning 2 production commits — Task 3 is the conventional docs commit per repo pattern.)

---

## Task 1: launcher script + paths.json (`src-tauri/src/app.rs` + `src-tauri/src/main.rs`)

**Files:**
- Modify: `src-tauri/src/app.rs` (add new pub fn on `impl AppState` near `try_init_memu`)
- Modify: `src-tauri/src/main.rs:408-410` region (Stage 3 — between the `let gbrain_home = ...` line and the existing `if let (Some(bun), Some(entry))` arm OR inside that arm; see step 1.5 for exact placement)
- Test: inline `#[cfg(test)]` block in `src-tauri/src/app.rs`

### Step 1.1: Locate the insertion point in `app.rs`

```bash
cd /Users/ryanliu/Documents/uclaw
grep -n "fn find_bun_path\|fn find_gbrain_entry\|fn try_init_memu" src-tauri/src/app.rs
```

You should see 3 fns. We'll add `write_gbrain_launcher_files` as a sibling pub fn — placement right AFTER `find_gbrain_entry` (which is the closest semantic neighbor: also concerned with bundled gbrain paths).

### Step 1.2: Write the failing test

In `src-tauri/src/app.rs`, find the existing `#[cfg(test)] mod tests` block (`grep -n "^mod tests\|^#\[cfg(test)\]" src-tauri/src/app.rs`). If a `mod tests` exists, add a new `mod gbrain_launcher_tests` sibling block at the END of the file. If no test mod exists at all, create one at file-end.

```rust
#[cfg(test)]
mod gbrain_launcher_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn write_gbrain_launcher_files_creates_run_sh_and_paths_json() {
        let data = tempdir().unwrap();
        let bun = data.path().join("fake-bun");
        let entry = data.path().join("fake-cli.ts");
        // Write placeholder files so the canonicalized paths exist as files.
        fs::write(&bun, "").unwrap();
        fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(data.path(), &bun, &entry)
            .expect("launcher write should succeed");

        let gbrain_home = data.path().join("gbrain");
        let run_sh = gbrain_home.join("run.sh");
        let paths_json = gbrain_home.join("paths.json");
        assert!(run_sh.is_file(), "run.sh should exist");
        assert!(paths_json.is_file(), "paths.json should exist");

        // run.sh content: shebang + GBRAIN_HOME export + exec line.
        let script = fs::read_to_string(&run_sh).unwrap();
        assert!(script.starts_with("#!/usr/bin/env bash\n"), "shebang missing");
        assert!(
            script.contains(&format!("export GBRAIN_HOME='{}'", gbrain_home.display())),
            "GBRAIN_HOME export missing or wrong path"
        );
        assert!(
            script.contains("exec '"),
            "exec line missing"
        );
        // The exec line must reference bun and entry by absolute path.
        assert!(script.contains(&bun.display().to_string()), "bun path missing");
        assert!(script.contains(&entry.display().to_string()), "entry path missing");

        // paths.json: serde_json parseable, contains the expected keys.
        let manifest_raw = fs::read_to_string(&paths_json).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
            .expect("paths.json should be valid JSON");
        assert_eq!(manifest["uclaw_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["bun_path"], bun.to_string_lossy().as_ref());
        assert_eq!(manifest["gbrain_entry"], entry.to_string_lossy().as_ref());
        assert_eq!(manifest["gbrain_home"], gbrain_home.to_string_lossy().as_ref());
        // brain_dir is the canonical PGLite layout from PR #205.
        let brain_dir = gbrain_home.join(".gbrain").join("brain.pglite");
        assert_eq!(manifest["brain_dir"], brain_dir.to_string_lossy().as_ref());
        let config_json_path = gbrain_home.join(".gbrain").join("config.json");
        assert_eq!(manifest["config_json"], config_json_path.to_string_lossy().as_ref());
        // generated_at_ms is a number (we don't pin a specific value).
        assert!(manifest["generated_at_ms"].is_i64(), "generated_at_ms should be i64");
    }

    #[cfg(unix)]
    #[test]
    fn write_gbrain_launcher_files_marks_run_sh_executable() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempdir().unwrap();
        let bun = data.path().join("fake-bun");
        let entry = data.path().join("fake-cli.ts");
        std::fs::write(&bun, "").unwrap();
        std::fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(data.path(), &bun, &entry).unwrap();
        let run_sh = data.path().join("gbrain").join("run.sh");
        let mode = std::fs::metadata(&run_sh).unwrap().permissions().mode();
        // mode includes file-type bits; mask to permission bits (lowest 9).
        assert_eq!(mode & 0o777, 0o755, "run.sh should be chmod 0o755");
    }

    #[test]
    fn write_gbrain_launcher_files_handles_paths_with_spaces() {
        let data = tempdir().unwrap();
        // Create a sub-path with a space — exercises shell_quote_path
        let nested = data.path().join("with space");
        std::fs::create_dir_all(&nested).unwrap();
        let bun = nested.join("bun");
        let entry = nested.join("cli.ts");
        std::fs::write(&bun, "").unwrap();
        std::fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(data.path(), &bun, &entry).unwrap();

        let run_sh_content = std::fs::read_to_string(
            data.path().join("gbrain").join("run.sh")
        ).unwrap();
        // Path with space must be single-quoted so the shell treats it as one arg.
        assert!(
            run_sh_content.contains(&format!("'{}'", bun.display())),
            "path with space should be single-quoted, got:\n{}",
            run_sh_content
        );
    }
}
```

Verify `tempfile` is in `[dev-dependencies]` (PR #205 confirmed it's already on the macOS target). If for any reason the test fails to find it, add `tempfile = "3"` under `[dev-dependencies]`.

### Step 1.3: Run the test to verify it fails

```bash
cd src-tauri && cargo test --lib gbrain_launcher_tests 2>&1 | tail -20
```

Expected: compile error `cannot find function 'write_gbrain_launcher_files' in this scope` (or method-not-found, depending on call shape). If you instead see "no test cases matched", the test module didn't compile — read the full output and fix.

### Step 1.4: Implement `write_gbrain_launcher_files` + `shell_quote_path`

Insert this code in `src-tauri/src/app.rs` IMMEDIATELY AFTER the existing `find_gbrain_entry` function (which currently sits at lines ~900-950 per CLAUDE.md's recent grep; find exact location with `grep -n "fn find_gbrain_entry" src-tauri/src/app.rs`):

```rust
    /// Sprint 2.2 — write self-describing launcher script + paths manifest
    /// to `<data_dir>/gbrain/`. Lets any tool (other Cowork sessions, debug
    /// scripts, CLI users) invoke the bundled gbrain without knowing
    /// whether uClaw is dev-mode or installed as a release `.app`.
    ///
    /// Overwrites on every boot so paths always reflect the current install.
    /// Best-effort caller: returns `io::Result` so the boot path can log a
    /// warning on failure (e.g. read-only data_dir) without aborting the
    /// gbrain seed.
    ///
    /// Files written:
    /// - `<gbrain_home>/run.sh` — POSIX launcher (chmod 0o755 on Unix).
    ///   Sets `GBRAIN_HOME=<gbrain_home>` and execs `<bun> <entry> "$@"`.
    /// - `<gbrain_home>/paths.json` — machine-readable manifest with
    ///   uclaw_version, absolute paths, and a generation timestamp.
    pub fn write_gbrain_launcher_files(
        data_dir: &std::path::Path,
        bun_path: &std::path::Path,
        entry_path: &std::path::Path,
    ) -> std::io::Result<()> {
        let gbrain_home = data_dir.join("gbrain");
        std::fs::create_dir_all(&gbrain_home)?;

        // run.sh — POSIX launcher. Bakes in absolute paths and sets
        // GBRAIN_HOME so the gbrain CLI resolves its layout
        // (`.gbrain/brain.pglite/`, `.gbrain/config.json`) under our
        // chosen home rather than the user's default `~/.gbrain/`.
        let run_sh = gbrain_home.join("run.sh");
        let script = format!(
            "#!/usr/bin/env bash\n\
             # Auto-generated by uClaw at every boot — do not hand-edit.\n\
             # Usage:\n\
             #   ~/.uclaw/gbrain/run.sh init --pglite --yes\n\
             #   ~/.uclaw/gbrain/run.sh serve\n\
             #   ~/.uclaw/gbrain/run.sh recall \"some query\"\n\
             export GBRAIN_HOME={home_q}\n\
             exec {bun_q} {entry_q} \"$@\"\n",
            home_q = shell_quote_path(&gbrain_home),
            bun_q = shell_quote_path(bun_path),
            entry_q = shell_quote_path(entry_path),
        );
        std::fs::write(&run_sh, script)?;

        // chmod +x — best-effort; if it fails the user can chmod manually.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &run_sh,
                std::fs::Permissions::from_mode(0o755),
            );
        }

        // paths.json — machine-readable manifest. The brain layout
        // (`.gbrain/brain.pglite/` + `.gbrain/config.json`) is what
        // `gbrain init --pglite` actually produces, NOT the dead
        // `pgdata/` vestige from pre-PR-205 days.
        let brain_dir = gbrain_home.join(".gbrain").join("brain.pglite");
        let config_json = gbrain_home.join(".gbrain").join("config.json");
        let manifest = serde_json::json!({
            "uclaw_version": env!("CARGO_PKG_VERSION"),
            "bun_path": bun_path,
            "gbrain_entry": entry_path,
            "gbrain_home": gbrain_home,
            "brain_dir": brain_dir,
            "config_json": config_json,
            "generated_at_ms": chrono::Utc::now().timestamp_millis(),
        });
        let manifest_str = serde_json::to_string_pretty(&manifest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(gbrain_home.join("paths.json"), manifest_str)?;
        Ok(())
    }
```

Then add the `shell_quote_path` free function in `src-tauri/src/app.rs` (place at module scope, after the `impl AppState` block where `write_gbrain_launcher_files` lives; if there's a natural location near other helpers, use that — otherwise just before the `#[cfg(test)]` block):

```rust
/// Minimal POSIX shell quoter for filesystem paths. Wraps in single
/// quotes and escapes any embedded single quotes via the `'\''` idiom.
/// Sufficient for paths (which can contain spaces, dashes, etc.) but
/// NOT general shell metacharacters — paths don't legitimately contain
/// the shell special chars that single-quoting can't handle.
fn shell_quote_path(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    if s.is_empty() {
        return "''".to_string();
    }
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}
```

### Step 1.5: Wire the call into `main.rs` Stage 3

```bash
grep -n "let gbrain_home\|ensure_bundled_gbrain_initialized" src-tauri/src/main.rs
```

Find the existing block (PR #205 wired this; `let gbrain_home = state_ref.data_dir.join("gbrain");` should be around line 408, and the `if let (Some(bun), Some(entry))` arm starts around line 482). Add the launcher-files call INSIDE that arm, BEFORE the `ensure_bundled_gbrain_initialized` call:

Before:
```rust
                            if let (Some(bun), Some(entry)) =
                                (bun_path.as_ref(), gbrain_entry.as_ref())
                            {
                                match uclaw_core::mcp::ensure_bundled_gbrain_initialized(
                                    bun, entry, &gbrain_home,
                                ) {
```

After:
```rust
                            if let (Some(bun), Some(entry)) =
                                (bun_path.as_ref(), gbrain_entry.as_ref())
                            {
                                // Sprint 2.2 — write self-describing launcher
                                // files BEFORE init. Best-effort: a write
                                // failure shouldn't abort the gbrain seed.
                                if let Err(e) = AppState::write_gbrain_launcher_files(
                                    &state_ref.data_dir, bun, entry,
                                ) {
                                    tracing::warn!(
                                        error = %e,
                                        "[Stage 3] failed to write ~/.uclaw/gbrain/{{run.sh,paths.json}}"
                                    );
                                }
                                match uclaw_core::mcp::ensure_bundled_gbrain_initialized(
                                    bun, entry, &gbrain_home,
                                ) {
```

Confirm `AppState` is in scope (it's already imported because the surrounding code uses `state_ref` which is `Arc<AppState>` and `AppState::find_bun_path` etc.). The call uses `AppState::write_gbrain_launcher_files(...)` as an associated function — no `self` needed.

### Step 1.6: Compile + test

```bash
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head -20
cargo build 2>&1 | grep -E "^error" | head -20
```

Both expected: zero errors.

```bash
cargo test --lib gbrain_launcher_tests 2>&1 | tail -10
cargo test --lib 2>&1 | tail -5
```

Expected: 3 new tests pass; total lib tests pass (no regression).

### Step 1.7: Commit

```bash
cd /Users/ryanliu/Documents/uclaw   # or the worktree path
git status -sb
# Should show: M src-tauri/src/app.rs  M src-tauri/src/main.rs
git add src-tauri/src/app.rs src-tauri/src/main.rs
git commit -m "feat(gbrain): self-describing launcher script + paths.json (Sprint 2.2 task 1/3)

Every Stage 3 boot now writes two files to ~/.uclaw/gbrain/:

- run.sh: POSIX launcher (chmod 0o755). Bakes in absolute bun + gbrain
  entry paths and exports GBRAIN_HOME=<data_dir>/gbrain so the gbrain
  CLI resolves its layout (.gbrain/brain.pglite/, .gbrain/config.json)
  under our chosen home rather than the user's default ~/.gbrain/.
  Usage: '~/.uclaw/gbrain/run.sh serve|init|recall \"q\"|…'.

- paths.json: machine-readable manifest with uclaw_version, absolute
  bun_path / gbrain_entry / gbrain_home / brain_dir / config_json,
  and a generated_at_ms timestamp. brain_dir + config_json reference
  the real PR #205 layout, not the pre-PR-205 dead pgdata/ vestige.

Called from main.rs Stage 3 inside the existing (Some(bun), Some(entry))
arm, BEFORE ensure_bundled_gbrain_initialized — so the files are present
before the first connect attempt. Best-effort: write failure logs a
warning but doesn't abort the gbrain seed.

3 unit tests cover content, chmod 0o755, and paths-with-spaces quoting."
```

---

## Task 2: `scripts/init-gbrain.sh` recovery script

**Files:**
- Create: `scripts/init-gbrain.sh` (new)

Mirror the style of `scripts/setup-bun-runtime.sh` (which is the closest sibling — same project, same color/flag/`step`/`success`/`warn`/`error` macro pattern).

### Step 2.1: Create the script with exact content

Create `scripts/init-gbrain.sh` with this content verbatim:

```bash
#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# init-gbrain.sh
# 对 ~/.uclaw/gbrain/ 跑 `gbrain init --pglite --yes`，初始化 PGLite brain。
#
# Sprint 2.2 of the gbrain integration track. 三种典型用途：
#   1. Power-user 手动初始化（不想等 uClaw 自动跑）
#   2. 验证 fresh-init 是否能跑通（开发/CI）
#   3. --force reset 已存在的 brain，从零开始
#
# 前提：scripts/setup-bun-runtime.sh + scripts/setup-gbrain-source.sh
# 已经跑过（src-tauri/bunembed/bun + src-tauri/gbrain-source/ 都在）。
#
# 正常 boot 路径在 uClaw 启动时已经自动跑这个（PR #205），所以一般
# 用户不需要手动调用。
# =============================================================================

# --- 颜色定义 ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
step()    { echo -e "\n${CYAN}${BOLD}▶ $*${NC}"; }

# --- 路径 ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUN_BIN="${PROJECT_DIR}/src-tauri/bunembed/bun"
GBRAIN_ENTRY="${PROJECT_DIR}/src-tauri/gbrain-source/src/cli.ts"
GBRAIN_HOME="${HOME}/.uclaw/gbrain"
BRAIN_DIR="${GBRAIN_HOME}/.gbrain/brain.pglite"
PG_VERSION="${BRAIN_DIR}/PG_VERSION"

# --- 参数解析 ---
OPT_HELP=false
OPT_FORCE=false
OPT_YES=false

usage() {
    local cmd
    cmd=$(basename "$0")
    cat <<EOF
${BOLD}用法:${NC} ${cmd} [选项]

初始化 ~/.uclaw/gbrain/ 下的 PGLite brain（运行 gbrain init --pglite --yes）。

${BOLD}选项:${NC}
  --help        显示此帮助
  --force       即使 brain 已初始化也重新初始化（删除 .gbrain/brain.pglite 后重建）
  --yes / -y    所有交互确认默认 yes（CI 用）

${BOLD}前提:${NC}
  ${PROJECT_DIR}/src-tauri/bunembed/bun    (运行 scripts/setup-bun-runtime.sh)
  ${PROJECT_DIR}/src-tauri/gbrain-source/  (运行 scripts/setup-gbrain-source.sh)

${BOLD}产出:${NC}
  ${BRAIN_DIR}/PG_VERSION  (PGLite 数据目录，63 migrations 跑完)
  ${GBRAIN_HOME}/.gbrain/config.json  (gbrain 自己写)

${BOLD}注:${NC} 正常情况下 uClaw 启动时已经自动跑过这个。手动调用一般是 power-user
场景（重置、调试、CI 验证）。
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)   OPT_HELP=true ;;
        --force)  OPT_FORCE=true ;;
        --yes|-y) OPT_YES=true ;;
        *) error "未知选项: $1"; usage; exit 1 ;;
    esac
    shift
done

if $OPT_HELP; then usage; exit 0; fi

confirm() {
    if $OPT_YES; then return 0; fi
    local prompt="$1 [y/N] "
    read -r -p "$(echo -e "${YELLOW}${prompt}${NC}")" answer
    case "$answer" in
        [yY][eE][sS]|[yY]) return 0 ;;
        *) return 1 ;;
    esac
}

step "Pre-flight"
if [[ ! -x "${BUN_BIN}" ]]; then
    error "找不到 bun: ${BUN_BIN}"
    error "请先跑: ${PROJECT_DIR}/scripts/setup-bun-runtime.sh"
    exit 1
fi
if [[ ! -f "${GBRAIN_ENTRY}" ]]; then
    error "找不到 gbrain CLI entry: ${GBRAIN_ENTRY}"
    error "请先跑: ${PROJECT_DIR}/scripts/setup-gbrain-source.sh"
    exit 1
fi
success "bun + gbrain entry 都在"
info "GBRAIN_HOME = ${GBRAIN_HOME}"

step "检查已有 brain"
if [[ -f "${PG_VERSION}" ]]; then
    local_version="$(cat "${PG_VERSION}")"
    if ! $OPT_FORCE; then
        warn "brain 已初始化 (PG_VERSION=${local_version})，跳过 init"
        info "如要重置: ${0} --force"
        exit 0
    fi
    warn "--force 启用，准备重置已有 brain"
    if ! confirm "确定删除 ${BRAIN_DIR} 重新初始化吗？"; then
        warn "取消"
        exit 0
    fi
    info "删除 ${BRAIN_DIR}"
    rm -rf "${BRAIN_DIR}"
    success "已删除"
else
    info "brain 未初始化，准备 fresh init"
fi

step "确保 GBRAIN_HOME 存在"
mkdir -p "${GBRAIN_HOME}"
success "GBRAIN_HOME ready: ${GBRAIN_HOME}"

step "运行 gbrain init --pglite --yes"
info "首次 init 会跑 ~63 PGLite migrations，约 30-60s..."
GBRAIN_HOME="${GBRAIN_HOME}" "${BUN_BIN}" "${GBRAIN_ENTRY}" init --pglite --yes
success "gbrain init 退出码 0"

step "验证 PG_VERSION 已生成"
if [[ ! -f "${PG_VERSION}" ]]; then
    error "init 退出 0 但 ${PG_VERSION} 不存在"
    error "可能 gbrain 写到了别处。检查: ls -la ${GBRAIN_HOME}/.gbrain/"
    exit 1
fi
final_version="$(cat "${PG_VERSION}")"
success "PG_VERSION = ${final_version}"

step "完成"
success "brain 已初始化在 ${BRAIN_DIR}"
info "重启 uClaw 即可正常使用 gbrain MCP（连接应在 ~2-5s 内完成）"
```

### Step 2.2: Make it executable + syntax check

```bash
cd /Users/ryanliu/Documents/uclaw   # or the worktree path
chmod +x scripts/init-gbrain.sh
bash -n scripts/init-gbrain.sh
```

`bash -n` should exit 0 with no output (syntax-OK).

### Step 2.3: Smoke-test `--help` (doesn't actually init anything)

```bash
./scripts/init-gbrain.sh --help
```

Expected: prints the usage block. Exit 0.

```bash
echo "exit=$?"
```

Expected: `exit=0`.

### Step 2.4: Commit

```bash
git status -sb
# Should show: ?? scripts/init-gbrain.sh
git add scripts/init-gbrain.sh
git commit -m "feat(scripts): init-gbrain.sh manual init/reset script (Sprint 2.2 task 2/3)

Power-user CLI for initializing or resetting the bundled gbrain brain
at ~/.uclaw/gbrain/. PR #205 already auto-inits on Stage 3 boot, so
this script is for three secondary use cases:

  1. Manual init outside of uClaw (don't want to start the app)
  2. Verify fresh-init works (dev / CI)
  3. --force reset an existing brain from scratch

Targets the canonical PR #205 layout: PG_VERSION marker at
\`\${GBRAIN_HOME}/.gbrain/brain.pglite/PG_VERSION\`. Mirrors
scripts/setup-bun-runtime.sh style: same color macros, --help / --force
/ --yes flag triplet, pre-flight check that points at the setup scripts
if bun / gbrain-source are absent.

Bash -n syntax-clean. Help text renders correctly on macOS bash 3.2."
```

---

## Task 3: hand-off doc + commit body + plan file

**Files:**
- Create: `docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md`
- Create: `docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt`
- Stage: `docs/superpowers/plans/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script.md` (this plan file — it's uncommitted)

### Step 3.1: Write the hand-off doc

Create `docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md`:

```markdown
# gbrain Sprint 2.2 — launcher script + paths.json + init recovery script

**Status:** ready to merge (assuming manual verify is green on Mac).
**Branch:** (set at finishing-branch time)
**Base:** `main`
**Predecessor:** PR #205 (Sprint 2.1 init-fix — established `ensure_bundled_gbrain_initialized` + the real `.gbrain/brain.pglite/` layout).

## Why this PR exists

PR #205 closed the original "gbrain serve exits with No brain configured"
bug by adding `ensure_bundled_gbrain_initialized` to Stage 3 boot. But two
follow-on gaps remained:

1. **Discoverability.** Other Cowork sessions, debug scripts, and CLI
   users had no way to invoke the bundled gbrain without grepping uClaw
   internals to figure out where `bun` and `gbrain/src/cli.ts` live (paths
   differ between dev mode and the release `.app` bundle).

2. **Manual init / reset path.** PR #205 made auto-init bulletproof, but
   power-user workflows (reset a brain, verify fresh-init in CI, init
   without launching uClaw) had no first-class entry point.

This PR ships both:

- Every boot writes `~/.uclaw/gbrain/run.sh` + `~/.uclaw/gbrain/paths.json`.
- `scripts/init-gbrain.sh` mirrors the existing `setup-bun-runtime.sh`
  style for manual init / `--force` reset.

## What changed

| File | Diff |
|---|---|
| `src-tauri/src/app.rs` | +`pub fn write_gbrain_launcher_files` + private `shell_quote_path` helper + 3 unit tests |
| `src-tauri/src/main.rs` | +5 lines (call site inside the existing Stage 3 `if let (Some(bun), Some(entry))` arm, BEFORE `ensure_bundled_gbrain_initialized`) |
| `scripts/init-gbrain.sh` | new file, executable |

## How the launcher works

Boot order (Stage 3, inside the `Some(bun), Some(entry)` arm):

1. `AppState::write_gbrain_launcher_files(data_dir, bun, entry)` — best-effort.
2. `ensure_bundled_gbrain_initialized(bun, entry, &gbrain_home)` (from PR #205).
3. `seed_bundled_gbrain(bun, entry, &gbrain_home)` (from PR #205).

`run.sh`:
- Shebang: `#!/usr/bin/env bash`
- Exports `GBRAIN_HOME=<gbrain_home>` (NOT `PGLITE_DATA_DIR` — gbrain reads its
  layout from `$GBRAIN_HOME/.gbrain/config.json`, written by `gbrain init`).
- `exec <bun> <gbrain_cli.ts> "$@"` — forwards all args.
- Single-quoted paths so spaces work.
- chmod 0o755 best-effort on Unix.

`paths.json`:
```json
{
  "uclaw_version": "0.1.0",
  "bun_path": "<abs path to bunembed/bun>",
  "gbrain_entry": "<abs path to gbrain-source/src/cli.ts>",
  "gbrain_home": "<HOME>/.uclaw/gbrain",
  "brain_dir": "<HOME>/.uclaw/gbrain/.gbrain/brain.pglite",
  "config_json": "<HOME>/.uclaw/gbrain/.gbrain/config.json",
  "generated_at_ms": 1747584000000
}
```

`brain_dir` + `config_json` reference the **real** PGLite layout `gbrain
init --pglite` produces — NOT the dead `pgdata/` directory that the
pre-PR-205 code tried to use.

## How `init-gbrain.sh` works

Three flags: `--force`, `--yes`, `--help`. Style mirrors `setup-bun-runtime.sh`.

Pre-flight checks `src-tauri/bunembed/bun` + `src-tauri/gbrain-source/src/cli.ts`
exist; on failure points the user at `scripts/setup-bun-runtime.sh` +
`scripts/setup-gbrain-source.sh`.

If `${BRAIN_DIR}/PG_VERSION` already exists without `--force` → exit 0
with a "already initialized" note. With `--force` → confirm (unless `--yes`),
`rm -rf brain.pglite`, then re-init.

Otherwise: `GBRAIN_HOME=~/.uclaw/gbrain bunembed/bun gbrain-source/src/cli.ts
init --pglite --yes`. Post-init verify checks the PG_VERSION marker landed;
if not, error out (catches the "init exited 0 but wrote elsewhere" bug class).

## How to verify locally

```bash
# Build
cd src-tauri && cargo build --lib && cargo test --lib gbrain_launcher_tests

# Launcher files should be written next boot (any restart of the debug app)
# Manually trigger:
cd src-tauri && cargo tauri dev > /tmp/uclaw-dev.log 2>&1 &
# Wait for [Stage 3] in logs, then:
ls -la ~/.uclaw/gbrain/run.sh ~/.uclaw/gbrain/paths.json
cat ~/.uclaw/gbrain/paths.json
~/.uclaw/gbrain/run.sh --help   # should print gbrain's own help

# init-gbrain.sh:
./scripts/init-gbrain.sh --help   # prints usage, exits 0
# Sandbox / safe-test: --force on a real brain (will confirm before destroying):
./scripts/init-gbrain.sh --force   # prompts; type 'n' to abort cleanly
```

## Commits (bisectable)

| # | sha | purpose |
|---|-----|---------|
| 1 | <task 1 sha> | launcher files (app.rs + main.rs + 3 tests) |
| 2 | <task 2 sha> | scripts/init-gbrain.sh |
| 3 | <this commit> | hand-off doc + commit body + plan |

## Files index

```
docs/superpowers/plans/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script.md (this PR's plan)
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md ← this
docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt (PR squash/merge body)
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-1-init-fix-handoff.md (PR #205 hand-off — predecessor)
```
```

### Step 3.2: Write the commit body

Create `docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt`:

```
feat(gbrain): launcher script + paths.json + init recovery script (Sprint 2.2)

## Why

PR #205 closed the "gbrain serve exits with No brain configured" bug.
Two follow-on gaps remained:

1. Discoverability — other tools couldn't find bun + gbrain entry
   without grepping uClaw internals.
2. Manual init / reset — power-user workflows had no first-class entry.

## What

- `src-tauri/src/app.rs`: new `AppState::write_gbrain_launcher_files`
  writes `~/.uclaw/gbrain/{run.sh,paths.json}` every boot. run.sh is
  a POSIX launcher (chmod 0o755) that exports GBRAIN_HOME + execs
  bun/gbrain with all args forwarded. paths.json is a machine-readable
  manifest referencing the real PR #205 .gbrain/brain.pglite layout.
- `src-tauri/src/main.rs`: +5 lines wiring the call into the existing
  Stage 3 `(Some(bun), Some(entry))` arm, BEFORE
  ensure_bundled_gbrain_initialized. Best-effort: write failure logs
  warn but doesn't abort the gbrain seed.
- `scripts/init-gbrain.sh`: new manual init / reset CLI. Mirrors
  setup-bun-runtime.sh style. --force / --yes / --help. Pre-flight
  checks point at setup-bun-runtime.sh + setup-gbrain-source.sh on
  missing deps. Defense-in-depth post-init PG_VERSION verify.
- 3 new unit tests on the launcher (content, chmod 0o755, paths with
  spaces).

## Commits (bisectable)

| # | sha     | task     | shape |
|---|---------|----------|-------|
| 1 | <hash>  | task 1/3 | launcher files (app.rs + main.rs + 3 tests) |
| 2 | <hash>  | task 2/3 | scripts/init-gbrain.sh |
| 3 | <hash>  | task 3/3 | hand-off doc + this commit body + plan |

## Verify

cd src-tauri && cargo build --lib && cargo test --lib gbrain_launcher_tests

After next boot (or `cargo tauri dev`):
  ls -la ~/.uclaw/gbrain/run.sh ~/.uclaw/gbrain/paths.json
  cat ~/.uclaw/gbrain/paths.json
  ~/.uclaw/gbrain/run.sh --help

Manual init / reset script:
  ./scripts/init-gbrain.sh --help

## Out of scope (per plan)

- Sprint 2.2.0 init logic (already in PR #205 — superseded the original
  stale Sprint 2.2 handoff).
- V41 migration / gbrain_migrator.rs (no EntityPage data to migrate yet).
- EntityPage substrate switch (Sprint 2.3).

## Files index

docs/superpowers/plans/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script.md
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md
docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt   ← this
```

### Step 3.3: Stage + commit

```bash
cd /Users/ryanliu/Documents/uclaw   # or worktree
git status -sb
# Should show 3 untracked files (plan + handoff + commit body) — plan might
# already be tracked if subagent-driven-development picked it up; if so,
# just add the two new files.

git add docs/superpowers/plans/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script.md \
        docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md \
        docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt
git commit -m "docs(gbrain): Sprint 2.2 hand-off + commit body (task 3/3)

Plan, hand-off doc, and PR squash/merge body for the launcher script
+ paths.json + init recovery script work that builds on PR #205's
ensure_bundled_gbrain_initialized."
```

### Step 3.4: Sanity check

```bash
git log --oneline -5
```

Expected: 3 new commits since `4de805e` (PR #205 merge into main).

```bash
cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head
cargo test --lib 2>&1 | tail -5
```

Both green.

---

## Self-Review

**1. Spec coverage:**
- ✅ `write_gbrain_launcher_files` writes both `run.sh` (with chmod) AND `paths.json` (Task 1 Step 1.4).
- ✅ `paths.json` references real `.gbrain/brain.pglite/` layout, NOT dead `pgdata/` (Task 1 Step 1.4, asserted in test).
- ✅ Wired into Stage 3 BEFORE `ensure_bundled_gbrain_initialized` (Task 1 Step 1.5).
- ✅ Best-effort failure handling (warn log, no abort) (Task 1 Step 1.5).
- ✅ `shell_quote_path` handles spaces (Task 1 test 3).
- ✅ `init-gbrain.sh` --force / --yes / --help flags (Task 2 Step 2.1).
- ✅ Pre-flight checks point at setup scripts (Task 2 Step 2.1).
- ✅ Post-init PG_VERSION verify (Task 2 Step 2.1).
- ✅ Hand-off doc + commit body (Task 3).

**2. Placeholder scan:** No `TBD` / `implement later` / "appropriate error handling" / vague references. Every code step has the actual code (Rust + Bash, both verbatim). Tests are written out in full (3 cases for the launcher). Commit messages and shell content are verbatim.

**3. Type consistency:** `write_gbrain_launcher_files(&Path, &Path, &Path) -> std::io::Result<()>` matches in: implementation (Step 1.4), test calls (Step 1.2), and call site (Step 1.5). `shell_quote_path(&Path) -> String` is a free fn used only by `write_gbrain_launcher_files`. The init script's `BRAIN_DIR` constant aligns with the Rust code's `gbrain_home.join(".gbrain").join("brain.pglite")` derivation.
