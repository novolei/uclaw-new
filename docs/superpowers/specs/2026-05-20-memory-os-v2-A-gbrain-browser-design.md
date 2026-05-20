# 子项目 A — gbrain 知识浏览器（前端通路）设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** [Agent Memory OS v2 north-star](2026-05-20-agent-memory-os-v2-second-brain-design.md) · 子项目 A（吃掉 E 的前端部分）
**Goals:** 1（充分利用 gbrain）、3（用户第二大脑：浏览/编辑/版本控制）、0（复活悬空前端 WikiView）

---

## 1. 背景与决策

gbrain（vendored 在 `src-tauri/gbrain-source/`）暴露 **47 个 MCP 操作**（19 read / 19 write / 9 admin），包含前端浏览器所需的一切，且**内建页面版本历史**（`get_versions` / `revert_version`，每次 `put_page` 自动建版本，最多 50 版/页）。因此 A **不需要扩展 gbrain 源码** —— 是纯代理层 + 前端复活。

前置决策（来自 north-star spec）：
- gbrain 前端访问走 **MCP 协议代理**（新 Tauri 命令内部调 `mcp_manager.call_tool("gbrain", op, params)`）
- UI 落点：**复活已有的 `WikiView.tsx`**（当前因 L2 paused 而悬空），把数据源从 `memory_wiki_*` / `memory_entity_page_*` 换成新 `gbrain_*` 命令。一次完成 A + E 的前端部分。

---

## 2. 架构 & 模块布局

```
ui/src/components/memory/WikiView.tsx  (复活, 重定向数据源)
  │  invoke('gbrain_*', params)
  ▼
src-tauri/src/tauri_commands.rs  (10 个 #[tauri::command] 薄封装)
  │  调用
  ▼
src-tauri/src/gbrain/browse.rs  (新模块, 与 chat_extractor.rs 同目录)
  │  mcp_manager.call_tool("gbrain", op, json) → CallToolResult → 强类型
  ▼
gbrain MCP server (已连接的持久连接, server_id = "gbrain")
```

`browse.rs` 单一职责：参数组装 → MCP 调用 → 反序列化 gbrain JSON 为 uClaw 强类型 → 错误归一化。每个 Tauri 命令是 `browse.rs` 函数的 2–3 行 IPC 封装。

新命令需在 `main.rs` 的 `invoke_handler!` 宏中注册（否则编译过但运行时失败 —— 见 CLAUDE.md "Adjacent edits"）。

---

## 3. Tauri 命令面（10 个）

所有命令签名 `async fn(state: State<'_, AppState>, …params) -> Result<T, String>`，gbrain 未连接时返回 `Err("gbrain_not_connected")`。

| 命令 | gbrain op | 关键参数 (gbrain 侧) | 返回类型 |
|---|---|---|---|
| `gbrain_list_pages` | `list_pages` | `limit`, `offset`, `sort`(updated_desc/asc·created_desc·slug), `type?`, `tag?`, `after_date?`, `before_date?` | `Vec<PageSummary{slug,title,type,updated_at}>` |
| `gbrain_get_page` | `get_page` | `slug`, `fuzzy?` | `PageDetail{slug,title,type,compiled_truth,frontmatter,timeline?,created_at,updated_at}` |
| `gbrain_search` | `search` | `query`, `limit`, `offset?`, `detail?` | `Vec<SearchHit{slug,title,snippet,similarity}>` |
| `gbrain_get_backlinks` | `get_backlinks` | `slug` | `Vec<Backlink{from_slug,link_type}>` |
| `gbrain_traverse_graph` | `traverse_graph` | `slug`, `depth`, `direction`(in/out/both), `type?` | `GraphResult{nodes,edges}` |
| `gbrain_get_versions` | `get_versions` | `slug` | `Vec<VersionMeta{version_id,created_at,created_by}>` |
| `gbrain_revert_version` | `revert_version` | `slug`, `version_id` | `RevertOutcome{new_updated_at}` |
| `gbrain_put_page` | `put_page` | `slug`, `type`, `title`, `body`, `frontmatter?` | `PageDetail`（更新后的页） |
| `gbrain_get_stats` | `get_stats` | — | `BrainStats{page_count,chunk_count,embedding_coverage_pct}` |
| `gbrain_find_orphans` | `find_orphans` | — | `Vec<String>`（slug 列表） |

**类型定义位置**：`browse.rs` 中定义 `#[derive(Serialize, Deserialize)]` 结构；TS 镜像在 `ui/src/lib/gbrain-browse.ts`。

**反序列化注意**：gbrain 的 `CallToolResult` 内容是 JSON 文本（MCP content block）。`browse.rs` 解析 content → `serde_json::from_str` 到目标类型；gbrain 字段名（如 `compiled_truth`）直接映射。

---

## 4. 前端：复活 WikiView

改造 `ui/src/components/memory/WikiView.tsx`（保留三区布局，换数据源）：

- **Overview 区**：`gbrain_get_stats` 显示页数/块数/嵌入覆盖率 + `gbrain_find_orphans` 计数警示（"N 个孤儿页"）。替代原 `memory_wiki_get_overview` 的合成 markdown。
- **列表区**：`gbrain_list_pages`，加 type 过滤下拉（entity/concept/person/company/…）+ tag 过滤 + 排序下拉。分页（limit/offset）。顶部加 `gbrain_search` 搜索框（输入即切换到搜索结果模式）。
- **详情区**：`gbrain_get_page` → 用现有 `react-markdown` + shiki 渲染 `compiled_truth`。下方"反向链接"子面板（`gbrain_get_backlinks`，点击跳转）。头部"编辑" + "版本史"按钮。
- **新增 lib**：`ui/src/lib/gbrain-browse.ts` —— 每个命令一个 `async` 包装 + TS 类型（镜像 Rust）。
- **主题合规**：用主题 token（`bg-popover`/`text-muted-foreground`），不硬编码颜色（CLAUDE.md 主题要求）。

---

## 5. 编辑 + 版本流

- 详情区"编辑" → 行内 markdown 编辑器（V1 用 textarea；TipTap 端口未来 W4）→ "保存" 调 `gbrain_put_page`（传回 slug/type/title/编辑后 body）。保存成功后重渲染详情。
- gbrain **自动**为每次 `put_page` 建版本 → 编辑零数据风险。
- "版本史"抽屉：`gbrain_get_versions` 列出（version_id + created_at + created_by）→ 点某版本可预览 → "回滚"调 `gbrain_revert_version`。
- 这实现 goal 3 的"可编辑 + 可版本控制"，主要靠 gbrain 内建。

---

## 6. 错误处理

| 场景 | 行为 |
|---|---|
| gbrain 未连接（MCP server 没起/连接失败） | 命令返回 `Err("gbrain_not_connected")`；WikiView 显示空状态卡片 + "去设置检查 gbrain"（链到已有 SystemTab 诊断行） |
| MCP 调用超时/失败 | 命令返回 `Err(<message>)`；前端 toast，不崩溃 |
| gbrain 返回意外 JSON 形状 | `browse.rs` 反序列化失败 → log warn + 返回 `Err("gbrain_response_parse_failed")`；前端降级 |
| 编辑保存冲突 | 不做乐观锁；gbrain 覆盖式 + 版本历史兜底（用户可回滚） |
| 搜索/列表空结果 | 正常空状态，不报错 |

---

## 7. 测试

**Rust（`browse.rs` 单测，mock MCP 响应，无需真 gbrain）：**
- `deserialize_page_detail_from_gbrain_json` —— 正常页 JSON → `PageDetail`
- `deserialize_list_pages_paginated` —— 列表 JSON → `Vec<PageSummary>`
- `deserialize_handles_empty_results` —— 空数组不 panic
- `deserialize_handles_malformed_json` —— 坏 JSON → `Err`，不 panic
- `gbrain_not_connected_returns_structured_err` —— MCP 无 gbrain server 时的错误路径

**TS（Vitest + RTL，mock `invoke`）：**
- WikiView 渲染列表（mock `gbrain_list_pages`）
- 点列表项 → 详情渲染 markdown（mock `gbrain_get_page`）
- 反向链接面板渲染 + 点击跳转
- 编辑保存流（mock `gbrain_put_page`）
- 版本史抽屉（mock `gbrain_get_versions` + revert）
- gbrain 未连接 → 空状态卡片
- 搜索框 → 结果模式

**手动 E2E（写进验证清单）：** 连真 gbrain → 浏览列表 → 搜索 → 打开页 → 看反向链接 → 编辑保存 → 查版本史 → 回滚。

---

## 8. 范围边界（明确不做 — 留给后续子项目）

- ❌ 知识图**可视化**（`gbrain_traverse_graph` 命令本项目建好，但渲染留给 C 的知识星云）
- ❌ 文件**摄入**（`file_upload` 留给 B）
- ❌ memory_nodes 侧任何改动（A 只碰 gbrain + WikiView）
- ❌ MemoryHealthPanel 深化（E 的剩余部分；A 只复活 WikiView）
- ❌ 双星云融合（C）

---

## 9. 提交形状（bisectable commits 预期）

预计单 PR，约 5–7 个 commit：
1. `feat(gbrain): browse.rs MCP-proxy read ops + types + tests`
2. `feat(gbrain): browse.rs write ops (put_page/revert) + tests`
3. `feat(tauri): register 10 gbrain_* commands + invoke_handler`
4. `feat(ui): gbrain-browse.ts IPC wrapper + types`
5. `feat(ui): repurpose WikiView data source → gbrain (list/detail/overview)`
6. `feat(ui): WikiView edit + version-history flow`
7. `test(ui): WikiView vitest + empty-state`

无新 migration（A 不碰 SQLite）。无新依赖。
