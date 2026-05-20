# gbrain transport → 真 MCP serve(StdioTransport)修复 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把捆绑 gbrain 的 MCP transport 从 `GbrainCliTransport`(CLI 垫片,吐人类文本、只 6 op)切到 `StdioTransport`(真 `gbrain serve`,返回 MCP JSON),让已合并的子项目 A(gbrain 浏览器)+ C(双星云知识层)真正可用;删除被取代的 CLI 垫片;加一条可复跑的 smoke 命令。

**Architecture:** `connect_server_shared`(mcp.rs:2531 的 io 块)对 bundled gbrain 改走 `StdioTransport::spawn`(args 含 `serve`),spawn 前调用提取出的自由函数 `cleanup_stale_pglite_lock` 清崩溃残留锁。reconnect 复用 connect 路径,故一处覆盖。删 `GbrainCliTransport` 及仅它引用的 helper。不改 A/C 的 parser/前端、不改 gbrain-source。

**Tech Stack:** Rust（tokio process、serde_json、rusqlite-free）、现有 uClaw MCP infra（StdioTransport / PR-3 reconnect / kill_on_drop）。

---

## 已核对的事实

- 决策点 `mcp.rs:2532-2557`:`TransportType::Stdio` 下 `if GbrainCliTransport::is_bundled_gbrain(&config) { GbrainCliTransport::new(...) } else { StdioTransport::spawn(&config.name,&config.command,&config.args,&config.env,id,notification_tx.clone()).await? }`。**else 分支已是我们要的**;改动 = 让 bundled 分支也走 `StdioTransport::spawn`(args 原样含 serve)+ 先清锁。
- `config.args` for bundled gbrain = `[".../gbrain-source/src/cli.ts", "serve"]`(mcp.rs:588)。GbrainCliTransport 故意剥 `serve`;StdioTransport 用原样 args → spawn `bun .../cli.ts serve`(真 MCP stdio server)。
- `StdioTransport::spawn`(641+)已有 `kill_on_drop(true)` + stdout reader + initialize 握手。
- `reconnect_server_shared`(2666)→ 调 `connect_server_shared`;`restart_server_shared` 同理。**spawn 只在 connect_server_shared 一处** → cleanup 放这里即覆盖初始连接 + 重连。
- `ensure_bundled_gbrain_initialized`(main.rs:520,boot)已在 connect 前跑 → brain 先 init。不动。
- `is_bundled_gbrain`(1051,关联函数)被调 2 处:`1963`(config 自愈,与 transport 无关)+ `2534`。提取为自由函数后更新这 2 处。
- `cleanup_stale_pglite_lock`(1304,GbrainCliTransport 方法,用 `self.env.get("GBRAIN_HOME")` + `pid_is_alive`)当前仅在 `call_cli`(1164)内调。`pid_is_alive` 是自由函数(保留)。
- CLI-only helper 候选(删除前须逐个 grep 确认引用全在被删代码内):`call_cli`、`suggest_page_slugs`、`gbrain_cli_error_payload`、`classify_gbrain_cli_failure`、`push_number_flag`、`push_string_flag`、`push_bool_flag`、`required_string`、`optional_string`。引用计数(含定义):7/14/5/6/4/4/2 等 —— 多在 call_cli 内,但**实现时必须 grep 每个、只删引用全在 GbrainCliTransport 内者**。
- A/C 的 parser:`gbrain::browse::{parse_list_pages,parse_page_detail,parse_search,parse_backlinks,parse_versions,parse_stats,parse_orphans,parse_links}` + `list_pages/get_stats` 等 async fn。**不改**。
- smoke 复用形态:`tauri_commands.rs:500 build_memory_gbrain_eval_harness_report` / `522 run_memory_gbrain_eval_harness`(`State` → `state.mcp_manager`)。

**验证命令:**
- 编译:`cd src-tauri && cargo build > /tmp/g.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/g.txt | head`
- 单测:`cd src-tauri && cargo test --lib mcp:: > /tmp/gt.txt 2>&1; grep "test result" /tmp/gt.txt`(含新 cleanup 测试);`cargo test --lib gbrain::browse`(A/C parser 不回归)
- **IRON RULE**:重定向到文件再 grep,绝不 `| tail` 取退出码。
- 3D/真 serve 无法单测 → 手动 E2E 清单(见末尾)。

---

## Task 1: 提取 `cleanup_stale_pglite_lock` + `is_bundled_gbrain` 为自由函数(行为不变)

**Files:** Modify `src-tauri/src/mcp.rs`.

- [ ] **Step 1: 加自由函数 `is_bundled_gbrain`**（放在 `GbrainCliTransport` impl 块之外、模块级；body 照搬原 `GbrainCliTransport::is_bundled_gbrain`）
```rust
fn is_bundled_gbrain(config: &McpServerConfig) -> bool {
    config.id == "gbrain"
        && config.transport_type == TransportType::Stdio
        && config.args.last().map(|s| s.as_str()) == Some("serve")
        && config.args.iter().any(|arg| {
            arg.ends_with("gbrain/src/cli.ts") || arg.ends_with("gbrain-source/src/cli.ts")
        })
}
```

- [ ] **Step 2: 加自由函数 `cleanup_stale_pglite_lock`**（模块级；从原 `GbrainCliTransport::cleanup_stale_pglite_lock` 搬出，把 `self.env` 改成入参 `env`）
```rust
/// 清掉 gbrain PGLite 的崩溃残留单写锁(锁文件里的 PID 已不存活时删除)。
/// 在 spawn 持久 `gbrain serve` 前调用,避免上次 serve 崩溃留下的锁让新 serve
/// 卡在 "Timed out waiting for PGLite lock"。
fn cleanup_stale_pglite_lock(env: &HashMap<String, String>) {
    let Some(home) = env.get("GBRAIN_HOME") else { return; };
    let lock_dir = std::path::Path::new(home).join(".gbrain").join("brain.pglite").join(".gbrain-lock");
    let lock_file = lock_dir.join("lock");
    let Ok(raw) = std::fs::read_to_string(&lock_file) else { return; };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else { return; };
    let Some(pid) = v.get("pid").and_then(|p| p.as_i64()) else { return; };
    if !pid_is_alive(pid) {
        if let Err(e) = std::fs::remove_dir_all(&lock_dir) {
            tracing::warn!(lock = %lock_dir.display(), error = %e, "Failed to remove stale gbrain PGLite lock");
        } else {
            tracing::warn!(pid, lock = %lock_dir.display(), "Removed stale gbrain PGLite lock for dead process");
        }
    }
}
```

- [ ] **Step 3: 把 GbrainCliTransport 内的旧方法改为委托(临时,Task 3 删整个 struct)**
  - `GbrainCliTransport::call_cli` 里 `self.cleanup_stale_pglite_lock();` → `cleanup_stale_pglite_lock(&self.env);`
  - 删掉 `GbrainCliTransport::cleanup_stale_pglite_lock` 方法 + `GbrainCliTransport::is_bundled_gbrain` 关联函数（已提为自由函数）。
  - 更新两个调用点:`1963` `GbrainCliTransport::is_bundled_gbrain(&state.config)` → `is_bundled_gbrain(&state.config)`;`2534` 同样改 `is_bundled_gbrain(&config)`。

- [ ] **Step 4: 编译（行为不变）**
  Run: `cd src-tauri && cargo build > /tmp/g1.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/g1.txt | head`
  Expected: EXIT=0。若报 `HashMap` 未导入,确认 mcp.rs 顶部已 `use std::collections::HashMap;`(GbrainCliTransport 用了,应已在)。

- [ ] **Step 5: 提交**
  ```bash
  git add src-tauri/src/mcp.rs
  git commit -m "refactor(mcp): extract cleanup_stale_pglite_lock + is_bundled_gbrain to free fns"
  ```

---

## Task 2: 切换 bundled gbrain → StdioTransport + spawn 前清锁

**Files:** Modify `src-tauri/src/mcp.rs`（`connect_server_shared` 的 transport 选择,约 2532-2557）。

- [ ] **Step 1: 替换 Stdio 分支**
  把:
  ```rust
  TransportType::Stdio => {
      if is_bundled_gbrain(&config) {
          tracing::warn!(server_id = %id, "Using bundled gbrain CLI-backed MCP transport instead of Bun stdio");
          Arc::new(GbrainCliTransport::new(&config.name, &config.command, &config.args, &config.env))
      } else {
          let t = StdioTransport::spawn(&config.name, &config.command, &config.args, &config.env, id, notification_tx.clone()).await?;
          Arc::new(t)
      }
  }
  ```
  改为:
  ```rust
  TransportType::Stdio => {
      // 子项目 A/C 修复:捆绑 gbrain 走真 MCP serve(返回 JSON),不再用 CLI 垫片。
      // serve 是持久 PGLite 单写锁持有者 → spawn 前清掉上次崩溃残留的死锁。
      // kill_on_drop(true) + PR-3 reconnect 覆盖运行期/崩溃期锁生命周期。
      if is_bundled_gbrain(&config) {
          cleanup_stale_pglite_lock(&config.env);
          tracing::info!(server_id = %id, "Spawning bundled gbrain via real MCP stdio serve");
      }
      let t = StdioTransport::spawn(&config.name, &config.command, &config.args, &config.env, id, notification_tx.clone()).await?;
      Arc::new(t)
  }
  ```
  > `config.args` 原样含 `serve` → spawn `bun .../cli.ts serve`。reconnect 走同一 `connect_server_shared` → cleanup 自动覆盖重连。

- [ ] **Step 2: 编译**
  Run: `cd src-tauri && cargo build > /tmp/g2.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/g2.txt | head`
  Expected: EXIT=0。此时 `GbrainCliTransport` 变成无引用(只剩定义)→ 可能有 dead_code 警告,Task 3 删除。若编译因 `GbrainCliTransport` unused 报 **error**(而非 warning),加临时 `#[allow(dead_code)]` 或直接进 Task 3。

- [ ] **Step 3: 提交**
  ```bash
  git add src-tauri/src/mcp.rs
  git commit -m "fix(mcp): use StdioTransport (real gbrain serve) for bundled gbrain + cleanup before spawn"
  ```

---

## Task 3: 删除被取代的 GbrainCliTransport + 死 helper

**Files:** Modify `src-tauri/src/mcp.rs`.

- [ ] **Step 1: 删 GbrainCliTransport 主体**
  删除 `struct GbrainCliTransport`、`impl GbrainCliTransport`(`new` + `call_cli` + `suggest_page_slugs`)、`impl McpTransport for GbrainCliTransport`、以及 GbrainCliTransport 的 docstring。

- [ ] **Step 2: 删仅它引用的自由 helper（逐个 grep 确认）**
  对每个候选 `gbrain_cli_error_payload`、`classify_gbrain_cli_failure`、`push_number_flag`、`push_string_flag`、`push_bool_flag`、`required_string`、`optional_string`:
  ```bash
  grep -nE "\bHELPER\b" src-tauri/src/mcp.rs
  ```
  - 若**所有**剩余引用都在已删的 GbrainCliTransport 代码内(即删完 Step 1 后 grep 只剩定义自身)→ 删除该 helper。
  - 若有**其他**引用者 → **保留**。
  - `pid_is_alive` **保留**(被 `cleanup_stale_pglite_lock` 用)。

- [ ] **Step 3: 编译 + 确认无 dead_code 残留**
  Run: `cd src-tauri && cargo build > /tmp/g3.txt 2>&1; echo EXIT=$?; grep -E "^error|never used|GbrainCliTransport" /tmp/g3.txt | head`
  Expected: EXIT=0,无 `GbrainCliTransport` 残留引用,无新 dead_code 警告(删干净)。

- [ ] **Step 4: 确认 A/C parser 单测不回归**
  Run: `cd src-tauri && cargo test --lib gbrain::browse > /tmp/g3t.txt 2>&1; grep "test result" /tmp/g3t.txt`
  Expected: 既有 gbrain::browse 测试全 ok(本任务不碰它们,只是确认无连带破坏)。

- [ ] **Step 5: 提交**
  ```bash
  git add src-tauri/src/mcp.rs
  git commit -m "refactor(mcp): remove superseded GbrainCliTransport + dead CLI helpers"
  ```

---

## Task 4: gbrain serve smoke 命令 + cleanup 单测

**Files:** Modify `src-tauri/src/tauri_commands.rs`、`src-tauri/src/main.rs`、`src-tauri/src/mcp.rs`(测试)。

- [ ] **Step 1: 加 smoke 命令**（tauri_commands.rs,贴近 `run_memory_gbrain_eval_harness`）
```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GbrainSmokeReport {
    pub list_pages_ok: bool,
    pub list_pages_count: usize,
    pub get_stats_ok: bool,
    pub error: Option<String>,
}

/// 真起的 gbrain serve 端到端 smoke:调 list_pages + get_stats,断言能解析成强类型。
/// 这是子项目 A/C 当初缺的"真集成网"——按需手动跑(bundled gbrain 在场 + 已 init)。
#[tauri::command]
pub async fn gbrain_serve_smoke(state: State<'_, AppState>) -> Result<GbrainSmokeReport, String> {
    let mut report = GbrainSmokeReport {
        list_pages_ok: false,
        list_pages_count: 0,
        get_stats_ok: false,
        error: None,
    };
    match crate::gbrain::browse::list_pages(&state.mcp_manager, 50, None, None, None, None).await {
        Ok(pages) => { report.list_pages_ok = true; report.list_pages_count = pages.len(); }
        Err(e) => { report.error = Some(format!("list_pages: {}", e.to_command_string())); }
    }
    match crate::gbrain::browse::get_stats(&state.mcp_manager).await {
        Ok(_) => { report.get_stats_ok = true; }
        Err(e) => {
            let msg = format!("get_stats: {}", e.to_command_string());
            report.error = Some(match report.error.take() { Some(prev) => format!("{prev}; {msg}"), None => msg });
        }
    }
    Ok(report)
}
```
> 确认 `list_pages` 的真实签名(A 的 browse.rs):`list_pages(mcp, limit, sort, page_type, tag, updated_after)`。若参数顺序/数量不符,按真实签名调整传参。

- [ ] **Step 2: 注册命令（main.rs invoke_handler,贴近其它 gbrain_* 注册行）**
```rust
            uclaw_core::tauri_commands::gbrain_serve_smoke,
```

- [ ] **Step 3: 加 `cleanup_stale_pglite_lock` 单测（mcp.rs `#[cfg(test)]`)**
```rust
#[cfg(test)]
mod pglite_lock_cleanup_tests {
    use super::*;
    use std::collections::HashMap;

    fn env_with_home(home: &std::path::Path) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("GBRAIN_HOME".to_string(), home.to_string_lossy().to_string());
        m
    }

    fn write_lock(home: &std::path::Path, pid: i64) -> std::path::PathBuf {
        let dir = home.join(".gbrain").join("brain.pglite").join(".gbrain-lock");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lock"), format!("{{\"pid\": {pid}}}")).unwrap();
        dir
    }

    #[test]
    fn no_lock_file_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        cleanup_stale_pglite_lock(&env_with_home(tmp.path())); // 不 panic
    }

    #[test]
    fn dead_pid_lock_is_removed() {
        let tmp = tempfile::tempdir().unwrap();
        // i32::MAX 量级的 PID 几乎不可能存活
        let dir = write_lock(tmp.path(), 2_000_000_000);
        cleanup_stale_pglite_lock(&env_with_home(tmp.path()));
        assert!(!dir.exists(), "dead-pid lock dir should be removed");
    }

    #[test]
    fn live_pid_lock_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_lock(tmp.path(), std::process::id() as i64); // 当前进程,存活
        cleanup_stale_pglite_lock(&env_with_home(tmp.path()));
        assert!(dir.exists(), "live-pid lock dir should be kept");
    }
}
```
> 确认 `tempfile` 在 dev-deps(uClaw 测试广泛用;若无则用 `std::env::temp_dir()` + 唯一子目录手动建/清)。`pid_is_alive` 的"存活"判定与平台相关 —— 用当前进程 PID 作"活"、极大 PID 作"死"是稳妥选择。

- [ ] **Step 4: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/g4.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/g4.txt | head`(EXIT=0)
  - `cd src-tauri && cargo test --lib pglite_lock_cleanup > /tmp/g4t.txt 2>&1; grep "test result" /tmp/g4t.txt`(3 passed)
  - 确认 `gbrain_serve_smoke` 定义 1 + 注册 1。

- [ ] **Step 5: 提交**
  ```bash
  git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/mcp.rs
  git commit -m "feat(harness): gbrain serve smoke command + cleanup_stale_pglite_lock tests"
  ```

---

## 手动 E2E 验证清单(写进 PR —— 这是当初缺的真测,务必跑)

`cargo tauri dev`(确保 bundled gbrain 已 bootstrap + init):
1. **进程**:`ps` 能看到 `bun .../gbrain-source/src/cli.ts serve` 子进程在跑(不是每次调用起停)。
2. **Wiki tab**:列表出真页面(不再 `gbrain_response_parse_failed`);点页 → 详情 markdown + 反向链接;编辑/版本史可用。
3. **双星云 tab**:知识层有节点 + 连线(不再"知识层未连接");点知识星 → 跳 wiki 打开该页。
4. **smoke 命令**:`invoke('gbrain_serve_smoke')` 返回 `listPagesOk=true` + `getStatsOk=true`(若 brain 空,count=0 但 ok=true、error=null)。
5. **崩溃恢复**:`kill` 掉 gbrain serve 子进程 → PR-3 health loop 自动重连 → 功能恢复(重连前清锁)。
6. **退出**:关闭 app → gbrain serve 子进程被杀(kill_on_drop)、`~/.uclaw`(或 GBRAIN_HOME)下无残留 `.gbrain-lock`。
7. **空 brain**:若 `gbrain list` 真无页 → wiki/星云空状态(非报错)——此为正常,需子项目 B 喂数据。

> 若 smoke/手动暴露真 serve 输出与 A parser 的微差(字段名/信封)→ 那是数据驱动的真修,单独小改 `gbrain::browse` parser(不在本计划预设范围,但允许就地修 + 补单测)。

---

## 自检(对照 spec)

- **Spec 覆盖**:§2 切换 → Task 2;§3 锁生命周期(cleanup 提取 + spawn 前调 + 重连复用)→ Task 1+2;§4 删垫片 → Task 3;§5 init 不变 → 无任务(已满足);§6 smoke → Task 4;§7 错误处理(serve 失败→not_connected→前端优雅)→ 复用现有,无新代码;§8 测试 → Task 4 cleanup 单测 + 手动清单。
- **占位符**:无 TBD。"逐个 grep 确认引用"是真实删除前置(哪些 helper 共享需运行时确认),非含糊。
- **类型/签名一致**:`cleanup_stale_pglite_lock(env: &HashMap<String,String>)` 提取后两处调用一致;`is_bundled_gbrain(config)` 两处调用更新;smoke 调 `browse::list_pages/get_stats`(A 既有签名)。
- **范围**:单 PR、4 commit、无新 migration、无新依赖(tempfile 若已在 dev-deps)。不改 A/C parser/前端、不改 gbrain-source、不动 PR-3 重连。
