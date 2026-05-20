# 修复 — gbrain transport 切到真 MCP serve(StdioTransport)设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** Agent Memory OS v2 跟进修复(让已合并的子项目 A + C 真正可用)
**Root cause:** 见下 §1。系统排查结论:A/C 按 gbrain MCP 契约设计,但 uClaw 实际走 CLI 垫片 → 数据形状不匹配 + op 缺失。

---

## 1. 背景与根因(系统排查确定)

子项目 A(gbrain 浏览器)+ C(双星云知识层)按 gbrain 的 **MCP 协议契约**设计:`mcp_manager.call_tool("gbrain", op, …)` → gbrain `dispatch.ts` 返回 `{content:[{type:"text", text: JSON.stringify(result)}]}` → uClaw 解析 text 为 JSON。

但 uClaw 对**捆绑 gbrain**(`mcp.rs:2534`)故意选 `GbrainCliTransport`(CLI 垫片,docstring:"avoids long-lived PGLite lock holders"),而非 `StdioTransport`(真 MCP serve)。CLI 垫片:
- `call_cli` 跑 `gbrain list/get/search`(**无 `--json`**)→ 返回**人类可读文本**(空 brain 时为空)→ A 的 `parse_*` 用 `serde_json::from_str` 解析失败:`expected value at line 1 column 1`。
- gbrain CLI 的 `--json` 零散(`list/get/search/backlinks/graph/history` 没有),"加 --json"无效。
- 只映射 6 op;A/C 还需 `get_backlinks/traverse_graph/get_versions/revert_version/get_stats/find_orphans` + C 的 `get_links` → 全走 `other =>` 报错。

mock 单测喂手写 JSON、`call_gbrain_eval_tool` 不解析文本,故都没暴露,直到首次真端到端。**影响:已合并的 A 和 C 在当前 transport 下不工作。**

**决策(已确认):走 Path 1 —— 切到真 MCP serve。** 因为:形状与 A 的 parser 天然匹配、解锁全部 op、是 A 设计时假定的形态;PGLite 锁顾虑由现有 `kill_on_drop` + `cleanup_stale_pglite_lock` + 单消费者覆盖。

---

## 2. 核心切换(`src-tauri/src/mcp.rs:2534`)

bundled-gbrain 分支从 `GbrainCliTransport` 改为:
```
// 伪代码
cleanup_stale_pglite_lock(&config.env);   // 清崩溃残留锁(自由函数,见 §4)
let t = StdioTransport::spawn(&config.name, &config.command, &config.args, &config.env, id, notification_tx.clone()).await?;
Arc::new(t)
```
- `config.args` 不再剥 `serve` —— 原样带 `serve`(`[".../gbrain-source/src/cli.ts", "serve"]`,见 mcp.rs:588),spawn 出**持久 `bun gbrain serve` MCP stdio server**。
- `tools/call` → gbrain MCP dispatch → `JSON.stringify(result)` → A 的 `parse_list_pages/parse_page_detail/...` 与 C 的 `parse_links` 形状匹配,全部 op 可用。
- 删除整个 bundled-gbrain 的 if 分支后,该分支与 else(普通 stdio)几乎相同;实现时合并为统一 `StdioTransport::spawn`,仅对 bundled gbrain 额外先调 `cleanup_stale_pglite_lock`。

---

## 3. PGLite 锁生命周期(全靠现有件 + 一处提取)

| 时机 | 机制 | 现状 |
|---|---|---|
| spawn 前 | `cleanup_stale_pglite_lock`(读 lock 文件 PID,不存活则删) | 已存在(在 GbrainCliTransport 上);提取为自由函数 |
| 运行中 drop(超时/失败/重连) | `kill_on_drop(true)` 杀子进程,不留占锁僵尸 | 已在 `StdioTransport::spawn` |
| 崩溃恢复 | PR-3 health/reconnect 循环 ping 失败→重连→重 spawn;重连前再清锁 | 已存在 |
| 并发 | connect 的 `Connecting` 状态守卫 + PR-3 串行化重连 → 不双 spawn | 已存在 |
| 退出 | `WindowEvent::Destroyed` stop+shutdown → transport drop → child killed | 已存在 |

唯一新增动作:**spawn 前(及重连前)对 bundled gbrain 调用 `cleanup_stale_pglite_lock`**。

---

## 4. 删除 GbrainCliTransport

- 删:`struct GbrainCliTransport`、`impl McpTransport for GbrainCliTransport`、`call_cli`、`suggest_page_slugs`、`gbrain_cli_error_payload`、`classify_gbrain_cli_failure`,以及**仅被它引用**的 helper(`push_number_flag`/`push_string_flag`/`push_bool_flag`/`required_string`/`optional_string` 等 —— 实现时逐个 `grep` 确认无其他引用者再删;有共享引用的保留)。
- **保留并提取** `cleanup_stale_pglite_lock` 为模块级自由函数(原是 GbrainCliTransport 方法),供 §2/§3 在 spawn 前 + 重连前调用。它依赖的 `pid_is_alive` 已是自由函数。
- `is_bundled_gbrain` 保留(判定是否在 spawn 前清锁;`GbrainCliTransport::is_bundled_gbrain` 改成自由函数或挪到合适位置)。
- 删 2534 的 if 分支后,清理对应的 `tracing::warn!("Using bundled gbrain CLI-backed...")`。

---

## 5. init 时序(不变)

`ensure_bundled_gbrain_initialized`(main.rs:520,boot Stage)已在 connect 前跑 —— brain 先 `gbrain init` 再 serve。serve 的 spawn 发生在 connect 路径,晚于 init。不改。

---

## 6. Smoke 验证命令(可复跑的真集成网)

新增 / 扩展一个 Tauri 命令(复用现有 `memory_gbrain_eval_harness` 的形态,或新 `gbrain_smoke`):
- 经 `mcp_manager`(真起的 serve)调 `gbrain::browse::list_pages` + `gbrain::browse::get_stats`;
- 断言返回能解析成 `Vec<PageSummary>` / `BrainStats`(即:真 serve 的输出形状与 A 的 parser 匹配);
- 返回结构化结果(成功/失败 + 错误信息),供按需手动跑(bundled gbrain 在场时)。
- 这是当初缺的那道真集成检查 —— 把"从没真测"变成"一条命令可复跑"。

---

## 7. 错误处理

- serve spawn 失败 / gbrain 未初始化 → gbrain server 进入 `Error`/`Disconnected` → `call_tool` 返回 not_connected → 前端已优雅(WikiView 空状态、双星云"知识层未连接"角标)。
- 不回退 CLI(已删)。
- 首次真集成可能暴露 gbrain serve 的 MCP 输出细节与 A parser 的微差 → smoke 命令会抓到 → 按需小调 parser(数据驱动的真修,不在本 spec 预设)。

---

## 8. 测试

- **Rust 单测**:提取后的 `cleanup_stale_pglite_lock` 纯逻辑 —— ① 无锁文件 → no-op ② 死 PID 锁 → 删 ③ 活 PID 锁 → 保留(用一个确定不存活的 PID,如极大值;活 PID 用 `std::process::id()`)。A/C 的 parser 单测已有,不动。
- **Smoke 命令**:手动按需(需 bundled gbrain + 已 init)。
- **手动 E2E 清单(写进验证)**:`cargo tauri dev` → ① 进程里有 `gbrain ... serve` 子进程 ② wiki tab 列表/详情出真数据(非 parse 错误)③ 双星云知识层有节点 + 连线 ④ 杀掉 gbrain serve 进程 → 自动重连恢复 ⑤ 退出 app → 子进程被杀、无残留锁。

---

## 9. 范围边界

- 只动:transport 选择(2534)+ 删 CLI 垫片 + 提取 cleanup + smoke 命令 + cleanup 单测。
- **不改** A/C 的 parser/命令/前端(它们本就对,现在终于喂对数据)。
- **不改** gbrain-source。
- **不动** PR-3 health/reconnect 逻辑(复用)。
- 无新 migration、无新依赖。

---

## 10. 提交形状(bisectable,预计单 PR ~4 commit)

1. `refactor(mcp): extract cleanup_stale_pglite_lock + is_bundled_gbrain to free fns`
2. `fix(mcp): use StdioTransport (real gbrain serve) for bundled gbrain + cleanup before spawn`
3. `refactor(mcp): remove superseded GbrainCliTransport + dead helpers`
4. `feat(harness): gbrain serve smoke command (list_pages/get_stats JSON assert) + cleanup tests`
