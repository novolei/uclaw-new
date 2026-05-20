# 修复 Path 2 — gbrain CLI 输出归一为 MCP JSON(适配器)设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** Agent Memory OS v2 跟进修复(让已合并的 A + C 真正可用)
**Supersedes:** Path 1(切 StdioTransport serve)——已证实 gbrain serve **stdio 握手 60s 超时**不可用,已回退(commits 84ab767 + 3e49bc5)。Path 2 守住 CLI 垫片。

---

## 1. 背景与铁证(只读实跑 gbrain CLI)

A/C 按 gbrain **MCP JSON 契约**设计;uClaw 走 `GbrainCliTransport`(CLI 垫片,伪握手 + 每调一次 shell CLI)。Path 1 想换真 serve 拿 JSON,但 serve **不在 60s 内应答 initialize 握手**(实测),回退。

Path 2 = **让 CLI 垫片吐 MCP JSON**。实跑 `bunembed/bun gbrain-source/src/cli.ts <cmd>`(GBRAIN_HOME=~/.uclaw/gbrain,brain 有 10 页)抓到**真实输出**:

| A/C op | CLI 命令 | 真实输出格式 | 处理 |
|---|---|---|---|
| `get_backlinks` | `backlinks <slug>` | **JSON** `[{from_slug,to_slug,link_type,context,link_source,origin_slug,origin_field}]` | 透传 |
| `traverse_graph` | `graph <slug> [--depth N] [--direction]` | **JSON** `[{slug,title,type,depth,links:[{to_slug,link_type}]}]` | 透传 |
| `find_orphans` | `orphans --json` | **JSON** `{orphans:[{slug,title,domain}],...}` | 透传 |
| `list_pages` | `list [--type][--tag][-n N]` | **TAB 文本** `slug\ttype\tYYYY-MM-DD\ttitle`/行 | 转 JSON |
| `get_stats` | `stats` | **标签行** `Pages: N` … + `By type:` | 转 JSON |
| `get_page` | `get <slug> [--fuzzy]` | **markdown**:`---`YAML frontmatter`---` + 正文 | 转 JSON |
| `search` | `search <q> [--limit][--offset]` | **文本** `[score] slug -- snippet…`(snippet 跨行) | 转 JSON |
| `get_versions` | `history <slug>` | **文本** `#N  <ISO8601>  <body 预览>`/版本 | 转 JSON |
| `get_links`(C) | **无 CLI 命令** → `graph <slug> --depth 1` | JSON GraphNode,取 `links[]` | graph→Link 转 |
| `revert_version` | `revert <slug> <version-id>` | 状态文本 | 转/透传状态 |
| `put_page` | `put <slug> --content <md>`(已映射) | 状态文本 | 现状即可 |

关键事实:
- **`--json` 对 `list`/`stats` 无效**(实测 `list --json` 与 `list` 输出**完全相同**)——只有 `orphans` 支持 `--json`。所以"加 --json"对核心命令不可行,必须**解析人类文本**。
- 噪声 `[ai.gateway] recipe "google"…`(每次首行)+ `[orphans.scan] start/done` 在 **stderr**;`call_cli` 只取 stdout(`if stdout.is_empty() && !stderr.is_empty()` 才回 stderr),故 JSON-native op 的 stdout 干净。适配器仍**防御性 strip** 前导非数据行。
- `call_cli` 当前**只映射 6 op**(search/query/list_pages/think/get_page/put_page);backlinks/graph/orphans/history/stats/revert/get_links **未映射 → 走 `other=>` 报错**。须补全。
- **无 `get_links` CLI 命令** → C 的 `full_graph` 的 `get_links` op 映射到 `graph <slug> --depth 1`,取返回 GraphNode 的 `links:[{to_slug,link_type}]`,注入 `from_slug=slug` → 组成 `[{from_slug,to_slug,link_type}]`(browse::parse_links 形状)。

---

## 2. 架构

**新模块 `src-tauri/src/gbrain/cli_format.rs`** —— 把每个 op 的 CLI stdout **归一成 A 的 `browse::parse_*` 期望的 MCP JSON 文本**。`GbrainCliTransport::call_cli`(mcp.rs)拿到 stdout 后,经 `cli_format::to_mcp_json(op, args, stdout)` 得到 JSON,再包进现有 `{content:[{type:"text", text: <json>}], isError:false}` 信封。**`browse.rs` 的 parse_* / async fn 完全不动**(仍解析 JSON,transport 无关 —— 将来若 serve 修好,browse 一行不改)。

```
WikiView/DualNebula → invoke gbrain_* → tauri_commands → browse::list_pages/get_page/... (parse JSON, 不变)
  → mcp_manager.call_tool → GbrainCliTransport::call_cli
      → 跑 gbrain CLI 命令(补全的 op→argv 映射)→ stdout(文本/JSON)
      → cli_format::to_mcp_json(op, args, &stdout) → 归一 JSON 文本
      → 包 {content:[{text: json}]} 返回
```

`cli_format.rs` 单一职责:gbrain-CLI-输出 → MCP-JSON。fragile 的格式知识全部隔离在此一处,有真实 fixture 单测兜底(合"按域拆分/不堆 god file"偏好)。

---

## 3. `cli_format::to_mcp_json(op, args, stdout) -> Result<String, McpError>`

先 `strip_noise`(去掉前导 `[ai.gateway]…` / `[*.scan]…` 等非数据行),再按 op 分派:

**JSON 透传(已是 JSON)**:`get_backlinks` / `traverse_graph` / `find_orphans` —— 提取 stdout 中的 JSON 子串(首个 `[`/`{` 起到末尾)原样返回。

**文本→JSON 转换**:
- `list_pages`:每行 `split('\t')` → `{slug, type, title, updated_at}`(列序:slug,type,date,title)→ `Vec` JSON。
- `get_stats`:正则/逐行抓 `Pages/Chunks/Embedded/Links/Tags` → `{page_count,chunk_count,embedded_count,link_count,tag_count}`(Timeline/By-type 忽略)。
- `get_page`:用 `serde_yml` 解 `---…---` frontmatter → object;frontmatter 后正文 = `compiled_truth`;`slug`=入参 slug;`title`/`type`=frontmatter 取;`tags`=frontmatter.tags → `{slug,type,title,compiled_truth,frontmatter,tags}`。
- `search`:每条 `^\[(score)\]\s+(slug)\s+--\s+(snippet)` → `{slug, score, chunk_text:snippet, title:slug}`(CLI 无独立 title,用 slug 兜底;snippet 取首行/截断)→ `Vec`。
- `get_versions`:每条 `^#(\d+)\s+(ISO)\s+(preview)` → `{id:N, snapshot_at:ISO, compiled_truth:preview}` → `Vec`。
- `get_links`(C):入参已让 call_cli 跑 `graph <slug> --depth 1`;stdout 是 GraphNode JSON `[{slug,links:[{to_slug,link_type}]}]` → 找 `slug==入参` 的节点,其 `links` map 成 `[{from_slug:入参slug, to_slug, link_type}]`。
- `revert_version`:CLI 成功即可;返回 `{"status":"reverted"}`(browse::revert 后会 re-fetch get_page,不依赖此返回 shape)。
- `put_page`:维持现状(返回状态文本;browse::put 也 re-fetch)。

---

## 4. `call_cli` op→argv 映射补全(mcp.rs)

现有 6 个保留,新增:
- `get_backlinks` → `backlinks <slug>`
- `traverse_graph` → `graph <slug>` + `--depth N`(默认不传用 CLI 默认)+ `--direction`(若给)
- `get_versions` → `history <slug>`
- `get_stats` → `stats`
- `find_orphans` → `orphans --json`
- `revert_version` → `revert <slug> <version_id>`
- `get_links` → `graph <slug> --depth 1`(注:cli_format 据 op 名 `get_links` 做 links 抽取,而非 graph 透传)

`other =>` 仍对真未知 op 报错。

---

## 5. 错误处理

- CLI 退出非 0(已有 `gbrain_cli_error_payload` 逻辑保留)→ `McpError` → call_tool 错误 → browse 返回错误 → 前端 toast/空态。
- `cli_format` 解析失败(格式不符)→ 返回 `McpError::Server("gbrain CLI <op> 输出解析失败: …")`,带原始 stdout 片段便于诊断。**不静默吞**。
- 空结果(空 brain):`list` 空 stdout → `[]` JSON;`stats` 仍有数;不报错。
- `strip_noise` 后仍非预期 → 解析失败错误(同上)。

---

## 6. 测试

**Rust 单测 `cli_format.rs`(用第 1 节实跑抓到的真实输出做 fixture,不凭猜)**:
- `list_pages`:tab 文本(含 CJK slug `people/刘磊`)→ 正确 `Vec<{slug,type,title,updated_at}>`;空输入→`[]`。
- `get_stats`:标签行 → `{page_count:10, …}`。
- `get_page`:真实 markdown(YAML frontmatter + 正文)→ `{slug,type:person,title,compiled_truth 含正文,frontmatter,tags:[myself,user]}`。
- `search`:`[0.3648] people/ryanliu -- …` → `{slug,score:0.3648,chunk_text,…}`。
- `get_versions`:`#6 <iso> …` → `{id:6, snapshot_at, compiled_truth}`。
- `get_links`:graph JSON `{slug,links:[{to_slug,link_type}]}` → `[{from_slug,to_slug,link_type}]`。
- JSON 透传(backlinks/graph/orphans):strip 噪声后原样返回、可被 serde 解析。
- `strip_noise`:去 `[ai.gateway]`/`[orphans.scan]` 行。

**复用现有 smoke 命令 `gbrain_serve_smoke`**(改名/留用):真起 CLI 垫片 → list_pages + get_stats → 断言解析成 PageSummary/BrainStats。

**手动 E2E(写进验证)**:`cargo tauri dev` → wiki 列表出真页(10 页)、详情 markdown、搜索、反链、版本史、双星云知识层有节点。

---

## 7. 范围

- 改:新增 `cli_format.rs`;`call_cli` 补 op 映射 + 接 cli_format;smoke 命令复用。
- **不改** `browse.rs` 的 parse_*/async fn、不改前端 A/C 组件。
- **不碰** StdioTransport / serve(已证不可用,留作未来)。
- 含 A(wiki)+ C(双星云:graph 透传 + get_links via graph)。
- 保留 `aab2ed2`(自由函数)+ `9e39bb2`(清锁单测)。
- 无新 migration、无新依赖(serde_yml 已在)。

---

## 8. 风险

- **CLI 文本格式漂移**(gbrain 升级改格式)→ fixture 单测抓回归;格式知识集中在 cli_format.rs 一处,改一处即可。
- **`history` 的 `#N` 是否就是 `revert <slug> <version-id>` 接受的 version-id** → 实现时**必须验证**(跑 `history` 拿 #N,再 `revert <slug> N` 看是否成功)。若不是,version-id 另寻来源。
- **`search` 无独立 title** → 用 slug 兜底(A 的 SearchHit 显示可接受);snippet 跨行 → 取首行/合并截断。
- **CLI 冷启动慢**(bun + PGLite WASM 每调一次)→ 已有 45s 超时;wiki 首次加载可能慢几秒,可接受(后续可加缓存,不在本范围)。

---

## 9. 提交形状(bisectable,单 PR ~5 commit)

1. `feat(gbrain): cli_format.rs — gbrain CLI output → MCP JSON adapter + fixture tests`
2. `feat(mcp): map remaining gbrain ops (backlinks/graph/history/stats/orphans/revert/get_links) in call_cli`
3. `feat(mcp): route call_cli stdout through cli_format::to_mcp_json`
4. `fix(gbrain): get_links via graph --depth 1 + version-id verification`(若 #N≠version-id 在此修)
5. `test: cli_format fixtures + smoke command wiring`

无新 migration、无新依赖。
